/// Returns `true` if a comment appears to be a pragma comment.
///
/// ```
/// assert!(ruff_python_trivia::is_pragma_comment("# type: ignore"));
/// assert!(ruff_python_trivia::is_pragma_comment("# noqa: F401"));
/// assert!(ruff_python_trivia::is_pragma_comment("# noqa"));
/// assert!(ruff_python_trivia::is_pragma_comment("# NoQA"));
/// assert!(ruff_python_trivia::is_pragma_comment("# nosec"));
/// assert!(ruff_python_trivia::is_pragma_comment("# nosec B602, B607"));
/// assert!(ruff_python_trivia::is_pragma_comment("# isort: off"));
/// assert!(ruff_python_trivia::is_pragma_comment("# isort: skip"));
/// ```
pub fn is_pragma_comment(comment: &str) -> bool {
    let Some(content) = comment.strip_prefix('#') else {
        return false;
    };

    let trimmed = content.trim_start();

    // Case-insensitive check for `noqa` or `nosec` anywhere in the comment
    if trimmed
        .bytes()
        .map(|b| b.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .windows(4)
        .any(|w| w == b"noqa")
        || trimmed
            .bytes()
            .map(|b| b.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .windows(5)
            .any(|w| w == b"nosec")
    {
        return true;
    }

    // Case-sensitive check for tool-specific pragmas like `isort:skip`
    for part in trimmed.split('#') {
        if let Some((pragma, _)) = part.trim().split_once(':') {
            if matches!(
                pragma,
                "isort" | "type" | "pyright" | "pylint" | "flake8" | "ruff"
            ) {
                return true;
            }
        }
    }

    false
}
