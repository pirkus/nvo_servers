use super::handler::Handler;

#[derive(PartialEq, Clone, Debug)]
pub struct Request {
    pub path: String,
    pub endpoint: Handler,
}

impl Request {
    pub fn create(path: &str, endpoint: Handler) -> Request {
        Request {
            path: path.to_string(),
            endpoint,
        }
    }
}
