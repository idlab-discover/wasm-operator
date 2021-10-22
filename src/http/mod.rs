pub mod request_executor;
pub mod request_stream_executor;
pub use request_executor::start_request_executor;
pub use request_stream_executor::start_request_stream_executor;

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
