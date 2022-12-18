use kube::{Client, api::{Api, Patch,ResourceExt, ListParams, PostParams, PatchParams,DeleteParams}};
use k8s_openapi::api::core::v1::Secret;
use bytes::Bytes;
use std::{collections::BTreeMap, borrow::Borrow, vec, str::from_utf8};
use k8s_openapi::ByteString;
use std::str;
use base64::{encode, decode};
use tokio::time;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::MicroTime;
use chrono::{Local, Utc};


#[tokio::main]
async fn main() {
    let res = main_async().await;
}


async fn main_async()  {
    let client = Client::try_default().await;
    let clientunwrapped;
    match client {
        Ok(c)=> {clientunwrapped = c},
        Err(e) => {panic!("couldn't launch client {e}")}
        
    }

    let secrets: Api<Secret> = Api::namespaced(clientunwrapped, "default");

    let mut interval = time::interval(time::Duration::from_secs(2));
    for _i in 0..20 {
        interval.tick().await;
        match change_secret(&secrets).await {
            Ok(_) =>{},
            Err(e) => println!("error in loop {:?}",e)

        }
    }
}




async fn change_secret(secrets: &Api<Secret>) -> Result<String, kube::Error> {
    let now_timestamp = MicroTime(Local::now().with_timezone(&Utc));


    match secrets.get("varsecret").await {
        Ok(mut secret) => {

            let mut secret_data: BTreeMap<String, ByteString>  = secret.data.unwrap();
            let secretvar = &secret_data["var"];

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
            secret_data.insert("var".to_string(), bytes);

            secret.data =  Some(secret_data);
            secrets.replace("varsecret", &PostParams::default(), &secret).await?;
            println!("{:?}    chamged secret {:?} to {:?}", now_timestamp.0.to_string(),original_nr,nr);
        }


        Err(e) => panic!("{}", e),
    }

    Ok("Secret data here ideally!".to_string())

}
