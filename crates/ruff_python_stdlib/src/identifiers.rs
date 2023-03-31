use crate::keyword::KWLIST;

/// Returns `true` if a string is a valid Python identifier (e.g., variable
/// name).
pub fn is_identifier(name: &str) -> bool {
    // Is the first character a letter or underscore?
    let mut chars = name.chars();
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
pub fn is_module_name(name: &str, parent: Option<&str>) -> bool {
    // Is the string a keyword?
    if KWLIST.contains(&name) {
        return false;
    }

    // Is the first character a letter or underscore? As a special case, we allow files in
    // `versions` and `migrations` directories to start with a digit (e.g., `0001_initial.py`), to
    // support common conventions used by Django and other frameworks.
    let mut chars = name.chars();
    if !parent.map_or(false, |parent| matches!(parent, "versions" | "migrations")) {
        if !chars
            .next()
            .map_or(false, |c| c.is_ascii_lowercase() || c == '_')
        {
            return false;
        }
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
        assert!(is_module_name("a", parent));
        assert!(is_module_name("abc", parent));
        assert!(is_module_name("abc0", parent));
        assert!(is_module_name("abc_", parent));
        assert!(is_module_name("a_b_c", parent));
        assert!(is_module_name("_abc", parent));
        assert!(!is_module_name("a-b-c", parent));
        assert!(!is_module_name("a_B_c", parent));
        assert!(!is_module_name("0abc", parent));
        assert!(!is_module_name("class", parent));
        assert!(!is_module_name("δ", parent));
    }

    #[test]
    fn test_is_module_name_versions() {
        let parent = Some("versions");
        assert!(is_module_name("a", parent));
        assert!(is_module_name("abc", parent));
        assert!(is_module_name("abc0", parent));
        assert!(is_module_name("abc_", parent));
        assert!(is_module_name("a_b_c", parent));
        assert!(is_module_name("_abc", parent));
        assert!(is_module_name("0abc", parent));
        assert!(!is_module_name("a-b-c", parent));
        assert!(!is_module_name("a_B_c", parent));
        assert!(!is_module_name("class", parent));
        assert!(!is_module_name("δ", parent));
    }

    #[test]
    fn test_is_module_name_migrations() {
        let parent = Some("migrations");
        assert!(is_module_name("a", parent));
        assert!(is_module_name("abc", parent));
        assert!(is_module_name("abc0", parent));
        assert!(is_module_name("abc_", parent));
        assert!(is_module_name("a_b_c", parent));
        assert!(is_module_name("_abc", parent));
        assert!(is_module_name("0abc", parent));
        assert!(!is_module_name("a-b-c", parent));
        assert!(!is_module_name("a_B_c", parent));
        assert!(!is_module_name("class", parent));
        assert!(!is_module_name("δ", parent));
    }
}
