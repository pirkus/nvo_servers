use crate::http::async_handler::AsyncHandler;
use crate::http::ConnState;
use kqueue_sys::EventFlag;
use log::debug;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::{io, sync::atomic::Ordering};

use super::async_http_server::{AsyncHttpServer, AsyncHttpServerBuilder, AsyncHttpServerTrt};

impl AsyncHttpServerTrt for AsyncHttpServer {
    fn start_blocking(&self) {
        let listener = match TcpListener::bind(&self.listen_addr) {
            Ok(l) => l,
            Err(e) => {
                log::error!("Could not start listening on {}: {}", self.listen_addr, e);
                return;
            }
        };
        
        if let Err(e) = listener.set_nonblocking(true) {
            log::error!("Failed to set listener to nonblocking mode: {}", e);
            return;
        }
        let kqueue = unsafe { kqueue_sys::kqueue() };

        add_event(kqueue, listener.as_raw_fd() as usize, kqueue_sys::EventFilter::EVFILT_READ, kqueue_sys::EventFlag::EV_ADD | kqueue_sys::EventFlag::EV_ENABLE);

        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                break;
            }
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);
            // extract this, the contents does not matter
            let mut kevent = kqueue_sys::kevent::new(0, kqueue_sys::EventFilter::EVFILT_WRITE, kqueue_sys::EventFlag::empty(), kqueue_sys::FilterFlag::empty());
            let events_number = unsafe { kqueue_sys::kevent(kqueue, core::ptr::null(), 0, &mut kevent, 1, core::ptr::null()) };

            if events_number == -1 {
                log::error!("Could not retrieve an event from kqueue");
                continue;
            }

            debug!("Events count: {events_number}");

            if kevent.ident as i32 == listener.as_raw_fd() {
                match listener.accept() {
                    Ok((connection, _)) => {
                        if let Err(e) = connection.set_nonblocking(true) {
                            log::error!("Failed to set connection to non-blocking: {}", e);
                            continue;
                        }
                        let fd = connection.as_raw_fd();
                        add_event(kqueue, fd as usize, kqueue_sys::EventFilter::EVFILT_READ, kqueue_sys::EventFlag::EV_ADD);
                        add_event(kqueue, fd as usize, kqueue_sys::EventFilter::EVFILT_WRITE, kqueue_sys::EventFlag::EV_ADD);

                        let state = ConnState::Read(Vec::new());
                        debug!("Insert event id: {fd}");
                        if let Ok(mut conns) = self.connections.lock() {
                            conns.insert(fd, (connection, state));
                        } else {
                            log::error!("Failed to acquire connections lock");
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) if e.kind() == io::ErrorKind::InvalidInput => continue,
                    Err(e) => {
                        log::error!("Failed to accept connection: {}", e);
                        continue;
                    }
                }
            } else {
                let endpoints = self.endpoints.clone();
                let conns = self.connections.clone();
                let fd = kevent.ident as i32;

                debug!("Got event id: {fd}");

                let option = conns.lock().ok().and_then(|mut conns| conns.remove(&fd));
                if let Some((conn, conn_status)) = option {
                    if kevent.flags.contains(EventFlag::EV_EOF) || conn_status == ConnState::Flush {
                        drop(conn);
                    } else {
                        let deps_map = self.deps_map.clone();
                        // Queue the async work without blocking
                        self.workers
                            .queue(async move {
                                if let Some((conn, new_state)) = AsyncHandler::handle_async_better(conn, &conn_status, endpoints, deps_map).await {
                                    if new_state != ConnState::Flush {
                                        if let Ok(mut conns_lock) = conns.lock() {
                                            conns_lock.insert(fd, (conn, new_state));
                                        } else {
                                            log::error!("Failed to re-insert connection - lock poisoned");
                                        }
                                    } else {
                                        drop(conn);
                                    }
                                }
                            })
                            .unwrap_or_else(|e| log::error!("Failed to queue async job: {e}"));
                    }
                }
            }
        }
    }

    fn builder() -> AsyncHttpServerBuilder {
        AsyncHttpServerBuilder::default()
    }

    fn shutdown_gracefully(self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.workers.poison_all()
    }
}

fn add_event(kqueue: i32, ident: usize, filter: kqueue_sys::EventFilter, flags: kqueue_sys::EventFlag) {
    let sock_kevent = kqueue_sys::kevent::new(ident, filter, flags, kqueue_sys::FilterFlag::empty());
    let socket_kevent_result = unsafe { kqueue_sys::kevent(kqueue, &sock_kevent, 1, core::ptr::null_mut(), 0, core::ptr::null()) };
    if socket_kevent_result == -1 {
        log::error!("Could not register change event on kqueue for socket {}", ident);
    }
}
