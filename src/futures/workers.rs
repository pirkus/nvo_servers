use std::future::Future;
use std::sync::mpsc::{channel, SendError, Sender};
use std::sync::{Arc, Mutex};

use crate::futures::result_handle::ResultHandle;
use log::debug;

use crate::futures::worker::{ChannelMsg, Worker};

use super::worker::Task;

pub struct Workers {
    workers: Vec<Worker>,
    sender: Sender<Arc<ChannelMsg>>,
}

type ShareableResultHandle<T> = Arc<ResultHandle<T>>;

impl Workers {
    pub fn new(size: usize) -> Workers {
        let (sender, receiver) = channel::<Arc<ChannelMsg>>();
        let receiver = Arc::new(Mutex::new(receiver));
        let _workers = (0..size).map(|x| Worker::new(x.to_string(), receiver.clone())).collect();

        debug!("Starting {size} workers (threads).");
        Workers { workers: _workers, sender }
    }

    pub fn queue(&self, future: impl Future<Output = ()> + 'static + Send) -> Result<(), SendError<Arc<ChannelMsg>>> {
        let task: Task = Task {
            future: Mutex::new(Some(Box::pin(future))),
            sender: self.sender.clone(),
        };
        self.sender.send(Arc::new(ChannelMsg::Task(task)))
    }

    pub fn queue_blocking<F>(&self, f: F) -> Result<(), SendError<Arc<ChannelMsg>>>
    where
        F: FnOnce() + Send + 'static,
    {
        self.queue(async { f() })
    }

    pub fn queue_with_result<F>(&self, future: F) -> Result<ShareableResultHandle<F::Output>, SendError<Arc<ChannelMsg>>>
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        let blocking_val: ShareableResultHandle<F::Output> = Arc::new(ResultHandle::new());
        let blocking_val_clone: ShareableResultHandle<F::Output> = blocking_val.clone();
        let inner_future = async move {
            let outer_future_res = future.await;
            blocking_val.set(outer_future_res);
        };
        let task: Task = Task {
            future: Mutex::new(Some(Box::pin(inner_future))),
            sender: self.sender.clone(),
        };

        match self.sender.send(Arc::new(ChannelMsg::Task(task))) {
            Ok(_) => Ok(blocking_val_clone),
            Err(z) => Err(z),
        }
    }

    pub fn poison_all(self) {
        self.workers.into_iter().for_each(|w| w.gracefully_shutdown(self.sender.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::hint::spin_loop;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::sleep;
    use std::time::Duration;

    use crate::utils;

    use super::*;

    #[test]
    fn workers_can_process_work() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        let workers = Workers::new(1);
        workers
            .queue(async {
                IS_MODIFIED.swap(true, Ordering::SeqCst);
            })
            .unwrap();

        while !IS_MODIFIED.load(Ordering::SeqCst) {
            sleep(Duration::from_millis(1));
        }

        workers.poison_all();
    }

    #[test]
    fn queue_with_result_does_not_block_and_return_a_result() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        static ORDER: Mutex<Vec<i8>> = Mutex::new(Vec::new());
        let workers = Workers::new(1);
        let a = utils::poor_mans_random();
        let b = utils::poor_mans_random();
        let f = async move {
            while !IS_MODIFIED.load(Ordering::SeqCst) {
                spin_loop()
            }
            ORDER.lock().unwrap().push(2);
            a / b
        };
        let res = workers.queue_with_result(f);

        ORDER.lock().unwrap().push(1);
        IS_MODIFIED.swap(true, Ordering::SeqCst); // comment out to 💀-🔐
        assert_eq!(res.unwrap().get(), a / b);
        assert_eq!(ORDER.lock().unwrap().clone(), [1, 2].to_vec());

        workers.poison_all()
    }

    #[test]
    fn queue_blocking_works() {
        static IS_MODIFIED_BLOCKING: AtomicBool = AtomicBool::new(false);
        let workers = Workers::new(1);
        
        workers
            .queue_blocking(|| {
                IS_MODIFIED_BLOCKING.store(true, Ordering::SeqCst);
            })
            .unwrap();

        while !IS_MODIFIED_BLOCKING.load(Ordering::SeqCst) {
            sleep(Duration::from_millis(1));
        }

        assert!(IS_MODIFIED_BLOCKING.load(Ordering::SeqCst));
        workers.poison_all();
    }



    #[test]
    fn poison_all_stops_workers() {
        static SHOULD_STOP_POISON: AtomicBool = AtomicBool::new(false);
        static TASK_COMPLETED_POISON: AtomicBool = AtomicBool::new(false);
        
        // Reset static variables
        SHOULD_STOP_POISON.store(false, Ordering::SeqCst);
        TASK_COMPLETED_POISON.store(false, Ordering::SeqCst);
        
        let workers = Workers::new(2);
        
        // Queue a task that will run until SHOULD_STOP_POISON is true
        workers
            .queue(async {
                while !SHOULD_STOP_POISON.load(Ordering::SeqCst) {
                    sleep(Duration::from_millis(1));
                }
                TASK_COMPLETED_POISON.store(true, Ordering::SeqCst);
            })
            .unwrap();
        
        // Give the task time to start
        sleep(Duration::from_millis(10));
        
        // Signal the task to stop and poison workers
        SHOULD_STOP_POISON.store(true, Ordering::SeqCst);
        workers.poison_all();
        
        // Wait a bit to ensure task completes
        sleep(Duration::from_millis(50));
        
        assert!(TASK_COMPLETED_POISON.load(Ordering::SeqCst));
    }

    #[test]
    fn queue_with_result_returns_correct_value() {
        let workers = Workers::new(2);
        
        // Test with a single result at a time
        let result1 = workers.queue_with_result(async { 42 }).unwrap();
        assert_eq!(result1.get(), 42);
        
        let result2 = workers.queue_with_result(async { "hello".to_string() }).unwrap();
        assert_eq!(result2.get(), "hello".to_string());
        
        workers.poison_all();
    }
}
