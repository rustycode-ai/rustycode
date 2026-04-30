/// Heuristic-based hints for common command-line and language errors.
pub fn get_tool_error_hint(command: &str, output: &str) -> Option<String> {
    let _cmd_lower = command.to_lowercase();
    let out_lower = output.to_lowercase();

    // Command timeout — common in QEMU-emulated environments
    if out_lower.contains("command timed out") || out_lower.contains("timed out after") {
        return Some(
            "HINT: The command timed out. This is common in slow/emulated environments. \
            Try: 1) Break into smaller steps, 2) Install deps separately before building, \
            3) Use simpler build flags, 4) Run build in background with `&` and poll."
                .to_string(),
        );
    }

    // Python / Cython hints
    if out_lower.contains(".pyx") && out_lower.contains("attributeerror") {
        return Some("HINT: The error is in a Cython (.pyx) file. After editing .pyx source files, \
            you MUST rebuild: run \"python setup.py build_ext --inplace\" then \"pip install -e .\" \
            to recompile the extension.".to_string());
    }

    // NumPy deprecation hint
    if (out_lower.contains("has no attribute 'int'")
        || out_lower.contains("has no attribute 'float'")
        || out_lower.contains("has no attribute 'complex'")
        || out_lower.contains("has no attribute 'bool'")
        || out_lower.contains("has no attribute 'str'"))
        && (out_lower.contains("numpy") || out_lower.contains("np."))
    {
        return Some("HINT: This is a NumPy 2.0 deprecation error. You MUST search and fix ALL source files \
            — both .py AND .pyx/.pxd files. Common unfixed locations: spacecurve.py, named.py, __init__.py. \
            Use: grep -rn \"np\\.\\(float\\|int\\|complex\\|bool\\)[^0-9_]\" . \
            Replace np.float → float, np.int → int, np.complex → complex, np.bool → bool. \
            After fixing ALL files, rebuild: \"python setup.py build_ext --inplace && pip install -e .\" \
            Then verify: python -c \"from pyknotid.spacecurves import Knot\"".to_string());
    }

    // Missing build dependencies
    if out_lower.contains("no module named 'setuptools'")
        || out_lower.contains("no module named 'cython'")
        || out_lower.contains("modulenotfounderror: no module named 'setuptools'")
        || out_lower.contains("modulenotfounderror: no module named 'cython'")
        || out_lower.contains("command not found: cython")
        || out_lower.contains("error: command 'cython' failed")
        || out_lower.contains("unable to find 'cython'")
    {
        return Some(
            "HINT: Missing Python build dependency. Install with: \
            pip install setuptools wheel cython \
            Then retry the build command."
                .to_string(),
        );
    }

    // Missing module hints
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
            ("fastapi", "fastapi"),
            ("pandas", "pandas"),
            ("numpy", "numpy"),
            ("torch", "torch"),
            ("tensorflow", "tensorflow"),
            ("dotenv", "python-dotenv"),
            ("aiohttp", "aiohttp"),
            ("httpx", "httpx"),
        ];
        for (import_name, pip_name) in &module_hints {
            if out_lower.contains(&format!("no module named '{}'", import_name.to_lowercase()))
                || out_lower.contains(&format!(
                    "no module named \"{}\"",
                    import_name.to_lowercase()
                ))
            {
                return Some(format!(
                    "HINT: Install the missing module: pip install {}",
                    pip_name
                ));
            }
        }
    }

    // Rust cargo errors
    if out_lower.contains("cargo ") && out_lower.contains("error") {
        if out_lower.contains("could not find") && out_lower.contains("in crate") {
            return Some(
                "HINT: Missing Rust dependency. Add it to Cargo.toml: \
                check the exact crate name on crates.io and add it under [dependencies]."
                    .to_string(),
            );
        }
        if out_lower.contains("multiple matching crates") {
            return Some(
                "HINT: Ambiguous crate name. Use the full crate path or \
                check crates.io for the exact package name."
                    .to_string(),
            );
        }
        if out_lower.contains("linker")
            && (out_lower.contains("not found") || out_lower.contains("failed"))
        {
            return Some("HINT: Linker error — usually a missing C library. On Ubuntu/Debian: \
                apt install build-essential pkg-config libssl-dev. On macOS: xcode-select --install.".to_string());
        }
    }

    // npm/Node.js errors
    if out_lower.contains("npm err!") || out_lower.contains("npm error") {
        if out_lower.contains("eacces") || out_lower.contains("permission denied") {
            return Some(
                "HINT: npm permission error. Fix with: \
                npm config set prefix ~/.npm-global && export PATH=~/.npm-global/bin:$PATH. \
                Do NOT use sudo npm install."
                    .to_string(),
            );
        }
        if out_lower.contains("enoent") && out_lower.contains("package.json") {
            return Some(
                "HINT: No package.json found. Run 'npm init -y' first, or cd to the project root."
                    .to_string(),
            );
        }
    }

    // Docker errors
    if out_lower.contains("docker") && out_lower.contains("error") {
        if out_lower.contains("permission denied") {
            return Some(
                "HINT: Docker permission error. Add user to docker group: \
                sudo usermod -aG docker $USER && newgrp docker. Or use 'sudo docker'."
                    .to_string(),
            );
        }
        if out_lower.contains("no such image") || out_lower.contains("not found") {
            return Some(
                "HINT: Docker image not found locally. Pull it first: docker pull <image>"
                    .to_string(),
            );
        }
    }

    // TypeScript errors
    if out_lower.contains("tsc") && out_lower.contains("error") && out_lower.contains("ts") {
        return Some(
            "HINT: TypeScript compilation error. Check for: \
            1) Missing type definitions (npm install -D @types/node), \
            2) Incorrect import paths, \
            3) Strict mode violations. Run 'npx tsc --noEmit' for details."
                .to_string(),
        );
    }

    // Git merge conflict
    if out_lower.contains("merge conflict") || out_lower.contains("concurrent modification") {
        return Some(
            "HINT: Merge conflict detected. Resolve by: \
            1) Open conflicting files (search for <<<<<<< markers), \
            2) Choose the correct code section, \
            3) Remove conflict markers, \
            4) git add the resolved files, \
            5) git commit to complete the merge."
                .to_string(),
        );
    }

    // C compiler errors
    if out_lower.contains("gcc") && out_lower.contains("error") {
        if out_lower.contains("implicit declaration") {
            return Some(
                "HINT: Implicit function declaration — missing #include. \
                Add the appropriate header (e.g., #include <stdlib.h> for malloc, \
                #include <string.h> for strlen)."
                    .to_string(),
            );
        }
        if out_lower.contains("undefined reference") {
            return Some(
                "HINT: Undefined reference — linker can't find a function. \
                Check: 1) Function name spelling, 2) Library linking (-l flag), \
                3) Source file compilation order."
                    .to_string(),
            );
        }
    }

    // Permission errors
    if out_lower.contains("permission denied")
        && !out_lower.contains("docker")
        && !out_lower.contains("npm")
        && (out_lower.contains(".sh") || out_lower.contains("script"))
    {
        return Some("HINT: Script not executable. Run: chmod +x <script>.sh".to_string());
    }

    // Port already in use
    if out_lower.contains("address already in use")
        || out_lower.contains("port") && out_lower.contains("in use")
    {
        return Some("HINT: Port already in use. Find the process: lsof -i :<port> or ss -tlnp | grep <port>. \
            Kill it: kill <PID>. Or use a different port.".to_string());
    }

    // SSH/git server errors
    if out_lower.contains("connection refused")
        && (out_lower.contains("git") || out_lower.contains("ssh"))
    {
        return Some(
            "HINT: SSH/git connection refused. Check: 1) sshd is running (service ssh status), \
            2) The service is listening on the expected port, \
            3) No firewall blocking. Start sshd: service ssh start"
                .to_string(),
        );
    }

    // Patch file created but not applied
    if out_lower.contains("wrote")
        && out_lower.contains(".patch")
        && !command.contains("patch ")
        && !command.contains("git apply")
    {
        return Some(
            "HINT: You created a patch file but didn't apply it. Use: patch -p1 < file.patch \
            or: git apply file.patch"
                .to_string(),
        );
    }

    // Build/setup.py errors
    if out_lower.contains("setup.py") && out_lower.contains("error") {
        if out_lower.contains("no module named 'cython'") || out_lower.contains("cython' failed") {
            return Some(
                "HINT: Cython is required for building. Install: pip install cython \
                Then retry: python setup.py build_ext --inplace"
                    .to_string(),
            );
        }
        if out_lower.contains("numpy")
            && (out_lower.contains("deprecated") || out_lower.contains("attributeerror"))
        {
            return Some("HINT: NumPy compatibility error. Fix .pyx and .pxd files: \
                grep -rn 'np\\.(float\\|int\\|complex\\|bool)[^0-9_]' . and replace with Python builtins. \
                Then rebuild: python setup.py build_ext --inplace && pip install -e .".to_string());
        }
    }

    // Python indentation/syntax errors — very common in LLM-generated code
    if out_lower.contains("indentationerror") {
        return Some(
            "HINT: IndentationError — Python requires consistent indentation. \
            Check for: 1) Mixed tabs and spaces, 2) Missing/wrong indentation level, \
            3) Copy-paste introducing wrong whitespace. Use: cat -A file.py | head -50 to see hidden chars."
                .to_string(),
        );
    }

    // pip install failures
    if out_lower.contains("pip") && out_lower.contains("error") {
        if out_lower.contains("no matching distribution") {
            return Some(
                "HINT: Package not found. Check: 1) Package name spelling, \
                2) Python version compatibility, 3) Try with --no-build-isolation. \
                For pre-built wheels: pip install --only-binary :all: <package>"
                    .to_string(),
            );
        }
        if out_lower.contains("subprocess-exited-with-error")
            || out_lower.contains("failed building wheel")
        {
            return Some(
                "HINT: Wheel build failed. Try: 1) pip install --no-build-isolation <package>, \
                2) Install build deps first: pip install setuptools wheel cython, \
                3) Use an older version: pip install <package>==<version>"
                    .to_string(),
            );
        }
    }

    // Go compilation errors
    if out_lower.contains("go:") && out_lower.contains("error") {
        if out_lower.contains("cannot find package") || out_lower.contains("no required module") {
            return Some(
                "HINT: Missing Go dependency. Run: go mod tidy or go get <package>@latest"
                    .to_string(),
            );
        }
        if out_lower.contains("syntax error") && out_lower.contains("unexpected") {
            return Some(
                "HINT: Go syntax error. Common causes: 1) Missing comma in struct literal, \
                2) Unused import, 3) Wrong variable declaration (:= vs =)"
                    .to_string(),
            );
        }
    }

    // CMake errors
    if out_lower.contains("cmake")
        && out_lower.contains("error")
        && out_lower.contains("could not find")
    {
        return Some(
            "HINT: CMake can't find a dependency. Install the dev package: \
            apt install lib<name>-dev (Debian) or brew install <name> (macOS). \
            Or set -D<NAME>_ROOT=/path/to/lib"
                .to_string(),
        );
    }

    // make errors
    if out_lower.contains("make:") && out_lower.contains("error") {
        if out_lower.contains("missing separator") {
            return Some(
                "HINT: Makefile syntax error — likely a tab vs spaces issue. \
                Makefiles REQUIRE tabs for recipe lines, not spaces. Use: cat -A Makefile to check."
                    .to_string(),
            );
        }
        if out_lower.contains("no rule to make target") {
            return Some(
                "HINT: Make can't find a target. Check: 1) File paths are correct, \
                2) You're in the right directory, 3) The Makefile exists"
                    .to_string(),
            );
        }
    }

    // Java errors
    if out_lower.contains("javac") && out_lower.contains("error") {
        return Some(
            "HINT: Java compilation error. Check: 1) Class name matches file name, \
            2) Import statements are correct, 3) Package declaration matches directory structure"
                .to_string(),
        );
    }

    // pytest specific failures
    if out_lower.contains("pytest") && out_lower.contains("failed") {
        if out_lower.contains("fixture") && out_lower.contains("not found") {
            return Some(
                "HINT: pytest fixture not found. The fixture may be in conftest.py — \
                check if conftest.py exists and defines the fixture. \
                Run: grep -rn 'def <fixture_name>' ."
                    .to_string(),
            );
        }
        if out_lower.contains("assert") {
            return Some(
                "HINT: pytest assertion failure. Look at the DIFF in the test output — \
                it shows expected vs actual values. Fix the code to match what the test expects."
                    .to_string(),
            );
        }
    }

    // disk space / memory errors
    if out_lower.contains("no space left on device") {
        return Some(
            "HINT: Disk full. Free space: 1) pip cache purge, 2) rm -rf ~/.cache/pip, \
            3) apt clean, 4) Remove large temp files: find /tmp -size +100M -delete"
                .to_string(),
        );
    }
    if out_lower.contains("cannot allocate memory") || out_lower.contains("out of memory") {
        return Some(
            "HINT: Out of memory. Try: 1) Reduce data size or batch size, \
            2) Use a smaller model or fewer workers, 3) Add swap: fallocate -l 2G /swapfile && mkswap /swapfile && swapon /swapfile"
                .to_string(),
        );
    }

    // Network/download errors
    if out_lower.contains("connection timed out") || out_lower.contains("could not resolve host") {
        return Some(
            "HINT: Network error. If running in a container/VM: 1) Check DNS, \
            2) Try a mirror URL, 3) Use offline packages if available, \
            4) Retry the command — transient network issues are common"
                .to_string(),
        );
    }

    // Encoding errors in Python
    if out_lower.contains("unicodeencodeerror") || out_lower.contains("unicodedecodeerror") {
        return Some(
            "HINT: Unicode encoding error. Fix: 1) Open files with encoding='utf-8', \
            2) Use .encode('utf-8', errors='replace') for output, \
            3) Set PYTHONIOENCODING=utf-8"
                .to_string(),
        );
    }

    // File not found in Python imports (common: wrong relative import)
    if out_lower.contains("modulenotfounderror") && out_lower.contains(".") {
        return Some(
            "HINT: Python relative import failed. Check: 1) __init__.py exists in the package, \
            2) The import path matches the directory structure, \
            3) You may need to install the package: pip install -e ."
                .to_string(),
        );
    }

    // Python RecursionError — common in recursive algorithms
    if out_lower.contains("recursionerror") || out_lower.contains("maximum recursion depth") {
        return Some(
            "HINT: Recursion depth exceeded. Fix: 1) Add a base case, \
            2) Use sys.setrecursionlimit(N) if legitimate deep recursion, \
            3) Rewrite as iterative (using a stack/loop) if recursion is too deep"
                .to_string(),
        );
    }

    // Python KeyError — common in dict access
    if out_lower.contains("keyerror") {
        return Some(
            "HINT: KeyError — accessing a dict key that doesn't exist. \
            Use dict.get(key, default) instead of dict[key], or check 'if key in dict' first."
                .to_string(),
        );
    }

    // Python IndexError — common in list/string access
    if out_lower.contains("indexerror") {
        return Some(
            "HINT: IndexError — accessing an index out of range. \
            Check the length before accessing: if len(list) > i, or use try/except."
                .to_string(),
        );
    }

    // Python TypeError — common type mismatches
    if out_lower.contains("typeerror") && out_lower.contains("not iterable") {
        return Some(
            "HINT: TypeError 'not iterable' — trying to iterate over None or a scalar. \
            Check the variable is not None before iterating: if var is not None."
                .to_string(),
        );
    }

    // Rust cargo build errors
    if out_lower.contains("cargo ") && out_lower.contains("error") {
        if out_lower.contains("mismatched types") || out_lower.contains("expected") {
            return Some(
                "HINT: Rust type mismatch. Check the expected vs found types in the error. \
                Common fixes: use .to_string(), .into(), as usize, or add type annotations."
                    .to_string(),
            );
        }
        if out_lower.contains("cannot find value") || out_lower.contains("not found in this scope")
        {
            return Some(
                "HINT: Rust variable not in scope. Check: 1) Variable is defined before use, \
                2) Correct spelling, 3) Variable isn't shadowed or moved"
                    .to_string(),
            );
        }
    }

    // Node.js / npm specific errors
    if out_lower.contains("npm err!")
        && (out_lower.contains("eexist") || out_lower.contains("already exists"))
    {
        return Some(
            "HINT: npm file conflict. Try: rm -rf node_modules package-lock.json && npm install"
                .to_string(),
        );
    }

    // Segfault / core dump — common in C/C++ and native extensions
    if out_lower.contains("segmentation fault")
        || out_lower.contains("segmentation fault (core dumped)")
    {
        return Some(
            "HINT: Segmentation fault — memory access violation. Common causes: \
            1) Null pointer dereference, 2) Array out of bounds, 3) Use-after-free, \
            4) Stack overflow from infinite recursion. Add bounds checks and null checks."
                .to_string(),
        );
    }

    // Compiler warnings treated as errors
    if out_lower.contains("-werror") && out_lower.contains("error:") {
        return Some(
            "HINT: A compiler warning was treated as error (-Werror). \
            Fix the WARNING itself (unused variable, implicit conversion, etc.), \
            or remove -Werror from the build flags if not required."
                .to_string(),
        );
    }

    // Python 3.12+ removed `imp` module
    if out_lower.contains("module 'imp' is deprecated")
        || out_lower.contains("cannot import name 'imp'")
        || out_lower.contains("no module named 'imp'")
    {
        return Some(
            "HINT: The `imp` module was removed in Python 3.12. \
            Replace `import imp` with `import importlib`. \
            Replace `imp.load_source('name', 'path')` with `importlib.util.spec_from_file_location`."
                .to_string(),
        );
    }

    // Python `has no attribute` — common in LLM-generated code referencing wrong API
    if out_lower.contains("attributeerror") && out_lower.contains("has no attribute") {
        return Some(
            "HINT: AttributeError — the object doesn't have that attribute/method. \
            Check: 1) The object's actual type (print(type(x))), \
            2) API docs for the correct attribute name, \
            3) Whether you need to call a method vs access a property."
                .to_string(),
        );
    }

    // C++ linker errors
    if (out_lower.contains("undefined reference to") || out_lower.contains("cannot find -l"))
        && (out_lower.contains("g++") || out_lower.contains("gcc") || out_lower.contains("ld:"))
    {
        return Some(
            "HINT: C++ linker error — function or library not found. \
            Check: 1) Function is defined (not just declared), \
            2) Source file with the definition is compiled and linked, \
            3) Library is installed: apt install lib<name>-dev, \
            4) Add -l<name> to the compile command."
                .to_string(),
        );
    }

    // C++ compilation errors with template/STL issues
    if out_lower.contains("no matching function for call to")
        || (out_lower.contains("template") && out_lower.contains("error"))
    {
        return Some(
            "HINT: C++ template error. Check: 1) Include correct headers (<algorithm>, <vector>, etc.), \
            2) Template argument types match the function signature, \
            3) Use auto or explicit type annotations to help the compiler."
                .to_string(),
        );
    }

    // Python SyntaxWarning (Python 3.12+ strict mode)
    if out_lower.contains("syntaxwarning")
        || (out_lower.contains("syntax")
            && out_lower.contains("warning")
            && out_lower.contains("invalid"))
    {
        return Some(
            "HINT: Python SyntaxWarning — often from invalid escape sequences. \
            Use raw strings (r'...') for regex patterns, or double backslashes ('\\\\'). \
            This will become a SyntaxError in future Python versions."
                .to_string(),
        );
    }

    // Missing `bash` or `make` in container — common in minimal containers
    if (out_lower.contains("bash: ") && out_lower.contains("not found"))
        || (out_lower.contains("make: ")
            && out_lower.contains("not found")
            && !out_lower.contains("makefile"))
    {
        return Some(
            "HINT: A basic command is missing from this environment. \
            If in a minimal container, install with: apt-get update && apt-get install -y bash make \
            Or use an alternative: sh instead of bash, python3 for scripting."
                .to_string(),
        );
    }

    // Rust "mismatched types" — most common Rust error
    if out_lower.contains("mismatched types")
        || (out_lower.contains("expected")
            && out_lower.contains("found")
            && out_lower.contains("rustc"))
    {
        return Some(
            "HINT: Rust type mismatch. Common fixes: \
            1) .to_string() or .into() for String conversions, \
            2) *val to dereference &T to T, \
            3) &val or &mut val to borrow, \
            4) as usize / as i32 for numeric casts, \
            5) Add explicit type annotation: let x: Vec<u8> = ..."
                .to_string(),
        );
    }

    // Python subprocess.TimeoutExpired
    if out_lower.contains("timeoutexpired")
        || (out_lower.contains("subprocess") && out_lower.contains("timeout"))
    {
        return Some(
            "HINT: Subprocess timed out. Increase timeout or make the subprocess faster: \
            1) Add timeout parameter: subprocess.run(cmd, timeout=60), \
            2) Process data in smaller chunks, \
            3) Use streaming output instead of waiting for full output."
                .to_string(),
        );
    }

    // pytest errors with fixtures/conftest
    if out_lower.contains("fixture")
        && out_lower.contains("not found")
        && out_lower.contains("pytest")
    {
        return Some(
            "HINT: pytest fixture not found. Check: \
            1) conftest.py exists in the test directory, \
            2) Fixture name spelling matches the test's argument, \
            3) conftest.py is in the right directory (pytest searches upward). \
            Run: grep -rn 'def <fixture_name>' ."
                .to_string(),
        );
    }

    // Python `pkg_resources` deprecation warning
    if out_lower.contains("pkg_resources")
        && (out_lower.contains("deprecated") || out_lower.contains("warning"))
    {
        return Some(
            "HINT: pkg_resources is deprecated. Use importlib.resources or importlib.metadata instead. \
            If this is from a dependency, the warning can usually be ignored — it's not an error."
                .to_string(),
        );
    }

    // Awk/sed errors — common in text processing tasks
    if (out_lower.contains("awk: ") && out_lower.contains("error"))
        || (out_lower.contains("sed: ") && out_lower.contains("error"))
    {
        return Some(
            "HINT: awk/sed error. Common issues: \
            1) Unmatched quotes or delimiters — escape special chars, \
            2) GNU vs BSD syntax differences — macOS sed needs -E instead of -r, \
            3) Use python3 -c for complex text processing instead."
                .to_string(),
        );
    }

    // Missing C/C++ headers
    if out_lower.contains("fatal error:")
        && out_lower.contains("no such file or directory")
        && (out_lower.contains(".h:") || out_lower.contains(".hpp:"))
    {
        return Some(
            "HINT: Missing C/C++ header file. Install the dev package: \
            apt install lib<name>-dev (Debian) or brew install <name> (macOS). \
            Or add -I/path/to/include to the compiler command."
                .to_string(),
        );
    }

    // Concurrent file access / file locking issues
    if out_lower.contains("resource temporarily unavailable")
        || out_lower.contains("text file busy")
    {
        return Some(
            "HINT: File is locked or being written by another process. \
            Wait a moment and retry. If persistent: check for zombie processes (ps aux | grep), \
            kill stale processes, or use a unique temp file name."
                .to_string(),
        );
    }

    // Common in data science tasks: pandas/numpy shape mismatch
    if out_lower.contains("valueerror")
        && (out_lower.contains("shape")
            || out_lower.contains("dimension")
            || out_lower.contains("size"))
    {
        return Some(
            "HINT: Array/data shape mismatch. Check: \
            1) Print shapes: print(arr.shape), print(df.shape), \
            2) Ensure input dimensions match what the function expects, \
            3) Use .reshape() or .transpose() to fix dimensions."
                .to_string(),
        );
    }

    // zip/importlib error for .whl or .egg files
    if out_lower.contains("badzipfile") || out_lower.contains("zipimport") {
        return Some(
            "HINT: Corrupted or incompatible Python package archive. \
            Try: pip install --force-reinstall --no-cache-dir <package>"
                .to_string(),
        );
    }

    // Python `NotImplementedError` — common when abstract methods aren't overridden
    if out_lower.contains("notimplementederror") {
        return Some(
            "HINT: NotImplementedError — a method was called that isn't implemented. \
            Check if you need to override an abstract method in a subclass, \
            or if you're using the wrong base class."
                .to_string(),
        );
    }

    // sqlite3 errors — common in web app tasks
    if out_lower.contains("sqlite3.") && out_lower.contains("error") {
        return Some(
            "HINT: SQLite error. Common causes: \
            1) Table doesn't exist — run CREATE TABLE first, \
            2) Column mismatch — check column names and count, \
            3) Database is locked — close other connections, use WAL mode."
                .to_string(),
        );
    }

    // Python `pip install` externally-managed-environment (PEP 668)
    if out_lower.contains("externally-managed-environment") {
        return Some(
            "HINT: pip refuses to install into system Python. Use: \
            pip install --break-system-packages <package> \
            Or: uv pip install <package> \
            Or create a venv: python3 -m venv .venv && source .venv/bin/activate"
                .to_string(),
        );
    }

    // Python `uv` tool not found
    if out_lower.contains("uv: command not found") || out_lower.contains("uv: not found") {
        return Some(
            "HINT: `uv` is not installed. Install with: curl -LsSf https://astral.sh/uv/install.sh | sh \
            Or use pip3 instead: pip3 install --break-system-packages <package>"
                .to_string(),
        );
    }

    // Python version mismatch (e.g., f-string syntax, walrus operator in 3.7)
    if out_lower.contains("syntaxerror") && out_lower.contains("python 3") {
        return Some(
            "HINT: Syntax not supported in this Python version. \
            Check python3 --version. Avoid: walrus operator (3.8+), \
            match/case (3.10+), type hints with | (3.10+). \
            Use older syntax compatible with the available version."
                .to_string(),
        );
    }

    // edit_file "not found" — most common edit failure
    if out_lower.contains("old_string not found") || out_lower.contains("text was not found") {
        return Some(
            "HINT: edit_file failed because old_string doesn't match the current file. \
            The file may have been modified. Use read_file to see the exact current contents, \
            then retry with the exact text from the file. Pay attention to whitespace and indentation."
                .to_string(),
        );
    }

    // exit code 137 (OOM killed) — common in constrained environments
    if out_lower.contains("exit code: 137") || out_lower.contains("killed") {
        return Some(
            "HINT: Process was killed (likely OOM or timeout). \
            Reduce memory usage: 1) Process data in smaller chunks, \
            2) Use generators instead of lists, \
            3) Free variables with del, 4) Set a memory limit."
                .to_string(),
        );
    }

    // Python ModuleNotFoundError for common patterns
    if out_lower.contains("modulenotfounderror") {
        // Check for common import vs pip name mismatches not covered above
        let extra_hints = [
            ("skimage", "scikit-image"),
            ("cv2", "opencv-python-headless"),
            ("serial", "pyserial"),
            ("Crypto", "pycryptodome"),
            ("dateutil", "python-dateutil"),
            ("gi", "PyGObject"),
            ("igraph", "python-igraph"),
        ];
        for (import_name, pip_name) in &extra_hints {
            if out_lower.contains(&format!("'{import_name}'"))
                || out_lower.contains(&format!("\"{import_name}\""))
            {
                return Some(format!(
                    "HINT: Install with: pip3 install --break-system-packages {pip_name}"
                ));
            }
        }
    }

    // Python "No such file or directory" for script execution
    if out_lower.contains("no such file or directory") && out_lower.contains(".py") {
        return Some(
            "HINT: Python script file not found. Check: \
            1) Use ls or glob to find the correct file path, \
            2) The file may be in a subdirectory — use find . -name '*.py' | grep <keyword>, \
            3) If creating a new file, write_file first then run it."
                .to_string(),
        );
    }

    // Common benchmark pattern: output file format mismatch
    if out_lower.contains("assertionerror") || out_lower.contains("assertion failed") {
        return Some(
            "HINT: Test assertion failed. Look at the test output above for expected vs actual values. \
            Pay close attention to: 1) Exact whitespace/newlines, 2) Number formatting (decimals, precision), \
            3) String case (upper/lower), 4) Trailing spaces or newlines. Read the test file to see the exact assertion."
                .to_string(),
        );
    }

    // Python SyntaxError with specific common patterns
    if out_lower.contains("syntaxerror") {
        if out_lower.contains("unexpected eof") || out_lower.contains("unexpected end") {
            return Some(
                "HINT: Unexpected EOF — missing closing bracket, parenthesis, or quote. \
                Check for: 1) Unclosed parentheses (), 2) Unclosed strings, \
                3) Missing closing brackets [] or {}."
                    .to_string(),
            );
        }
        if out_lower.contains("invalid syntax") && out_lower.contains("->") {
            return Some(
                "HINT: Invalid syntax near '->'. This may be a Python version issue — \
                type hints using 'X | Y' require Python 3.10+. Use 'Union[X, Y]' instead."
                    .to_string(),
            );
        }
    }

    // Python TypeError with common patterns
    if out_lower.contains("typeerror") {
        if out_lower.contains("unsupported operand") {
            return Some(
                "HINT: Unsupported operand types — you're using an operator with incompatible types. \
                Check the variable types with print(type(x)). Common: str + int → use str(x) or int(x) to convert."
                    .to_string(),
            );
        }
        if out_lower.contains("argument must be") || out_lower.contains("expected") {
            return Some(
                "HINT: Wrong argument type passed to a function. Check the function's expected types \
                and convert as needed: str(), int(), float(), list(), etc."
                    .to_string(),
            );
        }
    }

    // Binary not executable / wrong architecture
    if out_lower.contains("cannot execute")
        || out_lower.contains("not executable")
        || out_lower.contains("exec format error")
    {
        return Some(
            "HINT: Binary can't execute — likely wrong architecture. \
            Check: file <binary> to see the architecture. \
            If it's x86 on ARM (or vice versa), you need to recompile from source \
            or find a compatible binary."
                .to_string(),
        );
    }

    // Python `ModuleNotFoundError` for internal project imports — the module exists
    // but isn't in the Python path. Common when the task creates a new .py file
    // but the import uses the wrong relative path.
    if out_lower.contains("modulenotfounderror") && !out_lower.contains("pip") {
        // If the error mentions a module that could be a local file
        if out_lower.contains("no module named '") || out_lower.contains("no module named \"") {
            return Some(
                "HINT: Module import failed. If this is a local module (not a pip package): \
                1) Check the file is in the correct directory, \
                2) Add __init__.py to make it a package, \
                3) Add sys.path.insert(0, '.') at the top of the script, \
                4) Or use: from <directory> import <module>"
                    .to_string(),
            );
        }
    }

    // Python `FileNotFoundError` for output files — task expects a specific output file
    if out_lower.contains("filenotfounderror")
        && (out_lower.contains("output")
            || out_lower.contains("result")
            || out_lower.contains("solution"))
    {
        return Some(
            "HINT: Expected output file not found. The task may require creating a specific output file. \
            Check the task description for the expected filename and create it with write_file."
                .to_string(),
        );
    }

    // Node.js / JavaScript module not found
    if out_lower.contains("cannot find module") && out_lower.contains("require") {
        return Some(
            "HINT: Node.js module not found. Install with: npm install <module_name>. \
            If it's a local file, check the relative path in require('./...')."
                .to_string(),
        );
    }

    // Python import of local module fails due to __init__.py missing
    if out_lower.contains("importerror") && out_lower.contains("__init__.py") {
        return Some(
            "HINT: Package __init__.py is missing or empty. Create it: touch <directory>/__init__.py"
                .to_string(),
        );
    }

    None
}
