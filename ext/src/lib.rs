use k8s_openapi::api::core::v1::{Container, Pod, PodSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{ListParams, Meta, PostParams, WatchEvent};
use kube::runtime::Informer;
use kube::{Api, Client};

use kube_derive::CustomResource;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug)]
#[kube(group = "slinky.dev", version = "v1", namespaced)]
pub struct SimplePodSpec {
    image: String,
}

// TODO: Add status?

#[no_mangle]
pub extern "C" fn run() {
    let client = Client::default();

    let foos: Api<SimplePod> = Api::namespaced(client.clone(), "default");
    let inform = Informer::new(foos).params(ListParams::default().timeout(1));

    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");

    loop {
        let events = inform.poll().expect("Poll error");

        for e in events {
            match e {
                Ok(WatchEvent::Added(o)) | Ok(WatchEvent::Modified(o)) => {
                    reconcile_pod(&pods, &o.name(), &o.spec.image).expect("Reconcile error");
                }
                Ok(WatchEvent::Error(e)) => println!("Error event: {:?}", e),
                Err(e) => println!("Error event: {:?}", e),
                _ => {}
            }
        }
    }
}

fn reconcile_pod(pods: &Api<Pod>, name: &str, image: &str) -> Result<Pod, kube::Error> {
    match pods.get(&name) {
        Ok(mut existing) => {
            let existing_image = existing
                .spec
                .as_ref()
                .map(|spec| spec.containers[0].image.as_ref())
                .flatten()
                .expect("Malformed PodSpec, no image present");
            if existing_image == image {
                println!("Image is equal, doing nothing");
                Ok(existing)
            } else {
                let mut spec = existing.spec.unwrap();
                spec.containers[0].image = Some(image.to_string());
                existing.spec = Some(spec);
                println!("Replacing pod");
                pods.replace(&existing.name(), &PostParams::default(), &existing)
            }
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            println!("Creating pod");
            pods.create(&PostParams::default(), &pod(name, image))
        }
        e => e,
    }
}

fn pod(name: &str, image: &str) -> Pod {
    // TODO: Add ownerRef for deletion handling.
    Pod {
        metadata: Some(ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        }),
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
