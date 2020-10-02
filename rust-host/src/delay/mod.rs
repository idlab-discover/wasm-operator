use tokio::sync::mpsc::{Sender, UnboundedReceiver};
use crate::abi::commands::AbiCommand;
use crate::abi::dispatcher::{AsyncType, AsyncResult};
use std::convert::{TryFrom, TryInto};

use futures::{StreamExt};
use std::time::Duration;

pub async fn start_delay_executor(
    rx: UnboundedReceiver<AbiCommand<Duration>>,
    tx: Sender<AsyncResult>,
) -> anyhow::Result<()> {
    rx.for_each_concurrent(10, |delay_command| async {
        debug!(
            "Received delay command from '{}' with id {}: {:?}",
            &delay_command.controller_name, &delay_command.async_request_id, delay_command.value
        );

        tokio::time::delay_for(delay_command.value.into()).await;

        tx.clone().send(AsyncResult {
            async_request_id: delay_command.async_request_id,
            controller_name: delay_command.controller_name,
            value: None,
            async_type: AsyncType::Future
        }).await.expect("Send error");

    }).await;
    Ok(())
}