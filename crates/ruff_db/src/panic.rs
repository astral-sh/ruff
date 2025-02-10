use std::cell::Cell;
use std::panic::Location;
use std::sync::OnceLock;

#[derive(Default, Debug)]
pub struct PanicError {
    pub location: Option<String>,
    pub payload: Option<String>,
    pub backtrace: Option<std::backtrace::Backtrace>,
}

impl std::fmt::Display for PanicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "panicked at")?;
        if let Some(location) = &self.location {
            write!(f, " {location}")?;
        }
        if let Some(payload) = &self.payload {
            write!(f, ":\n{payload}")?;
        }
        if let Some(backtrace) = &self.backtrace {
            writeln!(f, "\nBacktrace: {backtrace}")?;
        }
        Ok(())
    }
}

thread_local! {
    static CAPTURE_PANIC_INFO: Cell<bool> = const { Cell::new(false) };
    static OUR_HOOK_RAN: Cell<bool> = const { Cell::new(false) };
    static LAST_PANIC: Cell<Option<PanicError>> = const { Cell::new(None) };
}

fn install_hook() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            OUR_HOOK_RAN.with(|cell| cell.set(true));
            let should_capture = CAPTURE_PANIC_INFO.with(Cell::get);
            if !should_capture {
                return (*prev)(info);
            }
            let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
                Some(s.to_string())
            } else {
                info.payload().downcast_ref::<String>().cloned()
            };
            let location = info.location().map(Location::to_string);
            let backtrace = std::backtrace::Backtrace::force_capture();
            LAST_PANIC.with(|cell| {
                cell.set(Some(PanicError {
                    payload,
                    location,
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
/// We assume that there is nothing else running in this process that needs to install a competing
/// panic hook.  We are careful to install our custom hook only once, and we do not ever restore
/// the previous hook (since you can always retain the previous hook's behavior by not calling this
/// wrapper).
pub fn catch_unwind<F, R>(f: F) -> Result<R, PanicError>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    install_hook();
    OUR_HOOK_RAN.with(|cell| cell.set(false));
    let prev_should_capture = CAPTURE_PANIC_INFO.with(|cell| cell.replace(true));
    let result = std::panic::catch_unwind(f).map_err(|_| {
        let our_hook_ran = OUR_HOOK_RAN.with(Cell::get);
        if !our_hook_ran {
            panic!("detected a competing panic hook");
        }
        LAST_PANIC.with(Cell::take).unwrap_or_default()
    });
    CAPTURE_PANIC_INFO.with(|cell| cell.set(prev_should_capture));
    result
}
