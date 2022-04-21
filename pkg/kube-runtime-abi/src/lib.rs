#![allow(unsafe_code)]

mod delay;
mod executor;
pub(crate) mod http_data;
mod memory;
mod requestor;

pub use delay::register_delay;
pub use executor::get_mut_executor;
pub use executor::get_spawner;
pub use executor::start_async;
pub use requestor::execute_request;
pub use requestor::execute_request_stream;
