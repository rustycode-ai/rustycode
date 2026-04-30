//! Project detection for auto-configuring LSP and tools.
//!
//! This module provides utilities for detecting project types from directory contents,
//! enabling automatic LSP server and tool configuration.

use std::path::{Path, PathBuf};

use crate::{LanguageId, LspConfig};

/// Project detector for auto-configuring tools based on project markers.
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect the build system from project markers in a directory.
    #[allow(clippy::too_many_lines)]
    ///
    /// Checks for common build system markers in priority order:
    /// - Cargo.toml → Cargo/Rust
    /// - package.json → Npm/Node.js
    /// - go.mod → Go
    /// - pom.xml → Maven/Java
    /// - build.gradle / build.gradle.kts → Gradle/Java
    /// - WORKSPACE / BUILD → Bazel
    /// - pyproject.toml / requirements.txt → Python/pip
    /// - composer.json → Composer/PHP
    /// - Gemfile → Ruby/Bundler
    /// - build.xml → Ant/Java
    /// - build.sbt → SBT/Scala
    /// - *.sln / *.csproj / *.fsproj → .NET
    /// - .Rproj / DESCRIPTION → R
    /// - *.ipynb → Jupyter
    pub fn detect_build_system(dir: &Path) -> Option<BuildSystem> {
        if dir.join("Cargo.toml").exists() {
            if dir.join("Cargo.lock").exists() || dir.join(".cargo").exists() {
                return Some(BuildSystem::CargoMake);
            }
            return Some(BuildSystem::Cargo);
        }
        if dir.join("package.json").exists() {
            if dir.join("pnpm-lock.yaml").exists() {
                return Some(BuildSystem::Pnpm);
            }
            if dir.join("yarn.lock").exists() {
                return Some(BuildSystem::Yarn);
            }
            if dir.join("package-lock.json").exists() {
                return Some(BuildSystem::Npm);
            }
            return Some(BuildSystem::Npm);
        }
        if dir.join("go.mod").exists() {
            return Some(BuildSystem::Go);
        }
        if dir.join("pom.xml").exists() {
            return Some(BuildSystem::Maven);
        }
        if dir.join("build.gradle.kts").exists() || dir.join("build.gradle").exists() {
            return Some(BuildSystem::Gradle);
        }
        if dir.join("WORKSPACE").exists() || dir.join("WORKSPACE.bazel").exists() {
            return Some(BuildSystem::Bazel);
        }
        if dir.join("BUILD").exists()
            && !dir.join("Cargo.toml").exists()
            && !dir.join("package.json").exists()
        {
            if dir.join("BUILD.bazel").exists() {
                return Some(BuildSystem::Bazel);
            }
            // Check for .bazel files in the directory
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Ok(path) = entry.path().into_os_string().into_string() {
                        if path.contains(".bazel") {
                            return Some(BuildSystem::Bazel);
                        }
                    }
                }
            }
        }
        if dir.join("pyproject.toml").exists()
            || dir.join("requirements.txt").exists()
            || dir.join("setup.py").exists()
        {
            return Some(BuildSystem::Pip);
        }
        if dir.join("composer.json").exists() {
            return Some(BuildSystem::Composer);
        }
        if dir.join("Gemfile").exists() {
            return Some(BuildSystem::Ruby);
        }
        if dir.join("CMakeLists.txt").exists() {
            return Some(BuildSystem::CMake);
        }
        if dir.join("Makefile").exists() || dir.join("makefile").exists() {
            return Some(BuildSystem::Make);
        }
        if dir.join("build.xml").exists() {
            return Some(BuildSystem::Ant);
        }
        if dir.join("build.sbt").exists() {
            return Some(BuildSystem::Sbt);
        }
        // .NET — check for solution/project files
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if Path::new(&name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sln"))
                    || Path::new(&name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("csproj"))
                    || Path::new(&name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("fsproj"))
                    || Path::new(&name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("vbproj"))
                {
                    return Some(BuildSystem::Dotnet);
                }
            }
        }
        // R
        if dir.join(".Rproj").exists() || dir.join("DESCRIPTION").exists() {
            return Some(BuildSystem::R);
        }
        // Jupyter notebooks
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if Path::new(&name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ipynb"))
                {
                    return Some(BuildSystem::Jupyter);
                }
            }
        }
        None
    }

    /// Detect recommended linters for a project based on build system.
    pub fn detect_linters(build_system: &BuildSystem) -> Vec<String> {
        match build_system {
            BuildSystem::Cargo => vec!["clippy".to_string(), "rustfmt".to_string()],
            BuildSystem::Npm | BuildSystem::Yarn | BuildSystem::Pnpm => {
                vec!["eslint".to_string(), "prettier".to_string()]
            }
            BuildSystem::Pip => vec!["ruff".to_string(), "mypy".to_string()],
            BuildSystem::Go => vec!["golangci-lint".to_string(), "gofmt".to_string()],
            BuildSystem::Maven | BuildSystem::Gradle => {
                vec!["checkstyle".to_string(), "pmd".to_string()]
            }
            BuildSystem::Bazel => vec!["buildifier".to_string()],
            BuildSystem::Composer => vec!["phpcs".to_string(), "php-cs-fixer".to_string()],
            BuildSystem::Ruby => vec!["rubocop".to_string()],
            BuildSystem::CMake
            | BuildSystem::Make
            | BuildSystem::CargoMake
            | BuildSystem::Jupyter => vec![],
            BuildSystem::Ant => vec!["checkstyle".to_string()],
            BuildSystem::Sbt => vec!["scalafix".to_string()],
            BuildSystem::Dotnet => vec!["dotnet format".to_string()],
            BuildSystem::R => vec!["lintr".to_string()],
        }
    }

    /// Detect recommended formatters for a project based on build system.
    pub fn detect_formatters(build_system: &BuildSystem) -> Vec<String> {
        match build_system {
            BuildSystem::Cargo => vec!["rustfmt".to_string()],
            BuildSystem::Npm | BuildSystem::Yarn | BuildSystem::Pnpm => {
                vec!["prettier".to_string(), "dprint".to_string()]
            }
            BuildSystem::Pip => vec!["ruff".to_string(), "black".to_string()],
            BuildSystem::Go => vec!["gofmt".to_string(), "goimports".to_string()],
            BuildSystem::Maven | BuildSystem::Gradle | BuildSystem::Ant => {
                vec!["google-java-format".to_string()]
            }
            BuildSystem::Bazel => vec!["buildifier".to_string()],
            BuildSystem::Composer => vec!["php-cs-fixer".to_string()],
            BuildSystem::Ruby => vec!["rubocop".to_string()],
            BuildSystem::CMake | BuildSystem::Make | BuildSystem::CargoMake | BuildSystem::R => {
                vec![]
            }
            BuildSystem::Sbt => vec!["scalafmt".to_string()],
            BuildSystem::Dotnet => vec!["dotnet format".to_string()],
            BuildSystem::Jupyter => vec!["black".to_string(), "nbqa".to_string()],
        }
    }

    /// Detect recommended LSP servers for a project based on build system.
    #[allow(clippy::too_many_lines)]
    pub fn detect_lsp_config(build_system: &BuildSystem) -> LspConfig {
        let mut servers = std::collections::HashMap::new();

        match build_system {
            BuildSystem::Cargo => {
                servers.insert(
                    "rust".to_string(),
                    crate::LspServerConfig::new("rust-analyzer", vec![]),
                );
            }
            BuildSystem::Npm | BuildSystem::Yarn | BuildSystem::Pnpm => {
                servers.insert(
                    "typescript".to_string(),
                    crate::LspServerConfig::new(
                        "typescript-language-server",
                        vec!["--stdio".to_string()],
                    ),
                );
                servers.insert(
                    "javascript".to_string(),
                    crate::LspServerConfig::new(
                        "typescript-language-server",
                        vec!["--stdio".to_string()],
                    ),
                );
                servers.insert(
                    "json".to_string(),
                    crate::LspServerConfig::new(
                        "vscode-json-languageserver",
                        vec!["--stdio".to_string()],
                    ),
                );
                servers.insert(
                    "html".to_string(),
                    crate::LspServerConfig::new(
                        "vscode-html-languageserver",
                        vec!["--stdio".to_string()],
                    ),
                );
                servers.insert(
                    "css".to_string(),
                    crate::LspServerConfig::new(
                        "vscode-css-languageserver",
                        vec!["--stdio".to_string()],
                    ),
                );
            }
            BuildSystem::Pip | BuildSystem::Jupyter => {
                servers.insert(
                    "python".to_string(),
                    crate::LspServerConfig::new("pyright-langserver", vec!["--stdio".to_string()]),
                );
            }
            BuildSystem::Go => {
                servers.insert(
                    "go".to_string(),
                    crate::LspServerConfig::new("gopls", vec!["serve".to_string()]),
                );
            }
            BuildSystem::Maven | BuildSystem::Gradle | BuildSystem::Ant => {
                servers.insert(
                    "java".to_string(),
                    crate::LspServerConfig::new("jdtls", vec![]),
                );
            }
            BuildSystem::Bazel => {
                servers.insert(
                    "bazel".to_string(),
                    crate::LspServerConfig::new("bazel-lsp", vec![]),
                );
            }
            BuildSystem::Composer => {
                servers.insert(
                    "php".to_string(),
                    crate::LspServerConfig::new("phpactor", vec!["language-server".to_string()]),
                );
            }
            BuildSystem::Ruby => {
                servers.insert(
                    "ruby".to_string(),
                    crate::LspServerConfig::new("solargraph", vec!["stdio".to_string()]),
                );
            }
            BuildSystem::CMake => {
                servers.insert(
                    "cmake".to_string(),
                    crate::LspServerConfig::new("cmake-language-server", vec![]),
                );
            }
            BuildSystem::Make | BuildSystem::CargoMake => {}
            BuildSystem::Sbt => {
                servers.insert(
                    "scala".to_string(),
                    crate::LspServerConfig::new("metals", vec![]),
                );
            }
            BuildSystem::Dotnet => {
                servers.insert(
                    "csharp".to_string(),
                    crate::LspServerConfig::new("omnisharp", vec!["--stdio".to_string()]),
                );
            }
            BuildSystem::R => {
                servers.insert(
                    "r".to_string(),
                    crate::LspServerConfig::new("r-languageserver", vec![]),
                );
            }
        }

        LspConfig { servers }
    }

    /// Auto-detect all project tools from a directory.
    ///
    /// This is the main entry point for project auto-detection.
    /// Returns `None` if no recognizable project is found.
    pub fn detect(dir: &Path) -> Option<ProjectToolDetection> {
        let build_system = Self::detect_build_system(dir)?;
        let linters = Self::detect_linters(&build_system);
        let formatters = Self::detect_formatters(&build_system);
        let lsp_config = Self::detect_lsp_config(&build_system);

        Some(ProjectToolDetection {
            build_system,
            linters,
            formatters,
            lsp_config,
        })
    }

    /// Find the project root by searching upward for build markers.
    pub fn find_project_root(start: &Path) -> Option<PathBuf> {
        LanguageId::detect_root_dir(start)
    }
}

/// Detected project tools.
#[derive(Debug, Clone)]
pub struct ProjectToolDetection {
    pub build_system: BuildSystem,
    pub linters: Vec<String>,
    pub formatters: Vec<String>,
    pub lsp_config: LspConfig,
}

/// Build system enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    Cargo,
    Maven,
    Gradle,
    Bazel,
    Npm,
    Pip,
    Yarn,
    Pnpm,
    Go,
    CargoMake,
    Make,
    CMake,
    Composer,
    Ruby,
    Ant,
    Sbt,
    Dotnet,
    R,
    Jupyter,
}

impl BuildSystem {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Cargo => "Cargo",
            Self::Maven => "Maven",
            Self::Gradle => "Gradle",
            Self::Bazel => "Bazel",
            Self::Npm => "Npm",
            Self::Pip => "Pip",
            Self::Yarn => "Yarn",
            Self::Pnpm => "Pnpm",
            Self::Go => "Go",
            Self::CargoMake => "CargoMake",
            Self::Make => "Make",
            Self::CMake => "CMake",
            Self::Composer => "Composer",
            Self::Ruby => "Ruby",
            Self::Ant => "Ant",
            Self::Sbt => "SBT",
            Self::Dotnet => ".NET",
            Self::R => "R",
            Self::Jupyter => "Jupyter",
        }
    }
}

impl std::fmt::Display for BuildSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_project(markers: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for marker in markers {
            let path = dir.path().join(marker);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&path, "").unwrap();
        }
        dir
    }

    #[test]
    fn test_detect_rust_project() {
        let dir = temp_project(&["Cargo.toml"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Cargo));
        assert!(detection.linters.contains(&"clippy".to_string()));
        assert!(detection.formatters.contains(&"rustfmt".to_string()));
        assert!(detection.lsp_config.servers.contains_key("rust"));
    }

    #[test]
    fn test_detect_npm_project() {
        let dir = temp_project(&["package.json"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Npm));
        assert!(detection.linters.contains(&"eslint".to_string()));
        assert!(detection.formatters.contains(&"prettier".to_string()));
        assert!(detection.lsp_config.servers.contains_key("typescript"));
    }

    #[test]
    fn test_detect_go_project() {
        let dir = temp_project(&["go.mod"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Go));
        assert!(detection.lsp_config.servers.contains_key("go"));
    }

    #[test]
    fn test_detect_python_project() {
        let dir = temp_project(&["pyproject.toml"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Pip));
        assert!(detection.linters.contains(&"ruff".to_string()));
        assert!(detection.lsp_config.servers.contains_key("python"));
    }

    #[test]
    fn test_detect_java_maven() {
        let dir = temp_project(&["pom.xml"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Maven));
        assert!(detection.lsp_config.servers.contains_key("java"));
    }

    #[test]
    fn test_detect_java_gradle() {
        let dir = temp_project(&["build.gradle"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Gradle));
    }

    #[test]
    fn test_detect_bazel() {
        let dir = temp_project(&["WORKSPACE", "BUILD"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Bazel));
    }

    #[test]
    fn test_detect_no_project() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ProjectDetector::detect(dir.path()).is_none());
    }

    #[test]
    fn test_detect_ruby() {
        let dir = temp_project(&["Gemfile"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Ruby));
        assert!(detection.linters.contains(&"rubocop".to_string()));
    }

    #[test]
    fn test_detect_php() {
        let dir = temp_project(&["composer.json"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Composer));
        assert!(detection.lsp_config.servers.contains_key("php"));
    }

    #[test]
    fn test_build_system_display() {
        assert_eq!(BuildSystem::Cargo.to_string(), "Cargo");
        assert_eq!(BuildSystem::Npm.to_string(), "Npm");
        assert_eq!(BuildSystem::Dotnet.to_string(), ".NET");
        assert_eq!(BuildSystem::Sbt.to_string(), "SBT");
    }

    #[test]
    fn test_detect_ant_project() {
        let dir = temp_project(&["build.xml"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Ant));
    }

    #[test]
    fn test_detect_sbt_project() {
        let dir = temp_project(&["build.sbt"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Sbt));
        assert!(detection.lsp_config.servers.contains_key("scala"));
    }

    #[test]
    fn test_detect_dotnet_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("App.sln"), "").unwrap();
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Dotnet));
        assert!(detection.lsp_config.servers.contains_key("csharp"));
    }

    #[test]
    fn test_detect_r_project() {
        let dir = temp_project(&[".Rproj"]);
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::R));
        assert!(detection.lsp_config.servers.contains_key("r"));
    }

    #[test]
    fn test_detect_jupyter_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("analysis.ipynb"), "{}").unwrap();
        let detection = ProjectDetector::detect(dir.path()).unwrap();
        assert!(matches!(detection.build_system, BuildSystem::Jupyter));
        assert!(detection.lsp_config.servers.contains_key("python"));
    }
}
