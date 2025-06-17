use std::collections::HashMap;
use std::fmt;

/// HTTP Headers with case-insensitive keys as per RFC 7230
#[derive(Clone, Debug, Default)]
pub struct Headers {
    // Store headers with lowercase keys for case-insensitive lookup
    // Value is a tuple of (original_key, value) to preserve case
    inner: HashMap<String, (String, String)>,
}

impl Headers {
    /// Create a new empty Headers collection
    pub fn new() -> Self {
        Headers {
            inner: HashMap::new(),
        }
    }

    /// Insert a header with case-insensitive key
    pub fn insert(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) {
        self.inner.insert(
            key.as_ref().to_lowercase(),
            (key.as_ref().to_string(), value.as_ref().to_string()),
        );
    }

    /// Get a header value with case-insensitive key lookup
    pub fn get(&self, key: impl AsRef<str>) -> Option<&str> {
        self.inner
            .get(&key.as_ref().to_lowercase())
            .map(|(_, v)| v.as_str())
    }

    /// Check if a header exists (case-insensitive)
    pub fn contains_key(&self, key: impl AsRef<str>) -> bool {
        self.inner.contains_key(&key.as_ref().to_lowercase())
    }

    /// Remove a header (case-insensitive)
    pub fn remove(&mut self, key: impl AsRef<str>) -> Option<String> {
        self.inner
            .remove(&key.as_ref().to_lowercase())
            .map(|(_, v)| v)
    }

    /// Get the number of headers
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if headers collection is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over headers with preserved case
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner
            .values()
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Parse Content-Length header
    pub fn content_length(&self) -> Option<usize> {
        self.get("content-length")
            .and_then(|v| v.parse::<usize>().ok())
    }

    /// Parse headers from HTTP request lines
    pub fn from_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Self {
        let mut headers = Headers::new();
        
        lines
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                
                // Split on first colon
                line.find(':').map(|pos| {
                    let (key, value) = line.split_at(pos);
                    let value = &value[1..]; // Skip the colon
                    (key.trim(), value.trim())
                })
            })
            .for_each(|(key, value)| {
                headers.insert(key, value);
            });
        
        headers
    }
}

impl fmt::Display for Headers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (original_key, value) in self.inner.values() {
            writeln!(f, "{}: {}", original_key, value)?;
        }
        Ok(())
    }
}

impl From<HashMap<String, String>> for Headers {
    fn from(map: HashMap<String, String>) -> Self {
        let mut headers = Headers::new();
        for (key, value) in map {
            headers.insert(key, value);
        }
        headers
    }
}

impl From<Headers> for HashMap<String, String> {
    fn from(headers: Headers) -> Self {
        headers
            .inner
            .into_iter()
            .map(|(_, (k, v))| (k, v))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_insensitive_get() {
        let mut headers = Headers::new();
        headers.insert("Content-Type", "application/json");
        
        assert_eq!(headers.get("content-type"), Some("application/json"));
        assert_eq!(headers.get("Content-Type"), Some("application/json"));
        assert_eq!(headers.get("CONTENT-TYPE"), Some("application/json"));
        assert_eq!(headers.get("CoNtEnT-tYpE"), Some("application/json"));
    }

    #[test]
    fn test_case_insensitive_insert_overwrites() {
        let mut headers = Headers::new();
        headers.insert("content-type", "text/plain");
        headers.insert("Content-Type", "application/json");
        
        assert_eq!(headers.get("content-type"), Some("application/json"));
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn test_contains_key() {
        let mut headers = Headers::new();
        headers.insert("X-Custom-Header", "value");
        
        assert!(headers.contains_key("x-custom-header"));
        assert!(headers.contains_key("X-CUSTOM-HEADER"));
        assert!(!headers.contains_key("X-Other-Header"));
    }

    #[test]
    fn test_remove() {
        let mut headers = Headers::new();
        headers.insert("Authorization", "Bearer token");
        
        assert_eq!(headers.remove("AUTHORIZATION"), Some("Bearer token".to_string()));
        assert!(!headers.contains_key("authorization"));
    }

    #[test]
    fn test_content_length() {
        let mut headers = Headers::new();
        headers.insert("Content-Length", "1234");
        
        assert_eq!(headers.content_length(), Some(1234));
        
        headers.insert("content-length", "invalid");
        assert_eq!(headers.content_length(), None);
    }

    #[test]
    fn test_from_lines() {
        let lines = vec![
            "Host: example.com",
            "Content-Type: text/html",
            "Content-Length: 42",
            "",  // Empty line should be ignored
            "X-Custom: value",
        ];
        
        let headers = Headers::from_lines(lines.into_iter());
        
        assert_eq!(headers.get("host"), Some("example.com"));
        assert_eq!(headers.get("content-type"), Some("text/html"));
        assert_eq!(headers.content_length(), Some(42));
        assert_eq!(headers.get("x-custom"), Some("value"));
    }

    #[test]
    fn test_from_lines_with_whitespace() {
        let lines = vec![
            "  Host:   example.com  ",
            "Content-Type:application/json",
        ];
        
        let headers = Headers::from_lines(lines.into_iter());
        
        assert_eq!(headers.get("host"), Some("example.com"));
        assert_eq!(headers.get("content-type"), Some("application/json"));
    }
} 