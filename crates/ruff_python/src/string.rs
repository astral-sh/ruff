use once_cell::sync::Lazy;
use regex::Regex;

pub static STRING_QUOTE_PREFIX_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^(?i)[urb]*['"](?P<raw>.*)['"]$"#).unwrap());

pub fn is_lower(s: &str) -> bool {
    let mut cased = false;
    for c in s.chars() {
        if c.is_uppercase() {
            return false;
        } else if !cased && c.is_lowercase() {
            cased = true;
        }
    }
    cased
}

pub fn is_upper(s: &str) -> bool {
    let mut cased = false;
    for c in s.chars() {
        if c.is_lowercase() {
            return false;
        } else if !cased && c.is_uppercase() {
            cased = true;
        }
    }
    cased
}

/// Remove prefixes (u, r, b) and quotes around a string. This expects the given
/// string to be a valid Python string representation, it doesn't do any
/// validation.
pub fn strip_quotes_and_prefixes(s: &str) -> &str {
    match STRING_QUOTE_PREFIX_REGEX.captures(s) {
        Some(caps) => match caps.name("raw") {
            Some(m) => m.as_str(),
            None => s,
        },
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use crate::string::{is_lower, is_upper, strip_quotes_and_prefixes};

    #[test]
    fn test_is_lower() {
        assert!(is_lower("abc"));
        assert!(is_lower("a_b_c"));
        assert!(is_lower("a2c"));
        assert!(!is_lower("aBc"));
        assert!(!is_lower("ABC"));
        assert!(!is_lower(""));
        assert!(!is_lower("_"));
    }

    #[test]
    fn test_is_upper() {
        assert!(is_upper("ABC"));
        assert!(is_upper("A_B_C"));
        assert!(is_upper("A2C"));
        assert!(!is_upper("aBc"));
        assert!(!is_upper("abc"));
        assert!(!is_upper(""));
        assert!(!is_upper("_"));
    }

    #[test]
    fn test_strip_quotes_and_prefixes() {
        assert_eq!(strip_quotes_and_prefixes(r#"'a'"#), "a");
        assert_eq!(strip_quotes_and_prefixes(r#"bur'a'"#), "a");
        assert_eq!(strip_quotes_and_prefixes(r#"UrB'a'"#), "a");
        assert_eq!(strip_quotes_and_prefixes(r#""a""#), "a");
    }
}
