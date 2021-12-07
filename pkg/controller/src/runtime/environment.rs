use anyhow::Error;
use anyhow::{Context, Result};
use std::sync::Arc;
use wasmtime::{Config, Engine, Linker, Module, Store};
use crate::abi::register_imports;
use crate::modules::ControllerModule;
use crate::modules::ControllerModuleMetadata;
use crate::runtime::controller_ctx::ControllerCtx;
use tokio::sync::mpsc::UnboundedSender;
use wasmtime_wasi::tokio::WasiCtxBuilder;

use crate::abi::AsyncRequest;

#[derive(Clone)]
pub struct Environment {
    engine: Engine,
    linker: Arc<Linker<ControllerCtx>>,
}

impl Environment {
    pub fn new() -> Result<Self, Error> {
        let mut config = Config::new();
        // We need this engine's `Store`s to be async, and consume fuel, so
        // that they can co-operatively yield during execution.
        config.async_support(true);
        // config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::tokio::add_to_linker(&mut linker, |cx: &mut ControllerCtx| {
            &mut cx.wasi_ctx
        })?;

        register_imports(&mut linker)?;

        Ok(Self {
            engine,
            linker: Arc::new(linker),
        })
    }

    pub async fn cache_precompile(
        &self,
        wasm_path: impl AsRef<std::path::Path>,
        cache_path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<std::path::PathBuf> {
        let wasm_bytes = std::fs::read(&wasm_path).with_context(|| "failed to read input file")?;

        let cache_key = blake3::hash(&wasm_bytes).to_hex().to_string();

        let cache_file = cache_path.as_ref().join(cache_key).with_extension("wasm");

        if !cache_file.exists() {
            std::fs::write(&cache_file, self.engine.precompile_module(&wasm_bytes)?)?;
        }

        Ok(cache_file)
    }

    pub async fn compile(
        &self,
        meta: ControllerModuleMetadata,
        wasm_path: impl AsRef<std::path::Path>,
        async_client_id: u64,
        async_request_sender: UnboundedSender<AsyncRequest>,
    ) -> anyhow::Result<ControllerModule> {
        let envs = meta.env.iter().map(|env| (env.name.clone(), env.value.clone())).collect::<Vec<(String, String)>>();

        let wasi_ctx = WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .envs(envs.as_ref())?
            .args(meta.args.as_ref())?
            .build();

        let controller_ctx = ControllerCtx::new(wasi_ctx, async_client_id, async_request_sender);

        let mut store = Store::new(&self.engine, controller_ctx);

        // Compile our webassembly into an `Instance`.
        // let module = Module::from_file(&self.engine, wasm_path)?;
        let module = unsafe { Module::deserialize_file(&self.engine, wasm_path)? };

        let instance = self.linker.instantiate_async(&mut store, &module).await?;

        Ok(ControllerModule::new(meta, instance, store))
    }
}
