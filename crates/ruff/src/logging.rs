use std::fmt::{Display, Formatter, Write};
use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use colored::Colorize;
use once_cell::sync::Lazy;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use ruff_notebook::Notebook;
use ruff_python_parser::{ParseError, ParseErrorType};
use ruff_source_file::{OneIndexed, SourceCode, SourceLocation};

use crate::fs;
use crate::source_kind::SourceKind;

pub static WARNINGS: Lazy<Mutex<Vec<&'static str>>> = Lazy::new(Mutex::default);

/// Warn a user once, with uniqueness determined by the given ID.
#[macro_export]
macro_rules! warn_user_once_by_id {
    ($id:expr, $($arg:tt)*) => {
        use colored::Colorize;
        use log::warn;

        if let Ok(mut states) = $crate::logging::WARNINGS.lock() {
            if !states.contains(&$id) {
                let message = format!("{}", format_args!($($arg)*));
                warn!("{}", message.bold());
                states.push($id);
            }
        }
    };
}

/// Warn a user once, with uniqueness determined by the calling location itself.
#[macro_export]
macro_rules! warn_user_once {
    ($($arg:tt)*) => {
        use colored::Colorize;
        use log::warn;

        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let message = format!("{}", format_args!($($arg)*));
            warn!("{}", message.bold());
        }
    };
}

#[macro_export]
macro_rules! warn_user {
    ($($arg:tt)*) => {{
        use colored::Colorize;
        use log::warn;

        let message = format!("{}", format_args!($($arg)*));
        warn!("{}", message.bold());
    }};
}

#[macro_export]
macro_rules! notify_user {
    ($($arg:tt)*) => {
        println!(
            "[{}] {}",
            chrono::Local::now()
                .format("%H:%M:%S %p")
                .to_string()
                .dimmed(),
            format_args!($($arg)*)
        )
    }
}

#[derive(Debug, Default, PartialOrd, Ord, PartialEq, Eq, Copy, Clone)]
pub enum LogLevel {
    /// No output ([`log::LevelFilter::Off`]).
    Silent,
    /// Only show lint violations, with no decorative output
    /// ([`log::LevelFilter::Off`]).
    Quiet,
    /// All user-facing output ([`log::LevelFilter::Info`]).
    #[default]
    Default,
    /// All user-facing output ([`log::LevelFilter::Debug`]).
    Verbose,
}

impl LogLevel {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn tracing_level(&self) -> tracing::Level {
        match self {
            LogLevel::Default => tracing::Level::INFO,
            LogLevel::Verbose => tracing::Level::DEBUG,
            LogLevel::Quiet => tracing::Level::WARN,
            LogLevel::Silent => tracing::Level::ERROR,
        }
    }
}

/// Log level priorities: 1. `RUST_LOG=`, 2. explicit CLI log level, 3. default to info
pub fn set_up_logging(level: &LogLevel) -> Result<()> {
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::builder()
            .with_default_directive(level.tracing_level().into())
            .parse_lossy("")
    });
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();
    Ok(())
}

pub struct DisplayParseError<'a> {
    error: ParseError,
    source_code: SourceCode<'a, 'a>,
    source_kind: &'a SourceKind,
}

impl<'a> DisplayParseError<'a> {
    pub fn new(
        error: ParseError,
        source_code: SourceCode<'a, 'a>,
        source_kind: &'a SourceKind,
    ) -> Self {
        Self {
            error,
            source_code,
            source_kind,
        }
    }
}

impl Display for DisplayParseError<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{header} {path}{colon}",
            header = "Failed to parse".bold(),
            path = fs::relativize_path(Path::new(&self.error.source_path)).bold(),
            colon = ":".cyan(),
        )?;

        let source_location = self.source_code.source_location(self.error.offset);

        // If we're working on a Jupyter notebook, translate the positions
        // with respect to the cell and row in the cell. This is the same
        // format as the `TextEmitter`.
        let error_location =
            if let Some(jupyter_index) = self.source_kind.as_ipy_notebook().map(Notebook::index) {
                write!(
                    f,
                    "cell {cell}{colon}",
                    cell = jupyter_index
                        .cell(source_location.row.get())
                        .unwrap_or_default(),
                    colon = ":".cyan(),
                )?;

                SourceLocation {
                    row: OneIndexed::new(
                        jupyter_index
                            .cell_row(source_location.row.get())
                            .unwrap_or(1) as usize,
                    )
                    .unwrap(),
                    column: source_location.column,
                }
            } else {
                source_location
            };

        write!(
            f,
            "{row}{colon}{column}{colon} {inner}",
            row = error_location.row,
            column = error_location.column,
            colon = ":".cyan(),
            inner = &DisplayParseErrorType(&self.error.error)
        )
    }
}

pub(crate) struct DisplayParseErrorType<'a>(&'a ParseErrorType);

impl<'a> DisplayParseErrorType<'a> {
    pub(crate) fn new(error: &'a ParseErrorType) -> Self {
        Self(error)
    }
}

impl Display for DisplayParseErrorType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            ParseErrorType::Eof => write!(f, "Expected token but reached end of file."),
            ParseErrorType::ExtraToken(ref tok) => write!(
                f,
                "Got extraneous token: {tok}",
                tok = TruncateAtNewline(&tok)
            ),
            ParseErrorType::InvalidToken => write!(f, "Got invalid token"),
            ParseErrorType::UnrecognizedToken(ref tok, ref expected) => {
                if let Some(expected) = expected.as_ref() {
                    write!(
                        f,
                        "Expected '{expected}', but got {tok}",
                        tok = TruncateAtNewline(&tok)
                    )
                } else {
                    write!(f, "Unexpected token {tok}", tok = TruncateAtNewline(&tok))
                }
            }
            ParseErrorType::Lexical(ref error) => write!(f, "{error}"),
        }
    }
}

/// Truncates the display text before the first newline character to avoid line breaks.
struct TruncateAtNewline<'a>(&'a dyn Display);

impl Display for TruncateAtNewline<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct TruncateAdapter<'a> {
            inner: &'a mut dyn Write,
            after_new_line: bool,
        }

        impl Write for TruncateAdapter<'_> {
            fn write_str(&mut self, s: &str) -> std::fmt::Result {
                if self.after_new_line {
                    Ok(())
                } else {
                    if let Some(end) = s.find(['\n', '\r']) {
                        self.inner.write_str(&s[..end])?;
                        self.inner.write_str("\u{23ce}...")?;
                        self.after_new_line = true;
                        Ok(())
                    } else {
                        self.inner.write_str(s)
                    }
                }
            }
        }

        write!(
            TruncateAdapter {
                inner: f,
                after_new_line: false,
            },
            "{}",
            self.0
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::logging::LogLevel;

    #[test]
    fn ordering() {
        assert!(LogLevel::Default > LogLevel::Silent);
        assert!(LogLevel::Default >= LogLevel::Default);
        assert!(LogLevel::Quiet > LogLevel::Silent);
        assert!(LogLevel::Verbose > LogLevel::Default);
        assert!(LogLevel::Verbose > LogLevel::Silent);
    }
}
