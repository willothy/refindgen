use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallConfig {
    pub nix_path: PathBuf,
    pub refind_path: PathBuf,
    pub efi_mount_point: PathBuf,
    pub efi_boot_mgr_path: PathBuf,
    pub can_touch_efi_variables: bool,
    pub efi_removable: bool,
    pub timeout: u32,
    pub max_generations: usize,
    pub extra_config: String,
    pub host_architecture: String,
    pub additional_files: HashMap<String, PathBuf>,
    pub luks_devices: Vec<(String, String)>,
}

impl InstallConfig {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read config file")?;

        serde_json::from_str(&content).context("Failed to parse config JSON")
    }
}
