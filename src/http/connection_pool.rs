use std::collections::VecDeque;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::io;

/// A pooled connection with metadata
#[derive(Debug)]
struct PooledConnection {
    stream: TcpStream,
    last_used: Instant,
}

/// Simple connection pool for reusing TCP connections
#[derive(Clone)]
pub struct ConnectionPool {
    connections: Arc<Mutex<VecDeque<PooledConnection>>>,
    max_connections: usize,
    max_idle_time: Duration,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(max_connections: usize, max_idle_time: Duration) -> Self {
        ConnectionPool {
            connections: Arc::new(Mutex::new(VecDeque::with_capacity(max_connections))),
            max_connections,
            max_idle_time,
        }
    }

    /// Get a connection from the pool or None if pool is empty
    pub fn get(&self) -> Option<TcpStream> {
        let mut pool = self.connections.lock().ok()?;
        let now = Instant::now();
        
        // Remove expired connections using functional style
        pool.retain(|conn| now.duration_since(conn.last_used) < self.max_idle_time);
        
        pool.pop_front().map(|conn| conn.stream)
    }

    /// Return a connection to the pool
    pub fn put(&self, stream: TcpStream) -> Result<(), io::Error> {
        let mut pool = self.connections.lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Pool lock poisoned"))?;
        
        // Only add if we haven't reached max capacity
        if pool.len() < self.max_connections {
            pool.push_back(PooledConnection {
                stream,
                last_used: Instant::now(),
            });
        }
        
        Ok(())
    }

    /// Clear all connections from the pool
    pub fn clear(&self) {
        if let Ok(mut pool) = self.connections.lock() {
            pool.clear();
        }
    }

    /// Get the current number of idle connections
    pub fn idle_count(&self) -> usize {
        self.connections
            .lock()
            .map(|pool| pool.len())
            .unwrap_or(0)
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        // Default: max 100 connections, 5 minute idle timeout
        Self::new(100, Duration::from_secs(300))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, SocketAddr};
    use std::thread;

    fn create_test_connection(addr: SocketAddr) -> TcpStream {
        TcpStream::connect(addr).expect("Failed to connect")
    }

    #[test]
    fn test_connection_pool_basic() {
        // Start a dummy server
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind");
        let addr = listener.local_addr().expect("Failed to get local addr");
        
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stream.is_ok() {
                    // Just accept connections
                }
            }
        });

        let pool = ConnectionPool::new(2, Duration::from_secs(60));
        
        // Add a connection
        let conn1 = create_test_connection(addr);
        pool.put(conn1).expect("Failed to put connection");
        assert_eq!(pool.idle_count(), 1);
        
        // Get it back
        let retrieved = pool.get();
        assert!(retrieved.is_some());
        assert_eq!(pool.idle_count(), 0);
        
        // Return it
        pool.put(retrieved.unwrap()).expect("Failed to return connection");
        assert_eq!(pool.idle_count(), 1);
    }

    #[test]
    fn test_max_connections() {
        let pool = ConnectionPool::new(2, Duration::from_secs(60));
        
        // Create dummy connections (they don't need to be real for this test)
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind");
        let addr = listener.local_addr().expect("Failed to get local addr");
        
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stream.is_ok() {
                    // Just accept connections
                }
            }
        });

        // Add connections up to the limit
        for _ in 0..3 {
            let conn = create_test_connection(addr);
            pool.put(conn).expect("Failed to put connection");
        }
        
        // Pool should only have 2 connections (max_connections)
        assert_eq!(pool.idle_count(), 2);
    }
} 