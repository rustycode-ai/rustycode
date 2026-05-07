//! Memory key-value storage methods.
//!
//! Contains `impl Storage` methods for the in-database key-value memory store.

use anyhow::Result;
use chrono::Utc;
use rusqlite::params;

use crate::records::MemoryRecord;
use crate::Storage;

impl Storage {
    pub fn upsert_memory(&self, scope: &str, key: &str, value: &str) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into memory (scope, key, value, updated_at) values (?1, ?2, ?3, ?4)
             on conflict(scope, key) do update set value = excluded.value, updated_at = excluded.updated_at",
            params![scope, key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn memory(&self, scope: &str) -> Result<Vec<MemoryRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select scope, key, value, updated_at from memory where scope = ?1 order by key",
        )?;
        let rows = stmt.query_map(params![scope], |row| {
            Ok(MemoryRecord {
                scope: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn memory_entry(&self, scope: &str, key: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare("select value from memory where scope = ?1 and key = ?2")?;
        let mut rows = stmt.query(params![scope, key])?;

        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }
}
