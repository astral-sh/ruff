use rustc_hash::FxHashSet;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ExceptHandlerExceptHandler, Expr, ExprContext, Identifier, Stmt, visitor};
use ruff_python_ast::ExceptHandler::ExceptHandler;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::ScopeKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables defined in `for`, `try`, `with` statements
/// that redefine function parameters.
///
/// ## Why is this bad?
/// Redefined variable can cause unexpected behavior because of overriden function parameter.
/// If nested functions are declared, inner function's body can override outer function's parameter.
///
/// ## Example
/// ```python
/// def show(host_id=10.11):
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
/// ## References
/// - [Pylint documentation](https://pylint.readthedocs.io/en/latest/user_guide/messages/refactor/redefined-argument-from-local.html)

#[violation]
pub struct RedefinedArgumentFromLocal {
    name: String,
}

impl Violation for RedefinedArgumentFromLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedArgumentFromLocal {name} = self;
        format!("Redefining argument with the local name `{name}`")
    }
}

#[derive(Default)]
struct StoredNamesVisitor<'a> {
    identifiers: Vec<&'a Identifier>,
    expressions: Vec<&'a ast::ExprName>,
}

impl<'a> StoredNamesVisitor<'a> {
    fn names(&self) -> Vec<(&str, TextRange)> {
        self.identifiers.iter()
            .map(|i| (i.as_str(), i.range()))
            .chain(self.expressions.iter().map(|e| (e.id.as_str(), e.range)))
            .collect::<Vec<_>>()
    }
}

/// `Visitor` to collect all stored names in a statement.
impl<'a> Visitor<'a> for StoredNamesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::For(ast::StmtFor { target, .. }) => {
                self.visit_expr(target)
            }
            Stmt::Try(ast::StmtTry { handlers, .. }) => {
                for handler in handlers {
                    let ExceptHandler(ExceptHandlerExceptHandler { name, .. })
                        = handler;
                    if let Some(ident) = name {
                        self.identifiers.push(ident);
                    }
                }
            }
            Stmt::With(ast::StmtWith { items, .. }) => {
                for item in items {
                    if let Some(expr) = &item.optional_vars {
                        self.visit_expr(expr);
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match &name.ctx {
                ExprContext::Store => self.expressions.push(name),
                _ => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// PLR1704
pub(crate) fn redefined_argument_from_local(
    checker: &mut Checker,
    stmt: &Stmt,
) {
    let mut visitor = StoredNamesVisitor::default();
    visitor.visit_stmt(stmt);

    let mut dianostics = vec![];
    let mut already_added: FxHashSet<TextRange> = FxHashSet::default();
    let mut scope = checker.semantic().current_scope();
    loop {
        if let ScopeKind::Function(ast::StmtFunctionDef { parameters, .. }) = scope.kind {
            for (name, range) in visitor.names() {
                if parameters.includes(name) && !already_added.contains(&range) {
                    dianostics.push(Diagnostic::new(
                        RedefinedArgumentFromLocal {
                            name: name.to_string(),
                        },
                        range,
                    ));
                    already_added.insert(range);
                }
            }
        }
        if let Some(scope_id) = scope.parent {
            scope = &checker.semantic().scopes[scope_id];
        } else {
            break
        };
    }
    checker.diagnostics.extend(dianostics);

    // // Different global functions can have same argument name which makes redundant diagnostics.
    // for scope in checker.semantic().scopes.iter() {
    //     let ScopeKind::Function(ast::StmtFunctionDef { parameters, .. })
    //         = scope.kind else {
    //         continue;
    //     };
    //     for name in parameters.posonlyargs
    //             .iter()
    //             .chain(&parameters.args)
    //             .chain(&parameters.kwonlyargs)
    //             .map(|arg| arg.parameter.name.as_str()) {
    //         all_names.insert(name);
    //     }
    //     if let Some(arg) = &parameters.vararg {
    //         all_names.insert(arg.name.as_str());
    //     }
    //     if let Some(arg) = &parameters.kwarg {
    //         all_names.insert(arg.name.as_str());
    //     }
    // }
    // for (name, range) in visitor.names() {
    //     if all_names.contains(name) {
    //         dianostics.push(Diagnostic::new(
    //             RedefinedArgumentFromLocal {
    //                 name: name.to_string(),
    //             },
    //             range,
    //         ));
    //     }
    // }
    // checker.diagnostics.extend(dianostics);
}
