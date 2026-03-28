//! LanceDB-backed tag vector storage.
//!
//! Stores semantic tags in a `tags` LanceDB table with 3072-dim embeddings.
//! Handles vector similarity search and basic CRUD. Co-occurrence is handled
//! separately by [`super::SqliteCoocStore`].

use std::sync::Arc;

use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::Table as LanceTable;
use lancedb::query::{ExecutableQuery, QueryBase};
use tokio::sync::Mutex;
use tracing::instrument;

use umms_core::error::{Result, StorageError, UmmsError};
use umms_core::tag::{Tag, TagMatch};
use umms_core::types::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TABLE_NAME: &str = "tags";
const DEFAULT_VECTOR_DIM: usize = 3072;

// ---------------------------------------------------------------------------
// LanceTagStore
// ---------------------------------------------------------------------------

/// LanceDB-backed storage for semantic tag embeddings.
///
/// Thread-safe: the inner [`LanceTable`] handle is wrapped in an `Arc<Mutex<_>>`
/// to serialise writes (same pattern as [`crate::vector::LanceVectorStore`]).
pub struct LanceTagStore {
    table: Arc<Mutex<LanceTable>>,
    schema: Arc<Schema>,
    vector_dim: usize,
}

impl LanceTagStore {
    /// Connect to (or create) a LanceDB database at `db_path` and open the
    /// `tags` table. If the table does not exist it is created with an
    /// empty initial batch.
    pub async fn new(db_path: &str, vector_dim: usize) -> std::result::Result<Self, UmmsError> {
        let schema = Arc::new(build_schema(vector_dim));

        let db = lancedb::connect(db_path)
            .execute()
            .await
            .map_err(lance_err)?;

        let table = if let Ok(t) = db.open_table(TABLE_NAME).execute().await {
            let existing_schema = t.schema().await.map_err(lance_err)?;
            let dim_matches = existing_schema
                .field_with_name("vector")
                .ok()
                .and_then(|f| match f.data_type() {
                    DataType::FixedSizeList(_, size) => Some(*size as usize == vector_dim),
                    _ => None,
                })
                .unwrap_or(false);

            if dim_matches {
                t
            } else {
                tracing::warn!(
                    expected = vector_dim,
                    "Tag vector dimension mismatch — dropping and recreating table"
                );
                let _ = db.drop_table(TABLE_NAME, &[]).await;
                let batch = empty_batch(&schema, vector_dim)?;
                db.create_table(TABLE_NAME, vec![batch])
                    .execute()
                    .await
                    .map_err(lance_err)?
            }
        } else {
            let batch = empty_batch(&schema, vector_dim)?;
            db.create_table(TABLE_NAME, vec![batch])
                .execute()
                .await
                .map_err(lance_err)?
        };

        Ok(Self {
            table: Arc::new(Mutex::new(table)),
            schema,
            vector_dim,
        })
    }

    /// Create a store with the default 3072-dim vectors.
    pub async fn with_default_dim(db_path: &str) -> std::result::Result<Self, UmmsError> {
        Self::new(db_path, DEFAULT_VECTOR_DIM).await
    }

    // ----- Public query methods used by CompositeTagStore --------------------

    /// Insert or replace a tag in the Lance table.
    #[instrument(skip(self, tag), fields(id = %tag.id, label = %tag.label))]
    pub async fn upsert_tag(&self, tag: &Tag) -> Result<TagId> {
        // Delete existing entry if present (upsert semantics).
        let table = self.table.lock().await;
        let _ = table.delete(&format!("id = '{}'", tag.id.as_str())).await;

        let batch = tags_to_batch(std::slice::from_ref(tag), &self.schema, self.vector_dim)?;
        table.add(vec![batch]).execute().await.map_err(lance_err)?;
        Ok(tag.id.clone())
    }

    /// Batch insert/replace tags.
    #[instrument(skip(self, tags), fields(count = tags.len()))]
    pub async fn upsert_batch_tags(&self, tags: &[Tag]) -> Result<Vec<TagId>> {
        if tags.is_empty() {
            return Ok(Vec::new());
        }

        let table = self.table.lock().await;

        // Delete all existing entries with matching IDs.
        for tag in tags {
            let _ = table.delete(&format!("id = '{}'", tag.id.as_str())).await;
        }

        let batch = tags_to_batch(tags, &self.schema, self.vector_dim)?;
        table.add(vec![batch]).execute().await.map_err(lance_err)?;
        Ok(tags.iter().map(|t| t.id.clone()).collect())
    }

    /// Vector similarity search over tag embeddings.
    #[instrument(skip(self, vector), fields(top_k))]
    pub async fn search_by_vector(
        &self,
        vector: &[f32],
        agent_id: Option<&AgentId>,
        top_k: usize,
    ) -> Result<Vec<TagMatch>> {
        let filter = match agent_id {
            Some(aid) => format!("(agent_id = '{}' OR agent_id = '')", aid.as_str()),
            None => String::new(),
        };

        let table = self.table.lock().await;
        let mut query = table.vector_search(vector).map_err(lance_err)?;
        query = query.limit(top_k);
        if !filter.is_empty() {
            query = query.only_if(filter);
        }
        let results = query.execute().await.map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        let mut matches = Vec::new();
        for batch in &batches {
            let tags = batch_to_tags(batch, self.vector_dim)?;
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            for (i, tag) in tags.into_iter().enumerate() {
                // Skip zero-vector placeholders.
                if tag.vector.iter().all(|&x| x == 0.0) {
                    continue;
                }
                let distance = distances.map_or(0.0, |d| d.value(i));
                let similarity = 1.0 / (1.0 + distance);
                matches.push(TagMatch { tag, similarity });
            }
        }

        Ok(matches)
    }

    /// Find tags by label substring.
    #[instrument(skip(self), fields(query, limit))]
    pub async fn find_by_label(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<Tag>> {
        let canonical = Tag::canonicalize(query);

        let mut filter_parts = vec![format!("canonical LIKE '%{canonical}%'")];
        if let Some(aid) = agent_id {
            filter_parts.push(format!("(agent_id = '{}' OR agent_id = '')", aid.as_str()));
        }
        let filter = filter_parts.join(" AND ");

        let table = self.table.lock().await;
        let results = table
            .query()
            .only_if(filter)
            .limit(limit)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        let mut all_tags = Vec::new();
        for batch in &batches {
            let tags = batch_to_tags(batch, self.vector_dim)?;
            all_tags.extend(tags);
        }

        Ok(all_tags)
    }

    /// Get all tags, optionally filtered by agent.
    pub async fn all_tags(&self, agent_id: Option<&AgentId>) -> Result<Vec<Tag>> {
        let filter = match agent_id {
            Some(aid) => format!("(agent_id = '{}' OR agent_id = '')", aid.as_str()),
            None => String::new(),
        };

        let table = self.table.lock().await;
        let mut query = table.query();
        if !filter.is_empty() {
            query = query.only_if(filter);
        }
        let results = query.execute().await.map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        let mut all_tags = Vec::new();
        for batch in &batches {
            let tags = batch_to_tags(batch, self.vector_dim)?;
            all_tags.extend(tags);
        }

        Ok(all_tags)
    }

    /// Get a single tag by ID.
    #[instrument(skip(self), fields(id = %id))]
    pub async fn get_tag(&self, id: &TagId) -> Result<Option<Tag>> {
        let table = self.table.lock().await;
        let filter = format!("id = '{}'", id.as_str());
        let results = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        for batch in &batches {
            let tags = batch_to_tags(batch, self.vector_dim)?;
            if let Some(tag) = tags.into_iter().next() {
                return Ok(Some(tag));
            }
        }
        Ok(None)
    }

    /// Get multiple tags by ID.
    pub async fn get_batch_tags(&self, ids: &[TagId]) -> Result<Vec<Tag>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let id_list: Vec<String> = ids.iter().map(|id| format!("'{}'", id.as_str())).collect();
        let filter = format!("id IN ({})", id_list.join(", "));

        let table = self.table.lock().await;
        let results = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        let mut all_tags = Vec::new();
        for batch in &batches {
            let tags = batch_to_tags(batch, self.vector_dim)?;
            all_tags.extend(tags);
        }

        Ok(all_tags)
    }

    /// Find a tag by canonical label + agent scope (for upsert dedup).
    pub async fn find_by_canonical(
        &self,
        canonical: &str,
        agent_id: Option<&AgentId>,
    ) -> Result<Option<Tag>> {
        let agent_filter = match agent_id {
            Some(aid) => format!("agent_id = '{}'", aid.as_str()),
            None => "agent_id = ''".to_string(),
        };
        let filter = format!("canonical = '{canonical}' AND {agent_filter}");

        let table = self.table.lock().await;
        let results = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        for batch in &batches {
            let tags = batch_to_tags(batch, self.vector_dim)?;
            if let Some(tag) = tags.into_iter().next() {
                return Ok(Some(tag));
            }
        }
        Ok(None)
    }

    /// Count tags, optionally filtered by agent.
    pub async fn count(&self, agent_id: Option<&AgentId>) -> Result<u64> {
        let filter = match agent_id {
            Some(aid) => format!("(agent_id = '{}' OR agent_id = '')", aid.as_str()),
            None => String::new(),
        };

        let table = self.table.lock().await;
        let mut query = table.query();
        if !filter.is_empty() {
            query = query.only_if(filter);
        }
        let results = query.execute().await.map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results.try_collect::<Vec<_>>().await.map_err(lance_err)?;

        let total: usize = batches.iter().map(RecordBatch::num_rows).sum();
        Ok(total as u64)
    }
}

// ---------------------------------------------------------------------------
// Arrow schema helpers
// ---------------------------------------------------------------------------

fn build_schema(vector_dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("label", DataType::Utf8, false),
        Field::new("canonical", DataType::Utf8, false),
        Field::new("agent_id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim as i32,
            ),
            true,
        ),
        Field::new("importance", DataType::Float32, false),
        Field::new("frequency", DataType::UInt64, false),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("updated_at", DataType::Utf8, false),
    ])
}

// ---------------------------------------------------------------------------
// Tag <-> RecordBatch conversion
// ---------------------------------------------------------------------------

/// Convert a slice of [`Tag`] to an Arrow [`RecordBatch`].
fn tags_to_batch(tags: &[Tag], schema: &Arc<Schema>, vector_dim: usize) -> Result<RecordBatch> {
    let len = tags.len();

    let ids: Vec<&str> = tags.iter().map(|t| t.id.as_str()).collect();
    let labels: Vec<&str> = tags.iter().map(|t| t.label.as_str()).collect();
    let canonicals: Vec<&str> = tags.iter().map(|t| t.canonical.as_str()).collect();
    let agent_ids: Vec<String> = tags
        .iter()
        .map(|t| {
            t.agent_id
                .as_ref()
                .map_or_else(String::new, |a| a.as_str().to_owned())
        })
        .collect();

    // Build flat vector array.
    let mut flat_values: Vec<f32> = Vec::with_capacity(len * vector_dim);
    for tag in tags {
        if tag.vector.len() == vector_dim {
            flat_values.extend_from_slice(&tag.vector);
        } else {
            flat_values.extend(std::iter::repeat_n(0.0_f32, vector_dim));
        }
    }
    let values_array = Float32Array::from(flat_values);
    let vector_list = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        vector_dim as i32,
        Arc::new(values_array) as ArrayRef,
        None,
    )
    .map_err(|e| UmmsError::Storage(StorageError::Lance(format!("Arrow error: {e}"))))?;

    let importances: Vec<f32> = tags.iter().map(|t| t.importance).collect();
    let frequencies: Vec<u64> = tags.iter().map(|t| t.frequency).collect();
    let created: Vec<String> = tags.iter().map(|t| t.created_at.to_rfc3339()).collect();
    let updated: Vec<String> = tags.iter().map(|t| t.updated_at.to_rfc3339()).collect();

    let batch = RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(StringArray::from(ids)) as ArrayRef,
            Arc::new(StringArray::from(labels)) as ArrayRef,
            Arc::new(StringArray::from(canonicals)) as ArrayRef,
            Arc::new(StringArray::from(
                agent_ids
                    .iter()
                    .map(std::string::String::as_str)
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(vector_list) as ArrayRef,
            Arc::new(Float32Array::from(importances)) as ArrayRef,
            Arc::new(UInt64Array::from(frequencies)) as ArrayRef,
            Arc::new(StringArray::from(
                created
                    .iter()
                    .map(std::string::String::as_str)
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(
                updated
                    .iter()
                    .map(std::string::String::as_str)
                    .collect::<Vec<_>>(),
            )) as ArrayRef,
        ],
    )
    .map_err(|e| UmmsError::Storage(StorageError::Lance(format!("Arrow error: {e}"))))?;

    Ok(batch)
}

/// Create an empty batch (zero rows) matching the schema. Used for table creation.
fn empty_batch(schema: &Arc<Schema>, vector_dim: usize) -> Result<RecordBatch> {
    let empty_str: Vec<&str> = vec![];

    let values_array = Float32Array::from(Vec::<f32>::new());
    let vector_list = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        vector_dim as i32,
        Arc::new(values_array) as ArrayRef,
        None,
    )
    .map_err(|e| UmmsError::Storage(StorageError::Lance(format!("Arrow error: {e}"))))?;

    let batch = RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef, // id
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef, // label
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef, // canonical
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef, // agent_id
            Arc::new(vector_list) as ArrayRef,                          // vector
            Arc::new(Float32Array::from(Vec::<f32>::new())) as ArrayRef, // importance
            Arc::new(UInt64Array::from(Vec::<u64>::new())) as ArrayRef, // frequency
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef, // created_at
            Arc::new(StringArray::from(empty_str)) as ArrayRef,         // updated_at
        ],
    )
    .map_err(|e| UmmsError::Storage(StorageError::Lance(format!("Arrow error: {e}"))))?;

    Ok(batch)
}

/// Extract [`Tag`] values from an Arrow [`RecordBatch`].
fn batch_to_tags(batch: &RecordBatch, _vector_dim: usize) -> Result<Vec<Tag>> {
    let n = batch.num_rows();
    if n == 0 {
        return Ok(Vec::new());
    }

    let col_str = |name: &str| -> std::result::Result<&StringArray, UmmsError> {
        batch
            .column_by_name(name)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                UmmsError::Storage(StorageError::Lance(format!("Missing column: {name}")))
            })
    };

    let ids = col_str("id")?;
    let labels = col_str("label")?;
    let canonicals = col_str("canonical")?;
    let agent_ids = col_str("agent_id")?;
    let importances = batch
        .column_by_name("importance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
        .ok_or_else(|| {
            UmmsError::Storage(StorageError::Lance("Missing column: importance".into()))
        })?;
    let frequencies = batch
        .column_by_name("frequency")
        .and_then(|c| c.as_any().downcast_ref::<UInt64Array>())
        .ok_or_else(|| {
            UmmsError::Storage(StorageError::Lance("Missing column: frequency".into()))
        })?;
    let created_col = col_str("created_at")?;
    let updated_col = col_str("updated_at")?;

    let vector_col = batch
        .column_by_name("vector")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>());

    let mut tags = Vec::with_capacity(n);
    for i in 0..n {
        let id = TagId::from_str(ids.value(i)).unwrap_or_else(|_| TagId::new());
        let label = labels.value(i).to_owned();
        let canonical = canonicals.value(i).to_owned();

        let agent_id_str = agent_ids.value(i);
        let agent_id = if agent_id_str.is_empty() {
            None
        } else {
            AgentId::from_str(agent_id_str).ok()
        };

        // Extract vector.
        let vector = vector_col
            .and_then(|vc| {
                if vc.is_null(i) {
                    return None;
                }
                let values = vc.value(i);
                let f32_arr = values.as_any().downcast_ref::<Float32Array>()?;
                Some(
                    (0..f32_arr.len())
                        .map(|j| f32_arr.value(j))
                        .collect::<Vec<f32>>(),
                )
            })
            .unwrap_or_default();

        let importance = importances.value(i);
        let frequency = frequencies.value(i);
        let created_at = DateTime::parse_from_rfc3339(created_col.value(i))
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let updated_at = DateTime::parse_from_rfc3339(updated_col.value(i))
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        tags.push(Tag {
            id,
            label,
            canonical,
            agent_id,
            vector,
            frequency,
            importance,
            created_at,
            updated_at,
        });
    }

    Ok(tags)
}

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------

/// Map a [`lancedb::Error`] into our [`UmmsError`].
fn lance_err<E: std::fmt::Display>(e: E) -> UmmsError {
    UmmsError::Storage(StorageError::Lance(e.to_string()))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::str::FromStr;

    fn temp_db_path(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "umms_lance_tag_test_{suffix}_{}",
            uuid::Uuid::new_v4()
        ));
        p
    }

    fn make_tag(label: &str, agent: Option<&str>, dim: usize, seed: u8) -> Tag {
        Tag {
            id: TagId::new(),
            label: label.to_owned(),
            canonical: Tag::canonicalize(label),
            agent_id: agent.map(|a| AgentId::from_str(a).unwrap()),
            vector: random_vector(dim, seed),
            frequency: 1,
            importance: 0.5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn random_vector(dim: usize, seed: u8) -> Vec<f32> {
        (0..dim)
            .map(|i| ((i as f32 + f32::from(seed)) * 0.1).sin())
            .collect()
    }

    #[tokio::test]
    async fn upsert_and_get_roundtrip() {
        let path = temp_db_path("roundtrip");
        let store = LanceTagStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let tag = make_tag("Rust ownership", Some("agent-a"), 8, 1);
        let id = tag.id.clone();

        store.upsert_tag(&tag).await.unwrap();

        let fetched = store.get_tag(&id).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.label, "Rust ownership");
        assert_eq!(fetched.canonical, "rust ownership");
    }

    #[tokio::test]
    async fn upsert_batch_and_count() {
        let path = temp_db_path("batch");
        let store = LanceTagStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let tags = vec![
            make_tag("Neural network", Some("agent-a"), 8, 1),
            make_tag("Deep learning", Some("agent-a"), 8, 2),
            make_tag("Shared tag", None, 8, 3),
        ];

        let ids = store.upsert_batch_tags(&tags).await.unwrap();
        assert_eq!(ids.len(), 3);

        let agent_id = AgentId::from_str("agent-a").unwrap();
        let count = store.count(Some(&agent_id)).await.unwrap();
        assert_eq!(count, 3); // 2 private + 1 shared (empty agent_id matches '')
    }

    #[tokio::test]
    async fn vector_search_returns_results() {
        let path = temp_db_path("search");
        let store = LanceTagStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let tag = make_tag("Rust ownership", Some("agent-a"), 8, 1);
        store.upsert_tag(&tag).await.unwrap();

        let query_vec = random_vector(8, 1); // Same seed = similar vector
        let agent_id = AgentId::from_str("agent-a").unwrap();
        let results = store
            .search_by_vector(&query_vec, Some(&agent_id), 5)
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert!(results[0].similarity > 0.0);
    }

    #[tokio::test]
    async fn find_by_label_works() {
        let path = temp_db_path("label");
        let store = LanceTagStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let tags = vec![
            make_tag("Rust ownership", Some("agent-a"), 8, 1),
            make_tag("Rust lifetimes", Some("agent-a"), 8, 2),
            make_tag("Python asyncio", Some("agent-a"), 8, 3),
        ];
        store.upsert_batch_tags(&tags).await.unwrap();

        let agent_id = AgentId::from_str("agent-a").unwrap();
        let found = store
            .find_by_label("rust", Some(&agent_id), 10)
            .await
            .unwrap();
        assert_eq!(found.len(), 2);
    }

    #[tokio::test]
    async fn upsert_updates_existing() {
        let path = temp_db_path("upsert_update");
        let store = LanceTagStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let mut tag = make_tag("Rust ownership", Some("agent-a"), 8, 1);
        let id = tag.id.clone();
        store.upsert_tag(&tag).await.unwrap();

        // Update frequency and importance.
        tag.frequency = 10;
        tag.importance = 0.9;
        store.upsert_tag(&tag).await.unwrap();

        let fetched = store.get_tag(&id).await.unwrap().unwrap();
        assert_eq!(fetched.frequency, 10);
        assert!((fetched.importance - 0.9).abs() < 0.01);

        // Should still be only one entry.
        let count = store.count(None).await.unwrap();
        assert_eq!(count, 1);
    }
}
