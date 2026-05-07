use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const RELEASES_API: &str = "https://api.github.com/repos/rustycode-ai/rustycode/releases";

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Update to latest nightly (prerelease) instead of stable
    #[arg(long)]
    pub nightly: bool,

    /// Target version tag (e.g. v0.2.0). Defaults to latest.
    #[arg(long)]
    pub target: Option<String>,
}

struct ReleaseInfo {
    tag: String,
    asset_url: String,
    asset_name: String,
}

pub async fn execute(args: &UpdateArgs) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("Current version: {current}");

    let release = find_release(args).await?;
    let version_tag = &release.tag;

    println!("Latest available: {version_tag}");

    if version_tag.trim_start_matches('v') == current && args.target.is_none() {
        println!("Already up to date.");
        return Ok(());
    }

    if args.check {
        println!("Update available: {current} → {version_tag}");
        println!("Run `rustycode update` to install.");
        return Ok(());
    }

    // Find current executable
    let exe_path = env::current_exe().context("failed to locate current executable")?;
    println!("Installing to: {}", exe_path.display());

    download_and_replace(&release, &exe_path).await?;

    println!("Updated to {version_tag} successfully.");
    Ok(())
}

async fn find_release(args: &UpdateArgs) -> Result<ReleaseInfo> {
    let platform = detect_platform()?;

    if let Some(target) = &args.target {
        // Fetch specific release by tag
        let url = format!("{RELEASES_API}/tags/{target}");
        let resp = http_get_json(&url).await?;
        return extract_asset(&resp, &platform);
    }

    if args.nightly {
        // List all releases, find first prerelease
        let url = format!("{RELEASES_API}?per_page=10");
        let resp = http_get_json(&url).await?;
        let releases = resp
            .as_array()
            .context("expected array of releases")?;

        for release in releases {
            if release
                .get("prerelease")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Ok(info) = extract_asset(release, &platform) {
                    return Ok(info);
                }
            }
        }
        anyhow::bail!("No nightly release found for platform {platform}");
    }

    // Latest stable release
    let url = format!("{RELEASES_API}/latest");
    let resp = http_get_json(&url).await?;
    extract_asset(&resp, &platform)
}

fn extract_asset(release: &serde_json::Value, platform: &str) -> Result<ReleaseInfo> {
    let tag = release["tag_name"]
        .as_str()
        .context("release missing tag_name")?
        .to_string();

    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };

    let assets = release["assets"]
        .as_array()
        .context("release missing assets")?;

    for asset in assets {
        let name = asset["name"].as_str().unwrap_or("");
        if name.contains(platform) && name.ends_with(ext) {
            return Ok(ReleaseInfo {
                tag,
                asset_url: asset["browser_download_url"]
                    .as_str()
                    .context("asset missing browser_download_url")?
                    .to_string(),
                asset_name: name.to_string(),
            });
        }
    }

    anyhow::bail!("no asset found for platform '{platform}' in release {tag}");
}

fn detect_platform() -> Result<String> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    let platform = match (os, arch) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("windows", "x86_64") => "windows-x64",
        _ => anyhow::bail!("unsupported platform: {os}-{arch}"),
    };

    Ok(platform.to_string())
}

async fn http_get_json(url: &str) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .user_agent("rustycode-cli")
        .build()?;

    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub API returned {status}: {body}");
    }

    let json: serde_json::Value = resp.json().await?;
    Ok(json)
}

async fn download_and_replace(release: &ReleaseInfo, exe_path: &Path) -> Result<()> {
    let tmp_dir = tempfile::tempdir().context("failed to create temp dir")?;

    // Download archive
    println!("Downloading {}...", release.asset_name);
    let client = reqwest::Client::builder()
        .user_agent("rustycode-cli")
        .build()?;

    let resp = client.get(&release.asset_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await?;

    let archive_path = tmp_dir.path().join(&release.asset_name);
    fs::write(&archive_path, &bytes)?;

    // Extract
    extract_archive(&archive_path, tmp_dir.path())?;

    // Find the binary
    let binary_name = if cfg!(windows) {
        "rustycode-cli.exe"
    } else {
        "rustycode-cli"
    };

    let new_binary = find_file_recursive(tmp_dir.path(), binary_name)
        .context("binary not found in archive")?;

    // Replace: rename old, copy new, remove old
    let old_path = exe_path.with_extension("old");
    fs::rename(exe_path, &old_path)
        .context("failed to rename current binary")?;
    fs::copy(&new_binary, exe_path)
        .context("failed to copy new binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(exe_path, fs::Permissions::from_mode(0o755))
            .context("failed to set executable permissions")?;
    }

    let _ = fs::remove_file(&old_path);
    Ok(())
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<()> {
    let archive_str = archive.to_string_lossy();

    if archive_str.ends_with(".tar.gz") || archive_str.ends_with(".tgz") {
        let status = std::process::Command::new("tar")
            .arg("xzf")
            .arg(archive)
            .arg("-C")
            .arg(dest)
            .current_dir(dest)
            .status()
            .context("failed to run tar")?;

        if !status.success() {
            anyhow::bail!("tar extraction failed");
        }
    } else if archive_str.ends_with(".zip") {
        let status = std::process::Command::new("unzip")
            .arg("-o")
            .arg(archive)
            .arg("-d")
            .arg(dest)
            .current_dir(dest)
            .status()
            .context("failed to run unzip")?;

        if !status.success() {
            anyhow::bail!("unzip extraction failed");
        }
    } else {
        anyhow::bail!("unsupported archive format: {}", archive_str);
    }

    Ok(())
}

fn find_file_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}
