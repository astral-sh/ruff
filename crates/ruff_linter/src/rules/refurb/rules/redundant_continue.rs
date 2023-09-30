use ruff_diagnostics::{Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, ExprAttribute, ExprCall};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest, registry::AsRule};

/// ## What it does
/// TODO
///
/// ## Why is this bad?
/// TODO
///
///
/// ## Example
/// ```python
/// def func():
///     while True:
///         pass
///
///         continue
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     while True:
///         pass
/// ```
///
/// ## References
/// - [Python documentation: `continue`](https://docs.python.org/3/reference/simple_stmts.html#continue)

#[violation]
pub struct RedundantContinue;

impl Violation for ImplicitCwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't explicitly continue if you are already at the end of the control flow")
    }
}

/// FURB133
pub(crate) fn redundant_continue(checker: &mut Checker, continue_stmt: &ast::Continue) {}

fn get_trailing_continue(node: Statement) -> impl Iterator<Item = Statement> {
    match node {
        Statement::ContinueStmt(_) => vec![node].into_iter(),
        Statement::MatchStmt { bodies, patterns } => bodies
            .into_iter()
            .zip(patterns.into_iter())
            .flat_map(|(body, pattern)| match (&*body.body, pattern) {
                (
                    _,
                    Pattern::AsPattern {
                        pattern: None,
                        name: None,
                    },
                ) => Vec::new().into_iter(),

                (vec![Statement::ContinueStmt(_)], _) => vec![].into_iter(),

                _ => get_trailing_continue(body.body.last().unwrap()),
            }),
        _ => {
            let stmt = match node {
                Statement::IfStmt {
                    else_body: Block { body },
                    ..
                }
                | Statement::WithStmt {
                    body: Block { body },
                    ..
                } => body.last().unwrap(),
                _ => return Vec::new().into_iter(),
            };
            get_trailing_continue(stmt)
        }
    }
}

fn check(node: Statement, errors: &mut Vec<Error>) {
    match node {
        Statement::ForStmt {
            body: Block { body },
            ..
        }
        | Statement::WhileStmt {
            body: Block { body },
            ..
        } => {
            if let vec![stmt @ Statement::ContinueStmt(_)] = &*body {
                return;
            }

            errors.extend(get_trailing_continue(body.last().unwrap()).map(ErrorInfo::from_node));
        }
        _ => (),
    }
}
