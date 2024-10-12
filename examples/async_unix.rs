fn main() {
    #[cfg(target_os = "linux")]
    unix_example::main()
}

#[cfg(target_os = "linux")]
mod unix_example {
    use env_logger::Env;
    use mongodb::{bson, Client, Collection};
    use nvo_servers::futures::workers::Workers;
    use nvo_servers::http::async_handler::AsyncHandler;
    use nvo_servers::http::async_http_server::{AsyncHttpServer, AsyncHttpServerTrt};
    use nvo_servers::http::response::Response;
    use nvo_servers::http::AsyncRequest;
    use serde_json::json;
    use std::collections::HashSet;
    use serde::{ Deserialize, Serialize };
    use bson::doc;

    #[derive(Serialize, Deserialize, Debug)]
    struct Restaurant {
        name: String,
        cuisine: String,
    }

    pub fn main() {
        async fn status_handler(req: AsyncRequest) -> Result<Response, String> {
            let mongo = req.deps.get::<Client>();
            let my_coll: Collection<Restaurant> = mongo
                .unwrap()
                .database("gym-log")
                .collection("restaurants");
            let doc = Restaurant { name: "kok".to_string(), cuisine: "terrible".to_string() };
            my_coll.insert_one(&doc).await.unwrap();
            let restaurant = my_coll.find_one(doc! { "name": "kok" }).await.unwrap().unwrap();
            println!("Name {name}, cuisine: {cuisine}", name = restaurant.name, cuisine = restaurant.cuisine);
            
            Ok(Response::create(200, json!({"status": "ok"}).to_string()))
        }

        let status_endpoint = AsyncHandler::new("GET", "/status", status_handler);

        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

        let async_runtime = Workers::new(1);
        let uri = "mongodb+srv://pirkus:kokotko@gym-log.w9emv.mongodb.net/?retryWrites=true&w=majority&appName=gym-log";
        let client = async_runtime.queue_with_result( async move { Client::with_uri_str(uri).await }).unwrap().get().unwrap();

        AsyncHttpServer::builder().with_port(8090).with_handlers(HashSet::from([status_endpoint])).with_dep(client).build().start_blocking()
    }
}
