use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use semver::{Version, VersionReq};
use serde::Deserialize;

const REFERENCE_CRATE: &str = "gpui-kit";
const CRATES: &[&str] = &["gpui-pre", "gpui-pre-wgpu", "gpui-base"];

#[derive(Deserialize)]
struct IndexEntry {
    vers: String,
    #[serde(default)]
    deps: Vec<IndexDep>,
    #[serde(default)]
    yanked: bool,
}

#[derive(Deserialize)]
struct IndexDep {
    name: String,
    package: Option<String>,
    req: String,
    #[serde(default)]
    kind: Option<String>,
}

impl IndexDep {
    fn crate_name(&self) -> &str {
        self.package.as_deref().unwrap_or(&self.name)
    }

    fn is_normal(&self) -> bool {
        matches!(self.kind.as_deref(), None | Some("normal"))
    }
}

struct TargetVersions {
    gpui_kit: Version,
    crates: Vec<(&'static str, Version)>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("clone") => cmd_clone(),
        Some("check") => cmd_check(),
        Some("versions") => cmd_versions(),
        Some("-h" | "--help" | "help") | None => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(format!("unknown task '{other}' (run with --help for usage)").into()),
    }
}

fn print_usage() {
    println!(
        "Usage: cargo xtask <TASK>\n\n\
         Tasks:\n  \
           clone      Resolve crate versions from latest `gpui-kit`, clone mismatched crates into ./patches, and update README.md\n  \
           check      Compare ./patches versions against latest `gpui-kit` without modifying files\n  \
           versions   Print crate versions currently in ./patches"
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live inside the workspace root")
        .to_path_buf()
}

fn sparse_index_url(crate_name: &str) -> String {
    let lower = crate_name.to_ascii_lowercase();
    let prefix = match lower.len() {
        1 => "1".to_string(),
        2 => "2".to_string(),
        3 => format!("3/{}", &lower[..1]),
        _ => format!("{}/{}", &lower[..2], &lower[2..4]),
    };
    format!("https://index.crates.io/{prefix}/{lower}")
}

fn fetch_index_entries(crate_name: &str) -> Result<Vec<IndexEntry>, Box<dyn std::error::Error>> {
    let url = sparse_index_url(crate_name);
    let body = ureq::get(&url)
        .header("User-Agent", "gpui-zoom-test-xtask")
        .call()
        .map_err(|e| format!("failed to fetch crates.io index for `{crate_name}` ({url}): {e}"))?
        .body_mut()
        .read_to_string()?;

    let mut entries = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: IndexEntry = serde_json::from_str(line)
            .map_err(|e| format!("failed to parse index entry for `{crate_name}`: {e}"))?;
        entries.push(entry);
    }
    Ok(entries)
}

fn pick_latest_entry<'a>(
    crate_name: &str,
    entries: &'a [IndexEntry],
    req: Option<&VersionReq>,
) -> Result<(&'a IndexEntry, Version), Box<dyn std::error::Error>> {
    let mut matching: Vec<(&IndexEntry, Version)> = Vec::new();
    for entry in entries {
        if entry.yanked {
            continue;
        }
        let Ok(ver) = Version::parse(&entry.vers) else {
            continue;
        };
        if let Some(req) = req {
            if !req.matches(&ver) {
                continue;
            }
        }
        matching.push((entry, ver));
    }

    // Prefer stable releases when no explicit prerelease requirement is given.
    let best = matching
        .iter()
        .filter(|(_, v)| v.pre.is_empty())
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .or_else(|| matching.iter().max_by(|(_, a), (_, b)| a.cmp(b)))
        .cloned();

    best.ok_or_else(|| {
        match req {
            Some(req) => format!("no non-yanked version of `{crate_name}` matches `{req}`"),
            None => format!("no non-yanked version found for `{crate_name}`"),
        }
        .into()
    })
}

fn find_dep_req(entry: &IndexEntry, target_crate: &str) -> Option<Result<VersionReq, semver::Error>> {
    entry
        .deps
        .iter()
        .find(|d| d.is_normal() && d.crate_name() == target_crate)
        .map(|d| VersionReq::parse(&d.req))
}

fn resolve_target_versions() -> Result<TargetVersions, Box<dyn std::error::Error>> {
    let kit_entries = fetch_index_entries(REFERENCE_CRATE)?;
    let (kit_entry, kit_version) = pick_latest_entry(REFERENCE_CRATE, &kit_entries, None)?;

    let gpui_pre_req = find_dep_req(kit_entry, "gpui-pre")
        .ok_or_else(|| format!("`{REFERENCE_CRATE}@{kit_version}` does not depend on `gpui-pre`"))??;
    let gpui_base_req = find_dep_req(kit_entry, "gpui-base")
        .ok_or_else(|| format!("`{REFERENCE_CRATE}@{kit_version}` does not depend on `gpui-base`"))??;

    let gpui_pre_entries = fetch_index_entries("gpui-pre")?;
    let (_, gpui_pre_ver) = pick_latest_entry("gpui-pre", &gpui_pre_entries, Some(&gpui_pre_req))?;

    let gpui_base_entries = fetch_index_entries("gpui-base")?;
    let (_, gpui_base_ver) =
        pick_latest_entry("gpui-base", &gpui_base_entries, Some(&gpui_base_req))?;

    // `gpui-kit` depends on `gpui-pre-wgpu` transitively via `gpui-pre-web` / `gpui-pre-platform`.
    let gpui_pre_wgpu_req = if let Some(req) = find_dep_req(kit_entry, "gpui-pre-wgpu") {
        req?
    } else if let Some(web_req) = find_dep_req(kit_entry, "gpui-pre-web") {
        let web_req = web_req?;
        let web_entries = fetch_index_entries("gpui-pre-web")?;
        let (web_entry, web_ver) = pick_latest_entry("gpui-pre-web", &web_entries, Some(&web_req))?;
        find_dep_req(web_entry, "gpui-pre-wgpu")
            .ok_or_else(|| format!("`gpui-pre-web@{web_ver}` does not depend on `gpui-pre-wgpu`"))??
    } else {
        VersionReq::parse(&format!("={gpui_pre_ver}"))?
    };

    let gpui_pre_wgpu_entries = fetch_index_entries("gpui-pre-wgpu")?;
    let (_, gpui_pre_wgpu_ver) =
        pick_latest_entry("gpui-pre-wgpu", &gpui_pre_wgpu_entries, Some(&gpui_pre_wgpu_req))?;

    Ok(TargetVersions {
        gpui_kit: kit_version,
        crates: vec![
            ("gpui-pre", gpui_pre_ver),
            ("gpui-pre-wgpu", gpui_pre_wgpu_ver),
            ("gpui-base", gpui_base_ver),
        ],
    })
}

fn read_local_version(patches_dir: &Path, name: &str) -> Option<String> {
    let manifest_path = patches_dir.join(name).join("Cargo.toml");
    read_package_version(&manifest_path).ok()
}

fn cmd_check() -> Result<(), Box<dyn std::error::Error>> {
    let patches_dir = workspace_root().join("patches");
    println!("Resolving versions from latest `{REFERENCE_CRATE}` on crates.io...");
    let target = resolve_target_versions()?;
    println!("Latest {REFERENCE_CRATE}@{}\n", target.gpui_kit);

    for (name, target_ver) in &target.crates {
        let target_str = target_ver.to_string();
        match read_local_version(&patches_dir, name) {
            Some(local) if local == target_str => {
                println!("  [ok]     {name}: {local} (up to date)");
            }
            Some(local) => {
                println!("  [update] {name}: {local} -> {target_str}");
            }
            None => {
                println!("  [clone]  {name}: <missing> -> {target_str}");
            }
        }
    }
    Ok(())
}

fn cmd_clone() -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace_root();
    let patches_dir = root.join("patches");

    println!("Resolving versions from latest `{REFERENCE_CRATE}` on crates.io...");
    let target = resolve_target_versions()?;
    println!("Latest {REFERENCE_CRATE}@{}", target.gpui_kit);

    fs::create_dir_all(&patches_dir)?;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let mut cloned_any = false;
    for (name, target_ver) in &target.crates {
        let target_str = target_ver.to_string();
        let local = read_local_version(&patches_dir, name);
        if local.as_deref() == Some(target_str.as_str()) {
            println!("Skipping {name}@{target_str} (already matches ./patches/{name})");
            continue;
        }

        let crate_dir = patches_dir.join(name);
        if crate_dir.exists() {
            println!(
                "Removing {} (local: {})...",
                crate_dir.display(),
                local.as_deref().unwrap_or("unknown")
            );
            fs::remove_dir_all(&crate_dir)?;
        }

        let spec = format!("{name}@{target_str}");
        let dest = format!("./patches/{name}");
        println!("Cloning {spec} into {dest}...");
        let status = Command::new(&cargo)
            .current_dir(&root)
            .args(["clone", &spec, "--", &dest])
            .status()
            .map_err(|e| format!("failed to execute `cargo clone {spec}`: {e}"))?;

        if !status.success() {
            return Err(format!(
                "`cargo clone {spec}` failed (make sure `cargo-clone` is installed via `cargo install cargo-clone`)"
            )
            .into());
        }
        cloned_any = true;
    }

    let versions = read_all_versions(&patches_dir)?;
    update_readme_versions(&root.join("README.md"), &versions)?;

    if cloned_any {
        println!("\nCurrent versions in ./patches:");
    } else {
        println!("\nAll crates already match latest {REFERENCE_CRATE}@{}:", target.gpui_kit);
    }
    for (name, ver) in &versions {
        println!("  - {name}@{ver}");
    }
    println!("\nUpdated README.md.");
    Ok(())
}

fn cmd_versions() -> Result<(), Box<dyn std::error::Error>> {
    let patches_dir = workspace_root().join("patches");
    let versions = read_all_versions(&patches_dir)?;
    for (name, ver) in &versions {
        println!("- {name}@{ver}");
    }
    Ok(())
}

fn read_all_versions(
    patches_dir: &Path,
) -> Result<Vec<(&'static str, String)>, Box<dyn std::error::Error>> {
    let mut results = Vec::with_capacity(CRATES.len());
    for &name in CRATES {
        let manifest_path = patches_dir.join(name).join("Cargo.toml");
        let version = read_package_version(&manifest_path)?;
        results.push((name, version));
    }
    Ok(results)
}

fn read_package_version(manifest_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(manifest_path)
        .map_err(|e| format!("failed to read {}: {e}", manifest_path.display()))?;

    let mut in_package = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some(rest) = line.strip_prefix("version") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let value = rest.split('#').next().unwrap_or("").trim();
                    if let Some(ver) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                        return Ok(ver.to_string());
                    }
                }
            }
        }
    }

    Err(format!("could not find [package].version in {}", manifest_path.display()).into())
}

fn update_readme_versions(
    readme_path: &Path,
    versions: &[(&str, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(readme_path)?;
    let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing_newline = content.ends_with('\n');

    let lines: Vec<&str> = content.lines().collect();
    let header_idx = lines
        .iter()
        .position(|l| l.trim() == "current versions:")
        .ok_or("README.md does not contain a `current versions:` section")?;

    // Skip optional blank lines after `current versions:`
    let mut list_start = header_idx + 1;
    while list_start < lines.len() && lines[list_start].trim().is_empty() {
        list_start += 1;
    }

    // Consume existing `- ...` version lines
    let mut list_end = list_start;
    while list_end < lines.len() && lines[list_end].trim_start().starts_with("- ") {
        list_end += 1;
    }

    let mut new_lines: Vec<String> = Vec::with_capacity(lines.len() + versions.len());
    new_lines.extend(lines[..list_start].iter().map(|s| (*s).to_string()));
    for (name, ver) in versions {
        new_lines.push(format!("- {name}@{ver}"));
    }
    new_lines.extend(lines[list_end..].iter().map(|s| (*s).to_string()));

    let mut updated = new_lines.join(line_ending);
    if trailing_newline {
        updated.push_str(line_ending);
    }

    fs::write(readme_path, updated)?;
    Ok(())
}
