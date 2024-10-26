use std::sync::Arc;
use std::thread;
use std::time::Duration;

use nvo_servers::http::async_http_server::AsyncHttpServer;
use nvo_servers::http::response::Response;
use serde_json::json;
use nvo_servers::http::async_handler::AsyncHandler;
use nvo_servers::http::AsyncRequest;

#[allow(dead_code)]
pub fn get_status_handler() -> AsyncHandler {
    async fn status_handler(_: AsyncRequest) -> Result<Response, String> {
        Ok(Response::create(200, json!({"status": "ok"}).to_string()))
    }

    AsyncHandler::new("GET", "/status", status_handler)
}

#[allow(dead_code)]
pub fn wait_for_server_to_start(server: Arc<AsyncHttpServer>) {
    while !server.started.load(std::sync::atomic::Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(10));
    }
}
