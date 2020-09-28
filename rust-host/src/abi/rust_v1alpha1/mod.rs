use crate::kube_watch::WatchCommand;

use super::AbiConfig;
use std::cell::{RefCell, Cell};
use tokio::sync::mpsc::UnboundedSender;

mod http_data;
mod watch_data;

use bytes::Bytes;
use http::HeaderMap;
use http_data::{HttpRequest, HttpResponse, Ptr};
use std::convert::{TryFrom, TryInto};

use crate::execution_time;
use tokio::runtime::Handle;

use crate::abi::rust_v1alpha1::watch_data::WatchRequest;
use wasmer_runtime::*;
use wasmer_runtime_core::{structures::TypedIndex, types::TableIndex};

pub(crate) struct Abi {}

impl super::Abi for Abi {
    fn generate_imports(&self, controller_name: &str, abi_config: AbiConfig) -> ImportObject {
        let request_ctx = RequestFnCtx {
            cluster_url: abi_config.cluster_url,
            http_client: abi_config.http_client,
        };
        let watch_ctx = WatchFnCtx {
            controller_name: controller_name.to_string(),
            watch_command_sender: abi_config.watch_command_sender,
            watch_counter: RefCell::new(0),
        };
        imports! {
            "http-proxy-abi" => {
                "request" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, size: u32, allocator_fn_ptr: u32| -> u64 {
                    request_ctx.request_impl(ctx, ptr, size, allocator_fn_ptr)
                }),
            },
            "kube-watch-abi" => {
                "watch" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, size: u32, allocator_fn_ptr: u32| -> u64 {
                    watch_ctx.watch_impl(ctx, ptr, size, allocator_fn_ptr)
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

    fn on_event(&self, instance: &Instance, event_id: u64, event: Vec<u8>) -> anyhow::Result<()> {
        let memory_location_size = event.len();
        let memory_location_ptr = self.allocate(instance, memory_location_size as u32)?;

        let allocation_wasm_ptr: WasmPtr<u8, Array> = WasmPtr::new(memory_location_ptr);
        let memory_cell = allocation_wasm_ptr
            .deref(instance.context().memory(0), 0, memory_location_size as u32)
            .expect("Unable to retrieve memory cell to write event");
        for (i, b) in event.iter().enumerate() {
            memory_cell[i].set(*b);
        }

        instance
            .exports
            .get::<Func<(u64, u32, u32), ()>>("on_event")?
            .call(event_id, memory_location_ptr, memory_location_size as u32)
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

pub(crate) struct RequestFnCtx {
    cluster_url: url::Url,
    http_client: reqwest::Client,
}

impl RequestFnCtx {
    fn request_impl(
        &self,
        ctx: &mut Ctx,
        ptr: WasmPtr<u8, Array>,
        size: u32,
        allocator_ptr_fn: u32,
    ) -> u64 {
        let inner_req_bytes: Vec<u8> = ptr
            .deref(ctx.memory(0), 0, size)
            .unwrap()
            .iter()
            .map(Cell::get)
            .collect();

        // Get the request
        let inner_request: HttpRequest = bincode::deserialize(&inner_req_bytes).unwrap();
        let req_uri = inner_request.uri.clone();

        let (inner_response, duration) = execution_time!({ self.execute_request(inner_request) });
        debug!(
            "Request '{}' duration: {} ms",
            req_uri,
            duration.as_millis()
        );

        let inner_res_bytes = bincode::serialize(&inner_response).unwrap();

        // Now we need to ask to the wasm module to allocate memory to serve the response
        // We get a TableIndex from our raw value passed in
        let allocator_fn_typed = TableIndex::new(allocator_ptr_fn as usize);
        // and use it to call the corresponding function
        let allocator_params = &[(inner_res_bytes.len() as i32).into()];
        let result = ctx
            .call_with_table_index(allocator_fn_typed, allocator_params)
            .unwrap();

        // Allocate and write response in wasm memory
        let allocation_ptr: u32 = result.get(0).unwrap().to_u128() as u32;
        let allocation_wasm_ptr: WasmPtr<u8, Array> = WasmPtr::new(allocation_ptr);
        let memory_cell = allocation_wasm_ptr
            .deref(ctx.memory(0), 0, inner_res_bytes.len() as u32)
            .expect("Unable to retrieve memory cell to write lorena");
        for (i, b) in inner_res_bytes.iter().enumerate() {
            memory_cell[i].set(*b);
        }

        // Return the packed bytes
        Ptr {
            ptr: allocation_ptr,
            size: inner_res_bytes.len() as u32,
        }
        .into()
    }

    fn execute_request(&self, mut inner_request: HttpRequest) -> HttpResponse {
        //TODO implement error propagation when requests goes wrong

        // Path request url
        inner_request.uri =
            http::Uri::try_from(self.generate_url(inner_request.uri.path_and_query().unwrap()))
                .expect("Cannot build the final uri");

        let request: http::Request<Vec<u8>> = inner_request.into();
        let response: reqwest::Response = Handle::current()
            .block_on(async {
                Handle::current();
                self.http_client.execute(request.try_into().unwrap()).await
            })
            .unwrap();

        let status_code = response.status();
        let mut headers = HeaderMap::with_capacity(response.headers().len());
        for (k, v) in response.headers().iter() {
            headers.append(k, v.clone());
        }
        let response_body: Bytes = Handle::current()
            .block_on(async { response.bytes().await })
            .unwrap();

        HttpResponse {
            status_code,
            headers,
            body: response_body.to_vec(),
        }
    }

    /// An internal url joiner to deal with the two different interfaces
    ///
    /// - api module produces a http::Uri which we can turn into a PathAndQuery (has a leading slash by construction)
    /// - config module produces a url::Url from user input (sometimes contains path segments)
    ///
    /// This deals with that in a pretty easy way (tested below)
    fn generate_url(&self, request_p_and_q: &http::uri::PathAndQuery) -> String {
        let base = self.cluster_url.as_str().trim_end_matches('/');
        format!("{}{}", base, request_p_and_q)
    }
}

pub(crate) struct WatchFnCtx {
    controller_name: String,

    watch_command_sender: UnboundedSender<WatchCommand>,
    watch_counter: RefCell<u64>,
}

impl WatchFnCtx {
    fn watch_impl(
        &self,
        ctx: &mut Ctx,
        ptr: WasmPtr<u8, Array>,
        size: u32,
        _allocator: u32,
    ) -> u64 {
        let watch_req_bytes: Vec<u8> = ptr
            .deref(ctx.memory(0), 0, size)
            .unwrap()
            .iter()
            .map(Cell::get)
            .collect();

        let watch_request: WatchRequest = bincode::deserialize(&watch_req_bytes).unwrap();

        let watch_counter: &mut u64 = &mut self.watch_counter.borrow_mut();
        let this_watch_counter: u64 = *watch_counter;
        *watch_counter += 1;

        debug!("Received new watch request '{:?}' from '{}'. Assigned id: {}", &watch_request, &self.controller_name, this_watch_counter);

        self.watch_command_sender
            .send(
                watch_request.into_watch_command(self.controller_name.clone(), this_watch_counter),
            )
            .unwrap();

        this_watch_counter
    }
}
