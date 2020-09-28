#[macro_use]
extern crate log;

use kube::{Client, Config};
use std::env;
use std::path::PathBuf;
use futures::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task;

mod abi;
mod kube_watch;
mod modules;
mod utils;

use crate::abi::AbiConfig;
use crate::kube_watch::{Dispatcher, WatchCommand, Watchers};
use crate::modules::{ControllerModule, ControllerModuleMetadata};

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
    let client = reqwest::ClientBuilder::from(kubeconfig.clone())
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

        let (watch_command_tx, watch_command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (watch_event_tx, watch_event_rx) = tokio::sync::mpsc::channel(10);

        info!("Starting controllers");
        for (path, mm, wasm_bytes) in mods {
            let url = cluster_url.clone();
            let client = client.clone();
            let tx = watch_command_tx.clone();

            joins.push(task::spawn_blocking(move || {
                info!(
                    "Starting module loaded from '{}' with meta {:?}",
                    path.to_str().unwrap(),
                    mm
                );
                start_controller(mm, wasm_bytes, url, client, tx)
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

        tokio::spawn(Watchers::start(
            watch_command_rx,
            watch_event_tx,
            kube_client,
        ));

        tokio::spawn(Dispatcher::start(controllers, watch_event_rx));

        tokio::signal::ctrl_c().await.unwrap();
        info!("Closing")
    });
}

fn start_controller(
    module_meta: ControllerModuleMetadata,
    wasm_bytes: Vec<u8>,
    cluster_url: url::Url,
    http_client: reqwest::Client,
    watch_command_sender: UnboundedSender<WatchCommand>,
) -> anyhow::Result<ControllerModule> {
    let config = AbiConfig {
        cluster_url,
        http_client,
        watch_command_sender,
    };

    let module_name = module_meta.name.clone();

    let (module, duration) =
        execution_time!({ ControllerModule::compile(module_meta, wasm_bytes, config)? });
    info!(
        "Compilation time '{}' duration: {} ms",
        &module_name,
        duration.as_millis()
    );

    module.start()?;

    Ok(module)
}
