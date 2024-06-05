use std::{fmt, iter};

use regex::Regex;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Stmt, WithItem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_semantic::{Binding, NodeId, Scope, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables defined in `for` loops and `with` statements that
/// get overwritten within the body, for example by another `for` loop or
/// `with` statement or by direct assignment.
///
/// ## Why is this bad?
/// Redefinition of a loop variable inside the loop's body causes its value
/// to differ from the original loop iteration for the remainder of the
/// block, in a way that will likely cause bugs.
///
/// In Python, unlike many other languages, `for` loops and `with`
/// statements don't define their own scopes. Therefore, a nested loop that
/// uses the same target variable name as an outer loop will reuse the same
/// actual variable, and the value from the last iteration will "leak out"
/// into the remainder of the enclosing loop.
///
/// While this mistake is easy to spot in small examples, it can be hidden
/// in larger blocks of code, where the definition and redefinition of the
/// variable may not be visible at the same time.
///
/// ## Example
/// ```python
/// for i in range(10):
///     i = 9
///     print(i)  # prints 9 every iteration
///
/// for i in range(10):
///     for i in range(10):  # original value overwritten
///         pass
///     print(i)  # also prints 9 every iteration
///
/// with path1.open() as f:
///     with path2.open() as f:
///         f = path2.open()
///     print(f.readline())  # prints a line from path2
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
        // Prefix the nouns describing the outer and inner kinds with "outer" and "inner"
        // to better distinguish them, but to avoid confusion, only do so if the outer and inner
        // kinds are equal. For example, instead of:
        //
        //    "Outer `for` loop variable `i` overwritten by inner assignment target."
        //
        // We have:
        //
        //    "`for` loop variable `i` overwritten by assignment target."
        //
        // While at the same time, we have:
        //
        //    "Outer `for` loop variable `i` overwritten by inner `for` loop target."
        //    "Outer `with` statement variable `f` overwritten by inner `with` statement target."

        format!(
            "{block_kind} variable {control_var_name} from {block_kind} is used outside of block"
        )
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
    println!("Running control_var_used_after_block");
    // if scope.uses_locals() && scope.kind.is_function() {
    //     return;
    // }

    // println!("nodes {:#?}", checker.semantic().nodes());

    for (name, binding) in scope
        .bindings()
        .map(|(name, binding_id)| (name, checker.semantic().binding(binding_id)))
        .filter_map(|(name, binding)| {
            println!("name={:?} kind={:?}", name, binding.kind);

            if binding.kind.is_loop_var() || binding.kind.is_with_item_var() {
                return Some((name, binding));
            }

            None
        })
    {
        println!("Binding {:?} {:?}", name, binding);
        // Find for-loop variable bindings
        let binding_statement = binding.statement(checker.semantic()).unwrap();
        let binding_source_node_id = binding.source.unwrap();
        // The node_id of the for-loop that contains the binding
        // let binding_statement_id = checker
        //     .semantic()
        //     .parent_statement_id(binding_source_node_id)
        //     .unwrap();
        let binding_statement_id = checker.semantic().statement_id(binding_source_node_id);

        println!("binding_statement={:?}", binding_statement);
        println!("Binding references {:?}", binding.references);

        // Loop over the references of those bindings to see if they're in the same block-scope
        for reference in binding.references() {
            let reference = checker.semantic().reference(reference);
            let reference_node_id = reference.expression_id().unwrap();

            // Traverse the hierarchy and look for a block match
            let statement_hierarchy: Vec<NodeId> = checker
                .semantic()
                .parent_statement_ids(reference_node_id)
                .collect();

            println!(
                "\t\x1b[32mreference\x1b[0m {:?} <- {:?}",
                reference, statement_hierarchy
            );

            let mut found_match = false;
            for ancestor_node_id in statement_hierarchy {
                let ancestor_statement = checker.semantic().statement(ancestor_node_id);
                println!("\t\tancestor_statement={:?}", ancestor_statement);
                println!(
                    "\t\tbinding_source_node_id={:?} binding_statement_id={:?} ancestor_node_id={:?}",
                    binding_source_node_id, binding_statement_id, ancestor_node_id
                );
                if binding_statement_id == ancestor_node_id {
                    println!("\t\to");
                    found_match = true;
                    break;
                } else {
                    println!("\t\tx");
                }
            }

            if !found_match {
                println!("\t\temits error={:?}", reference_node_id);
                let block_kind = match binding_statement {
                    Stmt::For(_) => BlockKind::For,
                    Stmt::With(_) => BlockKind::With,
                    _ => {
                        panic!("unexpected block item")
                    }
                };

                diagnostics.push(Diagnostic::new(
                    ControlVarUsedAfterBlock {
                        control_var_name: name.to_owned(),
                        block_kind,
                    },
                    reference.range(),
                ))
            }

            // TODO: Look if the reference is under the same block as the binding
            // (see `too_many_nested_blocks` for an example of how to work with blocks)
        }
    }
}
