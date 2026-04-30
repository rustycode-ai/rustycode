use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PathError {
    #[error("Cannot determine home directory")]
    HomeDirNotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct RustyCodePath;

impl RustyCodePath {
    pub fn home() -> Result<PathBuf, PathError> {
        dirs::home_dir().ok_or(PathError::HomeDirNotFound)
    }

    pub fn config_dir() -> Result<PathBuf, PathError> {
        Self::home().map(|h| h.join(".rustycode"))
    }

    pub fn tui_config_file() -> Result<PathBuf, PathError> {
        Self::config_dir().map(|p| p.join("tui-config.json"))
    }

    pub fn skills_dir() -> Result<PathBuf, PathError> {
        Self::config_dir().map(|p| p.join("skills"))
    }

    pub fn global_config_file() -> Result<PathBuf, PathError> {
        Self::config_dir().map(|p| p.join("config.json"))
    }
}
