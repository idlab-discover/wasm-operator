use kube::Config;
use std::env;
use std::path::PathBuf;
use tracing::info;

mod abi;
mod kube_client;
mod modules;
mod runtime;

use crate::modules::ControllerModuleMetadata;

use std::alloc::System;

#[global_allocator]
static A: System = System;

fn main() {
    std::env::set_var(
        "RUST_LOG",
        //"debug"
        "debug,tower=warn,rustls=warn,wasmtime_cranelift=warn,cranelift=warn,regalloc=warn,hyper=warn",
    );

    tracing_subscriber::fmt::init();

    // Bootstrap tokio runtime and kube-rs-async config/client
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Cannot create a tokio runtime");

    let kubeconfig = runtime
        .block_on(Config::infer())
        .expect("Cannot infer the kubeconfig");

    let cluster_url = kubeconfig.cluster_url.clone();

    let service = runtime
        .block_on(kube_client::create_client_service(kubeconfig))
        .expect("could not setup kube client");

    let mut args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("Usage: {} <modules-dir>", args.remove(0))
    }

    let path = PathBuf::from(args.remove(1));
    info!("Going to load from {}", path.to_str().unwrap());

    let cache_path = std::env::temp_dir().join("cache");
    std::fs::create_dir_all(&cache_path).unwrap();

    let swap_path = std::env::temp_dir().join("swap");
    std::fs::create_dir_all(&swap_path).unwrap();

    let mods = ControllerModuleMetadata::load_modules_from_dir(path)
        .expect("Cannot load the modules from the provided dir");

    runtime.block_on(async {
        let (runtime_command_sender, runtime_command_receiver) = tokio::sync::mpsc::channel(10);

        tokio::spawn(runtime::start(
            runtime_command_receiver,
            cluster_url,
            service,
            cache_path,
            swap_path,
        ));

        tokio::spawn(async move {
            for module_metadata in mods {
                runtime_command_sender
                    .send(runtime::Command::StartModule(module_metadata))
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
                    .unwrap();
            }
        });

        tokio::signal::ctrl_c().await.unwrap();
        info!("Closing")
    });
}
