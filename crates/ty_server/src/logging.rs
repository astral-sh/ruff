//! The logging system for `ty server`.
//!
//! Log messages are controlled by the `logLevel` setting which defaults to `"info"`. Log messages
//! are written to `stderr` by default, which should appear in the logs for most LSP clients. A
//! `logFile` path can also be specified in the settings, and output will be directed there
//! instead.
use std::sync::Arc;

use ruff_db::system::{SystemPath, SystemPathBuf};
use serde::Deserialize;
use tracing::Metadata;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::Interest;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;

pub fn init_logging(log_level: LogLevel, log_file: Option<&SystemPath>) {
    let log_file = log_file
        .map(|path| {
            // this expands `logFile` so that tildes and environment variables
            // are replaced with their values, if possible.
            if let Some(expanded) = shellexpand::full(&path.to_string())
                .ok()
                .map(|path| SystemPathBuf::from(&*path))
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
                .open(path.as_std_path())
                .map_err(|err| {
                    #[expect(clippy::print_stderr)]
                    {
                        eprintln!("Failed to open file at {path} for logging: {err}");
                    }
                })
                .ok()
        });

    let logger = match log_file {
        Some(file) => BoxMakeWriter::new(Arc::new(file)),
        None => BoxMakeWriter::new(std::io::stderr),
    };
    let is_trace_level = log_level == LogLevel::Trace;
    let subscriber = tracing_subscriber::Registry::default().with(
        tracing_subscriber::fmt::layer()
            .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S.%f".to_string()))
            .with_thread_names(is_trace_level)
            .with_target(is_trace_level)
            .with_ansi(false)
            .with_writer(logger)
            .with_filter(LogLevelFilter { filter: log_level }),
    );

    tracing::subscriber::set_global_default(subscriber)
        .expect("should be able to set global default subscriber");
}

/// The log level for the server as provided by the client during initialization.
///
/// The default log level is `info`.
#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
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

impl LogLevelFilter {
    fn is_enabled(&self, meta: &Metadata<'_>) -> bool {
        let filter = if meta.target().starts_with("ty")
            || meta.target().starts_with("ruff")
            || meta.target().starts_with("e2e")
        {
            self.filter.trace_level()
        } else {
            tracing::Level::WARN
        };

        meta.level() <= &filter
    }
}

impl<S> tracing_subscriber::layer::Filter<S> for LogLevelFilter {
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        _: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        self.is_enabled(meta)
    }

    fn callsite_enabled(&self, meta: &'static Metadata<'static>) -> Interest {
        // The result of `self.enabled(metadata, ...)` will always be
        // the same for any given `Metadata`, so we can convert it into
        // an `Interest`:
        if self.is_enabled(meta) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::from_level(self.filter.trace_level()))
    }
}
