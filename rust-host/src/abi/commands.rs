use core::marker::Sized;
use core::fmt::Debug;
use core::clone::Clone;

#[derive(Debug, Clone)]
pub struct AbiCommand<T: Sized + Debug> {
    pub async_request_id: u64,
    pub controller_name: String,
    pub value: T,
}
