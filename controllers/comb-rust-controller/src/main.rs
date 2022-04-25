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
use tracing::info;
use tracing::debug;
use futures::stream::FuturesUnordered;

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
}

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

    let nr_operators = env::var("NR_OPERATORS").unwrap_or("1".to_string()).parse::<i32>().unwrap_or(1);

    let mut futures = FuturesUnordered::new();
    for i in 0..nr_operators {
        let in_namespace = format!("native-rust-comb{}", i);
        let in_resources: Api<TestResource> = Api::namespaced(client.clone(), &in_namespace);

        let future = Controller::new(
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
                client: client.clone(),
                out_namespace: format!("native-rust-comb{}", i+1),
            }),
        )
        .for_each(|res| async move {
            match res {
                Ok((obj, _)) => debug!("Reconciled {:?}", obj),
                Err(e) => debug!("Reconcile error: {:?}", e),
            }
        });

        futures.push(future);
    }

    while let Some(()) = futures.next().await {}
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
                println!("nonce > current nonce, resetting resource");
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
