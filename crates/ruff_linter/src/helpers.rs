#[inline]
pub(crate) fn is_logger_method_name(attr: &str) -> bool {
    matches!(
        attr,
        "debug" | "info" | "warn" | "warning" | "error" | "critical" | "log" | "exception"
    )
}
