use super::{WatchKey};
use futures::{StreamExt, TryStreamExt};
use std::collections::HashMap;
use std::convert::TryInto;
use tokio::sync::mpsc::{Sender, UnboundedReceiver};
use crate::abi::dispatcher::{AsyncResult, AsyncType};
use crate::abi::commands::AbiCommand;

pub struct Watchers {
    cache: HashMap<WatchKey, Vec<(String, u64)>>,
    internal_dispatch_tx: Sender<(WatchKey, Vec<u8>)>,
}

impl Watchers {
    fn register_watch(&mut self, command: AbiCommand<WatchKey>, kube_client: kube::Client) {
        if self.cache.contains_key(&command.value) {
            debug!(
                "Found a watch already started for '{:?}', registering new receiver ({}, {})",
                &command.value, &command.controller_name, &command.async_request_id
            );
            self.cache
                .get_mut(&command.value)
                .unwrap()
                .push((command.controller_name, command.async_request_id))
        } else {
            debug!(
                "Starting a new watch for '{:?}', registering new receiver ({}, {})",
                &command.value, &command.controller_name, &command.async_request_id
            );
            let (watch_key, controller_name, async_request_id) =
                (command.value, command.controller_name, command.async_request_id);
            self.cache
                .insert(watch_key.clone(), vec![(controller_name, async_request_id)]);

            let mut internal_dispatch_tx = self.internal_dispatch_tx.clone();

            tokio::spawn(async move {
                let key = watch_key.clone();

                let mut stream = kube_client
                    .request_events(key.clone().try_into().expect("Watch request"))
                    .await
                    .expect("watch events stream")
                    .boxed();
                while let Some(event) = stream.try_next().await.expect("watch event") {
                    internal_dispatch_tx.send((key.clone(), event)).await.unwrap();
                }
            });
        }
    }

    pub async fn dispatch_event(
        &self,
        key: WatchKey,
        event: Vec<u8>,
        mut tx: Sender<AsyncResult>,
    ) -> anyhow::Result<()> {
        let subs = self.cache.get(&key).ok_or(anyhow::anyhow!(
            "Cannot find the subscribers list for key {:?}",
            &key
        ))?;

        for (controller_name, id) in subs {
            let watch_event = AsyncResult {
                controller_name: controller_name.clone(),
                async_request_id: id.clone(),
                async_type: AsyncType::Stream,
                value: Some(event.clone()),
            };

            debug!("Dispatching watch event with id '{}' for controller '{}'", controller_name, id);

            tx.send(watch_event)
            .await?;
        }
        Ok(())
    }

    pub async fn start(
        mut rx: UnboundedReceiver<AbiCommand<WatchKey>>,
        tx: Sender<AsyncResult>,
        kube_client: kube::Client,
    ) -> anyhow::Result<()> {
        info!("Starting the watch commands listener loop");

        let (internal_tx, mut internal_rx) = tokio::sync::mpsc::channel(10);
        let mut watchers = Watchers {
            cache: HashMap::new(),
            internal_dispatch_tx: internal_tx,
        };

        loop {
            tokio::select! {
                Some(command) = rx.recv() =>
                    watchers.register_watch(command, kube_client.clone()),
                Some((watch_key, event_payload)) = internal_rx.recv() =>
                    watchers.dispatch_event(watch_key, event_payload, tx.clone()).await?,
                else => break,
            }
        }
        Ok(())
    }
}
