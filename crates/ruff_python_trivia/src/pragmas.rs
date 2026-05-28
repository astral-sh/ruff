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
/// assert!(ruff_python_trivia::is_pragma_comment("# pyrefly: ignore[missing-attribute]"));
/// ```
pub fn is_pragma_comment(comment: &str) -> bool {
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
        // Case-sensitive match against a variety of pragmas that _do_ require a trailing colon.
        trimmed
        .split_once(':')
        .is_some_and(|(maybe_pragma, _)| matches!(maybe_pragma, "isort" | "type" | "pyright" | "pyrefly" | "pylint" | "flake8" | "ruff" | "ty"))
}

/// Returns the byte offset within `comment` where a trailing pragma comment starts,
/// or `None` if no pragma is found.
///
/// For a plain pragma like `# noqa: F401`, returns `Some(0)`.
/// For a nested pragma like `# some text # noqa: F401`, returns the offset of the
/// trailing `#` that begins the pragma (i.e., the start of `# noqa: F401`).
///
/// ```
/// assert_eq!(ruff_python_trivia::find_trailing_pragma_offset("# noqa: F401"), Some(0));
/// assert_eq!(ruff_python_trivia::find_trailing_pragma_offset("# type: ignore"), Some(0));
/// assert_eq!(ruff_python_trivia::find_trailing_pragma_offset("# some comment # noqa: F401"), Some(15));
/// assert_eq!(ruff_python_trivia::find_trailing_pragma_offset("## noqa: F401"), Some(1));
/// assert_eq!(ruff_python_trivia::find_trailing_pragma_offset("# just a comment"), None);
/// ```
pub fn find_trailing_pragma_offset(comment: &str) -> Option<usize> {
    comment.match_indices('#').find_map(|(offset, _)| {
        let sub_comment = &comment[offset..];
        if is_pragma_comment(sub_comment) {
            Some(offset)
        } else {
            None
        }
    })
}
