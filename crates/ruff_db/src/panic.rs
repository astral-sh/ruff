use std::backtrace::BacktraceStatus;
use std::cell::Cell;
use std::panic::Location;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct PanicError {
    pub location: Option<String>,
    pub payload: Payload,
    pub backtrace: Option<std::backtrace::Backtrace>,
    pub salsa_backtrace: Option<salsa::Backtrace>,
}

#[derive(Debug)]
pub struct Payload(Box<dyn std::any::Any + Send>);

impl Payload {
    pub fn as_str(&self) -> Option<&str> {
        if let Some(s) = self.0.downcast_ref::<String>() {
            Some(s)
        } else if let Some(s) = self.0.downcast_ref::<&str>() {
            Some(s)
        } else {
            None
        }
    }
}

impl std::fmt::Display for PanicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "panicked at")?;
        if let Some(location) = &self.location {
            write!(f, " {location}")?;
        }
        if let Some(payload) = self.payload.as_str() {
            write!(f, ":\n{payload}")?;
        }
        if let Some(backtrace) = &self.backtrace {
            match backtrace.status() {
                BacktraceStatus::Disabled => {
                    writeln!(
                        f,
                        "\nrun with `RUST_BACKTRACE=1` environment variable to display a backtrace"
                    )?;
                }
                BacktraceStatus::Captured => {
                    writeln!(f, "\nBacktrace: {backtrace}")?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct CapturedPanicInfo {
    backtrace: Option<std::backtrace::Backtrace>,
    location: Option<String>,
    salsa_backtrace: Option<salsa::Backtrace>,
}

thread_local! {
    static CAPTURE_PANIC_INFO: Cell<bool> = const { Cell::new(false) };
    static LAST_BACKTRACE: Cell<CapturedPanicInfo> = const {
        Cell::new(CapturedPanicInfo { backtrace: None, location: None, salsa_backtrace: None })
    };
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

            let location = info.location().map(Location::to_string);
            let backtrace = Some(std::backtrace::Backtrace::capture());

            LAST_BACKTRACE.set(CapturedPanicInfo {
                backtrace,
                location,
                salsa_backtrace: salsa::Backtrace::capture(),
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
/// panic hook. We are careful to install our custom hook only once, and we do not ever restore
/// the previous hook (since you can always retain the previous hook's behavior by not calling this
/// wrapper).
pub fn catch_unwind<F, R>(f: F) -> Result<R, PanicError>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    install_hook();
    let prev_should_capture = CAPTURE_PANIC_INFO.replace(true);
    let result = std::panic::catch_unwind(f).map_err(|payload| {
        // Try to get the backtrace and location from our custom panic hook.
        // The custom panic hook only runs once when `panic!` is called (or similar). It doesn't
        // run when the panic is propagated with `std::panic::resume_unwind`. The panic hook
        // is also not called when the panic is raised with `std::panic::resum_unwind` as is the
        // case for salsa unwinds (see the ignored test below).
        // Because of that, always take the payload from `catch_unwind` because it may have been transformed
        // by an inner `std::panic::catch_unwind` handlers and only use the information
        // from the custom handler to enrich the error with the backtrace and location.
        let CapturedPanicInfo {
            location,
            backtrace,
            salsa_backtrace,
        } = LAST_BACKTRACE.with(Cell::take);

        PanicError {
            location,
            payload: Payload(payload),
            backtrace,
            salsa_backtrace,
        }
    });
    CAPTURE_PANIC_INFO.set(prev_should_capture);
    result
}

#[cfg(test)]
mod tests {
    use salsa::{Database, Durability};

    #[test]
    #[ignore = "super::catch_unwind installs a custom panic handler, which could effect test isolation"]
    fn no_backtrace_for_salsa_cancelled() {
        #[salsa::input]
        struct Input {
            value: u32,
        }

        #[salsa::tracked]
        fn test_query(db: &dyn Database, input: Input) -> u32 {
            loop {
                // This should throw a cancelled error
                let _ = input.value(db);
            }
        }

        let db = salsa::DatabaseImpl::new();

        let input = Input::new(&db, 42);

        let result = std::thread::scope(move |scope| {
            {
                let mut db = db.clone();
                scope.spawn(move || {
                    // This will cancel the other thread by throwing a `salsa::Cancelled` error.
                    db.synthetic_write(Durability::MEDIUM);
                });
            }

            {
                scope.spawn(move || {
                    super::catch_unwind(|| {
                        test_query(&db, input);
                    })
                })
            }
            .join()
            .unwrap()
        });

        match result {
            Ok(_) => panic!("Expected query to panic"),
            Err(err) => {
                // Panics triggered with `resume_unwind` have no backtrace.
                assert!(err.backtrace.is_none());
            }
        }
    }
}
