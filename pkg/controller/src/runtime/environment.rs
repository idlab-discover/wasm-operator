use crate::abi::register_imports;
use crate::kube_client::KubeClientService;
use crate::modules::ControllerModule;
use crate::modules::ControllerModuleMetadata;
use crate::modules::OpsRunner;
use crate::modules::WasmRuntime;
use crate::runtime::controller_ctx::ControllerCtx;
use anyhow::Error;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::Semaphore as AsyncSemaphore;
use wasmtime::{Config, Engine, InstanceAllocationStrategy, Linker, OptLevel};
use wasmtime_wasi::WasiCtxBuilder;

#[derive(Clone)]
pub struct Environment {
    pub(crate) engine: Engine,
    pub(crate) linker: Arc<Linker<ControllerCtx>>,
}

impl Environment {
    pub fn new() -> Result<Self, Error> {
        let mut config = Config::new();
        config.generate_address_map(false);
        // TODO memory_init_cow is default true in newer versions of wasm time
        config.memory_init_cow(true);
        config.cranelift_opt_level(OptLevel::SpeedAndSize);

        if *super::COMPILE_WITH_UNINSTANCIATE {
            config.allocation_strategy(InstanceAllocationStrategy::Pooling {
                strategy: wasmtime::PoolingAllocationStrategy::ReuseAffinity,
                instance_limits: wasmtime::InstanceLimits {
                    count: *super::POOL_SIZE,
                    ..wasmtime::InstanceLimits::default()
                },
            });
        }

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |cx: &mut ControllerCtx| {
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
        wasm_path: std::path::PathBuf,
        cache_path: std::path::PathBuf,
    ) -> anyhow::Result<std::path::PathBuf> {
        let wasm_bytes = std::fs::read(&wasm_path).with_context(|| "failed to read input file")?;

        let cache_key = blake3::hash(&wasm_bytes).to_hex().to_string();

        let cache_file = cache_path.join(cache_key).with_extension("wasm");

        if !cache_file.exists() {
            std::fs::write(&cache_file, self.engine.precompile_module(&wasm_bytes)?)?;
        }

        Ok(cache_file)
    }

    pub fn new_controller_module(
        &self,
        meta: ControllerModuleMetadata,
        wasm_path: std::path::PathBuf,
        swap_path: std::path::PathBuf,
        async_client_id: u64,
        async_active_client_counter: Arc<AsyncSemaphore>,
        cluster_url: http::Uri,
        kube_client_service: KubeClientService,
    ) -> anyhow::Result<ControllerModule> {
        let ops_runner = Arc::new(Mutex::new(OpsRunner::new(
            meta.name.clone(),
            cluster_url,
            kube_client_service,
        )));

        let envs = meta
            .env
            .iter()
            .map(|env| (env.name.clone(), env.value.clone()))
            .collect::<Vec<(String, String)>>();

        let wasi_ctx = WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .envs(envs.as_ref())?
            .args(meta.args.as_ref())?
            .build();

        let controller_ctx = ControllerCtx::new(wasi_ctx, async_client_id, ops_runner.clone());

        Ok(ControllerModule::new(
            WasmRuntime::new(
                controller_ctx,
                wasm_path,
                swap_path,
                self.clone(),
                async_active_client_counter,
            ),
            ops_runner,
        ))
    }
}
