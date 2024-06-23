use env_logger::Env;
use nvo_servers::http::async_http_server::{AsyncUnixHttpServer, AsyncHttpServerTrt};
use nvo_servers::http::handler::Handler;
use nvo_servers::http::response::Response;
use serde_json::json;
use std::collections::HashSet;

fn main() {
    let status_endpoint = Handler::new("/status", "GET", |_| {
        Ok(Response::create(200, json!({"status": "ok"}).to_string()))
    });

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    AsyncUnixHttpServer::create_port(8090, HashSet::from([status_endpoint])).start_blocking()
}
