use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

pub struct TypeMap {
    map: HashMap<TypeId, Arc<dyn Any>>,
}

impl TypeMap {
    fn insert<T: Any>(&mut self, any: T) {
        self.map.insert(any.type_id(), Arc::new(any));
    }

    fn get<T: Any>(&mut self, type_id: &TypeId) -> Option<Arc<T>> {
        let get = self.map.get(type_id).cloned().and_then(|x| x.downcast_ref::<Arc<T>>().cloned());
        get
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn func_can_be_called() {}
}
