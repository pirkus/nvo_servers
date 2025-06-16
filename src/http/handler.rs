use crate::http::response::Response;
use crate::http::Request;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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

    pub fn handle<S>(&self, stream: &mut S, path: String) -> Result<u16, String>
    where
        S: Write + Read,
    {
        let request = Request::create(path.as_str(), Self::not_found("fix_me"), HashMap::new(), "".to_string());
        let res = (self.handler_func)(&request)?; // TODO[FL]: return 500 Internal somehow
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