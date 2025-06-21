/// Returns a tuple containing:
/// 1. A boolean indicating if a comment appears to be a pragma comment
/// 2. The position in the comment where the pragma begins (0 if the entire comment is a pragma)
///
/// ```
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# type: ignore"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# noqa: F401"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# noqa"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# NoQA"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# nosec"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# nosec B602, B607"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# isort: off"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# isort: skip"), (true, 0));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# test # noqa"), (true, 6));
/// assert_eq!(ruff_python_trivia::is_pragma_comment("# not a pragma"), (false, 0));
/// ```
pub fn is_pragma_comment(comment: &str) -> (bool, usize) {
    // Check if the entire comment is a pragma
    if let Some(content) = comment.strip_prefix('#') {
        let trimmed = content.trim_start();

        let is_pragma = 
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
                .is_some_and(|(maybe_pragma, _)| matches!(maybe_pragma, "isort" | "type" | "pyright" | "pylint" | "flake8" | "ruff" | "ty"));
        
        if is_pragma {
            return (true, 0);
        }
    } else {
        return (false, 0);
    }
    
    // If the entire comment isn't a pragma, check for nested pragma comments
    let content = comment.strip_prefix('#').unwrap_or(comment);
    
    let mut position = 0;
    for (i, part) in content.split('#').enumerate() {
        if i == 0 {
            position += part.len();
            continue;
        }
        
        let hash_position = position; // Position of the '#' character
        
        // Check if this part forms a pragma comment by itself
        let potential_pragma = format!("#{}", part.trim());
        let (is_pragma, _) = is_pragma_comment_internal(&potential_pragma);
        
        if is_pragma {
            return (true, hash_position);
        }
        
        position += 1 + part.len(); // +1 for the '#'
    }
    
    (false, 0)
}

/// Internal helper that uses the original boolean logic without recursion
fn is_pragma_comment_internal(comment: &str) -> (bool, usize) {
    let Some(content) = comment.strip_prefix('#') else {
        return (false, 0);
    };
    let trimmed = content.trim_start();

    let is_pragma = 
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
            .is_some_and(|(maybe_pragma, _)| matches!(maybe_pragma, "isort" | "type" | "pyright" | "pylint" | "flake8" | "ruff" | "ty"));
    
    (is_pragma, 0)
}
