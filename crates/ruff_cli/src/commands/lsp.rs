use anyhow::Result;
use tracing::metadata::Level;
use tracing::subscriber::Interest;
use tracing::Metadata;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::{Layer, Registry};
use tracing_tree::time::Uptime;

use ruff_linter::logging::LogLevel;

use crate::args::LspCommand;
use crate::ExitStatus;

/// Format a set of files, and return the exit status.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn lsp(_arguments: LspCommand, log_level: LogLevel) -> Result<ExitStatus> {
    let ruff_level = if log_level == LogLevel::Verbose {
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
            .with_filter(LoggingFilter { ruff_level }),
    );

    tracing::subscriber::set_global_default(subscriber)?;

    ruff_lsp::stdio();

    Ok(ExitStatus::Success)
}

struct LoggingFilter {
    ruff_level: Level,
}

impl LoggingFilter {
    fn is_enabled(&self, meta: &Metadata<'_>) -> bool {
        let filter = if meta.target().starts_with("ruff") {
            self.ruff_level
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
        Some(LevelFilter::from_level(self.ruff_level))
    }
}
