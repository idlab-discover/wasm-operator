use async_trait::async_trait;
use futures::stream::{StreamExt, TryStreamExt};
use mongodb::bson::{doc, Document};
use mongodb::{Client, Database};
use rand::distributions::{Alphanumeric, DistString};
use schemars::gen;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Instant;
use tokio::task;
use tokio::time;

#[derive(Debug, Serialize, Deserialize)]
struct Book {
    title: String,
    author: String,
}

#[tokio::main]
async fn main() {
    let c = get_connection().await.unwrap();

    generate_books(&c).await.unwrap();

    let mut totalReads = 0;

    let now = Instant::now();
    let mut interval = time::interval(time::Duration::from_secs(1));
    for _i in 0..100000 {
        interval.tick().await;

        let nr = read_ops(&c).await;
        totalReads += nr;
        let elapsed = now.elapsed();
        println!(
            "{:?} reads per sec",
            totalReads as f64 / elapsed.as_secs_f64()
        );
    }

    println!("Hello, world!");
}

async fn read_ops(db: &Database) -> i32 {
    let mut i = 0;
    let conn = db.collection("test");

    if let Ok(mut cursor) = conn.find(None, None).await {
        while let Some(result) = cursor.next().await {
            match result {
                Ok(_) => {
                    i += 1;
                }
                Err(_) => print!("err while getting next doc"),
            }
        }
    }

    i
}

async fn get_connection() -> Result<Database, mongodb::error::Error> {
    let client = Client::with_uri_str(
        &env::var("MONGO_URI").unwrap_or("mongodb://root:password@localhost:27017".to_string()),
    )
    .await?;

    let db = client.database(&env::var("MONGO_DBNAME").unwrap_or("test".to_string()));

    db.run_command(doc! {"ping": 1}, None).await?;
    println!("Connected successfully.");
    Ok(db)
}

fn generate_book() -> Document {
    let title = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    let author = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);

    return doc! {"title": title, "author":author};
}

async fn generate_books(db: &Database) -> Result<(), mongodb::error::Error> {
    let conn = db.collection("test");

    let mut books = vec![];
    for _ in 0..1000 {
        books.push(generate_book())
    }

    conn.insert_many(books, None).await?;
    Ok(())
}
