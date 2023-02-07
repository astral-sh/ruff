/// Returns `true` if a string is a valid Python identifier (e.g., variable
/// name).
pub fn is_identifier(s: &str) -> bool {
    // Is the first character a letter or underscore?
    if !s
        .chars()
        .next()
        .map_or(false, |c| c.is_alphabetic() || c == '_')
    {
        return false;
    }

    // Are the rest of the characters letters, digits, or underscores?
    s.chars().skip(1).all(|c| c.is_alphanumeric() || c == '_')
}

/// Returns `true` if a string is a private identifier, such that, when the
/// identifier is defined in a class definition, it will be mangled prior to
/// code generation.
///
/// See: <https://docs.python.org/3.5/reference/expressions.html?highlight=mangling#index-5>.
pub fn is_mangled_private(id: &str) -> bool {
    id.starts_with("__") && !id.ends_with("__")
}
