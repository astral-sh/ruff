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
