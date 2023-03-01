use std::collections::BTreeSet;

use ruff_python::str::{
    SINGLE_QUOTE_PREFIXES, SINGLE_QUOTE_SUFFIXES, TRIPLE_QUOTE_PREFIXES, TRIPLE_QUOTE_SUFFIXES,
};

use crate::ast::cast;
use crate::ast::helpers::{map_callable, to_call_path};
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};

/// Strip the leading and trailing quotes from a docstring.
pub fn raw_contents(contents: &str) -> &str {
    for pattern in TRIPLE_QUOTE_PREFIXES {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 3];
        }
    }
    for pattern in SINGLE_QUOTE_PREFIXES {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 1];
        }
    }
    unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
}

/// Return the leading quote string for a docstring (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    if let Some(first_line) = content.lines().next() {
        for pattern in TRIPLE_QUOTE_PREFIXES.iter().chain(SINGLE_QUOTE_PREFIXES) {
            if first_line.starts_with(pattern) {
                return Some(pattern);
            }
        }
    }
    None
}

/// Return the trailing quote string for a docstring (e.g., `"""`).
pub fn trailing_quote(content: &str) -> Option<&&str> {
    TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}

/// Return the index of the first logical line in a string.
pub fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            // Empty line. If this is the line _after_ the first logical line, stop.
            if logical_line.is_some() {
                break;
            }
        } else {
            // Non-empty line. Store the index.
            logical_line = Some(i);
        }
    }
    logical_line
}

/// Normalize a word by removing all non-alphanumeric characters
/// and converting it to lowercase.
pub fn normalize_word(first_word: &str) -> String {
    first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
}

/// Check decorator list to see if function should be ignored.
pub fn should_ignore_definition(
    checker: &Checker,
    definition: &Definition,
    ignore_decorators: &BTreeSet<String>,
) -> bool {
    if ignore_decorators.is_empty() {
        return false;
    }

    if let DefinitionKind::Function(parent)
    | DefinitionKind::NestedFunction(parent)
    | DefinitionKind::Method(parent) = definition.kind
    {
        for decorator in cast::decorator_list(parent) {
            if let Some(call_path) = checker.resolve_call_path(map_callable(decorator)) {
                if ignore_decorators
                    .iter()
                    .any(|decorator| to_call_path(decorator) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}
