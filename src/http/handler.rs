use crate::http::response::Response;
use crate::http::ConnState;
use crate::http::Request;
use log::{debug, error};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::{Read, Write};

#[derive(Clone, Debug)]
pub struct Handler {
    method: String,
    path: String,
    pub(crate) handler_func: fn(&Request) -> Result<Response, String>,
}

impl Handler {
    pub fn gen_key(&self) -> String {
        format!("{}-{}", self.path, self.method)
    }

    pub fn gen_key_from_str(path: &str, method: &str) -> String {
        format!("{}-{}", path, method)
    }

    pub fn handle<S>(&self, mut stream: S, path: String) -> Result<u16, String>
    where
        S: Write + Read,
    {
        let request = Request::create(path.as_str(), Self::not_found("fix_me"));
        let res = (self.handler_func)(&request).unwrap(); // TODO[FL]: return 500 Internal somehow
        let status_code = res.status_code;
        let status_line = res.get_status_line();
        let contents = res.response_body;
        let length = contents.len();

        let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");

        stream
            .write_all(response.as_bytes())
            .expect("Cannot write to output stream!");

        Ok(status_code)
    }

    pub async fn handle_async_better<S>(
        mut connection: S,
        conn_state: &ConnState,
        endpoints: &HashMap<String, Handler>,
    ) -> Option<(S, ConnState)>
    where
        S: Read + Write,
    {
        match conn_state {
            ConnState::Read(req, read_bytes) => {
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
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            return Some((connection, ConnState::Read(req, read)))
                        }
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

                let endpoint_key = Handler::gen_key_from_str(path, method);
                let endpoint = endpoints.get(&endpoint_key);

                debug!("Request payload: {:?}", request);

                let req_handler = match endpoint {
                    None => {
                        debug!(
                        "No handler registered for path: '{path}' and method: {method} not found."
                    );
                        Request::create(path, Handler::not_found(method))
                    }
                    Some(endpoint) => Request::create(path, endpoint.clone()),
                };
                Some((connection, ConnState::Write(req_handler, 0)))
            }
            ConnState::Write(req, written_bytes) => {
                let res = (req.endpoint.handler_func)(req).unwrap(); // TODO: catch panics
                let status_line = res.get_status_line();
                let contents = res.response_body;
                let length = contents.len();
                let response =
                    format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
                let response_len = response.len();
                let mut written = *written_bytes;
                while written != response_len {
                    debug!("writting...");
                    match connection.write(&response.as_bytes()[written..]) {
                        Ok(0) => {
                            debug!("client hung up");
                            return Some((connection, ConnState::Flush));
                        }
                        Ok(n) => written += n,
                        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                            return Some((connection, ConnState::Write(req.clone(), written)))
                        }
                        // Is this needed?
                        // Err(ref err) if err.kind() == Interrupted => {
                        //     return handle_connection_event(registry, connection, event, conn_state)
                        // }
                        Err(err) => panic!("{}", err), // I guess we don't wanna die here ?
                    }
                }
                Some((connection, ConnState::Flush))
            }
            ConnState::Flush => {
                if let Err(msg) = connection.flush() {
                    error!("Could not flush connection. Err kind: {}", msg.kind())
                };
                Some((connection, ConnState::Flush))
            }
        }
    }

    pub fn new(
        path: &str,
        method: &str,
        handler_func: fn(&Request) -> Result<Response, String>,
    ) -> Handler {
        Handler {
            path: path.to_string(),
            method: method.to_string(),
            handler_func,
        }
    }

    pub(crate) fn not_found(method: &str) -> Handler {
        let method = method.to_owned();
        Handler::new("", &method, |req| {
            Ok(Response::create(
                404,
                format!("Resource: {req_path} not found.", req_path = req.path),
            ))
        })
    }
}

impl PartialEq for Handler {
    fn eq(&self, other: &Self) -> bool {
        self.path.to_lowercase() == other.path.to_lowercase()
            && self.method.to_lowercase() == other.method.to_lowercase()
    }
}

impl Hash for Handler {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.method.hash(state);
    }
}

impl Eq for Handler {}
