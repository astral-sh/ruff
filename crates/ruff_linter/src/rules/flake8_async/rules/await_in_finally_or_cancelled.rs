use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    self as ast, visitor, Comprehension, ExceptHandler, Expr, ExprBooleanLiteral, ExprCall,
    Identifier, Stmt, StmtFunctionDef,
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
pub struct AwaitInFinallyOrCancelled {
    range: TextRange,
    location: String,
}

impl Violation for AwaitInFinallyOrCancelled {
    #[derive_message_formats]
    fn message(&self) -> String {
        let location = &self.location;

        format!("await inside {location} must have shielded cancel scope with a timeout")
    }
}

/// RUF102
pub(crate) fn await_in_finally_or_cancelled(
    checker: &mut Checker,
    handlers: &Vec<ExceptHandler>,
    finalbody: &[Stmt],
) {
    let mut visitor = PrunedAsyncVisitor::new(checker.semantic());

    let bare_except = vec!["", "", "bare except"];

    // location is selected based on the first matching item
    let interesting_exception_types = [
        bare_except.clone(),
        vec!["", "BaseException"],
        vec!["", "", "cancelled"],
        // TODO only for 3.7+, in case we care about older
        // TODO asyncio.CancelledError vs. CancelledError
        vec!["asyncio", "CancelledError"],
        // TODO only dependent on configuration?
        vec!["trio", "Cancelled"],
        vec!["", "Exception"],
    ];

    let mut concerns: Vec<AwaitInFinallyOrCancelled> = vec![];

    for handler in handlers {
        let ExceptHandler::ExceptHandler(handler) = handler;

        let types = match handler.type_.as_deref() {
            Some(t) => flattened_tuple(t, checker.semantic()),
            None => vec![bare_except.clone()],
        };

        let Some(type_segments) = interesting_exception_types
            .iter()
            .find(|iet| types.contains(iet))
        else {
            continue;
        };

        let location = {
            use itertools::Itertools;
            type_segments
                .iter()
                .filter(|segment| !segment.is_empty())
                .join(".")
        };

        visitor.clear();
        visitor.visit_body(&handler.body);
        for concern in visitor.async_ranges.clone() {
            concerns.push(AwaitInFinallyOrCancelled {
                range: concern,
                location: location.clone(),
            });
        }
        break;
    }

    visitor.clear();
    visitor.visit_body(finalbody);
    for concern in visitor.async_ranges.clone() {
        concerns.push(AwaitInFinallyOrCancelled {
            range: concern,
            location: "finally".to_string(),
        });
    }

    for concern in concerns {
        let range = concern.range;
        checker.diagnostics.push(Diagnostic::new(concern, range));
    }
}

/// RUF102
pub(crate) fn await_in_function_def(checker: &mut Checker, function_def: &StmtFunctionDef) {
    let interesting_names = ["__aexit__"];
    if !function_def.is_async || !interesting_names.contains(&function_def.name.as_str()) {
        return;
    }

    let mut visitor = PrunedAsyncVisitor::new(checker.semantic());
    let mut concerns: Vec<AwaitInFinallyOrCancelled> = vec![];

    // If there is no async then there is nothing to shield
    visitor.async_ranges = vec![];
    visitor.visit_body(&function_def.body);
    for concern in visitor.async_ranges {
        concerns.push(AwaitInFinallyOrCancelled {
            range: concern,
            location: "finally".to_string(),
        });
    }

    for concern in concerns {
        let range = concern.range;
        checker.diagnostics.push(Diagnostic::new(concern, range));
    }
}

fn flattened_tuple<'a>(t: &'a Expr, semantic: &'a SemanticModel<'a>) -> Vec<Vec<&'a str>> {
    let mut f = vec![];

    match t {
        Expr::Tuple(t) => {
            for e in t {
                f.append(&mut flattened_tuple(e, semantic));
            }
        }
        Expr::Name(..) | Expr::Attribute(..) => {
            if let Some(name) = semantic.resolve_qualified_name(t) {
                f.push(Vec::from(name.segments()));
            };
        }
        Expr::Call(call) => {
            if let Some(name) = semantic.resolve_qualified_name(&call.func) {
                if name.segments() == ["anyio", "get_cancelled_exc_class"] {
                    f.push(vec!["", "", "cancelled"]);
                }
            }
        }
        _ => (),
    }

    f
}

/// A [`Visitor`] that detects the presence of async expressions in the current scope.
struct PrunedAsyncVisitor<'a> {
    async_ranges: Vec<TextRange>,
    semantic: &'a SemanticModel<'a>,
    managers: Vec<Vec<&'static str>>,
    allowed_async_calls: Vec<Vec<&'static str>>,
}

impl<'a> PrunedAsyncVisitor<'a> {
    fn new(semantic: &'a SemanticModel) -> Self {
        let managers = vec![
            vec!["anyio", "CancelScope"],
            vec!["anyio", "move_on_after"],
            vec!["anyio", "fail_after"],
            vec!["trio", "CancelScope"],
            vec!["trio", "move_on_after"],
        ];

        let allowed_async_calls = vec![
            vec!["anyio", "aclose_forcefully"],
            vec!["trio", "aclose_forcefully"],
        ];

        Self {
            semantic,
            async_ranges: vec![],
            managers,
            allowed_async_calls,
        }
    }

    fn clear(&mut self) {
        self.async_ranges.clear();
    }
}

impl Visitor<'_> for PrunedAsyncVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => (),
            Stmt::With(ast::StmtWith {
                is_async, items, ..
            }) => {
                for item in items {
                    if let Expr::Call(ExprCall {
                        ref func,
                        arguments,
                        ..
                    }) = &item.context_expr
                    {
                        if let Some(name) = self.semantic.resolve_qualified_name(func) {
                            let segments = &Vec::from(name.segments());
                            if self.managers.contains(segments) {
                                // TODO better awareness of any variations between the known managers
                                let mut shield_satisfied = false;
                                let mut deadline_satisfied = false;
                                if let Some(..) = arguments.args.get(0) {
                                    // TODO check the value isn't inf?
                                    deadline_satisfied = true;
                                }
                                for keyword in &arguments.keywords {
                                    if let Some(Identifier { id: name, .. }) = &keyword.arg {
                                        match name.as_str() {
                                            "shield" => {
                                                if matches!(
                                                    keyword.value,
                                                    Expr::BooleanLiteral(ExprBooleanLiteral {
                                                        value: true,
                                                        ..
                                                    })
                                                ) {
                                                    shield_satisfied = true;
                                                }
                                            }
                                            "deadline" => deadline_satisfied = true,
                                            _ => (),
                                        };
                                    }
                                }
                                if shield_satisfied && deadline_satisfied {
                                    return;
                                }
                            }
                        }
                        if *is_async {
                            self.async_ranges.push(stmt.range());
                        }
                    }
                }
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Await(ast::ExprAwait { value, .. }) = expr {
            if let Expr::Call(ExprCall { ref func, .. }) = **value {
                if let Some(name) = self.semantic.resolve_qualified_name(func) {
                    let segments = &Vec::from(name.segments());
                    if self.allowed_async_calls.contains(segments) {
                        return;
                    }
                }
            };
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
