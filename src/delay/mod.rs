use crate::abi::commands::AbiCommand;
use crate::abi::dispatcher::{AsyncResult, AsyncType};
use log::debug;
use tokio::sync::mpsc::{Sender, UnboundedReceiver};

use std::time::Duration;

pub async fn start_delay_executor(
    mut rx: UnboundedReceiver<AbiCommand<Duration>>,
    otx: Sender<AsyncResult>,
) -> anyhow::Result<()> {
    while let Some(delay_command) = rx.recv().await {
        let tx = otx.clone();
        tokio::spawn(async move {
            debug!(
                "Received delay command from '{}' with id {}: {:?}",
                &delay_command.controller_name, &delay_command.async_request_id, delay_command.value
            );

            tokio::time::sleep(delay_command.value.into()).await;

            tx.clone()
                .send(AsyncResult {
                    async_request_id: delay_command.async_request_id,
                    controller_name: delay_command.controller_name,
                    value: None,
                    async_type: AsyncType::Future,
                })
                .await
                .expect("Send error");
        });
    }

    Ok(())
}
