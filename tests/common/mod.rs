use nvo_servers::http::handler::Handler;
use nvo_servers::http::response::Response;
use serde_json::json;

#[allow(dead_code)]
pub fn get_status_handler() -> Handler {
    Handler::new("/status", "GET", |_| {
        Ok(Response::create(200, json!({"status": "ok"}).to_string()))
    })
}
