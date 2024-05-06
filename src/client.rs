use futures::Future;
use log::debug;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering::SeqCst},
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
    thread::{self, JoinHandle},
    time::Duration,
};

struct RequestSharedState {
    iterations: u32,
    waker: Option<Waker>,
}

#[derive(Clone)]
pub struct Request {
    id: u32,
    shared_state: Arc<Mutex<RequestSharedState>>,
}

struct RequestWaker {
    running_requests: Arc<Mutex<Vec<Request>>>,
    new_requests_rx: Receiver<Request>,
    done: Arc<AtomicBool>,
}

struct RequestWakerHandle {
    waker_thread_handle: Option<JoinHandle<()>>,
    request_receiver_thread_handle: Option<JoinHandle<()>>,
}

pub struct Client {
    new_requests_tx: SyncSender<Request>,
    _request_waker_handle: RequestWakerHandle,
}

impl RequestWaker {
    fn run_until_dropped(new_requests_rx: Receiver<Request>) -> RequestWakerHandle {
        let running_requests = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let request_poller = RequestWaker {
            running_requests: running_requests.clone(),
            new_requests_rx,
            done: done.clone(),
        };
        let waker_thread_handle = thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                let mut locked_requests = running_requests.lock().unwrap();
                locked_requests.retain_mut(|request| {
                    let mut shared_state = request.shared_state.lock().unwrap();
                    shared_state.iterations -= 1;
                    if let Some(waker) = shared_state.waker.take() {
                        debug!("waker thread calls wake for {}", request.id);
                        waker.wake();
                    };
                    shared_state.iterations > 0 // removing requests with no iterations left
                });
                if locked_requests.is_empty() && done.load(SeqCst) {
                    break;
                }
            }
            debug!("waker thread is done");
        });
        let request_receiver_thread_handle = thread::spawn(move || {
            loop {
                let Ok(request) = request_poller.new_requests_rx.recv() else {
                    request_poller.done.store(true, SeqCst);
                    break;
                };
                request_poller
                    .running_requests
                    .lock()
                    .unwrap()
                    .push(request);
            }
            debug!("request receiver thread is done");
        });
        RequestWakerHandle {
            waker_thread_handle: Some(waker_thread_handle),
            request_receiver_thread_handle: Some(request_receiver_thread_handle),
        }
    }
}

impl Drop for RequestWakerHandle {
    fn drop(&mut self) {
        if let Some(waker_thread_handle) = self.waker_thread_handle.take() {
            waker_thread_handle.join().expect("join must succeed");
        }
        if let Some(request_receiver_thread_handle) = self.request_receiver_thread_handle.take() {
            request_receiver_thread_handle
                .join()
                .expect("join must succeed");
        }
    }
}

impl Client {
    pub fn new() -> Self {
        const MAX_RUNNING_REQUESTS: usize = 10;
        let (new_requests_tx, new_requests_rx) = sync_channel(MAX_RUNNING_REQUESTS);
        let _request_waker_handle = RequestWaker::run_until_dropped(new_requests_rx);
        Self {
            new_requests_tx,
            _request_waker_handle,
        }
    }

    pub fn new_request(&self, id: u32, iterations: u32) -> Result<Request, ()> {
        if iterations < 1 {
            return Err(());
        }
        let shared_state = Arc::new(Mutex::new(RequestSharedState {
            iterations,
            waker: None,
        }));
        let request = Request { id, shared_state };
        self.new_requests_tx
            .send(request.clone())
            .expect("send must succeed");
        Ok(request)
    }
}

impl Future for Request {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.shared_state.lock().unwrap();
        if state.iterations == 0 {
            debug!("id: {}, ready", self.id);
            Poll::Ready(())
        } else {
            debug!("id: {}, iterations: {}, pending", self.id, state.iterations);
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
