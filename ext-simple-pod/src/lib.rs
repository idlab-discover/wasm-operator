use k8s_openapi::api::core::v1::{Container, Pod, PodSpec};
use k8s_openapi::{
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use kube::{
    api::{ListParams, Meta, PostParams},
    Api, Client, CustomResource
};
use kube_runtime::controller::{Context, Controller, ReconcilerAction};
use futures::task::SpawnExt;

use serde::{Deserialize, Serialize};
use std::ops::Deref;
use futures::StreamExt;
use std::time::{Duration};
use snafu::Snafu;

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Pod doesn't have image"))]
    PodWithoutImage,
    #[snafu(display("Kube error: {}", source))]
    #[snafu(context(false))]
    UnknownKubeError{
        source: kube::Error
    }
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug)]
#[kube(group = "slinky.dev", version = "v1", namespaced)]
pub struct SimplePodSpec {
    image: String,
}

// TODO: Add status?

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

#[no_mangle]
pub extern "C" fn run() {
    let exec = kube::abi::get_mut_executor();
    // Start the main
    exec.deref().borrow_mut().spawner().spawn(main()).unwrap();
    // Give a little push to the executor
    exec.deref().borrow_mut().run_until_stalled();
}

async fn main() {
    let client = Client::default();

    let simple_pods: Api<SimplePod> = Api::namespaced(client.clone(), "default");
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");

    Controller::new(simple_pods, ListParams::default())
        .owns(pods, ListParams::default())
        .run(reconcile, error_policy, Context::new(Data { client }))
        .for_each(|res| async move { match res {
            Ok((obj, _)) => println!("Reconciled {:?}", obj),
            Err(e) => println!("Reconcile error: {:?}", e),
        }}).await;
}

/// Controller triggers this whenever our main object or our children changed
async fn reconcile(simple_pod: SimplePod, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let client = ctx.get_ref().client.clone();
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");

    let name = simple_pod.name();
    let image = simple_pod.spec.image;

    match pods.get(&name).await {
        Ok(mut existing) => {
            let existing_image = existing
                .spec
                .as_ref()
                .map(|spec| spec.containers[0].image.as_ref())
                .flatten()
                .ok_or(Error::PodWithoutImage)?;
            if existing_image == &image {
                println!("Image is equal, doing nothing");
            } else {
                let mut spec = existing.spec.unwrap();
                spec.containers[0].image = Some(image.to_string());
                existing.spec = Some(spec);
                println!("Replacing pod");
                pods.replace(&existing.name(), &PostParams::default(), &existing).await?;
            }
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            println!("Creating pod");
            pods.create(
                &PostParams::default(),
                &pod(&name, &image)
            ).await?;
        }
        Err(e) => Err(Error::UnknownKubeError { source: e })?,
    };

    Ok(ReconcilerAction {
        requeue_after: Some(Duration::from_secs(300)),
    })
}

fn pod(name: &str, image: &str) -> Pod {
    // TODO: Add ownerRef for deletion handling.
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "default-container".to_string(),
                image: Some(image.to_string()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status: None,
    }
}
