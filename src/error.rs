use std::fmt;
use std::io;
use crate::http::response::Response;

/// Comprehensive error type for the HTTP server
#[derive(Debug, Clone)]
pub enum ServerError {
    /// IO-related errors
    Io {
        context: String,
        kind: io::ErrorKind,
    },
    /// Connection-related errors
    Connection {
        fd: i32,
        context: String,
    },
    /// HTTP parsing errors
    HttpParse {
        context: String,
        status_code: u16,
    },
    /// Handler execution errors
    Handler {
        path: String,
        method: String,
        error: String,
    },
    /// Configuration errors
    Config {
        context: String,
    },
    /// Resource exhaustion
    ResourceExhausted {
        resource: String,
        limit: usize,
    },
    /// Timeout errors
    Timeout {
        operation: String,
        duration_ms: u64,
    },
}

impl ServerError {
    /// Create an IO error with context
    pub fn io(context: impl Into<String>, kind: io::ErrorKind) -> Self {
        ServerError::Io {
            context: context.into(),
            kind,
        }
    }
    
    /// Create a connection error
    pub fn connection(fd: i32, context: impl Into<String>) -> Self {
        ServerError::Connection {
            fd,
            context: context.into(),
        }
    }
    
    /// Convert to HTTP response
    pub fn to_response(&self) -> Response {
        match self {
            ServerError::HttpParse { status_code, context } => {
                Response::create(*status_code, context.clone())
            }
            ServerError::Handler { error, .. } => {
                Response::create(500, format!("Internal Server Error: {}", error))
            }
            ServerError::ResourceExhausted { resource, .. } => {
                Response::create(503, format!("Resource exhausted: {}", resource))
            }
            ServerError::Timeout { operation, .. } => {
                Response::create(504, format!("Operation timed out: {}", operation))
            }
            _ => Response::create(500, "Internal Server Error".to_string()),
        }
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::Io { context, kind } => {
                write!(f, "IO error ({}): {}", kind, context)
            }
            ServerError::Connection { fd, context } => {
                write!(f, "Connection error (fd: {}): {}", fd, context)
            }
            ServerError::HttpParse { context, status_code } => {
                write!(f, "HTTP parse error ({}): {}", status_code, context)
            }
            ServerError::Handler { path, method, error } => {
                write!(f, "Handler error for {} {}: {}", method, path, error)
            }
            ServerError::Config { context } => {
                write!(f, "Configuration error: {}", context)
            }
            ServerError::ResourceExhausted { resource, limit } => {
                write!(f, "Resource {} exhausted (limit: {})", resource, limit)
            }
            ServerError::Timeout { operation, duration_ms } => {
                write!(f, "Timeout during {} after {}ms", operation, duration_ms)
            }
        }
    }
}

impl std::error::Error for ServerError {}

impl From<io::Error> for ServerError {
    fn from(error: io::Error) -> Self {
        ServerError::io(error.to_string(), error.kind())
    }
}

/// Result type alias for server operations
pub type ServerResult<T> = Result<T, ServerError>;

/// Extension trait for functional error handling
pub trait ResultExt<T> {
    /// Add context to an error
    fn context(self, context: impl Into<String>) -> ServerResult<T>;
    
    /// Convert error to a different type with context
    fn map_err_context<F>(self, f: F) -> ServerResult<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Into<ServerError>,
{
    fn context(self, context: impl Into<String>) -> ServerResult<T> {
        self.map_err(|e| {
            let mut err = e.into();
            match &mut err {
                ServerError::Io { context: ctx, .. } => *ctx = context.into(),
                ServerError::Connection { context: ctx, .. } => *ctx = context.into(),
                ServerError::HttpParse { context: ctx, .. } => *ctx = context.into(),
                ServerError::Config { context: ctx } => *ctx = context.into(),
                _ => {}
            }
            err
        })
    }
    
    fn map_err_context<F>(self, f: F) -> ServerResult<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let mut err = e.into();
            let context = f();
            match &mut err {
                ServerError::Io { context: ctx, .. } => *ctx = context,
                ServerError::Connection { context: ctx, .. } => *ctx = context,
                ServerError::HttpParse { context: ctx, .. } => *ctx = context,
                ServerError::Config { context: ctx } => *ctx = context,
                _ => {}
            }
            err
        })
    }
}

/// Error type for HTTP-specific errors
#[derive(Debug, Clone)]
pub struct HttpError {
    pub status_code: u16,
    pub message: String,
}

impl From<ServerError> for HttpError {
    fn from(error: ServerError) -> Self {
        match error {
            ServerError::HttpParse { status_code, context } => HttpError {
                status_code,
                message: context,
            },
            _ => HttpError {
                status_code: 500,
                message: error.to_string(),
            },
        }
    }
}

impl From<HttpError> for Response {
    fn from(error: HttpError) -> Self {
        Response::create(error.status_code, error.message)
    }
}

/// Extension trait for converting Results to HTTP responses
pub trait IntoHttpResult<T> {
    fn into_http_result(self) -> Result<T, Response>;
}

impl<T, E> IntoHttpResult<T> for Result<T, E>
where
    E: Into<HttpError>,
{
    fn into_http_result(self) -> Result<T, Response> {
        self.map_err(|e| e.into().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_context() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let result: Result<(), io::Error> = Err(io_err);
        
        let with_context = result.context("Opening config file");
        assert!(with_context.is_err());
        
        let err = with_context.unwrap_err();
        assert!(matches!(err, ServerError::Io { .. }));
        let error_str = err.to_string();
        assert!(error_str.contains("Opening config file"));
        assert!(error_str.contains("entity not found") || error_str.contains("not found"));
    }
    
    #[test]
    fn test_error_to_response() {
        let err = ServerError::HttpParse {
            context: "Invalid header".to_string(),
            status_code: 400,
        };
        
        let response = err.to_response();
        assert_eq!(response.status_code, 400);
    }
    
    #[test]
    fn test_result_chains() {
        fn process_data(data: &str) -> ServerResult<String> {
            data.parse::<i32>()
                .map_err(|_| ServerError::HttpParse {
                    context: "Invalid number".to_string(),
                    status_code: 400,
                })
                .and_then(|n| {
                    if n > 0 {
                        Ok(format!("Positive: {}", n))
                    } else {
                        Err(ServerError::HttpParse {
                            context: "Number must be positive".to_string(),
                            status_code: 400,
                        })
                    }
                })
        }
        
        assert!(process_data("42").is_ok());
        assert!(process_data("-1").is_err());
        assert!(process_data("abc").is_err());
    }
} 