use anyhow::{Context, Result};
use regex::Regex;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

use crate::config::InstallConfig;

pub fn setup_efi_boot_entry(config: &InstallConfig) -> Result<()> {
    let efibootmgr = config.efi_boot_mgr_path.join("bin/efibootmgr");

    // Get current EFI boot entries
    let output = Command::new(&efibootmgr)
        .output()
        .context("Failed to run efibootmgr")?;

    if !output.status.success() {
        anyhow::bail!(
            "efibootmgr failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let efibootmgr_output = String::from_utf8(output.stdout)?;

    // Find existing rEFInd entry
    let entry_regex = Regex::new(r"Boot([0-9a-fA-F]{4})\*? rEFInd")?;
    let existing_entry = entry_regex
        .captures(&efibootmgr_output)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    // Find EFI partition
    let efi_partition = find_mounted_device(&config.efi_mount_point)?;
    let efi_disk = find_disk_device(&efi_partition)?;

    // Determine boot file based on architecture
    let boot_file = match config.host_architecture.as_str() {
        arch if arch.starts_with("x86_64") => "BOOTX64.EFI",
        arch if arch.starts_with("i686") => "BOOTIA32.EFI",
        arch if arch.starts_with("aarch64") => "BOOTAA64.EFI",
        arch => anyhow::bail!("Unsupported architecture: {}", arch),
    };

    let efi_path = format!("\\efi\\refind\\{}", boot_file);
    let partition_num = extract_partition_number(&efi_partition, &efi_disk)?;

    if let Some(entry_id) = existing_entry {
        // Update existing entry
        let boot_order_regex = Regex::new(r"BootOrder: ((?:[0-9a-fA-F]{4},?)*)")?;
        let boot_order = boot_order_regex
            .captures(&efibootmgr_output)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");

        // Delete old entry
        Command::new(&efibootmgr)
            .args(["-b", &entry_id, "-B"])
            .output()
            .context("Failed to delete old EFI entry")?;

        // Create new entry with same ID and preserve boot order
        let status = Command::new(&efibootmgr)
            .args([
                "-c",
                "-b",
                &entry_id,
                "-d",
                &efi_disk,
                "-p",
                &partition_num,
                "-l",
                &efi_path,
                "-L",
                "rEFInd",
                "-o",
                boot_order,
            ])
            .status()
            .context("Failed to create EFI entry")?;

        if !status.success() {
            anyhow::bail!("efibootmgr failed to create boot entry");
        }
    } else {
        // Create new entry
        let status = Command::new(&efibootmgr)
            .args([
                "-c",
                "-d",
                &efi_disk,
                "-p",
                &partition_num,
                "-l",
                &efi_path,
                "-L",
                "rEFInd",
            ])
            .status()
            .context("Failed to create EFI entry")?;

        if !status.success() {
            anyhow::bail!("efibootmgr failed to create boot entry");
        }
    }

    Ok(())
}

fn find_mounted_device(path: &Path) -> Result<String> {
    let path = std::fs::canonicalize(path)?;
    let mut current = path.as_path();

    // Walk up until we find a mount point
    while !is_mount_point(current)? {
        current = current
            .parent()
            .context("Reached filesystem root without finding mount point")?;
    }

    // Find the device for this mount point
    let mounts = std::fs::read_to_string("/proc/mounts")?;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == current.to_str().unwrap() {
            return Ok(parts[0].to_string());
        }
    }

    anyhow::bail!("Could not find device for mount point: {:?}", current)
}

fn is_mount_point(path: &Path) -> Result<bool> {
    let parent = match path.parent() {
        Some(p) => p,
        None => return Ok(true), // Root is always a mount point
    };

    let path_metadata = std::fs::metadata(path)?;
    let parent_metadata = std::fs::metadata(parent)?;

    // Different devices = mount point
    Ok(path_metadata.dev() != parent_metadata.dev())
}

fn find_disk_device(partition: &str) -> Result<String> {
    // /dev/nvme0n1p1 -> /dev/nvme0n1
    // /dev/sda1 -> /dev/sda

    let partition = std::fs::canonicalize(partition)?;
    let part_name = partition
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid partition path")?;

    // Handle nvme devices (nvme0n1p1 -> nvme0n1)
    if part_name.contains("nvme") {
        let re = Regex::new(r"^(nvme\d+n\d+)p\d+$")?;
        if let Some(caps) = re.captures(part_name) {
            return Ok(format!("/dev/{}", &caps[1]));
        }
    }

    // Handle sd devices (sda1 -> sda) and other devices
    let re = Regex::new(r"^([a-z]+)\d+$")?;
    if let Some(caps) = re.captures(part_name) {
        return Ok(format!("/dev/{}", &caps[1]));
    }

    anyhow::bail!(
        "Could not determine disk device for partition: {}",
        partition.display()
    )
}

fn extract_partition_number(partition: &str, disk: &str) -> Result<String> {
    // /dev/sda1 with disk /dev/sda -> "1"
    // /dev/nvme0n1p1 with disk /dev/nvme0n1 -> "1"

    let part = partition.trim_start_matches(disk);
    let part = part.trim_start_matches('p'); // For nvme devices

    Ok(part.to_string())
}
