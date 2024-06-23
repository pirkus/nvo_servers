mod common;

use std::collections::HashSet;
use std::thread;
use serde_json::Value;
use nvo_servers::http::mio_async_http_server::MioAsyncHttpServer;

#[test]
fn get_works() {
    env_logger::init();
    let port = 8090;
    let endpoints = HashSet::from([common::get_status_handler()]);
    let server = MioAsyncHttpServer::create_port(port, endpoints);
    let _server_thread = thread::spawn(move || server.start_blocking());
    let body: String = ureq::get(format!("http://localhost:{port}/status").as_str())
        .set("Example-Header", "header value")
        .call()
        .unwrap()
        .into_string()
        .unwrap();

    let resp: Value = serde_json::from_str(body.as_str()).unwrap();
    assert_eq!(resp["status"], "ok");
}