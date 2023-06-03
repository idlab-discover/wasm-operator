use anyhow::Error;
use futures::stream::Stream;
use http::header::HeaderMap;
use http::{self, Request, Response};
use http_body::Body as HttpBody;
use hyper::Body;
use hyper_timeout::TimeoutConnector;
use kube::client::ConfigExt;
use kube::Config;
use pin_project::pin_project;
use std::time::Duration;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower::{buffer::Buffer, util::BoxService, BoxError};
use tower::{Layer, ServiceBuilder, ServiceExt};
use tower_http::{
    classify::ServerErrorsFailureClass, map_response_body::MapResponseBodyLayer, trace::TraceLayer,
};
use tracing::Span;

// Wrap `http_body::Body` to implement `Stream`.
#[pin_project]
pub struct IntoStream<B> {
    #[pin]
    body: B,
}

impl<B> IntoStream<B> {
    pub(crate) fn new(body: B) -> Self {
        Self { body }
    }
}

impl<B> Stream for IntoStream<B>
where
    B: HttpBody,
{
    type Item = Result<B::Data, B::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().body.poll_data(cx)
    }
}

pub trait BodyStreamExt: HttpBody {
    fn into_stream(self) -> IntoStream<Self>
    where
        Self: Sized,
    {
        IntoStream::new(self)
    }
}

impl<T> BodyStreamExt for T where T: HttpBody {}

pub type KubeClientService =
    Buffer<BoxService<Request<Body>, Response<Body>, BoxError>, Request<Body>>;

pub(crate) async fn create_client_service(kubeconfig: Config) -> Result<KubeClientService, Error> {
    //let timeout = Some(Duration::new(9999999999999, 0));

    let client: hyper::Client<_, Body> = {
        let connector = kubeconfig.rustls_https_connector()?;

        let mut connector = TimeoutConnector::new(connector);
        // error  handling is not really  well implemented, if a connection times out we crash...
        //connector.set_connect_timeout(timeout);
        //connector.set_read_timeout(timeout);
        //connector.set_write_timeout(timeout);

        hyper::Client::builder().build(connector)
    };

    let service = ServiceBuilder::new()
        .layer(kubeconfig.base_uri_layer())
        .layer(tower_http::decompression::DecompressionLayer::new())
        .option_layer(kubeconfig.auth_layer()?)
        .layer(kubeconfig.extra_headers_layer()?)
        .layer(
            // Attribute names follow [Semantic Conventions].
            // [Semantic Conventions]: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/trace/semantic_conventions/http.md
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request<hyper::Body>| {
                    tracing::debug_span!(
                        "HTTP",
                         http.method = %req.method(),
                         http.url = %req.uri(),
                         http.status_code = tracing::field::Empty,
                         otel.name = req.extensions().get::<&'static str>().unwrap_or(&"HTTP"),
                         otel.kind = "client",
                         otel.status_code = tracing::field::Empty,
                    )
                })
                .on_request(|_req: &Request<hyper::Body>, _span: &Span| {
                    tracing::debug!("requesting");
                })
                .on_response(
                    |res: &Response<hyper::Body>, _latency: Duration, span: &Span| {
                        let status = res.status();
                        span.record("http.status_code", &status.as_u16());
                        if status.is_client_error() || status.is_server_error() {
                            span.record("otel.status_code", &"ERROR");
                        }
                    },
                )
                // Explicitly disable `on_body_chunk`. The default does nothing.
                .on_body_chunk(())
                .on_eos(|_: Option<&HeaderMap>, _duration: Duration, _span: &Span| {
                    tracing::debug!("stream closed");
                })
                .on_failure(
                    |ec: ServerErrorsFailureClass, _latency: Duration, span: &Span| {
                        // Called when
                        // - Calling the inner service errored
                        // - Polling `Body` errored
                        // - the response was classified as failure (5xx)
                        // - End of stream was classified as failure
                        span.record("otel.status_code", &"ERROR");
                        match ec {
                            ServerErrorsFailureClass::StatusCode(status) => {
                                span.record("http.status_code", &status.as_u16());
                                tracing::error!("failed with status {}", status)
                            }
                            ServerErrorsFailureClass::Error(err) => {
                                tracing::error!("failed with error {}", err)
                            }
                        }
                    },
                ),
        )
        .service(client);

    pub fn wrapper<B>(b: B) -> Body
    where
        B: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
    {
        Body::wrap_stream(b.into_stream())
    }

    let service = MapResponseBodyLayer::new(wrapper)
        .layer(service)
        .map_err(|e| e);

    Ok(Buffer::new(BoxService::new(service), 1024))
}
