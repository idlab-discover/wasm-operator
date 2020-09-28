use crate::abi::AbiVersion;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct ControllerModuleMetadata {
    pub name: String,
    pub abi: AbiVersion,
}

impl ControllerModuleMetadata {
    /// Load modules metadata and module bytes from a specific directory
    pub fn load_modules_from_dir(
        dir: PathBuf,
    ) -> Result<Vec<(PathBuf, ControllerModuleMetadata, Vec<u8>)>> {
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
                let mut v: Vec<u8> = Vec::new();
                let wasm_file_name = e.with_extension("wasm");
                File::open(wasm_file_name)?.read_to_end(&mut v)?;
                Ok((e, mm, v))
            })
            .collect()
    }
}
