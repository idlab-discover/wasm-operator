use tokio::sync::mpsc::{Sender, UnboundedReceiver};
use crate::abi::commands::AbiCommand;
use crate::abi::dispatcher::{AsyncType, AsyncResult};
use std::convert::{TryFrom, TryInto};
use http::HeaderMap;

use crate::abi::rust_v1alpha1::HttpResponse;
use futures::{StreamExt};
use std::time::Duration;

pub async fn start_delay_executor(
    rx: UnboundedReceiver<AbiCommand<Duration>>,
    tx: Sender<AsyncResult>,
) -> anyhow::Result<()> {
    rx.for_each_concurrent(10, |mut duration| async {
        tokio::time::delay_for(duration.value.into()).await;

        tx.clone().send(AsyncResult {
            async_request_id: duration.async_request_id,
            controller_name: duration.controller_name,
            value: None,
            async_type: AsyncType::Future
        }).await.expect("Send error");

    }).await;
    Ok(())
}