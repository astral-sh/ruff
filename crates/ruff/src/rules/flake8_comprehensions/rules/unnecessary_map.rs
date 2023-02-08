use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UnnecessaryMap {
        pub obj_type: String,
    }
);
impl Violation for UnnecessaryMap {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryMap { obj_type } = self;
        if obj_type == "generator" {
            format!("Unnecessary `map` usage (rewrite using a generator expression)")
        } else {
            format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
        }
    }
}

/// C417
pub fn unnecessary_map(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    fn diagnostic(kind: &str, location: Range) -> Diagnostic {
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

            if args.len() == 2 && matches!(&args[0].node, ExprKind::Lambda { .. }) {
                checker
                    .diagnostics
                    .push(diagnostic("generator", Range::from_located(expr)));
            }
        }
        "list" | "set" => {
            if !checker.is_builtin(id) {
                return;
            }

            if let Some(arg) = args.first() {
                if let ExprKind::Call { func, args, .. } = &arg.node {
                    let Some(argument) = helpers::first_argument_with_matching_function("map", func, args) else {
                        return;
                    };
                    if let ExprKind::Lambda { .. } = argument {
                        checker
                            .diagnostics
                            .push(diagnostic(id, Range::from_located(expr)));
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
                            checker
                                .diagnostics
                                .push(diagnostic(id, Range::from_located(expr)));
                        }
                    }
                }
            }
        }
        _ => (),
    }
}
