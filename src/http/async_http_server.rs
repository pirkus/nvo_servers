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
    pub deps_map: DepsMap,
}

pub struct AsyncHttpServerBuilder {
    pub listen_addr: String,
    pub handlers: HashSet<AsyncHandler>,
    pub workers_number: usize,
    pub deps_map: DepsMap,
}

impl AsyncHttpServerBuilder {
    pub fn with_addr(mut self, addr: &str) -> AsyncHttpServerBuilder {
        if addr.contains(":") {
            self.listen_addr = addr.to_string();
        } else {
            let mut split = addr.split(":");
            self.listen_addr = format!("{addr}:{port}", port = split.nth(1).unwrap());
        }
        self
    }

    pub fn with_port(mut self, port: usize) -> AsyncHttpServerBuilder {
        if port > 65536 {
            panic!("Port cannot be larger than 65535. Was: {port}")
        }
        let hostname = self.listen_addr.split(":").nth(0).unwrap();
        self.listen_addr = format!("{hostname}:{port}");
        self
    }

    pub fn with_handlers(mut self, handlers: HashSet<AsyncHandler>) -> AsyncHttpServerBuilder {
        handlers.into_iter().for_each(|ele| {
            self.handlers.insert(ele);
        });
        self
    }

    pub fn with_dep(mut self, dep: impl Any + Sync + Send) -> AsyncHttpServerBuilder {
        self.deps_map.insert(dep);
        self
    }

    pub fn with_deps(mut self, deps: Vec<impl Any + Sync + Send>) -> AsyncHttpServerBuilder {
        deps.into_iter().for_each(|d| self.deps_map.insert(d));
        self
    }

    pub fn with_custom_num_workers(mut self, num_workers: usize) -> AsyncHttpServerBuilder {
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
            deps_map: self.deps_map,
        }
    }
}

impl Default for AsyncHttpServerBuilder {
    fn default() -> Self {
        let thread_count = thread::available_parallelism().unwrap().get();
        Self {
            listen_addr: "0.0.0.0:9000".to_string(),
            handlers: Default::default(),
            workers_number: thread_count,
            deps_map: DepsMap::default(),
        }
    }
}
