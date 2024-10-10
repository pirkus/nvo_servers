use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

pub struct TypeMap {
    map: HashMap<TypeId, Arc<dyn Any>>,
}

impl TypeMap {
    pub fn new() -> TypeMap {
        TypeMap { map: HashMap::new() }
    }

    pub fn insert<T: Any + Sync + Send>(&mut self, any: T) {
        self.map.insert(any.type_id(), Arc::new(any));
    }

    pub fn get<T: Any + Sync + Send>(&mut self, type_id: &TypeId) -> Option<&T> {
        //        let get = self.map.get(type_id).unwrap().clone().downcast_ref().cloned();
        self.map.get(type_id).unwrap().downcast_ref::<T>()
    }
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::TypeMap;

    #[test]
    fn can_store_and_load() {
        let mut type_map = TypeMap::new();
        type_map.insert::<String>("a string".to_string());

        assert_eq!(*(type_map.get::<String>(&TypeId::of::<String>()).unwrap()), "a string".to_string());
    }
}
