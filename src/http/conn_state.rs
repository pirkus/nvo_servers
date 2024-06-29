use crate::http::request::Request;

#[derive(PartialEq, Clone, Debug)]
pub enum ConnState {
    Read(Vec<u8>, usize),
    Write(Request, usize),
    Flush,
}
