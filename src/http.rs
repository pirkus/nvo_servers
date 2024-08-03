use core::fmt;

use handler::Handler;

#[cfg(target_os = "freebsd")]
pub mod async_bsd_http_server;
pub mod async_http_server;
#[cfg(target_os = "linux")]
pub mod async_linux_http_server;

pub mod blocking_http_server;
pub mod handler;
pub mod http_status;
pub mod response;

// Request
#[derive(PartialEq, Clone, Debug)]
pub struct Request {
    pub path: String,
    pub endpoint: Handler,
}

impl Request {
    pub fn create(path: &str, endpoint: Handler) -> Request {
        Request {
            path: path.to_string(),
            endpoint,
        }
    }
}
//END: Request

// Connection State
#[derive(PartialEq, Clone, Debug)]
pub enum ConnState {
    Read(Vec<u8>, usize),
    Write(Request, usize),
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
//END: Connection State
