//! Configuration Migration Framework
//!
//! A generic framework for running configuration file migrations, inspired by
//! goose's `config/migrations.rs`. Migrations run automatically on config load
//! and upgrade older config formats to newer versions.
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::config_migration::{MigrationRunner, Migration};
//!
//! let mut runner = MigrationRunner::new();
//! runner.register(Migration::new(
//!     "2026-01_add_default_provider",
//!     |config| {
//!         if !config.contains_key("default_provider") {
//!             config.insert("default_provider".into(), "anthropic".into());
//!             return true; // changed
//!         }
//!         false
//!     },
//! ));
//!
//! let changed = runner.run(&mut config_map);
//! if changed {
//!     // Save updated config
//! }
//! ```

use serde_yaml::Mapping;
use std::fmt;

/// A single migration that transforms a config mapping.
pub struct Migration {
    /// Unique name for this migration (use date prefix for ordering)
    pub name: &'static str,
    /// The migration function. Returns `true` if the config was changed.
    pub migrate: fn(&mut Mapping) -> bool,
}

impl fmt::Debug for Migration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Migration")
            .field("name", &self.name)
            .finish()
    }
}

impl Migration {
    /// Create a new migration with a name and transform function.
    pub fn new(name: &'static str, migrate: fn(&mut Mapping) -> bool) -> Self {
        Self { name, migrate }
    }

    /// Run this migration on a config mapping.
    pub fn run(&self, config: &mut Mapping) -> bool {
        (self.migrate)(config)
    }
}

/// Runner that executes a series of migrations in order.
///
/// Migrations are run once — the runner tracks which migrations have
/// already been applied by checking a `_migrations` key in the config.
pub struct MigrationRunner {
    migrations: Vec<Migration>,
}

impl Default for MigrationRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl MigrationRunner {
    pub const fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Register a migration. Migrations run in registration order.
    pub fn register(&mut self, migration: Migration) {
        self.migrations.push(migration);
    }

    /// Run all pending migrations on a config mapping.
    ///
    /// Returns `true` if any migration made changes.
    pub fn run(&self, config: &mut Mapping) -> MigrationReport {
        let applied = self.get_applied_migrations(config);
        let mut report = MigrationReport::default();

        for migration in &self.migrations {
            if applied.contains(migration.name) {
                continue;
            }

            let changed = migration.run(config);
            if changed {
                report.migrations_run.push(migration.name.to_string());
                report.changed = true;
            }

            // Mark as applied regardless of whether it changed
            self.mark_applied(config, migration.name);
        }

        report
    }

    /// Get the set of already-applied migration names.
    fn get_applied_migrations(&self, config: &Mapping) -> std::collections::HashSet<String> {
        let key = serde_yaml::Value::String("_migrations".to_string());

        config
            .get(&key)
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Mark a migration as applied in the config.
    fn mark_applied(&self, config: &mut Mapping, name: &str) {
        let key = serde_yaml::Value::String("_migrations".to_string());

        let mut applied: Vec<serde_yaml::Value> = config
            .get(&key)
            .and_then(|v| v.as_sequence().cloned())
            .unwrap_or_default();

        applied.push(serde_yaml::Value::String(name.to_string()));
        config.insert(key, serde_yaml::Value::Sequence(applied));
    }

    /// Get count of registered migrations.
    pub const fn migration_count(&self) -> usize {
        self.migrations.len()
    }

    /// Get names of all registered migrations.
    pub fn migration_names(&self) -> Vec<&'static str> {
        self.migrations.iter().map(|m| m.name).collect()
    }
}

/// Report from running migrations.
#[derive(Debug, Default)]
pub struct MigrationReport {
    /// Whether any migration made changes
    pub changed: bool,
    /// Names of migrations that ran and made changes
    pub migrations_run: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    fn yaml_string(s: &str) -> Value {
        Value::String(s.to_string())
    }

    #[test]
    fn test_migration_runs_and_marks_applied() {
        let mut runner = MigrationRunner::new();
        runner.register(Migration::new("2026-01_add_default_provider", |config| {
            if !config.contains_key(yaml_string("default_provider")) {
                config.insert(yaml_string("default_provider"), yaml_string("anthropic"));
                return true;
            }
            false
        }));

        let mut config = Mapping::new();
        let report = runner.run(&mut config);

        assert!(report.changed);
        assert_eq!(report.migrations_run.len(), 1);
        assert_eq!(
            config.get(yaml_string("default_provider")),
            Some(&yaml_string("anthropic"))
        );

        // Should be marked as applied
        let applied = runner.get_applied_migrations(&config);
        assert!(applied.contains("2026-01_add_default_provider"));
    }

    #[test]
    fn test_migration_not_rerun_if_already_applied() {
        let mut runner = MigrationRunner::new();
        runner.register(Migration::new("test_migration", |config| {
            config.insert(yaml_string("test_key"), yaml_string("value"));
            true
        }));

        let mut config = Mapping::new();

        // First run
        let report1 = runner.run(&mut config);
        assert!(report1.changed);

        // Second run — should not re-run
        let report2 = runner.run(&mut config);
        assert!(!report2.changed);
    }

    #[test]
    fn test_multiple_migrations_run_in_order() {
        let mut runner = MigrationRunner::new();
        runner.register(Migration::new("001_first", |config| {
            config.insert(yaml_string("step"), yaml_string("first"));
            true
        }));
        runner.register(Migration::new("002_second", |config| {
            // Overwrite step to prove ordering
            config.insert(yaml_string("step"), yaml_string("second"));
            true
        }));

        let mut config = Mapping::new();
        let report = runner.run(&mut config);

        assert!(report.changed);
        assert_eq!(report.migrations_run.len(), 2);
        // Second migration ran last
        assert_eq!(
            config.get(yaml_string("step")),
            Some(&yaml_string("second"))
        );
    }

    #[test]
    fn test_migration_no_change_not_reported() {
        let mut runner = MigrationRunner::new();
        runner.register(Migration::new("noop", |_config| {
            false // No change
        }));

        let mut config = Mapping::new();
        let report = runner.run(&mut config);

        assert!(!report.changed);
        assert!(report.migrations_run.is_empty());

        // But it should still be marked as applied
        let applied = runner.get_applied_migrations(&config);
        assert!(applied.contains("noop"));
    }

    #[test]
    fn test_empty_runner_no_crash() {
        let runner = MigrationRunner::new();
        let mut config = Mapping::new();

        let report = runner.run(&mut config);
        assert!(!report.changed);
        assert_eq!(runner.migration_count(), 0);
    }

    #[test]
    fn test_migration_names() {
        let mut runner = MigrationRunner::new();
        runner.register(Migration::new("alpha", |_config| false));
        runner.register(Migration::new("beta", |_config| false));

        assert_eq!(runner.migration_names(), vec!["alpha", "beta"]);
        assert_eq!(runner.migration_count(), 2);
    }

    #[test]
    fn test_idempotent_double_run() {
        let mut runner = MigrationRunner::new();
        runner.register(Migration::new("add_version", |config| {
            config.insert(yaml_string("version"), Value::Number(2.into()));
            true
        }));

        let mut config = Mapping::new();

        let r1 = runner.run(&mut config);
        assert!(r1.changed);

        let r2 = runner.run(&mut config);
        assert!(!r2.changed);

        // Config should still have version=2
        assert_eq!(
            config.get(yaml_string("version")),
            Some(&Value::Number(2.into()))
        );
    }
}
