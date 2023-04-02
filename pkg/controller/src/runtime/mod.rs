use crate::kube_client::KubeClientService;
use crate::modules::ControllerModuleMetadata;
use futures::StreamExt;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Semaphore as AsyncSemaphore;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;
use tracing::Instrument;

mod environment;
pub mod http_engine;
pub use environment::Environment;
pub mod controller_ctx;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref COMPILE_WITH_UNINSTANCIATE: bool = env!("COMPILE_WITH_UNINSTANTIATE") == "TRUE";
    pub static ref POOL_SIZE: u32 = if *COMPILE_WITH_UNINSTANCIATE {
        // TODO why  set pool size to 10?
        100
    } else {
        1000
    };
}

pub enum Command {
    StartModule(ControllerModuleMetadata),
}

pub async fn start(
    receiver: Receiver<Command>,
    cluster_url: http::Uri,
    kube_client_service: KubeClientService,
    cache_path: std::path::PathBuf,
    swap_path: std::path::PathBuf,
) -> anyhow::Result<()> {
    let environment = Environment::new()?;
    let async_client_id_counter = Arc::new(AtomicU64::new(0));
    let async_active_client_counter = Arc::new(AsyncSemaphore::new(*POOL_SIZE as usize));

    ReceiverStream::new(receiver)
        .map(|command| async {
            match command {
                Command::StartModule(metadata) => {
                    let async_client_id_counter_clone = async_client_id_counter.clone();
                    let async_active_client_counter_clone = async_active_client_counter.clone();
                    let environment_clone = environment.clone();
                    let cluster_url_clone = cluster_url.clone();
                    let kube_client_service_clone = kube_client_service.clone();

                    let name = metadata.name.clone();

                    let start = Instant::now();
                    let serialized_wasm_path = environment_clone
                        .cache_precompile(metadata.wasm.clone(), cache_path.clone())
                        .await
                        .expect("precompiling failed");
                    debug!("precompilation: {} {:?}", name, start.elapsed());

                    let async_client_id =
                        async_client_id_counter_clone.fetch_add(1, Ordering::SeqCst);
                    let client_swap_path =
                        swap_path.join(format!("worker_{}_mem.bin", async_client_id));

                    let start = Instant::now();
                    let mut module = environment_clone
                        .new_controller_module(
                            metadata,
                            serialized_wasm_path,
                            client_swap_path,
                            async_client_id,
                            async_active_client_counter_clone,
                            cluster_url_clone,
                            kube_client_service_clone,
                        )
                        .expect("failed to create module");

                    debug!("compilation: {} {:?}", name, start.elapsed());

                    tokio::spawn(async move {
                        module
                            .start()
                            .instrument(tracing::debug_span!("client", client_id = async_client_id))
                            .await
                            .expect("The module execution failed")
                    });
                }
            }
        })
        .buffer_unordered(10)
        .collect::<()>()
        .await;

    Ok(())
}
