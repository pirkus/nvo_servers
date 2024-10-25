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

    pub fn get<T: Any + Sync + Send>(&self) -> Option<&T> {
        self.map.get(&TypeId::of::<T>()).unwrap().downcast_ref::<T>()
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

    #[test]
    fn can_store_and_load() {
        let mut type_map = DepsMap::new();
        type_map.insert("a string".to_string());

        assert_eq!(*type_map.get::<String>().unwrap(), "a string".to_string());
    }
}
