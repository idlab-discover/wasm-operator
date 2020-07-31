use wasmer_runtime::{func, imports, Func, ImportObject, Instance};
use reqwest::Url;
use tokio::runtime::Handle;

mod data;
mod func;

pub(crate) struct AbiContext {
    cluster_url: url::Url,
    rt_handle: tokio::runtime::Handle,
    http_client: reqwest::Client
}

pub(crate) struct Abi {}

impl super::Abi for Abi {
    fn generate_imports(&self, cluster_url: Url, rt_handle: Handle, http_client: reqwest::Client) -> ImportObject {
        imports! {
            "http-proxy-abi" => {
                // the func! macro autodetects the signature
                "request" => func!(func::request_fn(AbiContext {
                    cluster_url,
                    rt_handle,
                    http_client
                })),
            },
        }
    }

    fn start_controller(&self, instance: &Instance) {
        let run_fn: Func<(), ()> = instance.exports.get("run").unwrap();
        run_fn
            .call()
            .expect("Something went wrong while invoking run");
    }
}
