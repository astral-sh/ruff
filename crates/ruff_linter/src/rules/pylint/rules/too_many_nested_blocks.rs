use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::flat;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.15", category = Category::Pedantic)]
pub(crate) struct TooManyNestedBlocks {
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
pub(crate) fn too_many_nested_blocks(checker: &Checker, stmt: flat::View<'_, Stmt>) {
    // Only enforce nesting within functions or methods.
    if !checker.semantic().current_scope().kind.is_function() {
        return;
    }

    // If the statement isn't a leaf node, we don't want to emit a diagnostic, since the diagnostic
    // will be emitted on the leaves.
    if has_nested_block(stmt) {
        return;
    }

    let max_nested_blocks = checker.settings().pylint.max_nested_blocks;

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

    checker.report_diagnostic(
        TooManyNestedBlocks {
            nested_blocks: count,
            max_nested_blocks,
        },
        checker.semantic().statement(root_id).range(),
    );
}

/// Returns `true` if the given statement is a nested block.
fn is_nested_block(stmt: flat::View<'_, Stmt>) -> bool {
    matches!(
        stmt.classify(),
        flat::StmtKind::If(_)
            | flat::StmtKind::While(_)
            | flat::StmtKind::For(_)
            | flat::StmtKind::Try(_)
            | flat::StmtKind::With(_)
    )
}

/// Returns `true` if the given statement is a leaf node.
fn has_nested_block(stmt: flat::View<'_, Stmt>) -> bool {
    match stmt.classify() {
        flat::StmtKind::If(stmt_if) => {
            let body = stmt_if.body();
            let elif_else_clauses = stmt_if.elif_else_clauses();

            body.iter().any(is_nested_block)
                || elif_else_clauses
                    .iter()
                    .any(|elif_else| elif_else.body().iter().any(is_nested_block))
        }
        flat::StmtKind::While(stmt_while) => {
            let body = stmt_while.body();
            let orelse = stmt_while.orelse();

            body.iter().any(is_nested_block) || orelse.iter().any(is_nested_block)
        }
        flat::StmtKind::For(stmt_for) => {
            let body = stmt_for.body();
            let orelse = stmt_for.orelse();

            body.iter().any(is_nested_block) || orelse.iter().any(is_nested_block)
        }
        flat::StmtKind::Try(stmt_try) => {
            let body = stmt_try.body();
            let handlers = stmt_try.handlers();
            let orelse = stmt_try.orelse();
            let finalbody = stmt_try.finalbody();

            body.iter().any(is_nested_block)
                || handlers.iter().any(|handler| match handler.classify() {
                    flat::ExceptHandlerKind::ExceptHandler(except_handler_except_handler) => {
                        let body = except_handler_except_handler.body();
                        body.iter().any(is_nested_block)
                    }
                })
                || orelse.iter().any(is_nested_block)
                || finalbody.iter().any(is_nested_block)
        }
        flat::StmtKind::With(stmt_with) => {
            let body = stmt_with.body();

            body.iter().any(is_nested_block)
        }
        _ => false,
    }
}
