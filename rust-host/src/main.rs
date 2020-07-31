#[macro_use]
extern crate log;

mod abi;
mod utils;
mod modules;

use kube::Config;
use wasmer_runtime::{compile_with, error};
use wasmer_singlepass_backend::SinglePassCompiler;
use std::{env, thread};
use std::path::PathBuf;

use crate::abi::Abi;
use crate::modules::ModuleMetadata;

fn main() -> error::Result<()> {
    env_logger::init();

    // Bootstrap tokio runtime and kube config/client
    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .expect("Cannot create a tokio runtime");
    let kubeconfig = runtime
        .block_on(Config::infer())
        .expect("Cannot infer the kubeconfig");
    let cluster_url = kubeconfig.cluster_url.clone();
    let client = reqwest::ClientBuilder::from(kubeconfig)
        .build()
        .expect("Cannot build the http client from the kubeconfig");

    let mut args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("Usage: {} <modules-dir>", args.remove(0))
    }
    let path = PathBuf::from(args.remove(1));
    info!("Going to load from {}", path.to_str().unwrap());
    let mods = modules::load_modules_from_dir(path)
        .expect("Cannot load the modules from the provided dir");

    let mut joins = vec![];

    for m in mods {
        let (path, mm, wasm_bytes) = m;
        let url = cluster_url.clone();
        let rt_handle = runtime.handle().clone();
        let client = client.clone();
        let j = thread::spawn(move ||
            start_controller(path, mm, wasm_bytes, url,rt_handle, client)
        );
        joins.push(j);
    }

    for j in joins {
        j.join().unwrap();
    }
    Ok(())
}

fn start_controller(path: PathBuf, mm: ModuleMetadata, wasm_bytes: Vec<u8>, cluster_url: url::Url, rt_handle: tokio::runtime::Handle, http_client: reqwest::Client) {
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

    base_imports.extend(abi.generate_imports(cluster_url, rt_handle, http_client));

    // Compile our webassembly into an `Instance`.
    let instance = module
        .instantiate(&base_imports)
        .expect("Failed to instantiate wasm module");

    info!("Starting controller '{}'", &mm.name);
    abi.start_controller(&instance);
}
