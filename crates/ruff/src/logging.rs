use std::fmt::{Display, Formatter, Write};
use std::path::Path;
use std::sync::Mutex;

use crate::fs;
use anyhow::Result;
use colored::Colorize;
use fern;
use log::Level;
use once_cell::sync::Lazy;
use ruff_python_ast::source_code::SourceCode;
use rustpython_parser::{ParseError, ParseErrorType};

pub(crate) static WARNINGS: Lazy<Mutex<Vec<&'static str>>> = Lazy::new(Mutex::default);

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
    const fn level_filter(&self) -> log::LevelFilter {
        match self {
            LogLevel::Default => log::LevelFilter::Info,
            LogLevel::Verbose => log::LevelFilter::Debug,
            LogLevel::Quiet => log::LevelFilter::Off,
            LogLevel::Silent => log::LevelFilter::Off,
        }
    }
}

pub fn set_up_logging(level: &LogLevel) -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| match record.level() {
            Level::Error => {
                out.finish(format_args!(
                    "{}{} {}",
                    "error".red().bold(),
                    ":".bold(),
                    message
                ));
            }
            Level::Warn => {
                out.finish(format_args!(
                    "{}{} {}",
                    "warning".yellow().bold(),
                    ":".bold(),
                    message
                ));
            }
            Level::Info | Level::Debug | Level::Trace => {
                out.finish(format_args!(
                    "{}[{}][{}] {}",
                    chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                    record.target(),
                    record.level(),
                    message
                ));
            }
        })
        .level(level.level_filter())
        .level_for("globset", log::LevelFilter::Warn)
        .chain(std::io::stderr())
        .apply()?;
    Ok(())
}

pub struct DisplayParseError<'a> {
    error: ParseError,
    source_code: SourceCode<'a, 'a>,
}

impl<'a> DisplayParseError<'a> {
    pub fn new(error: ParseError, source_code: SourceCode<'a, 'a>) -> Self {
        Self { error, source_code }
    }
}

impl Display for DisplayParseError<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let source_location = self.source_code.source_location(self.error.location);

        write!(
            f,
            "{header} {path}{colon}{row}{colon}{column}{colon} {inner}",
            header = "Failed to parse".bold(),
            path = fs::relativize_path(Path::new(&self.error.source_path)).bold(),
            row = source_location.row,
            column = source_location.column,
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
                        "expected '{expected}', but got {tok}",
                        tok = TruncateAtNewline(&tok)
                    )
                } else {
                    write!(f, "unexpected token {tok}", tok = TruncateAtNewline(&tok))
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
            inner: &'a mut dyn std::fmt::Write,
            after_new_line: bool,
        }

        impl std::fmt::Write for TruncateAdapter<'_> {
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
