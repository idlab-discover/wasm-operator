use std::collections::HashMap;
use std::sync::{Mutex, Arc, Once};
use std::pin::Pin;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use futures::executor::LocalPool;
use std::cell::{RefCell};
use std::ops::Deref;
use std::mem;
use std::rc::Rc;
use futures::Stream;

pub fn get_mut_executor() -> Rc<RefCell<LocalPool>> {
    // Initialize it to a null value
    static mut SINGLETON: *const Rc<RefCell<LocalPool>> = 0 as *const Rc<RefCell<LocalPool>>;
    static ONCE: Once = Once::new();

    unsafe {
        ONCE.call_once(|| {
            // Make it
            let singleton = Rc::new(RefCell::new(LocalPool::new()));

            // Put it in the heap so it can outlive this call
            SINGLETON = mem::transmute(Box::new(singleton));
        });

        (*SINGLETON).clone()
    }
}

fn get_pending_futures() -> Rc<RefCell<HashMap<u64, Arc<Mutex<AbiFutureState>>>>> {
    // Initialize it to a null value
    static mut SINGLETON: *const Rc<RefCell<HashMap<u64, Arc<Mutex<AbiFutureState>>>>> = 0 as *const Rc<RefCell<HashMap<u64, Arc<Mutex<AbiFutureState>>>>>;
    static ONCE: Once = Once::new();

    unsafe {
        ONCE.call_once(|| {
            // Make it
            let singleton: Rc<RefCell<HashMap<u64, Arc<Mutex<AbiFutureState>>>>> = Rc::new(RefCell::new(HashMap::new()));

            // Put it in the heap so it can outlive this call
            SINGLETON = mem::transmute(Box::new(singleton));
        });

        (*SINGLETON).clone()
    }
}

pub fn start_future(future_id: u64) -> AbiFuture {
    let state = Arc::new(Mutex::new(
        AbiFutureState {
            value: None,
            completed: false,
            waker: None
        }
    ));
    get_pending_futures().deref().borrow_mut().insert(future_id, state.clone());

    AbiFuture {
        shared_state: state
    }
}

pub fn start_stream(stream_id: u64) -> AbiStream {
    let state = Arc::new(Mutex::new(
        AbiFutureState {
            value: None,
            completed: false,
            waker: None
        }
    ));
    get_pending_futures().deref().borrow_mut().insert(stream_id, state.clone());

    AbiStream {
        shared_state: state
    }
}

#[no_mangle]
pub extern "C" fn wakeup_future(future_id: u64, ptr: *const u8, len: usize) {
    let fut_state = get_pending_futures();
    let waker = {
        let state_arc = fut_state.deref().borrow_mut().remove(&future_id).unwrap();
        let mut state = state_arc.lock().unwrap();

        if !ptr.is_null() {
            state.value = Some(unsafe {
                Vec::from_raw_parts(
                    ptr as *mut u8,
                    len as usize,
                    len as usize,
                )
            });
        }

        state.completed = true;
        state.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake()
    }

    // Let's try to execute stuff up to the point where there isn't anything else to execute
    get_mut_executor().deref().borrow_mut().run_until_stalled();
}

#[no_mangle]
pub extern "C" fn wakeup_stream(stream_id: u64, ptr: *const u8, len: usize) {
    let fut_state = get_pending_futures();
    let waker = {
        let mut states = fut_state.deref().borrow_mut();
        let state_arc = states.remove(&stream_id).unwrap();
        let mut state = state_arc.lock().unwrap();

        let has_value = !ptr.is_null();
        if has_value {
            state.value = Some(unsafe {
                Vec::from_raw_parts(
                    ptr as *mut u8,
                    len as usize,
                    len as usize,
                )
            });
        }
        state.completed = true;
        let waker = state.waker.take();
        if has_value {
            states.insert(stream_id, state_arc.clone());
        }
        waker
    };
    if let Some(waker) = waker {
        waker.wake();
    }

    // Let's try to execute stuff up to the point where there isn't anything else to execute
    get_mut_executor().deref().borrow_mut().run_until_stalled();
}

pub struct AbiFuture {
    shared_state: Arc<Mutex<AbiFutureState>>,
}

/// Shared state between the future and the waiting thread
struct AbiFutureState {
    value: Option<Vec<u8>>,
    completed: bool,

    /// The waker for the task that `TimerFuture` is running on.
    /// The thread can use this after setting `completed = true` to tell
    /// `TimerFuture`'s task to wake up, see that `completed = true`, and
    /// move forward.
    waker: Option<Waker>,
}

impl Future for AbiFuture {
    type Output = Option<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Look at the shared state to see if the timer has already completed.
        let mut shared_state = self.shared_state.lock().unwrap();
        if shared_state.completed {
            Poll::Ready(shared_state.value.take())
        } else {
            // Set waker so that the thread can wake up the current task
            // when the timer has completed, ensuring that the future is polled
            // again and sees that `completed = true`.
            //
            // It's tempting to do this once rather than repeatedly cloning
            // the waker each time. However, the `TimerFuture` can move between
            // tasks on the executor, which could cause a stale waker pointing
            // to the wrong task, preventing `TimerFuture` from waking up
            // correctly.
            //
            // N.B. it's possible to check for this using the `Waker::will_wake`
            // function, but we omit that here to keep things simple.
            shared_state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub struct AbiStream {
    shared_state: Arc<Mutex<AbiFutureState>>,
}

impl Stream for AbiStream {
    type Item = Vec<u8>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Look at the shared state to see if the timer has already completed.
        let mut shared_state = self.shared_state.lock().unwrap();
        if shared_state.completed {
            shared_state.completed = false;
            Poll::Ready(shared_state.value.take())
        } else {
            // Set waker so that the thread can wake up the current task
            // when the timer has completed, ensuring that the future is polled
            // again and sees that `completed = true`.
            //
            // It's tempting to do this once rather than repeatedly cloning
            // the waker each time. However, the `TimerFuture` can move between
            // tasks on the executor, which could cause a stale waker pointing
            // to the wrong task, preventing `TimerFuture` from waking up
            // correctly.
            //
            // N.B. it's possible to check for this using the `Waker::will_wake`
            // function, but we omit that here to keep things simple.
            shared_state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
