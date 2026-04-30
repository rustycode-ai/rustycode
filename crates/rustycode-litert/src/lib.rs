#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod installer;
pub mod manager;
pub mod process;

pub use installer::{
    default_gemma_e4b_model_url, default_litert_lm_binary_url, default_litert_lm_install_dir,
    ensure_gemma_e4b_model, ensure_litert_lm_binary, ensure_litert_lm_runtime, find_executable,
    find_file, LiteRtLmInstallConfig, LiteRtLmInstallResult,
};

pub use manager::LitManager;
pub use process::{LitProcess, ProcessPool};
