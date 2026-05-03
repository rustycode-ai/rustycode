use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use futures::StreamExt;
use reqwest::Url;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tar::Archive;
use tokio::io::AsyncWriteExt;
use tracing::instrument;
use walkdir::WalkDir;

const DEFAULT_VERSION: &str = "v0.10.2";
const DEFAULT_BINARY_FILENAME: &str = "litert_lm_main";
const DEFAULT_MODEL_FILENAME: &str = "gemma-3n-e4b.litertlm";

#[derive(Debug, Clone)]
pub struct LiteRtLmInstallConfig {
    pub version: String,
    pub binary_url: String,
    pub model_url: String,
    pub install_dir: PathBuf,
    pub binary_filename: String,
    pub model_filename: String,
}

#[derive(Debug, Clone)]
pub struct LiteRtLmInstallResult {
    pub install_dir: PathBuf,
    pub binary_path: PathBuf,
    pub model_path: PathBuf,
}

impl LiteRtLmInstallConfig {
    pub fn new(
        binary_url: impl Into<String>,
        model_url: impl Into<String>,
        install_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            version: DEFAULT_VERSION.to_string(),
            binary_url: binary_url.into(),
            model_url: model_url.into(),
            install_dir: install_dir.into(),
            binary_filename: DEFAULT_BINARY_FILENAME.to_string(),
            model_filename: DEFAULT_MODEL_FILENAME.to_string(),
        }
    }
}

pub fn default_litert_lm_install_dir() -> PathBuf {
    dirs::cache_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rustycode")
        .join("litert-lm")
        .join(DEFAULT_VERSION)
}

pub fn default_litert_lm_binary_url() -> Result<String> {
    let arch = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-arm64"
    } else if cfg!(target_os = "windows") {
        "windows-x64"
    } else {
        anyhow::bail!(
            "Unsupported platform: {}/{}. Supported: macOS (arm64/x64), Linux (arm64/x64), Windows (x64)",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    };

    Ok(format!(
        "https://github.com/luengnat/LiteRT-LM/releases/download/{}/litert-lm-{}-{}.tar.gz",
        DEFAULT_VERSION,
        DEFAULT_VERSION.trim_start_matches('v'),
        arch
    ))
}

impl Default for LiteRtLmInstallConfig {
    #[allow(clippy::expect_used)] // Default::default() cannot return Result; panics on unsupported platform
    fn default() -> Self {
        let binary_url =
            default_litert_lm_binary_url().expect("LiteRT-LM is not supported on this platform");
        Self {
            version: DEFAULT_VERSION.to_string(),
            binary_url,
            model_url: default_gemma_e4b_model_url(),
            install_dir: default_litert_lm_install_dir(),
            binary_filename: DEFAULT_BINARY_FILENAME.to_string(),
            model_filename: DEFAULT_MODEL_FILENAME.to_string(),
        }
    }
}

pub fn default_gemma_e4b_model_url() -> String {
    "https://huggingface.co/MiCkSoftware/gemma-3n-E4B-it-litert-lm/resolve/main/gemma-3n-E4B-it-int4-Web.litertlm"
        .to_string()
}

pub async fn ensure_litert_lm_runtime(
    config: &LiteRtLmInstallConfig,
) -> Result<LiteRtLmInstallResult> {
    let binary_path = ensure_litert_lm_binary(config).await?;
    let model_path = ensure_gemma_e4b_model(config).await?;
    Ok(LiteRtLmInstallResult {
        install_dir: config.install_dir.clone(),
        binary_path,
        model_path,
    })
}

pub async fn ensure_litert_lm_binary(config: &LiteRtLmInstallConfig) -> Result<PathBuf> {
    let binary_dir = config.install_dir.join("bin");
    let binary_path = binary_dir.join(&config.binary_filename);

    if binary_path.exists() {
        return Ok(binary_path);
    }

    tokio::fs::create_dir_all(&binary_dir)
        .await
        .context("failed to create LiteRT-LM binary directory")?;

    let archive_path = config
        .install_dir
        .join(format!("{}.tar.gz", config.binary_filename));
    download_to_path(&config.binary_url, &archive_path).await?;
    extract_tar_gz(&archive_path, &config.install_dir).await?;
    let _ = tokio::fs::remove_file(&archive_path).await;

    let extracted = find_executable(&config.install_dir, &config.binary_filename)
        .ok_or_else(|| anyhow::anyhow!("LiteRT-LM binary was not found after extraction"))?;
    if extracted != binary_path {
        if let Some(parent) = binary_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("failed to create LiteRT-LM binary destination")?;
        }

        #[cfg(target_os = "macos")]
        {
            let runtime_root = extracted
                .parent()
                .and_then(|parent| parent.parent())
                .ok_or_else(|| anyhow::anyhow!("invalid LiteRT-LM extraction layout"))?;
            let runtime_bin = extracted.display().to_string();
            let runtime_lib = runtime_root.join("lib").display().to_string();
            let wrapper = format!(
                "#!/bin/sh\nLIB_DIR={runtime_lib:?}\nexport DYLD_LIBRARY_PATH=\"$LIB_DIR${{DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}}\"\nexport DYLD_FALLBACK_LIBRARY_PATH=\"$LIB_DIR${{DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}}\"\nexec {runtime_bin:?} \"$@\"\n"
            );
            tokio::fs::write(&binary_path, wrapper)
                .await
                .context("failed to create LiteRT-LM binary wrapper")?;
        }

        #[cfg(target_os = "linux")]
        {
            let runtime_root = extracted
                .parent()
                .and_then(|parent| parent.parent())
                .ok_or_else(|| anyhow::anyhow!("invalid LiteRT-LM extraction layout"))?;
            let runtime_bin = extracted.display().to_string();
            let runtime_lib = runtime_root.join("lib").display().to_string();
            let wrapper = format!(
                "#!/bin/sh\nLIB_DIR={runtime_lib:?}\nexport LD_LIBRARY_PATH=\"$LIB_DIR${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexec {runtime_bin:?} \"$@\"\n"
            );
            tokio::fs::write(&binary_path, wrapper)
                .await
                .context("failed to create LiteRT-LM binary wrapper")?;
        }

        #[cfg(target_os = "windows")]
        {
            tokio::fs::copy(&extracted, &binary_path)
                .await
                .context("failed to copy LiteRT-LM binary into cache")?;
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let runtime_root = extracted
                .parent()
                .and_then(|parent| parent.parent())
                .ok_or_else(|| anyhow::anyhow!("invalid LiteRT-LM extraction layout"))?;
            let runtime_bin = extracted.display().to_string();
            let runtime_lib = runtime_root.join("lib").display().to_string();
            let wrapper = format!(
                "#!/bin/sh\nLIB_DIR={runtime_lib:?}\nexport LD_LIBRARY_PATH=\"$LIB_DIR${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexec {runtime_bin:?} \"$@\"\n"
            );
            tokio::fs::write(&binary_path, wrapper)
                .await
                .context("failed to create LiteRT-LM binary wrapper")?;
        }
    }

    ensure_executable(&binary_path).await?;
    Ok(binary_path)
}

pub async fn ensure_gemma_e4b_model(config: &LiteRtLmInstallConfig) -> Result<PathBuf> {
    let model_dir = config.install_dir.join("models");
    let model_path = model_dir.join(&config.model_filename);

    if model_path.exists() {
        return Ok(model_path);
    }

    tokio::fs::create_dir_all(&model_dir)
        .await
        .context("failed to create LiteRT-LM model directory")?;

    let parsed_url = Url::parse(&config.model_url).context("invalid LiteRT-LM model URL")?;
    if is_archive_url(parsed_url.path()) {
        let archive_path = config
            .install_dir
            .join(format!("{}.download", config.model_filename));
        download_to_path(&config.model_url, &archive_path).await?;
        extract_model_from_archive(&archive_path, &model_dir, &config.model_filename).await?;
        let _ = tokio::fs::remove_file(&archive_path).await;
    } else {
        download_to_path(&config.model_url, &model_path).await?;
    }

    if !model_path.exists() {
        let fallback = find_file(&model_dir, &config.model_filename)
            .ok_or_else(|| anyhow::anyhow!("LiteRT-LM model was not found after download"))?;
        if fallback != model_path && tokio::fs::rename(&fallback, &model_path).await.is_err() {
            tokio::fs::copy(&fallback, &model_path)
                .await
                .context("failed to copy LiteRT-LM model into cache")?;
        }
        return Ok(model_path);
    }

    Ok(model_path)
}

#[instrument(skip_all, fields(url = %url, destination = %destination.display()))]
async fn download_to_path(url: &str, destination: &Path) -> Result<()> {
    let response = reqwest::Client::builder()
        .build()
        .context("failed to create download client")?
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to start download: {url}"))?
        .error_for_status()
        .with_context(|| format!("download failed for {url}"))?;

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let tmp_path = destination.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("failed to create {}", tmp_path.display()))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed to read download chunk")?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp_path, destination)
        .await
        .with_context(|| format!("failed to move {} into place", destination.display()))?;
    Ok(())
}

async fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::open(&archive_path)
            .with_context(|| format!("failed to open {}", archive_path.display()))?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        archive
            .unpack(&destination)
            .with_context(|| format!("failed to extract into {}", destination.display()))?;
        Ok(())
    })
    .await
    .context("tar extraction task failed")??;

    Ok(())
}

async fn extract_model_from_archive(
    archive_path: &Path,
    destination_dir: &Path,
    model_filename: &str,
) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let destination_dir = destination_dir.to_path_buf();
    let model_filename = model_filename.to_string();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::open(&archive_path)
            .with_context(|| format!("failed to open {}", archive_path.display()))?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        archive
            .unpack(&destination_dir)
            .with_context(|| format!("failed to extract into {}", destination_dir.display()))?;
        if find_file(&destination_dir, &model_filename).is_none() {
            anyhow::bail!(
                "model {} was not found inside {}",
                model_filename,
                archive_path.display()
            );
        }
        Ok(())
    })
    .await
    .context("model extraction task failed")??;

    Ok(())
}

pub fn find_executable(root: &Path, filename: &str) -> Option<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|entry| {
            let path = entry.path();
            if path.file_name() == Some(OsStr::new(filename)) {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
}

pub fn find_file(root: &Path, filename: &str) -> Option<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|entry| {
            let path = entry.path();
            if path.file_name() == Some(OsStr::new(filename)) {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
}

async fn ensure_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = tokio::fs::metadata(path).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(path, permissions).await?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

fn is_archive_url(path: &str) -> bool {
    path.ends_with(".tar.gz")
        || Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tgz"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_url_points_to_public_mirror() {
        assert!(default_gemma_e4b_model_url().contains("huggingface.co/MiCkSoftware"));
        assert!(default_gemma_e4b_model_url().ends_with("gemma-3n-E4B-it-int4-Web.litertlm"));
    }

    #[test]
    fn default_binary_url_format() {
        // Should succeed on supported platforms (macOS, Linux, Windows)
        if let Ok(url) = default_litert_lm_binary_url() {
            assert!(url.contains("github.com/luengnat/LiteRT-LM"));
            assert!(url.ends_with(".tar.gz"));
            assert!(url.contains(DEFAULT_VERSION));
        }
    }

    #[test]
    fn default_config_has_consistent_filenames() {
        let config = LiteRtLmInstallConfig::default();
        assert_eq!(config.binary_filename, "litert_lm_main");
        assert_eq!(config.model_filename, "gemma-3n-e4b.litertlm");
        assert!(!config.binary_url.is_empty());
        assert!(!config.model_url.is_empty());
    }

    #[test]
    fn custom_config_overrides_defaults() {
        let config = LiteRtLmInstallConfig::new(
            "https://example.com/binary",
            "https://example.com/model",
            "/tmp/test-litert",
        );
        assert_eq!(config.binary_url, "https://example.com/binary");
        assert_eq!(config.model_url, "https://example.com/model");
        assert_eq!(config.install_dir, PathBuf::from("/tmp/test-litert"));
    }

    #[test]
    fn is_archive_url_detects_tar_gz() {
        assert!(is_archive_url("model.tar.gz"));
        assert!(is_archive_url("model.tgz"));
        assert!(!is_archive_url("model.bin"));
        assert!(!is_archive_url("model.litertlm"));
    }

    #[test]
    fn find_executable_locates_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(bin_dir.join("litert_lm_main"), "binary").unwrap();

        let found = find_executable(dir.path(), "litert_lm_main");
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name().unwrap(), "litert_lm_main");
    }

    #[test]
    fn find_executable_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_executable(dir.path(), "nonexistent").is_none());
    }

    #[test]
    fn find_file_locates_nested_file() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("models").join("v1");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("model.litertlm"), "data").unwrap();

        let found = find_file(dir.path(), "model.litertlm");
        assert!(found.is_some());
    }

    #[test]
    fn default_install_dir_is_under_cache() {
        let dir = default_litert_lm_install_dir();
        assert!(dir.to_string_lossy().contains("rustycode"));
        assert!(dir.to_string_lossy().contains("litert-lm"));
    }

    #[tokio::test]
    async fn ensure_binary_skips_download_if_exists() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let binary_path = bin_dir.join("litert_lm_main");
        std::fs::write(&binary_path, "fake-binary").unwrap();

        let config = LiteRtLmInstallConfig {
            install_dir: dir.path().to_path_buf(),
            ..LiteRtLmInstallConfig::default()
        };

        let result = ensure_litert_lm_binary(&config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), binary_path);
    }

    #[tokio::test]
    async fn ensure_model_skips_download_if_exists() {
        let dir = tempfile::tempdir().unwrap();
        let model_dir = dir.path().join("models");
        std::fs::create_dir_all(&model_dir).unwrap();
        let model_path = model_dir.join("gemma-3n-e4b.litertlm");
        std::fs::write(&model_path, "fake-model").unwrap();

        let config = LiteRtLmInstallConfig {
            install_dir: dir.path().to_path_buf(),
            ..LiteRtLmInstallConfig::default()
        };

        let result = ensure_gemma_e4b_model(&config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), model_path);
    }
}
