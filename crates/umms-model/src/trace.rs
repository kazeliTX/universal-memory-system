//! Per-request LLM tracing — records every model call for debugging and monitoring.
//!
//! Uses an in-memory ring buffer (similar to [`umms_observe::AuditLog`]) so there
//! is no persistent storage overhead. Data is lost on restart by design.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Trace record
// ---------------------------------------------------------------------------

/// A single LLM request/response trace record.
#[derive(Debug, Clone, Serialize)]
pub struct ModelTrace {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub model_id: String,
    pub model_name: String,
    pub provider: String,
    pub task: String,
    pub request_type: String,

    // Request info
    pub input_preview: String,
    pub input_tokens_estimate: usize,

    // Response info
    pub success: bool,
    pub error_message: Option<String>,
    pub output_preview: Option<String>,
    pub output_dimension: Option<usize>,
    pub output_tokens_estimate: Option<usize>,

    // Performance
    pub latency_ms: u64,
    pub retry_count: u32,

    // Context
    pub caller: String,
}

// ---------------------------------------------------------------------------
// TraceStore — thread-safe ring buffer
// ---------------------------------------------------------------------------

/// In-memory ring buffer for model traces.
pub struct TraceStore {
    traces: Mutex<Vec<ModelTrace>>,
    max_size: usize,
}

impl TraceStore {
    /// Create a new trace store with the given maximum capacity.
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self {
            traces: Mutex::new(Vec::with_capacity(max_size.min(4096))),
            max_size,
        }
    }

    /// Record a new trace. If the buffer is full, the oldest trace is evicted.
    pub fn record(&self, trace: ModelTrace) {
        let mut buf = self.traces.lock().expect("trace store lock poisoned");
        if buf.len() >= self.max_size {
            buf.remove(0);
        }
        buf.push(trace);
    }

    /// Get the most recent traces (newest first), up to `limit`.
    pub fn traces(&self, limit: usize) -> Vec<ModelTrace> {
        let buf = self.traces.lock().expect("trace store lock poisoned");
        buf.iter().rev().take(limit).cloned().collect()
    }

    /// Get traces filtered by model ID (newest first), up to `limit`.
    pub fn traces_by_model(&self, model_id: &str, limit: usize) -> Vec<ModelTrace> {
        let buf = self.traces.lock().expect("trace store lock poisoned");
        buf.iter()
            .rev()
            .filter(|t| t.model_id == model_id)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get traces filtered by task (newest first), up to `limit`.
    pub fn traces_by_task(&self, task: &str, limit: usize) -> Vec<ModelTrace> {
        let buf = self.traces.lock().expect("trace store lock poisoned");
        buf.iter()
            .rev()
            .filter(|t| t.task == task)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Clear all traces.
    pub fn clear(&self) {
        let mut buf = self.traces.lock().expect("trace store lock poisoned");
        buf.clear();
    }

    /// Compute aggregated summary statistics.
    pub fn summary(&self) -> TraceSummary {
        let buf = self.traces.lock().expect("trace store lock poisoned");

        let total_traces = buf.len();
        let total_errors = buf.iter().filter(|t| !t.success).count();

        // Per-model aggregation
        let mut by_model_map: HashMap<String, (usize, usize, u64)> = HashMap::new();
        for t in buf.iter() {
            let entry = by_model_map.entry(t.model_id.clone()).or_insert((0, 0, 0));
            entry.0 += 1;
            if !t.success {
                entry.1 += 1;
            }
            entry.2 += t.latency_ms;
        }
        let by_model: Vec<ModelTraceStat> = by_model_map
            .into_iter()
            .map(|(model_id, (count, errors, total_lat))| ModelTraceStat {
                model_id,
                count,
                errors,
                avg_latency_ms: if count > 0 {
                    total_lat as f64 / count as f64
                } else {
                    0.0
                },
            })
            .collect();

        // Per-task aggregation
        let mut by_task_map: HashMap<String, (usize, usize, u64)> = HashMap::new();
        for t in buf.iter() {
            let entry = by_task_map.entry(t.task.clone()).or_insert((0, 0, 0));
            entry.0 += 1;
            if !t.success {
                entry.1 += 1;
            }
            entry.2 += t.latency_ms;
        }
        let by_task: Vec<TaskTraceStat> = by_task_map
            .into_iter()
            .map(|(task, (count, errors, total_lat))| TaskTraceStat {
                task,
                count,
                errors,
                avg_latency_ms: if count > 0 {
                    total_lat as f64 / count as f64
                } else {
                    0.0
                },
            })
            .collect();

        // Global latency stats
        let avg_latency_ms = if total_traces > 0 {
            buf.iter().map(|t| t.latency_ms).sum::<u64>() as f64 / total_traces as f64
        } else {
            0.0
        };

        let p99_latency_ms = if total_traces > 0 {
            let mut latencies: Vec<u64> = buf.iter().map(|t| t.latency_ms).collect();
            latencies.sort_unstable();
            let idx = ((latencies.len() as f64) * 0.99).ceil() as usize;
            let idx = idx.min(latencies.len()).saturating_sub(1);
            latencies[idx] as f64
        } else {
            0.0
        };

        TraceSummary {
            total_traces,
            total_errors,
            by_model,
            by_task,
            avg_latency_ms,
            p99_latency_ms,
        }
    }
}

impl std::fmt::Debug for TraceStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraceStore")
            .field("traces", &"<locked>")
            .field("max_size", &self.max_size)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Summary types
// ---------------------------------------------------------------------------

/// Aggregated statistics from the trace buffer.
#[derive(Debug, Serialize)]
pub struct TraceSummary {
    pub total_traces: usize,
    pub total_errors: usize,
    pub by_model: Vec<ModelTraceStat>,
    pub by_task: Vec<TaskTraceStat>,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
}

/// Per-model statistics.
#[derive(Debug, Serialize)]
pub struct ModelTraceStat {
    pub model_id: String,
    pub count: usize,
    pub errors: usize,
    pub avg_latency_ms: f64,
}

/// Per-task statistics.
#[derive(Debug, Serialize)]
pub struct TaskTraceStat {
    pub task: String,
    pub count: usize,
    pub errors: usize,
    pub avg_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// Helper: build a trace
// ---------------------------------------------------------------------------

/// Helper to estimate token count from text.
/// Uses chars/4 for ASCII, chars/2 for CJK-heavy text.
pub fn estimate_tokens(text: &str) -> usize {
    let total_chars = text.chars().count();
    let cjk_chars = text
        .chars()
        .filter(|c| {
            matches!(*c as u32,
                0x4E00..=0x9FFF  // CJK Unified Ideographs
                | 0x3400..=0x4DBF  // CJK Extension A
                | 0x3000..=0x303F  // CJK Symbols
                | 0xFF00..=0xFFEF  // Fullwidth Forms
            )
        })
        .count();

    if cjk_chars * 2 > total_chars {
        // Majority CJK
        total_chars / 2
    } else {
        total_chars / 4
    }
    .max(1)
}

/// Truncate text to the given max chars, appending "..." if truncated.
pub fn truncate_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

/// Create a new trace ID.
pub fn new_trace_id() -> String {
    Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trace(model_id: &str, task: &str, success: bool, latency_ms: u64) -> ModelTrace {
        ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id: model_id.to_owned(),
            model_name: "test-model".to_owned(),
            provider: "test".to_owned(),
            task: task.to_owned(),
            request_type: "embed".to_owned(),
            input_preview: "test input".to_owned(),
            input_tokens_estimate: 3,
            success,
            error_message: if success {
                None
            } else {
                Some("test error".to_owned())
            },
            output_preview: None,
            output_dimension: Some(3072),
            output_tokens_estimate: None,
            latency_ms,
            retry_count: 0,
            caller: "test".to_owned(),
        }
    }

    #[test]
    fn store_record_and_retrieve() {
        let store = TraceStore::new(10);
        store.record(make_trace("m1", "embedding", true, 100));
        store.record(make_trace("m2", "generation", true, 200));

        let all = store.traces(100);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].model_id, "m2"); // newest first
    }

    #[test]
    fn store_ring_buffer_eviction() {
        let store = TraceStore::new(3);
        for i in 0..5 {
            store.record(make_trace(&format!("m{i}"), "embedding", true, 100));
        }
        let all = store.traces(100);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].model_id, "m4"); // newest
        assert_eq!(all[2].model_id, "m2"); // oldest remaining
    }

    #[test]
    fn store_filter_by_model() {
        let store = TraceStore::new(10);
        store.record(make_trace("m1", "embedding", true, 100));
        store.record(make_trace("m2", "generation", true, 200));
        store.record(make_trace("m1", "embedding", true, 150));

        let filtered = store.traces_by_model("m1", 100);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn store_filter_by_task() {
        let store = TraceStore::new(10);
        store.record(make_trace("m1", "embedding", true, 100));
        store.record(make_trace("m2", "generation", true, 200));

        let filtered = store.traces_by_task("generation", 100);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn store_summary() {
        let store = TraceStore::new(10);
        store.record(make_trace("m1", "embedding", true, 100));
        store.record(make_trace("m1", "embedding", false, 200));
        store.record(make_trace("m2", "generation", true, 300));

        let summary = store.summary();
        assert_eq!(summary.total_traces, 3);
        assert_eq!(summary.total_errors, 1);
        assert_eq!(summary.by_model.len(), 2);
        assert_eq!(summary.by_task.len(), 2);
    }

    #[test]
    fn store_clear() {
        let store = TraceStore::new(10);
        store.record(make_trace("m1", "embedding", true, 100));
        store.clear();
        assert_eq!(store.traces(100).len(), 0);
    }

    #[test]
    fn estimate_tokens_ascii() {
        assert_eq!(estimate_tokens("hello world"), 2); // 11/4 = 2
    }

    #[test]
    fn truncate_short_text() {
        assert_eq!(truncate_preview("hi", 200), "hi");
    }

    #[test]
    fn truncate_long_text() {
        let long = "a".repeat(300);
        let result = truncate_preview(&long, 200);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), 203); // 200 + "..."
    }
}
