use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UseCapitalEnvironmentVariables {
        pub expected: String,
        pub original: String,
    }
);
impl AlwaysAutofixableViolation for UseCapitalEnvironmentVariables {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UseCapitalEnvironmentVariables { expected, original } = self;
        format!("Use capitalized environment variable `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> String {
        let UseCapitalEnvironmentVariables { expected, original } = self;
        format!("Replace `{original}` with `{expected}`")
    }
}

/// SIM112
pub fn use_capital_environment_variables(checker: &mut Checker, expr: &Expr) {
    // check `os.environ['foo']`
    if let ExprKind::Subscript { .. } = &expr.node {
        check_os_environ_subscript(checker, expr);
        return;
    }

    // check `os.environ.get('foo')` and `os.getenv('foo')`.
    let ExprKind::Call { func, args, .. } = &expr.node else {
        return;
    };
    let Some(arg) = args.get(0) else {
        return;
    };
    let ExprKind::Constant { value: Constant::Str(env_var), kind } = &arg.node else {
        return;
    };
    if !checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["os", "environ", "get"] || call_path.as_slice() == ["os", "getenv"]
    }) {
        return;
    }

    let capital_env_var = env_var.to_ascii_uppercase();
    if &capital_env_var == env_var {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UseCapitalEnvironmentVariables {
            expected: capital_env_var.clone(),
            original: env_var.clone(),
        },
        Range::from_located(arg),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let new_env_var = create_expr(ExprKind::Constant {
            value: capital_env_var.into(),
            kind: kind.clone(),
        });
        diagnostic.amend(Fix::replacement(
            unparse_expr(&new_env_var, checker.stylist),
            arg.location,
            arg.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn check_os_environ_subscript(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Subscript { value, slice, .. } = &expr.node else {
        return;
    };
    let ExprKind::Attribute { value: attr_value, attr, .. } = &value.node else {
        return;
    };
    let ExprKind::Name { id, .. } = &attr_value.node else {
        return;
    };
    if id != "os" || attr != "environ" {
        return;
    }
    let ExprKind::Constant { value: Constant::Str(env_var), kind } = &slice.node else {
        return;
    };
    let capital_env_var = env_var.to_ascii_uppercase();
    if &capital_env_var == env_var {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UseCapitalEnvironmentVariables {
            expected: capital_env_var.clone(),
            original: env_var.clone(),
        },
        Range::from_located(slice),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let new_env_var = create_expr(ExprKind::Constant {
            value: capital_env_var.into(),
            kind: kind.clone(),
        });
        diagnostic.amend(Fix::replacement(
            unparse_expr(&new_env_var, checker.stylist),
            slice.location,
            slice.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
