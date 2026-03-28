//! Benchmark results handler — reads criterion output from the target directory.

use std::sync::Arc;

use axum::extract::State;
use axum::response::Json;

use crate::response::*;
use crate::state::AppState;

/// `GET /api/benchmarks` — read criterion benchmark results.
pub async fn benchmarks(State(state): State<Arc<AppState>>) -> Json<BenchmarksResponse> {
    let criterion_dir = state
        .config
        .data_dir
        .parent() // ~/.umms/dev → ~/.umms
        .and_then(|p| p.parent()) // ~/.umms → ~/
        .unwrap_or_else(|| std::path::Path::new("."));

    // Criterion stores results in target/criterion/{bench_name}/new/estimates.json
    // Try to find the workspace root's target directory.
    let target_dir = find_criterion_dir(criterion_dir);

    let benchmarks = match target_dir {
        Some(dir) => read_criterion_results(&dir),
        None => Vec::new(),
    };

    Json(BenchmarksResponse { benchmarks })
}

/// Walk upward from a starting path to find `target/criterion/`.
fn find_criterion_dir(start: &std::path::Path) -> Option<std::path::PathBuf> {
    // Try the project root's target directory directly
    let candidates = [
        start.join("target").join("criterion"),
        // Also check the UMMS project dir if data_dir is nested
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("target").join("criterion"))
            .unwrap_or_default(),
    ];

    candidates.into_iter().find(|p| p.is_dir())
}

/// Read benchmark results from criterion's output format.
fn read_criterion_results(criterion_dir: &std::path::Path) -> Vec<BenchmarkEntry> {
    let mut results = Vec::new();

    let Ok(entries) = std::fs::read_dir(criterion_dir) else {
        return results;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let estimates_path = path.join("new").join("estimates.json");
        if !estimates_path.exists() {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(&estimates_path) else {
            continue;
        };

        let Ok(json): Result<serde_json::Value, _> = serde_json::from_str(&content) else {
            continue;
        };

        let name = entry.file_name().to_str().unwrap_or("unknown").to_owned();

        let mean_ns = json
            .pointer("/mean/point_estimate")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let median_ns = json
            .pointer("/median/point_estimate")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let std_dev_ns = json
            .pointer("/std_dev/point_estimate")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);

        results.push(BenchmarkEntry {
            name,
            mean_ns,
            median_ns,
            std_dev_ns,
        });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}
