use crate::typemap::DepsMap;

use super::ConnStream;
use super::{helpers, response::Response, AsyncRequest, ConnState};
use log::{debug, error};
use std::collections::{HashMap, HashSet};
use std::str::from_utf8;
use std::sync::Arc;
use std::{future::Future, io, pin::Pin};

pub struct AsyncHandler {
    pub method: String,
    pub path: String,
    pub func: Box<dyn AsyncHandlerFn + Sync>,
}

impl AsyncHandler {
    pub async fn handle_async_better<S>(mut connection: S, conn_state: &ConnState, endpoints: HashSet<Arc<AsyncHandler>>, deps_map: Arc<DepsMap>) -> Option<(S, ConnState)>
    where
        S: ConnStream,
    {
        match conn_state {
            ConnState::Read(req, read_bytes) => {
                let mut buf = [0u8; 8192];
                match connection.peek(&mut buf) {
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Some((connection, ConnState::Read(req.clone(), *read_bytes))),
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => return Some((connection, ConnState::Read(req.clone(), *read_bytes))),
                    Err(e) => {
                        error!("Unpeekable stream. Error: {e}");
                        return Some((connection, ConnState::Flush));
                    }
                }
                let http_req_size = match from_utf8(&buf).unwrap().find("\r\n\r\n") {
                    Some(n) => n,
                    None => {
                        error!("Received not an HTTP request.");
                        return Some((connection, ConnState::Flush));
                    }
                };
                let mut buf = vec![0u8; http_req_size];
                match connection.read_exact(&mut buf) {
                    Ok(()) => {
                        debug!("Read http req.");
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Some((connection, ConnState::Read(req.clone(), *read_bytes))),
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => return Some((connection, ConnState::Read(req.clone(), *read_bytes))),
                    Err(e) => panic!("{}", e), // TODO: probably don't wanna blow up here
                };

                let raw_req = String::from_utf8_lossy(&buf);
                let request: Vec<&str> = raw_req.split('\n').collect();

                let first_line: Vec<&str> = request[0].split(' ').collect();
                let method = first_line[0];
                let path = first_line[1];
                let _protocol = first_line[2];
                let headers = &request[1..]
                    .iter()
                    .map(|x| {
                        if x.contains(':') {
                            let split = x.split_once(':').unwrap();
                            (split.0.trim().to_string().to_lowercase(), split.1.trim().to_string().to_lowercase())
                        } else {
                            (x.trim().to_string(), "".to_string())
                        }
                    })
                    .collect::<HashMap<String, String>>();

                debug!("http_req_size = {http_req_size}; ");

                let endpoint = endpoints.iter().find(|x| x.method == method && helpers::path_matches_pattern(&x.path, path));

                debug!("Request payload: {:?}", request);

                let req_handler = match endpoint {
                    None => {
                        debug!("No handler registered for path: '{path}' and method: {method} not found.");
                        AsyncRequest::create(
                            path,
                            Arc::new(AsyncHandler::not_found(method)),
                            HashMap::new(),
                            Arc::new(DepsMap::default()),
                            headers.clone(),
                            connection.try_clone().unwrap(),
                        )
                    }
                    Some(endpoint) => {
                        debug!("Path: '{path}' and endpoint.path: '{endpoint_path}'", endpoint_path = endpoint.path);
                        AsyncRequest::create(
                            path,
                            endpoint.clone(),
                            helpers::extract_path_params(&endpoint.path, path),
                            deps_map,
                            headers.clone(),
                            connection.try_clone().unwrap(),
                        )
                    }
                };
                Some((connection, ConnState::Write(req_handler, 0)))
            }
            ConnState::Write(req, written_bytes) => {
                let res = (req.handler.func).call(req.clone()).await.unwrap(); // TODO: catch panics
                let status_line = res.get_status_line();
                let contents = res.response_body;
                let length = contents.len();
                let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
                let response_len = response.len();
                let mut written = *written_bytes;
                while written != response_len {
                    match connection.write(&response.as_bytes()[written..]) {
                        Ok(0) => {
                            debug!("client hung up");
                            return Some((connection, ConnState::Flush));
                        }
                        Ok(n) => written += n,
                        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => return Some((connection, ConnState::Write(req.clone(), written))),
                        Err(ref err) if err.kind() == io::ErrorKind::InvalidInput => return Some((connection, ConnState::Write(req.clone(), written))),
                        // Is this needed?
                        // Err(ref err) if err.kind() == Interrupted => {
                        //     return handle_connection_event(registry, connection, event, conn_state)
                        // }
                        Err(err) => panic!("{}", err), // I guess we don't wanna die here ?
                    }
                }
                Some((connection, ConnState::Flush))
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
            method: method.to_string(),
            path: path.to_string(),
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
    use crate::http::response::Response;
    use crate::http::{AsyncRequest, ConnState, ConnStream, Peek, TryClone};
    use crate::typemap::DepsMap;
    use env_logger::Env;
    use std::collections::{HashMap, HashSet};
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

    #[test]
    fn async_can_read_and_match_the_right_handler() {
        async fn ugh_handler(x: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(200, x.path))
        }

        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

        let workers = Workers::new(1);
        let handler = Arc::new(AsyncHandler::new("GET", "/some/:id", ugh_handler));
        let http_req = b"GET /some/1 HTTP/1.1\r\nHost: host:port\r\nConnection: close\r\n\r\n";
        let mut contents = vec![0u8; http_req.len()];
        contents[..http_req.len()].clone_from_slice(http_req);
        let conn = FakeConn {
            read_data: contents,
            write_data: Vec::new(),
        };

        let handler_clj = handler.clone();
        let conn_clj = conn.clone();
        let result =
            workers.queue_with_result(async move { AsyncHandler::handle_async_better(conn_clj, &ConnState::Read(Vec::new(), 0), HashSet::from([handler_clj]), Arc::new(DepsMap::default())).await });
        let (_, conn_state) = result.unwrap().get().unwrap();
        assert_eq!(
            conn_state,
            ConnState::Write(
                AsyncRequest::create(
                    "/some/1",
                    handler.clone(),
                    HashMap::from([("id".to_string(), "1".to_string())]),
                    Arc::new(DepsMap::default()),
                    HashMap::new(),
                    Arc::new(Mutex::new(conn)),
                ),
                0,
            )
        );

        workers.poison_all()
    }

    //TODO [FL]: add tests for all stages

    #[test]
    fn func_can_be_called() {
        async fn foo(x: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(200, x.path))
        }

        let http_req = b"GET /some/1 HTTP/1.1\r\nHost: host:port\r\nConnection: close\r\n\r\n";
        let mut contents = vec![0u8; http_req.len()];
        contents[..http_req.len()].clone_from_slice(http_req);
        let conn = FakeConn {
            read_data: contents,
            write_data: Vec::new(),
        };
        let workers = Workers::new(1);
        let some_path = "some_path";
        let conn_clj = conn.try_clone().unwrap();
        let res = workers
            .queue_with_result(async move {
                let async_handler = Arc::new(AsyncHandler::new("some method", "some path", foo));
                async_handler
                    .func
                    .call(AsyncRequest::create(some_path, async_handler.clone(), HashMap::new(), Arc::new(DepsMap::default()), HashMap::new(), conn_clj).clone())
                    .await
            })
            .unwrap()
            .get();

        assert_eq!(res.unwrap().status_code, 200);

        workers.poison_all()
    }
}
