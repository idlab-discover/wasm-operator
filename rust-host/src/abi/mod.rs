use crate::kube_watch::WatchCommand;
use serde::{Deserialize, Serialize};

use tokio::sync::mpsc::UnboundedSender;
use wasmer_runtime::{ImportObject, Instance};

#[cfg(feature = "abi-rust-v1alpha1")]
mod rust_v1alpha1;

pub struct AbiConfig {
    // Http proxy config
    pub cluster_url: url::Url,
    pub http_client: reqwest::Client,

    // Watch config
    pub watch_command_sender: UnboundedSender<WatchCommand>,
}

pub trait Abi {
    fn generate_imports(&self, controller_name: &str, abi_config: AbiConfig) -> ImportObject;
    fn start_controller(&self, instance: &Instance) -> anyhow::Result<()>;
    fn on_event(&self, instance: &Instance, event_id: u64, event: Vec<u8>) -> anyhow::Result<()>;
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
