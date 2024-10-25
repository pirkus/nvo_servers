/* TODO: FIX ME functionality has not caught up recently and is NOT on par with Async servers

use crate::futures::workers::Workers;
use crate::http::handler::Handler;
use crate::log_panic;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

pub struct HttpServer {
    endpoints: HashMap<String, Handler>,
    workers: Workers,
    listener: TcpListener,
}

pub trait HttpServerTrt {
    fn create_addr(addr: &str, endpoints: HashSet<Handler>) -> HttpServer;
    fn create_port(port: u32, endpoints: HashSet<Handler>) -> HttpServer;
    fn start_blocking(&self);
}

impl HttpServerTrt for HttpServer {
    fn create_addr(listen_addr: &str, endpoints: HashSet<Handler>) -> HttpServer {
        let thread_count = thread::available_parallelism().unwrap().get();
        let workers = Workers::new(thread_count);
        let endpoints = endpoints.into_iter().map(|x| (x.gen_key(), x)).collect();

        let listener = TcpListener::bind(listen_addr).unwrap_or_else(|e| log_panic!("Could not start listening on {listen_addr}, reason:\n{reason}", reason = e.to_string()));

        HttpServer { endpoints, workers, listener }
    }

    fn create_port(port: u32, endpoints: HashSet<Handler>) -> HttpServer {
        if port > 65535 {
            log_panic!("Port cannot be higher than 65535, was: {port}")
        }
        let thread_count = thread::available_parallelism().unwrap().get();
        let endpoints = endpoints.into_iter().map(|x| (x.gen_key(), x)).collect();
        let workers = Workers::new(thread_count);

        let listen_addr = format!("0.0.0.0:{port}");

        let listener = TcpListener::bind(listen_addr.clone()).unwrap_or_else(|e| log_panic!("Could not start listening on {listen_addr}, reason:\n{reason}", reason = e.to_string()));

        info!("Starting HTTP server on: {listen_addr}");
        HttpServer { endpoints, workers, listener }
    }

    fn start_blocking(&self) {
        for stream in self.listener.incoming() {
            let mut stream = stream.unwrap_or_else(|e| {
                log_panic!("Could not open tcp stream, reason:\n{}", e.to_string());
            });

            let http_request: Vec<String> = BufReader::new(&mut stream).lines().map(|x| x.unwrap()).take_while(|line| !line.is_empty()).collect();

            if http_request.is_empty() {
                // validate request further
                info!("Invalid request.");
                continue;
            }

            let first_line: Vec<&str> = http_request[0].split(' ').collect();
            let method = first_line[0];
            let path = first_line[1];
            let _protocol = first_line[2];
            let _headers = &http_request[1..];

            let endpoint_key = Handler::gen_key_from_str(path, method);
            let endpoint = self.endpoints.get(&endpoint_key);
            match endpoint {
                None => {
                    debug!("No handler registered for path: '{path}' and method: {method} not found.");
                    let contents = format!("Resource: {path} not found.");
                    let response = format!("HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{contents}", contents.len());

                    stream.write_all(response.as_bytes()).unwrap()
                }
                Some(endpoint) => {
                    let path_clj = String::from(path);
                    let endpoint = endpoint.clone();
                    let method_clj = String::from(method);
                    self.workers
                        .queue(async move {
                            // TODO: handle the error
                            let response_code = endpoint.handle(stream, path_clj.clone()).unwrap();
                            debug!("Handled request for path: '{path_clj}' and method: {method_clj}. {response_code}");
                        })
                        .unwrap();
                    debug!("Queued request for path: '{path}' and method: {method}.");
                }
            }
        }
    }
}*/
