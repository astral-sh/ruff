use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, ArgOrKeyword, Expr, ExprLambda, ParameterItem, visitor};
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
pub(crate) fn unnecessary_lambda(checker: &Checker, lambda: &ExprLambda) {
    let ExprLambda {
        parameters,
        body,
        range: _,
        node_index: _,
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

    let forwards = match parameters.as_deref() {
        None => arguments.is_empty(),
        Some(parameters) => forwards_arguments(parameters, arguments),
    };
    if !forwards {
        return;
    }

    // The lambda is necessary if it uses one of its parameters _as_ the function call.
    // Ex) `lambda x, y: x(y)`
    let names = {
        let mut finder = NameFinder::default();
        finder.visit_expr(func);
        finder.names
    };

    for name in &names {
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
    for name in names {
        if let Some(binding_id) = checker.semantic().lookup_symbol(name.id()).binding_id() {
            let binding = checker.semantic().binding(binding_id);
            if checker
                .semantic()
                .current_scope()
                .shadowed_binding(binding_id)
                .is_some()
                && binding
                    .expression(checker.semantic())
                    .is_some_and(Expr::is_named_expr)
            {
                return;
            }
        }
    }

    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        if func.is_named_expr() {
            format!("({})", checker.locator().slice(func.as_ref()))
        } else {
            checker.locator().slice(func.as_ref()).to_string()
        },
        lambda.range(),
    )));
}

/// Check whether the call forwards the lambda's positional-or-keyword and variadic parameters.
fn forwards_arguments(parameters: &ast::Parameters, arguments: &ast::Arguments) -> bool {
    let mut vararg = None;
    let mut kwarg = None;
    let mut positional = parameters.items.iter().filter_map(|item| match item {
        ParameterItem::PositionalOrKeyword(parameter) => Some(parameter),
        ParameterItem::Vararg(parameter) => {
            // Parameter lookup selects the first `*` parameter on parser recovery.
            vararg.get_or_insert(parameter);
            None
        }
        ParameterItem::Kwarg(parameter) => {
            // It selects the last `**` parameter.
            kwarg = Some(parameter);
            None
        }
        ParameterItem::PositionalOnly(_) | ParameterItem::KeywordOnly(_) => None,
    });
    let mut starred = None;
    let mut keyword = None;
    for argument in arguments.iter_source_order() {
        let (slot, value) = match argument {
            ArgOrKeyword::Arg(arg) => match arg {
                Expr::Name(name) => {
                    let Some(parameter) = positional.next() else {
                        return false;
                    };
                    if parameter.default.is_some()
                        || parameter.parameter.name.as_str() != name.id.as_str()
                    {
                        return false;
                    }
                    continue;
                }
                Expr::Starred(arg) => (&mut starred, arg.value.as_ref()),
                _ => return false,
            },
            ArgOrKeyword::Keyword(arg) => (&mut keyword, arg.value.as_ref()),
        };
        let Some(name) = value.as_name_expr() else {
            return false;
        };
        if slot.replace(name.id.as_str()).is_some() {
            return false;
        }
    }
    positional.next().is_none()
        && vararg.map(|param| param.name.as_str()) == starred
        && kwarg.map(|param| param.name.as_str()) == keyword
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
