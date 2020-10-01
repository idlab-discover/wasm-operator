use crate::kube_watch::{WatchKey};
use serde::{Deserialize, Serialize};

use tokio::sync::mpsc::UnboundedSender;
use wasmer_runtime::{ImportObject, Instance};
use dispatcher::AsyncType;
use std::fmt::Debug;
use crate::abi::commands::AbiCommand;
use std::time::Duration;

#[cfg(feature = "abi-rust-v1alpha1")]
pub(crate) mod rust_v1alpha1;

pub mod dispatcher;
pub mod commands;

pub struct AbiConfig {
    pub http_command_sender: UnboundedSender<AbiCommand<http::Request<Vec<u8>>>>,
    pub delay_command_sender: UnboundedSender<AbiCommand<Duration>>,
    pub watch_command_sender: UnboundedSender<AbiCommand<WatchKey>>,
}

pub trait Abi {
    fn generate_imports(&self, controller_name: &str, abi_config: AbiConfig) -> ImportObject;
    fn start_controller(&self, instance: &Instance) -> anyhow::Result<()>;
    fn wakeup(&self, instance: &Instance, async_request_id: u64, async_type: AsyncType, value: Option<Vec<u8>>) -> anyhow::Result<()>;
    fn allocate(&self, instance: &Instance, allocation_size: u32) -> anyhow::Result<u32>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AbiVersion {
    #[cfg(feature = "abi-rust-v1alpha1")]
    #[serde(alias = "rust_v1alpha1")]
    RustV1Alpha1,
}

impl AbiVersion {
    pub fn get_abi(&self) -> impl Abi {
        match self {
            #[cfg(feature = "abi-rust-v1alpha1")]
            AbiVersion::RustV1Alpha1 => rust_v1alpha1::Abi {},
        }
    }
}
