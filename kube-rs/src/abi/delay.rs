use std::future::Future;
use std::time::Duration;
use futures::FutureExt;

#[link(wasm_import_module = "delay-abi")]
extern "C" {
    // Returns the future identifier
    fn delay(millis: u64) -> u64;
}

pub fn register_delay(del: Duration) -> impl Future<Output=()> {
    let millis = del.as_millis() as u64;
    super::start_future(
        unsafe { delay(millis) }
    ).map(|v| ())
}