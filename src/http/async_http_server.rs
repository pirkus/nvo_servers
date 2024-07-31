use std::{
    collections::HashMap,
    net::TcpStream,
    sync::{Arc, Mutex},
};

use crate::futures::workers::Workers;

use super::{conn_state::ConnState, handler::Handler};

pub struct AsyncHttpServer {
    pub listen_addr: String,
    pub endpoints: HashMap<String, Handler>,
    pub workers: Workers,
    pub connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
}
