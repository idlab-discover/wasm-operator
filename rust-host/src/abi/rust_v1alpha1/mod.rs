use crate::kube_watch::{WatchKey};

use super::AbiConfig;
use super::dispatcher::AsyncType;
use std::cell::Cell;
use tokio::sync::mpsc::UnboundedSender;

mod http_data;
mod watch_data;

pub(crate) use http_data::{HttpRequest, HttpResponse};

use crate::abi::rust_v1alpha1::watch_data::WatchRequest;
use wasmer_runtime::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::fmt::Debug;
use crate::abi::commands::AbiCommand;

pub(crate) struct Abi {}

impl super::Abi for Abi {
    fn generate_imports(&self, controller_name: &str, abi_config: AbiConfig) -> ImportObject {
        let counter = Arc::new(AtomicU64::new(0));
        let request_ctx = AbiMethodCtx::new(controller_name, abi_config.http_command_sender, counter.clone());
        let watch_ctx = AbiMethodCtx::new(controller_name, abi_config.watch_command_sender, counter.clone());
        imports! {
            "http-proxy-abi" => {
                "request" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, size: u32| -> u64 {
                    request_ctx.request_impl(ctx, ptr, size)
                }),
            },
            "kube-watch-abi" => {
                "watch" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, size: u32| -> u64 {
                    watch_ctx.watch_impl(ctx, ptr, size)
                }),
            }
        }
    }

    fn start_controller(&self, instance: &Instance) -> anyhow::Result<()> {
        instance
            .exports
            .get::<Func<(), ()>>("run")?
            .call()
            .unwrap(); //TODO better error management

        Ok(())
    }

    fn wakeup(&self, instance: &Instance, async_request_id: u64, async_type: AsyncType, value: Option<Vec<u8>>) -> anyhow::Result<()> {
        let (memory_location_ptr, memory_location_size) = match value {
            None => (std::ptr::null::<*const u32>() as u32, 0),
            Some(event) => {
                let memory_location_size = event.len();
                let memory_location_ptr = self.allocate(instance, memory_location_size as u32)?;

                let allocation_wasm_ptr: WasmPtr<u8, Array> = WasmPtr::new(memory_location_ptr);
                let memory_cell = allocation_wasm_ptr
                    .deref(instance.context().memory(0), 0, memory_location_size as u32)
                    .expect("Unable to retrieve memory cell to write event");
                for (i, b) in event.iter().enumerate() {
                    memory_cell[i].set(*b);
                }

                (memory_location_ptr, memory_location_size)
            }
        };

        let wakeup_fn = match async_type {
            AsyncType::Future => instance.exports.get::<Func<(u64, u32, u32), ()>>("wakeup_future")?,
            AsyncType::Stream => instance.exports.get::<Func<(u64, u32, u32), ()>>("wakeup_stream")?,
        };

        wakeup_fn
            .call(async_request_id, memory_location_ptr, memory_location_size as u32)
            .unwrap(); //TODO better error management

        Ok(())
    }

    fn allocate(&self, instance: &Instance, allocation_size: u32) -> anyhow::Result<u32> {
        Ok(instance
            .exports
            .get::<Func<u32, u32>>("allocate")?
            .call(allocation_size)
            .unwrap() //TODO better error management
        )
    }
}

struct AbiMethodCtx<T: Sized + Debug> {
    controller_name: String,
    command_sender: UnboundedSender<AbiCommand<T>>,
    async_request_counter: Arc<AtomicU64>,
}

impl <T: Sized + Debug> AbiMethodCtx<T> {
    fn new(controller_name: &str, command_sender: UnboundedSender<AbiCommand<T>>, async_request_counter: Arc<AtomicU64>) -> Self {
        AbiMethodCtx {
            controller_name: controller_name.to_string(),
            command_sender,
            async_request_counter
        }
    }

    fn generate_async_request_id(&self) -> u64 {
        (&self.async_request_counter).fetch_add(1, Ordering::SeqCst)
    }
}

impl AbiMethodCtx<http::Request<Vec<u8>>> {
    fn request_impl(
        &self,
        ctx: &mut Ctx,
        ptr: WasmPtr<u8, Array>,
        size: u32
    ) -> u64 {
        let inner_req_bytes: Vec<u8> = ptr
            .deref(ctx.memory(0), 0, size)
            .unwrap()
            .iter()
            .map(Cell::get)
            .collect();

        // Get the request
        let inner_request: HttpRequest = bincode::deserialize(&inner_req_bytes).unwrap();

        let async_request_id = self.generate_async_request_id();

        self.command_sender
            .send(AbiCommand {
                async_request_id,
                controller_name: self.controller_name.clone(),
                value: inner_request.into()
            })
            .unwrap();

        async_request_id
    }
}

impl AbiMethodCtx<WatchKey> {
    fn watch_impl(
        &self,
        ctx: &mut Ctx,
        ptr: WasmPtr<u8, Array>,
        size: u32
    ) -> u64 {
        let watch_req_bytes: Vec<u8> = ptr
            .deref(ctx.memory(0), 0, size)
            .unwrap()
            .iter()
            .map(Cell::get)
            .collect();

        let watch_request: WatchRequest = bincode::deserialize(&watch_req_bytes).unwrap();
        let async_request_id = self.generate_async_request_id();
        debug!("Received new watch request '{:?}' from '{}'. Assigned id: {}", &watch_request, &self.controller_name, &async_request_id);

        self.command_sender
            .send(AbiCommand {
                async_request_id,
                controller_name: self.controller_name.clone(),
                value: watch_request.into()
            })
            .unwrap();

        async_request_id
    }
}
