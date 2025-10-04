use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BootSpec {
    pub system: String,
    pub init: PathBuf,
    pub kernel: PathBuf,
    pub kernel_params: Vec<String>,
    pub label: String,
    pub toplevel: PathBuf,
    #[serde(default)]
    pub initrd: Option<PathBuf>,
    #[serde(default)]
    pub initrd_secrets: Option<PathBuf>,
    #[serde(default)]
    pub specialisations: HashMap<String, Box<BootSpec>>,
}

#[derive(Debug, Deserialize)]
struct BootJson {
    #[serde(rename = "org.nixos.bootspec.v1")]
    bootspec: BootSpecV1,
    #[serde(rename = "org.nixos.specialisation.v1", default)]
    specialisation: HashMap<String, BootJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootSpecV1 {
    system: String,
    init: PathBuf,
    kernel: PathBuf,
    kernel_params: Vec<String>,
    label: String,
    toplevel: PathBuf,
    #[serde(default)]
    initrd: Option<PathBuf>,
    #[serde(default)]
    initrd_secrets: Option<PathBuf>,
}

impl BootSpec {
    pub fn load(system_path: &Path) -> Result<Self> {
        let boot_json_path = system_path.join("boot.json");
        let content = std::fs::read_to_string(&boot_json_path)
            .with_context(|| format!("Failed to read boot.json at {:?}", boot_json_path))?;

        let boot_json: BootJson =
            serde_json::from_str(&content).context("Failed to parse boot.json")?;

        Ok(Self::from_boot_json(boot_json))
    }

    fn from_boot_json(boot_json: BootJson) -> Self {
        let specialisations = boot_json
            .specialisation
            .into_iter()
            .map(|(k, v)| (k, Box::new(Self::from_boot_json(v))))
            .collect();

        Self {
            system: boot_json.bootspec.system,
            init: boot_json.bootspec.init,
            kernel: boot_json.bootspec.kernel,
            kernel_params: boot_json.bootspec.kernel_params,
            label: boot_json.bootspec.label,
            toplevel: boot_json.bootspec.toplevel,
            initrd: boot_json.bootspec.initrd,
            initrd_secrets: boot_json.bootspec.initrd_secrets,
            specialisations,
        }
    }
}
