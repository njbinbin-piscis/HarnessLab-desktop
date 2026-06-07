//! Memory plugin trait and built-in backends.
//!
//! Backends use a lightweight bag-of-words cosine similarity rather than a real
//! embedding model, which keeps them dependency-free and deterministic while
//! still ranking by lexical relevance. [`InMemoryVectorStore`] is session
//! scoped; [`SqliteMemory`] persists across runs.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;

/// A long-term memory backend the agent loop can store to and retrieve from.
#[async_trait]
pub trait MemoryPlugin: Send + Sync {
    /// Persist a memory for `owner`.
    async fn store(&self, owner: &str, text: &str) -> anyhow::Result<()>;

    /// Retrieve up to `k` memories for `owner` most relevant to `query`.
    async fn search(&self, owner: &str, query: &str, k: usize) -> anyhow::Result<Vec<String>>;

    /// A stable identifier for this backend.
    fn name(&self) -> &str;
}

/// Tokenize into lowercased alphanumeric words.
fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Bag-of-words cosine similarity between two token lists.
fn similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut counts_a: HashMap<&str, f64> = HashMap::new();
    let mut counts_b: HashMap<&str, f64> = HashMap::new();
    for t in a {
        *counts_a.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    for t in b {
        *counts_b.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    let mut dot = 0.0;
    for (t, va) in &counts_a {
        if let Some(vb) = counts_b.get(t) {
            dot += va * vb;
        }
    }
    let norm_a = counts_a.values().map(|v| v * v).sum::<f64>().sqrt();
    let norm_b = counts_b.values().map(|v| v * v).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Rank `entries` by similarity to `query`, returning the top `k` texts.
fn rank(entries: &[String], query: &str, k: usize) -> Vec<String> {
    let q = tokenize(query);
    let mut scored: Vec<(f64, &String)> = entries
        .iter()
        .map(|e| (similarity(&tokenize(e), &q), e))
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(_, e)| e.clone()).collect()
}

/// Session-scoped, in-memory vector store.
#[derive(Default)]
pub struct InMemoryVectorStore {
    data: Mutex<HashMap<String, Vec<String>>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MemoryPlugin for InMemoryVectorStore {
    async fn store(&self, owner: &str, text: &str) -> anyhow::Result<()> {
        let mut data = self.data.lock().unwrap();
        data.entry(owner.to_string())
            .or_default()
            .push(text.to_string());
        Ok(())
    }

    async fn search(&self, owner: &str, query: &str, k: usize) -> anyhow::Result<Vec<String>> {
        let data = self.data.lock().unwrap();
        Ok(data
            .get(owner)
            .map(|entries| rank(entries, query, k))
            .unwrap_or_default())
    }

    fn name(&self) -> &str {
        "InMemoryVectorStore"
    }
}

/// SQLite-backed persistent memory store.
pub struct SqliteMemory {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteMemory {
    /// Open (or create) a memory database at `path`.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                owner TEXT NOT NULL,
                text TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_owner ON memories(owner);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl MemoryPlugin for SqliteMemory {
    async fn store(&self, owner: &str, text: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO memories (owner, text, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![owner, text, now],
        )?;
        Ok(())
    }

    async fn search(&self, owner: &str, query: &str, k: usize) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT text FROM memories WHERE owner = ?1")?;
        let rows = stmt.query_map(rusqlite::params![owner], |row| row.get::<_, String>(0))?;
        let entries: Vec<String> = rows.filter_map(|r| r.ok()).collect();
        Ok(rank(&entries, query, k))
    }

    fn name(&self) -> &str {
        "SqliteMemory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_ranks_by_relevance() {
        let store = InMemoryVectorStore::new();
        store.store("a", "the cat sat on the mat").await.unwrap();
        store
            .store("a", "rust is a systems language")
            .await
            .unwrap();
        let hits = store
            .search("a", "programming language rust", 1)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("rust"));
    }

    #[tokio::test]
    async fn sqlite_persists_and_ranks() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("mem.db");
        {
            let store = SqliteMemory::open(&path).unwrap();
            store.store("o", "alpha beta gamma").await.unwrap();
        }
        let store = SqliteMemory::open(&path).unwrap();
        let hits = store.search("o", "beta", 5).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("beta"));
    }
}
