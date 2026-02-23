use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::{params, Connection};
use sqlite_vec::sqlite3_vec_init;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, Once};

use crate::config::MemoryConfig;
use crate::types::{MemoryCategory, MemoryEntry, MemorySource, MemoryStats, UserProfileFact};

/// Register the sqlite-vec extension globally (once per process).
fn ensure_sqlite_vec_registered() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    });
}

pub struct MemoryStore {
    conn: Mutex<Connection>,
    embedding_dimension: usize,
}

impl MemoryStore {
    pub fn new(config: &MemoryConfig) -> Result<Self> {
        let path = &config.storage_path;
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory for {path}"))?;
        }

        ensure_sqlite_vec_registered();

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open memory database at {path}"))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let store = Self {
            conn: Mutex::new(conn),
            embedding_dimension: config.embedding_dimension,
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn new_in_memory(embedding_dimension: usize) -> Result<Self> {
        ensure_sqlite_vec_registered();

        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let store = Self {
            conn: Mutex::new(conn),
            embedding_dimension,
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'manual',
                category TEXT NOT NULL DEFAULT 'other',
                container_tag TEXT NOT NULL DEFAULT 'default',
                metadata TEXT DEFAULT '{}',
                session_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_memories_container ON memories(container_tag);
            CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
            CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
            CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content,
                id UNINDEXED,
                tokenize='porter unicode61'
            );

            CREATE TABLE IF NOT EXISTS embedding_cache (
                content_hash TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS user_profile (
                id TEXT PRIMARY KEY,
                fact_type TEXT NOT NULL DEFAULT 'static',
                category TEXT NOT NULL DEFAULT 'fact',
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;

        let dim = self.embedding_dimension;
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec USING vec0(
                id TEXT PRIMARY KEY,
                embedding float[{dim}]
            );"
        ))?;

        Ok(())
    }

    pub fn insert(&self, entry: &MemoryEntry, embedding: &[f32]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let metadata_json = serde_json::to_string(&entry.metadata)?;
        let created = entry.created_at.to_rfc3339();
        let updated = entry.updated_at.to_rfc3339();

        tx.execute(
            "INSERT OR REPLACE INTO memories (id, content, source, category, container_tag, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.content,
                entry.source.as_str(),
                entry.category.as_str(),
                entry.container_tag,
                metadata_json,
                entry.session_id,
                created,
                updated,
            ],
        )?;

        let embedding_bytes = embedding_to_bytes(embedding);
        tx.execute(
            "INSERT OR REPLACE INTO memories_vec (id, embedding) VALUES (?1, ?2)",
            params![entry.id, embedding_bytes],
        )?;

        tx.execute(
            "INSERT OR REPLACE INTO memories_fts (id, content) VALUES (?1, ?2)",
            params![entry.id, entry.content],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn store_text_only(&self, entry: &MemoryEntry) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let metadata_json = serde_json::to_string(&entry.metadata)?;
        let created = entry.created_at.to_rfc3339();
        let updated = entry.updated_at.to_rfc3339();

        tx.execute(
            "INSERT OR REPLACE INTO memories (id, content, source, category, container_tag, metadata, session_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.content,
                entry.source.as_str(),
                entry.category.as_str(),
                entry.container_tag,
                metadata_json,
                entry.session_id,
                created,
                updated,
            ],
        )?;

        tx.execute(
            "INSERT OR REPLACE INTO memories_fts (id, content) VALUES (?1, ?2)",
            params![entry.id, entry.content],
        )?;

        let row_id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(row_id)
    }

    pub fn update_embedding(&self, id: i64, embedding: &[f32]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let embedding_bytes = embedding_to_bytes(embedding);
        let updated = tx.execute(
            "INSERT OR REPLACE INTO memories_vec (id, embedding)
             SELECT id, ?1 FROM memories WHERE rowid = ?2",
            params![embedding_bytes, id],
        )?;

        if updated == 0 {
            anyhow::bail!("memory row not found for rowid {id}");
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, source, category, container_tag, metadata, session_id, created_at, updated_at
             FROM memories WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| Ok(row_to_entry(row)))
            .optional()?;

        match result {
            Some(entry) => Ok(Some(entry?)),
            None => Ok(None),
        }
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let deleted = tx.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM memories_vec WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM memories_fts WHERE id = ?1", params![id])?;

        tx.commit()?;
        Ok(deleted > 0)
    }

    pub fn delete_by_ids(&self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut count = 0;

        for id in ids {
            count += tx.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
            tx.execute("DELETE FROM memories_vec WHERE id = ?1", params![id])?;
            tx.execute("DELETE FROM memories_fts WHERE id = ?1", params![id])?;
        }

        tx.commit()?;
        Ok(count)
    }

    pub fn list(&self, container_tag: Option<&str>, limit: usize) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match container_tag {
            Some(tag) => (
                "SELECT id, content, source, category, container_tag, metadata, session_id, created_at, updated_at
                 FROM memories WHERE container_tag = ?1 ORDER BY created_at DESC LIMIT ?2"
                    .to_string(),
                vec![Box::new(tag.to_string()), Box::new(limit as i64)],
            ),
            None => (
                "SELECT id, content, source, category, container_tag, metadata, session_id, created_at, updated_at
                 FROM memories ORDER BY created_at DESC LIMIT ?1"
                    .to_string(),
                vec![Box::new(limit as i64)],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_ref.as_slice(), |row| Ok(row_to_entry(row)))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row??);
        }
        Ok(entries)
    }

    pub fn vector_search(&self, embedding: &[f32], limit: usize) -> Result<Vec<(String, f32)>> {
        let conn = self.conn.lock().unwrap();
        let embedding_bytes = embedding_to_bytes(embedding);

        let mut stmt = conn.prepare(
            "SELECT id, distance FROM memories_vec WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![embedding_bytes, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<(String, f64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, bm25(memories_fts) as rank FROM memories_fts WHERE memories_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![query, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_cached_embedding(&self, content_hash: &str) -> Result<Option<Vec<f32>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT embedding FROM embedding_cache WHERE content_hash = ?1")?;

        let result = stmt
            .query_row(params![content_hash], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(bytes_to_embedding(&bytes))
            })
            .optional()?;

        Ok(result)
    }

    pub fn cache_embedding(
        &self,
        content_hash: &str,
        embedding: &[f32],
        model: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let bytes = embedding_to_bytes(embedding);
        conn.execute(
            "INSERT OR REPLACE INTO embedding_cache (content_hash, embedding, model) VALUES (?1, ?2, ?3)",
            params![content_hash, bytes, model],
        )?;
        Ok(())
    }

    pub fn add_profile_fact(&self, fact: &UserProfileFact) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO user_profile (id, fact_type, category, content, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                fact.id,
                fact.fact_type,
                fact.category,
                fact.content,
                fact.created_at.to_rfc3339(),
                fact.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_profile_facts(&self, fact_type: Option<&str>) -> Result<Vec<UserProfileFact>> {
        let conn = self.conn.lock().unwrap();
        let (sql, param_values): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match fact_type {
            Some(ft) => (
                "SELECT id, fact_type, category, content, created_at, updated_at FROM user_profile WHERE fact_type = ?1 ORDER BY created_at DESC".to_string(),
                vec![Box::new(ft.to_string())],
            ),
            None => (
                "SELECT id, fact_type, category, content, created_at, updated_at FROM user_profile ORDER BY created_at DESC".to_string(),
                vec![],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_ref.as_slice(), |row| Ok(row_to_profile_fact(row)))?;

        let mut facts = Vec::new();
        for row in rows {
            facts.push(row??);
        }
        Ok(facts)
    }

    pub fn remove_profile_fact(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute("DELETE FROM user_profile WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    pub fn stats(&self) -> Result<MemoryStats> {
        let conn = self.conn.lock().unwrap();

        let total: usize = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;

        let mut by_category = HashMap::new();
        let mut stmt = conn.prepare("SELECT category, COUNT(*) FROM memories GROUP BY category")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })?;
        for row in rows {
            let (cat, count) = row?;
            by_category.insert(cat, count);
        }

        let mut by_container = HashMap::new();
        let mut stmt =
            conn.prepare("SELECT container_tag, COUNT(*) FROM memories GROUP BY container_tag")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })?;
        for row in rows {
            let (tag, count) = row?;
            by_container.insert(tag, count);
        }

        let db_size = conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count, pragma_page_size",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap_or(0);

        Ok(MemoryStats {
            total_memories: total,
            total_by_category: by_category,
            total_by_container: by_container,
            db_size_bytes: db_size,
        })
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: usize = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn cleanup(&self, container_tag: &str, retention_days: Option<u32>, max_memories: Option<usize>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut total_deleted = 0;

        // If both are None, return early (no-op)
        if retention_days.is_none() && max_memories.is_none() {
            return Ok(0);
        }

        // Delete old entries by retention_days
        if let Some(days) = retention_days {
            let deleted = tx.execute(
                "DELETE FROM memories WHERE created_at < datetime('now', ?1) AND container_tag = ?2",
                params![format!("-{} days", days), container_tag],
            )?;
            total_deleted += deleted;
        }

        // Keep only newest N memories per container tag
        if let Some(max_count) = max_memories {
            let deleted = tx.execute(
                "DELETE FROM memories WHERE container_tag = ?1 AND id NOT IN (SELECT id FROM memories WHERE container_tag = ?2 ORDER BY created_at DESC LIMIT ?3)",
                params![container_tag, container_tag, max_count as i64],
            )?;
            total_deleted += deleted;
        }

        // Clean up orphaned FTS entries
        tx.execute(
            "DELETE FROM memories_fts WHERE rowid NOT IN (SELECT rowid FROM memories)",
            [],
        )?;

        tx.commit()?;

        tracing::info!(deleted = total_deleted, "memory cleanup completed");
        Ok(total_deleted)
    }
}

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<MemoryEntry> {
    let metadata_str: String = row.get(5)?;
    let metadata: HashMap<String, serde_json::Value> =
        serde_json::from_str(&metadata_str).unwrap_or_default();

    let created_str: String = row.get(7)?;
    let updated_str: String = row.get(8)?;

    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let updated_at = DateTime::parse_from_rfc3339(&updated_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(MemoryEntry {
        id: row.get(0)?,
        content: row.get(1)?,
        source: MemorySource::from_str_lossy(&row.get::<_, String>(2)?),
        category: MemoryCategory::from_str_lossy(&row.get::<_, String>(3)?),
        container_tag: row.get(4)?,
        metadata,
        session_id: row.get(6)?,
        created_at,
        updated_at,
    })
}

fn row_to_profile_fact(row: &rusqlite::Row<'_>) -> Result<UserProfileFact> {
    let created_str: String = row.get(4)?;
    let updated_str: String = row.get(5)?;

    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let updated_at = DateTime::parse_from_rfc3339(&updated_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(UserProfileFact {
        id: row.get(0)?,
        fact_type: row.get(1)?,
        category: row.get(2)?,
        content: row.get(3)?,
        created_at,
        updated_at,
    })
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_embedding(dim: usize, seed: f32) -> Vec<f32> {
        (0..dim)
            .map(|i| ((i as f32 + seed) / dim as f32).sin())
            .collect()
    }

    #[test]
    fn create_store_and_schema() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.total_memories, 0);
    }

    #[test]
    fn insert_and_get() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let entry = MemoryEntry::new("I prefer dark mode", MemorySource::Manual, "default");
        let embedding = make_embedding(128, 1.0);

        store.insert(&entry, &embedding).unwrap();
        let retrieved = store.get(&entry.id).unwrap().unwrap();

        assert_eq!(retrieved.content, "I prefer dark mode");
        assert_eq!(retrieved.category, MemoryCategory::Preference);
        assert_eq!(retrieved.container_tag, "default");
    }

    #[test]
    fn store_text_only_inserts_and_returns_row_id() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let entry = MemoryEntry::new("text-only memory", MemorySource::Conversation, "default");

        let row_id = store.store_text_only(&entry).unwrap();

        assert!(row_id > 0);
        assert_eq!(store.count().unwrap(), 1);
        let retrieved = store.get(&entry.id).unwrap().unwrap();
        assert_eq!(retrieved.content, "text-only memory");
    }

    #[test]
    fn update_embedding_updates_vector_row() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let entry = MemoryEntry::new("needs embedding", MemorySource::Conversation, "default");
        let embedding = make_embedding(128, 4.0);

        let row_id = store.store_text_only(&entry).unwrap();
        store.update_embedding(row_id, &embedding).unwrap();

        let results = store.vector_search(&embedding, 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, entry.id);
    }

    #[test]
    fn delete_removes_from_all_tables() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let entry = MemoryEntry::new("test content", MemorySource::Manual, "default");
        let embedding = make_embedding(128, 1.0);

        store.insert(&entry, &embedding).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        let deleted = store.delete(&entry.id).unwrap();
        assert!(deleted);
        assert_eq!(store.count().unwrap(), 0);
        assert!(store.get(&entry.id).unwrap().is_none());
    }

    #[test]
    fn list_with_container_filter() {
        let store = MemoryStore::new_in_memory(128).unwrap();

        let e1 = MemoryEntry::new("entry one", MemorySource::Manual, "work");
        let e2 = MemoryEntry::new("entry two", MemorySource::Manual, "personal");
        let e3 = MemoryEntry::new("entry three", MemorySource::Manual, "work");

        store.insert(&e1, &make_embedding(128, 1.0)).unwrap();
        store.insert(&e2, &make_embedding(128, 2.0)).unwrap();
        store.insert(&e3, &make_embedding(128, 3.0)).unwrap();

        let work = store.list(Some("work"), 10).unwrap();
        assert_eq!(work.len(), 2);

        let personal = store.list(Some("personal"), 10).unwrap();
        assert_eq!(personal.len(), 1);

        let all = store.list(None, 10).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn vector_search_returns_results() {
        let store = MemoryStore::new_in_memory(128).unwrap();

        for i in 0..5 {
            let entry = MemoryEntry::new(format!("memory {i}"), MemorySource::Manual, "default");
            store
                .insert(&entry, &make_embedding(128, i as f32))
                .unwrap();
        }

        let query_embedding = make_embedding(128, 2.5);
        let results = store.vector_search(&query_embedding, 3).unwrap();
        assert!(!results.is_empty());
        assert!(results.len() <= 3);
    }

    #[test]
    fn fts_search_returns_results() {
        let store = MemoryStore::new_in_memory(128).unwrap();

        let e1 = MemoryEntry::new("rust programming language", MemorySource::Manual, "default");
        let e2 = MemoryEntry::new("python scripting language", MemorySource::Manual, "default");
        let e3 = MemoryEntry::new("rust async runtime tokio", MemorySource::Manual, "default");

        store.insert(&e1, &make_embedding(128, 1.0)).unwrap();
        store.insert(&e2, &make_embedding(128, 2.0)).unwrap();
        store.insert(&e3, &make_embedding(128, 3.0)).unwrap();

        let results = store.fts_search("rust", 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn embedding_cache() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let hash = "abc123";
        let embedding = make_embedding(128, 1.0);

        assert!(store.get_cached_embedding(hash).unwrap().is_none());

        store
            .cache_embedding(hash, &embedding, "test-model")
            .unwrap();

        let cached = store.get_cached_embedding(hash).unwrap().unwrap();
        assert_eq!(cached.len(), 128);
        assert!((cached[0] - embedding[0]).abs() < 1e-6);
    }

    #[test]
    fn stats_counting() {
        let store = MemoryStore::new_in_memory(128).unwrap();

        let e1 = MemoryEntry::new("I prefer vim", MemorySource::Manual, "work");
        let e2 = MemoryEntry::new("The sky is blue", MemorySource::Manual, "personal");

        store.insert(&e1, &make_embedding(128, 1.0)).unwrap();
        store.insert(&e2, &make_embedding(128, 2.0)).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.total_memories, 2);
        assert!(stats.total_by_container.contains_key("work"));
        assert!(stats.total_by_container.contains_key("personal"));
    }

    #[test]
    fn profile_facts_crud() {
        let store = MemoryStore::new_in_memory(128).unwrap();

        let fact = UserProfileFact {
            id: "f1".to_string(),
            fact_type: "static".to_string(),
            category: "preference".to_string(),
            content: "Prefers dark mode".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store.add_profile_fact(&fact).unwrap();

        let facts = store.get_profile_facts(Some("static")).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "Prefers dark mode");

        let removed = store.remove_profile_fact("f1").unwrap();
        assert!(removed);

        let facts = store.get_profile_facts(None).unwrap();
        assert!(facts.is_empty());
    }
    #[test]
    fn cleanup_with_retention_days_keeps_recent_entries() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let container = "test-container";

        // Insert an entry
        let entry = MemoryEntry::new("recent memory", MemorySource::Manual, container);
        store.insert(&entry, &make_embedding(128, 1.0)).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        // Cleanup with retention_days=1000 keeps recent entries
        let deleted = store.cleanup(container, Some(1000), None).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn cleanup_with_max_memories_keeps_only_newest() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let container = "test-container";

        // Insert 5 entries
        for i in 0..5 {
            let entry = MemoryEntry::new(format!("memory {i}"), MemorySource::Manual, container);
            store.insert(&entry, &make_embedding(128, i as f32)).unwrap();
        }
        assert_eq!(store.count().unwrap(), 5);

        // Cleanup with max_memories=2 should keep only 2 newest
        let deleted = store.cleanup(container, None, Some(2)).unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn cleanup_with_both_none_is_noop() {
        let store = MemoryStore::new_in_memory(128).unwrap();
        let container = "test-container";

        // Insert an entry
        let entry = MemoryEntry::new("memory", MemorySource::Manual, container);
        store.insert(&entry, &make_embedding(128, 1.0)).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        // Cleanup with both None should be a no-op
        let deleted = store.cleanup(container, None, None).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(store.count().unwrap(), 1);
    }

}
