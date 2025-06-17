use std::fmt;
use std::io;

/// Comprehensive error type for the HTTP server
#[derive(Debug, Clone)]
pub enum ServerError {
    /// IO-related errors
    Io(String),
    
    /// HTTP request parsing errors
    InvalidRequest {
        reason: String,
        request_line: Option<String>,
    },
    
    /// Path matching errors
    PathMismatch {
        pattern: String,
        path: String,
    },
    
    /// Missing required headers
    MissingHeader {
        header_name: String,
    },
    
    /// Invalid header value
    InvalidHeader {
        header_name: String,
        value: String,
        reason: String,
    },
    
    /// Handler errors
    HandlerError {
        path: String,
        method: String,
        error: String,
    },
    
    /// Resource not found
    NotFound {
        path: String,
        method: String,
    },
    
    /// Internal server errors
    Internal(String),
    
    /// Worker pool errors
    WorkerPoolError(String),
    
    /// Configuration errors
    Configuration(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::Io(msg) => write!(f, "IO error: {}", msg),
            ServerError::InvalidRequest { reason, request_line } => {
                match request_line {
                    Some(line) => write!(f, "Invalid request '{}': {}", line, reason),
                    None => write!(f, "Invalid request: {}", reason),
                }
            }
            ServerError::PathMismatch { pattern, path } => {
                write!(f, "Path '{}' does not match pattern '{}'", path, pattern)
            }
            ServerError::MissingHeader { header_name } => {
                write!(f, "Missing required header: {}", header_name)
            }
            ServerError::InvalidHeader { header_name, value, reason } => {
                write!(f, "Invalid header '{}' with value '{}': {}", header_name, value, reason)
            }
            ServerError::HandlerError { path, method, error } => {
                write!(f, "Handler error for {} {}: {}", method, path, error)
            }
            ServerError::NotFound { path, method } => {
                write!(f, "Resource not found: {} {}", method, path)
            }
            ServerError::Internal(msg) => write!(f, "Internal server error: {}", msg),
            ServerError::WorkerPoolError(msg) => write!(f, "Worker pool error: {}", msg),
            ServerError::Configuration(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for ServerError {}

impl From<io::Error> for ServerError {
    fn from(error: io::Error) -> Self {
        ServerError::Io(error.to_string())
    }
}

/// Result type alias for server operations
pub type ServerResult<T> = Result<T, ServerError>;

/// HTTP-specific error that can be converted to a response
#[derive(Debug, Clone)]
pub struct HttpError {
    pub status_code: u16,
    pub message: String,
    pub details: Option<String>,
}

impl HttpError {
    pub fn new(status_code: u16, message: impl Into<String>) -> Self {
        Self {
            status_code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(404, message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(500, message)
    }

    pub fn to_response_body(&self) -> String {
        match &self.details {
            Some(details) => format!("{}\n\n{}", self.message, details),
            None => self.message.clone(),
        }
    }
}

impl From<ServerError> for HttpError {
    fn from(error: ServerError) -> Self {
        match error {
            ServerError::NotFound { path, method } => {
                HttpError::not_found(format!("Resource not found: {} {}", method, path))
            }
            ServerError::InvalidRequest { reason, .. } => {
                HttpError::bad_request(format!("Invalid request: {}", reason))
            }
            ServerError::MissingHeader { header_name } => {
                HttpError::bad_request(format!("Missing required header: {}", header_name))
                    .with_details("Length Required")
            }
            ServerError::InvalidHeader { header_name, .. } => {
                HttpError::bad_request(format!("Invalid header: {}", header_name))
            }
            _ => HttpError::internal_error("Internal server error"),
        }
    }
}

/// Extension trait for converting Results to HTTP responses
pub trait IntoHttpResult<T> {
    fn into_http_result(self) -> Result<T, HttpError>;
}

impl<T, E> IntoHttpResult<T> for Result<T, E>
where
    E: Into<ServerError>,
{
    fn into_http_result(self) -> Result<T, HttpError> {
        self.map_err(|e| HttpError::from(e.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = ServerError::PathMismatch {
            pattern: "/users/:id".to_string(),
            path: "/users/123/posts".to_string(),
        };
        
        let display = format!("{}", error);
        assert!(display.contains("/users/123/posts"));
        assert!(display.contains("/users/:id"));
    }

    #[test]
    fn test_http_error_conversion() {
        let server_error = ServerError::NotFound {
            path: "/api/users".to_string(),
            method: "GET".to_string(),
        };
        
        let http_error = HttpError::from(server_error);
        assert_eq!(http_error.status_code, 404);
        assert!(http_error.message.contains("GET /api/users"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = io::Error::new(io::ErrorKind::ConnectionRefused, "Connection refused");
        let server_error = ServerError::from(io_error);
        
        match server_error {
            ServerError::Io(msg) => assert!(msg.contains("Connection refused")),
            _ => panic!("Expected Io variant"),
        }
    }
} 