use futures::TryStreamExt;
use futures::{self, StreamExt};
use http::HeaderMap;
use http::{Request, Response};
use hyper::Body;
use log::warn;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex as MutexAsync;
use tokio_util::{
    codec::{FramedRead, LinesCodec, LinesCodecError},
    io::StreamReader,
};
use tower_service::Service;

use super::http_data::HttpResponse;
use super::http_data::HttpResponseStream;
use crate::abi::AsyncResult;
use crate::abi::AsyncType;

/// An internal url joiner to deal with the two different interfaces
///
/// - api module produces a http::Uri which we can turn into a PathAndQuery (has a leading slash by construction)
/// - config module produces a url::Url from user input (sometimes contains path segments)
///
/// This deals with that in a pretty easy way (tested below)
pub(crate) fn generate_url(
    cluster_url: &http::Uri,
    request_p_and_q: &http::uri::PathAndQuery,
) -> http::Uri {
    let mut parts = cluster_url.clone().into_parts();
    parts.path_and_query = Some(request_p_and_q.clone());
    http::Uri::from_parts(parts).expect("invalid path and query")
}

pub async fn start_request_executor<S>(
    async_request_id: u64,
    mut request: http::Request<Vec<u8>>,
    tx: Sender<AsyncResult>,
    cluster_url: http::Uri,
    http_client: Arc<MutexAsync<S>>,
) -> anyhow::Result<()>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Debug,
{
    // Patch the request URI
    *request.uri_mut() = http::Uri::try_from(generate_url(
        &cluster_url,
        request.uri().path_and_query().unwrap(),
    ))?;

    // Execute the request
    let response = http_client
        .lock()
        .await
        .call(request.map(hyper::Body::from))
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    // Serialize the response
    let status_code = response.status();
    let mut headers = HeaderMap::with_capacity(response.headers().len());
    for (k, v) in response.headers().iter() {
        headers.append(k, v.clone());
    }

    let body = hyper::body::to_bytes(response.into_body()).await?;

    tx.clone()
        .send(AsyncResult {
            async_request_id: async_request_id,
            async_type: AsyncType::Future,
            value: Some(bincode::serialize(&HttpResponse {
                status_code,
                headers,
                body: body.to_vec(),
            })?),
        })
        .await?;

    Ok(())
}

pub async fn start_request_stream_executor<S>(
    async_request_id: u64,
    mut request: http::Request<Vec<u8>>,
    tx: Sender<AsyncResult>,
    cluster_url: http::Uri,
    http_client: Arc<MutexAsync<S>>,
) -> anyhow::Result<()>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Debug,
{
    // Patch the request URI
    *request.uri_mut() = http::Uri::try_from(generate_url(
        &cluster_url,
        request.uri().path_and_query().unwrap(),
    ))?;

    // Execute the request
    let response = http_client
        .lock()
        .await
        .call(request.map(hyper::Body::from))
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    // Serialize the response
    let status_code = response.status();
    let mut headers = HeaderMap::with_capacity(response.headers().len());
    for (k, v) in response.headers().iter() {
        headers.append(k, v.clone());
    }

    tx.clone()
        .send(AsyncResult {
            async_request_id: async_request_id,
            async_type: AsyncType::Future,
            value: Some(bincode::serialize(&HttpResponseStream {
                status_code,
                headers,
            })?),
        })
        .await?;

    let mut frames = FramedRead::new(
        StreamReader::new(response.into_body().map_err(|e| {
            // Client timeout. This will be ignored.
            if e.is_timeout() {
                return std::io::Error::new(std::io::ErrorKind::TimedOut, e);
            }
            // Unexpected EOF from chunked decoder.
            // Tends to happen when watching for 300+s. This will be ignored.
            if e.to_string().contains("unexpected EOF during chunk") {
                return std::io::Error::new(std::io::ErrorKind::UnexpectedEof, e);
            }
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })),
        LinesCodec::new(),
    );

    while let Some(res) = frames.next().await {
        match res {
            Ok(line) => {
                tx.clone()
                    .send(AsyncResult {
                        async_request_id: async_request_id,
                        async_type: AsyncType::Stream,
                        value: Some(line.as_bytes().to_vec()),
                    })
                    .await?;
            }

            Err(LinesCodecError::Io(e)) => match e.kind() {
                // Client timeout
                std::io::ErrorKind::TimedOut => {
                    warn!("timeout in poll: {}", e); // our client timeout
                }
                // Unexpected EOF from chunked decoder.
                // Tends to happen after 300+s of watching.
                std::io::ErrorKind::UnexpectedEof => {
                    warn!("eof in poll: {}", e);
                }
                _ => warn!("TODO, fail: {}", LinesCodecError::Io(e)),
            },

            // Reached the maximum line length without finding a newline.
            // This should never happen because we're using the default `usize::MAX`.
            Err(LinesCodecError::MaxLineLengthExceeded) => {
                warn!("TODO, fail: {}", LinesCodecError::MaxLineLengthExceeded)
            }
        }
    }

    tx.clone()
        .send(AsyncResult {
            async_request_id: async_request_id,
            async_type: AsyncType::Stream,
            value: None,
        })
        .await?;

    Ok(())
}
