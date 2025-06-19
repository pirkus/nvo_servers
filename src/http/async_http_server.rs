use crate::futures::workers::Workers;
use crate::typemap::DepsMap;
use dashmap::DashMap;
use std::collections::HashSet;
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::{any::Any, default::Default};

use super::{async_handler::AsyncHandler, path_matcher::PathRouter, ConnState};

pub trait AsyncHttpServerTrt {
    fn builder() -> AsyncHttpServerBuilder;
    fn start_blocking(&self);
    fn shutdown_gracefully(self);
}

pub struct AsyncHttpServer {
    pub listen_addr: String,
    pub path_router: Arc<PathRouter<Arc<AsyncHandler>>>,
    pub workers: Workers,
    pub connections: Arc<DashMap<i32, (TcpStream, ConnState)>>,
    pub started: Arc<AtomicBool>,
    pub shutdown_requested: Arc<AtomicBool>,
    pub deps_map: Arc<DepsMap>,
}

pub struct AsyncHttpServerBuilder {
    addr: Option<String>,
    port: Option<usize>,
    handlers: HashSet<AsyncHandler>,
    deps: Vec<Box<dyn Any + Sync + Send>>,
    custom_num_workers: Option<usize>,
}

impl AsyncHttpServerBuilder {
    pub fn new() -> AsyncHttpServerBuilder {
        let thread_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4); // Default to 4 threads if detection fails
        Self {
            addr: None,
            port: None,
            handlers: Default::default(),
            deps: Default::default(),
            custom_num_workers: Some(thread_count),
        }
    }

    pub fn with_addr(self, addr: &str) -> Self {
        Self {
            addr: Some(addr.to_string()),
            ..self
        }
    }

    pub fn with_port(self, port: usize) -> Self {
        Self {
            port: Some(port),
            ..self
        }
    }

    pub fn with_handler(self, handler: AsyncHandler) -> Self {
        let mut handlers = self.handlers;
        handlers.insert(handler);
        Self { handlers, ..self }
    }

    pub fn with_handlers(self, new_handlers: HashSet<AsyncHandler>) -> Self {
        let handlers = self.handlers.into_iter().chain(new_handlers).collect();
        Self { handlers, ..self }
    }

    pub fn with_dep<T: Any + Send + Sync>(self, dep: T) -> Self {
        let mut deps = self.deps;
        deps.push(Box::new(dep));
        Self { deps, ..self }
    }

    pub fn with_deps(self, new_deps: Vec<Box<dyn Any + Sync + Send>>) -> Self {
        let deps = self.deps.into_iter().chain(new_deps).collect();
        Self { deps, ..self }
    }

    pub fn with_custom_num_workers(self, num_workers: usize) -> Self {
        Self {
            custom_num_workers: Some(num_workers),
            ..self
        }
    }

    pub fn build(self) -> AsyncHttpServer {
        // Create path router functionally
        let path_router = self.handlers.into_iter().fold(
            PathRouter::new(),
            |mut router, handler| {
                let path = handler.path.clone();
                router.add_route(&path, Arc::new(handler));
                router
            }
        );

        // Build deps map functionally
        let deps_map = self.deps.into_iter().fold(
            DepsMap::new(),
            |mut map, dep| {
                map.insert_boxed(dep);
                map
            }
        );

        let num_workers = self.custom_num_workers.unwrap_or(num_cpus::get());
        let listen_addr = format!(
            "{}:{}",
            self.addr.unwrap_or_else(|| "127.0.0.1".to_string()),
            self.port.unwrap_or(8080)
        );

        AsyncHttpServer {
            listen_addr,
            workers: Workers::new(num_workers),
            connections: Arc::new(DashMap::new()),
            path_router: Arc::new(path_router),
            deps_map: Arc::new(deps_map),
            started: Arc::new(AtomicBool::new(false)),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for AsyncHttpServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
