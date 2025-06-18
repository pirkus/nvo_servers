use core::fmt;
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
};

use async_handler::AsyncHandler;
use handler::Handler;
use log::debug;

use crate::typemap::DepsMap;
use self::headers::Headers;

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
pub mod async_bsd_http_server;
pub mod async_http_server;
#[cfg(target_os = "linux")]
pub mod async_linux_http_server;

pub mod async_handler;
pub mod blocking_http_server;
pub mod connection_pool;
pub mod connection_manager;
pub mod handler;
pub mod headers;
mod helpers;
pub mod http_status;
pub mod path_matcher;
pub mod response;
pub mod response_builder;

pub trait ConnStream: Read + Write + Peek + TryClone + Send + Sync {}

#[derive(PartialEq, Clone, Debug)]
pub struct Request {
    pub path: String,
    pub endpoint: Handler,
    pub path_params: HashMap<String, String>,
    pub body: String,
}

impl Request {
    pub fn create(path: &str, endpoint: Handler, path_params: HashMap<String, String>, body: String) -> Request {
        Request {
            path: path.to_string(),
            endpoint,
            path_params,
            body,
        }
    }
}

#[derive(Clone)]
pub struct AsyncRequest {
    pub path: String,
    pub handler: Arc<AsyncHandler>,
    pub path_params: HashMap<String, String>,
    pub deps: Arc<DepsMap>,
    pub headers: Headers,
    pub body: Arc<Mutex<dyn ConnStream>>,
}

impl AsyncRequest {
    pub fn create(path: &str, handler: Arc<AsyncHandler>, path_params: HashMap<String, String>, deps: Arc<DepsMap>, headers: Headers, body: Arc<Mutex<dyn ConnStream>>) -> Self {
        AsyncRequest {
            path: path.to_string(),
            handler,
            path_params,
            deps,
            headers,
            body,
        }
    }

    pub async fn body(&self) -> Result<String, Error> {
        // throw away \r\n\r\n which 4 chars
        let mut buf = vec![0u8; 4];
        loop {
            let mut body = self.body.lock()
                .map_err(|_| Error::new(500, "Failed to acquire body lock"))?;
            match body.read_exact(&mut buf) {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                Err(e) => return Err(Error::new(400, &format!("Failed to read request header: {}", e))),
            };
        }

        // Check if we have Content-Length
        if let Some(content_len) = self.headers.content_length() {
            debug!("Request content-length: {content_len}");
            let mut buf = vec![0u8; content_len];
            loop {
                let mut body = self.body.lock()
                    .map_err(|_| Error::new(500, "Failed to acquire body lock"))?;
                match body.read_exact(&mut buf) {
                    Ok(_) => break,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                    Err(e) => return Err(Error::new(400, &format!("Failed to read request body: {}", e))),
                };
            }
            String::from_utf8(buf)
                .map_err(|_| Error::new(400, "Invalid UTF-8 in request body"))
        } else if self.headers.get("transfer-encoding")
            .map(|te| te.to_lowercase().contains("chunked"))
            .unwrap_or(false) {
            // Handle chunked transfer encoding
            self.read_chunked_body().await
        } else {
            Err(Error::new(411, "Missing Content-Length header"))
        }
    }
    
    async fn read_chunked_body(&self) -> Result<String, Error> {
        let mut body_data = Vec::new();
        
        loop {
            // Read chunk size line
            let chunk_size_line = self.read_line().await?;
            
            // Parse chunk size (hex)
            let chunk_size = chunk_size_line.trim()
                .split(';') // Ignore chunk extensions
                .next()
                .ok_or_else(|| Error::new(400, "Invalid chunk size"))?
                .trim();
            
            let size = usize::from_str_radix(chunk_size, 16)
                .map_err(|_| Error::new(400, "Invalid chunk size format"))?;
            
            if size == 0 {
                // Last chunk - read trailing headers if any
                self.read_line().await?; // Read the final CRLF
                break;
            }
            
            // Read chunk data
            let mut chunk = vec![0u8; size];
            loop {
                let mut body = self.body.lock()
                    .map_err(|_| Error::new(500, "Failed to acquire body lock"))?;
                match body.read_exact(&mut chunk) {
                    Ok(_) => break,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                    Err(e) => return Err(Error::new(400, &format!("Failed to read chunk data: {}", e))),
                };
            }
            
            body_data.extend_from_slice(&chunk);
            
            // Read trailing CRLF after chunk data
            let mut crlf = [0u8; 2];
            loop {
                let mut body = self.body.lock()
                    .map_err(|_| Error::new(500, "Failed to acquire body lock"))?;
                match body.read_exact(&mut crlf) {
                    Ok(_) => break,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                    Err(e) => return Err(Error::new(400, &format!("Failed to read chunk trailer: {}", e))),
                };
            }
        }
        
        String::from_utf8(body_data)
            .map_err(|_| Error::new(400, "Invalid UTF-8 in chunked body"))
    }
    
    async fn read_line(&self) -> Result<String, Error> {
        let mut line = Vec::new();
        let mut prev_byte = 0u8;
        
        loop {
            let mut byte = [0u8; 1];
            loop {
                let mut body = self.body.lock()
                    .map_err(|_| Error::new(500, "Failed to acquire body lock"))?;
                match body.read_exact(&mut byte) {
                    Ok(_) => break,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                    Err(e) => return Err(Error::new(400, &format!("Failed to read line: {}", e))),
                };
            }
            
            if prev_byte == b'\r' && byte[0] == b'\n' {
                // Remove the \r from line
                line.pop();
                break;
            }
            
            line.push(byte[0]);
            prev_byte = byte[0];
        }
        
        String::from_utf8(line)
            .map_err(|_| Error::new(400, "Invalid UTF-8 in line"))
    }
}

impl std::fmt::Debug for AsyncRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncRequest").field("path", &self.path).field("path_params", &self.path_params).finish()
    }
}

impl PartialEq for AsyncRequest {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.path_params == other.path_params
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum ConnState {
    Read(Vec<u8>),
    Write(AsyncRequest, usize),
    Flush,
}

impl fmt::Display for ConnState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnState::Read(_) => write!(f, "Read"),
            ConnState::Write(_, _) => write!(f, "Write"),
            ConnState::Flush => write!(f, "Flush"),
        }
    }
}

pub trait Peek {
    fn peek(&self, buf: &mut [u8]) -> io::Result<usize>;
}

impl Peek for TcpStream {
    fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.peek(buf)
    }
}

pub trait TryClone {
    fn try_clone(&self) -> io::Result<Arc<Mutex<dyn ConnStream>>>;
}

impl TryClone for TcpStream {
    fn try_clone(&self) -> io::Result<Arc<Mutex<dyn ConnStream>>> {
        self.try_clone()
            .map(|stream| Arc::new(Mutex::new(stream)) as Arc<Mutex<dyn ConnStream>>)
    }
}

impl ConnStream for TcpStream {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub status_code: u16,
    pub title: String,
    pub desc: String,
}

impl Error {
    pub fn new(status_code: u16, title: &str) -> Error {
        Error {
            status_code,
            title: title.to_string(),
            desc: "".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn new_with_desc(status_code: u16, title: &str, desc: &str) -> Error {
        Error {
            status_code,
            title: title.to_string(),
            desc: desc.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::response::Response;
    use std::io::Cursor;
    
    // Mock ConnStream for testing
    struct MockStream {
        data: Cursor<Vec<u8>>,
    }
    
    impl MockStream {
        fn new(data: &[u8]) -> Arc<Mutex<Self>> {
            Arc::new(Mutex::new(MockStream {
                data: Cursor::new(data.to_vec()),
            }))
        }
    }
    
    impl Read for MockStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.data.read(buf)
        }
    }
    
    impl Write for MockStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    
    impl Peek for MockStream {
        fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }
    
    impl TryClone for MockStream {
        fn try_clone(&self) -> io::Result<Arc<Mutex<dyn ConnStream>>> {
            Ok(Arc::new(Mutex::new(MockStream {
                data: Cursor::new(self.data.get_ref().clone()),
            })) as Arc<Mutex<dyn ConnStream>>)
        }
    }
    
    impl ConnStream for MockStream {}
    
    #[test]
    fn test_chunked_body_reading() {
        use crate::futures::workers::Workers;
        
        // Create test data with chunked encoding
        let test_data = b"\r\n\r\n5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n";
        let stream = MockStream::new(test_data);
        
        let mut headers = Headers::new();
        headers.insert("Transfer-Encoding", "chunked");
        
        async fn dummy_handler(_: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(200, "".to_string()))
        }
        
        let request = AsyncRequest {
            path: "/test".to_string(),
            handler: Arc::new(AsyncHandler::new("GET", "/test", dummy_handler)),
            path_params: HashMap::new(),
            deps: Arc::new(DepsMap::default()),
            headers,
            body: stream as Arc<Mutex<dyn ConnStream>>,
        };
        
        // Use workers to run the async function
        let workers = Workers::new(1);
        let result = workers.queue_with_result(async move {
            request.body().await
        });
        
        let body = result.unwrap().get().unwrap();
        assert_eq!(body, "Hello World");
        
        workers.poison_all();
    }
}
