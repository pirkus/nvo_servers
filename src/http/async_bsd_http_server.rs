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
        let listener = TcpListener::bind(&self.listen_addr).unwrap();
        listener.set_nonblocking(true).unwrap();
        let kqueue = unsafe { kqueue_sys::kqueue() };
        let sock_kevent = kqueue_sys::kevent::new(
            listener.as_raw_fd() as usize,
            kqueue_sys::EventFilter::EVFILT_READ,
            kqueue_sys::EventFlag::EV_ADD | kqueue_sys::EventFlag::EV_ENABLE,
            kqueue_sys::FilterFlag::empty(),
        );
        let socket_kevent_result = unsafe { kqueue_sys::kevent(kqueue, &sock_kevent, 1, core::ptr::null_mut(), 0, core::ptr::null()) };
        if socket_kevent_result == -1 {
            panic!("could not register change event on kqueue for the socket");
        }

        loop {
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);
            // extract this, the contents does not matter
            let mut kevent = kqueue_sys::kevent::new(0, kqueue_sys::EventFilter::EVFILT_EMPTY, kqueue_sys::EventFlag::empty(), kqueue_sys::FilterFlag::empty());
            let events_number = unsafe { kqueue_sys::kevent(kqueue, core::ptr::null(), 0, &mut kevent, 1, core::ptr::null()) };
            if events_number == -1 {
                panic!("could not retrieve an event from kqueue");
            }
            debug!("Events count: {events_number}");

            if kevent.ident as i32 == listener.as_raw_fd() {
                match listener.accept() {
                    Ok((connection, _)) => {
                        connection.set_nonblocking(true).expect("Could not set.");
                        let fd = connection.as_raw_fd();

                        let conn_kevent = kqueue_sys::kevent::new(fd as usize, kqueue_sys::EventFilter::EVFILT_READ, kqueue_sys::EventFlag::EV_ADD, kqueue_sys::FilterFlag::empty());
                        let conn_kevent_result = unsafe { kqueue_sys::kevent(kqueue, &conn_kevent, 1, core::ptr::null_mut(), 0, core::ptr::null()) };
                        if conn_kevent_result < 0 {
                            // maybe we don't wanna blow up here?
                            panic!("Cannot register filter event for connection.");
                        }

                        let conn_kevent = kqueue_sys::kevent::new(fd as usize, kqueue_sys::EventFilter::EVFILT_WRITE, kqueue_sys::EventFlag::EV_ADD, kqueue_sys::FilterFlag::empty());
                        let conn_kevent_result = unsafe { kqueue_sys::kevent(kqueue, &conn_kevent, 1, core::ptr::null_mut(), 0, core::ptr::null()) };
                        if conn_kevent_result < 0 {
                            // maybe we don't wanna blow up here?
                            panic!("Cannot register filter event for connection.");
                        }

                        let state = ConnState::Read(Vec::new(), 0);

                        debug!("Insert event id: {fd}");
                        self.connections.lock().expect("locking problem").insert(fd, (connection, state));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    // do we wanna die here?
                    Err(e) => panic!("failed to accept: {}", e),
                }
            } else {
                let endpoints = self.endpoints.clone();
                let conns = self.connections.clone();

                let fd = kevent.ident as i32;
                debug!("Got event id: {fd}");

                let option = conns.lock().expect("Poisoned").remove(&fd);
                if let Some((conn, conn_status)) = option {
                    if kevent.flags.contains(EventFlag::EV_EOF) || conn_status == ConnState::Flush {
                        drop(conn);
                    } else {
                        let deps_map = self.deps_map.clone();
                        let result = self
                            .workers
                            .queue_with_result(async move { AsyncHandler::handle_async_better(conn, &conn_status, endpoints, deps_map).await })
                            .expect("Could not retrieve result from future.")
                            .get();
                        if let Some((conn, conn_state)) = result {
                            conns.lock().expect("Poisoned").insert(fd, (conn, conn_state));
                        }
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
