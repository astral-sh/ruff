use rustpython_ast::{AliasData, Located};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use crate::source_code::Locator;
use crate::ast::whitespace::indentation;

static CURLY_ESCAPE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\\N\{[^}]+})|([{}])").unwrap());

pub fn curly_escape(text: &str) -> String {
    // We don't support emojis right now.
    CURLY_ESCAPE
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

/// Converts a list of names and a module into a from import string. I do not love this and would
/// MUCH rather use libcst, so if you have any better ideas feel free to let me know.
pub fn get_fromimport_str(
    names: &[AliasData],
    module: &str,
    multi_line: bool,
    indent: &str,
    short_indent: &str,
) -> String {
    if names.is_empty() {
        return String::new();
    }
    let after_comma = if multi_line { '\n' } else { ' ' };
    let start_imps = if multi_line { "(\n" } else { "" };
    let after_imps = if multi_line {
        format!("\n{})", short_indent)
    } else {
        String::new()
    };
    let mut full_names: Vec<String> = vec![];
    for name in names {
        let asname_str = match &name.asname {
            Some(item) => format!(" as {}", item),
            None => String::new(),
        };
        let final_string = format!("{}{}{}", indent, name.name, asname_str);
        full_names.push(final_string);
    }
    format!(
        "from {} import {}{}{}",
        module,
        start_imps,
        full_names.join(format!(",{}", after_comma).as_str()),
        after_imps
    )
}

pub fn clean_indent<T>(locator: &Locator, located: &Located<T>) -> String {
    match indentation(locator, located) {
        // This is an opninionated way of formatting import statements
        None => "    ".to_string(),
        Some(item) => item.to_string(),
    }
}
