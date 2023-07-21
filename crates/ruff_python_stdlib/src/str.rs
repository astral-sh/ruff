/// Return `true` if a string is lowercase.
///
/// A string is lowercase if all alphabetic characters in the string are lowercase.
///
/// ## Examples
///
/// ```rust
/// use ruff_python_stdlib::str::is_lowercase;
///
/// assert!(is_lowercase("abc"));
/// assert!(is_lowercase("a_b_c"));
/// assert!(is_lowercase("a2c"));
/// assert!(!is_lowercase("aBc"));
/// assert!(!is_lowercase("ABC"));
/// assert!(is_lowercase(""));
/// assert!(is_lowercase("_"));
/// ```
pub fn is_lowercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || c.is_lowercase())
}

/// Return `true` if a string is uppercase.
///
/// A string is uppercase if all alphabetic characters in the string are uppercase.
///
/// ## Examples
///
/// ```rust
/// use ruff_python_stdlib::str::is_uppercase;
///
/// assert!(is_uppercase("ABC"));
/// assert!(is_uppercase("A_B_C"));
/// assert!(is_uppercase("A2C"));
/// assert!(!is_uppercase("aBc"));
/// assert!(!is_uppercase("abc"));
/// assert!(is_uppercase(""));
/// assert!(is_uppercase("_"));
/// ```
pub fn is_uppercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

/// Return `true` if a string is _cased_ as lowercase.
///
/// A string is cased as lowercase if it contains at least one lowercase character and no uppercase
/// characters.
///
/// This differs from `str::is_lowercase` in that it returns `false` for empty strings and strings
/// that contain only underscores or other non-alphabetic characters.
///
/// ## Examples
///
/// ```rust
/// use ruff_python_stdlib::str::is_cased_lowercase;
///
/// assert!(is_cased_lowercase("abc"));
/// assert!(is_cased_lowercase("a_b_c"));
/// assert!(is_cased_lowercase("a2c"));
/// assert!(!is_cased_lowercase("aBc"));
/// assert!(!is_cased_lowercase("ABC"));
/// assert!(!is_cased_lowercase(""));
/// assert!(!is_cased_lowercase("_"));
/// ```
pub fn is_cased_lowercase(s: &str) -> bool {
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

/// Return `true` if a string is _cased_ as uppercase.
///
/// A string is cased as uppercase if it contains at least one uppercase character and no lowercase
/// characters.
///
/// This differs from `str::is_uppercase` in that it returns `false` for empty strings and strings
/// that contain only underscores or other non-alphabetic characters.
///
/// ## Examples
///
/// ```rust
/// use ruff_python_stdlib::str::is_cased_uppercase;
///
/// assert!(is_cased_uppercase("ABC"));
/// assert!(is_cased_uppercase("A_B_C"));
/// assert!(is_cased_uppercase("A2C"));
/// assert!(!is_cased_uppercase("aBc"));
/// assert!(!is_cased_uppercase("abc"));
/// assert!(!is_cased_uppercase(""));
/// assert!(!is_cased_uppercase("_"));
/// ```
pub fn is_cased_uppercase(s: &str) -> bool {
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
