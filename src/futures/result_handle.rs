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

    pub fn try_get(&self) -> Option<T> {
        let mut data_lock = self.value.lock().expect("poisoned lock");
        let value = data_lock.take();
        if value.is_some() {
            debug!("Value retrieved (non-blocking).");
            self.is_set.notify_one();
        }
        value
    }

    pub fn is_ready(&self) -> bool {
        self.value.lock().expect("poisoned lock").is_some()
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

    #[test]
    fn try_get_returns_none_when_not_ready() {
        let under_test = ResultHandle::<u32>::new();
        assert_eq!(under_test.try_get(), None);
    }

    #[test]
    fn try_get_returns_some_when_ready() {
        let under_test = ResultHandle::new();
        let number = utils::poor_mans_random();
        under_test.set(number);
        assert_eq!(under_test.try_get(), Some(number));
    }

    #[test]
    fn is_ready_works() {
        let under_test = ResultHandle::new();
        assert!(!under_test.is_ready());
        
        under_test.set(42);
        assert!(under_test.is_ready());
        
        // After get, it should not be ready anymore
        let _ = under_test.get();
        assert!(!under_test.is_ready());
    }
}
