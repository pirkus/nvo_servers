# nvo_servers
Not very opinionated servers 

## Build
|Branch|OS           |Status                                                                                                                                                                                    |
|------|-------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
|main  |Linux        |[![CircleCI](https://dl.circleci.com/status-badge/img/gh/pirkus/nvo_servers/tree/main.svg?style=svg)](https://dl.circleci.com/status-badge/redirect/gh/pirkus/nvo_servers/tree/main)      |
|main  |macOS/FreeBSD|[![Build Status](https://api.cirrus-ci.com/github/pirkus/nvo_servers.svg)](https://cirrus-ci.com/github/pirkus/nvo_servers)                                                               |

### 🖥️ Platform Support
- Linux (epoll-based)
- macOS (kqueue-based)  
- FreeBSD (kqueue-based)

## Examples
For the best example, look into `./examples/async_linux_macos.rs`. 

This example starts a MongoDB in a container (Docker is required). It contains a POST handler that saves body into DB and a GET request that parses a path argument and loads data from the DB.

### Cargo.toml
```toml
[dependencies]
nvo_servers = { git = "https://github.com/pirkus/nvo_servers", tag = "v0.0.10" }
```

### Async I/O HTTP Server
Multithreaded server that automatically scales to CPU core count.

```rust
use nvo_servers::http::{
    AsyncHttpServer, AsyncHandler, AsyncRequest, Response,
    response_builder::ResponseBuilder,
};
use std::collections::HashSet;

pub fn main() {
    async fn status_handler(_: AsyncRequest) -> Result<Response, String> {
        Ok(Response::create(200, json!({"status": "ok"}).to_string()))
    }

    async fn echo_handler(req: AsyncRequest) -> Result<Response, String> {
        let body = req.body().await.map_err(|e| e.title)?;
        Ok(Response::create(200, body))
    }

    // Handler with path parameters
    async fn user_handler(req: AsyncRequest) -> Result<Response, String> {
        let user_id = req.path_params.get("id")
            .ok_or("Missing user ID")?;
        
        Ok(Response::create(200, format!("User ID: {}", user_id)))
    }

    let handlers = HashSet::from([
        AsyncHandler::new("GET", "/status", status_handler),
        AsyncHandler::new("POST", "/echo", echo_handler),
        AsyncHandler::new("GET", "/users/:id", user_handler),
    ]);

    env_logger::init();

    AsyncHttpServer::builder()
        .with_port(8090)
        .with_handlers(handlers)
        .build()
        .start_blocking()
}
```

### Chunked Response Example
```rust
use nvo_servers::http::response_builder::{ResponseBuilder, ChunkedResponseBuilder};

// Note: Chunked responses are primarily useful when building custom HTTP handling.
// The ResponseBuilder provides utilities for creating properly formatted chunked responses.
let chunked_response = ResponseBuilder::ok()
    .header("Content-Type", "text/plain")
    .chunked()
    .chunk("Hello")
    .chunk(" ")
    .chunk("World!")
    .build_http_string();

// This creates a properly formatted HTTP/1.1 chunked response:
// HTTP/1.1 200 OK
// Content-Type: text/plain
// Transfer-Encoding: chunked
//
// 5
// Hello
// 1
//  
// 6
// World!
// 0
//
```

### Blocking I/O HTTP Server
Also multithreaded with automatic CPU core scaling.

```rust
use nvo_servers::http::{HttpServer, Handler, Request, Response};
use std::collections::HashSet;

fn main() {
    fn hello_handler(_: &Request) -> Result<Response, String> {
        Ok(Response::create(200, "Hello, World!".to_string()))
    }

    let endpoints = HashSet::from([
        Handler::new("/hello", "GET", hello_handler),
    ]);

    HttpServer::create_port(8090, endpoints)
        .expect("Failed to create server")
        .start_blocking()
        .expect("Server error");
}
```

### Connection Pooling
The library includes built-in connection pooling:

```rust
use nvo_servers::http::connection_pool::ConnectionPool;
use std::time::Duration;

// Create a pool with max 100 connections and 5-minute timeout
let pool = ConnectionPool::new(100, Duration::from_secs(300));

// The pool is automatically used by the async servers
```

## Performance Testing
To send requests for testing:
```sh
# Basic load test
ab -k -n 1000000 -c 10 localhost:8090/status

# Test with keep-alive disabled
ab -n 10000 -c 100 localhost:8090/status

# Test chunked encoding
curl -H "Transfer-Encoding: chunked" -d @large_file.txt localhost:8090/echo
```

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.
