use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use kube::{
    api::{ListParams, PostParams},
    Api, Client, CustomResource, ResourceExt,
};
use kube_runtime::controller::{Action, Context, Controller};

use chrono::{Local, Utc};
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;
use tracing::info;

#[cfg(target_arch = "wasm32")]
use {futures::task::SpawnExt, std::ops::Deref};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Kube error: {}", source))]
    #[snafu(context(false))]
    UnknownKubeError { source: kube::Error },
}

#[derive(CustomResource, Debug, Serialize, Deserialize, Default, Clone, JsonSchema)]
#[kube(
    kind = "TestResource",
    group = "amurant.io",
    version = "v1",
    namespaced
)]
pub struct TestResourceSpec {
    nonce: i64,
    updated_at: Option<MicroTime>,
}

/// The controller triggers this on reconcile errors
fn error_policy(_error: &Error, _ctx: Context<Data>) -> Action {
    Action::requeue(Duration::from_secs(1))
}

// Data we want access to in error/reconcile calls
struct Data {
    client: Client,
    out_namespace: String,
    huge_mem_alloc: Arc<Vec<u8>>,
}

#[cfg(target_arch = "wasm32")]
fn main() {
    let exec = kube_runtime_abi::get_mut_executor();
    // Start the main
    exec.deref()
        .borrow_mut()
        .spawner()
        .spawn(main_async())
        .unwrap();
    // Give a little push to the executor
    exec.deref().borrow_mut().run_until_stalled();
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    main_async().await;
}

async fn main_async() {
    tracing_subscriber::fmt::init();

    let compile_nonce: &'static str = env!("COMPILE_NONCE");
    info!("compile_nonce: {}", compile_nonce);

    let client = Client::try_default()
        .await
        .expect("could not create kube client");

    let in_namespace = env::var("IN_NAMESPACE").unwrap_or("default".to_string());
    let out_namespace = env::var("OUT_NAMESPACE").unwrap_or("default".to_string());

    // const heap_mem_size: usize = 0 * 1024 * 1024; // 0MiB
    // const heap_mem_size: usize = 1 * 1024 * 1024; // 1MiB
    // const heap_mem_size: usize = 2 * 1024 * 1024; // 2MiB
    // const heap_mem_size: usize = 4 * 1024 * 1024; // 4MiB
    
    let heap_mem_size = env::var("HEAP_MEM_SIZE").unwrap_or("0".to_string());
    let heap_mem_size = heap_mem_size.parse::<usize>().unwrap();

    let in_resources: Api<TestResource> = Api::namespaced(client.clone(), in_namespace.as_str());

    let mut huge_mem_alloc = Vec::with_capacity(heap_mem_size);
    for i in 0..heap_mem_size {
        huge_mem_alloc.push((i + 9 % 256) as u8);
    }
    let huge_mem_alloc = Arc::new(huge_mem_alloc);

    Controller::new(
        in_resources,
        ListParams {
            bookmarks: false,
            ..ListParams::default()
        },
    )
    .run(
        reconcile,
        error_policy,
        Context::new(Data {
            client,
            out_namespace,
            huge_mem_alloc,
        }),
    )
    .for_each(|res| async move {
        match res {
            Ok((obj, _)) => debug!("Reconciled {:?}", obj),
            Err(e) => debug!("Reconcile error: {:?}", e),
        }
    })
    .await;
}

/// Controller triggers this whenever our main object or our children changed
async fn reconcile(
    in_test_resource: Arc<TestResource>,
    ctx: Context<Data>,
) -> Result<Action, Error> {
    let client = ctx.get_ref().client.clone();
    let out_namespace = ctx.get_ref().out_namespace.clone();

    let name = in_test_resource.name();
    let nonce = in_test_resource.spec.nonce.clone();

    let out_test_resources: Api<TestResource> =
        Api::namespaced(client.clone(), out_namespace.as_str());
    let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));

    match out_test_resources.get(&name).await {
        Ok(mut existing) => {
            if nonce > existing.spec.nonce {
                println!("nonce {} > current nonce  {}, resetting resource",nonce,existing.spec.nonce );
                existing.spec.nonce = nonce;
                existing.spec.updated_at = Some(now_timestamp);
                out_test_resources
                    .replace(&existing.name(), &PostParams::default(), &existing)
                    .await?;
            } else {
                debug!("nonce <= current nonce, doing nothing");
            }
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            debug!("Creating test resource");
            out_test_resources
                .create(
                    &PostParams::default(),
                    &test_resource(&name, &nonce, now_timestamp),
                )
                .await?;
        }
        Err(e) => Err(Error::UnknownKubeError { source: e })?,
    };

    Ok(Action::await_change())
}

fn test_resource(name: &str, nonce: &i64, start_timestamp: MicroTime) -> TestResource {
    TestResource {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        spec: TestResourceSpec {
            nonce: nonce.clone(),
            updated_at: Some(start_timestamp),
        },
    }
}
