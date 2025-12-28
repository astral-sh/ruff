use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::{Checker, LintContext};
use crate::rules::ssort::dependencies::{Dependencies, nodes_from_suite};
use crate::{FixAvailability, Violation};

/// ## What it does
///
/// Detects cycles in function and method calls within a module.
///
/// A cycle occurs when a function directly or indirectly calls itself through
/// one or more other functions (for example, `a() -> b() -> c() -> a()`).
///
/// ## Why is this bad?
///
/// Function call cycles make control flow harder to understand and reason about.
/// They can:
///
/// - Obscure program logic
/// - Make debugging more difficult
/// - Lead to unbounded recursion and runtime errors
///
/// In most cases, call cycles indicate a design issue and can be avoided by
/// restructuring the code or extracting shared logic into a separate function.
///
/// ## Example
/// ```python
/// def a():
///     b()
///
/// def b():
///     c()
///
/// def c():
///     a()
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.14.11")]
pub(crate) struct FunctionCallCycle {
    /// The detected cycle in function/method calls.
    pub cycle: String,
}

/// Allows FunctionCallCycle to be treated as a Violation.
impl Violation for FunctionCallCycle {
    /// Fix is not available.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    /// The message used to describe the violation.
    ///
    /// ## Returns
    /// A string describing the violation.
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function call cycle detected: {}", self.cycle)
    }
}

/// Recursively detect cycles in function/method calls within a suite of statements.
///
/// ## Arguments
/// * `suite` - The suite of statements to analyze.
/// * `context` - The linting context for reporting diagnostics.
fn detect_cycle_in_suite(suite: &[Stmt], context: &LintContext) {
    // Try to resolve the dependency order, if it errors, we have a cycle
    let nodes = nodes_from_suite(suite);
    let dependencies = Dependencies::from_nodes(&nodes);
    if let Some(cycle_error) = dependencies.dependency_order(&nodes, false).err() {
        // Close the cycle and build the display names
        let function_cycle = cycle_error
            .cycle
            .iter()
            .copied()
            .chain(std::iter::once(cycle_error.cycle[0]))
            .filter_map(|idx| nodes[idx].name.as_ref().map(|n| format!("{n}()")))
            .collect::<Vec<_>>();

        // Report the diagnostic on the first node in the cycle
        context.report_diagnostic(
            FunctionCallCycle {
                cycle: function_cycle.join(" -> "),
            },
            nodes[cycle_error.cycle[0]].stmt.range(),
        );
    }

    // Recurse into classes
    for node in &nodes {
        if let Stmt::ClassDef(class_def) = &node.stmt {
            detect_cycle_in_suite(&class_def.body, context);
        }
    }
}

/// SS002
pub(crate) fn detect_function_cycle(checker: &Checker, suite: &[Stmt]) {
    detect_cycle_in_suite(suite, checker.context());
}
