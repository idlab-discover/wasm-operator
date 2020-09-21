use wasmer_runtime::{func, imports, Func, ImportObject, Instance};
use reqwest::Url;
use tokio::runtime::Handle;
use crate::kube_watch::WatcherConfiguration;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

mod data;
mod func;

pub(crate) struct AbiContext {
    cluster_url: url::Url,
    rt_handle: tokio::runtime::Handle,
    http_client: reqwest::Client
}

pub(crate) struct Abi {}

impl super::Abi for Abi {
    fn generate_imports(&self, watcher_configuration: Arc<Mutex<WatcherConfiguration>>, cluster_url: Url, rt_handle: Handle, http_client: reqwest::Client) -> ImportObject {
        imports! {
            "http-proxy-abi" => {
                "request" => func!(func::request_fn(AbiContext {
                    cluster_url,
                    rt_handle,
                    http_client
                })),
            },
            "kube-watch-abi" => {
                "watch" => func!(func::watch_fn(watcher_configuration)),
            }
        }
    }

    fn start_controller(&self, instance: &Instance) {
        let run_fn: Func<(), ()> = instance.exports.get("run").unwrap();
        run_fn
            .call()
            .expect("Something went wrong while invoking run");
    }
}
