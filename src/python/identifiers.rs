use once_cell::sync::Lazy;
use regex::Regex;

pub static IDENTIFIER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());
