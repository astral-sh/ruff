use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    self as ast, Constant, Expr, ExprAttribute, ExprCall, StmtContinue, StmtFor, StmtMatch,
};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest, registry::AsRule};

/// ## What it does
/// Checks for redundant and useless trailing `continue` statements.
///
/// ## Why is this bad?
/// Trailing `continue` statements are unnecessary and can be removed to simplify the code.
///
/// ## Example
/// ```python
/// for x in range(10):
///     if x:
///         pass
///     else:
///         continue
/// ```
///
/// Use instead:
/// ```python
/// for x in range(10):
///     if x:
///         pass
/// ```
///
/// ## References
/// - [Python documentation: `continue`](https://docs.python.org/3/reference/simple_stmts.html#continue)

#[violation]
pub struct RedundantContinue;

impl Violation for RedundantContinue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't explicitly continue if you are already at the end of the control flow")
    }
}

fn get_trailing_continue(node: &ast::Stmt) -> Vec<&ast::StmtContinue> {
    match node {
        ast::Stmt::StmtContinue(_) => vec![node],
        ast::Stmt::StmtMatch { bodies, patterns } => {
            let mut continues = vec![];
            for (body, pattern) in bodies.iter().zip(patterns) {
                match (body.body.last(), pattern) {
                    (
                        _,
                        ast::Pattern::AsPattern {
                            pattern: None,
                            name: None,
                        },
                    ) => (),
                    (Some(ast::Stmt::StmtContinue(_)), _) => continue,
                    _ => continues.extend(get_trailing_continue(body.body.last().unwrap())),
                }
            }
            continues
        }
        ast::Stmt::IfStmt {
            else_body: Some(ast::Block { body }),
            ..
        }
        | ast::Stmt::WithStmt {
            body: ast::Block { body },
            ..
        } => get_trailing_continue(body.last().unwrap()),
        _ => vec![],
    }
}

fn check(node: &ast::Stmt, errors: &mut Vec<Diagnostic>) {
    match node {
        ast::Stmt::ForStmt {
            body: ast::Block { body },
            ..
        }
        | ast::Stmt::WhileStmt {
            body: ast::Block { body },
            ..
        } => {
            if body.len() > 1 {
                if let Some(ast::Stmt::StmtContinue(_)) = body.last() {
                    return;
                }
            }
            errors.extend(
                get_trailing_continue(body.last().unwrap())
                    .into_iter()
                    .map(|x| Diagnostic::new(RedundantContinue, x.range())),
            );
        }
        _ => (),
    }
}

pub(crate) fn redundant_continue(checker: &mut Checker, continue_stmt: &ast::Continue) {
    let mut errors = vec![];
    check(continue_stmt, &mut errors);
    for error in errors {
        if checker.patch(error.kind.rule()) {
            error.try_set_fix(|| {
                Ok(Fix::suggested_edits(
                    Edit::range_replacement("", error.range()),
                    [],
                ))
            });
        }
        checker.diagnostics.push(error);
    }
}
