//! The tracing system for `ruff server`.
//! 
//! Traces are controlled by the `logLevel` setting, along with the
//! trace level set through the LSP. On VS Code, the trace level can
//! also be set with `ruff.trace.server`. A trace level of `messages` or
//! `verbose` will enable tracing - otherwise, no traces will be shown.
//! 
//! `logLevel` can be used to configure the level of tracing that is shown.
//! By default, `logLevel` is set to `"info"`.
//! 
//! The server also supports the `RUFF_TRACE` environment variable, which will
//! override the trace value provided by the LSP client. Use this if there's no good way
//! to set the trace value through your editor's configuration.
//! 
//! Tracing will write to `stderr` by default, which should appear in the logs for most LSP clients.
//! A `logFile` path can also be specified in the settings, and output will be directed there instead.
use lsp_types::TraceValue;
use serde::Deserialize;
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt::time::Uptime, layer::SubscriberExt, Layer};

use crate::server::ClientSender;

const TRACE_ENV_KEY: &str = "RUFF_TRACE";

static LOGGING_SENDER: OnceLock<ClientSender> = OnceLock::new();

static TRACE_VALUE: Mutex<lsp_types::TraceValue> = Mutex::new(lsp_types::TraceValue::Off);

pub(crate) fn set_trace_value(trace_value: TraceValue) {
    let mut global_trace_value = TRACE_VALUE
        .lock()
        .expect("trace value mutex should be available");
    *global_trace_value = trace_value;
}

pub(crate) fn init_tracing(
    sender: ClientSender,
    log_level: LogLevel,
    log_file: Option<&std::path::Path>,
) {
    LOGGING_SENDER
        .set(sender)
        .expect("logging sender should only be initialized once");

    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::layer()
            .with_timer(Uptime::default())
            .with_thread_names(true)
            .with_ansi(false)
            .with_writer(TracingWriter::new(log_file))
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

struct TracingWriter(Option<PathBuf>);

impl TracingWriter {
    fn new(file: Option<&std::path::Path>) -> TracingWriter {
        Self(file.map(std::path::Path::to_path_buf))
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for TracingWriter {
    type Writer = Box<dyn std::io::Write>;

    fn make_writer(&self) -> Self::Writer {
        if let Some(file) = self.0.as_ref().and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        }) {
            Box::new(file)
        } else {
            Box::new(std::io::stderr())
        }
    }
}

#[inline]
fn trace_value() -> lsp_types::TraceValue {
    std::env::var(TRACE_ENV_KEY)
        .ok()
        .and_then(|trace| serde_json::from_value(serde_json::Value::String(trace)).ok())
        .unwrap_or_else(|| {
            *TRACE_VALUE
                .lock()
                .expect("trace value mutex should be available")
        })
}
