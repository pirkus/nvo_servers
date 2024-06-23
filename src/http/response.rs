use crate::http::http_status::HttpStatus;

pub struct Response {
    status_code: u16,
    response_body: String,
}

impl Response {
    pub fn create(status_code: u16, response_body: String) -> Response {
        Response {
            status_code,
            response_body,
        }
    }

    pub fn get_status_line(&self) -> String {
        let status_msg = HttpStatus::get_status_msg(self.status_code);
        format!(
            "HTTP/1.1 {status_code} {status_msg}",
            status_code = self.status_code
        )
    }

    pub fn get_body(&self) -> &str {
        &self.response_body
    }
    pub fn get_status_code(&self) -> u16 {
        self.status_code
    }
}
