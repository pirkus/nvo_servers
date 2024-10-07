use std::collections::HashMap;
use log::debug;

// TODO [FL]: write tests for these methods
pub fn extract_path_params(pattern: &str, path: &str) -> HashMap<String, String> {
    let split_pattern = pattern.split('/').collect::<Vec<&str>>();
    let split_path = path.split('/').collect::<Vec<&str>>();

    if split_pattern.len() != split_path.len() {
        panic!("split_pattern.len() != split_path.len() - this should be done prior to calling this method")
    }

    (0..split_path.len())
        .filter_map(|i| {
            if split_pattern[i].starts_with(':') {
                let mut chars = split_pattern[i].chars();
                chars.next();
                Some((chars.as_str().to_string(), split_path[i].to_string()))
            } else {
                None
            }
        })
        .collect()
}

pub fn path_matches_pattern(pattern: &str, path: &str) -> bool {
    let split_pattern = pattern.split('/').collect::<Vec<&str>>();
    let split_path = path.split('/').collect::<Vec<&str>>();
    debug!("split_pattern: {split_pattern:?}, split_path: {split_path:?}");
    if split_pattern.len() != split_path.len() {
        return false;
    }

    (0..split_path.len())
        .map(|i| split_path[i] == split_pattern[i] || split_pattern[i].starts_with(':'))
        .reduce(|acc, e| acc && e)
        .unwrap()
}