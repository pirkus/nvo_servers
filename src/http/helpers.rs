use std::collections::HashMap;

/// Error type for path-related operations
#[derive(Debug, Clone)]
pub enum PathError {
    #[allow(dead_code)]
    LengthMismatch { pattern_len: usize, path_len: usize },
}

/// Extract path parameters from a URL pattern and path
/// Returns an error if the pattern and path don't match in structure
#[allow(dead_code)]
pub fn extract_path_params(pattern: &str, path: &str) -> Result<HashMap<String, String>, PathError> {
    let split_pattern: Vec<&str> = pattern.split('/').collect();
    let split_path: Vec<&str> = path.split('/').collect();

    if split_pattern.len() != split_path.len() {
        return Err(PathError::LengthMismatch {
            pattern_len: split_pattern.len(),
            path_len: split_path.len(),
        });
    }

    Ok(split_pattern
        .iter()
        .zip(split_path.iter())
        .filter_map(|(pattern_part, path_part)| {
            pattern_part
                .strip_prefix(':')
                .map(|param_name| (param_name.to_string(), path_part.to_string()))
        })
        .collect())
}

/// Check if a path matches a pattern (with support for :param syntax)
#[allow(dead_code)]
pub fn path_matches_pattern(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    if pattern_parts.len() != path_parts.len() {
        return false;
    }

    pattern_parts
        .iter()
        .zip(path_parts.iter())
        .all(|(pattern_part, path_part)| {
            pattern_part.starts_with(':') || pattern_part == path_part
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_params_success() {
        let result = extract_path_params("/users/:id", "/users/123").unwrap();
        assert_eq!(result.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_extract_path_params_multiple() {
        let result = extract_path_params("/users/:id/posts/:post_id", "/users/123/posts/456").unwrap();
        assert_eq!(result.get("id"), Some(&"123".to_string()));
        assert_eq!(result.get("post_id"), Some(&"456".to_string()));
    }

    #[test]
    fn test_extract_path_params_error() {
        let result = extract_path_params("/users/:id", "/users/123/extra");
        assert!(matches!(result, Err(PathError::LengthMismatch { .. })));
    }

    #[test]
    fn test_path_matches_pattern() {
        assert!(path_matches_pattern("/users/:id", "/users/123"));
        assert!(!path_matches_pattern("/users/:id", "/users/123/extra"));
        assert!(path_matches_pattern("/users/:id/posts/:post_id", "/users/123/posts/456"));
    }

    #[test]
    fn test_edge_cases() {
        // Empty paths
        assert!(path_matches_pattern("", ""));
        assert!(!path_matches_pattern("/", ""));
        
        // No parameters
        assert!(path_matches_pattern("/users/list", "/users/list"));
        assert!(!path_matches_pattern("/users/list", "/users/detail"));
    }
}