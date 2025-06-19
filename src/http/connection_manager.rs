use std::net::TcpStream;
use std::sync::Arc;
use dashmap::DashMap;
use super::ConnState;

/// Functional connection manager using lock-free concurrent data structures
#[derive(Clone)]
pub struct ConnectionManager {
    connections: Arc<DashMap<i32, (TcpStream, ConnState)>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {
            connections: Arc::new(DashMap::new()),
        }
    }
    
    /// Insert a new connection - functional approach with no explicit locking
    pub fn insert(&self, fd: i32, connection: TcpStream, state: ConnState) {
        self.connections.insert(fd, (connection, state));
    }
    
    /// Take a connection for processing - returns Option without explicit locking
    pub fn take(&self, fd: i32) -> Option<(TcpStream, ConnState)> {
        self.connections.remove(&fd).map(|(_, value)| value)
    }
    
    /// Return a connection after processing - functional update
    pub fn return_connection(&self, fd: i32, connection: TcpStream, state: ConnState) {
        if state != ConnState::Flush {
            self.connections.insert(fd, (connection, state));
        }
    }
    
    /// Remove a connection completely
    pub fn remove(&self, fd: i32) -> Option<(TcpStream, ConnState)> {
        self.connections.remove(&fd).map(|(_, value)| value)
    }
    
    /// Get the number of active connections
    pub fn len(&self) -> usize {
        self.connections.len()
    }
    
    /// Check if there are no connections
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
    
    /// Clean up connections based on a predicate - functional approach
    pub fn cleanup_connections<F>(&self, predicate: F) -> Vec<i32>
    where
        F: Fn(&i32, &(TcpStream, ConnState)) -> bool,
    {
        // Collect connections to remove using functional approach
        let to_remove: Vec<i32> = self.connections
            .iter()
            .filter_map(|entry| {
                if predicate(entry.key(), entry.value()) {
                    Some(*entry.key())
                } else {
                    None
                }
            })
            .collect();
        
        // Remove collected connections
        to_remove.iter()
            .filter_map(|fd| self.connections.remove(fd))
            .count();
        
        to_remove
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use crate::http::AsyncRequest;
    use crate::http::async_handler::AsyncHandler;
    use crate::http::headers::Headers;
    use crate::typemap::DepsMap;
    use std::collections::HashMap;
    use std::sync::Mutex;
    
    fn create_test_connection() -> TcpStream {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        TcpStream::connect(addr).unwrap()
    }
    
    #[test]
    fn test_connection_insert_and_take() {
        let manager = ConnectionManager::new();
        let conn = create_test_connection();
        
        manager.insert(1, conn, ConnState::Read(Vec::new()));
        assert_eq!(manager.len(), 1);
        
        let taken = manager.take(1);
        assert!(taken.is_some());
        assert_eq!(manager.len(), 0);
    }
    
    #[test]
    fn test_connection_return() {
        let manager = ConnectionManager::new();
        let conn = create_test_connection();
        
        // Test return with non-Flush state
        manager.return_connection(1, conn, ConnState::Read(Vec::new()));
        assert_eq!(manager.len(), 1);
        
        // Test return with Flush state (should not insert)
        let conn2 = create_test_connection();
        manager.return_connection(2, conn2, ConnState::Flush);
        assert_eq!(manager.len(), 1);
    }
    
    #[test]
    fn test_cleanup_connections() {
        let manager = ConnectionManager::new();
        
        // Add multiple connections
        (0..5).for_each(|i| {
            let conn = create_test_connection();
            let state = if i % 2 == 0 {
                ConnState::Flush
            } else {
                ConnState::Read(Vec::new())
            };
            manager.insert(i, conn, state);
        });
        
        // Clean up connections in Flush state
        let removed = manager.cleanup_connections(|_, (_, state)| {
            matches!(state, ConnState::Flush)
        });
        
        assert_eq!(removed.len(), 3); // 0, 2, 4 are Flush
        assert_eq!(manager.len(), 2);
    }
}