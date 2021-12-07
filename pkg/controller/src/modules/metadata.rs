use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ControllerModuleMetadata {
    pub name: String,
    pub wasm: PathBuf,
    #[serde(default)]
    pub env: Vec<EnvironmentVariable>,
    #[serde(default)]
    pub args: Vec<String>,
}

impl ControllerModuleMetadata {
    /// Load modules metadata and module bytes from a specific directory
    pub fn load_modules_from_dir(dir: PathBuf) -> Result<Vec<ControllerModuleMetadata>> {
        let wasm_config = std::fs::read_to_string(dir.join("wasm_config.yaml"))?;

        wasm_config
            .split("\n---")
            .filter_map(|yaml_doc| match serde_yaml::from_str(yaml_doc) {
                Err(err) if err.to_string().contains("EOF while parsing a value") => None,
                result => Some(result.map_err(|e| anyhow::anyhow!("Failed to parse module metadata: {}", e))),
            })
            .collect()
    }
}
