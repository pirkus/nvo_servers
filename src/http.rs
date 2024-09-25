use core::fmt;
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

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
pub struct AsyncHandler {
    foo: Box<dyn AsyncFn>,
}

impl<T: Send, F> AsyncFn for T
where
    T: Fn(u8) -> F,
    F: Future<Output = u8> + 'static + Send,
{
    fn call(&self, args: u8) -> Pin<Box<dyn Future<Output = u8> + Send + 'static>> {
        Box::pin(self(args))
    }
}

trait AsyncFn: Send {
    fn call(&self, args: u8) -> Pin<Box<dyn Future<Output = u8> + Send + 'static>>;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::futures::workers::Workers;

    use super::AsyncHandler;

    #[test]
    fn z() {
        async fn foo(x: u8) -> u8 {
            x * 2
        }

        let workers = Workers::new(1);
        workers.queue(async {
            let z = AsyncHandler { foo: Box::new(foo) };
            z.foo.call(2).await;
        });
    }
}
