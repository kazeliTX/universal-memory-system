use std::sync::Arc;

use tauri::State;

use umms_api::AppState;
use umms_api::response::{
    ModelTraceResponse, ModelTraceStatResponse, TaskTraceStatResponse, TraceListResponse,
    TraceSummaryResponse,
};

#[tauri::command]
pub async fn list_traces(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
    model_id: Option<String>,
    task: Option<String>,
) -> Result<TraceListResponse, String> {
    let limit = limit.unwrap_or(50);

    let traces = match &state.model_pool {
        Some(pool) => {
            let store = &pool.trace_store;
            match (&model_id, &task) {
                (Some(mid), _) => store.traces_by_model(mid, limit),
                (_, Some(t)) => store.traces_by_task(t, limit),
                _ => store.traces(limit),
            }
        }
        None => Vec::new(),
    };

    let total = traces.len();
    let response_traces: Vec<ModelTraceResponse> = traces
        .into_iter()
        .map(|t| ModelTraceResponse {
            id: t.id,
            timestamp: t.timestamp.to_rfc3339(),
            model_id: t.model_id,
            model_name: t.model_name,
            provider: t.provider,
            task: t.task,
            request_type: t.request_type,
            input_preview: t.input_preview,
            input_tokens_estimate: t.input_tokens_estimate,
            success: t.success,
            error_message: t.error_message,
            output_preview: t.output_preview,
            output_dimension: t.output_dimension,
            output_tokens_estimate: t.output_tokens_estimate,
            latency_ms: t.latency_ms,
            retry_count: t.retry_count,
            caller: t.caller,
        })
        .collect();

    Ok(TraceListResponse {
        traces: response_traces,
        total,
    })
}

#[tauri::command]
pub async fn trace_summary(
    state: State<'_, Arc<AppState>>,
) -> Result<TraceSummaryResponse, String> {
    let summary = match &state.model_pool {
        Some(pool) => pool.trace_store.summary(),
        None => umms_model::TraceSummary {
            total_traces: 0,
            total_errors: 0,
            by_model: Vec::new(),
            by_task: Vec::new(),
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
        },
    };

    Ok(TraceSummaryResponse {
        total_traces: summary.total_traces,
        total_errors: summary.total_errors,
        by_model: summary
            .by_model
            .into_iter()
            .map(|m| ModelTraceStatResponse {
                model_id: m.model_id,
                count: m.count,
                errors: m.errors,
                avg_latency_ms: m.avg_latency_ms,
            })
            .collect(),
        by_task: summary
            .by_task
            .into_iter()
            .map(|t| TaskTraceStatResponse {
                task: t.task,
                count: t.count,
                errors: t.errors,
                avg_latency_ms: t.avg_latency_ms,
            })
            .collect(),
        avg_latency_ms: summary.avg_latency_ms,
        p99_latency_ms: summary.p99_latency_ms,
    })
}

#[tauri::command]
pub async fn clear_traces(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    if let Some(pool) = &state.model_pool {
        pool.trace_store.clear();
    }
    Ok(serde_json::json!({ "cleared": true }))
}
