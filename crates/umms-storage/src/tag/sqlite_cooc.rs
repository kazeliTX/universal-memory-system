//! SQLite-backed co-occurrence matrix for tags.
//!
//! Tracks how often tag pairs appear on the same memory entry and computes
//! Pointwise Mutual Information (PMI) incrementally.

use std::path::Path;
use std::sync::Arc;

use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};
use umms_core::tag::TagCooccurrence;
use umms_core::types::*;

// ---------------------------------------------------------------------------
// SqliteCoocStore
// ---------------------------------------------------------------------------

/// SQLite-backed co-occurrence storage for tag pairs.
///
/// All async methods delegate to synchronous `rusqlite` calls wrapped in
/// `tokio::task::spawn_blocking`. The inner `Connection` is protected by a
/// `tokio::sync::Mutex` so only one blocking task accesses it at a time.
pub struct SqliteCoocStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteCoocStore {
    /// Open (or create) a SQLite database at `path` and run migrations.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> std::result::Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref()).map_err(|e| StorageError::ConnectionFailed {
            backend: "sqlite-cooc".into(),
            reason: e.to_string(),
        })?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> std::result::Result<(), UmmsError> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS tag_cooccurrence (
                tag_a TEXT NOT NULL,
                tag_b TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 0,
                pmi REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (tag_a, tag_b)
            );

            CREATE TABLE IF NOT EXISTS tag_stats (
                tag_id TEXT PRIMARY KEY,
                total_entries INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_cooc_tag_a ON tag_cooccurrence(tag_a);
            CREATE INDEX IF NOT EXISTS idx_cooc_tag_b ON tag_cooccurrence(tag_b);
            ",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;

        Ok(())
    }

    /// Record that a set of tags co-occurred on the same memory entry.
    /// Updates the co-occurrence counts and recalculates PMI values.
    pub async fn record_cooccurrence(&self, tag_ids: &[TagId]) -> umms_core::error::Result<()> {
        if tag_ids.len() < 2 {
            return Ok(());
        }

        let ids: Vec<String> = tag_ids.iter().map(|id| id.as_str().to_owned()).collect();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            // Increment per-tag stats (total_entries).
            for id in &ids {
                conn.execute(
                    "INSERT INTO tag_stats (tag_id, total_entries) VALUES (?1, 1)
                     ON CONFLICT(tag_id) DO UPDATE SET total_entries = total_entries + 1",
                    params![id],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            }

            // Increment co-occurrence counts for all pairs.
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let (a, b) = if ids[i] < ids[j] {
                        (&ids[i], &ids[j])
                    } else {
                        (&ids[j], &ids[i])
                    };

                    conn.execute(
                        "INSERT INTO tag_cooccurrence (tag_a, tag_b, count, pmi)
                         VALUES (?1, ?2, 1, 0.0)
                         ON CONFLICT(tag_a, tag_b) DO UPDATE SET count = count + 1",
                        params![a, b],
                    )
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                }
            }

            // Recalculate PMI for affected pairs.
            // PMI = log2(P(a,b) / (P(a) * P(b)))
            // where P(a) = total_entries(a) / N, P(a,b) = count(a,b) / N
            // This simplifies to: log2(count(a,b) * N / (total_entries(a) * total_entries(b)))
            let total_n: f64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(total_entries), 0) FROM tag_stats",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            if total_n > 0.0 {
                for i in 0..ids.len() {
                    for j in (i + 1)..ids.len() {
                        let (a, b) = if ids[i] < ids[j] {
                            (&ids[i], &ids[j])
                        } else {
                            (&ids[j], &ids[i])
                        };

                        let cooc_count: f64 = conn
                            .query_row(
                                "SELECT count FROM tag_cooccurrence WHERE tag_a = ?1 AND tag_b = ?2",
                                params![a, b],
                                |row| row.get(0),
                            )
                            .unwrap_or(0.0);

                        let stats_a: f64 = conn
                            .query_row(
                                "SELECT total_entries FROM tag_stats WHERE tag_id = ?1",
                                params![a],
                                |row| row.get(0),
                            )
                            .unwrap_or(1.0);

                        let stats_b: f64 = conn
                            .query_row(
                                "SELECT total_entries FROM tag_stats WHERE tag_id = ?1",
                                params![b],
                                |row| row.get(0),
                            )
                            .unwrap_or(1.0);

                        let pmi = if stats_a > 0.0 && stats_b > 0.0 && cooc_count > 0.0 {
                            (cooc_count * total_n / (stats_a * stats_b)).log2()
                        } else {
                            0.0
                        };

                        conn.execute(
                            "UPDATE tag_cooccurrence SET pmi = ?3 WHERE tag_a = ?1 AND tag_b = ?2",
                            params![a, b, pmi],
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    }
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    /// Get co-occurring tags for a given tag, ordered by PMI descending.
    pub async fn cooccurrences(
        &self,
        tag_id: &TagId,
        limit: usize,
    ) -> umms_core::error::Result<Vec<TagCooccurrence>> {
        let id = tag_id.as_str().to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            let mut stmt = conn
                .prepare(
                    "SELECT tag_a, tag_b, count, pmi FROM tag_cooccurrence
                     WHERE tag_a = ?1 OR tag_b = ?1
                     ORDER BY pmi DESC
                     LIMIT ?2",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map(params![&id, limit as i64], |row| {
                    let first_tag: String = row.get(0)?;
                    let second_tag: String = row.get(1)?;
                    let count: i64 = row.get(2)?;
                    let pmi: f64 = row.get(3)?;

                    Ok(TagCooccurrence {
                        tag_a: TagId::from_str(&first_tag).unwrap_or_else(|_| TagId::new()),
                        tag_b: TagId::from_str(&second_tag).unwrap_or_else(|_| TagId::new()),
                        count: count as u64,
                        #[allow(clippy::cast_possible_truncation)]
                        pmi: pmi as f32,
                    })
                })
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(
                    row.map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?,
                );
            }

            Ok(results)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn make_tag_id(name: &str) -> TagId {
        TagId::from_str(name).unwrap()
    }

    #[tokio::test]
    async fn record_and_query_cooccurrence() {
        let store = SqliteCoocStore::new(":memory:").unwrap();

        let ids = vec![
            make_tag_id("tag-a"),
            make_tag_id("tag-b"),
            make_tag_id("tag-c"),
        ];

        store.record_cooccurrence(&ids).await.unwrap();

        let coocs = store
            .cooccurrences(&make_tag_id("tag-a"), 10)
            .await
            .unwrap();
        assert_eq!(coocs.len(), 2); // tag-a co-occurs with tag-b and tag-c

        for cooc in &coocs {
            assert!(cooc.count >= 1);
        }
    }

    #[tokio::test]
    async fn repeated_cooccurrence_increments_count() {
        let store = SqliteCoocStore::new(":memory:").unwrap();

        let ids = vec![make_tag_id("tag-x"), make_tag_id("tag-y")];

        store.record_cooccurrence(&ids).await.unwrap();
        store.record_cooccurrence(&ids).await.unwrap();
        store.record_cooccurrence(&ids).await.unwrap();

        let coocs = store
            .cooccurrences(&make_tag_id("tag-x"), 10)
            .await
            .unwrap();
        assert_eq!(coocs.len(), 1);
        assert_eq!(coocs[0].count, 3);
    }

    #[tokio::test]
    async fn single_tag_no_cooccurrence() {
        let store = SqliteCoocStore::new(":memory:").unwrap();

        let ids = vec![make_tag_id("lonely")];
        store.record_cooccurrence(&ids).await.unwrap();

        let coocs = store
            .cooccurrences(&make_tag_id("lonely"), 10)
            .await
            .unwrap();
        assert!(coocs.is_empty());
    }

    #[tokio::test]
    async fn pmi_is_computed() {
        let store = SqliteCoocStore::new(":memory:").unwrap();

        // Record several co-occurrences to get meaningful PMI.
        let pair_ab = vec![make_tag_id("alpha"), make_tag_id("beta")];
        let pair_ac = vec![make_tag_id("alpha"), make_tag_id("gamma")];

        for _ in 0..5 {
            store.record_cooccurrence(&pair_ab).await.unwrap();
        }
        store.record_cooccurrence(&pair_ac).await.unwrap();

        let coocs = store
            .cooccurrences(&make_tag_id("alpha"), 10)
            .await
            .unwrap();
        assert_eq!(coocs.len(), 2);

        // The pair with higher count should have non-zero PMI.
        let ab_cooc = coocs.iter().find(|c| {
            (c.tag_a.as_str() == "alpha" && c.tag_b.as_str() == "beta")
                || (c.tag_a.as_str() == "beta" && c.tag_b.as_str() == "alpha")
        });
        assert!(ab_cooc.is_some());
    }

    #[tokio::test]
    async fn empty_input_is_noop() {
        let store = SqliteCoocStore::new(":memory:").unwrap();
        store.record_cooccurrence(&[]).await.unwrap();
    }
}
