use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct ControllerModuleMetadata {
    pub name: String,
    pub envs: Vec<(String, String)>,
    pub args: Vec<String>,
}

impl ControllerModuleMetadata {
    /// Load modules metadata and module bytes from a specific directory
    pub fn load_modules_from_dir(
        dir: PathBuf,
    ) -> Result<Vec<(PathBuf, ControllerModuleMetadata, std::path::PathBuf)>> {
        fs::read_dir(dir)?
            .flat_map(|dir_entry| {
                match dir_entry {
                    Ok(e) => {
                        if e.path().extension() == Some(OsStr::new("yaml")) {
                            Some(e.path())
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
                .into_iter()
            })
            .map(|e| {
                let mm: ControllerModuleMetadata = serde_yaml::from_reader(File::open(&e)?)?;
                let wasm_file_name = e.with_extension("wasm");
                Ok((e, mm, wasm_file_name))
            })
            .collect()
    }
}
