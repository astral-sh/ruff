/// Returns `true` if a comment appears to be a pragma comment.
///
/// ```
/// assert!(ruff_python_trivia::is_pragma("# type: ignore"));
/// assert!(ruff_python_trivia::is_pragma("# noqa: F401"));
/// assert!(ruff_python_trivia::is_pragma("# noqa"));
/// assert!(ruff_python_trivia::is_pragma("# NoQA"));
/// assert!(ruff_python_trivia::is_pragma("# nosec"));
/// assert!(ruff_python_trivia::is_pragma("# nosec B602, B607"));
/// assert!(ruff_python_trivia::is_pragma("# isort: off"));
/// assert!(ruff_python_trivia::is_pragma("# isort: skip"));
/// ```
pub fn is_pragma(comment: &str) -> bool {
    let Some(content) = comment.strip_prefix('#') else {
        return false;
    };
    let trimmed = content.trim_start();

    // Case-insensitive match against `noqa` (which doesn't require a trailing colon).
    matches!(
        trimmed.as_bytes(),
        [b'n' | b'N', b'o' | b'O', b'q' | b'Q', b'a' | b'A', ..]
    ) ||
        // Case-insensitive match against pragmas that don't require a trailing colon.
        trimmed.starts_with("nosec") ||
        // Case-sensitive match against a variety of pragmas (which do require a trailing colon).
        trimmed
        .split_once(':')
        .is_some_and(|(maybe_pragma, _)| matches!(maybe_pragma, "isort" | "type" | "pyright" | "pylint" | "flake8" | "ruff"))
}
