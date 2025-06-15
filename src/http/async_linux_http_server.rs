use super::async_handler::AsyncHandler;
use super::async_http_server::{AsyncHttpServer, AsyncHttpServerBuilder, AsyncHttpServerTrt};
use super::ConnState;
use crate::log_panic;
use epoll::ControlOptions::EPOLL_CTL_ADD;
use epoll::{Event, Events};
use log::{debug, error};
use std::io;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::sync::atomic::Ordering;

const MAX_EVENTS: usize = 1024;

impl AsyncHttpServerTrt for AsyncHttpServer {
    fn start_blocking(&self) {
        let listener = self.setup_listener();
        let epoll_fd = self.setup_epoll(&listener);
        
        debug!("Started HTTP server on {}", self.listen_addr);
        self.run_event_loop(listener, epoll_fd);
    }

    fn shutdown_gracefully(self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.workers.poison_all();
    }

    fn builder() -> AsyncHttpServerBuilder {
        AsyncHttpServerBuilder::default()
    }
}

impl AsyncHttpServer {
    fn setup_listener(&self) -> TcpListener {
        let listener = TcpListener::bind(&self.listen_addr)
            .unwrap_or_else(|e| {
                log_panic!(
                    "Could not start listening on {addr}, reason:\n{reason}",
                    addr = self.listen_addr,
                    reason = e
                )
            });
        
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|e| {
                log_panic!(
                    "Failed to set listener to nonblocking mode, reason:\n{reason}",
                    reason = e
                )
            });
        
        listener
    }

    /// Set up epoll with the listener registered
    fn setup_epoll(&self, listener: &TcpListener) -> i32 {
        let epoll_fd = epoll::create(false)
            .unwrap_or_else(|e| {
                log_panic!("Failed to create epoll, reason:\n{reason}", reason = e)
            });

        let event = Event::new(Events::EPOLLIN | Events::EPOLLOUT, listener.as_raw_fd() as _);
        epoll::ctl(epoll_fd, EPOLL_CTL_ADD, listener.as_raw_fd(), event)
            .unwrap_or_else(|e| {
                log_panic!("Failed to register listener in epoll, reason:\n{e}")
            });

        epoll_fd
    }

    fn run_event_loop(&self, listener: TcpListener, epoll_fd: i32) {
        let mut events = [Event::new(Events::empty(), 0); MAX_EVENTS];
        
        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                debug!("Shutdown requested, stopping event loop");
                return;
            }
            
            self.started.store(true, Ordering::SeqCst);

            let event_count = self.wait_for_events(epoll_fd, &mut events);
            self.process_events(&listener, epoll_fd, &events[..event_count]);
        }
    }

    fn wait_for_events(&self, epoll_fd: i32, events: &mut [Event; MAX_EVENTS]) -> usize {
        epoll::wait(epoll_fd, -1, events)
            .unwrap_or_else(|e| {
                log_panic!("IO error waiting for epoll events, reason:\n{reason}", reason = e)
            })
    }

    fn process_events(&self, listener: &TcpListener, epoll_fd: i32, events: &[Event]) {
        for event in events {
            let fd = event.data as i32;
            
            if fd == listener.as_raw_fd() {
                self.handle_new_connections(listener, epoll_fd);
            } else {
                self.handle_existing_connection(fd);
            }
        }
    }

    fn handle_new_connections(&self, listener: &TcpListener, epoll_fd: i32) {
        loop {
            match listener.accept() {
                Ok((connection, addr)) => {
                    if let Err(e) = self.setup_new_connection(connection, epoll_fd) {
                        error!("Failed to setup connection from {addr:?}: {e}");
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                Err(e) => {
                    error!("Failed to accept connection: {e}");
                    break;
                }
            }
        }
    }

    fn setup_new_connection(&self, connection: std::net::TcpStream, epoll_fd: i32) -> Result<(), Box<dyn std::error::Error>> {
        connection.set_nonblocking(true)?;
        
        let fd = connection.as_raw_fd();
        let event = Event::new(Events::EPOLLIN | Events::EPOLLOUT, fd as _);
        epoll::ctl(epoll_fd, EPOLL_CTL_ADD, fd, event)?;
        
        let state = ConnState::Read(Vec::new());
        
        self.connections
            .lock()
            .map_err(|_| "Failed to acquire connections lock")?
            .insert(fd, (connection, state));
        
        debug!("Added new connection with fd: {fd}");
        Ok(())
    }

    fn handle_existing_connection(&self, fd: i32) {
        let connections = self.connections.clone();
        let connection_data = {
            connections
                .lock()
                .expect("Failed to acquire connections lock")
                .remove(&fd)
        };

        if let Some((conn, conn_status)) = connection_data {
            let endpoints = self.endpoints.clone();
            let deps_map = self.deps_map.clone();
            
            self.workers
                .queue(async move {
                    match AsyncHandler::handle_async_better(conn, &conn_status, endpoints, deps_map).await {
                        Some((conn, ConnState::Flush)) => {
                            debug!("Connection {fd} ready to flush, dropping");
                            drop(conn);
                        }
                        Some((conn, new_state)) => {
                            if let Ok(mut connections) = connections.lock() {
                                debug!("Updated connection {fd} state to: {new_state}");
                                connections.insert(fd, (conn, new_state));
                            } else {
                                error!("Failed to reinsert connection {fd} due to lock poisoning");
                            }
                        }
                        None => {
                            debug!("Connection {fd} closed by handler");
                        }
                    }
                })
                .unwrap_or_else(|e| error!("Failed to queue async job for fd {fd}: {e}"));
        }
    }
}
