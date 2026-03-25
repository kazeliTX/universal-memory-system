//! Prometheus-based metrics for UMMS.

use std::sync::OnceLock;

use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, gauge::Gauge, histogram::Histogram},
    registry::Registry,
};

/// Core metrics collected across UMMS services.
pub struct UmmsMetrics {
    /// Total memory write operations, labelled by layer.
    pub memory_writes_total: Counter,
    /// Total memory read operations.
    pub memory_reads_total: Counter,
    /// Total cache hits.
    pub cache_hits_total: Counter,
    /// Total cache misses.
    pub cache_misses_total: Counter,
    /// Histogram of retrieval durations in seconds.
    pub retrieval_duration_seconds: Histogram,
    /// Current number of active agents.
    pub active_agents: Gauge<i64, std::sync::atomic::AtomicI64>,
    /// Current number of stored memories.
    pub memory_count: Gauge<i64, std::sync::atomic::AtomicI64>,
}

static METRICS: OnceLock<UmmsMetrics> = OnceLock::new();

impl UmmsMetrics {
    /// Register all UMMS metrics on the provided `registry` and return a new
    /// [`UmmsMetrics`] instance.
    #[must_use]
    pub fn new(registry: &mut Registry) -> Self {
        let memory_writes_total = Counter::default();
        registry.register(
            "umms_memory_writes_total",
            "Total memory write operations",
            memory_writes_total.clone(),
        );

        let memory_reads_total = Counter::default();
        registry.register(
            "umms_memory_reads_total",
            "Total memory read operations",
            memory_reads_total.clone(),
        );

        let cache_hits_total = Counter::default();
        registry.register(
            "umms_cache_hits_total",
            "Total cache hits",
            cache_hits_total.clone(),
        );

        let cache_misses_total = Counter::default();
        registry.register(
            "umms_cache_misses_total",
            "Total cache misses",
            cache_misses_total.clone(),
        );

        let retrieval_duration_seconds =
            Histogram::new(exponential_buckets(0.001, 2.0, 15));
        registry.register(
            "umms_retrieval_duration_seconds",
            "Histogram of retrieval durations in seconds",
            retrieval_duration_seconds.clone(),
        );

        let active_agents = Gauge::<i64, std::sync::atomic::AtomicI64>::default();
        registry.register(
            "umms_active_agents",
            "Current number of active agents",
            active_agents.clone(),
        );

        let memory_count = Gauge::<i64, std::sync::atomic::AtomicI64>::default();
        registry.register(
            "umms_memory_count",
            "Current number of stored memories",
            memory_count.clone(),
        );

        Self {
            memory_writes_total,
            memory_reads_total,
            cache_hits_total,
            cache_misses_total,
            retrieval_duration_seconds,
            active_agents,
            memory_count,
        }
    }
}

/// Return a reference to the global [`UmmsMetrics`] instance.
///
/// # Panics
///
/// Panics if [`init_metrics`] has not been called yet.
#[must_use]
pub fn metrics() -> &'static UmmsMetrics {
    METRICS
        .get()
        .expect("metrics not initialised — call init_metrics() first")
}

/// Create a new [`Registry`], register all UMMS metrics on it, and store
/// the metrics in the global [`OnceLock`].  Returns the registry so it can
/// be kept for later encoding.
///
/// Safe to call multiple times; only the first call initialises the global
/// metrics.
#[must_use]
pub fn init_metrics() -> Registry {
    let mut registry = Registry::default();
    let m = UmmsMetrics::new(&mut registry);
    // If another thread raced us, the metrics from `m` are simply dropped.
    let _ = METRICS.set(m);
    registry
}

/// Encode the contents of `registry` into the OpenMetrics text exposition
/// format.
pub fn encode_metrics(registry: &Registry) -> String {
    let mut buf = String::new();
    encode(&mut buf, registry).expect("encoding metrics to String should not fail");
    buf
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

/// Generate exponential histogram bucket boundaries.
fn exponential_buckets(start: f64, factor: f64, count: usize) -> impl Iterator<Item = f64> {
    (0..count).map(move |i| start * factor.powi(i as i32))
}

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_and_encode() {
        let registry = init_metrics();
        let m = metrics();

        m.memory_writes_total.inc();
        m.memory_writes_total.inc();
        m.memory_reads_total.inc();
        m.cache_hits_total.inc();
        m.cache_misses_total.inc();
        m.retrieval_duration_seconds.observe(0.042);
        m.active_agents.set(3);
        m.memory_count.set(100);

        let output = encode_metrics(&registry);
        assert!(output.contains("umms_memory_writes_total"));
        assert!(output.contains("umms_memory_reads_total"));
        assert!(output.contains("umms_cache_hits_total"));
        assert!(output.contains("umms_cache_misses_total"));
        assert!(output.contains("umms_retrieval_duration_seconds"));
        assert!(output.contains("umms_active_agents"));
        assert!(output.contains("umms_memory_count"));
    }

    #[test]
    fn exponential_buckets_produces_expected_values() {
        let buckets: Vec<f64> = exponential_buckets(1.0, 2.0, 4).collect();
        assert_eq!(buckets.len(), 4);
        assert!((buckets[0] - 1.0).abs() < f64::EPSILON);
        assert!((buckets[1] - 2.0).abs() < f64::EPSILON);
        assert!((buckets[2] - 4.0).abs() < f64::EPSILON);
        assert!((buckets[3] - 8.0).abs() < f64::EPSILON);
    }
}
