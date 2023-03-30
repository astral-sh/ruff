use crate::keyword::KWLIST;

/// Returns `true` if a string is a valid Python identifier (e.g., variable
/// name).
pub fn is_identifier(s: &str) -> bool {
    // Is the first character a letter or underscore?
    let mut chars = s.chars();
    if !chars
        .next()
        .map_or(false, |c| c.is_alphabetic() || c == '_')
    {
        return false;
    }

    // Are the rest of the characters letters, digits, or underscores?
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Returns `true` if a string is a private identifier, such that, when the
/// identifier is defined in a class definition, it will be mangled prior to
/// code generation.
///
/// See: <https://docs.python.org/3.5/reference/expressions.html?highlight=mangling#index-5>.
pub fn is_mangled_private(id: &str) -> bool {
    id.starts_with("__") && !id.ends_with("__")
}

/// Returns `true` if a string is a PEP 8-compliant module name (i.e., consists of lowercase
/// letters, numbers, underscores, and is not a keyword).
pub fn is_module_name(parent: Option<&str>, s: &str) -> bool {
    // Is the string a keyword?
    if KWLIST.contains(&s) {
        return false;
    }
    let mut chars = s.chars();
    // Is the first character a letter or underscore?
    if !parent.map_or(false, |parent| ["versions", "migrations"].contains(&parent))
        && !chars
            .next()
            .map_or(false, |c| c.is_ascii_lowercase() || c == '_')
    {
        return false;
    }

    // Are the rest of the characters letters, digits, or underscores?
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use crate::identifiers::is_module_name;

    #[test]
    fn test_is_module_name() {
        let parent = None;
        assert!(is_module_name(parent, "a"));
        assert!(is_module_name(parent, "abc"));
        assert!(is_module_name(parent, "abc0"));
        assert!(is_module_name(parent, "abc_"));
        assert!(is_module_name(parent, "a_b_c"));
        assert!(is_module_name(parent, "_abc"));
        assert!(!is_module_name(parent, "a-b-c"));
        assert!(!is_module_name(parent, "a_B_c"));
        assert!(!is_module_name(parent, "0abc"));
        assert!(!is_module_name(parent, "class"));
        assert!(!is_module_name(parent, "δ"));
    }

    #[test]
    fn test_is_module_name_versions() {
        let parent = Some("versions");
        assert!(is_module_name(parent, "a"));
        assert!(is_module_name(parent, "abc"));
        assert!(is_module_name(parent, "abc0"));
        assert!(is_module_name(parent, "abc_"));
        assert!(is_module_name(parent, "a_b_c"));
        assert!(is_module_name(parent, "_abc"));
        assert!(is_module_name(parent, "0abc"));
        assert!(!is_module_name(parent, "a-b-c"));
        assert!(!is_module_name(parent, "a_B_c"));
        assert!(!is_module_name(parent, "class"));
        assert!(!is_module_name(parent, "δ"));
    }

    #[test]
    fn test_is_module_name_migrations() {
        let parent = Some("migrations");
        assert!(is_module_name(parent, "a"));
        assert!(is_module_name(parent, "abc"));
        assert!(is_module_name(parent, "abc0"));
        assert!(is_module_name(parent, "abc_"));
        assert!(is_module_name(parent, "a_b_c"));
        assert!(is_module_name(parent, "_abc"));
        assert!(is_module_name(parent, "0abc"));
        assert!(!is_module_name(parent, "a-b-c"));
        assert!(!is_module_name(parent, "a_B_c"));
        assert!(!is_module_name(parent, "class"));
        assert!(!is_module_name(parent, "δ"));
    }
}
