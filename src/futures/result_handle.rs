use std::sync::{Condvar, Mutex};

use log::debug;

#[derive(Default)]
pub struct ResultHandle<T> {
    value: Mutex<Option<T>>,
    is_set: Condvar,
}

impl<T> ResultHandle<T> {
    pub fn new() -> Self {
        Self {
            value: <_>::default(),
            is_set: <_>::default(),
        }
    }

    pub fn set(&self, val: T) {
        let mut data_lock = self.value.lock().expect("poisoned lock");
        while data_lock.is_some() {
            debug!("Waiting for value to be consumed.");
            data_lock = self.is_set.wait(data_lock).expect("sync broken");
        }
        *data_lock = Some(val);
        debug!("Value set");
        self.is_set.notify_one();
    }

    pub fn get(&self) -> T {
        let mut data_lock = self.value.lock().expect("poisoned lock");
        while data_lock.is_none() {
            debug!("Waiting for value to be set.");
            data_lock = self.is_set.wait(data_lock).expect("sync broken");
        }
        let value = data_lock.take().expect("cannot get value");
        debug!("Value retrieved.");
        self.is_set.notify_one();
        value
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use crate::utils;

    use super::ResultHandle;

    #[test]
    fn can_set_and_get() {
        let under_test: Arc<ResultHandle<u32>> = Arc::new(ResultHandle::new());
        let clone_under_test = under_test.clone();
        let number = utils::poor_mans_random();
        let t = thread::spawn(move || {
            assert_eq!(under_test.get(), number);
        });

        clone_under_test.set(number);
        t.join().unwrap();
    }

    #[test]
    fn can_get_and_set() {
        let under_test: Arc<ResultHandle<u32>> = Arc::new(ResultHandle::new());
        let clone_under_test = under_test.clone();
        let number = utils::poor_mans_random();
        let t = thread::spawn(move || under_test.clone().set(number));

        assert_eq!(clone_under_test.get(), number);
        t.join().unwrap();
    }
}
