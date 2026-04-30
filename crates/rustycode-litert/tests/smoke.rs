#![allow(clippy::unwrap_used, clippy::expect_used)]

use rustycode_litert::{
    default_litert_lm_binary_url, ensure_litert_lm_runtime, LiteRtLmInstallConfig,
};
use std::path::PathBuf;

fn unique_temp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    let base = std::env::current_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(".tmp");
    base.join(format!(
        "rustycode-litert-smoke-{}-{}",
        std::process::id(),
        nanos
    ))
}

#[tokio::test]
#[ignore = "requires network access and downloads large model binary"]
async fn litert_runtime_can_install_and_generate() {
    let install_dir = unique_temp_dir();
    let config = LiteRtLmInstallConfig {
        version: "v0.10.2".to_string(),
        binary_url: default_litert_lm_binary_url().expect("unsupported platform"),
        model_url:
            "https://huggingface.co/litert-community/Qwen3-0.6B/resolve/main/Qwen3-0.6B.litertlm"
                .to_string(),
        install_dir,
        binary_filename: "litert_lm_main".to_string(),
        model_filename: "Qwen3-0.6B.litertlm".to_string(),
    };

    let result = ensure_litert_lm_runtime(&config)
        .await
        .expect("LiteRT runtime should install");

    assert!(result.binary_path.exists(), "expected binary to exist");
    assert!(result.model_path.exists(), "expected model to exist");

    let output = tokio::process::Command::new(&result.binary_path)
        .args([
            "--backend",
            "cpu",
            "--model_path",
            result.model_path.to_str().expect("model path utf-8"),
            "--input_prompt",
            "Say hello in one short sentence.",
        ])
        .output()
        .await
        .expect("LiteRT binary should run");

    assert!(
        output.status.success(),
        "LiteRT binary failed: status={:?}, stderr={}, stdout={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(!stdout.is_empty(), "expected generation output");
    assert!(
        stdout.chars().any(|c| c.is_ascii_alphabetic()),
        "expected readable text output, got: {stdout}",
    );

    let _ = tokio::fs::remove_dir_all(&result.install_dir).await;
}
