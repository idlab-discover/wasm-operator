use crate::modules::ControllerModule;
use std::collections::HashMap;
use tokio::sync::mpsc::Receiver;

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum AsyncType {
    Future,
    Stream,
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct AsyncResult {
    pub controller_name: String,
    pub async_request_id: u64,
    pub async_type: AsyncType,
    pub value: Option<Vec<u8>>,
}

pub struct AsyncResultDispatcher {}

impl AsyncResultDispatcher {
    pub async fn start(map: HashMap<String, ControllerModule>, mut rx: Receiver<AsyncResult>) -> anyhow::Result<()> {
        let mut map = map;

        info!("Starting the watch events listener loop");

        while let Some(async_result) = rx.recv().await {
            if let Some(controller) = map.remove(&async_result.controller_name) {
                controller.wakeup(async_result.async_request_id, async_result.async_type, async_result.value)?;
                map.insert(async_result.controller_name.clone(), controller);
            } else {
                return Err(anyhow::anyhow!(
                    "Cannot find controller for event {:?}",
                    async_result
                ));
            }
        }
        Ok(())
    }
}
