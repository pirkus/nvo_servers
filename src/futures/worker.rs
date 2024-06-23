use log::{debug, error};
use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Wake, Waker};
use std::thread;
use std::thread::JoinHandle;

pub type Work = Box<dyn Future<Output = ()> + Send + 'static>;

pub struct Task {
    pub future: Mutex<Option<Pin<Work>>>,
    pub sender: Sender<Arc<Task>>,
}

pub struct Worker {
    _name: String,
    _thread_handle: JoinHandle<()>,
}

impl Worker {
    pub(crate) fn new(name: String, recv: Arc<Mutex<Receiver<Arc<Task>>>>) -> Worker {
        let worker_name = name.clone();
        let _thread_handle = thread::spawn(move || loop {
            match recv.lock().unwrap().recv() {
                Ok(task) => {
                    debug!("Executing job. Worker name: {worker_name}");
                    let mut future_mutex = task.future.lock().unwrap();
                    if let Some(mut future) = future_mutex.take() {
                        let waker = Waker::from(task.clone());
                        let context = &mut Context::from_waker(&waker);
                        if future.as_mut().poll(context).is_pending() {
                            *future_mutex = Some(future)
                        }
                    }
                }
                Err(e) => {
                    error!("Shutting down. Worker name: {worker_name}, reason {e}");
                    break;
                }
            }
        });

        Worker {
            _name: name,
            _thread_handle,
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.sender
            .send(self.clone())
            .expect("Something went wrong while trying to re-queue a task");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::Relaxed;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn worker_can_process_work() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        let (sender, recv) = channel::<Arc<Task>>();
        Worker::new("a-worker".to_string(), Arc::new(Mutex::new(recv)));
        let boxed_future = Box::pin(async {
            IS_MODIFIED.swap(true, Relaxed);
        });
        let task: Task = Task {
            future: Mutex::new(Some(boxed_future)),
            sender: sender.clone(),
        };
        sender.send(Arc::new(task)).unwrap();

        while !IS_MODIFIED.load(Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }
    }
}
