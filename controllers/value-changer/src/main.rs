use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{
    api::{Api, PostParams},
    Client,
};
use tokio::time::sleep;

use chrono::{Duration, Local, NaiveDateTime, Utc};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use kube_derive::CustomResource;

const KUBESECRET: &str = "varsecret";

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

async fn main_async() {
    println!("Changing secret every time from traces.csv");

    let wait_intervals = read_traces();

    let client = Client::try_default().await;
    let clientunwrapped;
    match client {
        Ok(c) => clientunwrapped = c,
        Err(e) => {
            panic!("couldn't launch client {e}")
        }
    }
    let secrets: Api<TestResource> = Api::namespaced(clientunwrapped, "default");

    for duration in wait_intervals {
        sleep(duration.to_std().expect("can't convert time to std")).await;
        match change_secret(&secrets).await {
            Ok(_) => {}
            Err(e) => println!("error in loop {:?}", e),
        }
    }
}

async fn change_secret(secrets: &Api<TestResource>) -> Result<String, Error> {
    let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));

    match secrets.get(KUBESECRET).await {
        Ok(mut existing) => {
            existing.spec.nonce += 1;
            secrets
                .replace(KUBESECRET, &PostParams::default(), &existing)
                .await?;
            println!(
                "DEBUG {:?}    doing changed secret {:?}",
                now_timestamp.0.to_string(),
                existing.spec.nonce
            );
        }
        Err(kube::Error::Api(ae)) if ae.code == 404 => {
            let index: i64 = 1;
            secrets
                .create(
                    &PostParams::default(),
                    &test_resource(KUBESECRET, &index, now_timestamp),
                )
                .await?;
            let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));
            println!(
                "DEBUG {:?}    doing changed secret {:?}",
                now_timestamp.0.to_string(),
                1
            );
        }

        Err(e) => panic!("{}", e),
    }
    Ok("ok".to_string())
}

// read the traces.csv file in the directory and convert time stamps into time intervals
fn read_traces() -> Vec<Duration> {
    let mut reader = csv::Reader::from_path("traces.csv").expect("can't read csv file");

    // let _header = reader.headers().expect("can't read headers");

    let mut parsed_dates = vec![];

    for date in reader.records() {
        let readdate = date.expect("msg");
        let record = readdate.get(0).expect("str");
        let parsed = NaiveDateTime::parse_from_str(record, "%Y-%m-%dT%H:%M:%S.%fZ")
            .expect("can't parse date");
        parsed_dates.push(parsed);
    }
    assert!(parsed_dates.len() > 1);

    let begin_date = *parsed_dates.first().expect("bigger than 1");
    let differences = parsed_dates
        .iter()
        .map(|x| x.signed_duration_since(begin_date))
        .collect::<Vec<_>>();
    // first previous duration will be 0
    let mut previous_duration = *differences.first().expect("bigger than 1");
    let mut interval_differences = vec![];
    // subtract previous time  from current time to get difference
    for i in differences {
        interval_differences.push(i - previous_duration);
        previous_duration = i;
    }

    interval_differences
}

fn test_resource(name: &str, nonce: &i64, start_timestamp: MicroTime) -> TestResource {
    TestResource {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        spec: TestResourceSpec {
            nonce: *nonce,
            updated_at: Some(start_timestamp),
        },
    }
}
