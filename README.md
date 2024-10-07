# nvo_servers
Not very opinionated servers 

## Build
|Branch|OS     |Status                                                                                                                                                                                    |
|------|-------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|main  |Linux  |[![CircleCI](https://dl.circleci.com/status-badge/img/gh/pirkus/nvo_servers/tree/main.svg?style=svg)](https://dl.circleci.com/status-badge/redirect/gh/pirkus/nvo_servers/tree/main)      |
|main  |FreeBSD|[![Build Status](https://api.cirrus-ci.com/github/pirkus/nvo_servers.svg)](https://cirrus-ci.com/github/pirkus/nvo_servers)                                                               |

## Examples
Are also available in ./examples directory
### Async I/O Http server
Multithreaded. Runs same amount of threads as CPU core count.

Todo:
1. Handle scenarios when handler_func throws
2. And much more ...
```rust
pub fn main() {
  async fn status_handler(_: AsyncRequest) -> Result<Response, String> {
    Ok(Response::create(200, json!({"status": "ok"}).to_string()))
  }

  let status_endpoint = AsyncHandler::new("GET", "/status", status_handler);

  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

  AsyncHttpServer::builder()
    .with_port(8090)
    .with_handlers(HashSet::from([status_endpoint]))
    .build()
    .start_blocking()
}
```
### Blocking I/O Http server
Multithreaded. Runs same amount of threads as CPU core count.

Todo:
1. Handle scenarios when handler_func throws
2. Graceful shutdown
3. Query params matching
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
