use std::collections::BTreeSet;

use ruff_python_ast::cast;
use ruff_python_ast::helpers::{map_callable, to_call_path};
use ruff_python_ast::whitespace::UniversalNewlineIterator;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};

/// Return the index of the first logical line in a string.
pub fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in UniversalNewlineIterator::from(content).enumerate() {
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
            if let Some(call_path) = checker.ctx.resolve_call_path(map_callable(decorator)) {
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
