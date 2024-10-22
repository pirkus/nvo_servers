use core::{fmt};
use std::{
    collections::HashMap, io::{self, Read, Write}, net::TcpStream, sync::{Arc, Mutex}
};

use async_handler::AsyncHandler;
use handler::Handler;

use crate::typemap::DepsMap;

#[cfg(target_os = "freebsd")]
pub mod async_bsd_http_server;
pub mod async_http_server;
#[cfg(target_os = "linux")]
pub mod async_linux_http_server;

pub mod async_handler;
pub mod blocking_http_server;
pub mod handler;
mod helpers;
pub mod http_status;
pub mod response;

pub trait ConnStream: Read + Write + Peek + TryClone + Send + Sync {}

#[derive(PartialEq, Clone, Debug)]
pub struct Request {
    pub path: String,
    pub endpoint: Handler,
    pub path_params: HashMap<String, String>,
}

impl Request {
    pub fn create(path: &str, endpoint: Handler, path_params: HashMap<String, String>) -> Request {
        Request {
            path: path.to_string(),
            endpoint,
            path_params,
        }
    }
}

#[derive(Clone)]
pub struct AsyncRequest {
    pub path: String,
    pub handler: Arc<AsyncHandler>,
    pub path_params: HashMap<String, String>,
    pub deps: Arc<DepsMap>,
    pub headers: HashMap<String, String>,
    pub body: Arc<Mutex<dyn ConnStream>>,
}

impl AsyncRequest {
    pub fn create(path: &str, handler: Arc<AsyncHandler>, path_params: HashMap<String, String>, deps: Arc<DepsMap>, headers: HashMap<String, String>, body: Arc<Mutex<dyn ConnStream>>) -> Self {
        AsyncRequest {
            path: path.to_string(),
            handler,
            path_params,
            deps,
            headers,
            body,
        }
    }

    pub async fn body(&self) -> String {
        // throw away \r\n\r\n which 4 chars 
        let mut buf = vec![0u8; 4];
        loop {
            match self.body.lock().unwrap().read_exact(&mut buf) {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_e) => panic!("Do we want to panic here")
            };
        }

        // TODO: header names to be case insensitive and
        // TODO: should we handle cases where content length is uknown? check RFC
        let content_length = self.headers.get("Content-Length").unwrap().parse::<usize>().unwrap();
        let mut buf = vec![0u8; content_length];
        loop {
            match self.body.lock().unwrap().read_exact(&mut buf) {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_e) => panic!("Do we want to panic here")
            };
        }
        String::from_utf8(buf).unwrap()
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
    Read(Vec<u8>, usize),
    Write(AsyncRequest, usize),
    Flush,
}

impl fmt::Display for ConnState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnState::Read(_, _) => write!(f, "Read"),
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

// fuck TcpStream for returning itself on try_clone
impl TryClone for TcpStream {
    fn try_clone(&self) -> io::Result<Arc<Mutex<dyn ConnStream>>> {
        Ok(Arc::new(Mutex::new(self.try_clone().unwrap())))
    }
}

impl ConnStream for TcpStream {}
