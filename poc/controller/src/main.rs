use hyper::client::connect::HttpConnector;
use hyper_tls::HttpsConnector;
use kube::client::ConfigExt;
use kube::Config as KubeConfig;
use log::info;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as MutexAsync;
use tower::ServiceBuilder;

mod abi;
mod modules;
mod runtime;

use crate::modules::ControllerModuleMetadata;

fn main() {
    std::env::set_var("RUST_LOG", "info,controller=debug,cranelift=warn,kube=debug,regalloc=warn,wasmer_compiler_cranelift=warn");
    env_logger::init();

    // Bootstrap tokio runtime and kube-rs-async config/client
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Cannot create a tokio runtime");

    let kubeconfig = runtime
        .block_on(KubeConfig::infer())
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
        .option_layer(
            kubeconfig
                .auth_layer()
                .expect("auth layer is not configurable from kube config"),
        )
        .service(hyper_client);

    let arc_service = Arc::new(MutexAsync::new(service));

    let mut args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("Usage: {} <modules-dir>", args.remove(0))
    }
    let path = PathBuf::from(args.remove(1));
    info!("Going to load from {}", path.to_str().unwrap());
    let mods = ControllerModuleMetadata::load_modules_from_dir(path)
        .expect("Cannot load the modules from the provided dir");

    runtime.block_on(async {
        let (runtime_command_sender, runtime_command_receiver) = tokio::sync::mpsc::channel(3);

        tokio::spawn(runtime::start(
            runtime_command_receiver,
            cluster_url,
            arc_service.clone(),
        ));

        tokio::spawn(async move {
            for (_config_path, module_metadata, wasm_path) in mods {
                runtime_command_sender
                    .send(runtime::Command::StartModule(module_metadata, wasm_path))
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
                    .unwrap();
            }
        });

        tokio::signal::ctrl_c().await.unwrap();
        info!("Closing")
    });
}
