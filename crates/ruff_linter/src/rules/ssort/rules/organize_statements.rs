use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::{Checker, LintContext};
use crate::rules::ssort::dependencies::{Dependencies, Node};
use crate::{FixAvailability, Locator, Violation};

/// ## What it does
///
/// Groups and sorts statements based on the order in which they are referenced.
///
/// ## Why is this bad?
///
/// Consistency is good. Use a common convention for statement ordering to make your code more
/// readable and idiomatic.
///
/// ## Example
///
/// ```python
/// def foo():
///     bar()
///
///
/// def baz():
///     pass
///
///
/// def bar():
///     baz()
/// ```
///
/// Use instead:
///
/// ```python
/// def baz():
///     pass
///
///
/// def bar():
///     baz()
///
///
/// def foo():
///     bar()
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct UnsortedStatements;

/// Allows UnsortedStatements to be treated as a Violation.
impl Violation for UnsortedStatements {
    /// Fix is sometimes available.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    /// The message used to describe the violation.
    ///
    /// ## Returns
    /// A string describing the violation.
    #[derive_message_formats]
    fn message(&self) -> String {
        "Statements are unsorted".to_string()
    }

    /// Returns the title for the fix.
    ///
    /// ## Returns
    /// A string describing the fix title.
    fn fix_title(&self) -> Option<String> {
        Some("Organize statements".to_string())
    }
}

/// Sort a suite of statements based on their reference order.
///
/// ## Arguments
/// * `suite` - The suite of statements to organize.
/// * `context` - The linting context.
/// * `locator` - The locator for the source code.
fn organize_suite(suite: &[Stmt], context: &LintContext, locator: &Locator) {
    // Collect all statements marking only functions and classes as movable
    let nodes: Vec<Node> = suite
        .iter()
        .enumerate()
        .map(|(index, stmt)| {
            let name = match stmt {
                Stmt::FunctionDef(def) => Some(&def.name.id),
                Stmt::ClassDef(def) => Some(&def.name.id),
                _ => None,
            };
            Node {
                index,
                name,
                stmt,
                movable: name.is_some(),
            }
        })
        .collect();

    // Return early if there are fewer than 2 movable nodes
    if nodes.iter().filter(|node| node.movable).count() < 2 {
        return;
    }

    // Check if the order has changed compared to the original order
    let dependencies = Dependencies::from_nodes(&nodes);
    let sorted = match dependencies.dependency_order(&nodes) {
        Ok(sorted) => sorted,
        Err(_) => return,
    };
    if sorted
        .iter()
        .enumerate()
        .all(|(i, &idx)| idx == nodes[i].index)
    {
        return;
    }

    // Build a replacement string from the sorted nodes
    let mut replacement = String::new();
    for (i, &node_idx) in sorted.iter().enumerate() {
        replacement.push_str(locator.slice(suite[node_idx].range()));

        // Add two newlines between statements but not at the end
        if i < nodes.len() - 1 {
            replacement.push_str("\n\n");
        }
    }

    // Report the violation and suggest a fix
    let start = suite.first().unwrap().range().start();
    let end = suite.last().unwrap().range().end();
    let mut diagnostic = context.report_diagnostic(UnsortedStatements, TextRange::new(start, end));
    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(replacement, start, end)));
}

/// SS001
pub(crate) fn organize_statements(checker: &Checker, suite: &[Stmt]) {
    organize_suite(suite, &checker.context(), checker.locator());
}
