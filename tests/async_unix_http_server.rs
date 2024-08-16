mod common;

#[test]
#[cfg(target_os = "linux")]
fn get_works() {
    use nvo_servers::http::async_http_server::AsyncHttpServer;
    use serde_json::Value;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    use crate::common;

    env_logger::init();
    let port = 8090;
    let endpoints = HashSet::from([common::get_status_handler()]);
    let server = Arc::new(AsyncHttpServer::create_port(port, endpoints));
    let server_clj = server.clone();
    let _server_thread = thread::spawn(move || server_clj.start_blocking());

    common::wait_for_server_to_start(server);

    let body: String = ureq::get(format!("http://localhost:{port}/status").as_str())
        .set("Example-Header", "header value")
        .call()
        .unwrap()
        .into_string()
        .unwrap();

    let resp: Value = serde_json::from_str(body.as_str()).unwrap();
    assert_eq!(resp["status"], "ok");
}
