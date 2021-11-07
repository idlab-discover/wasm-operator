use log::{debug, info};

use kube::Config;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use tokio::task;
use tower::ServiceBuilder;
use hyper::client::connect::HttpConnector;
use hyper_tls::HttpsConnector;
use kube::client::ConfigExt;
use std::time::Duration;
use wasmer::{Store, Universal};
use wasmer_compiler_cranelift::Cranelift;
use std::sync::Arc;
use tokio::sync::Mutex;

mod abi;
mod delay;
mod http;
mod modules;
mod utils;

use crate::abi::dispatcher::AsyncResultDispatcher;
use crate::abi::AbiConfig;
use crate::modules::{ControllerModule, ControllerModuleMetadata};

fn main() {
    std::env::set_var("RUST_LOG", "info,controller=debug,cranelift=warn,kube=debug,regalloc=warn,wasmer_compiler_cranelift=warn");
    env_logger::init();

    // Bootstrap tokio runtime and kube-rs-async config/client
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Cannot create a tokio runtime");

    let kubeconfig = runtime
        .block_on(Config::infer())
        .expect("Cannot infer the kubeconfig");

    let cluster_url = kubeconfig.cluster_url.clone();

    let https = kubeconfig
        .native_tls_https_connector()
        .expect("could not load https kube config");

    let hyper_client: hyper::Client<HttpsConnector<HttpConnector>, hyper::Body> =
        hyper::Client::builder()
            .pool_idle_timeout(Duration::from_secs(30))
            .build(https);

    let service = ServiceBuilder::new()
        .layer(kubeconfig.base_uri_layer())
        .option_layer(kubeconfig.auth_layer().expect("auth layer is not configurable from kube config"))
        .service(hyper_client);
    
    let arc_service = Arc::new(Mutex::new(service));

    let mut args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("Usage: {} <modules-dir>", args.remove(0))
    }
    let path = PathBuf::from(args.remove(1));
    info!("Going to load from {}", path.to_str().unwrap());
    let mods = ControllerModuleMetadata::load_modules_from_dir(path)
        .expect("Cannot load the modules from the provided dir");

    runtime.block_on(async {
        let mut joins = Vec::with_capacity(mods.len());

        let (requestor_command_tx, requestor_command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (stream_requestor_command_tx, stream_requestor_command_rx) =
            tokio::sync::mpsc::unbounded_channel();
        let (delay_command_tx, delay_command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (async_result_tx, async_result_rx) = tokio::sync::mpsc::channel(10);

        let store = Store::new(&Universal::new(Cranelift::new()).engine()); // TODO: check this engine

        info!("Starting controllers");
        for (path, mm, wasm_bytes) in mods {
            let requestor_command_sender = requestor_command_tx.clone();
            let stream_requestor_command_sender = stream_requestor_command_tx.clone();
            let delay_command_sender = delay_command_tx.clone();

            joins.push(task::spawn_blocking({
                let store_copy = store.clone();
                move || {
                    info!(
                        "Starting module loaded from '{}' with meta {:?}",
                        path.to_str().unwrap(),
                        mm
                    );
                    start_controller(
                        store_copy,
                        mm,
                        wasm_bytes,
                        AbiConfig {
                            requestor_command_sender,
                            stream_requestor_command_sender,
                            delay_command_sender,
                        },
                    )
                }
            }));
        }

        debug!("Joining started controllers");

        let controllers = futures::future::join_all(joins)
            .await
            .into_iter()
            .flat_map(|r| r.into_iter())
            .map(|r| r.map(|module| (module.name().to_string(), module)))
            .collect::<anyhow::Result<HashMap<String, ControllerModule>>>()
            .expect("All controllers started correctly");

        // Command executors
        tokio::spawn(http::start_request_executor(
            requestor_command_rx,
            async_result_tx.clone(),
            cluster_url.clone(),
            arc_service.clone(),
        ));
        tokio::spawn(http::start_request_stream_executor(
            stream_requestor_command_rx,
            async_result_tx.clone(),
            cluster_url,
            arc_service,
        ));
        tokio::spawn(delay::start_delay_executor(
            delay_command_rx,
            async_result_tx,
        ));

        // Result dispatcher
        tokio::spawn(AsyncResultDispatcher::start(controllers, async_result_rx));

        tokio::signal::ctrl_c().await.unwrap();
        info!("Closing")
    });
}

fn start_controller(
    store: Store,
    module_meta: ControllerModuleMetadata,
    wasm_bytes: Vec<u8>,
    abi_config: AbiConfig,
) -> anyhow::Result<ControllerModule> {
    let module_name = module_meta.name.clone();

    let (module, duration) = execution_time!({
        ControllerModule::compile(
            &store,
            module_meta,
            wasm_bytes,
            abi_config,
        )?
    });
    info!(
        "Compilation time '{}' duration: {} ms",
        &module_name,
        duration.as_millis()
    );

    module.start()?;

    Ok(module)
}
