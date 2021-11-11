use core::fmt::Debug;
use std::time::Duration;

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum AsyncType {
    Future, // will cause client to stop listening
    Stream, // client will continue listening
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct AsyncResult {
    pub async_request_id: u64,
    pub async_type: AsyncType,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum AsyncRequestValue {
    Http(http::Request<Vec<u8>>),
    HttpStream(http::Request<Vec<u8>>),
    Delay(Duration),
}

#[derive(Debug)]
pub struct AsyncRequest {
    pub async_request_id: u64,
    pub value: AsyncRequestValue,
}
