# nvo_servers
Not very opinionated servers 

## Build
|Branch|Status                                                                                                                                                                              |
|------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|main  |[![CircleCI](https://dl.circleci.com/status-badge/img/gh/pirkus/nvo_servers/tree/main.svg?style=svg)](https://dl.circleci.com/status-badge/redirect/gh/pirkus/nvo_servers/tree/main)|

## Examples
Are also available in ./examples directory
### Async I/O Http server
Multithreaded. Runs same amount of threads as CPU core count.

Todo:
1. Handle scenarios when handler_func throws
2. Graceful shutdown
3. Path params matching
4. Query params matching
5. And much more ...
```rust
fn main() {
    let endpoints = HashSet::from([
        Endpoint::new(
            "/",
            "GET",
            || Ok(Response::create(200, json!({"status": "ok"}).to_string()) ),
        ),
        Endpoint::new(
            "/who-am-i",
            "GET",
            || Ok(Response::create(200, json!({"name": "Filip", "forehead_size": "never-ending"}).to_string()) ),
        )]);

    AsyncHttpServer::create_port(8090, endpoints)
        .start_blocking();
}
```
### Blocking I/O Http server
Multithreaded. Runs same amount of threads as CPU core count.

Todo:
1. Handle scenarios when handler_func throws
2. Graceful shutdown
3. Path params matching
4. Query params matching
5. And much more ...
```rust
fn main() {
  //...
  HttpServer::create_port(8090, endpoints)
      .start_blocking();
}
```

#### To send requests for testing one can use:
```sh
ab -k -n 1000000 -c 10 localhost:8090/status
```
