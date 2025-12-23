use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::{Checker, LintContext};
use crate::rules::ssort::graph::{Node, build_dependency_graph, topological_sort};
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
    // Collect all function and class definitions into a nodes vector
    let nodes: Vec<Node> = suite
        .iter()
        .enumerate()
        .filter_map(|(idx, stmt)| {
            match stmt {
                Stmt::FunctionDef(def) => Some(Node {
                    index: idx,
                    name: &def.name,
                    stmt,
                }),
                // TODO: Support sorting inside classes
                Stmt::ClassDef(def) => Some(Node {
                    index: idx,
                    name: &def.name,
                    stmt,
                }),
                _ => None,
            }
        })
        .collect();

    // Return early if there won't be any changes
    if nodes.len() < 2 {
        return;
    }

    // Build a dependency graph and sort the nodes topologically to find the correct order
    let edges = build_dependency_graph(&nodes);
    let Ok(sorted) = topological_sort(&nodes, &edges) else {
        return;
    };

    // Check if the order has changed compared to the original order
    if sorted
        .iter()
        .enumerate()
        .all(|(i, &idx)| idx == nodes[i].index)
    {
        return;
    }

    // Build a replacement string from the sorted statements
    let mut replacement = String::new();
    for (i, &node_idx) in sorted.iter().enumerate() {
        replacement.push_str(locator.slice(suite[node_idx].range()));

        // Add two newlines between statements but not at the end
        if i < sorted.len() - 1 {
            replacement.push_str("\n\n");
        }
    }

    // Report the violation and suggest a fix
    let start = suite[nodes.first().unwrap().index].range().start();
    let end = suite[nodes.last().unwrap().index].range().end();
    let mut diagnostic = context.report_diagnostic(UnsortedStatements, TextRange::new(start, end));
    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(replacement, start, end)));
}

/// SS001
pub(crate) fn organize_statements(checker: &Checker, suite: &[Stmt]) {
    organize_suite(suite, &checker.context(), checker.locator());
}
