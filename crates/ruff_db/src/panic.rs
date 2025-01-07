use std::cell::Cell;
use std::sync::OnceLock;

#[derive(Default, Debug)]
pub struct PanicError {
    pub info: String,
    pub backtrace: Option<std::backtrace::Backtrace>,
}

impl std::fmt::Display for PanicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.info)?;
        if let Some(backtrace) = &self.backtrace {
            writeln!(f, "Backtrace: {backtrace}")
        } else {
            Ok(())
        }
    }
}

thread_local! {
    static CAPTURE_PANIC_INFO: Cell<bool> = const { Cell::new(false) };
    static LAST_PANIC: Cell<Option<PanicError>> = const { Cell::new(None) };
}

fn install_hook() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let should_capture = CAPTURE_PANIC_INFO.with(Cell::get);
            if !should_capture {
                return (*prev)(info);
            }
            let info = info.to_string();
            let backtrace = std::backtrace::Backtrace::force_capture();
            LAST_PANIC.with(|cell| {
                cell.set(Some(PanicError {
                    info,
                    backtrace: Some(backtrace),
                }));
            });
        }));
    });
}

/// Invokes a closure, capturing and returning the cause of an unwinding panic if one occurs.
///
/// ### Thread safety
///
/// This is implemented by installing a custom [panic hook](std::panic::set_hook).  This panic hook
/// is a global resource.  The hook that we install captures panic info in a thread-safe manner,
/// and also ensures that any threads that are _not_ currently using this `catch_unwind` wrapper
/// still use the previous hook (typically the default hook, which prints out panic information to
/// stderr).
///
/// Note that we are careful to install our custom hook only once, and we do not restore the
/// previous hook (since can always retain the previous hook's behavior by not calling this
/// wrapper).
pub fn catch_unwind<F, R>(f: F) -> Result<R, PanicError>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    install_hook();
    let prev_should_capture =
        CAPTURE_PANIC_INFO.with(|should_capture| should_capture.replace(true));
    let result = std::panic::catch_unwind(f)
        .map_err(|_| LAST_PANIC.with(std::cell::Cell::take).unwrap_or_default());
    CAPTURE_PANIC_INFO.with(|should_capture| should_capture.set(prev_should_capture));
    result
}
