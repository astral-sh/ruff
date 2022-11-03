use anyhow::Result;
use fern;

#[macro_export]
macro_rules! tell_user {
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

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub enum LogLevel {
    // No output (+ `log::LevelFilter::Off`).
    Silent,
    // Only show lint violations, with no decorative output (+ `log::LevelFilter::Off`).
    Quiet,
    // All user-facing output (+ `log::LevelFilter::Info`).
    Default,
    // All user-facing output (+ `log::LevelFilter::Debug`).
    Verbose,
}

impl LogLevel {
    fn level_filter(&self) -> log::LevelFilter {
        match self {
            LogLevel::Default => log::LevelFilter::Info,
            LogLevel::Verbose => log::LevelFilter::Debug,
            LogLevel::Quiet => log::LevelFilter::Off,
            LogLevel::Silent => log::LevelFilter::Off,
        }
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Default
    }
}

pub fn set_up_logging(level: &LogLevel) -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(level.level_filter())
        .chain(std::io::stdout())
        .apply()
        .map_err(|e| e.into())
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
