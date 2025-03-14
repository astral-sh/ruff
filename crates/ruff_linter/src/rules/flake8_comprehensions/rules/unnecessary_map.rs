use std::fmt;

use ruff_diagnostics::{Diagnostic, Fix};
use ruff_diagnostics::{FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprContext, Parameters, Stmt};
use ruff_python_ast::{visitor, ExprLambda};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary `map()` calls with lambda functions.
///
/// ## Why is this bad?
/// Using `map(func, iterable)` when `func` is a lambda is slower than
/// using a generator expression or a comprehension, as the latter approach
/// avoids the function call overhead, in addition to being more readable.
///
/// This rule also applies to `map()` calls within `list()`, `set()`, and
/// `dict()` calls. For example:
///
/// - Instead of `list(map(lambda num: num * 2, nums))`, use
///   `[num * 2 for num in nums]`.
/// - Instead of `set(map(lambda num: num % 2 == 0, nums))`, use
///   `{num % 2 == 0 for num in nums}`.
/// - Instead of `dict(map(lambda v: (v, v ** 2), values))`, use
///   `{v: v ** 2 for v in values}`.
///
/// ## Example
/// ```python
/// map(lambda x: x + 1, iterable)
/// ```
///
/// Use instead:
/// ```python
/// (x + 1 for x in iterable)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryMap {
    object_type: ObjectType,
}

impl Violation for UnnecessaryMap {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryMap { object_type } = self;
        format!("Unnecessary `map()` usage (rewrite using a {object_type})")
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryMap { object_type } = self;
        Some(format!("Replace `map()` with a {object_type}"))
    }
}

/// C417
pub(crate) fn unnecessary_map(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();
    let (func, arguments) = (&call.func, &call.arguments);

    if !arguments.keywords.is_empty() {
        return;
    }

    let Some(object_type) = ObjectType::from(func, semantic) else {
        return;
    };

    let parent = semantic.current_expression_parent();

    let (lambda, iterables) = match object_type {
        ObjectType::Generator => {
            let parent_call_func = match parent {
                Some(Expr::Call(call)) => Some(&call.func),
                _ => None,
            };

            // Exclude the parent if already matched by other arms.
            if parent_call_func.is_some_and(|func| is_list_set_or_dict(func, semantic)) {
                return;
            }

            let Some(result) = map_lambda_and_iterables(call, semantic) else {
                return;
            };

            result
        }

        ObjectType::List | ObjectType::Set | ObjectType::Dict => {
            let [Expr::Call(inner_call)] = arguments.args.as_ref() else {
                return;
            };

            let Some((lambda, iterables)) = map_lambda_and_iterables(inner_call, semantic) else {
                return;
            };

            if object_type == ObjectType::Dict {
                let (Expr::Tuple(ast::ExprTuple { elts, .. })
                | Expr::List(ast::ExprList { elts, .. })) = &*lambda.body
                else {
                    return;
                };

                if elts.len() != 2 {
                    return;
                }
            }

            (lambda, iterables)
        }
    };

    for iterable in iterables {
        // For example, (x+1 for x in (c:=a)) is invalid syntax
        // so we can't suggest it.
        if any_over_expr(iterable, &|expr| expr.is_named_expr()) {
            return;
        }

        if iterable.is_starred_expr() {
            return;
        }
    }

    if !lambda_has_expected_arity(lambda) {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryMap { object_type }, call.range);
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_map(
            call,
            parent,
            object_type,
            checker.locator(),
            checker.stylist(),
        )
        .map(Fix::unsafe_edit)
    });
    checker.report_diagnostic(diagnostic);
}

fn is_list_set_or_dict(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins", "list" | "set" | "dict"]
            )
        })
}

fn map_lambda_and_iterables<'a>(
    call: &'a ast::ExprCall,
    semantic: &'a SemanticModel,
) -> Option<(&'a ExprLambda, &'a [Expr])> {
    if !semantic.match_builtin_expr(&call.func, "map") {
        return None;
    }

    let arguments = &call.arguments;

    if !arguments.keywords.is_empty() {
        return None;
    }

    let Some((Expr::Lambda(lambda), iterables)) = arguments.args.split_first() else {
        return None;
    };

    Some((lambda, iterables))
}

/// A lambda as the first argument to `map()` has the "expected" arity when:
///
/// * It has exactly one parameter
/// * That parameter is not variadic
/// * That parameter does not have a default value
fn lambda_has_expected_arity(lambda: &ExprLambda) -> bool {
    let Some(parameters) = lambda.parameters.as_deref() else {
        return false;
    };

    let [parameter] = &*parameters.args else {
        return false;
    };

    if parameter.default.is_some() {
        return false;
    }

    if parameters.vararg.is_some() || parameters.kwarg.is_some() {
        return false;
    }

    if late_binding(parameters, &lambda.body) {
        return false;
    }

    true
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ObjectType {
    Generator,
    List,
    Set,
    Dict,
}

impl ObjectType {
    fn from(func: &Expr, semantic: &SemanticModel) -> Option<Self> {
        match semantic.resolve_builtin_symbol(func) {
            Some("map") => Some(Self::Generator),
            Some("list") => Some(Self::List),
            Some("set") => Some(Self::Set),
            Some("dict") => Some(Self::Dict),
            _ => None,
        }
    }
}

impl fmt::Display for ObjectType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ObjectType::Generator => fmt.write_str("generator expression"),
            ObjectType::List => fmt.write_str("list comprehension"),
            ObjectType::Set => fmt.write_str("set comprehension"),
            ObjectType::Dict => fmt.write_str("dict comprehension"),
        }
    }
}

/// Returns `true` if the lambda defined by the given parameters and body contains any names that
/// are late-bound within nested lambdas.
///
/// For example, given:
///
/// ```python
/// map(lambda x: lambda: x, range(4))  # (0, 1, 2, 3)
/// ```
///
/// The `x` in the inner lambda is "late-bound". Specifically, rewriting the above as:
///
/// ```python
/// (lambda: x for x in range(4))  # (3, 3, 3, 3)
/// ```
///
/// Would yield an incorrect result, as the `x` in the inner lambda would be bound to the last
/// value of `x` in the comprehension.
fn late_binding(parameters: &Parameters, body: &Expr) -> bool {
    let mut visitor = LateBindingVisitor::new(parameters);
    visitor.visit_expr(body);
    visitor.late_bound
}

#[derive(Debug)]
struct LateBindingVisitor<'a> {
    /// The arguments to the current lambda.
    parameters: &'a Parameters,
    /// The arguments to any lambdas within the current lambda body.
    lambdas: Vec<Option<&'a Parameters>>,
    /// Whether any names within the current lambda body are late-bound within nested lambdas.
    late_bound: bool,
}

impl<'a> LateBindingVisitor<'a> {
    fn new(parameters: &'a Parameters) -> Self {
        Self {
            parameters,
            lambdas: Vec::new(),
            late_bound: false,
        }
    }
}

impl<'a> Visitor<'a> for LateBindingVisitor<'a> {
    fn visit_stmt(&mut self, _stmt: &'a Stmt) {}

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ast::ExprLambda { parameters, .. }) => {
                self.lambdas.push(parameters.as_deref());
                visitor::walk_expr(self, expr);
                self.lambdas.pop();
            }
            Expr::Name(ast::ExprName {
                id,
                ctx: ExprContext::Load,
                ..
            }) => {
                // If we're within a nested lambda...
                if !self.lambdas.is_empty() {
                    // If the name is defined in the current lambda...
                    if self.parameters.includes(id) {
                        // And isn't overridden by any nested lambdas...
                        if !self.lambdas.iter().any(|parameters| {
                            parameters
                                .as_ref()
                                .is_some_and(|parameters| parameters.includes(id))
                        }) {
                            // Then it's late-bound.
                            self.late_bound = true;
                        }
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_body(&mut self, _body: &'a [Stmt]) {}
}
