use super::async_handler::AsyncHandler;
use super::async_http_server::{AsyncHttpServer, AsyncHttpServerBuilder, AsyncHttpServerTrt};
use super::ConnState;
use crate::log_panic;
use epoll::ControlOptions::EPOLL_CTL_ADD;
use epoll::{Event, Events};
use log::error;
use std::io;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::sync::atomic::Ordering;

impl AsyncHttpServerTrt for AsyncHttpServer {
    fn start_blocking(&self) {
        let listener = TcpListener::bind(&self.listen_addr).unwrap_or_else(|e| log_panic!("Could not start listening on {addr}, reason:\n{reason}", addr = self.listen_addr, reason = e.to_string()));
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|e| log_panic!("Failed to set listener to nonblocking mode, reason:\n{reason}", reason = e.to_string()));

        let epoll = epoll::create(false).unwrap_or_else(|e| log_panic!("Failed to create epoll, reason:\n{reason}", reason = e.to_string()));
        add_event(epoll, listener.as_raw_fd(), Events::EPOLLIN | Events::EPOLLOUT);

        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                return;
            }
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);

            let mut events = [Event::new(Events::empty(), 0); 1024];
            let num_events = epoll::wait(epoll, -1, &mut events).unwrap_or_else(|e| log_panic!("IO error, reason:\n{reason}", reason = e.to_string()));

            for event in &events[..num_events] {
                let fd = event.data as i32;

                if fd == listener.as_raw_fd() {
                    match listener.accept() {
                        Ok((connection, _)) => {
                            connection.set_nonblocking(true).expect("Failed to set connection to nonblocking mode.");
                            let fd = connection.as_raw_fd();
                            add_event(epoll, fd, Events::EPOLLIN | Events::EPOLLOUT);
                            let state = ConnState::Read(Vec::new());
                            self.connections.lock().expect("locking problem").insert(fd, (connection, state));
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                            continue;
                        }
                    }
                } else {
                    let conns = self.connections.clone();
                    let option = conns.lock().expect("Poisoned").remove(&fd);
                    let deps_map = self.deps_map.clone();

                    if let Some((conn, conn_status)) = option {
                        let endpoint = self.endpoints.clone();
                        self.workers
                            .queue(async move {
                                if let Some((conn, new_state)) = AsyncHandler::handle_async_better(conn, &conn_status, endpoint, deps_map).await {
                                    if new_state != ConnState::Flush {
                                        conns.lock().expect("Poisoned").insert(fd, (conn, new_state));
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

    fn shutdown_gracefully(self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.workers.poison_all()
    }

    fn builder() -> AsyncHttpServerBuilder {
        AsyncHttpServerBuilder::default()
    }
}

fn add_event(epoll: i32, fd: i32, events: Events) {
    let event = Event::new(events, fd as _);
    if let Err(e) = epoll::ctl(epoll, EPOLL_CTL_ADD, fd, event) {
        error!("Failed to register interest in epoll fd {}: {}", fd, e);
    }
}
