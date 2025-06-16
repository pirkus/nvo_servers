use std::{
    any::Any,
    collections::{HashMap, HashSet},
    net::TcpStream,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
};

use crate::{futures::workers::Workers, typemap::DepsMap};

use super::{async_handler::AsyncHandler, ConnState};

pub trait AsyncHttpServerTrt {
    fn builder() -> AsyncHttpServerBuilder;
    fn start_blocking(&self);
    fn shutdown_gracefully(self);
}

pub struct AsyncHttpServer {
    pub listen_addr: String,
    pub endpoints: HashSet<Arc<AsyncHandler>>,
    pub workers: Workers,
    pub connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
    pub started: AtomicBool,
    pub shutdown_requested: AtomicBool,
    pub deps_map: Arc<DepsMap>,
}

pub struct AsyncHttpServerBuilder {
    listen_addr: String,
    handlers: HashSet<AsyncHandler>,
    workers_number: usize,
    deps_map: DepsMap,
}

impl AsyncHttpServerBuilder {
    pub fn new() -> AsyncHttpServerBuilder {
        let thread_count = thread::available_parallelism().unwrap().get();
        Self {
            listen_addr: "0.0.0.0:9000".to_string(),
            handlers: Default::default(),
            workers_number: thread_count,
            deps_map: DepsMap::default(),
        }
    }

    pub fn with_addr(mut self, addr: &str) -> Self {
        self.listen_addr = addr.to_string();
        self
    }

    pub fn with_port(mut self, port: usize) -> Self {
        if port > 65536 {
            panic!("Port cannot be larger than 65535. Was: {port}")
        }
        let hostname = self.listen_addr.split(':').nth(0).unwrap();
        self.listen_addr = format!("{hostname}:{port}");
        self
    }

    pub fn with_handler(mut self, handler: AsyncHandler) -> Self {
        self.handlers.insert(handler);
        self
    }

    pub fn with_handlers(mut self, handlers: HashSet<AsyncHandler>) -> Self {
        handlers.into_iter().for_each(|ele| {
            self.handlers.insert(ele);
        });
        self
    }

    pub fn with_dep<T: Any + Send + Sync>(mut self, dep: T) -> Self {
        self.deps_map.insert(dep);
        self
    }

    pub fn with_deps(mut self, deps: Vec<Box<dyn Any + Sync + Send>>) -> Self {
        deps.into_iter().for_each(|d| {
            self.deps_map.insert_boxed(d);
        });
        self
    }

    pub fn with_custom_num_workers(mut self, num_workers: usize) -> Self {
        self.workers_number = num_workers;
        self
    }

    pub fn build(self) -> AsyncHttpServer {
        AsyncHttpServer {
            listen_addr: self.listen_addr,
            endpoints: self.handlers.into_iter().map(Arc::new).collect(),
            workers: Workers::new(self.workers_number),
            connections: Default::default(),
            started: AtomicBool::new(false),
            shutdown_requested: AtomicBool::new(false),
            deps_map: Arc::new(self.deps_map),
        }
    }
}

impl Default for AsyncHttpServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
