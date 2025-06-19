//! Functional concurrent data structures using only standard library

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::hash::Hash;

/// A functional concurrent map that provides safe concurrent access
/// without external dependencies
#[derive(Clone)]
pub struct FuncMap<K, V> {
    inner: Arc<Mutex<HashMap<K, V>>>,
}

impl<K: Hash + Eq, V> FuncMap<K, V> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Insert a value, returning the previous value if any
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        self.inner.lock().ok()?.insert(key, value)
    }
    
    /// Remove a value, returning it if present
    pub fn remove(&self, key: &K) -> Option<V> {
        self.inner.lock().ok()?.remove(key)
    }
    
    /// Apply a function to all entries that match a predicate
    /// Returns the keys that were processed
    pub fn retain_with<F>(&self, mut f: F) -> Vec<K>
    where
        F: FnMut(&K, &V) -> bool,
        K: Clone,
    {
        let mut processed = Vec::new();
        if let Ok(mut map) = self.inner.lock() {
            let keys_to_remove: Vec<K> = map.iter()
                .filter_map(|(k, v)| {
                    if !f(k, v) {
                        Some(k.clone())
                    } else {
                        None
                    }
                })
                .collect();
            
            for key in keys_to_remove {
                map.remove(&key);
                processed.push(key);
            }
        }
        processed
    }
    
    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.inner.lock().map(|m| m.len()).unwrap_or(0)
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.inner.lock().map(|m| m.is_empty()).unwrap_or(true)
    }
    
    /// Apply a function to find and remove an entry
    pub fn find_remove<F>(&self, mut predicate: F) -> Option<(K, V)>
    where
        F: FnMut(&K, &V) -> bool,
        K: Clone,
    {
        let mut map = self.inner.lock().ok()?;
        let key = map.iter()
            .find(|(k, v)| predicate(k, v))
            .map(|(k, _)| k.clone())?;
        map.remove_entry(&key)
    }
}

impl<K: Hash + Eq, V> Default for FuncMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}