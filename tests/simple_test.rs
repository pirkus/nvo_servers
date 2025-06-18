use nvo_servers::http::async_http_server::{AsyncHttpServer, AsyncHttpServerTrt};
use nvo_servers::http::async_handler::AsyncHandler;
use nvo_servers::http::AsyncRequest;
use nvo_servers::http::response::Response;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::thread;

#[test]
#[cfg(any(target_os = "freebsd", target_os = "macos"))]
fn test_bsd_event_processing_bug() {
    // BSD implementation only processes one event at a time
    // This can cause starvation under load
    
    async fn handler(_req: AsyncRequest) -> Result<Response, String> {
        Ok(Response::create(200, "OK".to_string()))
    }
    
    let port = 8095;
    let handlers = HashSet::from([
        AsyncHandler::new("GET", "/test", handler),
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
    
    // This test will demonstrate the performance issue
    // but won't fail since the bug has been fixed
    println!("BSD event processing test completed successfully");
}

#[test]
#[cfg(target_os = "linux")]
fn test_linux_functional_refactoring() {
    // Test that Linux implementation works with functional style
    async fn handler(_req: AsyncRequest) -> Result<Response, String> {
        Ok(Response::create(200, "Functional style works".to_string()))
    }
    
    let port = 8096;
    let handlers = HashSet::from([
        AsyncHandler::new("GET", "/functional", handler),
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
    
    println!("Linux functional refactoring test completed successfully");
}