use crate::futures::workers::Workers;
use crate::http::handler::Handler;
use crate::log_panic;
use epoll::ControlOptions::EPOLL_CTL_ADD;
use epoll::{Event, Events};
use log::{error, info};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};
use std::{io, thread};

use super::conn_state::ConnState;

pub struct AsyncUnixHttpServer {
    listen_addr: String,
    endpoints: HashMap<String, Handler>,
    workers: Workers,
    connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
}

impl AsyncUnixHttpServer {
    pub fn create_addr(listen_addr: String, handlers: HashSet<Handler>) -> AsyncUnixHttpServer {
        let endpoints = handlers.into_iter().map(|x| (x.gen_key(), x)).collect();
        let thread_count = thread::available_parallelism().unwrap().get();
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let workers = Workers::new(thread_count);

        AsyncUnixHttpServer {
            listen_addr,
            endpoints,
            workers,
            connections,
        }
    }

    pub fn create_port(port: u32, handlers: HashSet<Handler>) -> AsyncUnixHttpServer {
        if port > 65535 {
            log_panic!("Port cannot be higher than 65535, was: {port}")
        }
        let endpoints = handlers.into_iter().map(|x| (x.gen_key(), x)).collect();
        let listen_addr = format!("0.0.0.0:{port}");
        let thread_count = thread::available_parallelism().unwrap().get();
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let workers = Workers::new(thread_count);

        info!("Starting non-blocking IO HTTP server on: {listen_addr}");
        AsyncUnixHttpServer {
            listen_addr,
            endpoints,
            workers,
            connections,
        }
    }

    pub fn start_blocking(&self) {
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

        let epoll = epoll::create(false).unwrap_or_else(|e| {
            log_panic!(
                "Failed to create epoll, reason:\n{reason}",
                reason = e.to_string()
            )
        });
        // https://stackoverflow.com/questions/31357215/is-it-ok-to-share-the-same-epoll-file-descriptor-among-threads
        // To add multithreading: EPOLLIN | EPOLLET
        let event = Event::new(
            Events::EPOLLIN | Events::EPOLLOUT,
            listener.as_raw_fd() as _,
        );
        epoll::ctl(epoll, EPOLL_CTL_ADD, listener.as_raw_fd(), event).unwrap_or_else(|e| {
            log_panic!(
                "Failed to register interested in epoll fd, reason:\n{reason}",
                reason = e.to_string()
            )
        });

        // To add multithreading: spawn a new thread around here
        // events arr cannot be shared between threads, would be hard in rust anyway :D
        loop {
            let mut events = [Event::new(Events::empty(), 0); 1024];
            let num_events = epoll::wait(epoll, -1 /* block forever */, &mut events)
                .unwrap_or_else(|e| {
                    log_panic!("IO error, reason:\n{reason}", reason = e.to_string())
                });

            for event in &events[..num_events] {
                let fd = event.data as i32;

                if fd == listener.as_raw_fd() {
                    match listener.accept() {
                        Ok((connection, _)) => {
                            connection
                                .set_nonblocking(true)
                                .expect("Failed to set connection to nonblocking mode.");

                            let fd = connection.as_raw_fd();

                            let event = Event::new(Events::EPOLLIN | Events::EPOLLOUT, fd as _);
                            epoll::ctl(epoll, EPOLL_CTL_ADD, fd, event)
                                .expect("Failed to register interest in connection events.");

                            let state = ConnState::Read(Vec::new(), 0);

                            self.connections
                                .lock()
                                .expect("locking problem")
                                .insert(fd, (connection, state));
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        // do we wanna die here?
                        Err(e) => panic!("failed to accept: {}", e),
                    }
                } else {
                    let endpoints = self.endpoints.clone();
                    let conns = self.connections.clone();

                    let option = conns.lock().expect("Poisoned").remove(&fd);

                    if let Some((conn, conn_status)) = option {
                        self.workers
                            .queue(async move {
                                if let Some((conn, new_state)) =
                                    Handler::handle_async_better(conn, &conn_status, &endpoints)
                                        .await
                                {
                                    if new_state != ConnState::Flush {
                                        conns
                                            .lock()
                                            .expect("Poisoned")
                                            .insert(fd, (conn, new_state));
                                    } else {
                                        drop(conn)
                                    }
                                }
                            })
                            .unwrap_or_else(|e| error!("Failed to queue async job: {e}"));
                    }
                }
            }
        }
    }
}
