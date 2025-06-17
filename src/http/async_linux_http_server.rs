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

        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                return;
            }
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);

            let mut events = [Event::new(Events::empty(), 0); 1024];
            let num_events = match epoll::wait(epoll, -1, &mut events) {
                Ok(n) => n,
                Err(e) => {
                    error!("epoll::wait failed: {}", e);
                    continue;
                }
            };

            for event in &events[..num_events] {
                let fd = event.data as i32;

                if fd == listener.as_raw_fd() {
                    match listener.accept() {
                        Ok((connection, _)) => {
                            if let Err(e) = connection.set_nonblocking(true) {
                                error!("Failed to set connection to nonblocking mode: {}", e);
                                continue;
                            }
                            let fd = connection.as_raw_fd();
                            add_event(epoll, fd, Events::EPOLLIN | Events::EPOLLOUT);
                            let state = ConnState::Read(Vec::new());
                            if let Ok(mut conns) = self.connections.lock() {
                                conns.insert(fd, (connection, state));
                            } else {
                                error!("Failed to acquire connections lock");
                            }
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
                    let option = conns.lock().ok().and_then(|mut conns| conns.remove(&fd));
                    let deps_map = self.deps_map.clone();

                    if let Some((conn, conn_status)) = option {
                        let endpoint = self.endpoints.clone();
                        self.workers
                            .queue(async move {
                                if let Some((conn, new_state)) = AsyncHandler::handle_async_better(conn, &conn_status, endpoint, deps_map).await {
                                    if new_state != ConnState::Flush {
                                        if let Ok(mut conns_lock) = conns.lock() {
                                            conns_lock.insert(fd, (conn, new_state));
                                        } else {
                                            error!("Failed to re-insert connection - lock poisoned");
                                        }
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
