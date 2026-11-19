use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const CRATES: &[&str] = &["gpui-pre", "gpui-pre-wgpu", "gpui-base"];

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
           clone      Remove ./patches, run `cargo clone`, and update versions in README.md\n  \
           versions   Print crate versions currently in ./patches"
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live inside the workspace root")
        .to_path_buf()
}

fn cmd_clone() -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace_root();
    let patches_dir = root.join("patches");

    if patches_dir.exists() {
        println!("Removing {}...", patches_dir.display());
        fs::remove_dir_all(&patches_dir)?;
    }

    println!("Cloning crates: {}", CRATES.join(", "));
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&root)
        .arg("clone")
        .args(CRATES)
        .args(["--", "./patches"])
        .status()
        .map_err(|e| format!("failed to execute `cargo clone`: {e}"))?;

    if !status.success() {
        return Err(
            "`cargo clone` failed (make sure `cargo-clone` is installed via `cargo install cargo-clone`)"
                .into(),
        );
    }

    let versions = read_all_versions(&patches_dir)?;
    update_readme_versions(&root.join("README.md"), &versions)?;

    println!("\nCloned versions:");
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

fn read_all_versions(patches_dir: &Path) -> Result<Vec<(&'static str, String)>, Box<dyn std::error::Error>> {
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
