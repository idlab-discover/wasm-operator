use crate::abi::commands::AbiCommand;
use crate::abi::dispatcher::{AsyncResult, AsyncType};
use futures::{self, StreamExt, TryStreamExt};
use http::HeaderMap;
use hyper::client::connect::HttpConnector;
use hyper_tls::HttpsConnector;
use log::{debug, warn};
use std::convert::TryFrom;
use tokio::sync::mpsc::{Sender, UnboundedReceiver};
use tokio_util::{
    codec::{FramedRead, LinesCodec, LinesCodecError},
    io::StreamReader,
};

use crate::abi::rust_v1alpha1::HttpResponseStream;

pub async fn start_request_stream_executor(
    mut rx: UnboundedReceiver<AbiCommand<http::Request<Vec<u8>>>>,
    otx: Sender<AsyncResult>,
    ocluster_url: http::Uri,
    ohttp_client: hyper::Client<HttpsConnector<HttpConnector>, hyper::Body>,
) -> anyhow::Result<()> {
    while let Some(mut http_command) = rx.recv().await {
        let cluster_url = ocluster_url.clone();
        let http_client = ohttp_client.clone();
        let tx = otx.clone();
        tokio::spawn(async move {
            // Patch the request URI
            *http_command.value.uri_mut() = http::Uri::try_from(super::generate_url(
                &cluster_url,
                http_command.value.uri().path_and_query().unwrap(),
            ))
            .expect("Cannot build the final uri");

            debug!(
                "Received stream request command from '{}' with id {}: {} {:?}",
                &http_command.controller_name,
                &http_command.async_request_id,
                http_command.value.method().as_str(),
                http_command.value.uri()
            );

            // Execute the request
            let response = http_client
                .clone()
                .request(http_command.value.map(hyper::Body::from))
                .await
                .expect("Successful response");

            // Serialize the response
            let status_code = response.status();
            let mut headers = HeaderMap::with_capacity(response.headers().len());
            for (k, v) in response.headers().iter() {
                headers.append(k, v.clone());
            }

            tx.clone()
                .send(AsyncResult {
                    controller_name: http_command.controller_name.clone(),
                    async_request_id: http_command.async_request_id,
                    async_type: AsyncType::Future,
                    value: Some(
                        bincode::serialize(&HttpResponseStream {
                            status_code,
                            headers,
                        })
                        .expect("Error while serializing"),
                    ),
                })
                .await
                .expect("Send error");

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
                                controller_name: http_command.controller_name.clone(),
                                async_request_id: http_command.async_request_id,
                                async_type: AsyncType::Stream,
                                value: Some(line.as_bytes().to_vec()),
                            })
                            .await
                            .expect("Send error");
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
                    controller_name: http_command.controller_name.clone(),
                    async_request_id: http_command.async_request_id,
                    async_type: AsyncType::Stream,
                    value: None,
                })
                .await
                .expect("Send error");
        });
    }

    Ok(())
}
