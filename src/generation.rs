use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{bootspec::BootSpec, config::InstallConfig, fs};

pub fn get_system_path(profile: &str, generation: Option<u64>, spec: Option<&str>) -> PathBuf {
    let profiles_dir = PathBuf::from("/nix/var/nix/profiles");

    let mut path = if profile == "system" {
        if let Some(g) = generation {
            profiles_dir.join(format!("system-{}-link", g))
        } else {
            profiles_dir.join("system")
        }
    } else {
        let basename = if let Some(g) = generation {
            format!("{}-{}-link", profile, g)
        } else {
            profile.to_string()
        };
        profiles_dir.join("system-profiles").join(basename)
    };

    if let Some(s) = spec {
        path = path.join("specialisation").join(s);
    }

    path
}

pub fn get_profiles() -> Result<Vec<String>> {
    let profiles_dir = PathBuf::from("/nix/var/nix/profiles/system-profiles");

    if !profiles_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&profiles_dir).context("Failed to read profiles directory")?;

    let mut profiles = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with("-link") {
            profiles.push(name);
        }
    }

    Ok(profiles)
}

pub fn get_generations(profile: &str, config: &InstallConfig) -> Result<Vec<u64>> {
    let nix_env = config.nix_path.join("bin/nix-env");
    let profile_path = get_system_path(profile, None, None);

    let output = Command::new("sudo")
        .arg(nix_env)
        .args([
            "--list-generations",
            "-p",
            profile_path.to_str().unwrap(),
            "--option",
            "build-users-group",
            "",
        ])
        .output()
        .context("Failed to run nix-env")?;

    if !output.status.success() {
        anyhow::bail!(
            "nix-env failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut generations: Vec<u64> = stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next().and_then(|s| s.parse().ok()))
        .collect();

    // Keep only the last N generations
    if generations.len() > config.max_generations {
        let count = generations.len();
        generations = generations
            .into_iter()
            .skip(count - config.max_generations)
            .collect();
    }

    Ok(generations)
}

pub fn generate_config_entry(
    profile: &str,
    generation: u64,
    group_name: &str,
    refind_dir: &Path,
    file_tracker: &mut fs::FileTracker,
) -> Result<String> {
    let gen_path = get_system_path(profile, Some(generation), None);
    let bootspec = BootSpec::load(&gen_path)?;

    // Get generation timestamp
    let metadata = std::fs::symlink_metadata(&gen_path)?;
    let mtime = metadata.modified()?;
    let datetime: DateTime<Local> = mtime.into();
    let timestamp = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

    let mut entry = String::new();

    if !bootspec.specialisations.is_empty() {
        // Has specialisations - create nested menu
        entry.push_str(&format!(
            "menuentry \"NixOS {} Generation {}\" {{\n",
            group_name, generation
        ));

        // Default entry
        entry.push_str(&format_boot_entry(
            true,
            &bootspec,
            "Default",
            &timestamp,
            refind_dir,
            file_tracker,
        )?);

        // Specialisation entries
        for (spec_name, spec_bootspec) in &bootspec.specialisations {
            entry.push_str(&format_boot_entry(
                true,
                spec_bootspec,
                spec_name,
                &timestamp,
                refind_dir,
                file_tracker,
            )?);
        }

        entry.push_str("}\n");
    } else {
        // No specialisations - flat entry
        entry.push_str(&format_boot_entry(
            false,
            &bootspec,
            &format!("NixOS {} Generation {}", group_name, generation),
            &timestamp,
            refind_dir,
            file_tracker,
        )?);
    }

    Ok(entry)
}

fn format_boot_entry(
    is_submenu: bool,
    bootspec: &BootSpec,
    label: &str,
    _timestamp: &str,
    refind_dir: &Path,
    file_tracker: &mut fs::FileTracker,
) -> Result<String> {
    let mut entry = String::new();

    let prefix = if is_submenu { "sub" } else { "" };
    entry.push_str(&format!("{}menuentry \"{}\" {{\n", prefix, label));

    // Copy kernel and get URI
    let kernel_uri = copy_kernel_to_efi(&bootspec.kernel, refind_dir, file_tracker)?;
    entry.push_str(&format!("  loader {}\n", kernel_uri));

    // Copy initrd if present
    if let Some(ref initrd) = bootspec.initrd {
        let initrd_uri = copy_kernel_to_efi(initrd, refind_dir, file_tracker)?;
        entry.push_str(&format!("  initrd {}\n", initrd_uri));
    }

    // Build kernel parameters
    let mut params = vec![format!("init={}", bootspec.init.display())];
    params.extend(bootspec.kernel_params.iter().cloned());
    let params_str = params.join(" ");

    entry.push_str(&format!("  options \"{}\"\n", params_str));
    entry.push_str("}\n");

    Ok(entry)
}

fn copy_kernel_to_efi(
    source: &Path,
    refind_dir: &Path,
    file_tracker: &mut fs::FileTracker,
) -> Result<String> {
    // Get package ID and suffix from store path
    let source = std::fs::canonicalize(source)?;
    let parent = source.parent().context("No parent directory")?;
    let package_id = parent
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid package ID")?;
    let suffix = source
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid filename")?;

    let dest_filename = format!("{}-{}", package_id, suffix);
    let dest_path = refind_dir.join("kernels").join(&dest_filename);

    // Copy if not exists
    if !dest_path.exists() {
        std::fs::create_dir_all(dest_path.parent().unwrap())?;
        fs::copy_atomic(&source, &dest_path)?;
    }

    file_tracker.mark_used(&dest_path);

    // Return URI relative to EFI mount
    Ok(format!("/efi/refind/kernels/{}", dest_filename))
}
