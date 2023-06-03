use crate::abi::abicommand::AsyncResult;
use crate::abi::opcall::OpCall;
use crate::abi::AsyncRequestValue;
use crate::kube_client::KubeClientService;
use crate::runtime::http_engine::request_executor::start_request_executor;
use futures::stream::futures_unordered::FuturesUnordered;
use futures::StreamExt;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tracing::debug;

pub struct OpsRunner {
    name: String,
    cluster_url: http::Uri,
    service: KubeClientService,

    pub(crate) pending_ops: FuturesUnordered<OpCall<anyhow::Result<bool>>>,
    pub(crate) have_unpolled_ops: bool,
    pub(crate) nr_web_calls: usize,

    pub(crate) async_result_rx: Receiver<AsyncResult>,
    async_result_tx: Sender<AsyncResult>,
}

impl OpsRunner {
    pub(crate) fn new(name: String, cluster_url: http::Uri, service: KubeClientService) -> Self {
        let (async_result_tx, async_result_rx) = tokio::sync::mpsc::channel(10);
        Self {
            name,
            cluster_url,
            service,

            pending_ops: FuturesUnordered::new(),
            have_unpolled_ops: false,
            nr_web_calls: 0,

            async_result_rx,
            async_result_tx,
        }
    }

    fn handle_opcall(&mut self, opcall: OpCall<anyhow::Result<bool>>) {
        debug!("calling handle handle_opcall");
        self.pending_ops.push(opcall);
        self.have_unpolled_ops = true;
    }

    pub(crate) fn handle_request(&mut self, async_request_id: u64, request: AsyncRequestValue) {
        let name = self.name.clone();
        let result_sender = self.async_result_tx.clone();
        let cluster_url = self.cluster_url.clone();
        let service = self.service.clone();

        if let AsyncRequestValue::Http(_) = request {
            self.nr_web_calls += 1;
        }

        debug!("calling handle request");

        


        self.handle_opcall(match request {
            AsyncRequestValue::Http(value) => OpCall::eager(async move {
                debug!(
                    "Received request command from {} with id {}: {} {:?}",
                    name,
                    &async_request_id,
                    value.method().as_str(),
                    value.uri()
                );

                let (meta, body) = start_request_executor(value, cluster_url, service).await?;

                result_sender
                    .clone()
                    .send(AsyncResult {
                        async_request_id,
                        value: Some(bytes::Bytes::from(bincode::serialize(&meta)?)),
                        finished: false,
                    })
                    .await?;

                drop(meta);

                let full_body = hyper::body::to_bytes(body).await;

                result_sender
                    .clone()
                    .send(AsyncResult {
                        async_request_id,
                        value: Some(full_body?),
                        finished: true,
                    })
                    .await?;

                Ok(true)
            }),
            AsyncRequestValue::HttpStream(value) => OpCall::eager(async move {
                debug!(
                    "Received stream request command from {} with id {}: {} {:?}",
                    name,
                    &async_request_id,
                    value.method().as_str(),
                    value.uri()
                );

                let (meta, mut body) = start_request_executor(value, cluster_url, service).await?;

                result_sender
                    .clone()
                    .send(AsyncResult {
                        async_request_id,
                        value: Some(bytes::Bytes::from(bincode::serialize(&meta)?)),
                        finished: false,
                    })
                    .await?;

                drop(meta);

                while let Some(chunk) = body.next().await {
                    result_sender
                        .clone()
                        .send(AsyncResult {
                            async_request_id,
                            value: Some(chunk?),
                            finished: false,
                        })
                        .await?;
                }

                result_sender
                    .clone()
                    .send(AsyncResult {
                        async_request_id,
                        value: None,
                        finished: true,
                    })
                    .await?;

                Ok(false)
            }),
            AsyncRequestValue::Delay(value) => OpCall::eager(async move {
                debug!(
                    "Received delay command from with id {}: {:?}",
                    &async_request_id, value
                );

                tokio::time::sleep(value).await;

                result_sender
                    .clone()
                    .send(AsyncResult {
                        async_request_id,
                        value: None,
                        finished: true,
                    })
                    .await?;

                Ok(false)
            }),
        });
    }
}
