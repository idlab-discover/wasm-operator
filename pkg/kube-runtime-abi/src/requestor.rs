use super::http_data::HttpRequest;
use super::http_data::HttpResponseMeta;
use super::start_async;
use futures::Stream;

#[link(wasm_import_module = "http-proxy-abi")]
extern "C" {
    fn request(ptr: *const u8, len: usize, stream: u32) -> u64;
}

pub async fn execute_request_stream(
    req: http::Request<Vec<u8>>,
) -> http::Response<impl Stream<Item = Vec<u8>>> {
    let inner_request: HttpRequest<Vec<u8>> = req.into();
    let bytes = bincode::serialize(&inner_request).unwrap();

    let async_request_id: u64 = unsafe { request(bytes.as_ptr(), bytes.len(), 1) };

    let abi_async = start_async(async_request_id);

    let response_raw = abi_async.clone().await.unwrap(); // get first value from future trait

    let response: HttpResponseMeta = bincode::deserialize(&response_raw).unwrap();

    response.into(abi_async) // get next values from stream trait
}

pub async fn execute_request(req: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    let inner_request: HttpRequest<Vec<u8>> = req.into();
    let bytes = bincode::serialize(&inner_request).unwrap();

    let async_request_id: u64 = unsafe { request(bytes.as_ptr(), bytes.len(), 0) };

    let abi_async = start_async(async_request_id);

    let response_raw = abi_async.clone().await.unwrap(); // get first value from future trait

    let response: HttpResponseMeta = bincode::deserialize(&response_raw).unwrap();

    let body = abi_async.await.unwrap();

    response.into(body) // get next values from stream trait
}
