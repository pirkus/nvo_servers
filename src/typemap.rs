use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

#[derive(Clone)]
pub struct DepsMap {
    map: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl DepsMap {
    pub fn new() -> DepsMap {
        DepsMap { map: HashMap::new() }
    }

    pub fn insert<T: Any + Sync + Send>(&mut self, any: T) {
        self.map.insert(any.type_id(), Arc::new(any));
    }

    pub fn insert_boxed(&mut self, any: Box<dyn Any + Sync + Send>) {
        self.map.insert((*any).type_id(), Arc::from(any));
    }

    pub fn get<T: Any + Sync + Send>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.downcast_ref::<T>())
    }
}

impl Default for DepsMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::DepsMap;
    use std::any::Any;

    #[test]
    fn can_store_and_load() {
        let mut type_map = DepsMap::new();
        type_map.insert("a string".to_string());

        assert_eq!(*type_map.get::<String>().unwrap(), "a string".to_string());
    }

    #[test]
    fn get_non_existent_returns_none() {
        let type_map = DepsMap::new();
        assert!(type_map.get::<i32>().is_none());
    }

    #[test]
    fn can_store_and_load_multiple_types() {
        let mut type_map = DepsMap::new();
        type_map.insert("a string".to_string());
        type_map.insert(42i32);

        assert_eq!(*type_map.get::<String>().unwrap(), "a string".to_string());
        assert_eq!(*type_map.get::<i32>().unwrap(), 42);
    }

    #[test]
    fn can_overwrite_value() {
        let mut type_map = DepsMap::new();
        type_map.insert("a string".to_string());
        assert_eq!(*type_map.get::<String>().unwrap(), "a string".to_string());

        type_map.insert("another string".to_string());
        assert_eq!(*type_map.get::<String>().unwrap(), "another string".to_string());
    }

    #[test]
    fn can_store_and_load_boxed() {
        let mut type_map = DepsMap::new();
        let my_string: Box<dyn Any + Send + Sync> = Box::new("a boxed string".to_string());
        type_map.insert_boxed(my_string);

        assert_eq!(*type_map.get::<String>().unwrap(), "a boxed string".to_string());
    }
}
