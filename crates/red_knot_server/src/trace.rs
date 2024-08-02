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
use core::str;
use lsp_server::{Message, Notification};
use lsp_types::{
    notification::{LogTrace, Notification as _},
    ClientInfo, TraceValue,
};
use serde::Deserialize;
use std::{
    io::{Error as IoError, ErrorKind, Write},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex, OnceLock},
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    fmt::{time::Uptime, writer::BoxMakeWriter, MakeWriter},
    layer::SubscriberExt,
    Layer,
};

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

// A tracing writer that uses LSPs logTrace method.
struct TraceLogWriter;

impl Write for TraceLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let message = str::from_utf8(buf).map_err(|e| IoError::new(ErrorKind::InvalidData, e))?;
        LOGGING_SENDER
            .get()
            .expect("logging sender should be initialized at this point")
            .send(Message::Notification(Notification {
                method: LogTrace::METHOD.to_owned(),
                params: serde_json::json!({
                    "message": message
                }),
            }))
            .map_err(|e| IoError::new(ErrorKind::Other, e))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for TraceLogWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        Self
    }
}

pub(crate) fn init_tracing(
    sender: ClientSender,
    log_level: LogLevel,
    log_file: Option<&std::path::Path>,
    client: Option<&ClientInfo>,
) {
    LOGGING_SENDER
        .set(sender)
        .expect("logging sender should only be initialized once");

    let log_file = log_file
        .map(|path| {
            // this expands `logFile` so that tildes and environment variables
            // are replaced with their values, if possible.
            if let Some(expanded) = shellexpand::full(&path.to_string_lossy())
                .ok()
                .and_then(|path| PathBuf::from_str(&path).ok())
            {
                expanded
            } else {
                path.to_path_buf()
            }
        })
        .and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|err| {
                    #[allow(clippy::print_stderr)]
                    {
                        eprintln!(
                            "Failed to open file at {} for logging: {err}",
                            path.display()
                        );
                    }
                })
                .ok()
        });

    let logger = match log_file {
        Some(file) => BoxMakeWriter::new(Arc::new(file)),
        None => {
            if client.is_some_and(|client| {
                client.name.starts_with("Zed") || client.name.starts_with("Visual Studio Code")
            }) {
                BoxMakeWriter::new(TraceLogWriter)
            } else {
                BoxMakeWriter::new(std::io::stderr)
            }
        }
    };
    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::layer()
            .with_timer(Uptime::default())
            .with_thread_names(true)
            .with_ansi(false)
            .with_writer(logger)
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

/// Filters out traces which have a log level lower than the `logLevel` set by the client.
struct LogLevelFilter {
    filter: LogLevel,
}

/// Filters out traces if the trace value set by the client is `off`.
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
    std::env::var(TRACE_ENV_KEY)
        .ok()
        .and_then(|trace| serde_json::from_value(serde_json::Value::String(trace)).ok())
        .unwrap_or_else(|| {
            *TRACE_VALUE
                .lock()
                .expect("trace value mutex should be available")
        })
}
