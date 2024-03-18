use std::num::NonZeroUsize;

use crate::ExitStatus;
use anyhow::Result;
use ruff_linter::logging::LogLevel;
use ruff_server::Server;
use tracing::{level_filters::LevelFilter, metadata::Level, subscriber::Interest, Metadata};
use tracing_subscriber::{
    layer::{Context, Filter, SubscriberExt},
    Layer, Registry,
};
use tracing_tree::time::Uptime;

pub(crate) fn run_server(
    preview: bool,
    worker_threads: NonZeroUsize,
    log_level: LogLevel,
) -> Result<ExitStatus> {
    if !preview {
        tracing::error!("--preview needs to be provided as a command line argument while the server is still unstable.\nFor example: `ruff server --preview`");
        return Ok(ExitStatus::Error);
    }
    let trace_level = if log_level == LogLevel::Verbose {
        Level::TRACE
    } else {
        Level::DEBUG
    };

    let subscriber = Registry::default().with(
        tracing_tree::HierarchicalLayer::default()
            .with_indent_lines(true)
            .with_indent_amount(2)
            .with_bracketed_fields(true)
            .with_targets(true)
            .with_writer(|| Box::new(std::io::stderr()))
            .with_timer(Uptime::default())
            .with_filter(LoggingFilter { trace_level }),
    );

    tracing::subscriber::set_global_default(subscriber)?;

    let server = Server::new(worker_threads)?;

    server.run().map(|()| ExitStatus::Success)
}

struct LoggingFilter {
    trace_level: Level,
}

impl LoggingFilter {
    fn is_enabled(&self, meta: &Metadata<'_>) -> bool {
        let filter = if meta.target().starts_with("ruff") {
            self.trace_level
        } else {
            Level::INFO
        };

        meta.level() <= &filter
    }
}

impl<S> Filter<S> for LoggingFilter {
    fn enabled(&self, meta: &Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        self.is_enabled(meta)
    }

    fn callsite_enabled(&self, meta: &'static Metadata<'static>) -> Interest {
        if self.is_enabled(meta) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::from_level(self.trace_level))
    }
}
