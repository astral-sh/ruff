use std::io::StdoutLock;

use indicatif::ProgressDrawTarget;

use crate::logging::VerbosityLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Printer {
    verbosity: VerbosityLevel,
    no_progress: bool,
}

impl Printer {
    #[must_use]
    pub(crate) fn with_no_progress(self) -> Self {
        Self {
            verbosity: self.verbosity,
            no_progress: true,
        }
    }

    #[must_use]
    pub(crate) fn with_verbosity(self, verbosity: VerbosityLevel) -> Self {
        Self {
            verbosity,
            no_progress: self.no_progress,
        }
    }

    /// Return the [`ProgressDrawTarget`] for this printer.
    pub(crate) fn progress_target(self) -> ProgressDrawTarget {
        if self.no_progress {
            return ProgressDrawTarget::hidden();
        }

        match self.verbosity {
            VerbosityLevel::Quiet => ProgressDrawTarget::hidden(),
            VerbosityLevel::Default => ProgressDrawTarget::stderr(),
            // Hide the progress bar when in verbose mode.
            // Otherwise, it gets interleaved with log messages.
            VerbosityLevel::Verbose => ProgressDrawTarget::hidden(),
            VerbosityLevel::ExtraVerbose => ProgressDrawTarget::hidden(),
            VerbosityLevel::Trace => ProgressDrawTarget::hidden(),
        }
    }

    /// Return the [`Stdout`] stream for an important message.
    pub(crate) fn stdout_important(self) -> Stdout {
        match self.verbosity {
            VerbosityLevel::Quiet => Stdout::enabled(),
            VerbosityLevel::Default => Stdout::enabled(),
            VerbosityLevel::Verbose => Stdout::enabled(),
            VerbosityLevel::ExtraVerbose => Stdout::enabled(),
            VerbosityLevel::Trace => Stdout::enabled(),
        }
    }

    /// Return the [`Stdout`] stream for general messages.
    pub(crate) fn stdout(self) -> Stdout {
        match self.verbosity {
            VerbosityLevel::Quiet => Stdout::disabled(),
            VerbosityLevel::Default => Stdout::enabled(),
            VerbosityLevel::Verbose => Stdout::enabled(),
            VerbosityLevel::ExtraVerbose => Stdout::enabled(),
            VerbosityLevel::Trace => Stdout::enabled(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamStatus {
    Enabled,
    Disabled,
}

#[derive(Debug)]
pub(crate) struct Stdout {
    status: StreamStatus,
    lock: Option<StdoutLock<'static>>,
}

impl Stdout {
    pub(crate) fn lock(mut self) -> Self {
        match self.status {
            StreamStatus::Enabled => self.lock = Some(std::io::stdout().lock()),
            StreamStatus::Disabled => self.lock = None,
        }
        self
    }

    fn handle(&mut self) -> Box<dyn std::io::Write + '_> {
        match self.lock.as_mut() {
            Some(lock) => Box::new(lock),
            None => Box::new(std::io::stdout()),
        }
    }

    fn enabled() -> Self {
        Self {
            status: StreamStatus::Enabled,
            lock: None,
        }
    }

    fn disabled() -> Self {
        Self {
            status: StreamStatus::Disabled,
            lock: None,
        }
    }
}

impl std::fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self.status {
            StreamStatus::Enabled => {
                use std::io::Write;

                let _ = write!(self.handle(), "{s}");
            }
            StreamStatus::Disabled => {}
        }

        Ok(())
    }
}
