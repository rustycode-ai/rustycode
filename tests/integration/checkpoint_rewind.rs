//! Integration tests for checkpoint creation and rewind functionality

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustycode_core::recovery::CheckpointRecovery;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a test git repository
    fn create_test_repo() -> Result<(TempDir, String)> {
        let temp = TempDir::new()?;
        let repo_path = temp.path().to_string_lossy().to_string();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_path)
            .output()?;

        // Configure git user for commits
        std::process::Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()?;

        std::process::Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()?;

        Ok((temp, repo_path))
    }

    /// Helper to create a commit with a file
    fn create_commit(repo_path: &str, filename: &str, content: &str) -> Result<()> {
        let file_path = format!("{}/{}", repo_path, filename);
        fs::write(&file_path, content)?;

        std::process::Command::new("git")
            .args(&["add", filename])
            .current_dir(repo_path)
            .output()?;

        std::process::Command::new("git")
            .args(&["commit", "-m", &format!("Add {}", filename)])
            .current_dir(repo_path)
            .output()?;

        Ok(())
    }

    #[tokio::test]
    async fn test_checkpoint_create_and_restore() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "test.txt", "initial content")?;

        // Create checkpoint
        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        // Verify checkpoint has git hash
        assert!(checkpoint.checkpoint_git_hash.is_some());
        let git_hash = checkpoint.checkpoint_git_hash.as_ref().unwrap().clone();

        // Verify we have the commit hash
        assert!(!git_hash.is_empty());
        assert_eq!(git_hash.len(), 40); // SHA-1 hash length

        let file_path = format!("{}/test.txt", repo_path);

        // Verify initial content
        assert_eq!(fs::read_to_string(&file_path)?, "initial content");

        // Modify file
        fs::write(&file_path, "modified content")?;
        assert_eq!(fs::read_to_string(&file_path)?, "modified content");

        // Verify the file is actually modified
        assert_ne!(
            fs::read_to_string(&file_path)?,
            "initial content"
        );

        // Rewind to checkpoint
        recovery.rewind(&checkpoint).await?;

        // Verify file is restored to checkpoint state
        assert_eq!(fs::read_to_string(&file_path)?, "initial content");

        Ok(())
    }

    #[tokio::test]
    async fn test_rewind_with_new_files() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "file1.txt", "content1")?;

        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        let file1 = format!("{}/file1.txt", repo_path);
        let file2 = format!("{}/file2.txt", repo_path);

        // Verify initial state
        assert!(std::path::Path::new(&file1).exists());
        assert!(!std::path::Path::new(&file2).exists());

        // Create new file after checkpoint
        fs::write(&file2, "content2")?;
        assert!(std::path::Path::new(&file2).exists());

        // Verify both files exist now
        assert!(std::path::Path::new(&file1).exists());
        assert!(std::path::Path::new(&file2).exists());

        // Rewind (new file should be removed)
        recovery.rewind(&checkpoint).await?;

        // Verify new file is gone
        assert!(!std::path::Path::new(&file2).exists());
        assert_eq!(fs::read_to_string(&file1)?, "content1");

        Ok(())
    }

    #[tokio::test]
    async fn test_rewind_with_multiple_files() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;

        // Create initial commit with multiple files
        create_commit(&repo_path, "file1.txt", "content1")?;
        create_commit(&repo_path, "file2.txt", "content2")?;

        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        let file1 = format!("{}/file1.txt", repo_path);
        let file2 = format!("{}/file2.txt", repo_path);
        let file3 = format!("{}/file3.txt", repo_path);

        // Modify first file
        fs::write(&file1, "modified1")?;

        // Modify second file
        fs::write(&file2, "modified2")?;

        // Add new file
        fs::write(&file3, "new file")?;

        // Rewind
        recovery.rewind(&checkpoint).await?;

        // Verify all files are restored
        assert_eq!(fs::read_to_string(&file1)?, "content1");
        assert_eq!(fs::read_to_string(&file2)?, "content2");
        assert!(!std::path::Path::new(&file3).exists());

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_checkpoints() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "file.txt", "version 1")?;

        let recovery = CheckpointRecovery::new(&repo_path)?;

        // Create first checkpoint
        let checkpoint1 = recovery.create()?;
        let file = format!("{}/file.txt", repo_path);

        // Modify and commit a new version
        fs::write(&file, "version 2")?;
        std::process::Command::new("git")
            .args(&["add", "file.txt"])
            .current_dir(&repo_path)
            .output()?;
        std::process::Command::new("git")
            .args(&["commit", "-m", "Update file"])
            .current_dir(&repo_path)
            .output()?;

        // Create second checkpoint
        let checkpoint2 = recovery.create()?;

        // Modify to version 3
        fs::write(&file, "version 3")?;

        // Rewind to checkpoint 2
        recovery.rewind(&checkpoint2).await?;
        assert_eq!(fs::read_to_string(&file)?, "version 2");

        // Rewind to checkpoint 1
        recovery.rewind(&checkpoint1).await?;
        assert_eq!(fs::read_to_string(&file)?, "version 1");

        Ok(())
    }

    #[tokio::test]
    async fn test_checkpoint_with_deleted_files() -> Result<()> {
        let (_temp, repo_path) = create_test_repo()?;
        create_commit(&repo_path, "file1.txt", "content1")?;
        create_commit(&repo_path, "file2.txt", "content2")?;

        let recovery = CheckpointRecovery::new(&repo_path)?;
        let checkpoint = recovery.create()?;

        let file1 = format!("{}/file1.txt", repo_path);
        let file2 = format!("{}/file2.txt", repo_path);

        // Delete file2
        fs::remove_file(&file2)?;
        assert!(!std::path::Path::new(&file2).exists());

        // Rewind
        recovery.rewind(&checkpoint).await?;

        // Both files should be restored
        assert_eq!(fs::read_to_string(&file1)?, "content1");
        assert_eq!(fs::read_to_string(&file2)?, "content2");

        Ok(())
    }
}
