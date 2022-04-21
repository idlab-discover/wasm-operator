use http::HeaderMap;
use tower::ServiceExt;
use tower_service::Service;

use super::http_data::HttpResponseMeta;
use crate::kube_client::KubeClientService;

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

pub(crate) async fn start_request_executor(
    mut request: http::Request<Vec<u8>>,
    cluster_url: http::Uri,
    mut service: KubeClientService,
) -> anyhow::Result<(HttpResponseMeta, hyper::Body)> {
    // Patch the request URI
    *request.uri_mut() = generate_url(&cluster_url, request.uri().path_and_query().unwrap());

    let response = service
        .ready()
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?
        .call(request.map(hyper::Body::from))
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    // Serialize the response
    let status_code = response.status();
    let mut headers = HeaderMap::with_capacity(response.headers().len());
    for (k, v) in response.headers().iter() {
        headers.append(k, v.clone());
    }

    Ok((
        HttpResponseMeta {
            status_code,
            headers,
        },
        response.into_body(),
    ))
}
