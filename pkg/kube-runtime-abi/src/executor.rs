use futures::executor::{LocalPool, LocalSpawner};
use futures::task::noop_waker;
use futures::Stream;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::mem;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Once};
use std::task::{Context, Poll, Waker};

use crate::SpawnerError;

static mut SPAWNER: Option<LocalSpawner> = None;

pub fn get_mut_executor() -> Rc<RefCell<LocalPool>> {
    // Initialize it to a null value
    static mut SINGLETON: *const Rc<RefCell<LocalPool>> = 0 as *const Rc<RefCell<LocalPool>>;
    static ONCE: Once = Once::new();

    unsafe {
        ONCE.call_once(|| {
            // Make it
            let singleton = Rc::new(RefCell::new(LocalPool::new()));

            // Put it in the heap so it can outlive this call
            SINGLETON = mem::transmute::<
                Box<Rc<RefCell<LocalPool>>>,
                *const Rc<RefCell<LocalPool>>,
            >(Box::new(singleton));
        });

        let pool = (*SINGLETON).clone();
        SPAWNER = Some(pool.borrow_mut().spawner());

        pool
    }
}

pub fn get_spawner() -> Result<LocalSpawner, SpawnerError> {
    if let Some(spawner) = unsafe { SPAWNER.clone() } {
        Ok(spawner)
    } else {
        Err(SpawnerError::SpawnerNotInitialized)
    }
}

fn get_pending_async() -> Rc<RefCell<HashMap<u64, Arc<Mutex<AsyncState>>>>> {
    // Initialize it to a null value
    static mut SINGLETON: *const Rc<RefCell<HashMap<u64, Arc<Mutex<AsyncState>>>>> =
        0 as *const Rc<RefCell<HashMap<u64, Arc<Mutex<AsyncState>>>>>;
    static ONCE: Once = Once::new();

    unsafe {
        ONCE.call_once(|| {
            // Make it
            let singleton: Rc<RefCell<HashMap<u64, Arc<Mutex<AsyncState>>>>> =
                Rc::new(RefCell::new(HashMap::new()));

            // Put it in the heap so it can outlive this call
            SINGLETON = mem::transmute::<
                Box<Rc<RefCell<HashMap<u64, Arc<Mutex<AsyncState>>>>>>,
                *const Rc<RefCell<HashMap<u64, Arc<Mutex<AsyncState>>>>>,
            >(Box::new(singleton));
        });

        (*SINGLETON).clone()
    }
}

pub fn start_async(future_id: u64) -> AbiAsync {
    let state = Arc::new(Mutex::new(AsyncState {
        has_value: false,
        value: None,
        waker: noop_waker(),
    }));
    get_pending_async()
        .deref()
        .borrow_mut()
        .insert(future_id, state.clone());

    AbiAsync {
        shared_state: state,
    }
}

#[no_mangle]
pub extern "C" fn wakeup(stream_id: u64, finished: u32, ptr: *const u32, len: u32) {
    {
        let state_arc = get_pending_async()
            .deref()
            .borrow_mut()
            .get(&stream_id)
            .unwrap()
            .clone();

        let mut state = state_arc.lock().unwrap();

        if !ptr.is_null() {
            state.value =
                Some(unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) });
        }

        state.has_value = true;
        state.waker.wake_by_ref();

        if finished == 1 {
            get_pending_async().deref().borrow_mut().remove(&stream_id);
        }
    }

    // Let's try to execute stuff up to the point where there isn't anything else to execute
    get_mut_executor().deref().borrow_mut().run_until_stalled();
}

/// Shared state between the future and the waiting thread
struct AsyncState {
    has_value: bool, // since value's value can be None, we add this boolean
    value: Option<Vec<u8>>,
    waker: Waker,
}

impl Future for AsyncState {
    type Output = Option<Vec<u8>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.has_value {
            self.has_value = false;
            Poll::Ready(self.value.take())
        } else {
            self.waker = cx.waker().clone();
            Poll::Pending
        }
    }
}

impl Stream for AsyncState {
    type Item = Vec<u8>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.has_value {
            self.has_value = false;
            Poll::Ready(self.value.take())
        } else {
            self.waker = cx.waker().clone();
            Poll::Pending
        }
    }
}

#[derive(Clone)]
pub struct AbiAsync {
    shared_state: Arc<Mutex<AsyncState>>,
}

impl Future for AbiAsync {
    type Output = Option<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let state = &mut *self.shared_state.lock().unwrap();
        unsafe { Pin::new_unchecked(state) }.poll(cx)
    }
}

impl Stream for AbiAsync {
    type Item = Vec<u8>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let state = &mut *self.shared_state.lock().unwrap();
        unsafe { Pin::new_unchecked(state) }.poll_next(cx)
    }
}
