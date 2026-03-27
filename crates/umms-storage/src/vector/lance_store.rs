//! LanceDB-backed vector store implementing [`VectorStore`] from `umms-core`.
//!
//! Stores memory entries in a single `memories` table with an Arrow-based schema.
//! Vector search uses LanceDB's built-in ANN index; non-vector queries use
//! filtered full-table scans.

use std::sync::Arc;

use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Table as LanceTable;
use tokio::sync::Mutex;
use tracing::instrument;

use umms_core::error::{Result, StorageError, UmmsError};
use umms_core::traits::VectorStore;
use umms_core::types::*;

// ---------------------------------------------------------------------------
// LanceVectorStore
// ---------------------------------------------------------------------------

/// LanceDB-backed implementation of [`VectorStore`].
///
/// Thread-safe: the inner [`LanceTable`] handle is wrapped in an `Arc<Mutex<_>>`
/// to serialise writes (reads could be concurrent, but LanceDB's Rust client
/// recommends single-writer access).
pub struct LanceVectorStore {
    table: Arc<Mutex<LanceTable>>,
    schema: Arc<Schema>,
    vector_dim: usize,
}

impl LanceVectorStore {
    /// Connect to (or create) a LanceDB database at `db_path` and open the
    /// `memories` table. If the table does not exist it is created with an
    /// empty initial batch.
    pub async fn new(db_path: &str, vector_dim: usize) -> std::result::Result<Self, UmmsError> {
        let schema = Arc::new(build_schema(vector_dim));

        let db = lancedb::connect(db_path)
            .execute()
            .await
            .map_err(lance_err)?;

        let table = match db.open_table("memories").execute().await {
            Ok(t) => {
                // Validate dimension matches. If the existing table was created
                // with a different dimension, drop it and recreate.
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
                        "Vector dimension mismatch — dropping and recreating table"
                    );
                    let _ = db.drop_table("memories", &[]).await;
                    let batch = empty_batch(&schema, vector_dim)?;
                    db.create_table("memories", vec![batch])
                        .execute()
                        .await
                        .map_err(lance_err)?
                }
            }
            Err(_) => {
                // Table doesn't exist yet — create with an empty batch.
                let batch = empty_batch(&schema, vector_dim)?;
                db.create_table("memories", vec![batch])
                    .execute()
                    .await
                    .map_err(lance_err)?
            }
        };

        Ok(Self {
            table: Arc::new(Mutex::new(table)),
            schema,
            vector_dim,
        })
    }
}

// ---------------------------------------------------------------------------
// VectorStore implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl VectorStore for LanceVectorStore {
    #[instrument(skip(self, entry), fields(id = %entry.id, agent = %entry.agent_id))]
    async fn insert(&self, entry: &MemoryEntry) -> Result<()> {
        let batch = entries_to_batch(std::slice::from_ref(entry), &self.schema, self.vector_dim)?;
        let table = self.table.lock().await;
        table.add(vec![batch]).execute().await.map_err(lance_err)?;
        Ok(())
    }

    #[instrument(skip(self, entries), fields(count = entries.len()))]
    async fn insert_batch(&self, entries: &[MemoryEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let batch = entries_to_batch(entries, &self.schema, self.vector_dim)?;
        let table = self.table.lock().await;
        table.add(vec![batch]).execute().await.map_err(lance_err)?;
        Ok(())
    }

    #[instrument(skip(self, vector), fields(agent = %agent_id, top_k, include_shared))]
    async fn search(
        &self,
        agent_id: &AgentId,
        vector: &[f32],
        top_k: usize,
        include_shared: bool,
    ) -> Result<Vec<ScoredMemory>> {
        let filter = if include_shared {
            format!(
                "(agent_id = '{}' OR scope = 'shared')",
                agent_id.as_str()
            )
        } else {
            format!("agent_id = '{}'", agent_id.as_str())
        };

        let table = self.table.lock().await;
        let results = table
            .vector_search(vector)
            .map_err(lance_err)?
            .limit(top_k)
            .only_if(filter)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results
            .try_collect::<Vec<_>>()
            .await
            .map_err(lance_err)?;

        let mut scored = Vec::new();
        for batch in &batches {
            let entries = batch_to_entries(batch, self.vector_dim)?;
            // LanceDB returns a `_distance` column (L2 distance). Convert to a
            // similarity score: score = 1 / (1 + distance).
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            for (i, entry) in entries.into_iter().enumerate() {
                // Skip zero-vector placeholders (entries that had no real embedding).
                if entry.vector.as_ref().map_or(true, |v| v.iter().all(|&x| x == 0.0)) {
                    continue;
                }
                let distance = distances.map_or(0.0, |d| d.value(i));
                let score = 1.0 / (1.0 + distance);
                scored.push(ScoredMemory {
                    entry,
                    score,
                    source: ScoreSource::Vector,
                });
            }
        }

        Ok(scored)
    }

    #[instrument(skip(self), fields(id = %id))]
    async fn delete(&self, id: &MemoryId) -> Result<()> {
        let table = self.table.lock().await;
        table
            .delete(&format!("id = '{}'", id.as_str()))
            .await
            .map_err(lance_err)?;
        Ok(())
    }

    #[instrument(skip(self), fields(id = %id))]
    async fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>> {
        let table = self.table.lock().await;
        let filter = format!("id = '{}'", id.as_str());
        let results = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results
            .try_collect::<Vec<_>>()
            .await
            .map_err(lance_err)?;

        for batch in &batches {
            let entries = batch_to_entries(batch, self.vector_dim)?;
            if let Some(entry) = entries.into_iter().next() {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    #[instrument(skip(self), fields(id = %id))]
    async fn update_metadata(
        &self,
        id: &MemoryId,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
        scope: Option<IsolationScope>,
        agent_id: Option<AgentId>,
    ) -> Result<()> {
        let table = self.table.lock().await;
        let filter = format!("id = '{}'", id.as_str());

        if let Some(imp) = importance {
            table
                .update()
                .only_if(&filter)
                .column("importance", imp.to_string())
                .execute()
                .await
                .map_err(lance_err)?;
        }

        if let Some(t) = tags {
            let json = serde_json::to_string(&t).unwrap_or_else(|_| "[]".to_owned());
            table
                .update()
                .only_if(&filter)
                .column("tags", format!("'{json}'"))
                .execute()
                .await
                .map_err(lance_err)?;
        }

        if let Some(s) = scope {
            let s_str = serde_json::to_value(s)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "private".to_owned());
            table
                .update()
                .only_if(&filter)
                .column("scope", format!("'{s_str}'"))
                .execute()
                .await
                .map_err(lance_err)?;
        }

        if let Some(aid) = agent_id {
            table
                .update()
                .only_if(&filter)
                .column("agent_id", format!("'{}'", aid.as_str()))
                .execute()
                .await
                .map_err(lance_err)?;
        }

        Ok(())
    }

    async fn update_user_rating(
        &self,
        id: &MemoryId,
        rating: Option<f32>,
    ) -> Result<()> {
        let table = self.table.lock().await;
        let filter = format!("id = '{}'", id.as_str());
        match rating {
            Some(r) => {
                table
                    .update()
                    .only_if(&filter)
                    .column("user_rating", r.to_string())
                    .execute()
                    .await
                    .map_err(lance_err)?;
            }
            None => {
                table
                    .update()
                    .only_if(&filter)
                    .column("user_rating", "null".to_string())
                    .execute()
                    .await
                    .map_err(lance_err)?;
            }
        }
        Ok(())
    }

    #[instrument(skip(self), fields(agent = %agent_id, include_shared))]
    async fn count(&self, agent_id: &AgentId, include_shared: bool) -> Result<u64> {
        let filter = if include_shared {
            format!(
                "(agent_id = '{}' OR scope = 'shared')",
                agent_id.as_str()
            )
        } else {
            format!("agent_id = '{}'", agent_id.as_str())
        };

        let table = self.table.lock().await;
        let results = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results
            .try_collect::<Vec<_>>()
            .await
            .map_err(lance_err)?;
        let total: usize = batches.iter().map(RecordBatch::num_rows).sum();
        Ok(total as u64)
    }

    #[instrument(skip(self), fields(agent = %agent_id, offset, limit, include_shared))]
    async fn list(
        &self,
        agent_id: &AgentId,
        offset: u64,
        limit: u64,
        include_shared: bool,
    ) -> Result<Vec<MemoryEntry>> {
        let filter = if include_shared {
            format!(
                "(agent_id = '{}' OR scope = 'shared')",
                agent_id.as_str()
            )
        } else {
            format!("agent_id = '{}'", agent_id.as_str())
        };

        let table = self.table.lock().await;
        let results = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(lance_err)?;

        let batches: Vec<RecordBatch> = results
            .try_collect::<Vec<_>>()
            .await
            .map_err(lance_err)?;

        let mut all_entries = Vec::new();
        for batch in &batches {
            let entries = batch_to_entries(batch, self.vector_dim)?;
            all_entries.extend(entries);
        }

        // Sort by created_at descending (newest first)
        all_entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply offset/limit
        let start = (offset as usize).min(all_entries.len());
        let end = (start + limit as usize).min(all_entries.len());
        Ok(all_entries[start..end].to_vec())
    }

    #[instrument(skip(self), fields(agent = %agent_id, include_shared))]
    async fn delete_all(&self, agent_id: &AgentId, include_shared: bool) -> Result<u64> {
        let count = self.count(agent_id, include_shared).await?;
        if count == 0 {
            return Ok(0);
        }

        let filter = if include_shared {
            format!(
                "(agent_id = '{}' OR scope = 'shared')",
                agent_id.as_str()
            )
        } else {
            format!("agent_id = '{}'", agent_id.as_str())
        };

        let table = self.table.lock().await;
        table
            .delete(&filter)
            .await
            .map_err(lance_err)?;

        tracing::info!(agent_id = %agent_id, deleted = count, "deleted vector entries");
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Arrow schema helpers
// ---------------------------------------------------------------------------

fn build_schema(vector_dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("agent_id", DataType::Utf8, false),
        Field::new("layer", DataType::Utf8, false),
        Field::new("scope", DataType::Utf8, false),
        Field::new("modality", DataType::Utf8, false),
        Field::new("content_text", DataType::Utf8, true),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim as i32,
            ),
            true,
        ),
        Field::new("importance", DataType::Float32, false),
        Field::new("decay_category", DataType::Utf8, false),
        Field::new("tags", DataType::Utf8, false),
        Field::new("metadata", DataType::Utf8, false),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("accessed_at", DataType::Utf8, false),
        Field::new("access_count", DataType::UInt64, false),
        Field::new("user_rating", DataType::Float32, true),
    ])
}

// ---------------------------------------------------------------------------
// MemoryEntry <-> RecordBatch conversion
// ---------------------------------------------------------------------------

/// Convert a slice of [`MemoryEntry`] to an Arrow [`RecordBatch`].
fn entries_to_batch(
    entries: &[MemoryEntry],
    schema: &Arc<Schema>,
    vector_dim: usize,
) -> Result<RecordBatch> {
    let len = entries.len();

    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let agent_ids: Vec<&str> = entries.iter().map(|e| e.agent_id.as_str()).collect();
    let layers: Vec<String> = entries
        .iter()
        .map(|e| serialize_enum(&e.layer))
        .collect();
    let scopes: Vec<String> = entries
        .iter()
        .map(|e| serialize_enum(&e.scope))
        .collect();
    let modalities: Vec<String> = entries
        .iter()
        .map(|e| serialize_enum(&e.modality))
        .collect();
    let content_texts: Vec<Option<&str>> = entries
        .iter()
        .map(|e| e.content_text.as_deref())
        .collect();

    // Build flat vector array: entries without a vector get a zero-vector placeholder.
    let mut flat_values: Vec<f32> = Vec::with_capacity(len * vector_dim);
    for entry in entries {
        match &entry.vector {
            Some(v) if v.len() == vector_dim => flat_values.extend_from_slice(v),
            _ => flat_values.extend(std::iter::repeat(0.0_f32).take(vector_dim)),
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

    let importances: Vec<f32> = entries.iter().map(|e| e.importance).collect();
    let decay_cats: Vec<String> = entries
        .iter()
        .map(|e| serialize_enum(&e.decay_category))
        .collect();
    let tags: Vec<String> = entries
        .iter()
        .map(|e| serde_json::to_string(&e.tags).unwrap_or_else(|_| "[]".to_owned()))
        .collect();
    let metadata: Vec<String> = entries
        .iter()
        .map(|e| serde_json::to_string(&e.metadata).unwrap_or_else(|_| "null".to_owned()))
        .collect();
    let created: Vec<String> = entries.iter().map(|e| e.created_at.to_rfc3339()).collect();
    let accessed: Vec<String> = entries.iter().map(|e| e.accessed_at.to_rfc3339()).collect();
    let access_counts: Vec<u64> = entries.iter().map(|e| e.access_count).collect();
    let user_ratings: Vec<Option<f32>> = entries.iter().map(|e| e.user_rating).collect();

    let batch = RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(StringArray::from(ids)) as ArrayRef,
            Arc::new(StringArray::from(agent_ids)) as ArrayRef,
            Arc::new(StringArray::from(layers.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
                as ArrayRef,
            Arc::new(StringArray::from(scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
                as ArrayRef,
            Arc::new(StringArray::from(
                modalities.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(content_texts)) as ArrayRef,
            Arc::new(vector_list) as ArrayRef,
            Arc::new(Float32Array::from(importances)) as ArrayRef,
            Arc::new(StringArray::from(
                decay_cats.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(tags.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
                as ArrayRef,
            Arc::new(StringArray::from(
                metadata.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(
                created.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(StringArray::from(
                accessed.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )) as ArrayRef,
            Arc::new(UInt64Array::from(access_counts)) as ArrayRef,
            Arc::new(Float32Array::from(user_ratings)) as ArrayRef,
        ],
    )
    .map_err(|e| UmmsError::Storage(StorageError::Lance(format!("Arrow error: {e}"))))?;

    Ok(batch)
}

/// Create an empty batch (zero rows) matching the schema. Used for table creation.
fn empty_batch(schema: &Arc<Schema>, vector_dim: usize) -> Result<RecordBatch> {
    let empty_str: Vec<&str> = vec![];
    let empty_opt_str: Vec<Option<&str>> = vec![];

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
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_opt_str)) as ArrayRef,
            Arc::new(vector_list) as ArrayRef,
            Arc::new(Float32Array::from(Vec::<f32>::new())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str.clone())) as ArrayRef,
            Arc::new(StringArray::from(empty_str)) as ArrayRef,
            Arc::new(UInt64Array::from(Vec::<u64>::new())) as ArrayRef,
            Arc::new(Float32Array::from(Vec::<Option<f32>>::new())) as ArrayRef,
        ],
    )
    .map_err(|e| UmmsError::Storage(StorageError::Lance(format!("Arrow error: {e}"))))?;

    Ok(batch)
}

/// Extract [`MemoryEntry`] values from an Arrow [`RecordBatch`].
///
/// The batch may have extra columns (e.g. `_distance` from vector search) —
/// we only read the columns we know about.
fn batch_to_entries(batch: &RecordBatch, _vector_dim: usize) -> Result<Vec<MemoryEntry>> {
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
    let agent_ids = col_str("agent_id")?;
    let layers = col_str("layer")?;
    let scopes = col_str("scope")?;
    let modalities = col_str("modality")?;
    let content_texts = col_str("content_text")?;
    let importances = batch
        .column_by_name("importance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
        .ok_or_else(|| {
            UmmsError::Storage(StorageError::Lance("Missing column: importance".into()))
        })?;
    let decay_cats = col_str("decay_category")?;
    let tags_col = col_str("tags")?;
    let meta_col = col_str("metadata")?;
    let created_col = col_str("created_at")?;
    let accessed_col = col_str("accessed_at")?;
    let access_counts = batch
        .column_by_name("access_count")
        .and_then(|c| c.as_any().downcast_ref::<UInt64Array>())
        .ok_or_else(|| {
            UmmsError::Storage(StorageError::Lance("Missing column: access_count".into()))
        })?;

    // user_rating is optional (nullable, may be absent in older tables).
    let user_rating_col = batch
        .column_by_name("user_rating")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

    let vector_col = batch
        .column_by_name("vector")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>());

    let mut entries = Vec::with_capacity(n);
    for i in 0..n {
        let id = MemoryId::from_str(ids.value(i))
            .unwrap_or_else(|_| MemoryId::from_str("unknown").unwrap());
        let agent_id = AgentId::from_str(agent_ids.value(i))
            .unwrap_or_else(|_| AgentId::from_str("unknown").unwrap());
        let layer: MemoryLayer = deserialize_enum(layers.value(i));
        let scope: IsolationScope = deserialize_enum(scopes.value(i));
        let modality: Modality = deserialize_enum(modalities.value(i));
        let content_text = if content_texts.is_null(i) {
            None
        } else {
            Some(content_texts.value(i).to_owned())
        };

        // Extract vector
        let vector = vector_col.and_then(|vc| {
            if vc.is_null(i) {
                return None;
            }
            let values = vc.value(i);
            let f32_arr = values.as_any().downcast_ref::<Float32Array>()?;
            let v: Vec<f32> = (0..f32_arr.len()).map(|j| f32_arr.value(j)).collect();
            // Treat all-zero vectors as None (placeholder).
            if v.iter().all(|&x| x == 0.0) {
                None
            } else {
                Some(v)
            }
        });

        let importance = importances.value(i);
        let decay_category: DecayCategory = deserialize_enum(decay_cats.value(i));
        let tags_parsed: Vec<String> =
            serde_json::from_str(tags_col.value(i)).unwrap_or_default();
        let metadata_parsed: serde_json::Value =
            serde_json::from_str(meta_col.value(i)).unwrap_or(serde_json::Value::Null);
        let created_at = DateTime::parse_from_rfc3339(created_col.value(i))
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let accessed_at = DateTime::parse_from_rfc3339(accessed_col.value(i))
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let access_count = access_counts.value(i);
        let user_rating = user_rating_col.and_then(|col| {
            if col.is_null(i) { None } else { Some(col.value(i)) }
        });

        entries.push(MemoryEntry {
            id,
            agent_id,
            layer,
            scope,
            modality,
            content_text,
            vector,
            importance,
            decay_category,
            tags: tags_parsed,
            metadata: metadata_parsed,
            created_at,
            accessed_at,
            access_count,
            user_rating,
        });
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

/// Serialize an enum to its serde `snake_case` string form.
fn serialize_enum<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_value(val)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// Deserialize an enum from its serde `snake_case` string form.
fn deserialize_enum<T: serde::de::DeserializeOwned>(s: &str) -> T {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .expect("enum deserialization should not fail for known variants")
}

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

    /// Create a temp directory for a test LanceDB instance.
    fn temp_db_path(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("umms_lance_test_{suffix}_{}", uuid::Uuid::new_v4()));
        p
    }

    fn make_entry(
        agent: &str,
        scope: IsolationScope,
        vector: Option<Vec<f32>>,
        dim: usize,
    ) -> MemoryEntry {
        let agent_id = AgentId::from_str(agent).unwrap();
        let mut builder = MemoryEntryBuilder::new(agent_id, Modality::Text)
            .layer(MemoryLayer::EpisodicMemory)
            .scope(scope)
            .content_text("test content")
            .importance(0.7)
            .tags(vec!["tag1".into()])
            .metadata(serde_json::json!({"key": "value"}));
        if let Some(v) = vector {
            builder = builder.vector(v);
        } else {
            // No vector — store will use zero-vector placeholder.
        }
        let _ = dim; // used by caller to size vectors
        builder.build()
    }

    fn random_vector(dim: usize, seed: u8) -> Vec<f32> {
        (0..dim)
            .map(|i| ((i as f32 + f32::from(seed)) * 0.1).sin())
            .collect()
    }

    // -----------------------------------------------------------------------
    // insert + get roundtrip
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn insert_and_get_roundtrip() {
        let path = temp_db_path("roundtrip");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let entry = make_entry("agent-a", IsolationScope::Private, Some(random_vector(8, 1)), 8);
        let id = entry.id.clone();

        store.insert(&entry).await.unwrap();

        let fetched = store.get(&id).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.agent_id.as_str(), "agent-a");
        assert_eq!(fetched.content_text.as_deref(), Some("test content"));
        assert!(fetched.vector.is_some());
        assert_eq!(fetched.tags, vec!["tag1".to_string()]);

        // Cleanup
        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // vector search
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn search_by_vector_similarity() {
        let path = temp_db_path("search");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let agent = AgentId::from_str("agent-a").unwrap();

        // Insert two entries with different vectors.
        let v1 = random_vector(8, 1);
        let v2 = random_vector(8, 100);
        let e1 = make_entry("agent-a", IsolationScope::Private, Some(v1.clone()), 8);
        let e2 = make_entry("agent-a", IsolationScope::Private, Some(v2), 8);

        store.insert(&e1).await.unwrap();
        store.insert(&e2).await.unwrap();

        // Search with v1 — should return e1 as top result.
        let results = store.search(&agent, &v1, 2, false).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].entry.id, e1.id);

        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // agent isolation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn agent_isolation() {
        let path = temp_db_path("isolation");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let v = random_vector(8, 1);
        let e_a = make_entry("agent-a", IsolationScope::Private, Some(v.clone()), 8);
        let e_b = make_entry("agent-b", IsolationScope::Private, Some(v.clone()), 8);

        store.insert(&e_a).await.unwrap();
        store.insert(&e_b).await.unwrap();

        let agent_a = AgentId::from_str("agent-a").unwrap();
        let results = store.search(&agent_a, &v, 10, false).await.unwrap();

        // Only agent-a entries should be returned.
        for r in &results {
            assert_eq!(r.entry.agent_id.as_str(), "agent-a");
        }

        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // include_shared returns shared entries
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn search_include_shared() {
        let path = temp_db_path("shared");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let v = random_vector(8, 1);
        let e_private = make_entry("agent-a", IsolationScope::Private, Some(v.clone()), 8);
        let e_shared = make_entry("agent-b", IsolationScope::Shared, Some(v.clone()), 8);

        store.insert(&e_private).await.unwrap();
        store.insert(&e_shared).await.unwrap();

        let agent_a = AgentId::from_str("agent-a").unwrap();

        // Without shared: only agent-a's private entry.
        let results = store.search(&agent_a, &v, 10, false).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.agent_id.as_str(), "agent-a");

        // With shared: should also return agent-b's shared entry.
        let results = store.search(&agent_a, &v, 10, true).await.unwrap();
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // delete
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn delete_removes_entry() {
        let path = temp_db_path("delete");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let entry = make_entry("agent-a", IsolationScope::Private, Some(random_vector(8, 1)), 8);
        let id = entry.id.clone();

        store.insert(&entry).await.unwrap();
        assert!(store.get(&id).await.unwrap().is_some());

        store.delete(&id).await.unwrap();
        assert!(store.get(&id).await.unwrap().is_none());

        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // update_metadata
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn update_metadata_changes_importance() {
        let path = temp_db_path("update");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let entry = make_entry("agent-a", IsolationScope::Private, Some(random_vector(8, 1)), 8);
        let id = entry.id.clone();
        store.insert(&entry).await.unwrap();

        store
            .update_metadata(&id, Some(0.95), None, None, None)
            .await
            .unwrap();

        let updated = store.get(&id).await.unwrap().unwrap();
        assert!((updated.importance - 0.95).abs() < 0.01);

        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // count
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn count_returns_correct_numbers() {
        let path = temp_db_path("count");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let agent = AgentId::from_str("agent-a").unwrap();

        assert_eq!(store.count(&agent, false).await.unwrap(), 0);

        store
            .insert(&make_entry("agent-a", IsolationScope::Private, Some(random_vector(8, 1)), 8))
            .await
            .unwrap();
        store
            .insert(&make_entry("agent-a", IsolationScope::Private, Some(random_vector(8, 2)), 8))
            .await
            .unwrap();
        store
            .insert(&make_entry("agent-b", IsolationScope::Shared, Some(random_vector(8, 3)), 8))
            .await
            .unwrap();

        assert_eq!(store.count(&agent, false).await.unwrap(), 2);
        assert_eq!(store.count(&agent, true).await.unwrap(), 3);

        let _ = std::fs::remove_dir_all(&path);
    }

    // -----------------------------------------------------------------------
    // insert_batch
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn insert_batch_inserts_multiple() {
        let path = temp_db_path("batch");
        let store = LanceVectorStore::new(path.to_str().unwrap(), 8).await.unwrap();

        let entries: Vec<MemoryEntry> = (0..5)
            .map(|i| {
                make_entry(
                    "agent-a",
                    IsolationScope::Private,
                    Some(random_vector(8, i)),
                    8,
                )
            })
            .collect();

        store.insert_batch(&entries).await.unwrap();

        let agent = AgentId::from_str("agent-a").unwrap();
        assert_eq!(store.count(&agent, false).await.unwrap(), 5);

        let _ = std::fs::remove_dir_all(&path);
    }
}
