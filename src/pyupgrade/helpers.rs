use once_cell::sync::Lazy;
use regex::Regex;

static NAMED_UNICODE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<!\\)(?:\\\\)*(\\N\{[^}]+\})").unwrap());

/// Equivalent to the python regex fullmatch: https://docs.python.org/3/library/re.html
fn full_match(regex: Lazy<Regex>, string: &str) -> bool {
    match regex.find(string) {
        None => false,
        Some(item) => item.as_str() == string,
    }
}

pub fn curly_escape(string: &str) -> String {
    let final_str = String::new();
    let parts = NAMED_UNICODE_RE.split(string);
    for part in parts {
        if full_match(NAMED_UNICODE_RE, part) {
            final_str.push_str(part);
        } else {
            final_str.push_str(&part.replace("{", "{{").replace("}", "}}"));
        }
    }
    final_str
}
