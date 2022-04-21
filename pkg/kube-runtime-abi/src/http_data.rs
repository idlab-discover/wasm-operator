use serde::{Deserialize, Serialize};

/// Data structure to serialize/deserialize http request
#[derive(Serialize, Deserialize)]
pub(crate) struct HttpRequest<T> {
    #[serde(with = "http_serde::method")]
    method: http::Method,

    #[serde(with = "http_serde::uri")]
    pub(crate) uri: http::Uri,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,

    body: T,
}

impl<T> From<HttpRequest<T>> for http::Request<T> {
    fn from(req: HttpRequest<T>) -> http::Request<T> {
        let mut builder = http::Request::builder().method(req.method).uri(req.uri);

        for (h, v) in req.headers.iter() {
            builder = builder.header(h, v);
        }

        builder.body(req.body).unwrap()
    }
}

impl<T> From<http::Request<T>> for HttpRequest<T> {
    fn from(req: http::Request<T>) -> Self {
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
pub(crate) struct HttpResponseMeta {
    #[serde(with = "http_serde::status_code")]
    pub(crate) status_code: http::StatusCode,

    #[serde(with = "http_serde::header_map")]
    pub(crate) headers: http::HeaderMap,
}

impl HttpResponseMeta {
    pub(crate) fn into<T>(self, body: T) -> http::Response<T> {
        let mut builder = http::response::Builder::new().status(self.status_code);

        for (h, v) in self.headers.iter() {
            builder = builder.header(h, v);
        }

        builder.body(body).unwrap()
    }
}
