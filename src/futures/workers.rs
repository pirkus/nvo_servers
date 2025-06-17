use std::future::Future;
use std::sync::mpsc::{channel, SendError, Sender};
use std::sync::{Arc, Mutex};

use crate::futures::result_handle::ResultHandle;
use log::debug;

use crate::futures::worker::{ChannelMsg, Worker};

use super::worker::Task;

pub struct Workers {
    workers: Vec<Worker>,
    senders: Vec<Sender<Arc<ChannelMsg>>>,
    next_worker: Arc<Mutex<usize>>,
}

type ShareableResultHandle<T> = Arc<ResultHandle<T>>;

impl Workers {
    pub fn new(size: usize) -> Self {
        let (workers, senders): (Vec<_>, Vec<_>) = (0..size)
            .map(|id| {
                let (sender, receiver) = channel::<Arc<ChannelMsg>>();
                let worker = Worker::new(id.to_string(), receiver);
                (worker, sender)
            })
            .unzip();

        debug!("Starting {size} workers (threads).");
        Self { 
            workers, 
            senders,
            next_worker: Arc::new(Mutex::new(0)),
        }
    }

    pub fn queue(&self, future: impl Future<Output = ()> + 'static + Send) -> Result<(), SendError<Arc<ChannelMsg>>> {
        let sender = self.get_next_sender();
        self.send_task(Task::new(future, sender.clone()), &sender)
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
        let sender = self.get_next_sender();
        
        let wrapped_future = async move {
            result_handle.set(future.await);
        };
        
        self.send_task(Task::new(wrapped_future, sender.clone()), &sender)
            .map(|_| result_clone)
    }

    pub fn poison_all(self) {
        // Send shutdown message to all workers
        self.senders
            .iter()
            .for_each(|sender| {
                let _ = sender.send(Arc::new(ChannelMsg::Shutdown));
            });
        
        // Wait for all workers to finish
        self.workers
            .into_iter()
            .for_each(|worker| worker.join());
    }
    
    fn get_next_sender(&self) -> &Sender<Arc<ChannelMsg>> {
        let mut next = self.next_worker.lock().expect("Worker selection mutex poisoned");
        let index = *next;
        *next = (*next + 1) % self.senders.len();
        &self.senders[index]
    }
    
    fn send_task(&self, task: Task, sender: &Sender<Arc<ChannelMsg>>) -> Result<(), SendError<Arc<ChannelMsg>>> {
        sender.send(Arc::new(ChannelMsg::Task(task)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
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
        std::iter::repeat_with(|| {
            if condition() {
                Some(true)
            } else if start.elapsed() >= timeout {
                Some(false)
            } else {
                sleep(Duration::from_millis(1));
                None
            }
        })
        .find_map(|result| result)
        .unwrap_or(false)
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
            // Use a more cooperative waiting approach
            std::iter::repeat_with(|| {
                if IS_MODIFIED.load(Ordering::SeqCst) {
                    true
                } else {
                    std::thread::yield_now();
                    false
                }
            })
            .find(|&ready| ready)
            .unwrap();
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
        
        let workers = Workers::new(1);
        
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
        let workers = Workers::new(1);
        
        // Test with integer result
        let handle = workers
            .queue_with_result(async {
                42
            })
            .expect("Failed to queue task with result");
        
        // Wait for the result to be ready
        assert!(wait_for(
            || handle.is_ready(),
            Duration::from_secs(2)
        ));
        
        assert_eq!(handle.get(), 42);
        
        // Test with string result
        let handle2 = workers
            .queue_with_result(async {
                "hello".to_string()
            })
            .expect("Failed to queue task with result");
        
        assert!(wait_for(
            || handle2.is_ready(),
            Duration::from_secs(2)
        ));
        
        assert_eq!(handle2.get(), "hello".to_string());
        
        // Test try_get
        let handle3 = workers
            .queue_with_result(async {
                100
            })
            .expect("Failed to queue task with result");
        
        // Try to get immediately (might not be ready)
        let mut result = handle3.try_get();
        if result.is_none() {
            // Wait and try again
            assert!(wait_for(
                || handle3.is_ready(),
                Duration::from_secs(2)
            ));
            result = handle3.try_get();
        }
        
        assert_eq!(result, Some(100));
        
        workers.poison_all();
    }

    #[test]
    fn multiple_workers_can_process_tasks() {
        use std::sync::atomic::AtomicI32;
        
        static ACTIVE_COUNT: AtomicI32 = AtomicI32::new(0);
        static MAX_ACTIVE: AtomicI32 = AtomicI32::new(0);
        static COMPLETED_COUNT: AtomicI32 = AtomicI32::new(0);
        static UNIQUE_THREADS: Mutex<Vec<String>> = Mutex::new(Vec::new());
        
        // Reset state
        ACTIVE_COUNT.store(0, Ordering::SeqCst);
        MAX_ACTIVE.store(0, Ordering::SeqCst);
        COMPLETED_COUNT.store(0, Ordering::SeqCst);
        UNIQUE_THREADS.lock().unwrap().clear();
        
        let workers = Workers::new(3); // Use 3 workers
        
        // Queue blocking tasks to demonstrate true concurrency
        (0..6).for_each(|_| {
            workers.queue_blocking(|| {
                // Record thread name
                let thread_name = std::thread::current()
                    .name()
                    .unwrap_or("unnamed")
                    .to_string();
                UNIQUE_THREADS.lock().unwrap().push(thread_name);
                
                // Increment active count
                let active = ACTIVE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                
                // Update max active if needed
                std::iter::repeat_with(|| {
                    let max = MAX_ACTIVE.load(Ordering::SeqCst);
                    if active <= max {
                        Some(())
                    } else {
                        match MAX_ACTIVE.compare_exchange_weak(
                            max,
                            active,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => Some(()),
                            Err(_) => None,
                        }
                    }
                })
                .find_map(|result| result)
                .unwrap();
                
                // Simulate work
                sleep(Duration::from_millis(100));
                
                // Decrement active count
                ACTIVE_COUNT.fetch_sub(1, Ordering::SeqCst);
                COMPLETED_COUNT.fetch_add(1, Ordering::SeqCst);
            })
            .expect("Failed to queue task");
        });
        
        // Give tasks time to start running concurrently
        sleep(Duration::from_millis(50));
        
        // Wait for all tasks to complete
        assert!(wait_for(
            || COMPLETED_COUNT.load(Ordering::SeqCst) == 6,
            Duration::from_secs(2)
        ));
        
        // Verify we had multiple tasks running concurrently
        let max_active = MAX_ACTIVE.load(Ordering::SeqCst);
        assert!(
            max_active >= 2,
            "Expected at least 2 concurrent tasks, but max active was {}",
            max_active
        );
        
        // Verify multiple worker threads were used
        let thread_names = UNIQUE_THREADS.lock().unwrap();
        let unique_threads: HashSet<_> = thread_names.iter().collect();
        assert!(
            unique_threads.len() >= 2,
            "Expected at least 2 different worker threads, found: {:?}",
            unique_threads
        );
        
        workers.poison_all();
    }
}
