use crate::http::response::Response;
use crate::http::ConnState;
use crate::http::Request;
use log::{debug, error};
use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::{Read, Write};
use std::pin::Pin;

#[derive(Clone, Debug)]
pub struct Handler {
    method: String,
    path: String,
    pub(crate) handler_func: fn(&Request) -> Result<Response, String>,
}

#[derive(Clone)]
pub struct AsyncHandler<S>
where
    S: AsyncHandlerFn + ?Sized,
{
    pub method: String,
    pub path: String,
    pub future: Pin<Box<S>>, //pub(crate) handler_func: dyn FnOnce(i32, i32) -> Future<Output = Result<Response, String>>,
}

pub trait AsyncHandlerFn {
    fn call(&self, args: Request) -> Pin<Box<dyn Future<Output = Result<Response, String>> + Send + 'static>>;
}

impl<S> Hash for AsyncHandler<S>
where
    S: AsyncHandlerFn,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
}

impl<S> PartialEq for AsyncHandler<S>
where
    S: AsyncHandlerFn + std::cmp::PartialEq + Send,
{
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path && self.future == other.future
    }
}
//impl<S> Eq for AsyncHandler<S> where S:

impl<T, F> AsyncHandlerFn for T
where
    T: Fn(Request) -> F,
    F: Future<Output = Result<Response, String>> + Send + 'static,
{
    fn call(&self, args: Request) -> Pin<Box<dyn Future<Output = Result<Response, String>> + Send + 'static>> {
        Box::pin(self(args))
    }
}

impl<S> AsyncHandler<S>
where
    S: AsyncHandlerFn + Sized,
{
    pub fn new(method: &str, path: &str, future: S) -> AsyncHandler<S> {
        AsyncHandler {
            method: method.to_string(),
            path: path.to_string(),
            future: Box::pin(future),
        }
    }

    async fn handle_async_better<C>(&self, mut connection: C, conn_state: &ConnState) -> Option<(C, ConnState)>
    where
        C: Read + Write,
    {
        match conn_state {
            ConnState::Read(_, _) => {
                panic!("Use read_request to read.")
            }
            ConnState::Write(req, written_bytes) => {
                //let endpoint = endpoints
                //    .iter()
                //    .find(|x| x.method == req.method && path_matches_pattern(&x.path, &req.path))
                //    .expect("Endpoint not found. Add better message");

                let res = self.future.call(req.clone()).await.unwrap(); // TODO: catch panics
                let status_line = res.get_status_line();
                let contents = res.response_body;
                let length = contents.len();
                let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
                let response_len = response.len();
                let mut written = *written_bytes;
                while written != response_len {
                    match connection.write(&response.as_bytes()[written..]) {
                        Ok(0) => {
                            debug!("client hung up");
                            return Some((connection, ConnState::Flush));
                        }
                        Ok(n) => written += n,
                        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => return Some((connection, ConnState::Write(req.clone(), written))),
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
        let request = Request::create(path.as_str(), &self.method /*HashMap::new()*/);
        let res = (self.handler_func)(&request).unwrap(); // TODO[FL]: return 500 Internal somehow
        let status_code = res.status_code;
        let status_line = res.get_status_line();
        let contents = res.response_body;
        let length = contents.len();

        let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");

        stream.write_all(response.as_bytes()).expect("Cannot write to output stream!");

        Ok(status_code)
    }

    pub fn new(path: &str, method: &str, handler_func: fn(&Request) -> Result<Response, String>) -> Handler {
        Handler {
            path: path.to_string(),
            method: method.to_string(),
            handler_func,
        }
    }

    pub(crate) fn not_found(method: &str) -> Handler {
        let method = method.to_owned();
        Handler::new("", &method, |req| Ok(Response::create(404, format!("Resource: {req_path} not found.", req_path = req.path))))
    }
}

// TODO [FL]: extract the two methods below and test them properly?
pub fn extract_path_params(pattern: &str, path: &str) -> HashMap<String, String> {
    let split_pattern = pattern.split('/').collect::<Vec<&str>>();
    let split_path = path.split('/').collect::<Vec<&str>>();

    if split_pattern.len() != split_path.len() {
        panic!("split_pattern.len() != split_path.len() - this should be done prior to calling this method")
    }

    (0..split_path.len())
        .filter_map(|i| {
            if split_pattern[i].starts_with(':') {
                let mut chars = split_pattern[i].chars();
                chars.next();
                Some((chars.as_str().to_string(), split_path[i].to_string()))
            } else {
                None
            }
        })
        .collect()
}

pub fn path_matches_pattern(pattern: &str, path: &str) -> bool {
    let split_pattern = pattern.split('/').collect::<Vec<&str>>();
    let split_path = path.split('/').collect::<Vec<&str>>();

    if split_pattern.len() != split_path.len() {
        return false;
    }

    (0..split_path.len())
        .map(|i| split_path[i] == split_pattern[i] || split_pattern[i].starts_with(':'))
        .reduce(|acc, e| acc && e)
        .unwrap()
}

impl PartialEq for Handler {
    fn eq(&self, other: &Self) -> bool {
        self.path.to_lowercase() == other.path.to_lowercase() && self.method.to_lowercase() == other.method.to_lowercase()
    }
}

impl Hash for Handler {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.method.hash(state);
    }
}

impl Eq for Handler {}
