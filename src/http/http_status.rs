use log::error;

// https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/505
pub struct HttpStatus;
impl HttpStatus {
    pub fn get_status_msg(code: u16) -> String {
        match code {
            200 => "OK".to_string(),
            201 => "Created".to_string(),
            204 => "No Content".to_string(),
            301 => "Moved Permanently".to_string(),
            400 => "Bad Request".to_string(),
            401 => "Unauthorized".to_string(),
            403 => "Forbidden".to_string(),
            404 => "Not Found".to_string(),
            409 => "Conflict".to_string(),
            415 => "Unsupported Media Type".to_string(),
            418 => "I'm a teapot".to_string(),
            500 => "Internal Server Error".to_string(),
            503 => "Service Unavailable".to_string(),
            505 => "HTTP Version Not Supported".to_string(),
            _ => {
                let err_msg = format!("Status code: {code}, not found, please define it!");
                error!("{}", err_msg);
                err_msg
            }
        }
    }
}
