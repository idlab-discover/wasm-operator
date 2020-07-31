use crate::abi::AbiVersion;
use std::path::PathBuf;
use std::fs;
use std::fs::File;
use serde::{Serialize, Deserialize};
use std::ffi::OsStr;
use anyhow::Result;
use std::io::Read;

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleMetadata {
    pub name: String,
    pub abi: AbiVersion,
}

pub fn load_modules_from_dir(dir: PathBuf) -> Result<Vec<(PathBuf, ModuleMetadata, Vec<u8>)>> {
    fs::read_dir(dir)?
        .flat_map(|dir_entry|
            match dir_entry {
                Ok(e) => {
                    if e.path().extension() == Some(OsStr::new("yaml")) {
                        Some(e.path())
                    } else {
                        None
                    }
                },
                Err(_) => None
            }.into_iter()
        )
        .map(|e| {
            let mm: ModuleMetadata = serde_yaml::from_reader(File::open(&e)?)?;
            let mut v: Vec<u8> = Vec::new();
            let wasm_file_name = e.with_extension("wasm");
            File::open(wasm_file_name)?.read_to_end(&mut v)?;
            Ok((e, mm, v))
        })
        .collect()
}
