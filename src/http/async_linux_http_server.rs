use super::async_handler::AsyncHandler;
use super::async_http_server::{AsyncHttpServer, AsyncHttpServerBuilder, AsyncHttpServerTrt};
use super::ConnState;
use epoll::ControlOptions::EPOLL_CTL_ADD;
use epoll::{Event, Events};
use log::error;
use std::io;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::sync::atomic::Ordering;

const EVENT_BATCH_SIZE: usize = 1024;

impl AsyncHttpServerTrt for AsyncHttpServer {
    fn start_blocking(&self) {
        let listener = match TcpListener::bind(&self.listen_addr) {
            Ok(l) => l,
            Err(e) => {
                error!("Could not start listening on {}: {}", self.listen_addr, e);
                return;
            }
        };
        
        if let Err(e) = listener.set_nonblocking(true) {
            error!("Failed to set listener to nonblocking mode: {}", e);
            return;
        }

        let epoll = match epoll::create(false) {
            Ok(ep) => ep,
            Err(e) => {
                error!("Failed to create epoll: {}", e);
                return;
            }
        };
        add_event(epoll, listener.as_raw_fd(), Events::EPOLLIN | Events::EPOLLOUT);

        let mut events = [Event::new(Events::empty(), 0); EVENT_BATCH_SIZE];

        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                return;
            }
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);

            let num_events = match epoll::wait(epoll, -1, &mut events) {
                Ok(n) => n,
                Err(e) => {
                    error!("epoll::wait failed: {}", e);
                    continue;
                }
            };

            // Process events using functional approach
            events[..num_events]
                .iter()
                .for_each(|event| {
                    self.process_event(event, &listener, epoll);
                });
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

impl AsyncHttpServer {
    fn process_event(&self, event: &Event, listener: &TcpListener, epoll: i32) {
        let fd = event.data as i32;

        if fd == listener.as_raw_fd() {
            self.handle_new_connection(listener, epoll);
        } else {
            self.handle_existing_connection(fd);
        }
    }

    fn handle_new_connection(&self, listener: &TcpListener, epoll: i32) {
        match listener.accept() {
            Ok((connection, _)) => {
                if let Err(e) = connection.set_nonblocking(true) {
                    error!("Failed to set connection to nonblocking mode: {}", e);
                    return;
                }
                let fd = connection.as_raw_fd();
                add_event(epoll, fd, Events::EPOLLIN | Events::EPOLLOUT);
                let state = ConnState::Read(Vec::new());
                self.connections.insert(fd, (connection, state));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {},
            Err(e) if e.kind() == io::ErrorKind::InvalidInput => {},
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }

    fn handle_existing_connection(&self, fd: i32) {
        let conns = self.connections.clone();
        let option = conns.remove(&fd).map(|(_, value)| value);
        let deps_map = self.deps_map.clone();

        if let Some((conn, conn_status)) = option {
            let path_router = self.path_router.clone();
            self.workers
                .queue(async move {
                    if let Some((conn, new_state)) = AsyncHandler::handle_async_better(conn, &conn_status, path_router, deps_map).await {
                        if new_state != ConnState::Flush {
                            conns.insert(fd, (conn, new_state));
                        } else {
                            drop(conn)
                        }
                    }
                })
                .unwrap_or_else(|e| error!("Failed to queue async job: {e}"));
        }
    }
}

fn add_event(epoll: i32, fd: i32, events: Events) {
    let event = Event::new(events, fd as _);
    if let Err(e) = epoll::ctl(epoll, EPOLL_CTL_ADD, fd, event) {
        error!("Failed to register interest in epoll fd {}: {}", fd, e);
    }
}
