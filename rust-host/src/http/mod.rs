use tokio::sync::mpsc::{Sender, UnboundedReceiver};
use crate::abi::commands::AbiCommand;
use crate::abi::dispatcher::{AsyncType, AsyncResult};
use std::convert::{TryFrom, TryInto};
use http::HeaderMap;

use crate::abi::rust_v1alpha1::HttpResponse;

pub async fn start_request_executor(
    mut rx: UnboundedReceiver<AbiCommand<http::Request<Vec<u8>>>>,
    mut tx: Sender<AsyncResult>,
    cluster_url: url::Url,
    http_client: reqwest::Client,
) -> anyhow::Result<()> {
    while let Some(mut http_command) = rx.recv().await {
        // Patch the request URI
        *http_command.value.uri_mut() = http::Uri::try_from(
            generate_url(cluster_url.as_str(), http_command.value.uri().path_and_query().unwrap())
        ).expect("Cannot build the final uri");

        // Execute the request
        let response = http_client.execute(http_command.value.try_into().unwrap()).await?;

        // Serialize the response
        let status_code = response.status();
        let mut headers = HeaderMap::with_capacity(response.headers().len());
        for (k, v) in response.headers().iter() {
            headers.append(k, v.clone());
        }
        let response_body = response.bytes().await?;

        let inner_response = HttpResponse {
            status_code,
            headers,
            body: response_body.to_vec(),
        }; //TODO Design problem here: i'm using an abi version specific type here. Needs some engineering

        tx.send(AsyncResult {
            controller_name: http_command.controller_name,
            async_request_id: http_command.async_request_id,
            async_type: AsyncType::Future,
            value: Some(bincode::serialize(&inner_response)?)
        }).await?;

    }

    Ok(())
}

/// An internal url joiner to deal with the two different interfaces
///
/// - api module produces a http::Uri which we can turn into a PathAndQuery (has a leading slash by construction)
/// - config module produces a url::Url from user input (sometimes contains path segments)
///
/// This deals with that in a pretty easy way (tested below)
fn generate_url(cluster_url: &str, request_p_and_q: &http::uri::PathAndQuery) -> String {
    let base = cluster_url.trim_end_matches('/');
    format!("{}{}", base, request_p_and_q)
}