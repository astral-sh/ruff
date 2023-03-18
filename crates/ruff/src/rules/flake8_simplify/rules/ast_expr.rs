use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UncapitalizedEnvironmentVariables {
    pub expected: String,
    pub original: String,
}

impl AlwaysAutofixableViolation for UncapitalizedEnvironmentVariables {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UncapitalizedEnvironmentVariables { expected, original } = self;
        format!("Use capitalized environment variable `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> String {
        let UncapitalizedEnvironmentVariables { expected, original } = self;
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
    if !checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["os", "environ", "get"]
                || call_path.as_slice() == ["os", "getenv"]
        })
    {
        return;
    }

    let capital_env_var = env_var.to_ascii_uppercase();
    if &capital_env_var == env_var {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UncapitalizedEnvironmentVariables {
            expected: capital_env_var.clone(),
            original: env_var.clone(),
        },
        Range::from(arg),
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
        UncapitalizedEnvironmentVariables {
            expected: capital_env_var.clone(),
            original: env_var.clone(),
        },
        Range::from(slice),
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
