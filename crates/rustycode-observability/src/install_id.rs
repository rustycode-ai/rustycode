use std::fs;
use uuid::Uuid;

/// Persistent anonymous install identifier.
///
/// Generated once on first launch and stored in `~/.rustycode/analytics_id`.
/// Contains no PII — purely random UUID v4.
pub fn get_or_create_install_id(data_dir: &std::path::Path) -> Uuid {
    let id_path = data_dir.join("analytics_id");

    // Try reading existing ID
    if let Ok(content) = fs::read_to_string(&id_path) {
        let trimmed = content.trim();
        if let Ok(id) = trimmed.parse::<Uuid>() {
            return id;
        }
    }

    // Generate new ID
    let id = Uuid::new_v4();

    // Persist (non-fatal if it fails)
    if let Err(e) = fs::create_dir_all(data_dir).and_then(|()| fs::write(&id_path, id.to_string()))
    {
        tracing::debug!("could not persist analytics install ID: {e}");
    }

    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_persists_uuid() {
        let dir = tempfile::tempdir().unwrap();
        let id1 = get_or_create_install_id(dir.path());
        let id2 = get_or_create_install_id(dir.path());
        assert_eq!(id1, id2, "should return same ID on second call");

        let content = fs::read_to_string(dir.path().join("analytics_id")).unwrap();
        assert_eq!(content.trim(), id1.to_string());
    }

    #[test]
    fn handles_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("analytics_id"), "not-a-uuid").unwrap();
        let id = get_or_create_install_id(dir.path());
        assert!(id.to_string().len() == 36);
    }

    #[test]
    fn handles_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nonexistent = dir.path().join("deep/nested/dir");
        let id = get_or_create_install_id(&nonexistent);
        assert!(id.to_string().len() == 36);
    }
}
