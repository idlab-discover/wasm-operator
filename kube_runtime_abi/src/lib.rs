#![allow(unsafe_code)]

mod requestor;
mod memory;
mod executor;
mod delay;

pub use requestor::execute_request;
pub use requestor::execute_request_stream;
pub use executor::get_mut_executor;
pub use executor::get_spawner;
pub use executor::start_stream;
pub use executor::start_future;
pub use delay::register_delay;
