fn main() {
    #[cfg(target_os = "linux")]
    unix_example::main()
}

#[cfg(target_os = "linux")]
mod unix_example {
    use bson::doc;
    use env_logger::Env;
    use mongodb::{bson, Client, Collection};
    use nvo_servers::futures::workers::Workers;
    use nvo_servers::http::async_handler::AsyncHandler;
    use nvo_servers::http::async_http_server::{AsyncHttpServer, AsyncHttpServerTrt};
    use nvo_servers::http::response::Response;
    use nvo_servers::http::AsyncRequest;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::collections::HashSet;
    use testcontainers::core::{ContainerPort, WaitFor};
    use testcontainers::runners::SyncRunner;
    use testcontainers::{GenericImage, ImageExt};

    #[derive(Serialize, Deserialize, Debug)]
    struct Restaurant {
        name: String,
        cuisine: String,
    }

    pub fn main() {
        async fn status_handler(req: AsyncRequest) -> Result<Response, String> {
            let mongo = req.deps.get::<Client>().unwrap();
            let my_coll: Collection<Restaurant> = mongo.database("gym-log").collection("restaurants");
            let doc = Restaurant {
                name: "bri'ish".to_string(),
                cuisine: "terrible".to_string(),
            };
            my_coll.insert_one(&doc, None).await.unwrap();
            let restaurant = my_coll.find_one(doc! { "name": "bri'ish" }, None).await.unwrap().unwrap();
            println!("Name {name}, cuisine: {cuisine}", name = restaurant.name, cuisine = restaurant.cuisine);

            Ok(Response::create(200, json!({"status": "ok"}).to_string()))
        }

        async fn post_handler(_req: AsyncRequest) -> Result<Response, String> {
            println!("{:?}", _req.headers);
            let buf = _req.body().await;
            Ok(Response::create(200, json!({"recvd_body": buf}).to_string()))
        }

        let status_endpoint = AsyncHandler::new("GET", "/status", status_handler);
        let post_endpoint = AsyncHandler::new("POST", "/post", post_handler);

        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

        let async_runtime = Workers::new(1);
        let mongo = GenericImage::new("mongo", "6.0.7")
            .with_wait_for(WaitFor::message_on_stdout("server is ready"))
            .with_exposed_port(ContainerPort::Tcp(27017))
            .with_env_var("MONGO_INITDB_DATABASE", "gym-log")
            .with_env_var("MONGO_INITDB_ROOT_USERNAME", "root")
            .with_env_var("MONGO_INITDB_ROOT_PASSWORD", "root")
            .start()
            .unwrap();
        let mongo_port = mongo.get_host_port_ipv4(ContainerPort::Tcp(27017)).unwrap();

        let uri = format!("mongodb://root:root@localhost:{port}", port = mongo_port);
        let client = async_runtime.queue_with_result(async move { Client::with_uri_str(uri).await }).unwrap().get().unwrap();

        AsyncHttpServer::builder()
            .with_port(8090)
            .with_handlers(HashSet::from([status_endpoint, post_endpoint]))
            .with_dep(client)
            .build()
            .start_blocking()
    }
}
