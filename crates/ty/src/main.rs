mod allocator;

use colored::Colorize;
use std::io;
use ty::{ExitStatus, run};

pub fn main() -> ExitStatus {
    let result = run().unwrap_or_else(|error| {
        use io::Write;

        // Use `writeln` instead of `eprintln` to avoid panicking when the stderr pipe is broken.
        let mut stderr = io::stderr().lock();

        // This communicates that this isn't a linter error but ty itself hard-errored for
        // some reason (e.g. failed to resolve the configuration)
        writeln!(stderr, "{}", "ty failed".red().bold()).ok();
        // Currently we generally only see one error, but e.g. with io errors when resolving
        // the configuration it is help to chain errors ("resolving configuration failed" ->
        // "failed to read file: subdir/pyproject.toml")
        for cause in error.chain() {
            // Exit "gracefully" on broken pipe errors.
            //
            // See: https://github.com/BurntSushi/ripgrep/blob/bf63fe8f258afc09bae6caa48f0ae35eaf115005/crates/core/main.rs#L47C1-L61C14
            if let Some(ioerr) = cause.downcast_ref::<io::Error>() {
                if ioerr.kind() == io::ErrorKind::BrokenPipe {
                    return ExitStatus::Success;
                }
            }

            writeln!(stderr, "  {} {cause}", "Cause:".bold()).ok();
        }

        ExitStatus::Error
    });

    // Print allocator memory usage if TY_ALLOCATOR_STATS is set
    if std::env::var("TY_ALLOCATOR_STATS").is_ok() {
        use io::Write;
        let mut stderr = io::stderr().lock();

        if let Some(stats) = allocator::memory_usage_stats() {
            writeln!(stderr).ok();
            writeln!(stderr, "{}", "Memory Usage Statistics:".bold()).ok();
            write!(stderr, "{stats}").ok();
        } else {
            writeln!(stderr).ok();
            writeln!(
                stderr,
                "Allocator: {} (no detailed stats available)",
                allocator::allocator_name()
            )
            .ok();
        }
    }

    result
}
