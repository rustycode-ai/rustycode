//! File Snapshot System Demo
//!
//! This example demonstrates how to use the FileSnapshotManager to track
//! file changes and undo operations.

use rustycode_tools::FileSnapshotManager;
use std::fs;
use tempfile::TempDir;

fn main() -> anyhow::Result<()> {
    println!("=== File Snapshot System Demo ===\n");

    // Create a temporary directory for our demo
    let temp_dir = TempDir::new()?;
    let dir_path = temp_dir.path();

    println!("Working directory: {}", dir_path.display());

    // Create a snapshot manager with max 10 groups
    let mut snapshot_mgr = FileSnapshotManager::new(10);

    // Demo 1: Basic snapshot and undo
    println!("\n--- Demo 1: Basic Snapshot and Undo ---");
    let file1 = dir_path.join("demo1.txt");
    fs::write(&file1, "Original content")?;

    println!("Created file: {}", file1.display());
    println!("Initial content: {}", fs::read_to_string(&file1)?);

    // Create a snapshot group before modifying
    let group_id = snapshot_mgr.create_group("write_file");
    snapshot_mgr.snapshot_file(&group_id, &file1)?;

    // Modify the file
    fs::write(&file1, "Modified content")?;
    println!("Modified content: {}", fs::read_to_string(&file1)?);

    // Undo the changes
    println!("\nUndoing changes...");
    let result = snapshot_mgr.undo_last().unwrap();
    println!("Restored {} file(s)", result.restored.len());
    println!("Content after undo: {}", fs::read_to_string(&file1)?);

    // Demo 2: Undo deletes newly created files
    println!("\n--- Demo 2: Undo Deletes New Files ---");
    let file2 = dir_path.join("new_file.txt");

    // Snapshot non-existent file
    let group_id = snapshot_mgr.create_group("write_file");
    snapshot_mgr.snapshot_file(&group_id, &file2)?;

    // Create the file
    fs::write(&file2, "This is a new file")?;
    println!("Created new file: {}", file2.display());
    println!("File exists: {}", file2.exists());

    // Undo should delete it
    println!("\nUndoing creation...");
    snapshot_mgr.undo_last().unwrap();
    println!("File exists after undo: {}", file2.exists());

    // Demo 3: Multiple files in one group
    println!("\n--- Demo 3: Multiple Files in One Group ---");
    let file3 = dir_path.join("file3.txt");
    let file4 = dir_path.join("file4.txt");

    fs::write(&file3, "Content 3")?;
    fs::write(&file4, "Content 4")?;

    let group_id = snapshot_mgr.create_group("multi_edit");
    snapshot_mgr.snapshot_file(&group_id, &file3)?;
    snapshot_mgr.snapshot_file(&group_id, &file4)?;

    // Modify both files
    fs::write(&file3, "Modified 3")?;
    fs::write(&file4, "Modified 4")?;

    println!(
        "Before undo: {} = {}",
        file3.display(),
        fs::read_to_string(&file3)?
    );
    println!(
        "Before undo: {} = {}",
        file4.display(),
        fs::read_to_string(&file4)?
    );

    // Undo both at once
    println!("\nUndoing multi-file edit...");
    let result = snapshot_mgr.undo_last().unwrap();
    println!("Restored {} file(s)", result.restored.len());
    println!(
        "After undo: {} = {}",
        file3.display(),
        fs::read_to_string(&file3)?
    );
    println!(
        "After undo: {} = {}",
        file4.display(),
        fs::read_to_string(&file4)?
    );

    // Demo 4: Undo history
    println!("\n--- Demo 4: Undo History ---");
    let file5 = dir_path.join("file5.txt");
    fs::write(&file5, "Version 1")?;

    snapshot_mgr.create_group("edit1");
    fs::write(&file5, "Version 2")?;

    snapshot_mgr.create_group("edit2");
    fs::write(&file5, "Version 3")?;

    snapshot_mgr.create_group("edit3");
    fs::write(&file5, "Version 4")?;

    println!("Current content: {}", fs::read_to_string(&file5)?);
    println!("Available undo operations: {}", snapshot_mgr.undo_count());

    // List all groups
    println!("\nSnapshot groups:");
    for group in snapshot_mgr.list_groups() {
        println!(
            "  - {} ({}) - undone: {}",
            group.id, group.tool_name, group.undone
        );
    }

    // Undo one by one
    while snapshot_mgr.undo_count() > 0 {
        let result = snapshot_mgr.undo_last().unwrap();
        println!(
            "Undone {} - content now: {}",
            result.group_id,
            fs::read_to_string(&file5)?
        );
    }

    println!("\n=== Demo Complete ===");
    Ok(())
}
