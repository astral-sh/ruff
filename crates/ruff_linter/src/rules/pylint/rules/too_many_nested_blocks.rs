use ast::ExceptHandler;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions or methods with too many nested blocks.
///
/// By default, this rule allows up to five nested blocks.
/// This can be configured using the [`lint.pylint.max-nested-blocks`] option.
///
/// ## Why is this bad?
/// Functions or methods with too many nested blocks are harder to understand
/// and maintain.
///
/// ## Options
/// - `lint.pylint.max-nested-blocks`
#[violation]
pub struct TooManyNestedBlocks {
    nested_blocks: usize,
    max_nested_blocks: usize,
}

impl Violation for TooManyNestedBlocks {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyNestedBlocks {
            nested_blocks,
            max_nested_blocks,
        } = self;
        format!("Too many nested blocks ({nested_blocks} > {max_nested_blocks})")
    }
}

/// PLR1702
pub(crate) fn too_many_nested_blocks(checker: &mut Checker, stmt: &Stmt) {
    // Only enforce nesting within functions or methods.
    if !checker.semantic().current_scope().kind.is_function() {
        return;
    }

    // If the statement isn't a leaf node, we don't want to emit a diagnostic, since the diagnostic
    // will be emitted on the leaves.
    if has_nested_block(stmt) {
        return;
    }

    let max_nested_blocks = checker.settings.pylint.max_nested_blocks;

    // Traverse up the hierarchy, identifying the root node and counting the number of nested
    // blocks between the root and this leaf.
    let (count, root_id) =
        checker
            .semantic()
            .current_statement_ids()
            .fold((0, None), |(count, ancestor_id), id| {
                let stmt = checker.semantic().statement(id);
                if is_nested_block(stmt) {
                    (count + 1, Some(id))
                } else {
                    (count, ancestor_id)
                }
            });

    let Some(root_id) = root_id else {
        return;
    };

    // If the number of nested blocks is less than the maximum, we don't want to emit a diagnostic.
    if count <= max_nested_blocks {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        TooManyNestedBlocks {
            nested_blocks: count,
            max_nested_blocks,
        },
        checker.semantic().statement(root_id).range(),
    ));
}

/// Returns `true` if the given statement is a nested block.
fn is_nested_block(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::If(_) | Stmt::While(_) | Stmt::For(_) | Stmt::Try(_) | Stmt::With(_)
    )
}

/// Returns `true` if the given statement is a leaf node.
fn has_nested_block(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            body.iter().any(is_nested_block)
                || elif_else_clauses
                    .iter()
                    .any(|elif_else| elif_else.body.iter().any(is_nested_block))
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            body.iter().any(is_nested_block) || orelse.iter().any(is_nested_block)
        }
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            body.iter().any(is_nested_block) || orelse.iter().any(is_nested_block)
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            body.iter().any(is_nested_block)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => body.iter().any(is_nested_block),
                })
                || orelse.iter().any(is_nested_block)
                || finalbody.iter().any(is_nested_block)
        }
        Stmt::With(ast::StmtWith { body, .. }) => body.iter().any(is_nested_block),
        _ => false,
    }
}
