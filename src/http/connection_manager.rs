use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::net::TcpStream;
use std::io;
use log::{error, debug};
use super::ConnState;
use crate::http::AsyncRequest;

/// Thread-safe connection manager that handles connection lifecycle
pub struct ConnectionManager {
    connections: Arc<Mutex<HashMap<i32, (TcpStream, ConnState)>>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Insert a new connection with initial state
    pub fn insert(&self, fd: i32, connection: TcpStream, state: ConnState) -> Result<(), io::Error> {
        match self.connections.lock() {
            Ok(mut conns) => {
                conns.insert(fd, (connection, state));
                debug!("Inserted connection with fd: {}", fd);
                Ok(())
            }
            Err(e) => {
                error!("Failed to acquire lock for insert: {}", e);
                Err(io::Error::new(io::ErrorKind::Other, "Lock poisoned"))
            }
        }
    }

    /// Take a connection for processing
    pub fn take(&self, fd: i32) -> Option<(TcpStream, ConnState)> {
        match self.connections.lock() {
            Ok(mut conns) => {
                let result = conns.remove(&fd);
                if result.is_some() {
                    debug!("Took connection with fd: {}", fd);
                }
                result
            }
            Err(e) => {
                error!("Failed to acquire lock for take: {}", e);
                None
            }
        }
    }

    /// Return a connection after processing with new state
    pub fn return_connection(&self, fd: i32, connection: TcpStream, state: ConnState) -> Result<(), io::Error> {
        match self.connections.lock() {
            Ok(mut conns) => {
                // Check if connection wasn't already removed (e.g., due to error)
                if !conns.contains_key(&fd) {
                    conns.insert(fd, (connection, state));
                    debug!("Returned connection with fd: {}", fd);
                    Ok(())
                } else {
                    error!("Connection {} already exists, possible race condition", fd);
                    Err(io::Error::new(io::ErrorKind::AlreadyExists, "Connection already exists"))
                }
            }
            Err(e) => {
                error!("Failed to acquire lock for return: {}", e);
                Err(io::Error::new(io::ErrorKind::Other, "Lock poisoned"))
            }
        }
    }

    /// Remove a connection permanently
    pub fn remove(&self, fd: i32) -> Option<(TcpStream, ConnState)> {
        match self.connections.lock() {
            Ok(mut conns) => {
                let result = conns.remove(&fd);
                if result.is_some() {
                    debug!("Removed connection with fd: {}", fd);
                }
                result
            }
            Err(e) => {
                error!("Failed to acquire lock for remove: {}", e);
                None
            }
        }
    }

    /// Get the current number of connections
    pub fn connection_count(&self) -> usize {
        match self.connections.lock() {
            Ok(conns) => conns.len(),
            Err(_) => 0,
        }
    }

    /// Clean up connections based on a predicate
    pub fn cleanup_connections<F>(&self, mut predicate: F) -> Vec<i32>
    where
        F: FnMut(&ConnState) -> bool,
    {
        match self.connections.lock() {
            Ok(mut conns) => {
                let to_remove: Vec<i32> = conns
                    .iter()
                    .filter(|(_, (_, state))| predicate(state))
                    .map(|(fd, _)| *fd)
                    .collect();
                
                for fd in &to_remove {
                    conns.remove(fd);
                }
                
                debug!("Cleaned up {} connections", to_remove.len());
                to_remove
            }
            Err(e) => {
                error!("Failed to acquire lock for cleanup: {}", e);
                Vec::new()
            }
        }
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
    use std::net::TcpListener;
    use crate::http::AsyncRequest;

    #[test]
    fn test_connection_lifecycle() {
        let manager = ConnectionManager::new();
        
        // Create a dummy connection
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpStream::connect(addr).unwrap();
        let fd = 1;
        
        // Test insert
        assert!(manager.insert(fd, stream.try_clone().unwrap(), ConnState::Read(vec![])).is_ok());
        assert_eq!(manager.connection_count(), 1);
        
        // Test take
        let taken = manager.take(fd);
        assert!(taken.is_some());
        assert_eq!(manager.connection_count(), 0);
        
        // Test return - create a dummy request for Write state
        let (conn, _) = taken.unwrap();
        
        // Create a mock handler function
        async fn dummy_handler(_: AsyncRequest) -> Result<crate::http::response::Response, String> {
            Ok(crate::http::response::Response::create(200, "OK".to_string()))
        }
        
        let handler = Arc::new(crate::http::AsyncHandler::new("GET", "/", dummy_handler));
        let dummy_req = AsyncRequest::create(
            "/",
            handler,
            std::collections::HashMap::new(),
            Arc::new(crate::typemap::DepsMap::default()),
            crate::http::headers::Headers::new(),
            Arc::new(std::sync::Mutex::new(conn.try_clone().unwrap())),
        );
        
        assert!(manager.return_connection(fd, conn, ConnState::Write(dummy_req, 0)).is_ok());
        assert_eq!(manager.connection_count(), 1);
        
        // Test remove
        let removed = manager.remove(fd);
        assert!(removed.is_some());
        assert_eq!(manager.connection_count(), 0);
    }

    #[test]
    fn test_cleanup_connections() {
        let manager = ConnectionManager::new();
        
        // Add some connections
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        
        for i in 0..5 {
            let stream = TcpStream::connect(addr).unwrap();
            let state = if i % 2 == 0 {
                ConnState::Flush
            } else {
                ConnState::Read(vec![])
            };
            manager.insert(i, stream, state).unwrap();
        }
        
        // Clean up flush connections
        let cleaned = manager.cleanup_connections(|state| matches!(state, ConnState::Flush));
        assert_eq!(cleaned.len(), 3); // 0, 2, 4
        assert_eq!(manager.connection_count(), 2);
    }
}