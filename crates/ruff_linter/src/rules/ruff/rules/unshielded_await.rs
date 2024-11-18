use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::{QualifiedName, QualifiedNameBuilder};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    self as ast, visitor, Comprehension, ExceptHandler, Expr, ExprAttribute, ExprCall, Stmt,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

/// ## What it does
/// TODO
///
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// ## References
/// TODO
#[violation]
pub struct UnshieldedAwait;

impl Violation for UnshieldedAwait {
    #[derive_message_formats]
    fn message(&self) -> String {
        "shield it!".to_string()
    }
}

/// RUF102
pub(crate) fn unshielded_await_for_try(
    checker: &mut Checker,
    handlers: &Vec<ExceptHandler>,
    finalbody: &[Stmt],
) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(handler) = handler;

        let Some(t) = handler.type_.as_deref() else {
            todo!()
        };

        let mut builder = QualifiedNameBuilder::default();
        builder.push("asyncio");
        builder.push("CancelledError");
        let asyncio_cancelled_error = builder.build();

        let mut builder = QualifiedNameBuilder::default();
        builder.push("Exception");
        let exception = builder.build();

        let types = flattened_tuple(t, checker.semantic());

        // TODO i challenge you to make it worse than this
        if !types.iter().any(|tt| {
            format!("{tt}").as_str() == format!("{exception}").as_str()
                || format!("{tt}").as_str() == format!("{asyncio_cancelled_error}").as_str()
        }) {
            continue;
        }

        // If there are no awaits then there is nothing to shield
        let mut visitor = PrunedAwaitVisitor { seen_await: false, semantic: checker.semantic() };
        visitor.visit_body(&handler.body);
        if visitor.seen_await {
            checker
                .diagnostics
                .push(Diagnostic::new(UnshieldedAwait {}, handler.range));
        }
    }

    // If there are no awaits then there is nothing to shield
    let mut visitor = PrunedAwaitVisitor { seen_await: false, semantic: checker.semantic() };
    visitor.visit_body(finalbody);
    if visitor.seen_await {
        checker.diagnostics.push(Diagnostic::new(
            UnshieldedAwait {},
            // TODO yeah not sure where to get the finally range itself
            finalbody[0].range(),
        ));
    }
}

fn flattened_tuple<'a>(t: &'a Expr, semantic: &'a SemanticModel<'a>) -> Vec<QualifiedName<'a>> {
    let mut f = vec![];

    match t {
        Expr::Tuple(t) => {
            for e in t {
                f.append(&mut flattened_tuple(e, semantic));
            }
        }
        Expr::Name(..) | Expr::Attribute(..) => {
            if let Some(name) = semantic.resolve_qualified_name(t) {
                f.push(name);
            } else {
                panic!("inside unable to handle {t:?}");
            };
        }
        _ => panic!("outside unable to handle {t:?}"),
    }

    f
}

/// A [`Visitor`] that detects the presence of `await` expressions in the current scope.
struct PrunedAwaitVisitor<'a> {
    seen_await: bool,
    semantic: &'a SemanticModel<'a>,
}

impl Visitor<'_> for PrunedAwaitVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => (),
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                self.seen_await = true;
            }
            Stmt::With(ast::StmtWith {
                is_async: false,
                items,
                ..
            }) => {
                for item in items {
                    if let Expr::Call(ExprCall { ref func, .. }) = item.context_expr {
                        if let Some(name) = self.semantic.resolve_qualified_name(func) {
                            let mut builder = QualifiedNameBuilder::default();
                            builder.push("anyio");
                            builder.push("CancelScope");
                            let anyio_cancel_scope = builder.build();
                            // TODO what about x = y(); with y:? check the shield argument, etc
                            // TODO i challenge you to make it worse than this
                            if format!("{name}").as_str() == format!("{anyio_cancel_scope}").as_str() {
                                return;
                            }
                        }
                    }
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::For(ast::StmtFor { is_async: true, .. }) => {
                self.seen_await = true;
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Await(ast::ExprAwait { .. }) = expr {
            self.seen_await = true;
        } else {
            visitor::walk_expr(self, expr);
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'_ Comprehension) {
        if comprehension.is_async {
            self.seen_await = true;
        } else {
            visitor::walk_comprehension(self, comprehension);
        }
    }
}
