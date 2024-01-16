use unicode_ident::{is_xid_continue, is_xid_start};

use crate::keyword::is_keyword;

/// Returns `true` if a string is a valid Python identifier (e.g., variable
/// name).
pub fn is_identifier(name: &str) -> bool {
    // Is the first character a letter or underscore?
    let mut chars = name.chars();
    if !chars.next().is_some_and(is_identifier_start) {
        return false;
    }

    // Are the rest of the characters letters, digits, or underscores?
    if !chars.all(is_identifier_continuation) {
        return false;
    }

    // Is the identifier a keyword?
    if is_keyword(name) {
        return false;
    }

    true
}

// Checks if the character c is a valid starting character as described
// in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
fn is_identifier_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_') || is_xid_start(c)
}

// Checks if the character c is a valid continuation character as described
// in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
fn is_identifier_continuation(c: char) -> bool {
    // Arrange things such that ASCII codepoints never
    // result in the slower `is_xid_continue` getting called.
    if c.is_ascii() {
        matches!(c, 'a'..='z' | 'A'..='Z' | '_' | '0'..='9')
    } else {
        is_xid_continue(c)
    }
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
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
    {
        return false;
    }

    // Are the rest of the characters letters, digits, or underscores?
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return false;
    }

    // Is the identifier a keyword?
    if is_keyword(name) {
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
    if is_keyword(name) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use crate::identifiers::{is_identifier, is_migration_name, is_module_name};

    #[test]
    fn valid_identifiers() {
        assert!(is_identifier("_abc"));
        assert!(is_identifier("abc"));
        assert!(is_identifier("_"));
        assert!(is_identifier("a_b_c"));
        assert!(is_identifier("abc123"));
        assert!(is_identifier("abc_123"));
        assert!(is_identifier("漢字"));
        assert!(is_identifier("ひらがな"));
        assert!(is_identifier("العربية"));
        assert!(is_identifier("кириллица"));
        assert!(is_identifier("πr"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("percentile_co³t"));
        assert!(!is_identifier("HelloWorld❤️"));
    }

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
