use core::fmt::Debug;
use std::time::Duration;

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct AsyncResult {
    pub async_request_id: u64,
    pub value: Option<bytes::Bytes>,
    pub finished: bool,
}

#[derive(Debug)]
pub enum AsyncRequestValue {
    Http(http::Request<Vec<u8>>),
    HttpStream(http::Request<Vec<u8>>),
    Delay(Duration),
}

// #[derive(Debug)]
// pub struct AsyncRequest {
//     pub async_request_id: u64,
//     pub value: AsyncRequestValue,
// }
