use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    self as ast, visitor, Comprehension, ExceptHandler, Expr, ExprBooleanLiteral, ExprCall,
    Identifier, Stmt,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

// TODO review for and add handling of trio and asyncio solutions
// TODO will a config option be needed for which system is being used?
// TODO consider checking .__aexit__() as well

/// ## What it does
/// Checks for async yielding activities such as `await`, async loops, and async context managers
/// in cleanup contexts that are not shielded from cancellation.
///
/// ## Why is this bad?
/// The intent when coding in a `finally:` block and similar is usually to have that code execute
/// in to completion to undo an incomplete action or shutdown a resource.  If async activities in
/// these context are not shielded then they may be cancelled leaving the cleanup incomplete.
///
/// ## Example
/// ```python
/// session = await login(username, password)
/// try:
///     win_the_game(session)
/// finally:
///     await session.close()
/// ```
///
/// Use instead:
/// ```python
/// session = await login(username, password)
/// try:
///     win_the_game(session)
/// finally:
///     with anyio.CancelScope(shield=True):
///         await session.close()
/// ```
///
/// ## References
/// - [AnyIO shielding](https://anyio.readthedocs.io/en/stable/cancellation.html#shielding)
#[violation]
pub struct UnshieldedAsync;

impl Violation for UnshieldedAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        "shield it!".to_string()
    }
}

/// RUF102
pub(crate) fn unshielded_async_for_try(
    checker: &mut Checker,
    handlers: &Vec<ExceptHandler>,
    finalbody: &[Stmt],
) {
    let mut unshielded_async_ranges: Vec<TextRange> = vec![];

    for handler in handlers {
        let ExceptHandler::ExceptHandler(handler) = handler;

        let Some(t) = handler.type_.as_deref() else {
            todo!()
        };

        let types = flattened_tuple(t, checker.semantic());

        if !types.iter().any(|tt| {
            // TODO asyncio.CancelledError vs. CancelledError
            tt.segments() == ["", "Exception"] || tt.segments() == ["asyncio", "CancelledError"]
        }) {
            continue;
        }

        // If there is no async then there is nothing to shield
        let mut visitor = PrunedAsyncVisitor {
            semantic: checker.semantic(),
            async_ranges: vec![],
        };
        visitor.visit_body(&handler.body);
        unshielded_async_ranges.extend(visitor.async_ranges);
    }

    // If there is no async then there is nothing to shield
    let mut visitor = PrunedAsyncVisitor {
        semantic: checker.semantic(),
        async_ranges: vec![],
    };
    visitor.visit_body(finalbody);
    unshielded_async_ranges.extend(visitor.async_ranges);

    for range in unshielded_async_ranges {
        checker
            .diagnostics
            .push(Diagnostic::new(UnshieldedAsync {}, range));
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

/// A [`Visitor`] that detects the presence of async expressions in the current scope.
struct PrunedAsyncVisitor<'a> {
    async_ranges: Vec<TextRange>,
    semantic: &'a SemanticModel<'a>,
}

impl Visitor<'_> for PrunedAsyncVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => (),
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                self.async_ranges.push(stmt.range());
            }
            Stmt::With(ast::StmtWith {
                is_async: false,
                items,
                ..
            }) => {
                for item in items {
                    if let Expr::Call(ExprCall {
                        ref func,
                        arguments,
                        ..
                    }) = &item.context_expr
                    {
                        if let Some(name) = self.semantic.resolve_qualified_name(func) {
                            // TODO what about x = y(); with y:?
                            let managers: Vec<Vec<&str>> = vec![
                                vec!["anyio", "CancelScope"],
                                vec!["anyio", "CancelScope", "bogus"],
                                vec!["anyio", "move_on_after"],
                                vec!["anyio", "fail_after"],
                            ];

                            if managers.contains(&Vec::from(name.segments())) {
                                for keyword in &arguments.keywords {
                                    if let Some(Identifier { id: name, .. }) = &keyword.arg {
                                        if name.as_str() == "shield"
                                            && matches!(
                                                keyword.value,
                                                Expr::BooleanLiteral(ExprBooleanLiteral {
                                                    value: true,
                                                    ..
                                                })
                                            )
                                        {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::For(ast::StmtFor { is_async: true, .. }) => {
                self.async_ranges.push(stmt.range());
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Await(ast::ExprAwait { .. }) = expr {
            self.async_ranges.push(expr.range());
        } else {
            visitor::walk_expr(self, expr);
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'_ Comprehension) {
        if comprehension.is_async {
            self.async_ranges.push(comprehension.range());
        } else {
            visitor::walk_comprehension(self, comprehension);
        }
    }
}