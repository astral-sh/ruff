use lsp_types::TraceValue;
use serde::Deserialize;
use std::sync::{Mutex, OnceLock};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt::time::Uptime, layer::SubscriberExt, Layer};

use crate::server::ClientSender;

static LOGGING_SENDER: OnceLock<ClientSender> = OnceLock::new();

static TRACE_VALUE: Mutex<lsp_types::TraceValue> = Mutex::new(lsp_types::TraceValue::Off);

pub(crate) fn set_trace_value(trace_value: TraceValue) {
    let mut global_trace_value = TRACE_VALUE
        .lock()
        .expect("trace value mutex should be available");
    *global_trace_value = trace_value;
}

pub(crate) fn init_tracing(sender: ClientSender, log_level: LogLevel) {
    LOGGING_SENDER
        .set(sender)
        .expect("logging sender should only be initialized once");

    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::layer()
            .with_timer(Uptime::default())
            .with_thread_names(true)
            .with_ansi(false)
            .with_writer(|| Box::new(std::io::stderr()))
            .with_filter(TraceLevelFilter)
            .with_filter(LogLevelFilter { filter: log_level }),
    );

    tracing::subscriber::set_global_default(subscriber)
        .expect("should be able to set global default subscriber");
}

#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LogLevel {
    #[default]
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn trace_level(self) -> tracing::Level {
        match self {
            Self::Error => tracing::Level::ERROR,
            Self::Warn => tracing::Level::WARN,
            Self::Info => tracing::Level::INFO,
            Self::Debug => tracing::Level::DEBUG,
            Self::Trace => tracing::Level::TRACE,
        }
    }
}

struct LogLevelFilter {
    filter: LogLevel,
}

struct TraceLevelFilter;

impl<S> tracing_subscriber::layer::Filter<S> for LogLevelFilter {
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        _: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        let filter = if meta.target().starts_with("ruff") {
            self.filter.trace_level()
        } else {
            tracing::Level::INFO
        };

        meta.level() <= &filter
    }

    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(LevelFilter::from_level(self.filter.trace_level()))
    }
}

impl<S> tracing_subscriber::layer::Filter<S> for TraceLevelFilter {
    fn enabled(
        &self,
        _: &tracing::Metadata<'_>,
        _: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        trace_value() != lsp_types::TraceValue::Off
    }
}

#[inline]
fn trace_value() -> lsp_types::TraceValue {
    std::env::var("RUFF_TRACE")
        .ok()
        .and_then(|trace| serde_json::from_value(serde_json::Value::String(trace)).ok())
        .unwrap_or_else(|| {
            *TRACE_VALUE
                .lock()
                .expect("trace value mutex should be available")
        })
}
