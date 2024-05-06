use futures::future::{BoxFuture, Future, FutureExt};
use futures::task::{waker_ref, ArcWake};
use log::{debug, trace};
use std::marker::Send;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::task::Context;
use std::time::Duration;

pub struct Executor {
    ready_tasks_rx: Receiver<Arc<Task>>,
}

pub struct Spawner {
    ready_tasks_tx: SyncSender<Arc<Task>>,
}

struct Task {
    future: Mutex<BoxFuture<'static, ()>>,
    ready_tasks_tx: SyncSender<Arc<Task>>,
}

impl Executor {
    pub fn run_forever(self) {
        const IDLE_TIMEOUT: Duration = Duration::from_secs(5);
        loop {
            let task = self.ready_tasks_rx.recv_timeout(IDLE_TIMEOUT);
            match task {
                Err(RecvTimeoutError::Timeout) => {
                    trace!("executor is idle for {:?}", IDLE_TIMEOUT)
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Ok(task) => {
                    trace!("received task {:?}", Arc::as_ptr(&task));
                    let waker = waker_ref(&task);
                    let mut cx = Context::from_waker(&waker);
                    let _ = task.future.lock().unwrap().as_mut().poll(&mut cx);
                }
            }
        }
        debug!("executor terminated: tasks channel was disconnected, which I am not sure will ever happen")
    }
}

impl Spawner {
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let task = Arc::new(Task {
            future: Mutex::new(future.boxed()),
            ready_tasks_tx: self.ready_tasks_tx.clone(),
        });
        self.ready_tasks_tx
            .send(task)
            .expect("should be able to send a task to executor or block");
    }
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self
            .ready_tasks_tx
            .send(arc_self.clone())
            .expect("should be able to send a task to executor or block");
    }
}

pub fn new_executor_and_spawner() -> (Executor, Spawner) {
    const MAX_RUNNING_TASKS: usize = 10;
    let (tx, rx) = sync_channel(MAX_RUNNING_TASKS);
    let executor = Executor { ready_tasks_rx: rx };
    let spawner = Spawner { ready_tasks_tx: tx };
    (executor, spawner)
}
