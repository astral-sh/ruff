use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, AstNode, Expr, ExprKind, ExprLambda, Parameter};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// For example:
///
/// ```python
/// def func(a, b):
///     return a + b
///
///
/// foo = lambda x, y: func(x, y)
/// ```
///
/// Replacing the assignment to `foo` with `foo = func` would be a breaking
/// change for callers that execute the lambda by passing arguments by name, as
/// in: `foo(x=1, y=2)`. Since `func` does not define the arguments `x` and `y`,
/// unlike the lambda, the call would raise a `TypeError`.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.15.0", category = Category::Style)]
pub(crate) struct UnnecessaryLambda;

impl Violation for UnnecessaryLambda {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Lambda may be unnecessary; consider inlining inner function".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Inline function call".to_string())
    }
}

/// PLW0108
pub(crate) fn unnecessary_lambda(checker: &Checker, lambda: ExprLambda<'_>) {
    let parameters = lambda.parameters();
    let body = lambda.body();

    // The lambda should consist of a single function call.
    let ExprKind::Call(call_node) = body.classify() else {
        return;
    };
    let arguments = call_node.arguments();
    let func = call_node.func();

    // Ignore call chains.
    if let ExprKind::Attribute(attribute) = func.classify() {
        let value = attribute.value();
        if value.is_call_expr() {
            return;
        }
    }

    // If at least one of the lambda parameters has a default value, abort. We can't know if the
    // defaults provided by the lambda are the same as the defaults provided by the inner
    // function.
    if parameters.is_some_and(|parameters| {
        parameters.args().iter().any(|parameter_with_default| {
            let default = parameter_with_default.default();
            default.is_some()
        })
    }) {
        return;
    }

    match parameters {
        None => {
            if !arguments.is_empty() {
                return;
            }
        }
        Some(parameters) => {
            // Collect all starred arguments (e.g., `lambda *args: func(*args)`).
            let call_varargs: Vec<Expr<'_>> = arguments
                .args()
                .iter()
                .filter_map(|arg| {
                    if let ExprKind::Starred(starred_node) = arg.classify() {
                        let value = starred_node.value();
                        Some(value)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            // Collect all keyword arguments (e.g., `lambda x, y: func(x=x, y=y)`).
            let call_kwargs: Vec<Expr<'_>> = arguments
                .keywords()
                .iter()
                .map(ast::Keyword::value)
                .collect::<Vec<_>>();

            // Collect all positional arguments (e.g., `lambda x, y: func(x, y)`).
            let call_posargs: Vec<Expr<'_>> = arguments
                .args()
                .iter()
                .filter(|arg| !arg.is_starred_expr())
                .collect::<Vec<_>>();

            // Ex) `lambda **kwargs: func(**kwargs)`
            match parameters.kwarg() {
                None => {
                    if !call_kwargs.is_empty() {
                        return;
                    }
                }
                Some(kwarg) => {
                    let [call_kwarg] = call_kwargs[..] else {
                        return;
                    };

                    let ExprKind::Name(name_node) = call_kwarg.classify() else {
                        return;
                    };
                    let id = name_node.id();

                    if id.as_str() != kwarg.name().as_str() {
                        return;
                    }
                }
            }

            // Ex) `lambda *args: func(*args)`
            match parameters.vararg() {
                None => {
                    if !call_varargs.is_empty() {
                        return;
                    }
                }
                Some(vararg) => {
                    let [call_vararg] = call_varargs[..] else {
                        return;
                    };

                    let ExprKind::Name(name_node) = call_vararg.classify() else {
                        return;
                    };
                    let id = name_node.id();

                    if id.as_str() != vararg.name().as_str() {
                        return;
                    }
                }
            }

            // Ex) `lambda x, y: func(x, y)`
            let lambda_posargs: Vec<Parameter<'_>> = parameters
                .args()
                .iter()
                .map(ast::ParameterWithDefault::parameter)
                .collect::<Vec<_>>();
            if call_posargs.len() != lambda_posargs.len() {
                return;
            }
            for (param, arg) in lambda_posargs.iter().zip(call_posargs) {
                let ExprKind::Name(name_node) = arg.classify() else {
                    return;
                };
                let id = name_node.id();
                if id.as_str() != param.name().as_str() {
                    return;
                }
            }
        }
    }

    // The lambda is necessary if it uses one of its parameters _as_ the function call.
    // Ex) `lambda x, y: x(y)`
    let names: Vec<_> = func
        .descendants()
        .filter_map(AstNode::cast::<ast::ExprName>)
        .collect();

    for &name in &names {
        if let Some(binding_id) = checker.semantic().resolve_name(name) {
            let binding = checker.semantic().binding(binding_id);
            if checker.semantic().is_current_scope(binding.scope) {
                return;
            }
        }
    }

    let mut diagnostic = checker.report_diagnostic(UnnecessaryLambda, lambda.range());
    // Suppress the fix if the assignment expression target shadows one of the lambda's parameters.
    // This is necessary to avoid introducing a change in the behavior of the program.
    for &name in &names {
        if let Some(binding_id) = checker.semantic().lookup_symbol(name.id()).binding_id() {
            let binding = checker.semantic().binding(binding_id);
            if checker
                .semantic()
                .current_scope()
                .shadowed_binding(binding_id)
                .is_some()
                && binding
                    .expression(checker.semantic())
                    .is_some_and(ast::Expr::is_named_expr)
            {
                return;
            }
        }
    }

    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        if func.is_named_expr() {
            format!("({})", checker.locator().slice(func))
        } else {
            checker.locator().slice(func).to_string()
        },
        lambda.range(),
    )));
}
