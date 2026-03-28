//! Runtime statistics for model providers.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

/// Runtime statistics exposed for the dashboard / metrics.
#[derive(Debug, Default)]
pub struct EncoderStats {
    pub total_requests: AtomicU64,
    pub total_texts_encoded: AtomicU64,
    pub total_errors: AtomicU64,
    pub total_retries: AtomicU64,
    /// Cumulative encoding time in microseconds.
    pub total_duration_us: AtomicU64,
}

impl EncoderStats {
    pub fn snapshot(&self) -> EncoderStatsSnapshot {
        let reqs = self.total_requests.load(Ordering::Relaxed);
        let dur = self.total_duration_us.load(Ordering::Relaxed);
        EncoderStatsSnapshot {
            total_requests: reqs,
            total_texts_encoded: self.total_texts_encoded.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            total_retries: self.total_retries.load(Ordering::Relaxed),
            avg_latency_ms: if reqs > 0 {
                dur as f64 / reqs as f64 / 1000.0
            } else {
                0.0
            },
        }
    }
}

/// Point-in-time snapshot of encoder stats (safe to serialize).
#[derive(Debug, Clone, Serialize)]
pub struct EncoderStatsSnapshot {
    pub total_requests: u64,
    pub total_texts_encoded: u64,
    pub total_errors: u64,
    pub total_retries: u64,
    pub avg_latency_ms: f64,
}
