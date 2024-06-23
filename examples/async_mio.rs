use std::collections::HashSet;
use env_logger::Env;
use serde_json::json;
use nvo_servers::http::handler::Handler;
use nvo_servers::http::mio_async_http_server::MioAsyncHttpServer;
use nvo_servers::http::response::Response;

fn main() {
    let status_endpoint = Handler::new("/status", "GET", |_| {
        Ok(Response::create(200, json!({"status": "ok"}).to_string()))
    });

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    MioAsyncHttpServer::create_port(9000, HashSet::from([status_endpoint]))
        .start_blocking()
}