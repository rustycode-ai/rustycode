use serde_json::Value;

pub struct TransformedOutput {
    pub title: String,
    pub short: String,
    pub full: Option<String>,
    pub structured: Option<Value>,
}

/// Try to transform command output using a named transformer or heuristics.
pub fn transform_by_name(name: &str, stdout: &str, stderr: &str) -> Option<TransformedOutput> {
    match name {
        "compact_git_status" => Some(compact_git_status(stdout, stderr)),
        "test_summary" => Some(test_summary(stdout, stderr)),
        "cargo_build" => Some(cargo_build_summary(stdout, stderr)),
        "lint_summary" => Some(lint_summary(stdout, stderr)),
        "git_log" => Some(compact_git_log(stdout, stderr)),
        "docker_build" => Some(docker_build_summary(stdout, stderr)),
        "npm_install" => Some(npm_install_summary(stdout, stderr)),
        "auto" => auto_transform(stdout, stderr),
        _ => None,
    }
}

fn auto_transform(stdout: &str, stderr: &str) -> Option<TransformedOutput> {
    let combined = format!("{stdout}\n{stderr}");
    if combined.contains("test result") || combined.contains("FAILED") {
        Some(test_summary(stdout, stderr))
    } else if combined.contains("Compiling") || combined.contains("Finished") {
        Some(cargo_build_summary(stdout, stderr))
    } else if combined.contains("warning:") || combined.contains("clippy") {
        Some(lint_summary(stdout, stderr))
    } else if combined.contains("modified")
        || combined.contains("On branch")
        || combined
            .lines()
            .any(|l| l.starts_with(' ') || l.starts_with('M') || l.starts_with('?'))
    {
        Some(compact_git_status(stdout, stderr))
    } else if combined.contains("Step") && combined.contains("Successfully built") {
        Some(docker_build_summary(stdout, stderr))
    } else if combined.contains("npm") && combined.contains("added") {
        Some(npm_install_summary(stdout, stderr))
    } else {
        None
    }
}

fn compact_git_status(stdout: &str, _stderr: &str) -> TransformedOutput {
    let mut files = Vec::new();
    for line in stdout.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        // simple heuristics: take last path fragment
        let item = if let Some(idx) = t.rfind('/') {
            t.get(idx + 1..).unwrap_or(t).to_string()
        } else {
            t.to_string()
        };
        files.push(item);
        if files.len() >= 20 {
            break;
        }
    }

    let count = stdout.lines().filter(|l| !l.trim().is_empty()).count();
    let short = if count == 0 {
        "no changes".to_string()
    } else {
        format!("{} changed files (showing up to {})", count, files.len())
    };

    TransformedOutput {
        title: "git status (compact)".to_string(),
        short: short.clone(),
        full: Some(stdout.to_string()),
        structured: Some(Value::Array(files.into_iter().map(Value::String).collect())),
    }
}

fn test_summary(stdout: &str, _stderr: &str) -> TransformedOutput {
    let mut failures = Vec::new();
    let mut passed = 0;
    let mut total = 0;

    for line in stdout.lines() {
        let t = line.trim();
        if t.contains("test result:") {
            // Parse: "test result: ok. X passed; Y failed; Z ignored"
            if t.contains("passed") {
                let parts: Vec<&str> = t.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    if part == &"passed" && i > 0 {
                        if let Ok(n) = parts[i - 1].parse::<usize>() {
                            passed = n;
                        }
                    }
                }
            }
            total = passed + failures.len();
        } else if t.contains("FAILED") || t.contains("failed") {
            failures.push(t.to_string());
        } else if t.starts_with("test ") && t.contains("... FAILED") {
            // format: test foo::bar ... FAILED
            let parts: Vec<&str> = t.split_whitespace().collect();
            if parts.len() >= 2 {
                failures.push(parts[1].to_string());
            }
        }
        if failures.len() >= 20 {
            break;
        }
    }

    let failed_count = failures.len();
    let short = if failed_count == 0 {
        if passed > 0 {
            format!("✅ {passed} tests passed")
        } else {
            "tests passed or no failures detected".to_string()
        }
    } else {
        format!(
            "❌ {}/{} tests failed (showing {})",
            failed_count,
            total.max(failed_count + passed),
            failures.len().min(failed_count)
        )
    };

    TransformedOutput {
        title: "test summary".to_string(),
        short: short.clone(),
        full: Some(stdout.to_string()),
        structured: Some(Value::Array(
            failures.into_iter().map(Value::String).collect(),
        )),
    }
}

fn cargo_build_summary(stdout: &str, stderr: &str) -> TransformedOutput {
    let mut compiled = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut finished = false;

    for line in stdout.lines().chain(stderr.lines()) {
        let t = line.trim();
        if t.starts_with("Compiling") {
            if let Some(name) = t.strip_prefix("Compiling") {
                compiled.push(name.trim().to_string());
            }
        } else if t.starts_with("Finished") {
            finished = true;
        } else if t.contains("warning:") {
            warnings.push(t.to_string());
        } else if t.contains("error:") || t.contains("error[") {
            errors.push(t.to_string());
        }
    }

    let status = if !errors.is_empty() {
        format!("❌ Build failed: {} errors", errors.len())
    } else if !warnings.is_empty() {
        format!("⚠️  Build succeeded: {} warnings", warnings.len())
    } else if finished {
        "✅ Build succeeded".to_string()
    } else {
        "Build in progress".to_string()
    };

    let short = format!("{} ({} crates compiled)", status, compiled.len());

    TransformedOutput {
        title: "cargo build summary".to_string(),
        short,
        full: Some(format!("{stdout}\n{stderr}")),
        structured: Some(serde_json::json!({
            "compiled": compiled,
            "errors": errors,
            "warnings": warnings,
            "status": if finished { "success" } else { "incomplete" }
        })),
    }
}

fn lint_summary(stdout: &str, stderr: &str) -> TransformedOutput {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for line in stdout.lines().chain(stderr.lines()) {
        let t = line.trim();
        if t.contains("warning:") {
            // Extract filename and line number
            let parts: Vec<&str> = t.split(':').collect();
            if parts.len() >= 2 {
                warnings.push(format!("{}:{}", parts[0], parts[1]));
            }
        } else if t.contains("error:") {
            let parts: Vec<&str> = t.split(':').collect();
            if parts.len() >= 2 {
                errors.push(format!("{}:{}", parts[0], parts[1]));
            }
        }
    }

    let short = if !errors.is_empty() {
        format!("❌ {} errors", errors.len())
    } else if !warnings.is_empty() {
        format!("⚠️  {} warnings", warnings.len())
    } else {
        "✅ No issues".to_string()
    };

    TransformedOutput {
        title: "lint summary".to_string(),
        short,
        full: Some(format!("{stdout}\n{stderr}")),
        structured: Some(serde_json::json!({
            "warnings": warnings,
            "errors": errors
        })),
    }
}

fn compact_git_log(stdout: &str, _stderr: &str) -> TransformedOutput {
    let commits: Vec<String> = stdout
        .lines()
        .take(10)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                None
            } else if parts.len() <= 8 {
                Some(parts.join(" "))
            } else {
                Some(format!("{} {}", parts[..8].join(" "), parts[8..].join(" ")))
            }
        })
        .collect();

    let total = stdout.lines().count();
    let short = format!("{} commits (showing {})", total, commits.len());

    TransformedOutput {
        title: "git log (compact)".to_string(),
        short,
        full: Some(stdout.to_string()),
        structured: Some(Value::Array(
            commits.into_iter().map(Value::String).collect(),
        )),
    }
}

fn docker_build_summary(stdout: &str, stderr: &str) -> TransformedOutput {
    let mut steps = Vec::new();
    let mut current_step = None;
    let mut image_name = None;

    for line in stdout.lines().chain(stderr.lines()) {
        let t = line.trim();
        if t.starts_with("Step") && t.contains("/") {
            current_step = Some(t.to_string());
        } else if t.contains("--->") {
            if let Some(step) = &current_step {
                steps.push(format!("{step} ✓"));
            }
            current_step = None;
        } else if t.starts_with("Successfully built") {
            if let Some(id) = t.split_whitespace().nth(2) {
                image_name = Some(id.to_string());
            }
        }
    }

    let image_id = image_name.as_deref();
    let short = if let Some(image) = image_id {
        format!("✅ Image: {} ({} steps)", image, steps.len())
    } else if !steps.is_empty() {
        format!("Building: {} steps completed", steps.len())
    } else {
        "Docker build".to_string()
    };

    TransformedOutput {
        title: "docker build summary".to_string(),
        short,
        full: Some(format!("{stdout}\n{stderr}")),
        structured: Some(serde_json::json!({
            "steps": steps,
            "image": image_name
        })),
    }
}

fn npm_install_summary(stdout: &str, stderr: &str) -> TransformedOutput {
    let mut packages = Vec::new();
    let mut added = 0;
    let mut removed = 0;
    let mut changed = 0;

    for line in stdout.lines().chain(stderr.lines()) {
        let t = line.trim();
        if t.starts_with("+") {
            if let Some(pkg) = t.split_whitespace().nth(1) {
                packages.push(format!("+ {pkg}"));
                added += 1;
            }
        } else if t.starts_with("-") {
            if let Some(pkg) = t.split_whitespace().nth(1) {
                packages.push(format!("- {pkg}"));
                removed += 1;
            }
        } else if t.starts_with("~") {
            if let Some(pkg) = t.split_whitespace().nth(1) {
                packages.push(format!("~ {pkg}"));
                changed += 1;
            }
        }
    }

    let short = format!("✅ {added} added, {removed} removed, {changed} changed");

    TransformedOutput {
        title: "npm install summary".to_string(),
        short,
        full: Some(format!("{stdout}\n{stderr}")),
        structured: Some(serde_json::json!({
            "packages": packages,
            "added": added,
            "removed": removed,
            "changed": changed
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_git_status_simple() {
        let out = " M src/lib.rs\n?? newfile.txt\n";
        let t = compact_git_status(out, "");
        assert!(t.short.contains("changed files"));
        assert!(t.structured.is_some());
    }

    #[test]
    fn test_summary_detects_failed() {
        let out = "running 2 tests\ntest foo::bar ... ok\ntest foo::baz ... FAILED\n";
        let t = test_summary(out, "");
        assert!(t.short.contains("failed") || t.short.contains("❌"));
        assert!(t.structured.is_some());
    }

    #[test]
    fn transform_by_name_returns_none_for_unknown() {
        assert!(transform_by_name("unknown_transformer", "out", "err").is_none());
    }

    #[test]
    fn transform_by_name_known_transformers() {
        assert!(transform_by_name("compact_git_status", "M file", "").is_some());
        assert!(transform_by_name("test_summary", "test result: ok", "").is_some());
        assert!(transform_by_name("cargo_build", "Compiling x", "").is_some());
        assert!(transform_by_name("lint_summary", "warning: x", "").is_some());
        // git_log needs lines with 8+ whitespace-separated parts
        assert!(transform_by_name(
            "git_log",
            "abc1234 def5678 ghi9012 jkl3456 mno7890 pqr1234 stu5678 commit msg",
            ""
        )
        .is_some());
        assert!(transform_by_name("docker_build", "Step 1/5", "").is_some());
        assert!(transform_by_name("npm_install", "added 1", "").is_some());
        assert!(transform_by_name("auto", "test result: ok", "").is_some());
    }

    #[test]
    fn cargo_build_summary_success() {
        let out = "Compiling foo v1.0\nCompiling bar v2.0\nFinished dev [unoptimized]\n";
        let t = cargo_build_summary(out, "");
        assert!(t.short.contains("Build succeeded"));
        assert!(t.short.contains("2 crates"));
    }

    #[test]
    fn cargo_build_summary_with_errors() {
        let out = "Compiling foo\nerror: could not compile\n";
        let t = cargo_build_summary(out, "");
        assert!(t.short.contains("Build failed"));
    }

    #[test]
    fn cargo_build_summary_with_warnings() {
        let out = "Compiling foo\nwarning: unused variable\nFinished dev\n";
        let t = cargo_build_summary(out, "");
        assert!(t.short.contains("warning"));
    }

    #[test]
    fn lint_summary_with_warnings() {
        let out = "src/main.rs:10:5: warning: unused variable\n";
        let t = lint_summary(out, "");
        assert!(t.short.contains("warning"));
        let s = t.structured.unwrap();
        assert!(s["warnings"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn lint_summary_with_errors() {
        let out = "src/lib.rs:5:1: error: expected semicolon\n";
        let t = lint_summary(out, "");
        assert!(t.short.contains("error"));
    }

    #[test]
    fn lint_summary_clean() {
        let t = lint_summary("", "");
        assert!(t.short.contains("No issues"));
    }

    #[test]
    fn git_log_compact() {
        let out = "abc1234 def5678 ghi9012 jkl3456 mno7890 pqr1234 stu5678 Some commit message\nxyz9999 aaa1111 bbb2222 ccc3333 ddd4444 eee5555 fff6666 Another commit\n";
        let t = compact_git_log(out, "");
        assert!(t.short.contains("commits"));
    }

    #[test]
    fn auto_detects_test_output() {
        let t = transform_by_name("auto", "test result: ok. 5 passed\n", "");
        assert!(t.is_some());
        assert!(t.unwrap().title.contains("test"));
    }

    #[test]
    fn auto_detects_cargo_build() {
        let t = transform_by_name("auto", "Compiling my-crate\nFinished\n", "");
        assert!(t.is_some());
        assert!(t.unwrap().title.contains("cargo"));
    }

    #[test]
    fn auto_returns_none_for_unknown() {
        let t = transform_by_name("auto", "random output\nnothing special\n", "");
        assert!(t.is_none());
    }

    #[test]
    fn npm_install_summary_output() {
        let out = "added 5 packages\n+ package-a@1.0\n+ package-b@2.0\n";
        let t = npm_install_summary(out, "");
        assert!(t.short.contains("added"));
        let s = t.structured.unwrap();
        assert!(s["added"].as_u64().unwrap() >= 2);
    }

    #[test]
    fn test_summary_all_passed() {
        let out = "running 42 tests\ntest result: ok. 42 passed; 0 failed; 0 ignored\n";
        let t = test_summary(out, "");
        assert!(t.short.contains("42") || t.short.contains("passed"));
    }

    #[test]
    fn compact_git_status_empty() {
        let t = compact_git_status("", "");
        assert!(t.short.contains("no changes"));
    }

    #[test]
    fn docker_build_summary_with_image() {
        let out = "Step 1/3 : FROM ubuntu\n ---> abc123\nSuccessfully built def456\n";
        let t = docker_build_summary(out, "");
        assert!(t.short.contains("def456"));
    }
}
