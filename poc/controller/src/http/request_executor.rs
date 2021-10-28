use crate::abi::commands::AbiCommand;
use crate::abi::dispatcher::{AsyncResult, AsyncType};
use http::HeaderMap;
use log::debug;
use std::convert::TryFrom;
use tokio::sync::mpsc::{Sender, UnboundedReceiver};

use crate::abi::rust_v1alpha1::HttpResponse;

use tower_service::Service;
use http::{Request, Response};
use hyper::Body;

pub async fn start_request_executor<S>(
    mut rx: UnboundedReceiver<AbiCommand<Request<Vec<u8>>>>,
    otx: Sender<AsyncResult>,
    ocluster_url: http::Uri,
    ohttp_client: S,
) -> anyhow::Result<()>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static + Clone,
    S::Future: Send + 'static,
    S::Error: std::fmt::Debug,
{
    while let Some(mut http_command) = rx.recv().await {
        let cluster_url = ocluster_url.clone();
        let http_client = ohttp_client;
        let tx = otx.clone();
        tokio::spawn(async move {
            // Patch the request URI
            *http_command.value.uri_mut() = http::Uri::try_from(super::generate_url(
                &cluster_url,
                http_command.value.uri().path_and_query().unwrap(),
            ))
            .expect("Cannot build the final uri");

            debug!(
                "Received request command from '{}' with id {}: {} {:?}",
                &http_command.controller_name,
                &http_command.async_request_id,
                http_command.value.method().as_str(),
                http_command.value.uri()
            );

            // Execute the request
            let response = http_client
                .clone()
                .call(http_command.value.map(hyper::Body::from))
                .await
                .expect("Successful response");

            // Serialize the response
            let status_code = response.status();
            let mut headers = HeaderMap::with_capacity(response.headers().len());
            for (k, v) in response.headers().iter() {
                headers.append(k, v.clone());
            }

            let body = hyper::body::to_bytes(response.into_body())
                .await
                .expect("error while receiving");

            tx.clone()
                .send(AsyncResult {
                    controller_name: http_command.controller_name.clone(),
                    async_request_id: http_command.async_request_id,
                    async_type: AsyncType::Future,
                    value: Some(
                        bincode::serialize(&HttpResponse {
                            status_code,
                            headers,
                            body: body.to_vec(),
                        })
                        .expect("Error while serializing"),
                    ),
                })
                .await
                .expect("Send error");
        });
    }

    Ok(())
}
