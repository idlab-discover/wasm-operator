use super::dispatcher::AsyncType;
use super::AbiConfig;
use std::cell::Cell;
use tokio::sync::mpsc::UnboundedSender;

mod http_data;

pub(crate) use http_data::{HttpRequest, HttpResponse, HttpResponseStream};

use crate::abi::commands::AbiCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wasmer::*;

pub(crate) struct Abi {}

impl super::Abi for Abi {
    fn register_imports(
        &self,
        imports: &mut ImportObject,
        store: &Store,
        controller_name: &str,
        abi_config: AbiConfig,
    ) {
        let counter = Arc::new(AtomicU64::new(0));
        let request_env = AbiMethodRequest {
            controller_name: controller_name.to_string(),
            request_command_sender: abi_config.requestor_command_sender,
            stream_requestor_command_sender: abi_config.stream_requestor_command_sender,
            async_request_counter: counter.clone(),
            memory: LazyInit::new(),
        };
        let delay_env = AbiMethodDelay {
            controller_name: controller_name.to_string(),
            command_sender: abi_config.delay_command_sender,
            async_request_counter: counter.clone(),
        };

        let mut request_exports = Exports::new();
        request_exports.insert(
            "request",
            Function::new_native_with_env(store, request_env.clone(), abi_request),
        );
        request_exports.insert(
            "request_stream",
            Function::new_native_with_env(store, request_env, abi_request),
        );
        imports.register("http-proxy-abi", request_exports);

        let mut delay_exports = Exports::new();
        delay_exports.insert(
            "delay",
            Function::new_native_with_env(store, delay_env, abi_delay),
        );
        imports.register("delay-abi", delay_exports);
    }

    fn start_controller(&self, instance: &Instance) -> anyhow::Result<()> {
        instance
            .exports
            .get_native_function::<(), ()>("_start")?
            .call()
            .unwrap(); //TODO better error management

        Ok(())
    }

    fn wakeup(
        &self,
        instance: &Instance,
        async_request_id: u64,
        async_type: AsyncType,
        value: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let (memory_location_ptr, memory_location_size) = match value {
            None => (std::ptr::null::<*const u32>() as u32, 0),
            Some(event) => {
                let memory_location_size = event.len();
                let memory_location_ptr = self.allocate(instance, memory_location_size as u32)?;
                let memory = instance.exports.get_memory("memory")?;

                let allocation_wasm_ptr: WasmPtr<u8, Array> = WasmPtr::new(memory_location_ptr);
                let memory_cell = allocation_wasm_ptr
                    .deref(memory, 0, memory_location_size as u32)
                    .expect("Unable to retrieve memory cell to write event");
                for (i, b) in event.iter().enumerate() {
                    memory_cell[i].set(*b);
                }

                (memory_location_ptr, memory_location_size)
            }
        };

        let wakeup_fn = match async_type {
            AsyncType::Future => instance
                .exports
                .get_native_function::<(u64, u32, u32), ()>("wakeup_future")?,
            AsyncType::Stream => instance
                .exports
                .get_native_function::<(u64, u32, u32), ()>("wakeup_stream")?,
        };

        wakeup_fn
            .call(
                async_request_id,
                memory_location_ptr,
                memory_location_size as u32,
            )
            .unwrap(); //TODO better error management

        Ok(())
    }

    fn allocate(&self, instance: &Instance, allocation_size: u32) -> anyhow::Result<u32> {
        Ok(
            instance
                .exports
                .get_native_function::<u32, u32>("allocate")?
                .call(allocation_size)
                .unwrap(), //TODO better error management
        )
    }
}

#[derive(WasmerEnv, Clone)]
struct AbiMethodRequest {
    controller_name: String,
    request_command_sender: UnboundedSender<AbiCommand<http::Request<Vec<u8>>>>,
    stream_requestor_command_sender: UnboundedSender<AbiCommand<http::Request<Vec<u8>>>>,
    async_request_counter: Arc<AtomicU64>,
    #[wasmer(export)]
    memory: LazyInit<Memory>,
}

fn abi_request(env: &AbiMethodRequest, ptr: WasmPtr<u8, Array>, size: u32, stream: u32) -> u64 {
    let inner_req_bytes: Vec<u8> = ptr
        .deref(
            env.memory_ref()
                .expect("Memory should be set on `AbiMethodRequest` first"),
            0,
            size,
        )
        .unwrap()
        .iter()
        .map(Cell::get)
        .collect();

    // Get the request
    let inner_request: HttpRequest = bincode::deserialize(&inner_req_bytes).unwrap();

    let async_request_id = (&env.async_request_counter).fetch_add(1, Ordering::SeqCst);

    if stream == 0 {
        env.request_command_sender
            .send(AbiCommand {
                async_request_id,
                controller_name: env.controller_name.clone(),
                value: inner_request.into(),
            })
            .unwrap();
    } else {
        env.stream_requestor_command_sender
            .send(AbiCommand {
                async_request_id,
                controller_name: env.controller_name.clone(),
                value: inner_request.into(),
            })
            .unwrap();
    }

    async_request_id
}

#[derive(WasmerEnv, Clone)]
struct AbiMethodDelay {
    controller_name: String,
    command_sender: UnboundedSender<AbiCommand<Duration>>,
    async_request_counter: Arc<AtomicU64>,
}

fn abi_delay(env: &AbiMethodDelay, millis: u64) -> u64 {
    let async_request_id = (&env.async_request_counter).fetch_add(1, Ordering::SeqCst);

    env.command_sender
        .send(AbiCommand {
            async_request_id,
            controller_name: env.controller_name.clone(),
            value: Duration::from_millis(millis),
        })
        .unwrap();

    async_request_id
}
