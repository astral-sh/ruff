use ruff_python_ast::call_path::from_qualified_name;
use std::collections::BTreeSet;

use ruff_python_ast::cast;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::str::is_implicit_concatenation;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};

/// Return the index of the first logical line in a string.
pub(crate) fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in content.universal_newlines().enumerate() {
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
pub(crate) fn normalize_word(first_word: &str) -> String {
    first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
}

/// Check decorator list to see if function should be ignored.
pub(crate) fn should_ignore_definition(
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
                    .any(|decorator| from_qualified_name(decorator) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Return true if a line ends with an odd number of backslashes (i.e., ends with an escape).
pub(crate) fn ends_with_backslash(line: &str) -> bool {
    line.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// Check if a docstring should be ignored.
pub(crate) fn should_ignore_docstring(contents: &str) -> bool {
    // Avoid analyzing docstrings that contain implicit string concatenations.
    // Python does consider these docstrings, but they're almost certainly a
    // user error, and supporting them "properly" is extremely difficult.
    is_implicit_concatenation(contents)
}
