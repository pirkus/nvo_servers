use super::response::Response;
use super::http_status::HttpStatus;
use super::headers::Headers;

/// Immutable HTTP response builder using functional patterns
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    status_code: u16,
    headers: Headers,
    body: Option<String>,
}

impl ResponseBuilder {
    /// Create a new response builder with a status code
    pub fn new(status_code: u16) -> Self {
        ResponseBuilder {
            status_code,
            headers: Headers::new(),
            body: None,
        }
    }

    /// Create a successful (200 OK) response builder
    pub fn ok() -> Self {
        Self::new(200)
    }

    /// Create a not found (404) response builder
    pub fn not_found() -> Self {
        Self::new(404)
    }

    /// Create an internal server error (500) response builder
    pub fn internal_error() -> Self {
        Self::new(500)
    }

    /// Add a header (returns a new builder)
    pub fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        let mut headers = self.headers;
        headers.insert(key, value);
        ResponseBuilder { headers, ..self }
    }

    /// Set the response body (returns a new builder)
    pub fn body(self, body: impl Into<String>) -> Self {
        ResponseBuilder {
            body: Some(body.into()),
            ..self
        }
    }

    /// Set JSON body with appropriate content-type header
    pub fn json<T: serde::Serialize>(self, data: &T) -> Self {
        match serde_json::to_string(data) {
            Ok(json) => self
                .header("Content-Type", "application/json")
                .body(json),
            Err(e) => self
                .status(500)
                .body(format!("JSON serialization error: {}", e)),
        }
    }

    /// Change the status code (returns a new builder)
    pub fn status(self, status_code: u16) -> Self {
        ResponseBuilder { status_code, ..self }
    }

    /// Build the final Response
    pub fn build(self) -> Response {
        let body = self.body.unwrap_or_default();
        let response = Response::create(self.status_code, body);
        
        // Add any custom headers
        // Note: This would require modifying Response to support headers
        response
    }

    /// Build a formatted HTTP response string
    pub fn build_http_string(self) -> String {
        let body = self.body.unwrap_or_default();
        let status_msg = HttpStatus::get_status_msg(self.status_code);
        let mut headers = self.headers;
        
        // Set Content-Length if not already set
        if !headers.contains_key("content-length") {
            headers.insert("Content-Length", body.len().to_string());
        }
        
        // Build response
        let status_line = format!("HTTP/1.1 {} {}", self.status_code, status_msg);
        let headers_str: Vec<String> = headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        
        format!(
            "{}\r\n{}\r\n\r\n{}",
            status_line,
            headers_str.join("\r\n"),
            body
        )
    }

    /// Build a chunked HTTP response string
    pub fn build_chunked_http_string(self, chunks: Vec<String>) -> String {
        let status_msg = HttpStatus::get_status_msg(self.status_code);
        let mut headers = self.headers;
        
        // Set Transfer-Encoding header
        headers.insert("Transfer-Encoding", "chunked");
        
        // Build response
        let status_line = format!("HTTP/1.1 {} {}", self.status_code, status_msg);
        let headers_str: Vec<String> = headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        
        let mut response = format!(
            "{}\r\n{}\r\n\r\n",
            status_line,
            headers_str.join("\r\n")
        );
        
        // Add chunks
        for chunk in chunks {
            let chunk_bytes = chunk.as_bytes();
            response.push_str(&format!("{:X}\r\n", chunk_bytes.len()));
            response.push_str(&chunk);
            response.push_str("\r\n");
        }
        
        // Add final chunk
        response.push_str("0\r\n\r\n");
        
        response
    }
    
    /// Create a chunked response for streaming data
    pub fn chunked(self) -> ChunkedResponseBuilder {
        ChunkedResponseBuilder {
            builder: self,
            chunks: Vec::new(),
        }
    }
}

/// Builder for chunked responses
#[derive(Debug, Clone)]
pub struct ChunkedResponseBuilder {
    builder: ResponseBuilder,
    chunks: Vec<String>,
}

impl ChunkedResponseBuilder {
    /// Add a chunk to the response
    pub fn chunk(mut self, data: impl Into<String>) -> Self {
        self.chunks.push(data.into());
        self
    }
    
    /// Build the chunked HTTP response string
    pub fn build_http_string(self) -> String {
        self.builder.build_chunked_http_string(self.chunks)
    }
}

/// Extension trait for functional response creation
pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        ResponseBuilder::ok().body(self).build()
    }
}

impl IntoResponse for &str {
    fn into_response(self) -> Response {
        self.to_string().into_response()
    }
}

impl IntoResponse for Result<Response, String> {
    fn into_response(self) -> Response {
        match self {
            Ok(response) => response,
            Err(e) => ResponseBuilder::internal_error()
                .body(format!("Internal Server Error: {}", e))
                .build(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_builder_chaining() {
        let response = ResponseBuilder::ok()
            .header("X-Custom", "value")
            .header("Content-Type", "text/plain")
            .body("Hello, World!")
            .build();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.response_body, "Hello, World!");
    }

    #[test]
    fn test_response_builder_http_string() {
        let http_string = ResponseBuilder::ok()
            .header("Content-Type", "text/plain")
            .body("Hello")
            .build_http_string();

        assert!(http_string.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(http_string.contains("Content-Type: text/plain"));
        assert!(http_string.contains("Content-Length: 5"));
        assert!(http_string.ends_with("\r\n\r\nHello"));
    }

    #[test]
    fn test_into_response_trait() {
        let response = "Hello".into_response();
        assert_eq!(response.status_code, 200);
        assert_eq!(response.response_body, "Hello");

        let result: Result<Response, String> = Err("Something went wrong".to_string());
        let response = result.into_response();
        assert_eq!(response.status_code, 500);
        assert!(response.response_body.contains("Something went wrong"));
    }
    
    #[test]
    fn test_chunked_response_builder() {
        let chunked_response = ResponseBuilder::ok()
            .header("Content-Type", "text/plain")
            .chunked()
            .chunk("Hello")
            .chunk(" ")
            .chunk("World!")
            .build_http_string();

        assert!(chunked_response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(chunked_response.contains("Transfer-Encoding: chunked"));
        assert!(chunked_response.contains("5\r\nHello\r\n")); // 5 = "Hello".len() in hex
        assert!(chunked_response.contains("1\r\n \r\n")); // 1 = " ".len() in hex
        assert!(chunked_response.contains("6\r\nWorld!\r\n")); // 6 = "World!".len() in hex
        assert!(chunked_response.ends_with("0\r\n\r\n")); // Final chunk marker
    }
} 