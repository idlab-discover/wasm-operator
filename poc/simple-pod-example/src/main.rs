use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use futures::task::SpawnExt;
use kube::{
    api::{ListParams, PostParams},
    Api, Client, CustomResource, ResourceExt,
};
use kube_runtime::controller::{Context, Controller, ReconcilerAction};

use chrono::{Local, Utc};
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::env;
use std::ops::Deref;
use std::time::Duration;

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Kube error: {}", source))]
    #[snafu(context(false))]
    UnknownKubeError { source: kube::Error },
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Resource", group = "amurant.io", version = "v1", namespaced)]
pub struct ResourceSpec {
    nonce: String,
    start_timestamp: Option<MicroTime>,
    end_timestamp: Option<MicroTime>,
}

/// The controller triggers this on reconcile errors
fn error_policy(_error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(1)),
    }
}

// Data we want access to in error/reconcile calls
struct Data {
    client: Client,
}

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

async fn main_async() {
    let client = Client::default();

    let in_namespace = env::var("IN_NAMESPACE").unwrap_or("default".to_string());
    let in_resources: Api<Resource> = Api::namespaced(client.clone(), in_namespace.as_str());

    Controller::new(in_resources, ListParams::default())
        .run(reconcile, error_policy, Context::new(Data { client }))
        .for_each(|res| async move {
            match res {
                Ok((obj, _)) => println!("Reconciled {:?}", obj),
                Err(e) => println!("Reconcile error: {:?}", e),
            }
        })
        .await;
}

/// Controller triggers this whenever our main object or our children changed
async fn reconcile(in_resource: Resource, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let client = ctx.get_ref().client.clone();

    let name = in_resource.name();
    let nonce = in_resource.spec.nonce.clone();

    let out_namespace = env::var("OUT_NAMESPACE").unwrap_or("default".to_string());
    let out_resources: Api<Resource> = Api::namespaced(client.clone(), out_namespace.as_str());
    let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));
    match out_resources.get(&name).await {
        Ok(mut existing) => {
            if nonce != existing.spec.nonce {
                println!("nonce != current nonce, resetting resource");
                existing.spec.nonce = nonce;
                existing.spec.start_timestamp = Some(now_timestamp);
                existing.spec.end_timestamp = None;
                out_resources
                    .replace(&existing.name(), &PostParams::default(), &existing)
                    .await?;
            } else if existing.spec.end_timestamp.is_none() {
                println!("end_timestamp is None, update end_timestamp");
                existing.spec.end_timestamp = Some(now_timestamp);
                out_resources
                    .replace(&existing.name(), &PostParams::default(), &existing)
                    .await?;
            } else {
                println!("end_timestamp is set, doing nothing");
            }
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            println!("Creating pod");
            out_resources
                .create(
                    &PostParams::default(),
                    &resource(&name, &nonce, now_timestamp),
                )
                .await?;
        }
        Err(e) => Err(Error::UnknownKubeError { source: e })?,
    };

    Ok(ReconcilerAction {
        requeue_after: None,
    })
}

fn resource(name: &str, nonce: &str, start_timestamp: MicroTime) -> Resource {
    Resource {
        api_version: "amurant.io/v1".to_string(),
        kind: "Resource".to_string(),
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        spec: ResourceSpec {
            nonce: nonce.to_string(),
            start_timestamp: Some(start_timestamp),
            end_timestamp: None,
        },
    }
}
