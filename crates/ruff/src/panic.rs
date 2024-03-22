#[derive(Default, Debug)]
pub(crate) struct PanicError {
    pub(crate) info: String,
    pub(crate) backtrace: Option<std::backtrace::Backtrace>,
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
    static LAST_PANIC: std::cell::Cell<Option<PanicError>> = const { std::cell::Cell::new(None) };
}

/// [`catch_unwind`](std::panic::catch_unwind) wrapper that sets a custom [`set_hook`](std::panic::set_hook)
/// to extract the backtrace. The original panic-hook gets restored before returning.
pub(crate) fn catch_unwind<F, R>(f: F) -> Result<R, PanicError>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let info = info.to_string();
        let backtrace = std::backtrace::Backtrace::force_capture();
        LAST_PANIC.with(|cell| {
            cell.set(Some(PanicError {
                info,
                backtrace: Some(backtrace),
            }));
        });
    }));

    let result = std::panic::catch_unwind(f)
        .map_err(|_| LAST_PANIC.with(std::cell::Cell::take).unwrap_or_default());

    std::panic::set_hook(prev);

    result
}
