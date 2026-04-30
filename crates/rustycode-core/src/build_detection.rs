//! Build system detection and guidance for automated build tasks.
//!
//! This module automatically detects Python build systems (setup.py, Cython, etc.)
//! and provides guidance on the correct sequence of build steps. It addresses the
//! problem where agents build projects but don't install them properly, leading
//! to false success claims.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Detected build system type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildSystem {
    /// Python package with setup.py
    Setuptools,
    /// Python package with Cython .pyx files
    Cython,
    /// CMake-based build system
    CMake,
    /// Makefile-based build
    Make,
    /// Cargo-based Rust build
    Cargo,
    /// Generic pip-installable package
    PipInstall,
}

impl BuildSystem {
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Setuptools => "Setuptools (setup.py)",
            Self::Cython => "Cython Extension",
            Self::CMake => "CMake",
            Self::Make => "Make",
            Self::Cargo => "Cargo (Rust)",
            Self::PipInstall => "Pip Package",
        }
    }

    /// Get description of what this build system does
    pub fn description(&self) -> &'static str {
        match self {
            Self::Setuptools => "Python package installation via setup.py",
            Self::Cython => "Python package with compiled Cython extensions (.pyx files)",
            Self::CMake => "C/C++ project built with CMake",
            Self::Make => "C/C++ or generic project built with Make",
            Self::Cargo => "Rust package built with Cargo",
            Self::PipInstall => "Python package installable via pip",
        }
    }
}

/// A single build step to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStep {
    /// Order in which this step should run (0-based)
    pub sequence: usize,
    /// Command to execute
    pub command: String,
    /// Human-readable description
    pub description: String,
    /// Whether this step is critical (failure = build fails)
    pub critical: bool,
    /// Expected success indicators
    pub success_pattern: Option<String>,
}

impl BuildStep {
    /// Create a critical build step
    pub fn critical(seq: usize, command: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            sequence: seq,
            command: command.into(),
            description: desc.into(),
            critical: true,
            success_pattern: None,
        }
    }

    /// Create a non-critical step
    pub fn optional(seq: usize, command: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            sequence: seq,
            command: command.into(),
            description: desc.into(),
            critical: false,
            success_pattern: None,
        }
    }

    /// Add expected success pattern
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.success_pattern = Some(pattern.into());
        self
    }
}

/// Analysis results for a detected project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildAnalysis {
    /// Detected build systems (primary first)
    pub build_systems: Vec<BuildSystem>,
    /// Required build steps in order
    pub build_steps: Vec<BuildStep>,
    /// Verification commands to run after build
    pub verification_steps: Vec<String>,
    /// The package name (if detected)
    pub package_name: Option<String>,
    /// Directory containing setup.py or equivalent
    pub build_dir: PathBuf,
}

impl BuildAnalysis {
    /// Create empty analysis
    pub fn new(build_dir: PathBuf) -> Self {
        Self {
            build_systems: Vec::new(),
            build_steps: Vec::new(),
            verification_steps: Vec::new(),
            package_name: None,
            build_dir,
        }
    }

    /// Get human-readable guidance for the build
    pub fn guidance(&self) -> String {
        let mut lines = vec![format!(
            "Detected build system: {}",
            self.build_systems
                .first()
                .map(|bs| bs.name())
                .unwrap_or("unknown")
        )];

        if let Some(pkg) = &self.package_name {
            lines.push(format!("Package name: {}", pkg));
        }

        lines.push(format!("\nBuild steps ({} total):", self.build_steps.len()));
        for step in &self.build_steps {
            let critical = if step.critical {
                "CRITICAL"
            } else {
                "optional"
            };
            lines.push(format!(
                "  {}. [{}] {}",
                step.sequence + 1,
                critical,
                step.description
            ));
            lines.push(format!("     Command: {}", step.command));
        }

        if !self.verification_steps.is_empty() {
            lines.push(format!(
                "\nVerification steps ({} total):",
                self.verification_steps.len()
            ));
            for (i, verify) in self.verification_steps.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, verify));
            }
        }

        lines.join("\n")
    }
}

/// Detect build system in a directory
pub fn detect_build_system(workspace: &Path) -> Result<BuildAnalysis, String> {
    let mut analysis = BuildAnalysis::new(workspace.to_path_buf());

    // Check for Python setup.py
    if workspace.join("setup.py").exists() {
        analysis.build_systems.push(BuildSystem::Setuptools);

        // Check for Cython files
        if has_cython_files(workspace) {
            analysis.build_systems.insert(0, BuildSystem::Cython);
        }

        // Extract package name from setup.py
        analysis.package_name = extract_package_name(workspace);

        // Generate build steps for Python
        generate_python_build_steps(&mut analysis);
    }

    // Check for CMakeLists.txt
    if workspace.join("CMakeLists.txt").exists() {
        analysis.build_systems.push(BuildSystem::CMake);
        generate_cmake_build_steps(&mut analysis);
    }

    // Check for Makefile
    if workspace.join("Makefile").exists() || workspace.join("makefile").exists() {
        analysis.build_systems.push(BuildSystem::Make);
        generate_make_build_steps(&mut analysis);
    }

    // Check for Cargo.toml (Rust)
    if workspace.join("Cargo.toml").exists() {
        analysis.build_systems.push(BuildSystem::Cargo);
        generate_cargo_build_steps(&mut analysis);
    }

    // Check for requirements.txt
    if workspace.join("requirements.txt").exists()
        && !analysis.build_systems.contains(&BuildSystem::Setuptools)
    {
        analysis.build_systems.push(BuildSystem::PipInstall);
        analysis.build_steps.push(BuildStep::critical(
            0,
            "pip install -r requirements.txt",
            "Install Python dependencies",
        ));
    }

    if analysis.build_systems.is_empty() {
        return Err("No recognized build system detected".to_string());
    }

    Ok(analysis)
}

/// Check if directory contains Cython files
fn has_cython_files(workspace: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(workspace) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "pyx" || ext == "pxd" {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract package name from setup.py
fn extract_package_name(workspace: &Path) -> Option<String> {
    let setup_py = workspace.join("setup.py");
    if let Ok(content) = std::fs::read_to_string(&setup_py) {
        // Look for name="..." or name='...'
        for line in content.lines() {
            if line.contains("name=") || line.contains("name =") {
                if let Some(start) = line.find('"') {
                    if let Some(end) = line[start + 1..].find('"') {
                        return Some(line[start + 1..start + 1 + end].to_string());
                    }
                }
                if let Some(start) = line.find('\'') {
                    if let Some(end) = line[start + 1..].find('\'') {
                        return Some(line[start + 1..start + 1 + end].to_string());
                    }
                }
            }
        }
    }
    None
}

/// Generate build steps for Python projects
fn generate_python_build_steps(analysis: &mut BuildAnalysis) {
    let has_cython = analysis.build_systems.contains(&BuildSystem::Cython);

    // Step 1: Install build dependencies
    analysis.build_steps.push(BuildStep::critical(
        0,
        "pip install setuptools wheel",
        "Install Python build tools",
    ));

    if has_cython {
        analysis.build_steps.push(BuildStep::critical(
            1,
            "pip install cython",
            "Install Cython compiler",
        ));
    }

    // Step 2: Build extensions (if Cython)
    if has_cython {
        analysis.build_steps.push(
            BuildStep::critical(
                if has_cython { 2 } else { 1 },
                "python setup.py build_ext --inplace",
                "Build Cython extensions",
            )
            .with_pattern(r"\.so|\.pyd"),
        );
    }

    // Step 3: Install package globally
    let install_step = if has_cython { 3 } else { 2 };
    analysis.build_steps.push(BuildStep::critical(
        install_step,
        "pip install -e .",
        "Install package to site-packages",
    ));

    // Verification
    if let Some(pkg_name) = &analysis.package_name {
        analysis.verification_steps.push(format!(
            "python3 -c 'import {}; print(\"OK\")' # Verify import works",
            pkg_name
        ));
        analysis
            .verification_steps
            .push(format!("pip show {} # Verify installation", pkg_name));
    }
}

/// Generate build steps for CMake projects
fn generate_cmake_build_steps(analysis: &mut BuildAnalysis) {
    analysis.build_steps.push(BuildStep::critical(
        0,
        "mkdir -p build",
        "Create build directory",
    ));
    analysis.build_steps.push(
        BuildStep::critical(1, "cd build && cmake ..", "Run CMake configuration")
            .with_pattern("Configuring done|Build files"),
    );
    analysis.build_steps.push(
        BuildStep::critical(2, "cd build && make", "Build project")
            .with_pattern(r"Built target|100%|Linking"),
    );
    analysis
        .verification_steps
        .push("ls -la build/*/".to_string());
}

/// Generate build steps for Make projects
fn generate_make_build_steps(analysis: &mut BuildAnalysis) {
    analysis.build_steps.push(
        BuildStep::critical(0, "make", "Build project with make")
            .with_pattern(r"done|built|complete|100%"),
    );
    analysis.build_steps.push(BuildStep::optional(
        1,
        "make install",
        "Install built artifacts",
    ));
}

/// Generate build steps for Cargo projects
fn generate_cargo_build_steps(analysis: &mut BuildAnalysis) {
    analysis.build_steps.push(
        BuildStep::critical(0, "cargo build --release", "Build Rust project")
            .with_pattern("Finished|Compiling"),
    );
    analysis
        .verification_steps
        .push("ls -la target/release/".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_names() {
        assert_eq!(BuildSystem::Setuptools.name(), "Setuptools (setup.py)");
        assert_eq!(BuildSystem::Cython.name(), "Cython Extension");
    }

    #[test]
    fn test_build_step_critical() {
        let step = BuildStep::critical(0, "echo test", "test command");
        assert!(step.critical);
        assert_eq!(step.sequence, 0);
    }

    #[test]
    fn test_build_step_optional() {
        let step = BuildStep::optional(1, "echo optional", "optional command");
        assert!(!step.critical);
    }

    #[test]
    fn test_build_step_with_pattern() {
        let step = BuildStep::critical(0, "cmd", "desc").with_pattern("success|ok");
        assert_eq!(step.success_pattern, Some("success|ok".to_string()));
    }

    #[test]
    fn test_build_analysis_new() {
        let analysis = BuildAnalysis::new(PathBuf::from("/tmp"));
        assert_eq!(analysis.build_systems.len(), 0);
        assert_eq!(analysis.build_steps.len(), 0);
    }

    #[test]
    fn test_build_analysis_guidance() {
        let mut analysis = BuildAnalysis::new(PathBuf::from("/tmp"));
        analysis.build_systems.push(BuildSystem::Setuptools);
        analysis.package_name = Some("mypackage".to_string());
        analysis
            .build_steps
            .push(BuildStep::critical(0, "pip install", "install deps"));

        let guidance = analysis.guidance();
        assert!(guidance.contains("Setuptools"));
        assert!(guidance.contains("mypackage"));
        assert!(guidance.contains("Build steps"));
    }
}
