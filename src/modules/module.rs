use super::ControllerModuleMetadata;
use crate::abi::{dispatcher::AsyncType, Abi, AbiConfig};
use log::debug;
use wasmer::*;
use wasmer_wasi::WasiState;

pub struct ControllerModule {
    meta: ControllerModuleMetadata,
    instance: Instance,
}

impl ControllerModule {
    pub fn compile(
        store: &Store,
        meta: ControllerModuleMetadata,
        wasm_bytes: Vec<u8>,
        abi_config: AbiConfig,
    ) -> anyhow::Result<ControllerModule> {
        let module = Module::new(&store, wasm_bytes)?;

        // get the version of the WASI module in a non-strict way, meaning we're
        // allowed to have extra imports
        // let wasi_version = wasmer_wasi::get_wasi_version(&module, false)
        //    .expect("WASI version detected from Wasm module");

        // Resolve abi
        let abi = meta.abi.get_abi();

        let mut wasi_env = WasiState::new(meta.name.clone())
            .env("RUST_LOG", "debug")
            .finalize()?;

        // WASI imports
        let mut base_imports = wasi_env.import_object(&module)?;

        abi.register_imports(&mut base_imports, &store, &meta.name, abi_config);

        // Compile our webassembly into an `Instance`.
        let instance = Instance::new(&module, &base_imports)?;

        Ok(ControllerModule { meta, instance })
    }

    pub fn name(&self) -> &str {
        &self.meta.name
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let abi = self.meta.abi.get_abi();
        abi.start_controller(&self.instance)?;
        debug!("start_controller completed '{:?}'", &self.meta);
        Ok(())
    }

    pub fn wakeup(
        &self,
        async_request_id: u64,
        async_type: AsyncType,
        value: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let abi = self.meta.abi.get_abi();
        Ok(abi.wakeup(&self.instance, async_request_id, async_type, value)?)
    }
}

// https://github.com/bytecodealliance/wasmtime/issues/793#issuecomment-692740254
unsafe impl Send for ControllerModule {}
