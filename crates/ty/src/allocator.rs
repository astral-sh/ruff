//! Global allocator configuration for ty.
//!
//! By default:
//! - Windows uses mimalloc
//! - Unix-like platforms (on supported architectures) use jemalloc
//! - Other platforms use the system allocator
//!
//! The `mimalloc` feature can be enabled to prefer mimalloc over jemalloc
//! on platforms that support both.
//!
//! # Memory Statistics
//!
//! Set `TY_ALLOCATOR_STATS=1` to print memory usage statistics on exit.
//!
//! ## jemalloc (default on Unix-like platforms)
//!
//! The `TY_ALLOCATOR_STATS` output includes:
//! - **Allocated**: Total bytes allocated by the application
//! - **Active**: Total bytes in active pages (may be higher than allocated due to fragmentation)
//! - **Resident**: Total bytes in physically resident pages
//! - **Mapped**: Total bytes in active extents mapped by the allocator
//! - **Retained**: Total bytes in virtual memory mappings retained for future reuse
//! - **Metadata**: Total bytes dedicated to allocator metadata
//! - **Fragmentation**: Percentage of resident memory not actively used
//!
//! For more detailed jemalloc statistics, use the `MALLOC_CONF` environment variable:
//! ```bash
//! # Print stats on exit
//! MALLOC_CONF=stats_print:true ty check .
//!
//! # Print detailed stats including per-arena and per-size-class info
//! MALLOC_CONF=stats_print:true,stats_print_opts:gblam ty check .
//! ```
//!
//! Available `stats_print_opts` flags:
//! - `g`: general statistics
//! - `m`: merged arena statistics
//! - `d`: destroyed arena statistics (if enabled)
//! - `a`: per-arena statistics
//! - `b`: per-size-class statistics for bins
//! - `l`: per-size-class statistics for large objects
//! - `x`: mutex statistics (if enabled)
//!
//! ## mimalloc (Windows default, or with `--features mimalloc`)
//!
//! For detailed mimalloc statistics, use environment variables:
//! ```bash
//! # Print stats on exit
//! MIMALLOC_SHOW_STATS=1 ty check .
//!
//! # More verbose output
//! MIMALLOC_VERBOSE=1 ty check .
//!
//! # Both together for maximum detail
//! MIMALLOC_SHOW_STATS=1 MIMALLOC_VERBOSE=1 ty check .
//! ```

use std::fmt::Write;

// Condition for platforms where we can use either jemalloc or mimalloc
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    not(target_os = "aix"),
    not(target_os = "android"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
        target_arch = "riscv64"
    )
))]
mod unix_allocator {
    #[cfg(feature = "mimalloc")]
    #[global_allocator]
    pub(super) static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

    #[cfg(not(feature = "mimalloc"))]
    #[global_allocator]
    pub(super) static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
}

// Windows always uses mimalloc
#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Returns the name of the allocator currently in use.
#[must_use]
pub(crate) fn allocator_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "mimalloc"
    }

    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "openbsd"),
        not(target_os = "aix"),
        not(target_os = "android"),
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "powerpc64",
            target_arch = "riscv64"
        )
    ))]
    {
        #[cfg(feature = "mimalloc")]
        {
            "mimalloc"
        }

        #[cfg(not(feature = "mimalloc"))]
        {
            "jemalloc"
        }
    }

    #[cfg(not(any(
        target_os = "windows",
        all(
            not(target_os = "openbsd"),
            not(target_os = "aix"),
            not(target_os = "android"),
            any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "powerpc64",
                target_arch = "riscv64"
            )
        )
    )))]
    {
        "system"
    }
}

/// Collects and formats memory usage statistics from the allocator.
///
/// Returns a formatted string with memory statistics specific to the
/// allocator in use, or `None` if memory statistics are not available
/// for the current allocator.
#[must_use]
pub(crate) fn memory_usage_stats() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        mimalloc_stats()
    }

    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "openbsd"),
        not(target_os = "aix"),
        not(target_os = "android"),
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "powerpc64",
            target_arch = "riscv64"
        )
    ))]
    {
        #[cfg(feature = "mimalloc")]
        {
            mimalloc_stats()
        }

        #[cfg(not(feature = "mimalloc"))]
        {
            jemalloc_stats()
        }
    }

    #[cfg(not(any(
        target_os = "windows",
        all(
            not(target_os = "openbsd"),
            not(target_os = "aix"),
            not(target_os = "android"),
            any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "powerpc64",
                target_arch = "riscv64"
            )
        )
    )))]
    {
        None
    }
}

/// Collect jemalloc memory statistics
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    not(target_os = "aix"),
    not(target_os = "android"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
        target_arch = "riscv64"
    ),
    not(feature = "mimalloc")
))]
fn jemalloc_stats() -> Option<String> {
    use tikv_jemalloc_ctl::{epoch, stats};

    // Advance the epoch to get fresh statistics
    if epoch::advance().is_err() {
        return None;
    }

    let allocated = stats::allocated::read().ok()?;
    let active = stats::active::read().ok()?;
    let resident = stats::resident::read().ok()?;
    let mapped = stats::mapped::read().ok()?;
    let retained = stats::retained::read().ok()?;
    let metadata = stats::metadata::read().ok()?;

    let mut output = String::new();
    writeln!(output, "Allocator: jemalloc").ok()?;
    writeln!(output, "  Allocated:     {} ({} bytes)", format_bytes(allocated), allocated).ok()?;
    writeln!(output, "  Active:        {} ({} bytes)", format_bytes(active), active).ok()?;
    writeln!(output, "  Resident:      {} ({} bytes)", format_bytes(resident), resident).ok()?;
    writeln!(output, "  Mapped:        {} ({} bytes)", format_bytes(mapped), mapped).ok()?;
    writeln!(output, "  Retained:      {} ({} bytes)", format_bytes(retained), retained).ok()?;
    writeln!(output, "  Metadata:      {} ({} bytes)", format_bytes(metadata), metadata).ok()?;
    writeln!(output).ok()?;
    writeln!(output, "  Fragmentation: {:.2}%", fragmentation_percent(allocated, resident)).ok()?;
    writeln!(output).ok()?;
    writeln!(output, "  Tip: Set MALLOC_CONF=stats_print:true for detailed jemalloc stats on exit").ok()?;

    Some(output)
}

/// Collect mimalloc memory statistics
#[cfg(any(
    target_os = "windows",
    all(
        not(target_os = "openbsd"),
        not(target_os = "aix"),
        not(target_os = "android"),
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "powerpc64",
            target_arch = "riscv64"
        ),
        feature = "mimalloc"
    )
))]
fn mimalloc_stats() -> Option<String> {
    // mimalloc doesn't have a simple stats API like jemalloc-ctl
    // We can use the heap stats from the default heap
    let mut output = String::new();
    writeln!(output, "Allocator: mimalloc").ok()?;
    writeln!(output, "  (Detailed stats available via MIMALLOC_SHOW_STATS=1 environment variable)").ok()?;

    // Try to get basic heap stats if available
    // mimalloc::heap::stats() is not always available, so we provide basic info
    writeln!(output).ok()?;
    writeln!(output, "  Tip: Set MIMALLOC_SHOW_STATS=1 to see detailed allocation statistics on exit").ok()?;
    writeln!(output, "  Tip: Set MIMALLOC_VERBOSE=1 for even more detailed output").ok()?;

    Some(output)
}

/// Format bytes in a human-readable format
#[cfg(any(
    test,
    all(
        not(target_os = "windows"),
        not(target_os = "openbsd"),
        not(target_os = "aix"),
        not(target_os = "android"),
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "powerpc64",
            target_arch = "riscv64"
        ),
        not(feature = "mimalloc")
    )
))]
fn format_bytes(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Calculate fragmentation percentage
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    not(target_os = "aix"),
    not(target_os = "android"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
        target_arch = "riscv64"
    ),
    not(feature = "mimalloc")
))]
fn fragmentation_percent(allocated: usize, resident: usize) -> f64 {
    if resident == 0 {
        0.0
    } else {
        ((resident - allocated) as f64 / resident as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocator_name() {
        let name = allocator_name();
        assert!(
            name == "jemalloc" || name == "mimalloc" || name == "system",
            "Unexpected allocator name: {name}"
        );
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }
}
