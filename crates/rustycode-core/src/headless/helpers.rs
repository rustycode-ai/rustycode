use crate::headless::hints;
use rustycode_protocol::agent_protocol::AgentAction;
use rustycode_protocol::ToolCall;
use rustycode_tools::{ToolContext, ToolRegistry};
use serde_json;
use std::path::Path;

/// Dispatches a structured AgentAction to the ToolRegistry.
pub fn dispatch_agent_action(
    action: AgentAction,
    cwd: &Path,
    tool_registry: &ToolRegistry,
) -> String {
    let (name, args) = match action {
        AgentAction::EditFile { path, content } => (
            "edit_file".to_string(),
            serde_json::json!({"path": path, "content": content}),
        ),
        AgentAction::Bash { command, cwd } => (
            "bash".to_string(),
            serde_json::json!({"command": command, "cwd": cwd.unwrap_or_else(|| ".".to_string())}),
        ),
        AgentAction::ListFiles { path } => {
            ("list_dir".to_string(), serde_json::json!({"path": path}))
        }
        AgentAction::Complete { message } => return format!("Task completed: {}", message),
    };

    let call = ToolCall {
        call_id: "headless-structured".to_string(),
        name,
        arguments: args,
    };

    let ctx = ToolContext::new(cwd);
    let result = tool_registry.execute(&call, &ctx);

    if result.success {
        result.output
    } else {
        result
            .error
            .unwrap_or_else(|| "Error executing structured action".to_string())
    }
}

pub fn summarize_tool_args(name: &str, partial_json: &str) -> String {
    if name == "bash" {
        if let Ok(args) = serde_json::from_str::<serde_json::Value>(partial_json) {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                let cmd = cmd.trim();
                if cmd.len() > 60 {
                    return format!("{:.60}...", cmd);
                }
                return cmd.to_string();
            }
        }
        return "bash command".to_string();
    }

    if let Ok(args) = serde_json::from_str::<serde_json::Value>(partial_json) {
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            return path.to_string();
        }
    }

    format!("{} args", name)
}

pub(crate) fn normalize_tool_name(name: &str) -> &str {
    match name {
        "Edit" => "edit_file",
        "Read" => "read_file",
        "Write" | "Create" => "write_file",
        "Bash" | "Shell" => "bash",
        "Grep" | "Search" => "grep",
        "Glob" | "Find" => "glob",
        _ => name,
    }
}

/// Execute a tool via the shared registry and apply headless-specific post-processing.
pub fn execute_headless_tool(
    cwd: &Path,
    tool_name: &str,
    tool_json: &str,
    tool_registry: &ToolRegistry,
) -> String {
    let resolved_name = normalize_tool_name(tool_name);
    let args: serde_json::Value = match serde_json::from_str(tool_json) {
        Ok(v) => v,
        Err(e) => return format!("Error: Failed to parse tool arguments: {}", e),
    };

    let call = ToolCall {
        call_id: "headless".to_string(),
        name: resolved_name.to_string(),
        arguments: args.clone(),
    };

    let ctx = ToolContext::new(cwd);
    let result = tool_registry.execute(&call, &ctx);

    let mut output = if result.success {
        result.output
    } else {
        result.error.unwrap_or_else(|| "Error".to_string())
    };

    // Bash-specific hints that improve agent behavior in headless mode.
    if tool_name == "bash" {
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            let cmd_lower = command.to_lowercase();
            let out_lower = output.to_lowercase();

            // Grep line-number hint
            if let Some(first_line) = output.lines().next() {
                if let Some(line_num) = first_line
                    .split(':')
                    .nth(1)
                    .and_then(|p| p.parse::<usize>().ok())
                {
                    if line_num > 100 {
                        let offset = line_num.saturating_sub(5);
                        output.push_str(&format!(
                            "\n\nHINT: To read the code around line {}, use read_file with offset={} and limit=80",
                            line_num, offset
                        ));
                    }
                }
            }

            if out_lower.contains(".pyx") && out_lower.contains("attributeerror") {
                output.push_str(
                    "\n\nHINT: The error is in a Cython (.pyx) file. After editing .pyx source files, \
                    you MUST rebuild: run \"python setup.py build_ext --inplace\" then \"pip install -e .\" \
                    to recompile the extension.",
                );
            }

            if (out_lower.contains("has no attribute 'int'")
                || out_lower.contains("has no attribute 'float'")
                || out_lower.contains("has no attribute 'complex'")
                || out_lower.contains("has no attribute 'bool'")
                || out_lower.contains("has no attribute 'str'"))
                && (out_lower.contains("numpy") || out_lower.contains("np."))
            {
                output.push_str(
                    "\n\nHINT: This is a NumPy 2.0 deprecation error. Search ALL source files \
                    (including .pyx, .pxd Cython files) for the deprecated pattern. \
                    Use: grep -rn \"np.int[^0-9_]\" --include=\"*.py\" --include=\"*.pyx\" --include=\"*.pxd\" . \
                    Then fix ALL occurrences and rebuild if you edited Cython files.",
                );
            }

            if out_lower.contains("no module named 'setuptools'")
                || out_lower.contains("no module named 'cython'")
                || out_lower.contains("modulenotfounderror: no module named 'setuptools'")
                || out_lower.contains("modulenotfounderror: no module named 'cython'")
                || out_lower.contains("command not found: cython")
                || out_lower.contains("error: command 'cython' failed")
                || out_lower.contains("unable to find 'cython'")
            {
                output.push_str(
                    "\n\nHINT: Missing Python build dependency. Install with: \
                    pip install setuptools wheel cython \
                    Then retry the build command.",
                );
            }

            if out_lower.contains("cannot import name 'gcd' from 'fractions'")
                || out_lower.contains("attributeerror: module 'fractions' has no attribute 'gcd'")
            {
                output.push_str(
                    "\n\nHINT: `fractions.gcd` was removed in Python 3.9+. \
                    Replace `from fractions import gcd` with `from math import gcd` in the source file.",
                );
            }
            if out_lower.contains("cannot import name 'mapping' from 'collections'")
                || out_lower.contains("cannot import name 'mutablemapping' from 'collections'")
                || out_lower.contains("cannot import name 'iterable' from 'collections'")
            {
                output.push_str(
                    "\n\nHINT: Several ABCs were moved from `collections` to `collections.abc` \
                    in Python 3.10+. Replace `from collections import Mapping/MutableMapping/Iterable` \
                    with `from collections.abc import Mapping/MutableMapping/Iterable`.",
                );
            }

            if out_lower.contains("command timed out") {
                output.push_str(
                    "\n\nHINT: The command timed out. Compilation/build steps can be very slow in emulated \
                    environments. Try: 1) Break the build into smaller steps (compile one extension at a time), \
                    2) Use simpler compiler flags, 3) Pre-install all dependencies before building, \
                    4) Run the build in background and poll for completion with a loop.",
                );
            }

            if out_lower.contains("no space left")
                || out_lower.contains("disk full")
                || out_lower.contains("cannot write: no space")
                || out_lower.contains("errno 28")
                || out_lower.contains("write error: no space")
            {
                output.push_str(
                    "\n\nHINT: Disk is full. Free space FIRST before retrying: \
                    rm -rf build/ dist/ *.egg-info __pycache__ .cache/ /tmp/* 2>/dev/null \
                    Then retry with a smaller build (avoid creating temp files, use --no-build-isolation).",
                );
            }

            if cmd_lower.contains("build_ext")
                && !out_lower.contains("error")
                && !out_lower.contains("failed")
            {
                output.push_str(
                    "\n\nREMINDER: build_ext completed. You MUST also run `pip install -e .` (or `pip install -e /path/to/package`) \
                    to install the package so it can be imported system-wide. \
                    Without this step, `import <package>` will fail with ModuleNotFoundError.",
                );
            }

            if cmd_lower.contains("git checkout")
                && cmd_lower.contains(".")
                && !cmd_lower.contains("git checkout -b")
            {
                output.push_str(
                    "\n\nWARNING: You just used git checkout to revert a file. This undoes your previous work! \
                    If your edit caused an error, FIX the error instead of reverting. \
                    Move FORWARD — edit the file to fix the issue, don't go back to square one.",
                );
            }

            if (cmd_lower.contains("nohup")
                || cmd_lower.contains("&")
                || cmd_lower.contains("http.server")
                || cmd_lower.contains("python -m http")
                || cmd_lower.contains("flask run")
                || cmd_lower.contains("uvicorn")
                || cmd_lower.contains("gunicorn"))
                && !out_lower.contains("error")
            {
                output.push_str(
                    "\n\nREMINDER: Server started. Before declaring done, verify it responds: \
                    sleep 2 && curl -s -o /dev/null -w '%{http_code}' http://localhost:<port>/path \
                    If you get 000, the server isn't ready — add sleep or check the port.",
                );
            }

            if cmd_lower.contains("pip") && cmd_lower.contains("install") {
                let network_error = out_lower.contains("connectionerror")
                    || out_lower.contains("connection refused")
                    || out_lower.contains("could not find a version")
                    || out_lower.contains("403 forbidden")
                    || out_lower.contains("404 not found")
                    || out_lower.contains("ssl:")
                    || out_lower.contains("timed out")
                    || out_lower.contains("read timed out");
                if network_error {
                    output.push_str(
                        "\n\nHINT: pip install failed due to a network/authentication error. \
                        Try: 1) pip install --retries 5 <package>, 2) Use a different package index, \
                        3) Try a specific version: pip install <package>==<version>",
                    );
                }
                if out_lower.contains("permission denied")
                    || out_lower.contains("not writable")
                    || out_lower.contains("access is denied")
                {
                    output.push_str(
                        "\n\nHINT: pip install failed due to permission error. \
                        Try: pip install --user <package> or use a virtual environment.",
                    );
                }
            }

            if out_lower.contains("modulenotfounderror") || out_lower.contains("no module named") {
                let module_hints = [
                    ("cv2", "opencv-python"),
                    ("PIL", "Pillow"),
                    ("sklearn", "scikit-learn"),
                    ("scipy", "scipy"),
                    ("yaml", "pyyaml"),
                    ("Crypto", "pycryptodome"),
                    ("bs4", "beautifulsoup4"),
                    ("lxml", "lxml"),
                    ("pytest", "pytest"),
                    ("flask", "flask"),
                    ("django", "django"),
                    ("requests", "requests"),
                    ("boto3", "boto3"),
                    ("grpc", "grpcio"),
                ];
                for (import_name, pip_name) in &module_hints {
                    if out_lower
                        .contains(&format!("no module named '{}'", import_name.to_lowercase()))
                        || out_lower.contains(&format!(
                            "no module named \"{}\"",
                            import_name.to_lowercase()
                        ))
                    {
                        output.push_str(&format!(
                            "\n\nHINT: Install the missing module: pip install {}",
                            pip_name
                        ));
                        break;
                    }
                }
            }

            if out_lower.contains("python: command not found")
                || out_lower.contains("python: not found")
                || out_lower.contains("python: no such file")
            {
                output.push_str(
                    "\n\nHINT: `python` is not available. Try `python3` instead. \
                    Many containers only have python3 installed. \
                    You can check with: which python3",
                );
            }
            if out_lower.contains("python3: command not found")
                || out_lower.contains("python3: not found")
            {
                output.push_str(
                    "\n\nHINT: `python3` is not available. Try `python` instead. \
                    Check with: which python",
                );
            }

            if cmd_lower.contains("git")
                && (out_lower.contains("merge conflict") || out_lower.contains("conflict"))
                && (out_lower.contains("<<<<<")
                    || (out_lower.contains("=====") && out_lower.contains(">>>>>")))
            {
                output.push_str(
                    "\n\nHINT: Git merge conflict detected. To resolve: \
                        1) Open the conflicted files and remove conflict markers (<<<<<<, ======, >>>>>>), \
                        2) Keep the correct version of the code, \
                        3) git add <resolved-files>, then git commit.",
                );
            }

            if cmd_lower.contains("gcc") || cmd_lower.contains("g++") || cmd_lower.contains("make")
            {
                if out_lower.contains("undefined reference") {
                    output.push_str(
                        "\n\nHINT: Linker error (undefined reference). \
                        You may need to add -l flags (e.g., -lm for math, -lpthread for threads) \
                        or ensure all source files are included in the compile command.",
                    );
                }
                if out_lower.contains("fatal error:") && out_lower.contains(".h: no such file") {
                    output.push_str(
                        "\n\nHINT: Missing header file. Install the dev package: \
                        apt-get install lib<name>-dev (e.g., libssl-dev, libffi-dev)",
                    );
                }
            }

            if cmd_lower.contains("pytest") || cmd_lower.contains("python -m pytest") {
                let cmd_trimmed = command.trim().to_lowercase();
                if (cmd_trimmed.contains("-k \"not ")
                    || cmd_trimmed.contains("-k 'not ")
                    || cmd_trimmed.contains("-k=\"not ")
                    || cmd_trimmed.contains("-k='not ")
                    || cmd_trimmed.contains("--ignore"))
                    && !out_lower.contains("error")
                {
                    output.push_str(
                        "\n\nWARNING: You excluded some tests from the run. \
                        ALL tests must pass for the task to be complete. \
                        Do NOT skip failing tests — fix the code so they pass.",
                    );
                }
            }

            if out_lower.contains("externally-managed-environment")
                || out_lower.contains("pep 668")
                || out_lower.contains("error: externally-managed-environment")
            {
                output.push_str(
                    "\n\nHINT: PEP 668 blocks pip install to system Python. \
                    Use: pip install --break-system-packages <package>",
                );
            }

            if output.contains("timed out") {
                let net_commands = [
                    "git clone",
                    "curl",
                    "wget",
                    "apt-get",
                    "pip install",
                    "npm install",
                ];
                if net_commands.iter().any(|nc| command.contains(nc)) {
                    output.push_str(
                        "\n\nHINT: Network command timed out. If this keeps happening:\n\
                        1. Check if files already exist: `ls /app/`\n\
                        2. Try an alternative download method (curl vs wget vs git)\n\
                        3. Use `timeout 60` prefix to fail faster\n\
                        4. If ALL network fails, work with local files only",
                    );
                }
            }
        }
    }

    if tool_name == "edit_file" || tool_name == "Edit" {
        let out_lower = output.to_lowercase();
        if out_lower.contains("not found")
            || out_lower.contains("no match")
            || out_lower.contains("could not find")
        {
            let file_path = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("the file");
            output.push_str(&format!(
                "\n\nFIX: The old_string doesn't match the actual file content. Do:\n\
                1. read_file {} to see the EXACT current text\n\
                2. Copy the exact text you want to replace (including whitespace/indentation)\n\
                3. Use edit_file again with the exact text as old_string\n\
                TIP: Use grep to find the exact line, then read_file with offset/limit to get the precise text.",
                file_path
            ));
        }
    }

    if tool_name == "write_file" || tool_name == "Write" {
        let out_lower = output.to_lowercase();
        if !out_lower.contains("error")
            && !out_lower.contains("failed")
            && !out_lower.contains("denied")
        {
            let file_path = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if file_path.ends_with(".py")
                || file_path.ends_with(".pyx")
                || file_path.ends_with(".pxd")
            {
                output.push_str(
                    "\n\nREMINDER: File written successfully. Now verify it works: \
                    run `python -c \"import <module>\"` or the test command from the task.",
                );
            }
        }
    }

    if tool_name == "grep" || tool_name == "Grep" {
        let no_results = output.contains("no matches")
            || output.contains("No files found")
            || output.contains("0 matches")
            || output.trim().is_empty();
        if no_results {
            let pattern = args
                .get("pattern")
                .or_else(|| args.get("regex"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            output.push_str(&format!(
                "\n\nTIP: grep found nothing for '{}'. Try: 1) Use ignore_case: true, \
                2) Broaden the pattern (shorter string or regex), 3) Check the file extension filter, \
                4) Use glob to verify the files exist first.",
                pattern
            ));
        }
    }

    if tool_name == "read_file" || tool_name == "Read" {
        let line_count = output.lines().count();
        let has_offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) > 0;
        let has_limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) > 0;
        if line_count > 300 && !has_offset && !has_limit {
            output.push_str(
                "\n\nTIP: This is a large file. For future edits, use grep to find the relevant \
                function/line, then read_file with offset and limit to read only the section you need. \
                This saves context window space and avoids truncation.",
            );
        }
    }

    output
}

pub fn enrich_tool_output(_tool_name: &str, output: &str) -> String {
    if let Some(hint) = hints::get_tool_error_hint("", output) {
        format!("{}\n\n{}", output, hint)
    } else {
        output.to_string()
    }
}

pub fn enrich_tool_output_with_args(tool_name: &str, tool_json: &str, output: &str) -> String {
    let args: serde_json::Value = match serde_json::from_str(tool_json) {
        Ok(v) => v,
        Err(_) => return enrich_tool_output(tool_name, output),
    };

    let mut output = output.to_string();

    if tool_name == "bash" {
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            let cmd_lower = command.to_lowercase();
            let out_lower = output.to_lowercase();

            if (out_lower.contains("has no attribute 'int'")
                || out_lower.contains("has no attribute 'float'")
                || out_lower.contains("has no attribute 'complex'")
                || out_lower.contains("has no attribute 'bool'")
                || out_lower.contains("has no attribute 'str'"))
                && (out_lower.contains("numpy") || out_lower.contains("np."))
            {
                output.push_str(
                    "\n\nHINT: This is a NumPy 2.0 deprecation error. Search ALL source files \
                    for the deprecated pattern and fix ALL occurrences.",
                );
            }

            if out_lower.contains("command timed out") {
                output.push_str(
                    "\n\nHINT: The command timed out. Try: 1) Break into smaller steps, \
                    2) Use simpler flags, 3) Pre-install dependencies, 4) Run in background.",
                );
            }

            if out_lower.contains("no space left") || out_lower.contains("disk full") {
                output.push_str(
                    "\n\nHINT: Disk is full. Free space FIRST: rm -rf build/ dist/ *.egg-info __pycache__ .cache/",
                );
            }

            if out_lower.contains("python: command not found")
                || out_lower.contains("python: not found")
            {
                output.push_str("\n\nHINT: `python` is not available. Try `python3` instead.");
            }
            if out_lower.contains("python3: command not found")
                || out_lower.contains("python3: not found")
            {
                output.push_str("\n\nHINT: `python3` is not available. Try `python` instead.");
            }

            if out_lower.contains("externally-managed-environment") {
                output.push_str(
                    "\n\nHINT: PEP 668 blocks pip install to system Python. Use: pip install --break-system-packages",
                );
            }

            if cmd_lower.contains("build_ext") && !out_lower.contains("error") {
                output.push_str(
                    "\n\nREMINDER: build_ext completed. You MUST also run `pip install -e .` \
                    to install the package so it can be imported system-wide.",
                );
            }
        }
    }

    if tool_name == "edit_file" {
        let out_lower = output.to_lowercase();
        if out_lower.contains("not found") || out_lower.contains("no match") {
            let file_path = args
                .get("path")
                .or_else(|| args.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("the file");
            output.push_str(&format!(
                "\n\nFIX: The old_string doesn't match. read_file {} to see exact current text, then retry.",
                file_path
            ));
        }
    }

    let hint_cmd = if tool_name == "bash" {
        args.get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        tool_json.to_string()
    };
    if let Some(hint) = hints::get_tool_error_hint(&hint_cmd, &output) {
        output.push_str(&format!("\n\n{}", hint));
    }

    output
}

pub fn summarize_tool_args_for(tool_name: &str, _output: &str) -> String {
    format!("{} args", tool_name)
}
