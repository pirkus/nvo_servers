use crate::futures::workers::Workers;
use crate::http::handler::Handler;
use crate::http::path_matcher::PathRouter;
use crate::error::{ServerError, ServerResult};
use log::{debug, error, info};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

pub struct HttpServer {
    path_router: PathRouter<Handler>,
    workers: Workers,
    listener: TcpListener,
}

pub trait HttpServerTrt {
    fn create_addr(addr: &str, endpoints: HashSet<Handler>) -> ServerResult<HttpServer>;
    fn create_port(port: u32, endpoints: HashSet<Handler>) -> ServerResult<HttpServer>;
    fn start_blocking(&self) -> ServerResult<()>;
}

impl HttpServerTrt for HttpServer {
    fn create_addr(listen_addr: &str, endpoints: HashSet<Handler>) -> ServerResult<HttpServer> {
        let thread_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4); // Default to 4 threads if detection fails
        let workers = Workers::new(thread_count);
        
        // Build PathRouter from endpoints
        let mut path_router = PathRouter::new();
        endpoints.into_iter().for_each(|handler| {
            let path = handler.path().to_string();
            path_router.add_route(&path, handler);
        });

        let listener = TcpListener::bind(listen_addr)
            .map_err(|e| ServerError::Io(format!("Could not start listening on {}: {}", listen_addr, e)))?;

        Ok(HttpServer { path_router, workers, listener })
    }

    fn create_port(port: u32, endpoints: HashSet<Handler>) -> ServerResult<HttpServer> {
        if port > 65535 {
            return Err(ServerError::Configuration(format!("Port cannot be higher than 65535, was: {}", port)));
        }
        let thread_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4); // Default to 4 threads if detection fails
        let workers = Workers::new(thread_count);
        
        // Build PathRouter from endpoints
        let mut path_router = PathRouter::new();
        endpoints.into_iter().for_each(|handler| {
            let path = handler.path().to_string();
            path_router.add_route(&path, handler);
        });

        let listen_addr = format!("0.0.0.0:{port}");

        let listener = TcpListener::bind(&listen_addr)
            .map_err(|e| ServerError::Io(format!("Could not start listening on {}: {}", listen_addr, e)))?;

        info!("Starting HTTP server on: {listen_addr}");
        Ok(HttpServer { path_router, workers, listener })
    }

    fn start_blocking(&self) -> ServerResult<()> {
        self.listener
            .incoming()
            .for_each(|stream_result| {
                match stream_result {
                    Ok(mut stream) => {
                        let http_request: Vec<String> = BufReader::new(&mut stream)
                            .lines()
                            .filter_map(Result::ok)
                            .take_while(|line| !line.is_empty())
                            .collect();

                        if http_request.is_empty() {
                            info!("Invalid request.");
                            return;
                        }

                        let first_line: Vec<&str> = http_request[0].split(' ').collect();
                        if first_line.len() < 3 {
                            info!("Invalid request line.");
                            return;
                        }
                        
                        let method = first_line[0];
                        let path = first_line[1];
                        let _protocol = first_line[2];
                        let _headers = &http_request[1..];

                        match self.path_router.find_match(path) {
                            Some((endpoint, _path_params)) if endpoint.method() == method => {
                                let path_clj = String::from(path);
                                let endpoint = endpoint.clone();
                                let method_clj = String::from(method);
                                self.workers
                                    .queue_blocking(move || {
                                        match endpoint.handle(&mut stream, path_clj.clone()) {
                                            Ok(response_code) => {
                                                debug!(
                                                    "Handled request for path: '{path_clj}' and method: {method_clj}. {response_code}"
                                                );
                                            }
                                            Err(e) => {
                                                error!("Handler error for path: '{path_clj}' and method: {method_clj}: {e}");
                                            }
                                        }
                                    })
                                    .unwrap_or_else(|e| {
                                        error!("Failed to queue request: {}", e);
                                    });
                                debug!("Queued request for path: '{path}' and method: {method}.");
                            }
                            _ => {
                                debug!(
                                    "No handler registered for path: '{path}' and method: {method} not found."
                                );
                                let contents = format!("Resource: {path} not found.");
                                let response = format!(
                                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{contents}",
                                    contents.len()
                                );

                                if let Err(e) = stream.write_all(response.as_bytes()) {
                                    error!("Failed to write response: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Could not open tcp stream: {}", e);
                    }
                }
            });
        
        // This is never reached due to the infinite loop, but needed for type checking
        Ok(())
    }
}
