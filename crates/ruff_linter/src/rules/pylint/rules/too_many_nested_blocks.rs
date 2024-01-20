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
/// This can be configured using the [`pylint.max-nested-blocks`] option.
///
/// ## Why is this bad?
/// Functions or methods with too many nested blocks are harder to understand
/// and maintain.
///
/// ## Options
/// - `pylint.max-nested-blocks`
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
    // check that we're in a function
    // if not, return
    if !checker.semantic().current_scope().kind.is_function() {
        return;
    }

    // check if this statement has any more branching statements
    // if so, return
    if stmt_has_more_stmts(stmt) {
        return;
    }

    let max_nested_blocks = checker.settings.pylint.max_nested_blocks;

    let (count, oldest_ancestor_id) =
        checker
            .semantic()
            .current_statement_ids()
            .fold((0, None), |(count, previous_id), id| {
                let stmt = checker.semantic().statement(id);
                if stmt.is_with_stmt()
                    || stmt.is_if_stmt()
                    || stmt.is_try_stmt()
                    || stmt.is_while_stmt()
                    || stmt.is_for_stmt()
                {
                    // we want to emit the diagnostic on the
                    // oldest nested statement
                    return (count + 1, Some(id));
                }
                (count, previous_id)
            });

    let Some(oldest_ancestor_id) = oldest_ancestor_id else {
        return;
    };

    if count <= max_nested_blocks {
        return;
    }

    let oldest_ancestor = checker.semantic().statement(oldest_ancestor_id);

    checker.diagnostics.push(Diagnostic::new(
        TooManyNestedBlocks {
            nested_blocks: count,
            max_nested_blocks,
        },
        oldest_ancestor.range(),
    ));
}

fn stmt_has_more_stmts(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            body.iter().any(stmt_has_more_stmts)
                || elif_else_clauses
                    .iter()
                    .any(|elif_else| elif_else.body.iter().any(stmt_has_more_stmts))
        }
        Stmt::While(ast::StmtWhile { body, orelse, .. }) => {
            body.iter().any(stmt_has_more_stmts) || orelse.iter().any(stmt_has_more_stmts)
        }
        Stmt::For(ast::StmtFor { body, orelse, .. }) => {
            body.iter().any(stmt_has_more_stmts) || orelse.iter().any(stmt_has_more_stmts)
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            body.iter().any(stmt_has_more_stmts)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => body.iter().any(stmt_has_more_stmts),
                })
                || orelse.iter().any(stmt_has_more_stmts)
                || finalbody.iter().any(stmt_has_more_stmts)
        }
        Stmt::With(ast::StmtWith { body, .. }) => body.iter().any(stmt_has_more_stmts),
        _ => false,
    }
}
