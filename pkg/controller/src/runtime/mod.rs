use crate::modules::ControllerModuleMetadata;
use futures::StreamExt;
use http::{Request, Response};
use hyper::Body;
use log::debug;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex as MutexAsync;
use tokio_stream::wrappers::ReceiverStream;
use tower_service::Service;

mod environment;
pub mod http_engine;
pub use environment::Environment;
pub mod controller_ctx;

pub enum Command {
    StartModule(ControllerModuleMetadata),
}

pub async fn start<S>(
    receiver: Receiver<Command>,
    cluster_url: http::Uri,
    http_client: Arc<MutexAsync<S>>,
    cache_path: impl AsRef<std::path::Path>,
) -> anyhow::Result<()>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Debug,
{
    let environment = Environment::new()?;
    let async_client_id_counter = Arc::new(AtomicU64::new(0));

    ReceiverStream::new(receiver)
        .map(|command| async {
            match command {
                Command::StartModule(metadata) => {
                    let async_client_id_counter_clone = async_client_id_counter.clone();
                    let environment_clone = environment.clone();
                    let cluster_url_clone = cluster_url.clone();
                    let http_client_clone = http_client.clone();

                    let name = metadata.name.clone();

                    let start = Instant::now();

                    let serialized_wasm = environment_clone
                        .cache_precompile(&metadata.wasm, cache_path.as_ref())
                        .await
                        .expect("precompiling failed");
                    debug!("precompilation: {} {:?}", name, start.elapsed());

                    let (async_request_tx, async_request_rx) =
                        tokio::sync::mpsc::unbounded_channel();

                    let async_client_id =
                        async_client_id_counter_clone.fetch_add(1, Ordering::SeqCst);

                    let start = Instant::now();
                    let mut module = environment_clone
                        .compile(metadata, serialized_wasm, async_client_id, async_request_tx)
                        .await
                        .expect("Failed to compile module");

                    debug!("compilation: {} {:?}", name, start.elapsed());

                    tokio::spawn(async move {
                        module
                            .start(async_request_rx, cluster_url_clone, http_client_clone)
                            .await
                    });
                }
            }
        })
        .buffer_unordered(10)
        .collect::<()>()
        .await;

    Ok(())
}
