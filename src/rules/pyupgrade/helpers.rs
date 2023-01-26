use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rustpython_ast::{AliasData, Located};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::source_code::Locator;

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

/// Converts a list of names and a module into a from import string. I do not
/// love this and would MUCH rather use libcst, so if you have any better ideas
/// feel free to let me know.
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
        format!("\n{short_indent})")
    } else {
        String::new()
    };
    let mut full_names: Vec<String> = vec![];
    for name in names {
        let asname_str = match &name.asname {
            Some(item) => format!(" as {item}"),
            None => String::new(),
        };
        let final_string = format!("{indent}{}{asname_str}", name.name);
        full_names.push(final_string);
    }
    format!(
        "from {} import {}{}{}",
        module,
        start_imps,
        full_names.join(format!(",{after_comma}").as_str()),
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

pub struct ImportFormatting {
    pub multi_line: bool,
    pub indent: String,
    pub short_indent: String,
    pub start_indent: String,
}

impl ImportFormatting {
    pub fn new<T>(locator: &Locator, stmt: &Located<T>, args: &[Located<AliasData>]) -> Self {
        let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
        let multi_line = module_text.contains('\n');
        let start_indent = clean_indent(locator, stmt);
        let indent = match args.get(0) {
            None => panic!("No args for import"),
            Some(item) if multi_line => clean_indent(locator, item),
            Some(_) => String::new(),
        };
        let short_indent = if indent.len() > 3 {
            indent[3..].to_string()
        } else {
            indent.to_string()
        };
        Self {
            multi_line,
            indent,
            short_indent,
            start_indent,
        }
    }
}
