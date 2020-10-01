#[macro_use]
extern crate log;

use kube::{Client, Config};
use std::env;
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::task;

mod abi;
mod kube_watch;
mod http;
mod modules;
mod delay;
mod utils;

use crate::abi::AbiConfig;
use crate::kube_watch::{Watchers};
use crate::modules::{ControllerModule, ControllerModuleMetadata};
use crate::abi::dispatcher::AsyncResultDispatcher;

fn main() {
    env_logger::init();

    // Bootstrap tokio runtime and kube-rs-async config/client
    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .expect("Cannot create a tokio runtime");
    let kubeconfig = runtime
        .block_on(Config::infer())
        .expect("Cannot infer the kubeconfig");
    let cluster_url = kubeconfig.cluster_url.clone();

    let http_client = reqwest::ClientBuilder::from(kubeconfig.clone())
        .build()
        .expect("Cannot build the http client from the kubeconfig");
    let kube_client = Client::new(kubeconfig);

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

        let (http_command_tx, http_command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (delay_command_tx, delay_command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (watch_command_tx, watch_command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (async_result_tx, async_result_rx) = tokio::sync::mpsc::channel(10);

        info!("Starting controllers");
        for (path, mm, wasm_bytes) in mods {
            let http_command_sender = http_command_tx.clone();
            let delay_command_sender = delay_command_tx.clone();
            let watch_command_sender = watch_command_tx.clone();

            joins.push(task::spawn_blocking(move || {
                info!(
                    "Starting module loaded from '{}' with meta {:?}",
                    path.to_str().unwrap(),
                    mm
                );
                start_controller(mm, wasm_bytes, AbiConfig {
                    http_command_sender,
                    delay_command_sender,
                    watch_command_sender
                })
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
        tokio::spawn(Watchers::start(watch_command_rx, async_result_tx.clone(), kube_client));
        tokio::spawn(http::start_request_executor(http_command_rx, async_result_tx.clone(), cluster_url, http_client));
        tokio::spawn(delay::start_delay_executor(delay_command_rx, async_result_tx));

        // Result dispatcher
        tokio::spawn(AsyncResultDispatcher::start(controllers, async_result_rx));

        tokio::signal::ctrl_c().await.unwrap();
        info!("Closing")
    });
}

fn start_controller(
    module_meta: ControllerModuleMetadata,
    wasm_bytes: Vec<u8>,
    abi_config: AbiConfig,
) -> anyhow::Result<ControllerModule> {
    let module_name = module_meta.name.clone();

    let (module, duration) =
        execution_time!({ ControllerModule::compile(module_meta, wasm_bytes, abi_config)? });
    info!(
        "Compilation time '{}' duration: {} ms",
        &module_name,
        duration.as_millis()
    );

    module.start()?;

    Ok(module)
}
