use std::{
    ffi::OsStr,
    fs::{self, symlink_metadata, File},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use clap::Parser;

/// Generate a rEFInd config from NixOS generations and dump it as a String.
/// - Main entry shows only the newest/default generation
/// - Submenu lists all generations
///
/// Pure dry-run: no writes, no copies, no syncs.
#[derive(Parser, Debug)]
#[command(name = "nixos-generate-refind-conf")]
#[command(version, about)]
struct Cli {
    /// ESP mount root (where /efi lives). Often /boot.
    #[arg(long, default_value = "/boot")]
    efi_mount: PathBuf,

    /// Seconds to show menu before defaulting (omit to keep rEFInd's default)
    #[arg(long)]
    timeout: Option<u32>,

    /// Extra rEFInd config to append verbatim (path to a file)
    #[arg(long)]
    extra_config: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct Gen {
    profile: Option<String>,
    number: u32,
}

#[derive(Clone, Debug)]
struct GenDetails {
    profile: Option<String>,
    number: u32,
    loader: String,
    initrd: String,
    kernel_params: String,
    description: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let s = generate_config_string(&cli.efi_mount, cli.timeout, cli.extra_config.as_deref())?;

    println!("{s}");
    Ok(())
}

/// Public, reusable entry point. Produces the rEFInd config as a String.
/// Auto-discovers the "default" generation. Pure dry-run.
fn generate_config_string(
    efi_mount: &Path,
    timeout: Option<u32>,
    extra_config_path: Option<&Path>,
) -> Result<String> {
    // Gather generations (system + profiles)
    let mut gens = get_generations(None)?;
    for p in get_profiles()? {
        gens.extend(get_generations(Some(&p))?);
    }
    if gens.is_empty() {
        anyhow::bail!("No NixOS generations found.");
    }

    // Pick default target path via heuristic, then map to a generation.
    let default_target = discover_default_target();
    let default = if let Some(target) = default_target {
        find_generation_by_target(&gens, &target)?.unwrap_or_else(|| newest_generation(&gens))
    } else {
        newest_generation(&gens)
    };

    // Build submenu for all generations, newest -> oldest
    let mut rev = gens.clone();
    rev.sort_by(|a, b| b.number.cmp(&a.number));

    let mut submenu = String::new();
    for g in &rev {
        let d = generation_details(g, efi_mount)?;
        submenu.push_str(&submenu_entry(&d));
        submenu.push('\n');
    }

    // Main entry: default (or newest)
    let main_details = generation_details(&default, efi_mount)?;

    // Assemble full config
    build_config_text(timeout, extra_config_path, &main_details, &submenu)
}

/// Try to discover the “default” system target path:
/// 1) /nix/var/nix/profiles/system (current profile selection)
/// 2) /run/current-system (booted)
/// If neither resolves, return None (caller will fall back to newest).
fn discover_default_target() -> Option<PathBuf> {
    let candidates = [
        Path::new("/nix/var/nix/profiles/system"),
        Path::new("/run/current-system"),
    ];
    for p in candidates {
        if let Ok(t) = fs::read_link(p) {
            return Some(t);
        }
    }
    None
}

/// Read generations via `nix-env --list-generations -p <profile_path>`
fn get_generations(profile: Option<&str>) -> Result<Vec<Gen>> {
    let prof_path = match profile {
        Some(p) => format!("/nix/var/nix/profiles/system-profiles/{p}"),
        None => "/nix/var/nix/profiles/system".to_string(),
    };

    let output = Command::new("sudo")
        .args([
            "nix-env",
            "--list-generations",
            "-p",
            &prof_path,
            // "--option",
            // "build-users-group",
            // "",
        ])
        .output()
        .with_context(|| "failed to execute nix-env")?;

    if !output.status.success() {
        anyhow::bail!("nix-env --list-generations failed for {}", prof_path);
    }

    let s = String::from_utf8_lossy(&output.stdout);
    let mut gens = Vec::new();
    for line in s.lines() {
        println!("Gen: {line}");
        if let Some((first, _)) = line.trim().split_once(' ') {
            if let Ok(n) = first.trim().parse::<u32>() {
                gens.push(Gen {
                    profile: profile.map(str::to_string),
                    number: n,
                });
            }
        }
    }
    Ok(gens)
}

fn get_profiles() -> Result<Vec<String>> {
    let dir = Path::new("/nix/var/nix/profiles/system-profiles");
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with("-link") {
            out.push(name.to_string());
        }
    }
    Ok(out)
}

fn newest_generation(gens: &[Gen]) -> Gen {
    gens.iter()
        .cloned()
        .max_by_key(|g| g.number)
        .expect("non-empty")
}

/// Match a discovered default *target path* to a generation’s system link target.
fn find_generation_by_target(gens: &[Gen], target: &Path) -> Result<Option<Gen>> {
    for g in gens {
        let link = system_dir(&g.profile, g.number);
        let link_target = fs::read_link(&link).unwrap_or(link.clone());
        if path_eq(&link_target, target) {
            return Ok(Some(g.clone()));
        }
    }
    Ok(None)
}

/// `/nix/var/nix/profiles/system[-profiles/<profile>]-<number>-link`
fn system_dir(profile: &Option<String>, number: u32) -> PathBuf {
    match profile {
        Some(p) => Path::new("/nix/var/nix/profiles")
            .join("system-profiles")
            .join(format!("{p}-{number}-link")),
        None => Path::new("/nix/var/nix/profiles").join(format!("system-{number}-link")),
    }
}

fn path_eq(a: &Path, b: &Path) -> bool {
    fn canon(p: &Path) -> PathBuf {
        fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }
    canon(a) == canon(b)
}

/// Build details for a generation; **no copying** (dry-run).
fn generation_details(g: &Gen, efi_mount: &Path) -> Result<GenDetails> {
    // Resolve store paths for kernel & initrd
    let kernel_store = profile_path(&g.profile, g.number, "kernel")?;
    let initrd_store = profile_path(&g.profile, g.number, "initrd")?;

    // Compute where they'd be staged (but don't copy)
    let (loader_rel, _loader_abs) = efi_target_for_store(&kernel_store, efi_mount);
    let (initrd_rel, _initrd_abs) = efi_target_for_store(&initrd_store, efi_mount);

    // Generation dir (link target of system link itself)
    let gen_dir = fs::read_link(system_dir(&g.profile, g.number))
        .unwrap_or_else(|_| system_dir(&g.profile, g.number));

    // kernel params: systemConfig=... init=.../init + contents of kernel-params
    let mut kernel_params = format!(
        "systemConfig={} init={}/init ",
        gen_dir.display(),
        gen_dir.display()
    );
    let params_file = gen_dir.join("kernel-params");
    if let Ok(mut f) = File::open(&params_file) {
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        kernel_params.push_str(&s);
    }

    // human description
    let description = describe_generation(&gen_dir).unwrap_or_else(|_| "Unknown".to_string());

    Ok(GenDetails {
        profile: g.profile.clone(),
        number: g.number,
        loader: loader_rel,
        initrd: initrd_rel,
        kernel_params,
        description,
    })
}

fn profile_path(profile: &Option<String>, number: u32, name: &str) -> Result<PathBuf> {
    let target = system_dir(profile, number).join(name);
    let link =
        fs::read_link(&target).with_context(|| format!("readlink {} failed", target.display()))?;
    Ok(link)
}

/// Map a store path (/nix/store/<hash>-<name>/…/<file>) to:
///  - rEFInd-visible path: /efi/nixos/<name>-<file>.efi (string in config)
///  - absolute path on ESP: <efi_mount>/efi/nixos/<name>-<file>.efi (not used here)
fn efi_target_for_store(store_file: &Path, efi_mount: &Path) -> (String, PathBuf) {
    let file_name = store_file
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("image");
    let store_dir = store_file
        .parent()
        .and_then(|p| p.file_name())
        .and_then(OsStr::to_str)
        .unwrap_or("store");

    let rel = format!("/efi/nixos/{}-{}.efi", store_dir, file_name);
    let abs = efi_mount.join(rel.trim_start_matches('/'));
    (rel, abs)
}

fn describe_generation(gen_dir: &Path) -> Result<String> {
    let nixos_version = fs::read_to_string(gen_dir.join("nixos-version"))
        .unwrap_or_else(|_| "Unknown".to_string())
        .trim()
        .to_string();

    let kernel_path = gen_dir.join("kernel");
    let kernel_real = fs::canonicalize(&kernel_path).unwrap_or(kernel_path);
    let modules_dir = kernel_real
        .parent()
        .unwrap_or(&kernel_real)
        .join("lib/modules");
    let kernel_version = fs::read_dir(&modules_dir)
        .ok()
        .and_then(|mut it| it.next())
        .and_then(|e| e.ok())
        .and_then(|e| e.file_name().into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let md = symlink_metadata(gen_dir)?;
    #[cfg(target_os = "linux")]
    let sec = { std::os::unix::fs::MetadataExt::ctime(&md) };
    #[cfg(not(target_os = "linux"))]
    let sec = 0;

    let date = chrono::NaiveDateTime::from_timestamp_opt(sec, 0)
        .map(|dt| dt.date().to_string())
        .unwrap_or_else(|| "unknown-date".to_string());

    Ok(format!(
        "NixOS {}, Linux Kernel {}, Built on {}",
        nixos_version, kernel_version, date
    ))
}

fn build_config_text(
    timeout: Option<u32>,
    extra_config_path: Option<&Path>,
    main_details: &GenDetails,
    submenu: &str,
) -> Result<String> {
    let mut out = String::new();
    if let Some(secs) = timeout {
        out.push_str(&format!("timeout {}\n", secs));
    }
    if let Some(p) = extra_config_path {
        let mut s = String::new();
        File::open(p)
            .with_context(|| format!("open extra config {}", p.display()))?
            .read_to_string(&mut s)?;
        out.push_str(&s);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str(&menu_entry(main_details, submenu));
    Ok(out)
}

fn menu_entry(main: &GenDetails, submenu_entries: &str) -> String {
    format!(
        r#"
menuentry "NixOS" {{
    loader {}
    initrd {}
    options "{}"
{}}}
"#,
        main.loader,
        main.initrd,
        escape_quotes(main.kernel_params.trim()),
        indent(submenu_entries.trim_end(), 4),
    )
}

fn submenu_entry(d: &GenDetails) -> String {
    format!(
        r#"
submenuentry "Generation {} {}" {{
    loader {}
    initrd {}
    options "{}"
}}
"#,
        d.number,
        d.description,
        d.loader,
        d.initrd,
        escape_quotes(d.kernel_params.trim()),
    )
}

fn escape_quotes(s: &str) -> String {
    s.replace('"', r#""""#)
}

fn indent(s: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    s.lines()
        .map(|l| format!("{pad}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
