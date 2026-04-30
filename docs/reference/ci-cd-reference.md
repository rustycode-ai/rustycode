# CI/CD Reference Guide

This document provides a quick reference for RustyCode's CI/CD infrastructure.

## Workflow Files

### Test Workflow (`.github/workflows/test.yml`)

**Triggers:**
- Push to `main` branch
- Pull requests to `main` branch

**Jobs:**

#### Rust Matrix
Tests across Rust versions: `stable`, `beta`, `nightly`

- Build workspace
- Run all tests
- Run provider v2-focused verification
- Run migration verification
- Run security scanning

#### Quality Checks
- Check formatting
- Run clippy
- Keep PR coverage focused on stable-only quality gates

### Benchmark Workflow (`.github/workflows/bench.yml`)

**Triggers:**
- Daily schedule (00:00 UTC)
- Manual workflow dispatch
- Pushes to `main` affecting crates/, benches/, or Cargo.toml

**Steps:**
- Validate benchmark manifest and script wiring
- Compile all benchmark targets on PRs and pushes
- Run performance regression checks on PRs
- Run the full benchmark suite on `main`, schedule, and manual dispatch

### Documentation Workflow (`.github/workflows/docs.yml`)

**Steps:**
- Build workspace docs
- Run provider v2 doctests
- Check Markdown links
- Upload generated docs as an artifact

## Pre-commit Hooks

### Configuration (`.pre-commit-config.yaml`)

**Hooks:**
1. `cargo fmt` - Format code automatically
2. `cargo fmt --check` - Verify formatting
3. `cargo clippy` - Lint with warnings as errors
4. `cargo test` - Run test suite
5. `cargo doc` - Build documentation

### Installation

```bash
# Install pre-commit
brew install pre-commit  # macOS
pip install pre-commit   # Python

# Install hooks in repository
pre-commit install

# Run hooks manually on all files
pre-commit run --all-files

# Run hooks on staged files only
pre-commit run

# Uninstall hooks
pre-commit uninstall
```

### Skipping Hooks (Not Recommended)

```bash
# Skip for a single commit
git commit --no-verify -m "message"

# Skip for all commits in a session
export SKIP=RUST-FMT,RUST-CLIPPY,RUST-TEST
```

## CI Status Badges

Add to README.md:

```markdown
![Test](https://github.com/luengnat/rustycode/workflows/Test/badge.svg)
![Benchmarks](https://github.com/luengnat/rustycode/workflows/Benchmarks/badge.svg)
![Documentation](https://github.com/luengnat/rustycode/workflows/Documentation/badge.svg)
```

## Local Development

### Pre-commit Workflow

```bash
# 1. Make code changes
vim src/lib.rs

# 2. Stage changes
git add .

# 3. Pre-commit hooks run automatically
git commit -m "feat: add feature"
```

### Manual Verification

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run linter
cargo clippy --all-targets -- -D warnings

# Run tests
cargo test

# Build documentation
./scripts/check-docs.sh

# Run benchmark validation
./scripts/validate-benchmarks.sh
```

## Troubleshooting

### CI Fails But Tests Pass Locally

**Possible causes:**
1. Rust version mismatch - CI tests on stable/beta/nightly
2. Race conditions in tests - CI runs with different timing
3. Environment-specific code - OS differences

**Debug steps:**
```bash
# Test with specific Rust version
rustup install beta
rustup override set beta
cargo test

# Check for race conditions
cargo test -- --test-threads=1

# Run with verbose output
cargo test -- --nocapture --verbose
```

### Pre-commit Hooks Fail

**Format check fails:**
```bash
cargo fmt
git add .
git commit
```

**Clippy fails:**
```bash
# See all warnings
cargo clippy --all-targets

# Auto-fix some issues
cargo clippy --all-targets --fix
```

**Tests fail:**
```bash
# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests in package
cargo test -p rustycode-id
```

### Coverage Report Issues

If coverage generation fails:
```bash
# Install tarpaulin locally
cargo install cargo-tarpaulin

# Run locally to verify
./scripts/check-docs.sh
```

## Performance Benchmarks

### Running Locally

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench id_performance

# Save baseline
cargo bench --bench id_performance -- --save-baseline main

# Compare with baseline
./scripts/bench-compare.sh candidate main
```

### Benchmark Results

Results are stored in:
- `target/criterion/` - HTML reports
- `criterion/main/` - Baseline data

View HTML report:
```bash
open target/criterion/<bench-name>/report/index.html
```

## CI Configuration Details

### Caching Strategy

Three cache layers for faster builds:
1. **Cargo registry** - Downloaded crates
2. **Cargo index** - Git index of crates.io
3. **Build target** - Compiled artifacts

Cache keys include:
- OS (linux)
- Rust version (stable/beta/nightly)
- Cargo.lock hash

### Matrix Strategy

Testing across Rust versions catches:
- Forward compatibility issues (beta)
- Experimental features (nightly)
- Production stability (stable)

### Fail-Fast Disabled

CI continues testing all Rust versions even if one fails, providing complete feedback.

## Continuous Improvement

### Monitoring CI Health

Check workflow runs:
```bash
# List recent workflow runs
gh run list --workflow=ci.yml

# View specific run
gh run view <run-id>

# Watch logs in real-time
gh run watch
```

### Benchmark Regression Detection

Manual workflow triggers for testing:
```bash
# Trigger benchmark workflow via CLI
gh workflow run bench.yml

# Trigger with specific parameters
gh workflow run bench.yml -f rust_version=nightly
```

## Best Practices

1. **Always run pre-commit hooks locally** before pushing
2. **Fix clippy warnings** immediately, don't ignore them
3. **Write tests for new features** - CI requires all tests pass
4. **Document public APIs** - CI builds documentation
5. **Monitor benchmark trends** - performance regressions matter
6. **Keep dependencies updated** - CI tests across Rust versions help catch breakage

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Pre-commit Documentation](https://pre-commit.com/)
- [Criterion Benchmark Library](https://bheisler.github.io/criterion.rs/book/)
- [Cargo Tarpaulin](https://github.com/xd009642/tarpaulin)
- [Codecov Documentation](https://docs.codecov.com/)
