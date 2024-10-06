use log::{debug, error, info};
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Wake, Waker};
use std::thread;
use std::thread::JoinHandle;

pub type Work = Box<dyn Future<Output = ()> + Send + 'static>;

pub struct Task {
    pub future: Mutex<Option<Pin<Work>>>,
    pub sender: Sender<Arc<ChannelMsg>>,
}

pub enum ChannelMsg {
    Task(Task),
    Shutdown,
}

pub struct Worker {
    name: String,
    thread_handle: JoinHandle<()>,
}

impl Worker {
    pub(crate) fn new(name: String, recv: Arc<Mutex<Receiver<Arc<ChannelMsg>>>>) -> Worker {
        let worker_name = name.clone();
        let thread_handle = thread::spawn(move || loop {
            match recv.lock().unwrap().recv() {
                Ok(task_ptr) => {
                    debug!("Executing job. Worker name: {worker_name}");
                    match task_ptr.deref() {
                        ChannelMsg::Task(task) => {
                            let mut future_mutex = task.future.lock().unwrap();
                            if let Some(mut future) = future_mutex.take() {
                                let waker = Waker::from(task_ptr.clone());
                                let context = &mut Context::from_waker(&waker);
                                if future.as_mut().poll(context).is_pending() {
                                    *future_mutex = Some(future)
                                }
                            }
                        }

                        ChannelMsg::Shutdown => break,
                    }
                }
                Err(e) => {
                    error!("Shutting down. Worker name: {worker_name}, reason {e}");
                    break;
                }
            }
        });

        Worker { name, thread_handle }
    }

    pub fn gracefully_shutdown(self, sender: Sender<Arc<ChannelMsg>>) {
        info!("Gracefully shutting down worker {}", self.name);
        sender.send(Arc::new(ChannelMsg::Shutdown)).unwrap();
        self.thread_handle.join().unwrap();
    }
}

impl Wake for ChannelMsg {
    fn wake(self: Arc<Self>) {
        let self_clone = self.clone();
        match self.deref() {
            ChannelMsg::Task(task) => task.sender.send(self_clone).expect("Something went wrong while trying to re-queue a task"),

            ChannelMsg::Shutdown => (),
        }
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
        let (sender, recv) = channel::<Arc<ChannelMsg>>();
        let worker = Worker::new("a-worker".to_string(), Arc::new(Mutex::new(recv)));
        let boxed_future = Box::pin(async {
            IS_MODIFIED.swap(true, Relaxed);
        });
        let task = ChannelMsg::Task(Task {
            future: Mutex::new(Some(boxed_future)),
            sender: sender.clone(),
        });
        sender.send(Arc::new(task)).unwrap();

        while !IS_MODIFIED.load(Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }

        worker.gracefully_shutdown(sender)
    }
}
