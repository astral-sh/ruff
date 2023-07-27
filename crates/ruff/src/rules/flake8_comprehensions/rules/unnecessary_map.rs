use std::fmt;

use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Ranged, Stmt};

use ruff_diagnostics::{AutofixKind, Violation};
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::includes_arg_name;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary `map` calls with `lambda` functions.
///
/// ## Why is this bad?
/// Using `map(func, iterable)` when `func` is a `lambda` is slower than
/// using a generator expression or a comprehension, as the latter approach
/// avoids the function call overhead, in addition to being more readable.
///
/// ## Examples
/// ```python
/// map(lambda x: x + 1, iterable)
/// ```
///
/// Use instead:
/// ```python
/// (x + 1 for x in iterable)
/// ```
///
/// This rule also applies to `map` calls within `list`, `set`, and `dict`
/// calls. For example:
///
/// - Instead of `list(map(lambda num: num * 2, nums))`, use
///   `[num * 2 for num in nums]`.
/// - Instead of `set(map(lambda num: num % 2 == 0, nums))`, use
///   `{num % 2 == 0 for num in nums}`.
/// - Instead of `dict(map(lambda v: (v, v ** 2), values))`, use
///   `{v: v ** 2 for v in values}`.
#[violation]
pub struct UnnecessaryMap {
    object_type: ObjectType,
}

impl Violation for UnnecessaryMap {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryMap { object_type } = self;
        format!("Unnecessary `map` usage (rewrite using a {object_type})")
    }

    fn autofix_title(&self) -> Option<String> {
        let UnnecessaryMap { object_type } = self;
        Some(format!("Replace `map` with a {object_type}"))
    }
}

/// C417
pub(crate) fn unnecessary_map(
    checker: &mut Checker,
    expr: &Expr,
    parent: Option<&Expr>,
    func: &Expr,
    args: &[Expr],
) {
    let Some(id) = helpers::expr_name(func) else {
        return;
    };

    let object_type = match id {
        "map" => ObjectType::Generator,
        "list" => ObjectType::List,
        "set" => ObjectType::Set,
        "dict" => ObjectType::Dict,
        _ => return,
    };

    if !checker.semantic().is_builtin(id) {
        return;
    }

    match object_type {
        ObjectType::Generator => {
            // Exclude the parent if already matched by other arms.
            if let Some(Expr::Call(ast::ExprCall { func, .. })) = parent {
                if let Some(name) = helpers::expr_name(func) {
                    if matches!(name, "list" | "set" | "dict") {
                        return;
                    }
                }
            };

            // Only flag, e.g., `map(lambda x: x + 1, iterable)`.
            let [Expr::Lambda(ast::ExprLambda { args, body, .. }), _] = args else {
                return;
            };

            if late_binding(args, body) {
                return;
            }
        }
        ObjectType::List | ObjectType::Set => {
            // Only flag, e.g., `list(map(lambda x: x + 1, iterable))`.
            let [Expr::Call(ast::ExprCall { func, args, .. })] = args else {
                return;
            };

            if args.len() != 2 {
                return;
            }

            let Some(argument) = helpers::first_argument_with_matching_function("map", func, args)
            else {
                return;
            };

            let Expr::Lambda(ast::ExprLambda { args, body, .. }) = argument else {
                return;
            };

            if late_binding(args, body) {
                return;
            }
        }
        ObjectType::Dict => {
            // Only flag, e.g., `dict(map(lambda v: (v, v ** 2), values))`.
            let [Expr::Call(ast::ExprCall { func, args, .. })] = args else {
                return;
            };

            let Some(argument) = helpers::first_argument_with_matching_function("map", func, args)
            else {
                return;
            };

            let Expr::Lambda(ast::ExprLambda { args, body, .. }) = argument else {
                return;
            };

            let (Expr::Tuple(ast::ExprTuple { elts, .. }) | Expr::List(ast::ExprList { elts, .. })) =
                body.as_ref()
            else {
                return;
            };

            if elts.len() != 2 {
                return;
            }

            if late_binding(args, body) {
                return;
            }
        }
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryMap { object_type }, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_map(
                checker.locator(),
                checker.stylist(),
                expr,
                parent,
                object_type,
            )
            .map(Fix::suggested)
        });
    }
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ObjectType {
    Generator,
    List,
    Set,
    Dict,
}

impl fmt::Display for ObjectType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ObjectType::Generator => fmt.write_str("generator expression"),
            ObjectType::List => fmt.write_str("`list` comprehension"),
            ObjectType::Set => fmt.write_str("`set` comprehension"),
            ObjectType::Dict => fmt.write_str("`dict` comprehension"),
        }
    }
}

/// Returns `true` if the lambda defined by the given arguments and body contains any names that
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
fn late_binding(args: &Arguments, body: &Expr) -> bool {
    let mut visitor = LateBindingVisitor::new(args);
    visitor.visit_expr(body);
    visitor.late_bound
}

#[derive(Debug)]
struct LateBindingVisitor<'a> {
    /// The arguments to the current lambda.
    args: &'a Arguments,
    /// The arguments to any lambdas within the current lambda body.
    lambdas: Vec<&'a Arguments>,
    /// Whether any names within the current lambda body are late-bound within nested lambdas.
    late_bound: bool,
}

impl<'a> LateBindingVisitor<'a> {
    fn new(args: &'a Arguments) -> Self {
        Self {
            args,
            lambdas: Vec::new(),
            late_bound: false,
        }
    }
}

impl<'a> Visitor<'a> for LateBindingVisitor<'a> {
    fn visit_stmt(&mut self, _stmt: &'a Stmt) {}

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ast::ExprLambda { args, .. }) => {
                self.lambdas.push(args);
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
                    if includes_arg_name(id, self.args) {
                        // And isn't overridden by any nested lambdas...
                        if !self.lambdas.iter().any(|args| includes_arg_name(id, args)) {
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
