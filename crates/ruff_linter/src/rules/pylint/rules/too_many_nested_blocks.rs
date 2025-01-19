use std::ptr;

use ast::ExceptHandler;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

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
#[derive(ViolationMetadata)]
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
pub(crate) fn too_many_nested_blocks(checker: &mut Checker, stmt: &Stmt) {
    let semantic = checker.semantic();

    // Only enforce nesting within functions or methods.
    if !semantic.current_scope().kind.is_function() {
        return;
    }

    // If the statement isn't a leaf node, we don't want to emit a diagnostic, since the diagnostic
    // will be emitted on the leaves.
    if has_nested_block(stmt) {
        return;
    }

    if !current_stmt_is_first_within_parent(semantic) {
        return;
    }

    let max_nested_blocks = checker.settings.pylint.max_nested_blocks;

    let Some(current_id) = semantic.current_statement_id() else {
        return;
    };

    // Traverse up the hierarchy, identifying the root node and counting the number of nested
    // blocks between the root and this leaf.
    let (count, encountered_root) =
        semantic
            .current_statement_ids()
            .fold((0, false), |(count, encountered_root), id| {
                let stmt = semantic.statement(id);
                if is_nested_block(stmt) {
                    (count + 1, true)
                } else {
                    (count, encountered_root)
                }
            });

    if !encountered_root {
        return;
    }

    // If the number of nested blocks is less than the maximum, we don't want to emit a diagnostic.
    if count <= max_nested_blocks {
        return;
    }

    let current_stmt_start = semantic.statement(current_id).start();
    let current_stmt_line_start = checker.locator().line_start(current_stmt_start);
    let indentation_range = TextRange::new(current_stmt_line_start, current_stmt_start);

    checker.diagnostics.push(Diagnostic::new(
        TooManyNestedBlocks {
            nested_blocks: count,
            max_nested_blocks,
        },
        indentation_range,
    ));
}

/// Returns `true` if the given statement is a nested block.
fn is_nested_block(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::If(_) | Stmt::While(_) | Stmt::For(_) | Stmt::Try(_) | Stmt::With(_)
    )
}

/// Returns `true` if the given statement is not a leaf node.
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

fn current_stmt_is_first_within_parent(semantic: &SemanticModel) -> bool {
    let Some(parent) = semantic.current_statement_parent() else {
        return false;
    };
    let current = semantic.current_statement();

    match parent {
        Stmt::If(ast::StmtIf { body, .. })
        | Stmt::While(ast::StmtWhile { body, .. })
        | Stmt::For(ast::StmtFor { body, .. })
        | Stmt::Try(ast::StmtTry { body, .. })
        | Stmt::With(ast::StmtWith { body, .. }) => {
            let [first, ..] = &body[..] else {
                return false;
            };

            ptr::eq(first, current)
        }
        _ => false,
    }
}
