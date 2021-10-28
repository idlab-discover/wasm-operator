use serde::{Deserialize, Serialize};
use super::{start_future, start_stream};
use futures::Stream;

#[link(wasm_import_module = "http-proxy-abi")]
extern "C" {
    fn request(ptr: *const u8, len: usize, stream: u32) -> u64;
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

pub async fn execute_request(req: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    let inner_request: HttpRequest = req.into();
    let bytes = bincode::serialize(&inner_request).unwrap();

    let async_request_id: u64 =
        unsafe { request(bytes.as_ptr(), bytes.len(), 0) }.into();

    let response_raw = start_future(async_request_id).await.unwrap();

    let response: HttpResponse = bincode::deserialize(&response_raw).unwrap();

    response.into()
}

#[derive(Serialize, Deserialize)]
struct HttpResponseStream {
    #[serde(with = "http_serde::status_code")]
    status_code: http::StatusCode,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,
}

impl HttpResponseStream {
    fn into(self, body_stream: impl Stream<Item=Vec<u8>>) -> http::Response<impl Stream<Item=Vec<u8>>> {
        let mut builder = http::response::Builder::new().status(self.status_code);

        for (h, v) in self.headers.iter() {
            builder = builder.header(h, v);
        }

        builder.body(body_stream).unwrap()
    }
}

pub async fn execute_request_stream(req: http::Request<Vec<u8>>) -> http::Response<impl Stream<Item=Vec<u8>>> {
    let inner_request: HttpRequest = req.into();
    let bytes = bincode::serialize(&inner_request).unwrap();

    let async_request_id: u64 =
        unsafe { request(bytes.as_ptr(), bytes.len(), 1) }.into();

    let response_raw = start_future(async_request_id).await.unwrap();

    let response_metadata: HttpResponseStream = bincode::deserialize(&response_raw).unwrap();

    let response_bodystream = start_stream(async_request_id);

    response_metadata.into(response_bodystream)
}
