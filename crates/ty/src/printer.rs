use std::io::StdoutLock;

use anyhow::Result;
use indicatif::ProgressDrawTarget;

use crate::logging::VerbosityLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Printer {
    verbosity: VerbosityLevel,
    no_progress: bool,
}

impl Printer {
    pub(crate) fn new(verbosity: VerbosityLevel, no_progress: bool) -> Self {
        Self {
            verbosity,
            no_progress,
        }
    }

    /// Return the [`ProgressDrawTarget`] for this printer.
    pub(crate) fn progress_target(self) -> ProgressDrawTarget {
        if self.no_progress {
            return ProgressDrawTarget::hidden();
        }

        match self.verbosity {
            VerbosityLevel::Silent => ProgressDrawTarget::hidden(),
            VerbosityLevel::Quiet => ProgressDrawTarget::hidden(),
            VerbosityLevel::Default => ProgressDrawTarget::stderr(),
            // Hide the progress bar when in verbose mode.
            // Otherwise, it gets interleaved with log messages.
            VerbosityLevel::Verbose => ProgressDrawTarget::hidden(),
            VerbosityLevel::ExtraVerbose => ProgressDrawTarget::hidden(),
            VerbosityLevel::Trace => ProgressDrawTarget::hidden(),
        }
    }

    /// Return the [`Stdout`] stream for important messages.
    ///
    /// Unlike [`Self::stdout_general`], the returned stream will be enabled when
    /// [`VerbosityLevel::Quiet`] is used.
    fn stdout_important(self) -> Stdout {
        match self.verbosity {
            VerbosityLevel::Silent => Stdout::disabled(),
            VerbosityLevel::Quiet => Stdout::enabled(),
            VerbosityLevel::Default => Stdout::enabled(),
            VerbosityLevel::Verbose => Stdout::enabled(),
            VerbosityLevel::ExtraVerbose => Stdout::enabled(),
            VerbosityLevel::Trace => Stdout::enabled(),
        }
    }

    /// Return the [`Stdout`] stream for general messages.
    ///
    /// The returned stream will be disabled when [`VerbosityLevel::Quiet`] is used.
    fn stdout_general(self) -> Stdout {
        match self.verbosity {
            VerbosityLevel::Silent => Stdout::disabled(),
            VerbosityLevel::Quiet => Stdout::disabled(),
            VerbosityLevel::Default => Stdout::enabled(),
            VerbosityLevel::Verbose => Stdout::enabled(),
            VerbosityLevel::ExtraVerbose => Stdout::enabled(),
            VerbosityLevel::Trace => Stdout::enabled(),
        }
    }

    /// Return the [`Stdout`] stream for a summary message that was explicitly requested by the
    /// user.
    ///
    /// For example, in `ty version` the user has requested the version information and we should
    /// display it even if [`VerbosityLevel::Quiet`] is used. Or, in `ty check`, if the
    /// `TY_MEMORY_REPORT` variable has been set, we should display the memory report because the
    /// user has opted-in to display.
    pub(crate) fn stream_for_requested_summary(self) -> Stdout {
        self.stdout_important()
    }

    /// Return the [`Stdout`] stream for a summary message on failure.
    ///
    /// For example, in `ty check`, this would be used for the message indicating the number of
    /// diagnostics found. The failure summary should capture information that is not reflected in
    /// the exit code.
    pub(crate) fn stream_for_failure_summary(self) -> Stdout {
        self.stdout_important()
    }

    /// Return the [`Stdout`] stream for a summary message on success.
    ///
    /// For example, in `ty check`, this would be used for the message indicating that no diagnostic
    /// were found. The success summary does not capture important information for users that have
    /// opted-in to [`VerbosityLevel::Quiet`].
    pub(crate) fn stream_for_success_summary(self) -> Stdout {
        self.stdout_general()
    }

    /// Return the [`Stdout`] stream for detailed messages.
    ///
    /// For example, in `ty check`, this would be used for the diagnostic output.
    pub(crate) fn stream_for_details(self) -> Stdout {
        self.stdout_general()
    }

    pub(crate) fn clear_screen() -> Result<()> {
        clearscreen::clear()?;
        Ok(())
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

    pub(crate) fn lock(mut self) -> Self {
        match self.status {
            StreamStatus::Enabled => {
                // Drop the previous lock first, to avoid deadlocking
                self.lock.take();
                self.lock = Some(std::io::stdout().lock());
            }
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

    pub(crate) fn is_enabled(&self) -> bool {
        matches!(self.status, StreamStatus::Enabled)
    }
}

impl std::fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self.status {
            StreamStatus::Enabled => {
                let _ = write!(self.handle(), "{s}");
                Ok(())
            }
            StreamStatus::Disabled => Ok(()),
        }
    }
}
