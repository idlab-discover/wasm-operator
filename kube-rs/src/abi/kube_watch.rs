use serde::{Deserialize, Serialize};
use crate::Resource;
use std::ffi::c_void;
use futures::Stream;
use crate::api::params::WatchParams;

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

pub fn register_watch(resource: Resource, watch_params: WatchParams) -> impl Stream<Item=Vec<u8>> {
    let watch_request = WatchRequest{resource, watch_params };
    let serialized_watch_request = bincode::serialize(&watch_request).unwrap();

    let watch_id = unsafe {
        watch(serialized_watch_request.as_ptr(), serialized_watch_request.len(), super::memory::allocate)
    };

    super::start_stream(watch_id)
}