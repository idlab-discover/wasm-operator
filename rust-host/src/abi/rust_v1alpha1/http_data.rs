use serde::{Deserialize, Serialize};

// Hack to serialize/deserialize http request
#[derive(Serialize, Deserialize)]
pub(crate) struct HttpRequest {
    #[serde(with = "http_serde::method")]
    method: http::Method,

    #[serde(with = "http_serde::uri")]
    pub uri: http::Uri,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,

    body: Vec<u8>,
}

impl Into<http::Request<Vec<u8>>> for HttpRequest {
    fn into(self) -> http::Request<Vec<u8>> {
        let mut builder = http::Request::builder().method(self.method).uri(self.uri);

        for (h, v) in self.headers.iter() {
            builder = builder.header(h, v);
        }

        builder.body(self.body).unwrap()
    }
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
pub(crate) struct HttpResponse {
    #[serde(with = "http_serde::status_code")]
    pub(crate) status_code: http::StatusCode,

    #[serde(with = "http_serde::header_map")]
    pub(crate) headers: http::HeaderMap,

    pub(crate) body: Vec<u8>,
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

impl From<http::Response<Vec<u8>>> for HttpResponse {
    fn from(res: http::Response<Vec<u8>>) -> Self {
        let (parts, body) = res.into_parts();

        HttpResponse {
            status_code: parts.status,
            headers: parts.headers,
            body,
        }
    }
}
