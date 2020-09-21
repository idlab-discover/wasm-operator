use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{Container, Pod, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::api::{ListParams, Meta, PostParams, WatchEvent};
use kube::runtime::Informer;
use kube::{Api, Client};

use kube_derive::CustomResource;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug)]
#[kube(group = "cache.example.com", version = "v1alpha1", namespaced)]
#[kube(status = "MemcachedStatus")]
pub struct MemcachedSpec {
    size: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct MemcachedStatus {
    nodes: Vec<String>,
}

#[no_mangle]
pub extern "C" fn run() {
    let client = Client::default();

    let mems: Api<Memcached> = Api::namespaced(client.clone(), "default");
    let inform = Informer::new(mems).params(ListParams::default());

    inform.poll(move |e| {
        match e {
            WatchEvent::Added(mut o) | WatchEvent::Modified(mut o) => {
                reconcile(&client, &mut o).expect("Reconcile error");
            }
            WatchEvent::Error(e) => println!("Error event: {:?}", e),
            e => println!("Not handled event: {:?}", e)
        }
    });
}

fn reconcile(client: &Client, mem: &mut Memcached) -> Result<(), kube::Error> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), "default");
    let mems: Api<Memcached> = Api::namespaced(client.clone(), "default");
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), "default");

    match deployments.get(&mem.name()) {
        Ok(mut existing) => {
            let existing_scale = existing
                .spec
                .as_ref()
                .map(|spec| spec.replicas.as_ref())
                .flatten();
            if existing_scale == Some(&mem.spec.size) {
                println!("Scale is already correct");
                Ok(existing)
            } else {
                let mut spec = existing.spec.unwrap();
                spec.replicas = Some(mem.spec.size);
                existing.spec = Some(spec);
                println!("Replacing deployment");
                deployments.replace(&existing.name(), &PostParams::default(), &existing)
            }
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            println!("Creating deployment");
            deployments.create(&PostParams::default(), &memcached_deployment(mem))
        }
        e => e,
    }
    .and_then(|_| pods.list(&ListParams::default().labels(&format!("memcached_cr={}", mem.name()))))
    .map(|mempods| {
        let pod_names: Vec<String> = mempods.iter().map(Pod::name).collect();

        mem.status = Some(MemcachedStatus { nodes: pod_names });
        mems.replace_status(
            &mem.name(),
            &PostParams::default(),
            serde_json::to_vec(&mem).unwrap(),
        )
    })
    .map(|_| ())
}

fn memcached_deployment(mem: &Memcached) -> Deployment {
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("memcached_cr".to_string(), mem.name());
    labels.insert("app".to_string(), "memcached".to_string());

    Deployment {
        metadata: Some(ObjectMeta {
            name: Some(mem.name()),
            ..Default::default()
        }),
        spec: Some(DeploymentSpec {
            replicas: Some(mem.spec.size),
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "memcached".to_string(),
                        image: Some("memcached:1.4.36-alpine".to_string()),
                        command: Some(vec![
                            "memcached".to_string(),
                            "-m=64".to_string(),
                            "-o".to_string(),
                            "modern".to_string(),
                            "-v".to_string(),
                        ]),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }),
        status: None,
    }
}
