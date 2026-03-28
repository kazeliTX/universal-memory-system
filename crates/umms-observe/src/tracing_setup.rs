//! Tracing initialization for UMMS services.

use std::sync::Once;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

static INIT: Once = Once::new();

/// Initialize the global tracing subscriber.
///
/// - `level`: filter string compatible with [`EnvFilter`] (e.g. `"info"`, `"umms=debug,info"`).
///   Falls back to `"info"` when the string is empty.
/// - `json_format`: when `true` the subscriber emits structured JSON lines;
///   otherwise it uses the human-readable "pretty" formatter.
///
/// Safe to call multiple times; only the first invocation has any effect.
pub fn init_tracing(level: &str, json_format: bool) {
    INIT.call_once(|| {
        let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));

        if json_format {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json().with_writer(std::io::stdout))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().pretty().with_writer(std::io::stdout))
                .init();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_init_does_not_panic() {
        // The global subscriber may already be set by another test, so we
        // only verify that calling init_tracing twice does not panic.
        init_tracing("debug", false);
        init_tracing("trace", true);
    }
}
