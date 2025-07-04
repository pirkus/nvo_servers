use crate::typemap::DepsMap;

use super::ConnStream;
use super::{headers::Headers, response::Response, AsyncRequest, ConnState};
use super::path_matcher::PathRouter;
use crate::futures::catch_unwind::CatchUnwind;
use log::{debug, error};
use std::collections::HashMap;
use std::str::from_utf8;
use std::sync::Arc;
use std::{future::Future, io, pin::Pin};

enum WriteResult {
    Complete,
    Partial(usize),
    ConnectionClosed,
}

const INITIAL_BUFFER_SIZE: usize = 8192;
const MAX_REQUEST_SIZE: usize = 1_048_576; // 1MB max request size

pub struct AsyncHandler {
    pub method: Arc<str>,
    pub path: Arc<str>,
    pub func: Box<dyn AsyncHandlerFn + Sync>,
}

impl AsyncHandler {
    /// Dynamically read HTTP request with growable buffer
    async fn read_http_request<S: ConnStream>(connection: &mut S) -> io::Result<Vec<u8>> {
        let mut buffer = vec![0u8; INITIAL_BUFFER_SIZE];
        let mut total_read = 0;
        
        loop {
            // Peek to see if we have enough data
            let peek_size = match connection.peek(&mut buffer[total_read..]) {
                Ok(n) => n,
                Err(e) => return Err(e),
            };
            
            // Check if we have the complete headers
            let search_end = total_read + peek_size;
            if let Ok(text) = from_utf8(&buffer[..search_end]) {
                if let Some(header_end) = text.find("\r\n\r\n") {
                    // Found end of headers, read exact amount
                    let mut result = vec![0u8; header_end];
                    connection.read_exact(&mut result)?;
                    return Ok(result);
                }
            }
            
            // Need more data - check if we're at buffer limit
            if search_end >= MAX_REQUEST_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP request headers too large"
                ));
            }
            
            // Grow buffer if needed
            if search_end >= buffer.len() {
                buffer.resize(buffer.len() * 2, 0);
            }
            
            total_read = search_end;
        }
    }

    /// Write all bytes using a functional approach
    fn write_all_bytes<S: ConnStream>(connection: &mut S, data: &[u8], offset: usize) -> WriteResult {
        if offset >= data.len() {
            return WriteResult::Complete;
        }

        match connection.write(&data[offset..]) {
            Ok(0) => {
                debug!("client hung up");
                WriteResult::ConnectionClosed
            }
            Ok(n) => {
                let new_offset = offset + n;
                if new_offset >= data.len() {
                    WriteResult::Complete
                } else {
                    WriteResult::Partial(new_offset)
                }
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => WriteResult::Partial(offset),
            Err(ref err) if err.kind() == io::ErrorKind::InvalidInput => WriteResult::Partial(offset),
            Err(err) => {
                error!("Failed to write response: {}", err);
                WriteResult::ConnectionClosed
            }
        }
    }

    pub async fn handle_async_better<S>(mut connection: S, conn_state: &ConnState, path_router: Arc<PathRouter<Arc<AsyncHandler>>>, deps_map: Arc<DepsMap>) -> Option<(S, ConnState)>
    where
        S: ConnStream,
    {
        match conn_state {
            ConnState::Read(req) => {
                // Dynamic buffer sizing implementation
                let request_data = match Self::read_http_request(&mut connection).await {
                    Ok(data) => data,
                    Err(e) => {
                        match e.kind() {
                            io::ErrorKind::WouldBlock | io::ErrorKind::InvalidInput => {
                                return Some((connection, ConnState::Read(req.clone())));
                            }
                            _ => {
                                error!("Failed to read HTTP request: {}", e);
                                return Some((connection, ConnState::Flush));
                            }
                        }
                    }
                };

                let raw_req = String::from_utf8_lossy(&request_data);
                let request: Vec<&str> = raw_req.split('\n').collect();

                let first_line: Vec<&str> = request[0].split(' ').collect();
                let method = first_line[0];
                let path = first_line[1];
                let _protocol = first_line[2];
                let headers = Headers::from_lines(request[1..].iter().copied());

                debug!("http_req_size = {}; ", request_data.len());

                let endpoint_result = path_router.find_match(path);

                debug!("Request payload: {:?}", request);

                let req_handler = match endpoint_result {
                    None => {
                        debug!("No handler registered for path: '{path}' and method: {method} not found.");
                        AsyncRequest::create(
                            path,
                            Arc::new(AsyncHandler::not_found(method)),
                            HashMap::new(),
                            Arc::new(DepsMap::default()),
                            headers.clone(),
                            match connection.try_clone() {
                                Ok(c) => c,
                                Err(e) => {
                                    error!("Failed to clone connection: {}", e);
                                    return Some((connection, ConnState::Flush));
                                }
                            },
                        )
                    }
                    Some((endpoint, path_params)) => {
                        // Check if the method matches
                        if endpoint.method.as_ref() != method {
                            debug!("Method mismatch for path: '{path}'. Expected: '{}', got: '{}'", endpoint.method, method);
                            AsyncRequest::create(
                                path,
                                Arc::new(AsyncHandler::not_found(method)),
                                HashMap::new(),
                                Arc::new(DepsMap::default()),
                                headers.clone(),
                                match connection.try_clone() {
                                    Ok(c) => c,
                                    Err(e) => {
                                        error!("Failed to clone connection: {}", e);
                                        return Some((connection, ConnState::Flush));
                                    }
                                },
                            )
                        } else {
                            debug!("Path: '{path}' matched endpoint path: '{endpoint_path}'", endpoint_path = endpoint.path);
                            AsyncRequest::create(
                                path,
                                endpoint.clone(),
                                path_params,
                                deps_map,
                                headers.clone(),
                                match connection.try_clone() {
                                    Ok(c) => c,
                                    Err(e) => {
                                        error!("Failed to clone connection: {}", e);
                                        return Some((connection, ConnState::Flush));
                                    }
                                },
                            )
                        }
                    }
                };
                Some((connection, ConnState::Write(req_handler, 0)))
            }
            ConnState::Write(req, written_bytes) => {
                let res = CatchUnwind::new(req.handler.func.call(req.clone()))
                    .await
                    .unwrap_or_else(|e| {
                        Ok(if e.is::<&str>() {
                            let panic_msg = *e.downcast::<&str>().expect("&str");
                            Response::create(500, format!("Internal server error\n:{panic_msg}"))
                        } else if e.is::<String>() {
                            let panic_msg = *e.downcast::<String>().expect("String");
                            Response::create(500, format!("Internal server error\n:{panic_msg}"))
                        } else {
                            Response::create(500, "Cannot interpret error.".to_string())
                            // [FL] TODO: custom error handlers
                        })
                    })
                    .unwrap();
                let status_line = res.get_status_line();
                let contents = res.response_body;
                let length = contents.len();
                let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
                let response_bytes = response.as_bytes();
                
                // Functional approach: try to write all remaining bytes
                match Self::write_all_bytes(&mut connection, response_bytes, *written_bytes) {
                    WriteResult::Complete => Some((connection, ConnState::Flush)),
                    WriteResult::Partial(new_written) => Some((connection, ConnState::Write(req.clone(), new_written))),
                    WriteResult::ConnectionClosed => Some((connection, ConnState::Flush)),
                }
            }
            ConnState::Flush => {
                if let Err(msg) = connection.flush() {
                    error!("Could not flush connection. Err kind: {}", msg.kind())
                };
                Some((connection, ConnState::Flush))
            }
        }
    }
}

impl Eq for AsyncHandler {}

impl PartialEq for AsyncHandler {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl std::hash::Hash for AsyncHandler {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
}

impl AsyncHandler {
    pub fn new(method: &str, path: &str, func: impl AsyncHandlerFn + 'static) -> AsyncHandler {
        AsyncHandler {
            method: Arc::from(method),
            path: Arc::from(path),
            func: Box::new(func),
        }
    }

    pub(crate) fn not_found(method: &str) -> AsyncHandler {
        async fn not_found_fn(req: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(404, format!("Resource: {req_path} not found.", req_path = req.path)))
        }

        AsyncHandler::new("", method, not_found_fn)
    }
}

impl<T: Send + Sync + 'static, F: Send + 'static> AsyncHandlerFn for T
where
    T: Fn(AsyncRequest) -> F,
    F: Future<Output = Result<Response, String>>,
{
    fn call(&self, args: AsyncRequest) -> Pin<Box<dyn Future<Output = Result<Response, String>> + Send + 'static>> {
        Box::pin(self(args))
    }
}

pub trait AsyncHandlerFn: Send + Sync + 'static {
    fn call(&self, args: AsyncRequest) -> Pin<Box<dyn Future<Output = Result<Response, String>> + Send + 'static>>;
}

#[cfg(test)]
mod tests {
    use crate::futures::workers::Workers;
    use crate::http::async_handler::AsyncHandler;
    use crate::http::headers::Headers;
    use crate::http::path_matcher::PathRouter;
    use crate::http::response::Response;
    use crate::http::{AsyncRequest, ConnState, ConnStream, Peek, TryClone};
    use crate::typemap::DepsMap;

    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::{
        cmp::min,
        io::{Read, Write},
    };

    #[derive(Clone)]
    struct FakeConn {
        read_data: Vec<u8>,
        write_data: Vec<u8>,
    }

    impl Read for FakeConn {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let size: usize = min(self.read_data.len(), buf.len());
            buf[..size].copy_from_slice(&self.read_data[..size]);
            Ok(size)
        }
    }

    impl Write for FakeConn {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.write_data = Vec::from(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Peek for FakeConn {
        fn peek(&self, buf: &mut [u8]) -> std::io::Result<usize> {
            let size: usize = min(self.read_data.len(), buf.len());
            buf[..size].copy_from_slice(&self.read_data[..size]);
            Ok(size)
        }
    }

    impl TryClone for FakeConn {
        fn try_clone(&self) -> std::io::Result<Arc<Mutex<dyn ConnStream>>> {
            Ok(Arc::new(Mutex::new(self.clone())))
        }
    }

    impl ConnStream for FakeConn {}

    impl FakeConn {
        fn new(read_data: &str) -> Self {
            FakeConn {
                read_data: read_data.as_bytes().to_vec(),
                write_data: Vec::default(),
            }
        }
    }

    #[test]
    fn async_can_read_and_match_the_right_handler() {
        async fn ugh_handler(x: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(200, x.path))
        }

        let workers = Workers::new(1);
        let handler = Arc::new(AsyncHandler::new("GET", "/some/:id", ugh_handler));
        let conn = FakeConn::new("GET /some/1 HTTP/1.1\r\nHost: host:port\r\nConnection: close\r\n\r\n");

        let handler_clj = handler.clone();
        let conn_clj = conn.clone();
        
        // Create a PathRouter and add the handler
        let mut router = PathRouter::new();
        router.add_route("/some/:id", handler_clj.clone());
        let router = Arc::new(router);
        
        let result =
            workers.queue_with_result(async move { AsyncHandler::handle_async_better(conn_clj, &ConnState::Read(Vec::new()), router, Arc::new(DepsMap::default())).await });
        let (_conn, conn_state) = result.unwrap().get().unwrap();
        match conn_state {
            ConnState::Write(req, 0) => {
                assert_eq!(req.path, "/some/1");
                assert_eq!(req.path_params.get("id"), Some(&"1".to_string()));
            }
            _ => panic!("Expected Write state"),
        }

        workers.poison_all()
    }

    //TODO [FL]: add tests for all stages

    #[test]
    fn write_can_catch_a_panic() {
        async fn ugh_handler(_: AsyncRequest) -> Result<Response, String> {
            panic!("panic")
        }

        let workers = Workers::new(1);
        let handler = Arc::new(AsyncHandler::new("GET", "/some/:id", ugh_handler));
        let conn = FakeConn::new("GET /some/1 HTTP/1.1\r\nHost: host:port\r\nConnection: close\r\n\r\n");

        let handler_clj = handler.clone();
        let conn_clj = conn.clone();
        
        // Create a PathRouter and add the handler
        let mut router = PathRouter::new();
        router.add_route("/some/:id", handler_clj.clone());
        let router = Arc::new(router);
        
        let write_state = ConnState::Write(
            AsyncRequest::create(
                "/some/1",
                handler.clone(),
                HashMap::from([("id".to_string(), "1".to_string())]),
                Arc::new(DepsMap::default()),
                Headers::new(),
                Arc::new(Mutex::new(conn)),
            ),
            0,
        );

        let result = workers.queue_with_result(async move { AsyncHandler::handle_async_better(conn_clj, &write_state, router, Arc::new(DepsMap::default())).await });
        let (conn, _conn_state) = result.unwrap().get().unwrap();
        assert_eq!(
            String::from_utf8(conn.write_data).unwrap(),
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 28\r\n\r\nInternal server error\n:panic"
        );
    }

    // #[test]
    // fn read_can_handle_req_larger_than_8192() {
    //     todo!()
    // }
}
