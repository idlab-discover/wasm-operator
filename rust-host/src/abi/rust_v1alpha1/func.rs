use super::data::{HttpRequest, HttpResponse, Ptr};
use bytes::Bytes;
use http::HeaderMap;
use std::convert::{TryFrom, TryInto};
use wasmer_runtime::{Array, Ctx, WasmPtr};
use wasmer_runtime_core::{structures::TypedIndex, types::TableIndex};

use crate::execution_time;
use super::AbiContext;
use tokio::runtime::Handle;

pub(crate) fn request_fn(
    abi_context: AbiContext
) -> impl Fn(&mut Ctx, WasmPtr<u8, Array>, u32, u32) -> u64 {
    let f = move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32, allocator_fn: u32| -> u64 {
        let (ptr, duration) = execution_time!({
            let inner_req_bytes: Vec<u8> = ptr
                .deref(ctx.memory(0), 0, len)
                .unwrap()
                .iter()
                .map(|c| c.get())
                .collect();
            let inner_request: HttpRequest = bincode::deserialize(&inner_req_bytes).unwrap();

            let inner_response = run_request(
                &abi_context,
                inner_request,
            );

            let inner_res_bytes = bincode::serialize(&inner_response).unwrap();

            // Now we need to ask to the wasm module to allocate memory to serve the response
            // We get a TableIndex from our raw value passed in
            let allocator_fn_typed = TableIndex::new(allocator_fn as usize);
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

            Ptr {
                ptr: allocation_ptr,
                size: inner_res_bytes.len() as u32,
            }
            .into()
        });
        println!("Request duration: {} ms", duration.as_millis());
        ptr
    };
    f
}

/// Wrapper around the hacks to run the request
pub(crate) fn run_request(
    abi_context: &AbiContext,
    mut inner_request: HttpRequest,
) -> HttpResponse {
    // Path request url
    inner_request.uri = http::Uri::try_from(finalize_url(
        &abi_context.cluster_url,
        inner_request.uri.path_and_query().unwrap(),
    ))
    .expect("Cannot build the final uri");

    let request: http::Request<Vec<u8>> = inner_request.into();
    let response: reqwest::Response = abi_context.rt_handle
        .block_on(async {
            Handle::current();
            abi_context.http_client.execute(request.try_into().unwrap()).await
        })
        .unwrap();

    let status_code = response.status();
    let mut headers = HeaderMap::with_capacity(response.headers().len());
    for (k, v) in response.headers().iter() {
        headers.append(k, v.clone());
    }
    let response_body: Bytes = abi_context.rt_handle
        .block_on(async {
            response.bytes().await
        })
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
fn finalize_url(cluster_url: &reqwest::Url, request_p_and_q: &http::uri::PathAndQuery) -> String {
    let base = cluster_url.as_str().trim_end_matches('/'); // pandq always starts with a slash
    format!("{}{}", base, request_p_and_q)
}
