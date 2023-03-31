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
    if !chars.all(|c| c.is_alphanumeric() || c == '_') {
        return false;
    }

    // Is the identifier a keyword?
    if KWLIST.contains(&name) {
        return false;
    }

    true
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
pub fn is_module_name(name: &str) -> bool {
    // Is the first character a letter or underscore?
    let mut chars = name.chars();
    if !chars
        .next()
        .map_or(false, |c| c.is_ascii_lowercase() || c == '_')
    {
        return false;
    }

    // Are the rest of the characters letters, digits, or underscores?
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return false;
    }

    // Is the identifier a keyword?
    if KWLIST.contains(&name) {
        return false;
    }

    true
}

/// Returns `true` if a string appears to be a valid migration file name (e.g., `0001_initial.py`).
pub fn is_migration_name(name: &str) -> bool {
    // Are characters letters, digits, or underscores?
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return false;
    }

    // Is the identifier a keyword?
    if KWLIST.contains(&name) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use crate::identifiers::{is_migration_name, is_module_name};

    #[test]
    fn module_name() {
        assert!(is_module_name("_abc"));
        assert!(is_module_name("a"));
        assert!(is_module_name("a_b_c"));
        assert!(is_module_name("abc"));
        assert!(is_module_name("abc0"));
        assert!(is_module_name("abc_"));
        assert!(!is_module_name("0001_initial"));
        assert!(!is_module_name("0abc"));
        assert!(!is_module_name("a-b-c"));
        assert!(!is_module_name("a_B_c"));
        assert!(!is_module_name("class"));
        assert!(!is_module_name("δ"));
    }

    #[test]
    fn migration_name() {
        assert!(is_migration_name("0001_initial"));
        assert!(is_migration_name("0abc"));
        assert!(is_migration_name("_abc"));
        assert!(is_migration_name("a"));
        assert!(is_migration_name("a_b_c"));
        assert!(is_migration_name("abc"));
        assert!(is_migration_name("abc0"));
        assert!(is_migration_name("abc_"));
        assert!(!is_migration_name("a-b-c"));
        assert!(!is_migration_name("a_B_c"));
        assert!(!is_migration_name("class"));
        assert!(!is_migration_name("δ"));
    }
}
