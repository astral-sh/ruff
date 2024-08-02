use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{BindingId, NodeId, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables defined in `for` loops that are used outside of their
/// respective blocks.
///
/// ## Why is this bad?
/// Usage of of a control variable outside of the block they're defined in will probably
/// lead to flawed logic in a way that will likely cause bugs. The variable might not
/// contain what you expect.
///
/// In Python, unlike many other languages, `for` loops don't define their own scopes.
/// Therefore, usage of the control variables outside of the block will be the the value
/// from the last iteration until re-assigned.
///
/// While this mistake is easy to spot in small examples, it can be hidden in larger
/// blocks of code, where the the loop and downstream usage may not be visible at the
/// same time.
///
/// ## Example
/// ```python
/// for x in range(10):
///     pass
///
/// print(x)  # prints 9
/// ```
#[violation]
pub struct ForVariableUsedAfterBlock {
    control_var_name: String,
}

impl Violation for ForVariableUsedAfterBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ForVariableUsedAfterBlock { control_var_name } = self;

        format!("`for` loop variable {control_var_name} is used outside of block")
    }
}

/// Based on wemake-python-styleguide (WPS441) to forbid control variables after the block body.
pub(crate) fn for_variable_used_after_block(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Keep track of the node_ids of variable references that have already been
    // accounted for as part of a different variable binding. This helps us avoid
    // complaining when people use the same variable name in multiple blocks under the
    // same scope.
    let mut known_good_reference_node_ids: Vec<NodeId> = Vec::new();

    let all_bindings: Vec<(&str, BindingId)> = scope.all_bindings().collect();
    // We want to reverse the bindings so that we iterate in source order and shadowed
    // bindings come first. This way we can gather `known_good_reference_node_ids` and
    // mark off things as we go which will allow us to ignore later bindings trying to
    // reference the same variable.
    let reversed_bindings = all_bindings.iter().rev();

    // Find for-loop variable bindings
    let loop_var_bindings = reversed_bindings
        .map(|(name, binding_id)| (name, checker.semantic().binding(*binding_id)))
        .filter_map(|(name, binding)| binding.kind.is_loop_var().then_some((name, binding)));

    for (&name, binding) in loop_var_bindings {
        let binding_statement = binding.statement(checker.semantic()).unwrap();
        let binding_source_node_id = binding.source.unwrap();
        // The node_id of the for-loop that contains the binding
        let binding_statement_id = checker.semantic().statement_id(binding_source_node_id);

        // Loop over the references of those bindings to see if they're in the same block-scope
        'references: for reference in binding.references() {
            let reference = checker.semantic().reference(reference);
            let reference_node_id = reference.expression_id().unwrap();

            // Skip any reference that come before the control var binding in the source
            // order, skip it because people can assign and use the same variable name
            // above the block.
            if reference.range().end() < binding_statement.range().start() {
                continue;
            }

            // Traverse the hierarchy and look for a block match
            let statement_hierarchy = checker.semantic().parent_statement_ids(reference_node_id);

            for ancestor_node_id in statement_hierarchy {
                if binding_statement_id == ancestor_node_id {
                    known_good_reference_node_ids.push(reference_node_id);
                    continue 'references;
                }
            }

            // If the reference wasn't used in the same block, report a violation/diagnostic
            if !known_good_reference_node_ids.contains(&reference_node_id) {
                diagnostics.push(Diagnostic::new(
                    ForVariableUsedAfterBlock {
                        control_var_name: name.to_owned(),
                    },
                    reference.range(),
                ));
            }
        }
    }
}
