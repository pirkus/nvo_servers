mod common;

use nvo_servers::http::blocking_http_server::{HttpServer, HttpServerTrt};
use serde_json::Value;
use std::collections::HashSet;
use std::thread;
use nvo_servers::http::handler::Handler;
use nvo_servers::http::response::Response;

#[test]
fn get_works() {
    env_logger::init();
    let port = 8090;
    let endpoints = HashSet::from([Handler::new("/status", "GET", |_| Ok(Response::create(200, "{\"status\": \"ok\"}".to_string())))]);
    let server = HttpServer::create_port(port, endpoints).expect("Failed to create server");
    let _server_thread = thread::spawn(move || {
        server.start_blocking().expect("Server failed to start")
    });
    
    // Give the server time to start
    thread::sleep(std::time::Duration::from_millis(100));
    
    let body: String = ureq::get(format!("http://localhost:{port}/status").as_str())
        .set("Example-Header", "header value")
        .call()
        .unwrap()
        .into_string()
        .unwrap();

    let resp: Value = serde_json::from_str(body.as_str()).unwrap();
    assert_eq!(resp["status"], "ok");
}
