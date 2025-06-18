use nvo_servers::http::{AsyncHttpServer, AsyncHandler, AsyncRequest, Response};
use nvo_servers::http::async_http_server::{AsyncHttpServer, AsyncHttpServerBuilder, AsyncHttpServerTrt};
use nvo_servers::http::async_handler::AsyncHandler;
use nvo_servers::http::AsyncRequest;
use nvo_servers::http::response::Response;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::thread;

#[test]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
fn test_connection_leak_on_invalid_request() {
    // This test demonstrates that invalid requests can cause connection leaks
    let port = 8091;
    let handlers = HashSet::new();
    let server = Arc::new(
        AsyncHttpServer::builder()
            .with_port(port)
            .with_handlers(handlers)
            .build()
    );
    
    let server_clone = server.clone();
    thread::spawn(move || server_clone.start_blocking());
    
    // Wait for server to start
    thread::sleep(Duration::from_millis(100));
    
    // Send invalid request (missing HTTP version)
    let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    use std::io::Write;
    stream.write_all(b"GET /test\r\n\r\n").unwrap();
    
    // The connection should be closed, but it might leak
    thread::sleep(Duration::from_millis(100));
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
fn test_panic_in_handler_recovery() {
    // Test that server continues working after handler panic
    async fn panic_handler(_req: AsyncRequest) -> Result<Response, String> {
        panic!("Intentional panic for testing");
    }
    
    async fn normal_handler(_req: AsyncRequest) -> Result<Response, String> {
        Ok(Response::create(200, "OK".to_string()))
    }
    
    let port = 8092;
    let handlers = HashSet::from([
        AsyncHandler::new("GET", "/panic", panic_handler),
        AsyncHandler::new("GET", "/normal", normal_handler),
    ]);
    
    let server = Arc::new(
        AsyncHttpServer::builder()
            .with_port(port)
            .with_handlers(handlers)
            .build()
    );
    
    let server_clone = server.clone();
    thread::spawn(move || server_clone.start_blocking());
    
    thread::sleep(Duration::from_millis(100));
    
    // Trigger panic
    let panic_response = reqwest::blocking::get(format!("http://localhost:{}/panic", port));
    assert!(panic_response.is_ok());
    let response = panic_response.unwrap();
    assert_eq!(response.status(), 500);
    
    // Server should still work
    let normal_response = reqwest::blocking::get(format!("http://localhost:{}/normal", port)).unwrap();
    assert_eq!(normal_response.status(), 200);
    assert_eq!(normal_response.text().unwrap(), "OK");
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
fn test_large_request_handling() {
    // Test handling of requests larger than 8192 bytes buffer
    async fn echo_handler(req: AsyncRequest) -> Result<Response, String> {
        let body = req.body().await.map_err(|e| e.to_string())?;
        Ok(Response::create(200, body))
    }
    
    let port = 8093;
    let handlers = HashSet::from([
        AsyncHandler::new("POST", "/echo", echo_handler),
    ]);
    
    let server = Arc::new(
        AsyncHttpServer::builder()
            .with_port(port)
            .with_handlers(handlers)
            .build()
    );
    
    let server_clone = server.clone();
    thread::spawn(move || server_clone.start_blocking());
    
    thread::sleep(Duration::from_millis(100));
    
    // Send large request
    let large_body = "x".repeat(10000);
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("http://localhost:{}/echo", port))
        .body(large_body.clone())
        .send();
    
    // This will likely fail with current implementation
    assert!(response.is_ok(), "Large request should be handled");
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
fn test_concurrent_connections_stress() {
    // Test handling many concurrent connections
    async fn slow_handler(_req: AsyncRequest) -> Result<Response, String> {
        // Simulate slow processing without tokio
        std::thread::sleep(Duration::from_millis(10));
        Ok(Response::create(200, "OK".to_string()))
    }
    
    let port = 8094;
    let handlers = HashSet::from([
        AsyncHandler::new("GET", "/slow", slow_handler),
    ]);
    
    let server = Arc::new(
        AsyncHttpServer::builder()
            .with_port(port)
            .with_handlers(handlers)
            .build()
    );
    
    let server_clone = server.clone();
    thread::spawn(move || server_clone.start_blocking());
    
    thread::sleep(Duration::from_millis(100));
    
    // Spawn many concurrent requests
    let handles: Vec<_> = (0..100)
        .map(|_| {
            let port = port;
            thread::spawn(move || {
                reqwest::blocking::get(format!("http://localhost:{}/slow", port))
            })
        })
        .collect();
    
    // All should complete successfully
    for handle in handles {
        let result = handle.join().unwrap();
        assert!(result.is_ok(), "Concurrent request should succeed");
    }
}

#[test]
#[cfg(target_os = "freebsd")]
fn test_bsd_single_event_bug() {
    // BSD implementation only processes one event at a time
    // This can cause starvation under load
    
    async fn counter_handler(_req: AsyncRequest) -> Result<Response, String> {
        Ok(Response::create(200, "counted".to_string()))
    }
    
    let port = 8095;
    let handlers = HashSet::from([
        AsyncHandler::new("GET", "/count", counter_handler),
    ]);
    
    let server = Arc::new(
        AsyncHttpServer::builder()
            .with_port(port)
            .with_handlers(handlers)
            .build()
    );
    
    let server_clone = server.clone();
    thread::spawn(move || server_clone.start_blocking());
    
    thread::sleep(Duration::from_millis(100));
    
    // Send multiple requests simultaneously
    let start = std::time::Instant::now();
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let port = port;
            thread::spawn(move || {
                reqwest::blocking::get(format!("http://localhost:{}/count", port))
            })
        })
        .collect();
    
    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    
    let elapsed = start.elapsed();
    
    // With single event processing, this takes much longer than it should
    println!("BSD event processing took: {:?}", elapsed);
    // This assertion might fail on BSD due to the bug
    assert!(elapsed < Duration::from_secs(1), "Requests should complete quickly");
}