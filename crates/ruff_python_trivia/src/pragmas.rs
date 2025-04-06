/// Returns `true` if a comment appears to be a pragma comment.
///
/// ```
/// use ruff_python_trivia::{default_pragma_tags, default_pragma_tags_case_insensitive, is_pragma_comment};
///
/// let tags = default_pragma_tags();
/// let case_insensitive_tags = default_pragma_tags_case_insensitive();
/// assert!(is_pragma_comment("# type: ignore", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# noqa: F401", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# noqa", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# NoQA", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# nosec", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# nosec B602, B607", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# isort: off", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# isort: skip", &tags, &case_insensitive_tags));
/// assert!(is_pragma_comment("# pragma: no cover", &["pragma:".to_string()], &[]));
/// ```
pub fn is_pragma_comment(
    comment: &str,
    pragma_tags: &[String],
    pragma_tags_case_insensitive: &[String],
) -> bool {
    let Some(content) = comment.strip_prefix('#') else {
        return false;
    };
    let trimmed = content.trim_start();

    // Check for case-insensitive match against tags
    pragma_tags_case_insensitive
    .iter()
    .any(|tag| {
        trimmed
            .get(..tag.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(tag))
    })
    // Check for case-sensitive tags
    || pragma_tags
        .iter()
        .any(|tag| trimmed.starts_with(tag))
}

/// Returns `true` if a comment appears to be a pragma comment using default pragma tags.
/// Used by linter as it doesn't allow custom pragma tags.
///
/// ```
/// use ruff_python_trivia::is_pragma_comment_with_defaults;
///
/// assert!(is_pragma_comment_with_defaults("# type: ignore"));
/// assert!(is_pragma_comment_with_defaults("# noqa: F401"));
/// assert!(is_pragma_comment_with_defaults("# noqa"));
/// assert!(is_pragma_comment_with_defaults("# NoQA"));
/// assert!(is_pragma_comment_with_defaults("# nosec"));
/// assert!(is_pragma_comment_with_defaults("# nosec B602, B607"));
/// assert!(is_pragma_comment_with_defaults("# isort: off"));
/// assert!(is_pragma_comment_with_defaults("# isort: skip"));
/// ```
pub fn is_pragma_comment_with_defaults(comment: &str) -> bool {
    is_pragma_comment(
        comment,
        &default_pragma_tags(),
        &default_pragma_tags_case_insensitive(),
    )
}

/// Returns the default list of pragma tags that should be ignored for line-too-long by the formatter.
pub fn default_pragma_tags() -> Vec<String> {
    vec![
        "type:".to_string(),
        "pyright:".to_string(),
        "pylint:".to_string(),
        "flake8:".to_string(),
        "ruff:".to_string(),
        "isort:".to_string(),
        "nosec".to_string(),
    ]
}

/// Returns the default list of case-insensitive pragma tags that should be ignored for line-too-long by the formatter.
pub fn default_pragma_tags_case_insensitive() -> Vec<String> {
    vec!["noqa".to_string()]
}
