use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct FileTracker {
    files: HashMap<PathBuf, bool>,
}

impl FileTracker {
    pub fn new(base_dir: &Path) -> Result<Self> {
        let mut files = HashMap::new();

        if base_dir.exists() {
            for entry in WalkDir::new(base_dir) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    files.insert(entry.path().to_path_buf(), false);
                }
            }
        }

        Ok(Self { files })
    }

    pub fn mark_used(&mut self, path: &Path) {
        self.files.insert(path.to_path_buf(), true);
    }

    pub fn cleanup(&self) -> Result<()> {
        for (path, used) in &self.files {
            if !used && path.exists() {
                std::fs::remove_file(path)
                    .with_context(|| format!("Failed to remove unused file: {:?}", path))?;
            }
        }
        Ok(())
    }
}

/// Copy file atomically (write to .tmp then rename)
pub fn copy_atomic(source: &Path, dest: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    let temp_dest = dest.with_extension("tmp");

    // Copy to temporary file
    std::fs::copy(source, &temp_dest)
        .with_context(|| format!("Failed to copy {:?} to {:?}", source, temp_dest))?;

    // Atomic rename
    std::fs::rename(&temp_dest, dest)
        .with_context(|| format!("Failed to rename {:?} to {:?}", temp_dest, dest))?;

    Ok(())
}

/// Write data atomically (write to .tmp then rename)
pub fn write_atomic(dest: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp_dest = dest.with_extension("tmp");

    let mut file = std::fs::File::create(&temp_dest)
        .with_context(|| format!("Failed to create temp file: {:?}", temp_dest))?;

    file.write_all(data)?;
    file.sync_all()?;
    drop(file);

    std::fs::rename(&temp_dest, dest)
        .with_context(|| format!("Failed to rename {:?} to {:?}", temp_dest, dest))?;

    Ok(())
}

/// Sync filesystem using syncfs()
pub fn sync_filesystem(mount_point: &Path) -> Result<()> {
    let file = std::fs::File::open(mount_point)
        .with_context(|| format!("Failed to open mount point: {:?}", mount_point))?;

    file.sync_all().map_err(|error| {
        anyhow::anyhow!("Failed to sync filesystem at {:?}: {}", mount_point, error)
    })?;

    Ok(())
}
