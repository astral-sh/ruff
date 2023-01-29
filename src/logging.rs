use anyhow::Result;
use fern;

#[macro_export]
macro_rules! warn_user_once {
    ($($arg:tt)*) => {
        use colored::Colorize;
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let message = format!("{}", format_args!($($arg)*));
            eprintln!(
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                message.bold(),
            );
        }
    };
}

#[macro_export]
macro_rules! warn_user {
    ($($arg:tt)*) => {
        use colored::Colorize;
        let message = format!("{}", format_args!($($arg)*));
        eprintln!(
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            message.bold(),
        );
    };
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
    // No output (+ `log::LevelFilter::Off`).
    Silent,
    // Only show lint violations, with no decorative output (+ `log::LevelFilter::Off`).
    Quiet,
    // All user-facing output (+ `log::LevelFilter::Info`).
    #[default]
    Default,
    // All user-facing output (+ `log::LevelFilter::Debug`).
    Verbose,
}

impl LogLevel {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn level_filter(&self) -> log::LevelFilter {
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
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ));
        })
        .level(level.level_filter())
        .chain(std::io::stderr())
        .apply()?;
    Ok(())
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
