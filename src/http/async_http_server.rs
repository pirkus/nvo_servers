use std::{collections::{HashMap, HashSet}, net::TcpStream, sync::{atomic::AtomicBool, Arc, Mutex}, thread};

use crate::futures::workers::Workers;

use super::{async_handler::AsyncHandler, ConnState};

pub trait AsyncHttpServerTrt {
    fn create_addr(listen_addr: &str, handlers: HashSet<AsyncHandler>) -> AsyncHttpServer;
    fn create_port(port: u32, handlers: HashSet<AsyncHandler>) -> AsyncHttpServer;
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
}

impl AsyncHttpServer {
    pub fn new_thread_count(listen_addr: &str, handlers: HashSet<AsyncHandler>, thread_count: usize) -> AsyncHttpServer {
        AsyncHttpServer {
            listen_addr: listen_addr.to_string(),
            endpoints: handlers.into_iter().map(Arc::new).collect(),
            workers: Workers::new(thread_count),
            connections: Arc::new(Mutex::new(HashMap::new())),
            started: AtomicBool::new(false),
            shutdown_requested: AtomicBool::new(false),
        }
    }

    pub fn new_default(listen_addr: &str, handlers: HashSet<AsyncHandler>) -> AsyncHttpServer {
        let thread_count = thread::available_parallelism().unwrap().get();
        AsyncHttpServer::new_thread_count(listen_addr, handlers, thread_count)
    }
}
