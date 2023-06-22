use std::collections::BTreeSet;

use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::cast;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::str::is_implicit_concatenation;
use ruff_python_semantic::{Definition, Member, MemberKind, SemanticModel};
use ruff_python_whitespace::UniversalNewlines;

/// Return the index of the first logical line in a string.
pub(super) fn logical_line(content: &str) -> Option<usize> {
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
pub(super) fn normalize_word(first_word: &str) -> String {
    first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
}

/// Return true if a line ends with an odd number of backslashes (i.e., ends with an escape).
pub(super) fn ends_with_backslash(line: &str) -> bool {
    line.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// Check decorator list to see if function should be ignored.
pub(crate) fn should_ignore_definition(
    definition: &Definition,
    ignore_decorators: &BTreeSet<String>,
    semantic: &SemanticModel,
) -> bool {
    if ignore_decorators.is_empty() {
        return false;
    }

    if let Definition::Member(Member {
        kind: MemberKind::Function | MemberKind::NestedFunction | MemberKind::Method,
        stmt,
        ..
    }) = definition
    {
        for decorator in cast::decorator_list(stmt) {
            if let Some(call_path) = semantic.resolve_call_path(map_callable(&decorator.expression))
            {
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

/// Check if a docstring should be ignored.
pub(crate) fn should_ignore_docstring(contents: &str) -> bool {
    // Avoid analyzing docstrings that contain implicit string concatenations.
    // Python does consider these docstrings, but they're almost certainly a
    // user error, and supporting them "properly" is extremely difficult.
    is_implicit_concatenation(contents)
}
