use std::net::TcpStream;
use std::sync::Arc;
use dashmap::DashMap;
use std::time::{Duration, Instant};
use log::debug;

/// Connection wrapper with metadata for pool management
#[derive(Debug)]
struct PooledConnection {
    stream: TcpStream,
    last_used: Instant,
}

/// Functional connection pool for reusing TCP connections
#[derive(Clone)]
pub struct ConnectionPool {
    // Using DashMap for lock-free concurrent access
    // Key is a connection ID, value is the pooled connection
    connections: Arc<DashMap<u64, PooledConnection>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    max_idle_time: Duration,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new() -> Self {
        Self::with_max_idle_time(Duration::from_secs(60))
    }
    
    /// Create a pool with custom idle timeout
    pub fn with_max_idle_time(max_idle_time: Duration) -> Self {
        ConnectionPool {
            connections: Arc::new(DashMap::new()),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            max_idle_time,
        }
    }
    
    /// Get an idle connection from the pool
    pub fn get(&self) -> Option<TcpStream> {
        let now = Instant::now();
        
        // Find and remove the first valid connection functionally
        self.connections
            .iter()
            .find_map(|entry| {
                let id = *entry.key();
                let conn = entry.value();
                
                // Check if connection is still fresh
                if now.duration_since(conn.last_used) < self.max_idle_time {
                    // Try to remove and return it
                    self.connections.remove(&id)
                        .and_then(|(_, mut pooled)| {
                            pooled.last_used = now;
                            Some(pooled.stream)
                        })
                } else {
                    // Connection is stale, remove it
                    self.connections.remove(&id);
                    None
                }
            })
    }
    
    /// Return a connection to the pool
    pub fn put(&self, stream: TcpStream) {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pooled = PooledConnection {
            stream,
            last_used: Instant::now(),
        };
        
        self.connections.insert(id, pooled);
        debug!("Connection {} returned to pool", id);
    }
    
    /// Clean up stale connections
    pub fn cleanup(&self) {
        let now = Instant::now();
        
        // Collect and remove stale connections functionally
        let stale_ids: Vec<u64> = self.connections
            .iter()
            .filter_map(|entry| {
                if now.duration_since(entry.value().last_used) > self.max_idle_time {
                    Some(*entry.key())
                } else {
                    None
                }
            })
            .collect();
        
        // Remove stale connections
        let removed_count = stale_ids.iter()
            .filter_map(|id| self.connections.remove(id))
            .count();
            
        if removed_count > 0 {
            debug!("Cleaned up {} stale connections", removed_count);
        }
    }
    
    /// Get the current size of the pool
    pub fn size(&self) -> usize {
        self.connections.len()
    }
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;
    
    #[test]
    fn test_pool_operations() {
        let pool = ConnectionPool::new();
        
        // Create a test listener
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        
        // Add connections to the pool
        for _ in 0..3 {
            let stream = TcpStream::connect(addr).unwrap();
            pool.put(stream);
        }
        
        assert_eq!(pool.size(), 3);
        
        // Get a connection
        let conn = pool.get();
        assert!(conn.is_some());
        assert_eq!(pool.size(), 2);
        
        // Return it
        pool.put(conn.unwrap());
        assert_eq!(pool.size(), 3);
    }
    
    #[test]
    fn test_cleanup() {
        let pool = ConnectionPool::with_max_idle_time(Duration::from_millis(100));
        
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        
        // Add a connection
        let stream = TcpStream::connect(addr).unwrap();
        pool.put(stream);
        assert_eq!(pool.size(), 1);
        
        // Wait for it to become stale
        thread::sleep(Duration::from_millis(150));
        
        // Cleanup should remove it
        pool.cleanup();
        assert_eq!(pool.size(), 0);
    }
    
    #[test]
    fn test_concurrent_access() {
        let pool = Arc::new(ConnectionPool::new());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        
        // Spawn multiple threads that add connections
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let pool_clone = pool.clone();
                let addr = addr.clone();
                thread::spawn(move || {
                    let stream = TcpStream::connect(addr).unwrap();
                    pool_clone.put(stream);
                })
            })
            .collect();
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        assert_eq!(pool.size(), 5);
    }
} 