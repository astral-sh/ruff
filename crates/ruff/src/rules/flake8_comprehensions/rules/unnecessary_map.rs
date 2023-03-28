use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AutofixKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
    pub obj_type: String,
}

impl Violation for UnnecessaryMap {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryMap { obj_type } = self;
        if obj_type == "generator" {
            format!("Unnecessary `map` usage (rewrite using a generator expression)")
        } else {
            format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|UnnecessaryMap { obj_type }| {
            if obj_type == "generator" {
                format!("Replace `map` using a generator expression")
            } else {
                format!("Replace `map` using a `{obj_type}` comprehension")
            }
        })
    }
}

/// C417
pub fn unnecessary_map(
    checker: &mut Checker,
    expr: &Expr,
    parent: Option<&Expr>,
    func: &Expr,
    args: &[Expr],
) {
    fn create_diagnostic(kind: &str, location: Range) -> Diagnostic {
        Diagnostic::new(
            UnnecessaryMap {
                obj_type: kind.to_string(),
            },
            location,
        )
    }

    let Some(id) = helpers::expr_name(func)  else {
        return;
    };
    match id {
        "map" => {
            if !checker.ctx.is_builtin(id) {
                return;
            }

            // Exclude the parent if already matched by other arms
            if let Some(parent) = parent {
                if let ExprKind::Call { func: f, .. } = &parent.node {
                    if let Some(id_parent) = helpers::expr_name(f) {
                        if id_parent == "dict" || id_parent == "set" || id_parent == "list" {
                            return;
                        }
                    }
                };
            };

            if args.len() == 2 && matches!(&args[0].node, ExprKind::Lambda { .. }) {
                let mut diagnostic = create_diagnostic("generator", Range::from(expr));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.try_set_fix(|| {
                        fixes::fix_unnecessary_map(
                            checker.locator,
                            checker.stylist,
                            expr,
                            parent,
                            "generator",
                        )
                    });
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        "list" | "set" => {
            if !checker.ctx.is_builtin(id) {
                return;
            }

            if let Some(arg) = args.first() {
                if let ExprKind::Call { func, args, .. } = &arg.node {
                    if args.len() != 2 {
                        return;
                    }
                    let Some(argument) = helpers::first_argument_with_matching_function("map", func, args) else {
                        return;
                    };
                    if let ExprKind::Lambda { .. } = argument {
                        let mut diagnostic = create_diagnostic(id, Range::from(expr));
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.try_set_fix(|| {
                                fixes::fix_unnecessary_map(
                                    checker.locator,
                                    checker.stylist,
                                    expr,
                                    parent,
                                    id,
                                )
                            });
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        "dict" => {
            if !checker.ctx.is_builtin(id) {
                return;
            }

            if args.len() == 1 {
                if let ExprKind::Call { func, args, .. } = &args[0].node {
                    let Some(argument) = helpers::first_argument_with_matching_function("map", func, args) else {
                        return;
                    };
                    if let ExprKind::Lambda { body, .. } = &argument {
                        if matches!(&body.node, ExprKind::Tuple { elts, .. } | ExprKind::List { elts, .. } if elts.len() == 2)
                        {
                            let mut diagnostic = create_diagnostic(id, Range::from(expr));
                            if checker.patch(diagnostic.kind.rule()) {
                                diagnostic.try_set_fix(|| {
                                    fixes::fix_unnecessary_map(
                                        checker.locator,
                                        checker.stylist,
                                        expr,
                                        parent,
                                        id,
                                    )
                                });
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
            }
        }
        _ => (),
    }
}
