use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, visitor, Expr, ExprLambda, Parameter, ParameterWithDefault};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `lambda` definitions that consist of a single function call
/// with the same arguments as the `lambda` itself.
///
/// ## Why is this bad?
/// When a `lambda` is used to wrap a function call, and merely propagates
/// the `lambda` arguments to that function, it can typically be replaced with
/// the function itself, removing a level of indirection.
///
/// ## Example
/// ```python
/// df.apply(lambda x: str(x))
/// ```
///
/// Use instead:
/// ```python
/// df.apply(str)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe for two primary reasons.
///
/// First, the lambda body itself could contain an effect.
///
/// For example, replacing `lambda x, y: (func()(x, y))` with `func()` would
/// lead to a change in behavior, as `func()` would be evaluated eagerly when
/// defining the lambda, rather than when the lambda is called.
///
/// However, even when the lambda body itself is pure, the lambda may
/// change the argument names, which can lead to a change in behavior when
/// callers pass arguments by name.
///
/// For example, replacing `foo = lambda x, y: func(x, y)` with `foo = func`,
/// where `func` is defined as `def func(a, b): return a + b`, would be a
/// breaking change for callers that execute the lambda by passing arguments by
/// name, as in: `foo(x=1, y=2)`. Since `func` does not define the arguments
/// `x` and `y`, unlike the lambda, the call would raise a `TypeError`.
#[violation]
pub struct UnnecessaryLambda;

impl AlwaysFixableViolation for UnnecessaryLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Lambda may be unnecessary; consider inlining inner function")
    }

    fn fix_title(&self) -> String {
        "Inline function call".to_string()
    }
}

/// PLW0108
pub(crate) fn unnecessary_lambda(checker: &mut Checker, lambda: &ExprLambda) {
    let ExprLambda {
        parameters,
        body,
        range: _,
    } = lambda;

    // The lambda should consist of a single function call.
    let Expr::Call(ast::ExprCall {
        arguments, func, ..
    }) = body.as_ref()
    else {
        return;
    };

    // Ignore call chains.
    if let Expr::Attribute(ast::ExprAttribute { value, .. }) = func.as_ref() {
        if value.is_call_expr() {
            return;
        }
    }

    // If at least one of the lambda parameters has a default value, abort. We can't know if the
    // defaults provided by the lambda are the same as the defaults provided by the inner
    // function.
    if parameters.as_ref().is_some_and(|parameters| {
        parameters
            .args
            .iter()
            .any(|ParameterWithDefault { default, .. }| default.is_some())
    }) {
        return;
    }

    match parameters.as_ref() {
        None => {
            if !arguments.is_empty() {
                return;
            }
        }
        Some(parameters) => {
            // Collect all starred arguments (e.g., `lambda *args: func(*args)`).
            let call_varargs: Vec<&Expr> = arguments
                .args
                .iter()
                .filter_map(|arg| {
                    if let Expr::Starred(ast::ExprStarred { value, .. }) = arg {
                        Some(value.as_ref())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            // Collect all keyword arguments (e.g., `lambda x, y: func(x=x, y=y)`).
            let call_kwargs: Vec<&Expr> = arguments
                .keywords
                .iter()
                .map(|kw| &kw.value)
                .collect::<Vec<_>>();

            // Collect all positional arguments (e.g., `lambda x, y: func(x, y)`).
            let call_posargs: Vec<&Expr> = arguments
                .args
                .iter()
                .filter(|arg| !arg.is_starred_expr())
                .collect::<Vec<_>>();

            // Ex) `lambda **kwargs: func(**kwargs)`
            match parameters.kwarg.as_ref() {
                None => {
                    if !call_kwargs.is_empty() {
                        return;
                    }
                }
                Some(kwarg) => {
                    let [call_kwarg] = &call_kwargs[..] else {
                        return;
                    };

                    let Expr::Name(ast::ExprName { id, .. }) = call_kwarg else {
                        return;
                    };

                    if id.as_str() != kwarg.name.as_str() {
                        return;
                    }
                }
            }

            // Ex) `lambda *args: func(*args)`
            match parameters.vararg.as_ref() {
                None => {
                    if !call_varargs.is_empty() {
                        return;
                    }
                }
                Some(vararg) => {
                    let [call_vararg] = &call_varargs[..] else {
                        return;
                    };

                    let Expr::Name(ast::ExprName { id, .. }) = call_vararg else {
                        return;
                    };

                    if id.as_str() != vararg.name.as_str() {
                        return;
                    }
                }
            }

            // Ex) `lambda x, y: func(x, y)`
            let lambda_posargs: Vec<&Parameter> = parameters
                .args
                .iter()
                .map(|ParameterWithDefault { parameter, .. }| parameter)
                .collect::<Vec<_>>();
            if call_posargs.len() != lambda_posargs.len() {
                return;
            }
            for (param, arg) in lambda_posargs.iter().zip(call_posargs.iter()) {
                let Expr::Name(ast::ExprName { id, .. }) = arg else {
                    return;
                };
                if id.as_str() != param.name.as_str() {
                    return;
                }
            }
        }
    }

    // The lambda is necessary if it uses one of its parameters _as_ the function call.
    // Ex) `lambda x, y: x(y)`
    let names = {
        let mut finder = NameFinder::default();
        finder.visit_expr(func);
        finder.names
    };

    for name in names {
        if let Some(binding_id) = checker.semantic().resolve_name(name) {
            let binding = checker.semantic().binding(binding_id);
            if checker.semantic().is_current_scope(binding.scope) {
                return;
            }
        }
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryLambda, lambda.range());
    diagnostic.set_fix(Fix::applicable_edit(
        Edit::range_replacement(
            checker.locator().slice(func.as_ref()).to_string(),
            lambda.range(),
        ),
        Applicability::Unsafe,
    ));
    checker.diagnostics.push(diagnostic);
}

/// Identify all `Expr::Name` nodes in an AST.
#[derive(Debug, Default)]
struct NameFinder<'a> {
    /// A map from identifier to defining expression.
    names: Vec<&'a ast::ExprName>,
}

impl<'a> Visitor<'a> for NameFinder<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(expr_name) = expr {
            self.names.push(expr_name);
        }
        visitor::walk_expr(self, expr);
    }
}
