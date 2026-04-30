# Migration Automation Scripts

These scripts standardize migration verification, cleanup, rollback, coverage, benchmarking, and CI execution for RustyCode.

All reports and backups are written under `target/migration/` so repeated runs stay idempotent and do not overwrite source files without an explicit `--apply`.

## Prerequisites

- `bash`
- `cargo`
- `python3`
- Optional:
  - `cargo-tarpaulin` or `cargo-llvm-cov` for coverage
  - an existing Criterion baseline in `target/criterion/` for benchmark comparisons

## Main Scripts

### `verify_migration.sh`

Validates the sortable-ID migration by running targeted crate checks plus static assertions.

```bash
./scripts/verify_migration.sh
./scripts/verify_migration.sh --quick
./scripts/verify_migration.sh --strict
```

### `cleanup_legacy.sh`

Previews or removes deprecated files. Dry-run by default. When `--apply` is used, files are backed up and a rollback manifest is created.

```bash
./scripts/cleanup_legacy.sh
./scripts/cleanup_legacy.sh --apply --category backup-files
./scripts/cleanup_legacy.sh --apply --path ./some/custom/file
```

### `rollback.sh`

Restores files removed by `cleanup_legacy.sh` using the generated manifest.

```bash
./scripts/rollback.sh
./scripts/rollback.sh --backup-dir target/migration/backups/<timestamp> --apply
```

### `test_coverage.sh`

Generates a coverage report using `cargo-tarpaulin` or `cargo-llvm-cov`.

```bash
./scripts/test_coverage.sh
./scripts/test_coverage.sh --threshold 80
```

### `benchmark_comparison.sh`

Runs Criterion benchmarks, saves a candidate baseline, and compares against an existing baseline when available.

```bash
./scripts/benchmark_comparison.sh
./scripts/benchmark_comparison.sh --baseline release-previous --candidate migration-after
./scripts/benchmark_comparison.sh --capture-only --candidate before-migration
```

### `run_all_tests.sh`

Runs the local migration quality gate.

```bash
./scripts/run_all_tests.sh
./scripts/run_all_tests.sh --fast
./scripts/run_all_tests.sh --with-coverage
```

### `validate_configs.sh`

Checks workspace TOML files, security config, required scripts, and benchmark workflow references.

```bash
./scripts/validate_configs.sh
./scripts/validate_configs.sh --strict
```

### `generate_report.sh`

Builds a consolidated migration report from the latest artifacts.

```bash
./scripts/generate_report.sh
./scripts/generate_report.sh --refresh
```

### `ci_integration.sh`

Runs the automation stack in a CI-friendly mode and writes to `GITHUB_STEP_SUMMARY` when present.

```bash
./scripts/ci_integration.sh
./scripts/ci_integration.sh --mode github --with-coverage
```

## Compatibility Wrappers

The existing scripts below now forward into the new benchmark automation so older entrypoints keep working:

- `bench.sh`
- `bench-compare.sh`

## Output Layout

- Reports: `target/migration/reports/`
- Logs: `target/migration/logs/`
- Cleanup backups: `target/migration/backups/`
- Coverage artifacts: `target/migration/coverage/`

Each primary script also writes a `*_latest.md` report for easy CI pickup.
