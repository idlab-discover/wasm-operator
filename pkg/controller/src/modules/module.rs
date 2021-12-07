use super::ControllerModuleMetadata;
use crate::abi::abicommand::AsyncResult;
use crate::abi::abicommand::AsyncType;
use crate::abi::opcall::OpCall;
use crate::abi::AsyncRequest;
use crate::abi::AsyncRequestValue;
use crate::runtime::controller_ctx::ControllerCtx;
use crate::runtime::http_engine::request_executor::start_request_executor;
use crate::runtime::http_engine::request_executor::start_request_stream_executor;
use futures::future::poll_fn;
use futures::future::Future;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::FutureExt;
use futures::StreamExt;
use http::{Request, Response};
use hyper::Body;
use log::debug;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex as MutexAsync;
use tower_service::Service;
use wasmtime::{Instance, Store};

pub struct ControllerModule {
    meta: ControllerModuleMetadata,
    instance: Instance,
    store: Store<ControllerCtx>,
    pending_ops: FuturesUnordered<OpCall<anyhow::Result<()>>>,
}

impl ControllerModule {
    pub(crate) fn new(
        meta: ControllerModuleMetadata,
        instance: Instance,
        store: Store<ControllerCtx>,
    ) -> Self {
        Self {
            meta: meta,
            instance: instance,
            store: store,
            pending_ops: FuturesUnordered::new(),
        }
    }

    fn handle_request<S>(
        name: String,
        request: AsyncRequestValue,
        result_sender: Sender<AsyncResult>,
        async_request_id: u64,
        cluster_url: http::Uri,
        http_client: Arc<MutexAsync<S>>,
    ) -> OpCall<anyhow::Result<()>>
    where
        S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
        S::Future: Send + 'static,
        S::Error: std::fmt::Debug,
    {
        match request {
            AsyncRequestValue::Http(value) => OpCall::eager(async move {
                debug!(
                    "Received request command from {} with id {}: {} {:?}",
                    name,
                    &async_request_id,
                    value.method().as_str(),
                    value.uri()
                );

                start_request_executor(
                    async_request_id,
                    value,
                    result_sender,
                    cluster_url,
                    http_client,
                )
                .await?;

                Ok(())
            }),
            AsyncRequestValue::HttpStream(value) => OpCall::eager(async move {
                debug!(
                    "Received stream request command from {} with id {}: {} {:?}",
                    name,
                    &async_request_id,
                    value.method().as_str(),
                    value.uri()
                );

                start_request_stream_executor(
                    async_request_id,
                    value,
                    result_sender,
                    cluster_url,
                    http_client,
                )
                .await?;

                Ok(())
            }),
            AsyncRequestValue::Delay(value) => OpCall::eager(async move {
                debug!(
                    "Received delay command from with id {}: {:?}",
                    &async_request_id, value
                );

                tokio::time::sleep(value.into()).await;

                result_sender
                    .clone()
                    .send(AsyncResult {
                        async_request_id: async_request_id,
                        value: None,
                        async_type: AsyncType::Future,
                    })
                    .await?;

                Ok(())
            }),
        }
    }

    fn handle_requests_poll<S>(
        name: &str,
        pending_ops: &mut FuturesUnordered<OpCall<anyhow::Result<()>>>,
        result_sender: &Sender<AsyncResult>,
        async_request_receiver: &mut UnboundedReceiver<AsyncRequest>,
        cluster_url: &http::Uri,
        http_client: &Arc<MutexAsync<S>>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<anyhow::Result<()>>
    where
        S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
        S::Future: Send + 'static,
        S::Error: std::fmt::Debug,
    {
        match async_request_receiver.poll_recv(cx) {
            Poll::Ready(Some(request)) => {
                let opcall = Self::handle_request(
                    name.to_string(),
                    request.value,
                    result_sender.clone(),
                    request.async_request_id,
                    cluster_url.clone(),
                    http_client.clone(),
                );

                pending_ops.push(opcall);

                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }

    pub async fn start<S>(
        &mut self,
        mut async_request_receiver: UnboundedReceiver<AsyncRequest>,
        cluster_url: http::Uri,
        http_client: Arc<MutexAsync<S>>,
    ) -> anyhow::Result<()>
    where
        S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
        S::Future: Send + 'static,
        S::Error: std::fmt::Debug,
    {
        let (async_result_tx, mut async_result_rx) = tokio::sync::mpsc::channel(10);
        let worker_name = self.meta.name.clone();

        let mut maybe_result: Option<AsyncResult> = None;
        loop {
            {
                let mut wasm_work: Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
                if let Some(result) = maybe_result.take() {
                    wasm_work = Box::pin(crate::abi::wakeup(
                        &mut self.store,
                        &self.instance,
                        result.async_request_id,
                        result.async_type,
                        result.value,
                    ));
                } else {
                    wasm_work = Box::pin(crate::abi::start_controller(
                        &mut self.store,
                        &self.instance,
                    ));
                }

                let pending_ops = &mut self.pending_ops;
                poll_fn(|cx| {
                    match wasm_work.poll_unpin(cx) {
                        Poll::Pending => {}    // wasm is running, check requests
                        other => return other, // wasm is ready, continue to next step
                    }

                    loop {
                        // receive new ops
                        match Self::handle_requests_poll(
                            worker_name.as_str(),
                            pending_ops,
                            &async_result_tx,
                            &mut async_request_receiver,
                            &cluster_url,
                            &http_client,
                            cx,
                        ) {
                            Poll::Ready(Ok(())) => {}
                            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)), // Found an error, stop
                            Poll::Pending => break,
                        }
                    }

                    Poll::Pending
                })
                .await?;
            }

            let next_result = poll_fn(|cx| {
                loop {
                    // check results
                    if let Poll::Ready(Some(result)) = async_result_rx.poll_recv(cx) {
                        return Poll::Ready(Ok(Some(result)));
                    }

                    'inner: loop {
                        // receive new ops
                        match Self::handle_requests_poll(
                            worker_name.as_str(),
                            &mut self.pending_ops,
                            &async_result_tx,
                            &mut async_request_receiver,
                            &cluster_url,
                            &http_client,
                            cx,
                        ) {
                            Poll::Ready(Ok(())) => {}
                            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)), // Found an error, stop
                            Poll::Pending => break 'inner,
                        }
                    }

                    // run the existing ops
                    match self.pending_ops.poll_next_unpin(cx) {
                        Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(err)), // Found a finished op
                        Poll::Ready(Some(Ok(()))) => continue, // Found a finished op
                        Poll::Ready(None) => return Poll::Ready(Ok(None)), // No more pending ops
                        Poll::Pending => break,
                    }
                }

                Poll::Pending
            })
            .await;

            if let Err(err) = next_result {
                return Err(err);
            }

            if let Ok(Some(result)) = next_result {
                maybe_result = Some(result);
                continue;
            }

            debug!("{}: DONE", worker_name.as_str());
            break;
        }

        Ok(())
    }
}
