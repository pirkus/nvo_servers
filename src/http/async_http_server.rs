use std::{
    collections::{HashMap, HashSet},
    net::TcpStream,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use crate::futures::workers::Workers;

use super::{async_handler::AsyncHandler, ConnState};

pub struct AsyncHttpServer {
    pub listen_addr: String,
    pub endpoints: HashSet<Arc<AsyncHandler>>,
    pub workers: Workers,
    pub connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
    pub started: AtomicBool,
}
