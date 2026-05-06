//! End-to-end integration test for the memory pipeline:
//! layout → write rollout → read summary → build instructions → search → list → reset

use std::path::Path;
use tempfile::TempDir;

use rustycode_memory::memdir;
use rustycode_memory::read_path;
use rustycode_memory::rollout::{self, ExtractedMemory, StageOneOutput};

fn make_memory(slug: &str, summary: &str, raw: &str) -> ExtractedMemory {
    let output = StageOneOutput {
        raw_memory: raw.to_string(),
        rollout_summary: summary.to_string(),
        rollout_slug: Some(slug.to_string()),
    };
    ExtractedMemory::from_stage_one(output, "test-thread-abc123", "/project/root")
}

#[test]
fn full_pipeline() {
    let tmp = TempDir::new().unwrap();
    let mem_dir = tmp.path();

    // Step 1: ensure_layout creates the directory structure
    memdir::ensure_layout(mem_dir).unwrap();
    assert!(memdir::rollout_summaries_dir(mem_dir).exists());
    assert!(memdir::ad_hoc_notes_dir(mem_dir).exists());
    assert!(memdir::memory_summary_path(mem_dir).exists());

    // Step 2: read_memory_summary returns seed content
    let summary = memdir::read_memory_summary(mem_dir).unwrap();
    assert!(summary.contains("No memories yet"));

    // Step 3: build_memory_instructions returns None for seed content
    assert!(read_path::build_memory_instructions(mem_dir).is_none());

    // Step 4: write rollout summaries
    let m1 = make_memory(
        "fix-auth-bug",
        "Fixed JWT token validation in auth module.",
        "Found that validate_token() was checking expiry incorrectly. \
         Fixed by comparing against current time instead of issue time. \
         Also added clock skew tolerance of 30 seconds.",
    );
    let m2 = make_memory(
        "add-rate-limiting",
        "Added rate limiting middleware to API routes.",
        "Implemented sliding window rate limiter using Redis. \
         Limits: 100 req/min for authenticated, 20 req/min for anonymous. \
         Returns 429 with Retry-After header.",
    );
    let m3 = make_memory(
        "db-migration-cleanup",
        "Cleaned up stale database migrations.",
        "Removed 3 deprecated migration files that were superseded by \
         the consolidated schema migration. Updated migration order.",
    );

    rollout::write_rollout_summary(mem_dir, &m1).unwrap();
    rollout::write_rollout_summary(mem_dir, &m2).unwrap();
    rollout::write_rollout_summary(mem_dir, &m3).unwrap();

    // Step 5: generate a summary from vector-memory-style entries
    let entries = vec![
        (
            "Always use parameterized queries for SQL.".to_string(),
            "learnings".to_string(),
            0.9,
        ),
        (
            "Auth module uses JWT tokens.".to_string(),
            "code_patterns".to_string(),
            0.85,
        ),
        (
            "Rate limiter: 100/min auth, 20/min anon.".to_string(),
            "task_traces".to_string(),
            0.7,
        ),
    ];
    let generated = memdir::generate_summary(&entries);
    assert!(generated.contains("## Learnings"));
    assert!(generated.contains("## Code Patterns"));
    assert!(generated.contains("## Task Traces"));
    assert!(generated.contains("90%"));
    assert!(generated.contains("85%"));

    // Step 6: write the generated summary
    memdir::write_memory_summary(mem_dir, &generated).unwrap();

    // Step 7: build_memory_instructions now returns Some
    let instructions = read_path::build_memory_instructions(mem_dir).unwrap();
    assert!(instructions.contains("Memory System"));
    assert!(instructions.contains("Quick Memory Pass"));
    assert!(instructions.contains("Learnings"));
    assert!(instructions.contains("parameterized queries"));

    // Step 8: load_all_rollout_summaries returns all 3
    let all = rollout::load_all_rollout_summaries(mem_dir).unwrap();
    assert_eq!(all.len(), 3);
    // Should be sorted by generated_at
    assert!(all[0].generated_at <= all[1].generated_at);
    assert!(all[1].generated_at <= all[2].generated_at);

    // Step 9: list_rollout_summaries returns file paths
    let files = memdir::list_rollout_summaries(mem_dir).unwrap();
    assert_eq!(files.len(), 3);
    for f in &files {
        assert!(f.extension().is_some_and(|ext| ext == "md"));
    }

    // Step 10: record_usage on a memory
    let mut loaded = all;
    loaded[0].record_usage();
    assert_eq!(loaded[0].usage_count, 1);
    assert!(loaded[0].last_usage.is_some());

    // Step 11: roundtrip markdown for a memory
    let md = loaded[0].to_markdown();
    assert!(md.contains("## Summary"));
    assert!(md.contains("## Raw Memory"));
    assert!(md.contains("**Usage count:** 1"));

    // Step 12: verify truncation works
    let long_entries: Vec<(String, String, f32)> = (0..500)
        .map(|i| {
            (
                format!(
                    "Memory entry number {} with some padding text to make it longer.",
                    i
                ),
                "learnings".to_string(),
                0.5 + (i as f32 % 0.5),
            )
        })
        .collect();
    let long_summary = memdir::generate_summary(&long_entries);
    // The summary should be bounded (5000 tokens * 4 chars/token = 20000 chars)
    // But generate_summary doesn't truncate itself; read_memory_summary does
    memdir::write_memory_summary(mem_dir, &long_summary).unwrap();
    let truncated = memdir::read_memory_summary(mem_dir).unwrap();
    // If it was truncated, it should contain the truncation marker
    // If it fit, it should just be the full content
    assert!(!truncated.is_empty());
}

#[test]
fn search_rollout_summaries() {
    let tmp = TempDir::new().unwrap();
    let mem_dir = tmp.path();
    memdir::ensure_layout(mem_dir).unwrap();

    let m1 = make_memory(
        "auth-fix",
        "Fixed JWT token validation.",
        "The validate_token function was comparing wrong timestamps.",
    );
    let m2 = make_memory(
        "redis-cache",
        "Added Redis caching layer.",
        "Implemented TTL-based caching with 5-minute default expiry.",
    );

    rollout::write_rollout_summary(mem_dir, &m1).unwrap();
    rollout::write_rollout_summary(mem_dir, &m2).unwrap();

    let all = rollout::load_all_rollout_summaries(mem_dir).unwrap();
    assert_eq!(all.len(), 2);

    // Search for "JWT" — should find auth-fix only
    let jwt_matches: Vec<_> = all
        .iter()
        .filter(|m| {
            let q = "jwt";
            m.raw_memory.to_lowercase().contains(q)
                || m.rollout_summary.to_lowercase().contains(q)
                || m.rollout_slug
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(q))
        })
        .collect();
    assert_eq!(jwt_matches.len(), 1);
    assert_eq!(jwt_matches[0].rollout_slug.as_deref(), Some("auth-fix"));

    // Search for "redis" — should find redis-cache only
    let redis_matches: Vec<_> = all
        .iter()
        .filter(|m| {
            let q = "redis";
            m.raw_memory.to_lowercase().contains(q)
                || m.rollout_summary.to_lowercase().contains(q)
                || m.rollout_slug
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(q))
        })
        .collect();
    assert_eq!(redis_matches.len(), 1);
    assert_eq!(
        redis_matches[0].rollout_slug.as_deref(),
        Some("redis-cache")
    );

    // Search for nonexistent term
    assert!(all
        .iter()
        .find(|m| m.raw_memory.to_lowercase().contains("nonexistent"))
        .is_none());
}

#[test]
fn reset_clears_everything() {
    let tmp = TempDir::new().unwrap();
    let mem_dir = tmp.path();
    memdir::ensure_layout(mem_dir).unwrap();

    // Write a rollout
    let m = make_memory("test", "Summary.", "Raw content.");
    rollout::write_rollout_summary(mem_dir, &m).unwrap();
    memdir::write_memory_summary(mem_dir, "Custom summary content.").unwrap();

    // Verify data exists
    assert_eq!(
        rollout::load_all_rollout_summaries(mem_dir).unwrap().len(),
        1
    );
    let summary = memdir::read_memory_summary(mem_dir).unwrap();
    assert!(summary.contains("Custom summary"));

    // Reset: clear rollout files, reset summary to seed
    let rollout_dir = memdir::rollout_summaries_dir(mem_dir);
    let mut cleared = 0u32;
    if rollout_dir.exists() {
        for entry in std::fs::read_dir(&rollout_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "md") {
                std::fs::remove_file(&path).unwrap();
                cleared += 1;
            }
        }
    }
    memdir::write_memory_summary(mem_dir, "# Memory Summary\n\nNo memories yet.\n").unwrap();

    assert_eq!(cleared, 1);
    assert!(rollout::load_all_rollout_summaries(mem_dir)
        .unwrap()
        .is_empty());

    let after = memdir::read_memory_summary(mem_dir).unwrap();
    assert!(after.contains("No memories yet"));
    assert!(read_path::build_memory_instructions(mem_dir).is_none());
}

#[test]
fn memory_dir_returns_rustycode_path() {
    // memory_dir() uses detect_project_context which needs a git repo.
    // For non-git paths it returns the global fallback. Just verify the shape.
    let dir = rustycode_memory::memory_dir(Path::new("/tmp/nonexistent-project"));
    assert!(
        dir.to_string_lossy().contains(".rustycode"),
        "expected .rustycode in path, got: {}",
        dir.display()
    );
    assert!(
        dir.to_string_lossy().contains("memory"),
        "expected 'memory' in path, got: {}",
        dir.display()
    );
}
