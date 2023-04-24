use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Violation};
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

#[violation]
pub struct DictGetWithNoneDefault {
    pub expected: String,
    pub original: String,
}

impl Violation for DictGetWithNoneDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DictGetWithNoneDefault { expected, original } = self;
        format!("Use `{expected}` instead of `{original}`")
    }
}

/// SIM112
pub fn use_capital_environment_variables(checker: &mut Checker, expr: &Expr) {
    // Ex) `os.environ['foo']`
    if let ExprKind::Subscript { .. } = &expr.node {
        check_os_environ_subscript(checker, expr);
        return;
    }

    // Ex) `os.environ.get('foo')`, `os.getenv('foo')`
    let ExprKind::Call { func, args, .. } = &expr.node else {
        return;
    };
    let Some(arg) = args.get(0) else {
        return;
    };
    let ExprKind::Constant { value: Constant::Str(env_var), .. } = &arg.node else {
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

    checker.diagnostics.push(Diagnostic::new(
        UncapitalizedEnvironmentVariables {
            expected: capital_env_var,
            original: env_var.clone(),
        },
        Range::from(arg),
    ));
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
        diagnostic.set_fix(Edit::replacement(
            unparse_expr(&new_env_var, checker.stylist),
            slice.location,
            slice.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM910
pub fn dict_get_with_none_default(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Call { func, args, keywords } = &expr.node else {
        return;
    };
    if !keywords.is_empty() {
        return;
    }
    let ExprKind::Attribute { value, attr, .. } = &func.node else {
        return;
    };
    if !matches!(value.node, ExprKind::Dict { .. }) {
        return;
    }
    if attr != "get" {
        return;
    }
    let Some(key) = args.get(0) else {
        return;
    };
    if !matches!(key.node, ExprKind::Constant { .. } | ExprKind::Name { .. }) {
        return;
    }
    let Some(default) = args.get(1) else {
        return;
    };
    if !matches!(
        default.node,
        ExprKind::Constant {
            value: Constant::None,
            ..
        }
    ) {
        return;
    };

    let expected = format!(
        "{}({})",
        checker.locator.slice(func),
        checker.locator.slice(key)
    );
    let original = checker.locator.slice(expr).to_string();

    let mut diagnostic = Diagnostic::new(
        DictGetWithNoneDefault {
            expected: expected.clone(),
            original,
        },
        Range::from(expr),
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            expected,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
