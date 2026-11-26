use colored::Colorize;
use std::io;
use ty::{ExitStatus, run};

#[cfg(all(
    not(target_os = "macos"),
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
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[expect(
    unsafe_code,
    reason = "The mimalloc bindings omit the purge-delay option"
)]
mod mimalloc_policy {
    use std::ffi::{c_int, c_long};

    // See the [vendored 2.3.2 header](https://docs.rs/crate/libmimalloc-sys/0.1.49/source/c_src/mimalloc/v2/include/mimalloc.h#411).
    const MI_OPTION_PURGE_DELAY: c_int = 15;
    const MI_VERSION_2_3_2: c_int = 20302;

    unsafe extern "C" {
        safe fn mi_version() -> c_int;
        fn mi_option_set_default(option: c_int, value: c_long);
    }

    /// Mark unused pages reusable promptly unless the user set a purge delay.
    pub(super) fn configure_purging() {
        if mi_version() == MI_VERSION_2_3_2 {
            // SAFETY: The option index is valid for this version. We set it before
            // starting worker threads because mimalloc's options are process-wide.
            unsafe { mi_option_set_default(MI_OPTION_PURGE_DELAY, 0) };
        }
    }

    #[test]
    fn purge_delay_option_version() {
        assert_eq!(
            mi_version(),
            MI_VERSION_2_3_2,
            "Check the purge-delay option index against the new mimalloc header"
        );
    }
}

fn main() -> ExitStatus {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    mimalloc_policy::configure_purging();

    run().unwrap_or_else(|error| {
        use io::Write;

        // Exit "gracefully" on broken pipe errors.
        //
        // See: https://github.com/BurntSushi/ripgrep/blob/bf63fe8f258afc09bae6caa48f0ae35eaf115005/crates/core/main.rs#L47C1-L61C14
        if error.chain().any(|cause| {
            cause
                .downcast_ref::<io::Error>()
                .is_some_and(|ioerr| ioerr.kind() == io::ErrorKind::BrokenPipe)
        }) {
            return ExitStatus::Success;
        }

        // Use `writeln` instead of `eprintln` to avoid panicking when the stderr pipe is broken.
        let mut stderr = io::stderr().lock();

        // This communicates that this isn't a linter error but ty itself hard-errored for
        // some reason (e.g. failed to resolve the configuration)
        writeln!(stderr, "{}", "ty failed".red().bold()).ok();
        // Currently we generally only see one error, but e.g. with io errors when resolving
        // the configuration it is help to chain errors ("resolving configuration failed" ->
        // "failed to read file: subdir/pyproject.toml")
        for cause in error.chain() {
            writeln!(stderr, "  {} {cause}", "Cause:".bold()).ok();
        }

        ExitStatus::Error
    })
}
