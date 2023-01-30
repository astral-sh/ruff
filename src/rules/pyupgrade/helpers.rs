use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rustpython_ast::{Alias, AliasData, Located};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::source_code::{Locator, Stylist};

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

/// Converts a list of names and a module into an `import from`-style import.
pub fn format_import_from(
    names: &[AliasData],
    module: &str,
    multi_line: bool,
    indent: &str,
    start_indent: &str,
    stylist: &Stylist,
) -> String {
    // Construct the whitespace strings.
    let line_ending = stylist.line_ending().as_str();
    let after_comma = if multi_line { line_ending } else { " " };
    let before_imports = if multi_line {
        format!("({line_ending}")
    } else {
        String::new()
    };
    let after_imports = if multi_line {
        format!("{line_ending}{start_indent})")
    } else {
        String::new()
    };

    // Generate the formatted names.
    let full_names = {
        let full_names: Vec<String> = names
            .iter()
            .map(|name| match &name.asname {
                Some(asname) => format!("{indent}{} as {asname}", name.name),
                None => format!("{indent}{}", name.name),
            })
            .collect();
        let mut full_names = full_names.join(&format!(",{}", after_comma));
        if multi_line {
            full_names.push(',');
        }
        full_names
    };

    format!(
        "from {} import {}{}{}",
        module, before_imports, full_names, after_imports
    )
}

pub struct ImportFormatting<'a> {
    /// Whether the import is multi-line.
    pub multi_line: bool,
    /// Whether the import is on its own line.
    pub own_line: bool,
    /// The indentation of the members of the import.
    pub member_indent: &'a str,
    /// The indentation of the import statement.
    pub stmt_indent: &'a str,
}

impl<'a> ImportFormatting<'a> {
    pub fn new<T>(
        locator: &'a Locator<'a>,
        stylist: &'a Stylist<'a>,
        stmt: &'a Located<T>,
        args: &'a [Alias],
    ) -> Self {
        let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
        let multi_line = module_text.contains(stylist.line_ending().as_str());
        let own_line = indentation(locator, stmt).is_some();
        let stmt_indent =
            indentation(locator, stmt).unwrap_or_else(|| stylist.indentation().as_str());
        let member_indent = if multi_line {
            indentation(locator, &args[0]).unwrap_or_else(|| stylist.indentation().as_str())
        } else {
            ""
        };
        Self {
            multi_line,
            own_line,
            member_indent,
            stmt_indent,
        }
    }
}
