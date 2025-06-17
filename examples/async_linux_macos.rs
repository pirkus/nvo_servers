fn main() {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    unix_example::main()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
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
        pub name: String,
    }

    async fn get_handler(req: AsyncRequest) -> Result<Response, String> {
        let mongo = req.deps.get::<Client>().unwrap();
        let my_coll: Collection<Restaurant> = mongo.database("gym-log").collection("restaurants");
        let restaurant_name = req.path_params.get("name").unwrap();
        match my_coll.find_one(doc! { "name": restaurant_name }, None).await {
            Ok(r) => match r {
                Some(r) => {
                    let name = r.name;
                    Ok(Response::create(200, json!({"name": name}).to_string()))
                }
                None => Ok(Response::create(404, json!({"err": "restaurant not found"}).to_string())),
            },
            Err(e) => {
                let e = e.to_string();
                Ok(Response::create(500, json!({"err": e}).to_string()))
            }
        }
    }

    async fn post_handler(req: AsyncRequest) -> Result<Response, String> {
        let mongo = req.deps.get::<Client>().unwrap();
        let my_coll: Collection<Restaurant> = mongo.database("gym-log").collection("restaurants");
        let buf = req.body().await.unwrap();
        let doc = Restaurant { name: buf.clone() };

        my_coll.insert_one(&doc, None).await.unwrap();
        Ok(Response::create(200, json!({"name": buf}).to_string()))
    }

    pub fn main() {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

        let async_runtime = Workers::new(1);
        let mongo_container = GenericImage::new("mongo", "6.0.7")
            .with_wait_for(WaitFor::message_on_stdout("server is ready"))
            .with_exposed_port(ContainerPort::Tcp(27017))
            .with_env_var("MONGO_INITDB_DATABASE", "gym-log")
            .with_env_var("MONGO_INITDB_ROOT_USERNAME", "root")
            .with_env_var("MONGO_INITDB_ROOT_PASSWORD", "root")
            .start()
            .unwrap();
        let mongo_port = mongo_container.get_host_port_ipv4(ContainerPort::Tcp(27017)).unwrap();

        let uri = format!("mongodb://root:root@localhost:{port}", port = mongo_port);
        let client = async_runtime.queue_with_result(async move { Client::with_uri_str(uri).await }).unwrap().get().unwrap();

        AsyncHttpServer::builder()
            .with_port(8090)
            .with_handlers(HashSet::from([AsyncHandler::new("GET", "/get/:name", get_handler), AsyncHandler::new("POST", "/post", post_handler)]))
            .with_dep(client)
            .build()
            .start_blocking();
    }
}
