mod common;

#[test]
#[cfg(target_os = "freebsd")]
fn get_works() {
    use nvo_servers::http::async_bsd_http_server::AsyncBsdHttpServer;
    use nvo_servers::http::async_bsd_http_server::AsyncHttpServerTrt;

    use env_logger::Env;
    use serde_json::Value;
    use std::collections::HashSet;
    use std::thread;

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let port = 8090;
    let endpoints = HashSet::from([common::get_status_handler()]);
    println!("Before");
    let server = AsyncBsdHttpServer::create_port(port, endpoints);
    println!("After");
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