#[macro_use]
extern crate log;

mod abi;
mod utils;
mod modules;
mod kube_watch;

use kube::{Config, Client};
use wasmer_runtime::{compile_with, error, Func, WasmPtr, Array, Instance};
use wasmer_singlepass_backend::SinglePassCompiler;
use std::{env, thread};
use std::path::PathBuf;

use crate::abi::Abi;
use crate::modules::ModuleMetadata;
use std::rc::Rc;
use crate::kube_watch::WatcherConfiguration;
use futures::{TryStreamExt, StreamExt};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use serde::de::DeserializeOwned;

fn main() -> error::Result<()> {
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
    let mods = modules::load_modules_from_dir(path)
        .expect("Cannot load the modules from the provided dir");

    let mut joins = Vec::with_capacity(mods.len());

    for m in mods {
        let (path, mm, wasm_bytes) = m;
        let url = cluster_url.clone();
        let rt_handle = runtime.handle().clone();
        let client = client.clone();
        let kube_client = kube_client.clone();
        let j = thread::spawn(move ||
            start_controller(path, mm, wasm_bytes, url,rt_handle, client, kube_client)
        );
        joins.push(j);
    }

    info!("Joining started controllers");

    let joined: Vec<(String, Instance, Vec<(u64, http::Request<Vec<u8>>)>)> = joins.into_iter()
        .map(|j| j.join().expect("thread join"))
        .collect();

    let mut instances = HashMap::new();
    let mut watch_to_start = Vec::with_capacity(joined.len());

    for (instance_name, instance, watch_confs) in joined {
        instances.insert(instance_name.clone(), instance);
        watch_to_start.push((instance_name, watch_confs))
    }

    let (tx, rx) = mpsc::channel();

    info!("Spawning watchers");

    // Spawn watchers
    for (instance_name, watch_reqs) in watch_to_start {
        for (id, req) in watch_reqs {
            let kube_client = kube_client.clone();
            let tx = tx.clone();
            let instance_name = instance_name.clone();

            info!("Starting watch {} for controller {}", id, &instance_name);

            runtime.spawn(async move {
                let mut stream = kube_client.request_events(req).await
                    .expect("watch events stream")
                    .boxed();
                while let Some(event) = stream.try_next().await.expect("watch event") {
                    info!("Sending new event to instance {} with event id {}", &instance_name, id);
                    tx.send((instance_name.clone(), id, event)).unwrap();
                }
            });
        }
    }

    info!("Started listening events channel");

    // Loop on the rx channel
    for (instance_name, event_id, event) in rx {
        let instance = instances.get(&instance_name).unwrap();
        let allocation_size: u32 = event.len() as u32;

        let allocate_fn: Func<u32, u32> = instance.exports.get("allocate").unwrap();

        let allocation_ptr: u32 = allocate_fn.call(allocation_size).expect("allocation");
        let allocation_wasm_ptr: WasmPtr<u8, Array> = WasmPtr::new(allocation_ptr);
        let memory_cell = allocation_wasm_ptr
            .deref(instance.context().memory(0), 0, allocation_size)
            .expect("Unable to retrieve memory cell to write event");
        for (i, b) in event.iter().enumerate() {
            memory_cell[i].set(*b);
        }

        let run_fn: Func<(u64, u32, u32), ()> = instance.exports.get("on_event").unwrap();
        run_fn
            .call(event_id, allocation_ptr, allocation_size)
            .expect("Something went wrong while invoking run");
    }

    Ok(())
}

fn start_controller(path: PathBuf, mm: ModuleMetadata, wasm_bytes: Vec<u8>, cluster_url: url::Url, rt_handle: tokio::runtime::Handle, http_client: reqwest::Client, kube_client: kube::Client) -> (String, wasmer_runtime::Instance, Vec<(u64, http::Request<Vec<u8>>)>) {
    info!("Starting module loaded from {} with meta {:?}", path.to_str().unwrap(), mm);

    // Compile the module
    let (module, duration) = execution_time!({
            compile_with(&wasm_bytes, &SinglePassCompiler::new())
              .expect("wasm compilation")
        });
    info!("Compilation time '{}' duration: {} ms", &mm.name, duration.as_millis());

    // get the version of the WASI module in a non-strict way, meaning we're
    // allowed to have extra imports
    let wasi_version = wasmer_wasi::get_wasi_version(&module, false)
        .expect("WASI version detected from Wasm module");

    // Resolve abi
    let abi = mm.abi.get_abi();

    // WASI imports
    let mut base_imports = wasmer_wasi::generate_import_object_for_version(
        wasi_version,
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let watcher_configuration = Arc::new(Mutex::new(WatcherConfiguration::new()));

    base_imports.extend(abi.generate_imports(Arc::clone(&watcher_configuration), cluster_url, rt_handle, http_client));

    // Compile our webassembly into an `Instance`.
    let instance = module
        .instantiate(&base_imports)
        .expect("Failed to instantiate wasm module");

    info!("Starting controller '{}'", &mm.name);
    abi.start_controller(&instance);

    let config = watcher_configuration.lock().unwrap();
    (mm.name, instance, config.generate_watch_requests())
}