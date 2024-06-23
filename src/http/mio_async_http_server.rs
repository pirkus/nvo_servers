use ErrorKind::Interrupted;
use io::ErrorKind;
use std::collections::{HashMap, HashSet};
use std::{io, thread};
use std::io::ErrorKind::WouldBlock;
use std::sync::{Arc, Mutex};
use log::{debug, info};
use mio::{Events, Interest, Poll, Token};
use mio::net::{TcpListener, TcpStream};
use crate::futures::workers::Workers;
use crate::http::async_http_server::ConnState;
use crate::http::handler::Handler;
use crate::log_panic;

const NEW_CONN_TOKEN: Token = Token(0);

pub struct MioAsyncHttpServer {
    listen_addr: String,
    endpoints: HashMap<String, Handler>,
    _workers: Workers,
    connections: Arc<Mutex<HashMap<Token, (TcpStream, ConnState)>>>,
}

impl MioAsyncHttpServer {
    pub fn create_port(port: u32, handlers: HashSet<Handler>) -> MioAsyncHttpServer {
        if port > 65535 {
            log_panic!("Port cannot be higher than 65535, was: {port}")
        }
        let endpoints = handlers.into_iter().map(|x| (x.gen_key(), x)).collect();
        let listen_addr = format!("0.0.0.0:{port}");
        let thread_count = thread::available_parallelism().unwrap().get();
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let _workers = Workers::new(thread_count);

        info!("Starting non-blocking IO HTTP server on: {listen_addr}");
        MioAsyncHttpServer {
            listen_addr,
            endpoints,
            _workers,
            connections,
        }
    }
    pub fn start_blocking(&self) {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(128);

        let addr = self.listen_addr.parse().unwrap();
        let mut listener = TcpListener::bind(addr).unwrap();

        poll.registry().register(&mut listener, NEW_CONN_TOKEN, Interest::READABLE).unwrap();

        // Unique token for each incoming connection.
        let mut unique_token = Token(NEW_CONN_TOKEN.0 + 1);

        loop {
            if let Err(err) = poll.poll(&mut events, None) {
                if err.kind() == Interrupted {
                    continue;
                }
                panic!("{}", err);
            }

            for event in events.iter() {
                match event.token() {
                    NEW_CONN_TOKEN => loop {
                        let (mut connection, _) = match listener.accept() {
                            Ok((connection, address)) => (connection, address),
                            Err(e) if e.kind() == WouldBlock => {
                                break;
                            }
                            Err(e) => {
                                panic!("{}", e);
                            }
                        };

                        unique_token = Token(unique_token.0 + 1);
                        poll.registry().register(
                            &mut connection,
                            unique_token,
                            Interest::READABLE.add(Interest::WRITABLE),
                        ).unwrap();

                        let state = ConnState::Read(Vec::new(), 0);
                        self.connections.lock().expect("poisoned").insert(unique_token, (connection, state));
                    },
                    conn_token => {
                        let conn_and_conn_state = self.connections.lock().expect("poisoned").remove(&conn_token);

                        // self._workers.queue(async move {
                            if let Some((mut connection, conn_state)) = conn_and_conn_state {
                                let result = Handler::handle_async_mio(poll.registry(), &mut connection, event, &conn_state, &self.endpoints);
                                if let Ok(ConnState::Flush) = result {
                                    debug!("De-registering events for connection token: {:?}", conn_token.0);
                                    poll.registry().deregister(&mut connection).unwrap();
                                } else {
                                    debug!("Re-queueing connection with token: {:?}. Connection state: {:?}", conn_token.0, conn_state.clone());
                                    self.connections.lock().expect("poisoned").insert(conn_token, (connection, result.unwrap()));
                                }
                            }
                        // }).unwrap();
                    }
                }
            }
        }
    }
}