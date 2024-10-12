use core::fmt;
use std::{collections::HashMap, sync::Arc};

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
}

impl AsyncRequest {
    pub fn create(path: &str, handler: Arc<AsyncHandler>, path_params: HashMap<String, String>, deps: Arc<DepsMap>) -> Self {
        AsyncRequest {
            path: path.to_string(),
            handler,
            path_params,
            deps: deps,
        }
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
