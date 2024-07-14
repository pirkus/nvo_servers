use crate::futures::workers::Workers;
use crate::http::handler::Handler;
use crate::http::request::Request;
use crate::log_panic;
use kqueue_sys::EventFlag;
use log::{debug, error, info};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};
use std::{io, thread};

use super::conn_state::ConnState;

pub struct AsyncBsdHttpServer {
    listen_addr: String,
    endpoints: HashMap<String, Handler>,
    workers: Workers,
    connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
}

pub trait AsyncHttpServerTrt {
    fn create_addr(addr: String, endpoints: HashSet<Handler>) -> AsyncBsdHttpServer;
    fn create_port(port: u32, endpoints: HashSet<Handler>) -> AsyncBsdHttpServer;
    fn start_blocking(&self);
}

impl AsyncHttpServerTrt for AsyncBsdHttpServer {
    fn create_addr(listen_addr: String, handlers: HashSet<Handler>) -> AsyncBsdHttpServer {
        let endpoints = handlers.into_iter().map(|x| (x.gen_key(), x)).collect();
        let thread_count = thread::available_parallelism().unwrap().get();
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let workers = Workers::new(thread_count);

        AsyncBsdHttpServer {
            listen_addr,
            endpoints,
            workers,
            connections,
        }
    }

    fn create_port(port: u32, handlers: HashSet<Handler>) -> AsyncBsdHttpServer {
        if port > 65535 {
            log_panic!("Port cannot be higher than 65535, was: {port}")
        }
        let endpoints = handlers.into_iter().map(|x| (x.gen_key(), x)).collect();
        let listen_addr = format!("0.0.0.0:{port}");
        let thread_count = thread::available_parallelism().unwrap().get();
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let workers = Workers::new(thread_count);

        info!("Starting non-blocking IO HTTP server on: {listen_addr}");
        AsyncBsdHttpServer {
            listen_addr,
            endpoints,
            workers,
            connections,
        }
    }

    fn start_blocking(&self) {
        let listener = TcpListener::bind(&self.listen_addr).unwrap_or_else(|e| {
            log_panic!(
                "Could not start listening on {addr}, reason:\n{reason}",
                addr = self.listen_addr,
                reason = e.to_string()
            )
        });
        listener.set_nonblocking(true).unwrap_or_else(|e| {
            log_panic!(
                "Failed to set listener to nonblocking mode, reason:\n{reason}",
                reason = e.to_string()
            )
        });

        let kqueue = unsafe { kqueue_sys::kqueue() };
        let sock_kevent = kqueue_sys::kevent::new(
            listener.as_raw_fd() as usize,
            kqueue_sys::EventFilter::EVFILT_READ,
            kqueue_sys::EventFlag::EV_ADD | kqueue_sys::EventFlag::EV_ENABLE,
            kqueue_sys::FilterFlag::empty(),
        );
        let socket_kevent_result = unsafe {
            kqueue_sys::kevent(
                kqueue,
                &sock_kevent,
                1,
                core::ptr::null_mut(),
                0,
                core::ptr::null(),
            )
        };
        if socket_kevent_result == -1 {
            panic!("could not register change event on kqueue for the socket");
        }

        loop {
            info!("kevent");
            // extract this, the contents does not matter
            let mut kevent = kqueue_sys::kevent::new(
                0,
                kqueue_sys::EventFilter::EVFILT_EMPTY,
                kqueue_sys::EventFlag::empty(),
                kqueue_sys::FilterFlag::empty(),
            );
            let events_number = unsafe {
                kqueue_sys::kevent(
                    kqueue,
                    core::ptr::null(),
                    0,
                    &mut kevent,
                    1,
                    core::ptr::null(),
                )
            };
            if events_number == -1 {
                panic!("could not retrieve an event from kqueue");
            }
            info!("Events number: {events_number}");

            if kevent.ident as i32 == listener.as_raw_fd() {
                match listener.accept() {
                    Ok((connection, _)) => {
                        connection.set_nonblocking(true).unwrap_or_else(|e| {
                            log_panic!(
                                "Failed to set connection to nonblocking mode, reason:\n{reason}",
                                reason = e.to_string()
                            )
                        });

                        let fd = connection.as_raw_fd();

                        let conn_kevent = kqueue_sys::kevent::new(
                            fd as usize,
                            kqueue_sys::EventFilter::EVFILT_READ,
                            kqueue_sys::EventFlag::EV_ADD,
                            kqueue_sys::FilterFlag::empty(),
                        );
                        let conn_kevent_result = unsafe {
                            kqueue_sys::kevent(
                                kqueue,
                                &conn_kevent,
                                1,
                                core::ptr::null_mut(),
                                0,
                                core::ptr::null(),
                            )
                        };
                        if conn_kevent_result < 0 {
                            // maybe we don't wanna blow up here?
                            panic!("Cannot register filter event for connection.");
                        }

                        let state = ConnState::Read(Vec::new(), 0);

                        info!("Insert event: {fd}");
                        self.connections
                            .lock()
                            .expect("locking problem")
                            .insert(fd, (connection, state));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    // do we wanna die here?
                    Err(e) => panic!("failed to accept: {}", e),
                }
            } else {
                let endpoints = self.endpoints.clone();
                let conns = self.connections.clone();

                let fd = kevent.ident as i32;
                info!("Event ident: {fd}");

                let option = conns.lock().expect("Poisoned").remove(&fd);
                if let Some((conn, _conn_status)) = option {
                    if kevent.flags.contains(EventFlag::EV_EOF) {
                        drop(conn.try_clone().unwrap());
                    }
                    self.workers
                        .queue(async move { read(conn, &endpoints) })
                        .unwrap_or_else(|e| error!("Failed to queue async job: {e}"));
                }
            }
        }
    }
}

fn read<S>(mut connection: S, endpoints: &HashMap<String, Handler>)
where
    S: Read + Write,
{
    let mut req = Vec::new();
    let mut read = 0;
    while read < 4 || &req[read - 4..read] != b"\r\n\r\n" {
        let mut buf = [0u8; 1024];
        match connection.read(&mut buf) {
            Ok(0) => {
                debug!("client disconnected unexpectedly");
                return;
            }
            Ok(n) => {
                req.extend(buf.iter().clone());
                read += n
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
            Err(e) => panic!("{}", e),
        }
    }

    let raw_req = String::from_utf8_lossy(&req[..read]);
    let request: Vec<&str> = raw_req.split('\n').collect();

    let first_line: Vec<&str> = request[0].split(' ').collect();
    let method = first_line[0];
    let path = first_line[1];
    let _protocol = first_line[2];
    let _headers = &request[1..];

    let endpoint_key = Handler::gen_key_from_str(path, method);
    let endpoint = endpoints.get(&endpoint_key);

    debug!("Request payload: {:?}", request);

    let req_handler = match endpoint {
        None => {
            debug!("No handler registered for path: '{path}' and method: {method} not found.");
            Request::create(path, Handler::not_found(method))
        }
        Some(endpoint) => Request::create(path, endpoint.clone()),
    };

    //write
    let res = (req_handler.endpoint.handler_func)(&req_handler).unwrap(); // TODO: catch panics
    let status_line = res.get_status_line();
    let contents = res.get_body();
    let length = contents.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
    let response_len = response.len();
    let mut written = 0;
    while written != response_len {
        match connection.write(&response.as_bytes()[written..]) {
            Ok(0) => debug!("client hung up"),
            Ok(n) => written += n,
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {}
            // Is this needed?
            // Err(ref err) if err.kind() == Interrupted => {
            //     return handle_connection_event(registry, connection, event, conn_state)
            // }
            Err(err) => panic!("{}", err), // I guess we don't wanna die here ?
        }
    }
    drop(connection);
}
