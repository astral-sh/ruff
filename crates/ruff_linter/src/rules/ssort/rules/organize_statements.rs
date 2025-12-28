use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
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
///
/// ## Limitations
///
/// This rule will not sort bodies containing circular dependencies, as they cannot be
/// reordered while preserving the dependency relationships. However, nested bodies within
/// those statements may still be sorted independently.
///
/// For example, the top-level functions `a` and `b` will not be sorted due to their circular
/// dependency, but the class `C` will have its methods sorted:
///
/// ```python
/// def a():
///     return b()
///
///
/// def b():
///     return a()
///
///
/// class C:
///     def method_b(self):
///         return self.method_a()
///
///     def method_a(self):
///         pass
/// ```
///
/// After applying this rule:
///
/// ```python
/// def a():
///     return b()
///
///
/// def b():
///     return a()
///
///
/// class C:
///     def method_a(self):
///         pass
///
///     def method_b(self):
///         return self.method_a()
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

/// Get the replacement text for a statement, organizing its body if it is a class definition.
///
/// ## Arguments
/// * `locator` - The locator for the source code.
/// * `stmt` - The statement to get the replacement text for.
///
/// ## Returns
/// A string representing the replacement text for the statement.
fn get_replacement_text(locator: &Locator, stmt: &Stmt) -> String {
    // If the statement is a class, get its definition, otherwise, return the original text
    let original_text = locator.slice(stmt.range()).to_string();
    let class_def = match stmt {
        Stmt::ClassDef(c) => c,
        _ => return original_text,
    };

    // Construct the replacement text by sorting the class body
    let inner_replacement = organize_suite(&class_def.body, locator);

    // Compute the range of the body text and insert the replacement text between them
    let body_range = TextRange::new(
        class_def.body.first().unwrap().range().start(),
        class_def.body.last().unwrap().range().end(),
    );
    format!(
        "{}{}{}",
        locator.slice(TextRange::new(stmt.range().start(), body_range.start())),
        inner_replacement,
        locator.slice(TextRange::new(body_range.end(), stmt.range().end()))
    )
}

/// Get the separator text following a statement in the suite.
///
/// ## Arguments
/// * `suite` - The suite of statements.
/// * `locator` - The locator for the source code.
/// * `stmt_idx` - The index of the statement in the suite.
///
/// ## Returns
/// A string representing the separator text following the statement.
fn separator_for(suite: &[Stmt], locator: &Locator, stmt_idx: usize) -> String {
    if let Some(next) = suite.get(stmt_idx + 1) {
        // Preserve the exact text between this statement and the next
        let range = TextRange::new(suite[stmt_idx].range().end(), next.range().start());
        locator.slice(range).to_string()
    } else if suite.len() > 1 {
        // Reuse the first separator as the trailing text after the last statement may include EOF
        // newlines or comments
        let range = TextRange::new(suite[0].range().end(), suite[1].range().start());
        locator.slice(range).to_string()
    } else {
        // Use a two-newline separator since we have a single statement suite
        "\n\n".to_string()
    }
}

/// Sort a suite of statements based on their reference order.
///
/// ## Arguments
/// * `suite` - The suite of statements to organize.
/// * `locator` - The locator for the source code.
///
/// ## Returns
/// A string representing the organized suite of statements.
fn organize_suite(suite: &[Stmt], locator: &Locator) -> String {
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

    // Get the dependency order of the nodes
    let dependencies = Dependencies::from_nodes(&nodes);
    let sorted = dependencies
        .dependency_order(&nodes)
        .unwrap_or_else(|_| (0..nodes.len()).collect());

    // Build a replacement string by reordering statements while preserving formatting
    let mut replacement = String::new();
    for (i, &node_idx) in sorted.iter().enumerate() {
        // Append the statement text
        let suite_text = get_replacement_text(locator, &nodes[node_idx].stmt);
        replacement.push_str(&suite_text);

        // Append the separator unless this is the last sorted node
        if i < sorted.len() - 1 {
            replacement.push_str(&separator_for(suite, locator, node_idx));
        }
    }
    replacement
}

/// SS001
pub(crate) fn organize_statements(checker: &Checker, suite: &[Stmt]) {
    // Build the replacement string recursively by sorting the statements in the suite
    let replacement = organize_suite(suite, checker.locator());

    // Only report a diagnostic if the replacement is different
    let range = TextRange::new(
        suite.first().unwrap().range().start(),
        suite.last().unwrap().range().end(),
    );
    let original_text = checker.locator().slice(range);
    if replacement != original_text {
        checker
            .report_diagnostic(UnsortedStatements, range)
            .set_fix(Fix::safe_edit(Edit::replacement(
                replacement,
                range.start(),
                range.end(),
            )));
    }
}
