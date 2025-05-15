//! Sets up logging for ty

use crate::args::TerminalColor;
use anyhow::Context;
use colored::Colorize;
use std::fmt;
use std::fs::File;
use std::io::{BufWriter, IsTerminal};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

/// Logging flags to `#[command(flatten)]` into your CLI
#[derive(clap::Args, Debug, Clone, Default)]
#[command(about = None, long_about = None)]
pub(crate) struct Verbosity {
    #[arg(
        long,
        short = 'v',
        help = "Use verbose output (or `-vv` and `-vvv` for more verbose output)",
        action = clap::ArgAction::Count,
        global = true,
    )]
    verbose: u8,
}

impl Verbosity {
    /// Returns the verbosity level based on the number of `-v` flags.
    ///
    /// Returns `None` if the user did not specify any verbosity flags.
    pub(crate) fn level(&self) -> VerbosityLevel {
        match self.verbose {
            0 => VerbosityLevel::Default,
            1 => VerbosityLevel::Verbose,
            2 => VerbosityLevel::ExtraVerbose,
            _ => VerbosityLevel::Trace,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum VerbosityLevel {
    /// Default output level. Only shows Ruff and ty events up to the [`WARN`](tracing::Level::WARN).
    Default,

    /// Enables verbose output. Emits Ruff and ty events up to the [`INFO`](tracing::Level::INFO).
    /// Corresponds to `-v`.
    Verbose,

    /// Enables a more verbose tracing format and emits Ruff and ty events up to [`DEBUG`](tracing::Level::DEBUG).
    /// Corresponds to `-vv`
    ExtraVerbose,

    /// Enables all tracing events and uses a tree-like output format. Corresponds to `-vvv`.
    Trace,
}

impl VerbosityLevel {
    const fn level_filter(self) -> LevelFilter {
        match self {
            VerbosityLevel::Default => LevelFilter::WARN,
            VerbosityLevel::Verbose => LevelFilter::INFO,
            VerbosityLevel::ExtraVerbose => LevelFilter::DEBUG,
            VerbosityLevel::Trace => LevelFilter::TRACE,
        }
    }

    pub(crate) const fn is_trace(self) -> bool {
        matches!(self, VerbosityLevel::Trace)
    }

    pub(crate) const fn is_extra_verbose(self) -> bool {
        matches!(self, VerbosityLevel::ExtraVerbose)
    }
}

pub(crate) fn setup_tracing(
    level: VerbosityLevel,
    color: TerminalColor,
) -> anyhow::Result<TracingGuard> {
    use tracing_subscriber::prelude::*;

    // The `TY_LOG` environment variable overrides the default log level.
    let filter = if let Ok(log_env_variable) = std::env::var("TY_LOG") {
        EnvFilter::builder()
            .parse(log_env_variable)
            .context("Failed to parse directives specified in TY_LOG environment variable.")?
    } else {
        match level {
            VerbosityLevel::Default => {
                // Show warning traces
                EnvFilter::default().add_directive(LevelFilter::WARN.into())
            }
            level => {
                let level_filter = level.level_filter();

                // Show info|debug|trace events, but allow `TY_LOG` to override
                let filter = EnvFilter::default().add_directive(
                    format!("ty={level_filter}")
                        .parse()
                        .expect("Hardcoded directive to be valid"),
                );

                filter.add_directive(
                    format!("ruff={level_filter}")
                        .parse()
                        .expect("Hardcoded directive to be valid"),
                )
            }
        }
    };

    let (profiling_layer, guard) = setup_profile();

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(profiling_layer);

    let ansi = match color {
        TerminalColor::Auto => {
            colored::control::SHOULD_COLORIZE.should_colorize() && std::io::stderr().is_terminal()
        }
        TerminalColor::Always => true,
        TerminalColor::Never => false,
    };

    if level.is_trace() {
        let subscriber = registry.with(
            tracing_subscriber::fmt::layer()
                .event_format(tracing_subscriber::fmt::format().pretty())
                .with_thread_ids(true)
                .with_ansi(ansi)
                .with_writer(std::io::stderr),
        );

        subscriber.init();
    } else {
        let subscriber = registry.with(
            tracing_subscriber::fmt::layer()
                .event_format(TyFormat {
                    display_level: true,
                    display_timestamp: level.is_extra_verbose(),
                    show_spans: false,
                })
                .with_ansi(ansi)
                .with_writer(std::io::stderr),
        );

        subscriber.init();
    }

    Ok(TracingGuard {
        _flame_guard: guard,
    })
}

#[expect(clippy::type_complexity)]
fn setup_profile<S>() -> (
    Option<tracing_flame::FlameLayer<S, BufWriter<File>>>,
    Option<tracing_flame::FlushGuard<BufWriter<File>>>,
)
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    if let Ok("1" | "true") = std::env::var("TY_LOG_PROFILE").as_deref() {
        let (layer, guard) = tracing_flame::FlameLayer::with_file("tracing.folded")
            .expect("Flame layer to be created");
        (Some(layer), Some(guard))
    } else {
        (None, None)
    }
}

pub(crate) struct TracingGuard {
    _flame_guard: Option<tracing_flame::FlushGuard<BufWriter<File>>>,
}

struct TyFormat {
    display_timestamp: bool,
    display_level: bool,
    show_spans: bool,
}

/// See <https://docs.rs/tracing-subscriber/0.3.18/src/tracing_subscriber/fmt/format/mod.rs.html#1026-1156>
impl<S, N> FormatEvent<S, N> for TyFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let ansi = writer.has_ansi_escapes();

        if self.display_timestamp {
            let timestamp = jiff::Zoned::now()
                .strftime("%Y-%m-%d %H:%M:%S.%f")
                .to_string();
            if ansi {
                write!(writer, "{} ", timestamp.dimmed())?;
            } else {
                write!(
                    writer,
                    "{} ",
                    jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S.%f")
                )?;
            }
        }

        if self.display_level {
            let level = meta.level();
            // Same colors as tracing
            if ansi {
                let formatted_level = level.to_string();
                match *level {
                    tracing::Level::TRACE => {
                        write!(writer, "{} ", formatted_level.purple().bold())?;
                    }
                    tracing::Level::DEBUG => write!(writer, "{} ", formatted_level.blue().bold())?,
                    tracing::Level::INFO => write!(writer, "{} ", formatted_level.green().bold())?,
                    tracing::Level::WARN => write!(writer, "{} ", formatted_level.yellow().bold())?,
                    tracing::Level::ERROR => write!(writer, "{} ", level.to_string().red().bold())?,
                }
            } else {
                write!(writer, "{level} ")?;
            }
        }

        if self.show_spans {
            let span = event.parent();
            let mut seen = false;

            let span = span
                .and_then(|id| ctx.span(id))
                .or_else(|| ctx.lookup_current());

            let scope = span.into_iter().flat_map(|span| span.scope().from_root());

            for span in scope {
                seen = true;
                if ansi {
                    write!(writer, "{}:", span.metadata().name().bold())?;
                } else {
                    write!(writer, "{}:", span.metadata().name())?;
                }
            }

            if seen {
                writer.write_char(' ')?;
            }
        }

        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
