#![cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

pub fn poor_mans_random() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos()
}

pub mod test_with_server {
    use crate::http::async_http_server::{AsyncHttpServerBuilder, AsyncHttpServerTrt};

    pub fn test_with_server<F>(port: u16, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let server = AsyncHttpServerBuilder::new().with_port(port as usize).build();
        let _server_handle = std::thread::spawn(move || {
            server.start_blocking();
        });

        // Give the server time to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Run the test
        f();

        // Note: In a real test, you'd want to properly shutdown the server
    }
}
