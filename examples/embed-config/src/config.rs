use purple_garden::{GardenValue, pg_pkg};

#[derive(Debug, GardenValue)]
pub struct RetryConfig {
    attempts: i64,
    backoff_ms: i64,
}

#[derive(Debug, GardenValue)]
pub struct AppConfig {
    service: String,
    workers: i64,
    debug: bool,
    retry: RetryConfig,
}

/// Configuration helpers backed by Rust.
#[pg_pkg]
pub mod config {
    use super::AppConfig;

    /// Render a compact deployment summary from a Garden record.
    pub fn summary(config: AppConfig) -> String {
        let mode = if config.debug { "debug" } else { "release" };
        format!(
            "{}: workers={} mode={} retry={}x/{}ms",
            config.service, config.workers, mode, config.retry.attempts, config.retry.backoff_ms,
        )
    }
}
