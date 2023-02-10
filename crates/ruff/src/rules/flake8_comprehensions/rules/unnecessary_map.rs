use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::rules::flake8_comprehensions::rules::helpers::function_name;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    /// ### What it does
    /// Checks for unnecessary `map` usage.
    ///
    /// ## Why is this bad?
    /// `map(func, iterable)` has great performance when func is a built-in function, and it
    /// makes sense if your function already has a name. But if your func is a lambda, it’s
    /// faster to use a generator expression or a comprehension, as it avoids the function call
    /// overhead. For example:
    ///
    /// Rewrite `map(lambda x: x + 1, iterable)` to `(x + 1 for x in iterable)`
    /// Rewrite `map(lambda item: get_id(item), items)` to `(get_id(item) for item in items)`
    /// Rewrite `list(map(lambda num: num * 2, nums))` to `[num * 2 for num in nums]`
    /// Rewrite `set(map(lambda num: num % 2 == 0, nums))` to `{num % 2 == 0 for num in nums}`
    /// Rewrite `dict(map(lambda v: (v, v ** 2), values))` to `{v : v ** 2 for v in values}`
    pub struct UnnecessaryMap {
        pub obj_type: String,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryMap {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryMap { obj_type } = self;
        if obj_type == "generator" {
            format!("Unnecessary `map` usage (rewrite using a generator expression)")
        } else {
            format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
        }
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryMap { obj_type } = self;
        if obj_type == "generator" {
            format!("Replace `map` using a generator expression")
        } else {
            format!("Replace `map` using a `{obj_type}` comprehension")
        }
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
                    let Some(id_parent) = function_name(f)  else {
                        return;
                    };
                    if id_parent == "dict" || id_parent == "set" || id_parent == "list" {
                        return;
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
