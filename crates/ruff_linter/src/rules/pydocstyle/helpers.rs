use std::collections::BTreeSet;

use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::QualifiedName;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_source_file::UniversalNewlines;

/// Return the index of the first logical line in a string.
pub(super) fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in content.universal_newlines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.chars().all(|c| matches!(c, '-' | '~' | '=' | '#')) {
            // Empty line, or underline. If this is the line _after_ the first logical line, stop.
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

    let Some(function) = definition.as_function_def() else {
        return false;
    };

    function.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                ignore_decorators
                    .iter()
                    .any(|decorator| QualifiedName::from_dotted_name(decorator) == qualified_name)
            })
    })
}
