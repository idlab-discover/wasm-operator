use super::ControllerModuleMetadata;
use crate::abi::{Abi, AbiConfig};
use wasmer_runtime::*;
use wasmer_singlepass_backend::SinglePassCompiler;

pub struct ControllerModule {
    meta: ControllerModuleMetadata,
    instance: Instance,
}

impl ControllerModule {
    pub fn compile(
        meta: ControllerModuleMetadata,
        wasm_bytes: Vec<u8>,
        abi_config: AbiConfig,
    ) -> anyhow::Result<ControllerModule> {
        let module = compile_with(&wasm_bytes, &SinglePassCompiler::new())?;

        // get the version of the WASI module in a non-strict way, meaning we're
        // allowed to have extra imports
        let wasi_version = wasmer_wasi::get_wasi_version(&module, false)
            .expect("WASI version detected from Wasm module");

        // Resolve abi
        let abi = meta.abi.get_abi();

        // WASI imports
        let mut base_imports = wasmer_wasi::generate_import_object_for_version(
            wasi_version,
            vec![],
            vec![],
            vec![],
            vec![],
        );

        base_imports.extend(abi.generate_imports(&meta.name, abi_config));

        // Compile our webassembly into an `Instance`.
        let instance = module
            .instantiate(&base_imports)
            .unwrap(); //TODO better error management

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

    pub fn on_event(&self, event_id: u64, event: Vec<u8>) -> anyhow::Result<()> {
        let abi = self.meta.abi.get_abi();
        Ok(abi.on_event(&self.instance, event_id, event)?)
    }
}

// https://github.com/bytecodealliance/wasmtime/issues/793#issuecomment-692740254
unsafe impl Send for ControllerModule {}
