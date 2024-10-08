fn main() {
    #[cfg(target_os = "linux")]
    unix_example::main()
}

#[cfg(target_os = "linux")]
mod unix_example {
    use env_logger::Env;
    use nvo_servers::http::async_handler::AsyncHandler;
    use nvo_servers::http::async_http_server::{AsyncHttpServer, AsyncHttpServerTrt};
    use nvo_servers::http::response::Response;
    use nvo_servers::http::AsyncRequest;
    use serde_json::json;
    use std::collections::HashSet;

    pub fn main() {
        async fn status_handler(_: AsyncRequest) -> Result<Response, String> {
            Ok(Response::create(200, json!({"status": "ok"}).to_string()))
        }

        let status_endpoint = AsyncHandler::new("GET", "/status", status_handler);

        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

        AsyncHttpServer::builder().with_port(8090).with_handlers(HashSet::from([status_endpoint])).build().start_blocking()
    }
}
