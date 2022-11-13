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

#[cfg(test)]
mod tests {
    use crate::python::string::{is_lower, is_upper};

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
}
