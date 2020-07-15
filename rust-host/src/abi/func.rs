use wasmer_runtime::{Array, Ctx, WasmPtr};
use wasmer_runtime_core::{structures::TypedIndex, types::TableIndex};
use super::abi::{HttpRequest, HttpResponse, Ptr};
use std::convert::TryInto;
use bytes::Bytes;
use http::HeaderMap;
use std::cell::RefCell;

pub fn request_fn(rt: RefCell<tokio::runtime::Runtime>, http_client: reqwest::Client) -> impl Fn(&mut Ctx, WasmPtr<u8, Array>, u32, u32) -> u64 {
    let f = move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32, allocator_fn: u32| -> u64 {
        let inner_req_bytes: Vec<u8> = ptr
            .deref(ctx.memory(0), 0, len)
            .unwrap()
            .iter()
            .map(|c| c.get())
            .collect();
        let inner_request: HttpRequest = bincode::deserialize(&inner_req_bytes).unwrap();

        let inner_response = run_request(&mut rt.borrow_mut(), &http_client, inner_request);

        let inner_res_bytes = bincode::serialize(&inner_response).unwrap();

        // Now we need to ask to the wasm module to allocate memory to serve the response
        // We get a TableIndex from our raw value passed in
        let allocator_fn_typed = TableIndex::new(allocator_fn as usize);
        // and use it to call the corresponding function
        let allocator_params = &[(inner_res_bytes.len() as i32).into()];
        let result = ctx.call_with_table_index(allocator_fn_typed, allocator_params).unwrap();

        // Allocate and write response in wasm memory
        let allocation_ptr: u32 = result.get(0).unwrap().to_u128() as u32;
        let allocation_wasm_ptr: WasmPtr<u8, Array> = WasmPtr::new(allocation_ptr);
        let memory_cell = allocation_wasm_ptr
            .deref(ctx.memory(0), 0, inner_res_bytes.len() as u32)
            .expect("Unable to retrieve memory cell to write lorena");
        for (i, b) in inner_res_bytes.iter().enumerate() {
            memory_cell[i].set(*b);
        }

        Ptr {
            ptr: allocation_ptr,
            size: inner_res_bytes.len() as u32
        }.into()
    };
    f
}

/// Wrapper around the hacks to run the request
pub(crate) fn run_request(rt: &mut tokio::runtime::Runtime, http_client: &reqwest::Client, inner_request: HttpRequest) -> HttpResponse {
    let request: http::Request<Vec<u8>> = inner_request.into();
    let response: reqwest::Response = rt.block_on(
        async {
            http_client.execute(request.try_into().unwrap()).await
        })
        .unwrap();

    let status_code = response.status();
    let mut headers = HeaderMap::with_capacity(response.headers().len());
    for (k, v) in response.headers().iter() {
        headers.append(k, v.clone());
    }
    let response_body: Bytes = rt.block_on(
        async {
            response.bytes().await
        })
        .unwrap();

    HttpResponse {
        status_code,
        headers,
        body: response_body.to_vec(),
    }
}