use once_cell::sync::Lazy;
use regex::{Captures, Regex};

static CURLY_BRACES: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\\N\{[^}]+})|([{}])").unwrap());

pub fn curly_escape(text: &str) -> String {
    // Match all curly braces. This will include named unicode escapes (like
    // \N{SNOWMAN}), which we _don't_ want to escape, so take care to preserve them.
    CURLY_BRACES
        .replace_all(text, |caps: &Captures| {
            if let Some(match_) = caps.get(1) {
                match_.as_str().to_string()
            } else {
                if &caps[2] == "{" {
                    "{{".to_string()
                } else {
                    "}}".to_string()
                }
            }
        })
        .to_string()
}
