use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[allow(dead_code)]
pub struct TestConfig {
    pub project_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    _temp_dir: TempDir,
}

#[allow(dead_code)]
impl TestConfig {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_dir = temp_dir.path().to_path_buf();
        let config_dir = project_dir.join(".rustycode");
        let data_dir = project_dir.join("data");
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&data_dir).expect("Failed to create data dir");

        Self {
            project_dir,
            config_dir,
            data_dir,
            _temp_dir: temp_dir,
        }
    }

    pub fn write_config(&self, name: &str, content: &str) -> PathBuf {
        let path = self.config_dir.join(name);
        fs::write(&path, content).expect("Failed to write config");
        path
    }

    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }
}

#[allow(dead_code)]
pub struct TestEnv {
    vars: Vec<(String, Option<String>)>,
}

#[allow(dead_code)]
impl TestEnv {
    pub fn new() -> Self {
        Self { vars: Vec::new() }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        self.vars.push((key.to_string(), prev));
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for (key, prev) in &self.vars {
            match prev {
                Some(val) => std::env::set_var(key, val),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[allow(dead_code)]
pub async fn retry_async<F, Fut, T>(max_retries: usize, f: F) -> T
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let mut last = None;
    for _ in 0..max_retries {
        last = Some(f().await);
    }
    last.expect("at least one retry required")
}
