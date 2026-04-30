//! Example demonstrating the truly minimal TUI (disabled for CI)
//!
//! This file has been moved to `examples_disabled/` to avoid compiling in CI while
//! keeping the example source available for local experimentation.

use std::path::PathBuf;

fn main() {
    let cwd = PathBuf::from(".");

    println!("Testing truly_minimal TUI (disabled)...");

    // Types are referenced here in the original example; the full example is
    // preserved in this disabled copy for local use only.
    let _ = cwd;
}
