use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use kube::{
    api::{ListParams, PostParams},
    Api, Client, CustomResource, ResourceExt,
};
use kube_runtime::controller::{Action, Context, Controller};
use k8s_openapi::ByteString;
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
use std::{collections::BTreeMap};
use k8s_openapi::api::core::v1::Secret;
use std::str;
use base64::{ decode};


//dunno
//use kube_runtime::controller::Error;
//use tracing_subscriber::registry::Data;
use kube::Error;
use std::ops::Deref;
use futures::task::SpawnExt;



const NRITERATIONS: i32 = 20;
const TIMEINTERVAL:u64 = 2;
const KUBESECRET:&str = "varsecret";
const KUBESECRETVAR:&str = "var";



#[derive(CustomResource, Debug, Serialize, Deserialize, Default, Clone, JsonSchema)]
#[kube(
    kind = "TestResource",
    group = "amurant.io",
    version = "v1",
    namespaced // maybe secrets here ?
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
    //out_namespace: String,
    //huge_mem_alloc: Arc<Vec<u8>>,
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
    println!("main launched simple");

    let client = Client::try_default()
        .await
        .expect("could not create kube client");

    let in_resources: Api<TestResource> = Api::namespaced(client.clone(), "default");
    
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
        //    out_namespace,
         //   huge_mem_alloc,
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


async fn reconcile(
    in_test_resource: Arc<TestResource>,
    ctx: Context<Data>,
) -> Result<Action, Error> {

    println!("reconcile called");

    let client = ctx.get_ref().client.clone();
    let secrets: Api<Secret> = Api::namespaced(client, "default");



    let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));


    match secrets.get(KUBESECRET).await {
        Ok(mut secret) => {

            let mut secret_data: BTreeMap<String, ByteString>  = secret.data.unwrap();
            let secretvar = &secret_data[KUBESECRETVAR];

            let decoded = serde_json::to_string(&secretvar).unwrap();

            let mut chars = decoded.chars();
            chars.next();
            chars.next_back();
            let trimmed = chars.as_str();

            let decodedarr = decode(trimmed).unwrap();
            let decodedstr = String::from_utf8(decodedarr).unwrap();

            let mut nr : i32 = decodedstr.parse().unwrap();
            let original_nr = nr;
            nr +=1;
            let nrstr = nr.to_string();
            let bytesstr =nrstr.as_bytes();

            //let vec1 = vec![2];
            let bytes= ByteString(bytesstr.to_vec());
            secret_data.insert(KUBESECRETVAR.to_string(), bytes);

            secret.data =  Some(secret_data);
            secrets.replace(KUBESECRET, &PostParams::default(), &secret).await?;
            println!("{:?}    changed secret {:?} to {:?}", now_timestamp.0.to_string(),original_nr,nr);
        }


        Err(e) => panic!("{}", e),
    }





    Ok(Action::await_change())
}





