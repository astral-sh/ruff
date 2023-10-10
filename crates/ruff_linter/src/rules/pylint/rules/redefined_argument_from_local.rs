use ruff_python_ast::{self as ast, Decorator, Expr, Parameters, Stmt, ExprName, ExprContext, visitor, ExceptHandlerExceptHandler};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;

/// ## What it does
///
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// def show(host_id=10.11):
///     # +1: [redefined-argument-from-local]
///     for host_id, host in [[12.13, "Venus"], [14.15, "Mars"]]:
///         print(host_id, host)
/// ```
///
/// Use instead:
/// ```python
/// def show(host_id=10.11):
///     for inner_host_id, host in [[12.13, "Venus"], [14.15, "Mars"]]:
///         print(host_id, inner_host_id, host)
/// ```
///
/// ## References
/// - [Python documentation: `property`](https://docs.python.org/3/library/functions.html#property)
#[violation]
pub struct RedefinedArgumentFromLocal;

impl Violation for RedefinedArgumentFromLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot have defined parameters for properties")
    }
}

#[derive(Default)]
struct StoredNamesVisitor<'a> {
    stored: Vec<&'a String>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for StoredNamesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::For(ast::StmtFor {
                          target,
                          ..
                      }) => {
                self.visit_expr(target)
            },
            Stmt::Try(ast::StmtTry {
                handlers,
                ..
                      }) => {
                for handler in handlers {
                    if let ExceptHandlerExceptHandler {name, ..} = handler {
                        if let Some(ident) = name {
                            self.stored.push(ident.id);
                        }
                    };
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match &name.ctx {
                ExprContext::Store => self.stored.push(name),
                _ => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

pub(crate) fn redefined_argument_from_local(
    checker: &mut Checker,
    names: &[ExprName],
) {}
