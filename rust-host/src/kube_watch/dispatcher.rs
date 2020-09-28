use crate::kube_watch::WatchEvent;
use crate::modules::ControllerModule;
use std::collections::HashMap;
use tokio::sync::mpsc::Receiver;

pub struct Dispatcher {}

impl Dispatcher {

    pub async fn start(map: HashMap<String, ControllerModule>, mut rx: Receiver<WatchEvent>) -> anyhow::Result<()> {
        let mut map = map;

        info!("Starting the watch events listener loop");

        while let Some(event) = rx.recv().await {
            if let Some(controller) = map.remove(&event.controller_name) {
                map.insert(event.controller_name.clone(), tokio::runtime::Handle::current().spawn_blocking(move || {
                    controller.on_event(event.watch_id, event.event).unwrap();
                    controller
                }).await?);
            } else {
                return Err(anyhow::anyhow!(
                    "Cannot find controller for event {:?}",
                    event
                ));
            }
        }
        Ok(())
    }
}
