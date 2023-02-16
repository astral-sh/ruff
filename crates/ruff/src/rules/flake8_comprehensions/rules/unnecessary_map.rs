use log::error;
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::violation::{AutofixKind, Availability, Violation};

use super::helpers;

define_violation!(
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
    /// * Instead of `list(map(lambda num: num * 2, nums))`, use
    ///   `[num * 2 for num in nums]`.
    /// * Instead of `set(map(lambda num: num % 2 == 0, nums))`, use
    ///   `{num % 2 == 0 for num in nums}`.
    /// * Instead of `dict(map(lambda v: (v, v ** 2), values))`, use
    ///   `{v: v ** 2 for v in values}`.
    pub struct UnnecessaryMap {
        pub obj_type: String,
    }
);
impl Violation for UnnecessaryMap {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

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

    let Some(id) = helpers::function_name(func)  else {
        return;
    };
    match id {
        "map" => {
            if !checker.is_builtin(id) {
                return;
            }

            // Exclude the parent if already matched by other arms
            if let Some(parent) = parent {
                if let ExprKind::Call { func: f, .. } = &parent.node {
                    if let Some(id_parent) = helpers::function_name(f) {
                        if id_parent == "dict" || id_parent == "set" || id_parent == "list" {
                            return;
                        }
                    }
                };
            };

            if args.len() == 2 && matches!(&args[0].node, ExprKind::Lambda { .. }) {
                let mut diagnostic = create_diagnostic("generator", Range::from_located(expr));
                if checker.patch(diagnostic.kind.rule()) {
                    match fixes::fix_unnecessary_map(
                        checker.locator,
                        checker.stylist,
                        expr,
                        parent,
                        "generator",
                    ) {
                        Ok(fix) => {
                            diagnostic.amend(fix);
                        }
                        Err(e) => error!("Failed to generate fix: {e}"),
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        "list" | "set" => {
            if !checker.is_builtin(id) {
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
                        let mut diagnostic = create_diagnostic(id, Range::from_located(expr));
                        if checker.patch(diagnostic.kind.rule()) {
                            match fixes::fix_unnecessary_map(
                                checker.locator,
                                checker.stylist,
                                expr,
                                parent,
                                id,
                            ) {
                                Ok(fix) => {
                                    diagnostic.amend(fix);
                                }
                                Err(e) => error!("Failed to generate fix: {e}"),
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        "dict" => {
            if !checker.is_builtin(id) {
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
                            let mut diagnostic = create_diagnostic(id, Range::from_located(expr));
                            if checker.patch(diagnostic.kind.rule()) {
                                match fixes::fix_unnecessary_map(
                                    checker.locator,
                                    checker.stylist,
                                    expr,
                                    parent,
                                    id,
                                ) {
                                    Ok(fix) => {
                                        diagnostic.amend(fix);
                                    }
                                    Err(e) => error!("Failed to generate fix: {e}"),
                                }
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
