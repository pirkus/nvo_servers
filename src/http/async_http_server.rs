use std::{
    collections::HashMap,
    net::TcpStream,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use crate::futures::workers::Workers;

use super::{handler::Handler, ConnState};

pub struct AsyncHttpServer {
    pub listen_addr: String,
    pub endpoints: HashMap<String, Handler>,
    pub workers: Workers,
    pub connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
    pub started: AtomicBool,
}
