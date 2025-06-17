use std::collections::HashMap;

/// Pre-compiled path pattern for efficient matching
#[derive(Debug, Clone)]
pub struct CompiledPath {
    segments: Vec<PathSegment>,
    param_count: usize,
}

#[derive(Debug, Clone)]
enum PathSegment {
    Literal(String),
    Parameter(String),
}

impl CompiledPath {
    /// Compile a path pattern for efficient reuse
    pub fn new(pattern: &str) -> Self {
        let segments: Vec<PathSegment> = pattern
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|segment| {
                if let Some(param_name) = segment.strip_prefix(':') {
                    PathSegment::Parameter(param_name.to_string())
                } else {
                    PathSegment::Literal(segment.to_string())
                }
            })
            .collect();

        let param_count = segments
            .iter()
            .filter(|s| matches!(s, PathSegment::Parameter(_)))
            .count();

        CompiledPath { segments, param_count }
    }

    /// Check if a path matches this pattern
    pub fn matches(&self, path: &str) -> bool {
        let path_segments: Vec<&str> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if self.segments.len() != path_segments.len() {
            return false;
        }

        self.segments
            .iter()
            .zip(path_segments.iter())
            .all(|(pattern_seg, path_seg)| match pattern_seg {
                PathSegment::Literal(literal) => literal == path_seg,
                PathSegment::Parameter(_) => true,
            })
    }

    /// Extract parameters from a matching path
    pub fn extract_params(&self, path: &str) -> Option<HashMap<String, String>> {
        let path_segments: Vec<&str> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if self.segments.len() != path_segments.len() {
            return None;
        }

        let mut params = HashMap::with_capacity(self.param_count);

        for (pattern_seg, path_seg) in self.segments.iter().zip(path_segments.iter()) {
            match pattern_seg {
                PathSegment::Parameter(name) => {
                    params.insert(name.clone(), (*path_seg).to_string());
                }
                PathSegment::Literal(literal) if literal != path_seg => return None,
                _ => {}
            }
        }

        Some(params)
    }

    /// Get a unique key for this pattern (for HashMap storage)
    pub fn pattern_key(&self) -> String {
        self.segments
            .iter()
            .map(|seg| match seg {
                PathSegment::Literal(s) => s.as_str(),
                PathSegment::Parameter(_) => ":param",
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Router that pre-compiles all patterns for efficient matching
#[derive(Debug, Clone)]
pub struct PathRouter<T> {
    routes: Vec<(CompiledPath, T)>,
}

impl<T: Clone> PathRouter<T> {
    pub fn new() -> Self {
        PathRouter { routes: Vec::new() }
    }

    pub fn add_route(&mut self, pattern: &str, handler: T) {
        let compiled = CompiledPath::new(pattern);
        self.routes.push((compiled, handler));
    }

    /// Find the first matching route and extract parameters
    pub fn find_match(&self, path: &str) -> Option<(&T, HashMap<String, String>)> {
        self.routes
            .iter()
            .find_map(|(compiled_path, handler)| {
                compiled_path.extract_params(path)
                    .map(|params| (handler, params))
            })
    }
}

impl<T: Clone> Default for PathRouter<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiled_path_matching() {
        let pattern = CompiledPath::new("/users/:id/posts/:post_id");
        
        assert!(pattern.matches("/users/123/posts/456"));
        assert!(!pattern.matches("/users/123"));
        assert!(!pattern.matches("/users/123/posts/456/comments"));
    }

    #[test]
    fn test_compiled_path_params() {
        let pattern = CompiledPath::new("/users/:id/posts/:post_id");
        
        let params = pattern.extract_params("/users/123/posts/456").unwrap();
        assert_eq!(params.get("id"), Some(&"123".to_string()));
        assert_eq!(params.get("post_id"), Some(&"456".to_string()));
    }

    #[test]
    fn test_path_router() {
        let mut router = PathRouter::new();
        router.add_route("/users/:id", "user_handler");
        router.add_route("/posts/:id", "post_handler");
        router.add_route("/users/:user_id/posts/:post_id", "user_post_handler");

        let (handler, params) = router.find_match("/users/123").unwrap();
        assert_eq!(*handler, "user_handler");
        assert_eq!(params.get("id"), Some(&"123".to_string()));

        let (handler, params) = router.find_match("/users/123/posts/456").unwrap();
        assert_eq!(*handler, "user_post_handler");
        assert_eq!(params.get("user_id"), Some(&"123".to_string()));
        assert_eq!(params.get("post_id"), Some(&"456".to_string()));
    }

    #[test]
    fn test_empty_path_segments() {
        let pattern = CompiledPath::new("/users//posts/");
        assert!(pattern.matches("/users/posts"));
        
        let pattern2 = CompiledPath::new("users/:id");
        assert!(pattern2.matches("users/123"));
    }
} 