#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum VerbosityLevel {
    Info,
    Debug,
    Trace,
}

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
    pub(crate) fn level(&self) -> Option<VerbosityLevel> {
        match self.verbose {
            0 => None,
            1 => Some(VerbosityLevel::Info),
            2 => Some(VerbosityLevel::Debug),
            _ => Some(VerbosityLevel::Trace),
        }
    }
}
