//! BM25 sparse retrieval powered by tantivy.
//!
//! Maintains an in-process full-text index that mirrors the vector store.
//! When a memory is inserted into the vector store, it should also be
//! indexed here via `index_entry`. The index lives in-memory by default
//! but can be persisted to disk.

use std::sync::Arc;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use tokio::sync::Mutex;
use tracing::instrument;

use umms_core::error::{Result, UmmsError};
use umms_core::types::{AgentId, MemoryEntry};

/// BM25 full-text search index.
pub struct Bm25Index {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    // Schema fields
    f_id: Field,
    f_agent_id: Field,
    f_content: Field,
    f_scope: Field,
}

impl Bm25Index {
    /// Create a new in-memory BM25 index.
    pub fn new() -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let f_id = schema_builder.add_text_field("id", STRING | STORED);
        let f_agent_id = schema_builder.add_text_field("agent_id", STRING | STORED);
        let f_content = schema_builder.add_text_field("content", TEXT | STORED);
        let f_scope = schema_builder.add_text_field("scope", STRING | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let writer = index
            .writer(15_000_000) // 15 MB heap
            .map_err(|e| UmmsError::Internal(format!("BM25 writer init failed: {e}")))?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| UmmsError::Internal(format!("BM25 reader init failed: {e}")))?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            f_id,
            f_agent_id,
            f_content,
            f_scope,
        })
    }

    /// Index a memory entry. Call this whenever a new memory is inserted.
    #[instrument(skip(self, entry), fields(id = %entry.id))]
    pub async fn index_entry(&self, entry: &MemoryEntry) -> Result<()> {
        let content = entry.content_text.as_deref().unwrap_or("");
        if content.is_empty() {
            return Ok(());
        }

        let scope_str = serde_json::to_value(&entry.scope)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "private".to_owned());

        let mut writer = self.writer.lock().await;
        writer.add_document(doc!(
            self.f_id => entry.id.as_str(),
            self.f_agent_id => entry.agent_id.as_str(),
            self.f_content => content,
            self.f_scope => scope_str,
        )).map_err(|e| UmmsError::Internal(format!("BM25 index failed: {e}")))?;

        writer
            .commit()
            .map_err(|e| UmmsError::Internal(format!("BM25 commit failed: {e}")))?;
        self.reader
            .reload()
            .map_err(|e| UmmsError::Internal(format!("BM25 reload failed: {e}")))?;

        Ok(())
    }

    /// Index a batch of entries.
    pub async fn index_batch(&self, entries: &[MemoryEntry]) -> Result<()> {
        let mut writer = self.writer.lock().await;
        for entry in entries {
            let content = entry.content_text.as_deref().unwrap_or("");
            if content.is_empty() {
                continue;
            }
            let scope_str = serde_json::to_value(&entry.scope)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "private".to_owned());

            writer.add_document(doc!(
                self.f_id => entry.id.as_str(),
                self.f_agent_id => entry.agent_id.as_str(),
                self.f_content => content,
                self.f_scope => scope_str,
            )).map_err(|e| UmmsError::Internal(format!("BM25 index failed: {e}")))?;
        }
        writer
            .commit()
            .map_err(|e| UmmsError::Internal(format!("BM25 commit failed: {e}")))?;
        self.reader
            .reload()
            .map_err(|e| UmmsError::Internal(format!("BM25 reload failed: {e}")))?;
        Ok(())
    }

    /// Search the BM25 index, scoped to an agent + shared entries.
    #[instrument(skip(self), fields(query, agent = %agent_id, top_k))]
    pub fn search(
        &self,
        query: &str,
        agent_id: &AgentId,
        top_k: usize,
        include_shared: bool,
    ) -> Result<Vec<(String, f32)>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.f_content]);

        let parsed = query_parser
            .parse_query(query)
            .map_err(|e| UmmsError::Internal(format!("BM25 query parse failed: {e}")))?;

        let top_docs = searcher
            .search(&parsed, &TopDocs::with_limit(top_k * 3)) // over-fetch, then filter by agent
            .map_err(|e| UmmsError::Internal(format!("BM25 search failed: {e}")))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc::<TantivyDocument>(doc_address)
                .map_err(|e| UmmsError::Internal(format!("BM25 doc read failed: {e}")))?;

            let doc_agent = doc
                .get_first(self.f_agent_id)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let doc_scope = doc
                .get_first(self.f_scope)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Agent isolation filter
            let agent_match = doc_agent == agent_id.as_str();
            let shared_match = include_shared && doc_scope == "shared";
            if !agent_match && !shared_match {
                continue;
            }

            let id = doc
                .get_first(self.f_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();

            results.push((id, score));
            if results.len() >= top_k {
                break;
            }
        }

        Ok(results)
    }

    /// Number of documents in the index.
    pub fn doc_count(&self) -> u64 {
        let searcher = self.reader.searcher();
        searcher.num_docs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use umms_core::types::{MemoryEntryBuilder, Modality};

    fn make_entry(agent: &str, text: &str) -> MemoryEntry {
        let aid = AgentId::from_str(agent).unwrap();
        MemoryEntryBuilder::new(aid, Modality::Text)
            .content_text(text.to_owned())
            .build()
    }

    #[tokio::test]
    async fn basic_search() {
        let idx = Bm25Index::new().unwrap();
        let entries = vec![
            make_entry("coder", "Rust ownership model prevents data races"),
            make_entry("coder", "Python asyncio event loop"),
            make_entry("coder", "Rust borrow checker ensures memory safety"),
        ];
        idx.index_batch(&entries).await.unwrap();

        let agent = AgentId::from_str("coder").unwrap();
        let results = idx.search("Rust memory safety", &agent, 5, false).unwrap();

        assert!(!results.is_empty());
        // The Rust-related entries should rank higher
    }

    #[tokio::test]
    async fn agent_isolation() {
        let idx = Bm25Index::new().unwrap();
        idx.index_entry(&make_entry("coder", "Rust systems programming"))
            .await
            .unwrap();
        idx.index_entry(&make_entry("writer", "Creative writing techniques"))
            .await
            .unwrap();

        let coder = AgentId::from_str("coder").unwrap();
        let results = idx.search("programming", &coder, 10, false).unwrap();
        assert_eq!(results.len(), 1);

        // "Creative writing" should not appear for coder
        let results = idx.search("writing", &coder, 10, false).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn empty_content_skipped() {
        let idx = Bm25Index::new().unwrap();
        let mut entry = make_entry("coder", "");
        entry.content_text = None;
        idx.index_entry(&entry).await.unwrap();
        assert_eq!(idx.doc_count(), 0);
    }
}
