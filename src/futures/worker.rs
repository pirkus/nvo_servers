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

impl Task {
    pub fn new(
        future: impl Future<Output = ()> + Send + 'static,
        sender: Sender<Arc<ChannelMsg>>
    ) -> Self {
        Self {
            future: Mutex::new(Some(Box::pin(future))),
            sender,
        }
    }
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
    pub(crate) fn new(name: String, recv: Receiver<Arc<ChannelMsg>>) -> Worker {
        let worker_name = name.clone();
        let thread_handle = thread::Builder::new()
            .name(worker_name.clone())
            .spawn(move || {
                std::iter::repeat(())
                    .map(|_| recv.recv())
                    .take_while(|result| match result {
                        Ok(task_ptr) => !matches!(task_ptr.deref(), ChannelMsg::Shutdown),
                        Err(e) => {
                            error!("Shutting down. Worker name: {worker_name}, reason {e}");
                            false
                        }
                    })
                    .for_each(|result| {
                        if let Ok(task_ptr) = result {
                            debug!("Executing job. Worker name: {worker_name}");
                            if let ChannelMsg::Task(task) = task_ptr.deref() {
                                Self::process_task(task, &task_ptr);
                            }
                        }
                    });
            })
            .expect("Failed to spawn worker thread");

        Worker { name, thread_handle }
    }
    
    fn process_task(task: &Task, task_ptr: &Arc<ChannelMsg>) {
        task.future
            .lock()
            .ok()
            .and_then(|mut future_mutex| future_mutex.take())
            .map(|mut future| {
                let waker = Waker::from(task_ptr.clone());
                let context = &mut Context::from_waker(&waker);
                if future.as_mut().poll(context).is_pending() {
                    if let Ok(mut future_mutex) = task.future.lock() {
                        *future_mutex = Some(future);
                    }
                }
            });
    }

    pub fn join(self) {
        info!("Waiting for worker {} to finish", self.name);
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
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn worker_can_process_work() {
        static IS_MODIFIED: AtomicBool = AtomicBool::new(false);
        let (sender, recv) = channel::<Arc<ChannelMsg>>();
        let worker = Worker::new("a-worker".to_string(), recv);
        
        let task = Task::new(
            async {
                IS_MODIFIED.swap(true, Relaxed);
            },
            sender.clone()
        );
        
        sender.send(Arc::new(ChannelMsg::Task(task))).unwrap();

        // Wait for task to complete
        std::iter::repeat_with(|| IS_MODIFIED.load(Relaxed))
            .find(|&ready| {
                if !ready {
                    thread::sleep(Duration::from_millis(10));
                }
                ready
            })
            .unwrap();

        // Send shutdown message and wait for worker to finish
        sender.send(Arc::new(ChannelMsg::Shutdown)).unwrap();
        worker.join()
    }
}
