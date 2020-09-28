use serde::{Deserialize, Serialize};
use crate::Resource;
use std::collections::HashMap;
use std::sync::Mutex;
use crate::api::resource::WatchParams;
use once_cell::sync::Lazy;
use std::ffi::c_void;

type WatchCallback = dyn Fn(Vec<u8>) + Send;

static REGISTERED_WATCH: Lazy<Mutex<HashMap<u64, Box<WatchCallback>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Serialize, Deserialize)]
pub(crate) struct WatchRequest {
    pub(crate) resource: Resource,
    pub(crate) watch_params: WatchParams
}

#[link(wasm_import_module = "kube-watch-abi")]
extern "C" {
    // Returns the watch identifier
    fn watch(watch_req_ptr: *const u8, watch_req_len: usize, allocator_fn: extern "C" fn(usize) -> *mut c_void) -> u64;
}

#[no_mangle]
pub extern "C" fn on_event(watch_id: u64, ev_ptr: *const u8, ev_len: usize) {
    let watches = REGISTERED_WATCH.lock().unwrap();
    let callback = watches.get(&watch_id).unwrap();

    let event_raw = unsafe {
        Vec::from_raw_parts(
            ev_ptr as *mut u8,
            ev_len as usize,
            ev_len as usize,
        )
    };

    callback(event_raw)
}

pub fn register_watch<F: 'static + Fn(Vec<u8>) + Send>(resource: Resource, watch_params: WatchParams, callback: F) {
    let watch_request = WatchRequest{resource, watch_params };
    let serialized_watch_request = bincode::serialize(&watch_request).unwrap();

    let watch_id = unsafe {
        watch(serialized_watch_request.as_ptr(), serialized_watch_request.len(), super::memory::allocate)
    };

    REGISTERED_WATCH.lock().unwrap().insert(watch_id, Box::new(callback));
}