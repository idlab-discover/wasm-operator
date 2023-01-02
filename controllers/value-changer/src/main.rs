use kube::{Client, api::{Api,  PostParams} };
use tokio::time;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use chrono::{Local, Utc};
use snafu::Snafu;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use kube_derive::CustomResource;

const NRITERATIONS: i32 = 20;
const TIMEINTERVAL:u64 = 2;
const KUBESECRET:&str = "varsecret";


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


#[tokio::main]
async fn main() {
    let _ = main_async().await;
}
// TODO maybe use a csv file with  change times to simulate a real operator instead of constant interval..., once date is collected

async fn main_async()  {
    println!("Changing secret every {TIMEINTERVAL} seconds");

    let client = Client::try_default().await;
    let clientunwrapped;
    match client {
        Ok(c)=> {clientunwrapped = c},
        Err(e) => {panic!("couldn't launch client {e}")}
        
    }
    
    let secrets: Api<TestResource> = Api::namespaced(clientunwrapped, "default");
    let mut interval = time::interval(time::Duration::from_secs(TIMEINTERVAL));
    for _i in 0..NRITERATIONS {
        interval.tick().await;
        match change_secret(&secrets).await {
            Ok(_) =>{},
            Err(e) => println!("error in loop {:?}",e)

        }
    }
}




async fn change_secret(secrets: &Api<TestResource>) -> Result<String, Error> {
    let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));


    match secrets.get(KUBESECRET).await {
        Ok(mut existing) => {

            existing.spec.nonce +=1;
            secrets.replace(KUBESECRET, &PostParams::default(), &existing).await?;
            println!("{:?}    changed secret {:?} to {:?}", now_timestamp.0.to_string(),existing.spec.nonce -1,existing.spec.nonce );
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            let index: i64 = 1;
            secrets
                .create(
                    &PostParams::default(),
                    &test_resource(&KUBESECRET, &index, now_timestamp),
                )
                .await?;
        }

        Err(e) => panic!("{}", e),
    }

    Ok("Secret data here ideally!".to_string())

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