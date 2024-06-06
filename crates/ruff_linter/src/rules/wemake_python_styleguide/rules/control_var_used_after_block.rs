use std::fmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Stmt;
use ruff_python_semantic::{BindingId, NodeId, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables defined in `for` loops and `with` statements that are used
/// outside of their respective blocks.
///
/// ## Why is this bad?
/// Usage of of a control variable outside of the block they're defined in will probably
/// lead to flawed logic in a way that will likely cause bugs. The variable might not
/// contain what you expect.
///
/// In Python, unlike many other languages, `for` loops and `with` statements don't
/// define their own scopes. Therefore, usage of the control variables outside of the
/// block will be the the value from the last iteration until re-assigned.
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
///
/// with path.open() as f:
///     pass
///
/// print(f.readline())  # prints a line from a file that is already closed (error)
/// ```
#[violation]
pub struct ControlVarUsedAfterBlock {
    control_var_name: String,
    block_kind: BlockKind,
}

impl Violation for ControlVarUsedAfterBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ControlVarUsedAfterBlock {
            control_var_name,
            block_kind,
        } = self;

        format!("{block_kind} variable {control_var_name} is used outside of block")
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum BlockKind {
    For,
    With,
}

impl fmt::Display for BlockKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlockKind::For => fmt.write_str("`for` loop"),
            BlockKind::With => fmt.write_str("`with` statement"),
        }
    }
}

/// WPS441: Forbid control variables after the block body.
pub(crate) fn control_var_used_after_block(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Keep track of node_ids we know are used in the same block. This helps us when
    // people use the same variable name in multiple blocks.
    let mut known_good_reference_node_ids: Vec<NodeId> = Vec::new();

    let all_bindings: Vec<(&str, BindingId)> = scope.all_bindings().collect();
    // We want to reverse the bindings so that we iterate in source order and shadowed
    // bindings come first.
    let reversed_bindings = all_bindings.iter().rev();

    // Find for-loop and with-statement variable bindings
    for (&name, binding) in reversed_bindings
        .map(|(name, binding_id)| (name, checker.semantic().binding(*binding_id)))
        .filter_map(|(name, binding)| {
            if binding.kind.is_loop_var() || binding.kind.is_with_item_var() {
                return Some((name, binding));
            }

            None
        })
    {
        let binding_statement = binding.statement(checker.semantic()).unwrap();
        let binding_source_node_id = binding.source.unwrap();
        // The node_id of the for-loop that contains the binding
        let binding_statement_id = checker.semantic().statement_id(binding_source_node_id);

        // Loop over the references of those bindings to see if they're in the same block-scope
        for reference in binding.references() {
            let reference = checker.semantic().reference(reference);
            let reference_node_id = reference.expression_id().unwrap();

            // Traverse the hierarchy and look for a block match
            let statement_hierarchy: Vec<NodeId> = checker
                .semantic()
                .parent_statement_ids(reference_node_id)
                .collect();

            let mut is_used_in_block = false;
            for ancestor_node_id in statement_hierarchy {
                if binding_statement_id == ancestor_node_id {
                    is_used_in_block = true;
                    known_good_reference_node_ids.push(reference_node_id);
                    break;
                }
            }

            // If the reference wasn't used in the same block, report a violation/diagnostic
            if !is_used_in_block && !known_good_reference_node_ids.contains(&reference_node_id) {
                let block_kind = match binding_statement {
                    Stmt::For(_) => BlockKind::For,
                    Stmt::With(_) => BlockKind::With,
                    _ => {
                        panic!("Unexpected block item. This is a problem with ruff itself. Fix the `filter_map` above.")
                    }
                };

                diagnostics.push(Diagnostic::new(
                    ControlVarUsedAfterBlock {
                        control_var_name: name.to_owned(),
                        block_kind,
                    },
                    reference.range(),
                ));
            }
        }
    }
}
