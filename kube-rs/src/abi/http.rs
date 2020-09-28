use serde::{Deserialize, Serialize};
use super::memory;
use std::ffi::c_void;

#[link(wasm_import_module = "http-proxy-abi")]
extern "C" {
    fn request(ptr: *const u8, len: usize, allocator_fn: extern "C" fn(usize) -> *mut c_void) -> u64;
}

/// Data structure to serialize/deserialize http request
#[derive(Serialize, Deserialize)]
struct HttpRequest {
    #[serde(with = "http_serde::method")]
    method: http::Method,

    #[serde(with = "http_serde::uri")]
    uri: http::Uri,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,

    body: Vec<u8>,
}

impl From<http::Request<Vec<u8>>> for HttpRequest {
    fn from(req: http::Request<Vec<u8>>) -> Self {
        let (parts, body) = req.into_parts();

        HttpRequest {
            method: parts.method,
            uri: parts.uri,
            headers: parts.headers,
            body,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct HttpResponse {
    #[serde(with = "http_serde::status_code")]
    status_code: http::StatusCode,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,

    body: Vec<u8>,
}

impl Into<http::Response<Vec<u8>>> for HttpResponse {
    fn into(self) -> http::Response<Vec<u8>> {
        let mut builder = http::response::Builder::new().status(self.status_code);

        for (h, v) in self.headers.iter() {
            builder = builder.header(h, v);
        }

        builder.body(self.body).unwrap()
    }
}

pub fn execute_request(req: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    let inner_request: HttpRequest = req.into();
    let bytes = bincode::serialize(&inner_request).unwrap();

    let response_ptr: memory::Ptr =
        unsafe { request(bytes.as_ptr(), bytes.len(), memory::allocate) }.into();

    let response_raw = unsafe {
        Vec::from_raw_parts(
            response_ptr.ptr as *mut u8,
            response_ptr.size as usize,
            response_ptr.size as usize,
        )
    };
    let response_inner: HttpResponse = bincode::deserialize(&response_raw).unwrap();

    response_inner.into()
}