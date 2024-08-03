use std::sync::Arc;
use std::thread;
use std::time::Duration;

use nvo_servers::http::async_http_server::AsyncHttpServer;
use nvo_servers::http::handler::Handler;
use nvo_servers::http::response::Response;
use serde_json::json;

#[allow(dead_code)]
pub fn get_status_handler() -> Handler {
    Handler::new("/status", "GET", |_| {
        Ok(Response::create(200, json!({"status": "ok"}).to_string()))
    })
}

#[allow(dead_code)]
pub fn wait_for_server_to_start(server: Arc<AsyncHttpServer>) {
    while !server.started.load(std::sync::atomic::Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(10));
    }
}
