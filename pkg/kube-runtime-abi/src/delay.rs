use futures::FutureExt;
use std::future::Future;
use std::time::Duration;

#[link(wasm_import_module = "delay-abi")]
extern "C" {
    // Returns the future identifier
    fn delay(millis: u64) -> u64;
}

pub fn register_delay(del: Duration) -> impl Future<Output = ()> + Send {
    let millis = del.as_millis() as u64;
    super::start_async(unsafe { delay(millis) }).map(|_v| ())
}
