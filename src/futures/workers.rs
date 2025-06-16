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
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = channel::<Arc<ChannelMsg>>();
        let receiver = Arc::new(Mutex::new(receiver));
        
        let workers = (0..size)
            .map(|id| Worker::new(id.to_string(), Arc::clone(&receiver)))
            .collect();

        debug!("Starting {size} workers (threads).");
        Self { workers, sender }
    }

    pub fn queue(&self, future: impl Future<Output = ()> + 'static + Send) -> Result<(), SendError<Arc<ChannelMsg>>> {
        self.send_task(Task::new(future, self.sender.clone()))
    }

    pub fn queue_blocking<F>(&self, f: F) -> Result<(), SendError<Arc<ChannelMsg>>>
    where
        F: FnOnce() + Send + 'static,
    {
        self.queue(async move { f() })
    }

    pub fn queue_with_result<F>(&self, future: F) -> Result<ShareableResultHandle<F::Output>, SendError<Arc<ChannelMsg>>>
    where
        F: Future + Send + 'static,
        F::Output: Send,
    {
        let result_handle = Arc::new(ResultHandle::new());
        let result_clone = Arc::clone(&result_handle);
        
        let wrapped_future = async move {
            result_handle.set(future.await);
        };
        
        self.send_task(Task::new(wrapped_future, self.sender.clone()))
            .map(|_| result_clone)
    }

    pub fn poison_all(self) {
        self.workers
            .into_iter()
            .for_each(|worker| worker.gracefully_shutdown(self.sender.clone()));
    }
    
    fn send_task(&self, task: Task) -> Result<(), SendError<Arc<ChannelMsg>>> {
        self.sender.send(Arc::new(ChannelMsg::Task(task)))
    }
}

#[cfg(test)]
mod tests {
    use std::hint::spin_loop;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use crate::utils;

    use super::*;

    // Helper function to wait for a condition with timeout
    fn wait_for<F>(condition: F, timeout: Duration) -> bool
    where
        F: Fn() -> bool,
    {
        let start = Instant::now();
        while !condition() && start.elapsed() < timeout {
            sleep(Duration::from_millis(1));
        }
        condition()
    }

    #[test]
    fn workers_can_process_work() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        let workers = Workers::new(1);
        workers
            .queue(async {
                IS_MODIFIED.store(true, Ordering::SeqCst);
            })
            .expect("Failed to queue task");

        assert!(wait_for(
            || IS_MODIFIED.load(Ordering::SeqCst),
            Duration::from_secs(2)
        ));

        workers.poison_all();
    }

    #[test]
    fn queue_with_result_does_not_block_and_return_a_result() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        static ORDER: Mutex<Vec<i8>> = Mutex::new(Vec::new());
        
        let workers = Workers::new(1);
        let (a, b) = (utils::poor_mans_random(), utils::poor_mans_random());
        
        let future = async move {
            while !IS_MODIFIED.load(Ordering::SeqCst) {
                spin_loop()
            }
            ORDER.lock().unwrap().push(2);
            a / b
        };
        
        let result_handle = workers
            .queue_with_result(future)
            .expect("Failed to queue task with result");

        ORDER.lock().unwrap().push(1);
        IS_MODIFIED.store(true, Ordering::SeqCst);
        
        assert_eq!(result_handle.get(), a / b);
        assert_eq!(*ORDER.lock().unwrap(), vec![1, 2]);

        workers.poison_all();
    }

    #[test]
    fn queue_blocking_works() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        
        let workers = Workers::new(1);
        
        workers
            .queue_blocking(|| {
                IS_MODIFIED.store(true, Ordering::SeqCst);
            })
            .expect("Failed to queue blocking task");

        assert!(wait_for(
            || IS_MODIFIED.load(Ordering::SeqCst),
            Duration::from_secs(2)
        ));

        workers.poison_all();
    }

    #[test]
    fn poison_all_stops_workers() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        
        // Reset the counter
        COUNTER.store(0, Ordering::SeqCst);
        
        let workers = Workers::new(2);
        
        // Queue tasks using iterator
        (0..5).for_each(|_| {
            workers
                .queue(async {
                    COUNTER.fetch_add(1, Ordering::SeqCst);
                })
                .expect("Failed to queue task");
        });
        
        // Wait for tasks to complete
        wait_for(
            || COUNTER.load(Ordering::SeqCst) >= 5,
            Duration::from_secs(2)
        );
        
        workers.poison_all();
        
        assert!(COUNTER.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn queue_with_result_returns_correct_value() {
        static FUTURE_POLLED: AtomicBool = AtomicBool::new(false);
        
        let workers = Workers::new(2);
        
        // Test with integer result
        let handle = workers
            .queue_with_result(async {
                FUTURE_POLLED.store(true, Ordering::SeqCst);
                42
            })
            .expect("Failed to queue task with result");
        
        // Wait for the future to be polled
        assert!(wait_for(
            || FUTURE_POLLED.load(Ordering::SeqCst),
            Duration::from_secs(2)
        ));
        
        assert_eq!(handle.get(), 42);
        
        // Test with string result
        let handle2 = workers
            .queue_with_result(async {
                "hello".to_string()
            })
            .expect("Failed to queue task with result");
        
        sleep(Duration::from_millis(10));
        assert_eq!(handle2.get(), "hello".to_string());
        
        workers.poison_all();
    }
}
