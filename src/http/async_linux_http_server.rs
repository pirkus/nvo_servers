use crate::futures::workers::Workers;
use crate::http::handler::path_matches_pattern;
use crate::http::Request;
use crate::log_panic;
use epoll::ControlOptions::EPOLL_CTL_ADD;
use epoll::{Event, Events};
use log::{debug, error, info};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::{io, thread};

use super::async_http_server::AsyncHttpServer;
use super::handler::{AsyncHandler, AsyncHandlerFn};
use super::ConnState;

impl AsyncHttpServer {
    pub fn create_addr(listen_addr: String, handlers: HashSet<AsyncHandler<dyn AsyncHandlerFn>>) -> AsyncHttpServer {
        let thread_count = thread::available_parallelism().unwrap().get();
        AsyncHttpServer {
            listen_addr,
            endpoints: handlers,
            workers: Workers::new(thread_count),
            connections: Arc::new(Mutex::new(HashMap::new())),
            started: AtomicBool::new(false),
        }
    }

    pub fn create_port(port: u32, handlers: HashSet<AsyncHandler<dyn AsyncHandlerFn>>) -> AsyncHttpServer {
        if port > 65535 {
            panic!("Port cannot be higher than 65535, was: {port}")
        }
        let listen_addr = format!("0.0.0.0:{port}");
        let thread_count = thread::available_parallelism().unwrap().get();
        info!("Starting non-blocking IO HTTP server on: {listen_addr}");
        AsyncHttpServer {
            listen_addr,
            endpoints: handlers,
            workers: Workers::new(thread_count),
            connections: Arc::new(Mutex::new(HashMap::new())),
            started: AtomicBool::new(false),
        }
    }

    pub fn start_blocking(&self) {
        let listener = TcpListener::bind(&self.listen_addr).unwrap_or_else(|e| log_panic!("Could not start listening on {addr}, reason:\n{reason}", addr = self.listen_addr, reason = e.to_string()));
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|e| log_panic!("Failed to set listener to nonblocking mode, reason:\n{reason}", reason = e.to_string()));

        let epoll = epoll::create(false).unwrap_or_else(|e| log_panic!("Failed to create epoll, reason:\n{reason}", reason = e.to_string()));
        // https://stackoverflow.com/questions/31357215/is-it-ok-to-share-the-same-epoll-file-descriptor-among-threads
        // To add multithreading: EPOLLIN | EPOLLET
        let event = Event::new(Events::EPOLLIN | Events::EPOLLOUT, listener.as_raw_fd() as _);
        epoll::ctl(epoll, EPOLL_CTL_ADD, listener.as_raw_fd(), event).unwrap_or_else(|e| panic!("Failed to register interested in epoll fd, reason:\n{e}"));

        // To add multithreading: spawn a new thread around here
        // events arr cannot be shared between threads, would be hard in rust anyway :D
        loop {
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);

            let mut events = [Event::new(Events::empty(), 0); 1024];
            let num_events = epoll::wait(epoll, -1 /* block forever */, &mut events).unwrap_or_else(|e| log_panic!("IO error, reason:\n{reason}", reason = e.to_string()));

            for event in &events[..num_events] {
                let fd = event.data as i32;

                if fd == listener.as_raw_fd() {
                    match listener.accept() {
                        Ok((connection, _)) => {
                            connection.set_nonblocking(true).expect("Failed to set connection to nonblocking mode.");

                            let fd = connection.as_raw_fd();

                            let event = Event::new(Events::EPOLLIN | Events::EPOLLOUT, fd as _);
                            epoll::ctl(epoll, EPOLL_CTL_ADD, fd, event).expect("Failed to register interest in connection events.");

                            let state = ConnState::Read(Vec::new(), 0);

                            self.connections.lock().expect("locking problem").insert(fd, (connection, state));
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        // do we wanna die here?
                        Err(e) => panic!("failed to accept: {}", e),
                    }
                } else {
                    let conns = self.connections.clone();

                    let option = conns.lock().expect("Poisoned").remove(&fd);

                    if let Some((conn, conn_status)) = option {
                        self.workers
                            .queue(async move {
                                let new_conn_state = match conn_status {
                                    ConnState::Read(raw_req, read_bytes) => {
                                        println!("zzz");

                                        Self::read_request(conn, &raw_req, &read_bytes).await
                                    }
                                    ConnState::Write(req, written_bytes) => todo!(),
                                    ConnState::Flush => {
                                        drop(conn);
                                        return;
                                    }
                                };

                                if let Some((conn, new_state)) = new_conn_state {
                                    if new_state != ConnState::Flush {
                                        conns.lock().expect("Poisoned").insert(fd, (conn, new_state));
                                    } else {
                                        drop(conn);
                                    }
                                }
                            })
                            .unwrap_or_else(|e| error!("Failed to queue async job: {e}"));
                    }
                }
            }
        }
    }

    pub async fn read_request<C>(mut connection: C, req: &Vec<u8>, read_bytes: &usize) -> Option<(C, ConnState)>
    where
        C: Read + Write,
    {
        let mut req = req.clone();
        let mut read = *read_bytes;
        while read < 4 || &req[read - 4..read] != b"\r\n\r\n" {
            let mut buf = [0u8; 1024];
            match connection.read(&mut buf) {
                Ok(0) => {
                    debug!("client disconnected unexpectedly");
                    return Some((connection, ConnState::Flush));
                }
                Ok(n) => {
                    req.extend(buf.iter().clone());
                    read += n;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Some((connection, ConnState::Read(req, read))),
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

        debug!("Request payload: {:?}", request);

        let req_handler = Request::create(path, method /*extract_path_params(path_pattern, path)*/);

        //let endpoint = endpoints
        //    .iter()
        //    .find(|x| x.method == method && path_matches_pattern(&x.path, &path))
        //    .expect("Endpoint not found. Add better message");

        Some((connection, ConnState::Write(req_handler, 0)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::Write;
    use std::{cmp::min, io::Read};

    use env_logger::Env;

    use crate::futures::workers::Workers;
    use crate::http::async_http_server::AsyncHttpServer;
    use crate::http::handler::AsyncHandler;
    use crate::http::response::Response;
    use crate::http::{ConnState, Request};

    struct FakeConn {
        read_data: Vec<u8>,
        write_data: Vec<u8>,
    }

    impl Read for FakeConn {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let size: usize = min(self.read_data.len(), buf.len());
            buf[..size].copy_from_slice(&self.read_data[..size]);
            Ok(size)
        }
    }

    impl Write for FakeConn {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.write_data = Vec::from(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    //TODO: fix me

    #[test] //
    fn async_can_read_and_match_the_right_handler() {
        //
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init(); //
                                                                                         //
        let workers = Workers::new(1); //
        let http_req = b"GET /some/1 HTTP/1.1\r\nHost: host:port\r\nConnection: close\r\n\r\n"; //
        let mut contents = vec![0u8; http_req.len()]; //
        contents[..http_req.len()].clone_from_slice(http_req); //
        let conn = FakeConn {
            //
            read_data: contents,
            write_data: Vec::new(),
        };
        //
        let result = workers.queue_with_result(async move {
            //
            async fn future(_req: Request) -> Result<Response, String> {
                //
                Ok(Response::create(200, "ugh".to_string())) //
            } //

            let a: AsyncHandler<_> = AsyncHandler::new("GET", "/some/:id", future); //
                                                                                    //
            let hash_set: HashSet<AsyncHandler<_>> = HashSet::new(); //
            AsyncHttpServer::read_request(conn, &Vec::new(), &0).await
            //
        });
        let (_, conn_state) = result.unwrap().get().unwrap(); //
        assert_eq!(
            //
            conn_state,                                                                                                       //
            ConnState::Write(Request::create("/some/1", "GET" /*HashMap::from([("id".to_string(), "1".to_string())])*/,), 0)  //
        ); //
    } //

    //TODO [FL]: add tests for all stages
}
