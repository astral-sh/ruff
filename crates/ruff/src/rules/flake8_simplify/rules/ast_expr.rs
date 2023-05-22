use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UncapitalizedEnvironmentVariables {
    expected: String,
    original: String,
}

impl Violation for UncapitalizedEnvironmentVariables {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UncapitalizedEnvironmentVariables { expected, original } = self;
        format!("Use capitalized environment variable `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> Option<String> {
        let UncapitalizedEnvironmentVariables { expected, original } = self;
        Some(format!("Replace `{original}` with `{expected}`"))
    }
}

/// ## What it does
/// Check for `dict.get()` calls that pass `None` as the default value.
///
/// ## Why is this bad?
/// `None` is the default value for `dict.get()`, so it is redundant to pass it
/// explicitly.
///
/// ## Example
/// ```python
/// ages = {"Tom": 23, "Maria": 23, "Dog": 11}
/// age = ages.get("Cat", None)  # None
/// ```
///
/// Use instead:
/// ```python
/// ages = {"Tom": 23, "Maria": 23, "Dog": 11}
/// age = ages.get("Cat")  # None
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/stdtypes.html#dict.get)
#[violation]
pub struct DictGetWithNoneDefault {
    expected: String,
    original: String,
}

impl AlwaysAutofixableViolation for DictGetWithNoneDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DictGetWithNoneDefault { expected, original } = self;
        format!("Use `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> String {
        let DictGetWithNoneDefault { expected, original } = self;
        format!("Replace `{original}` with `{expected}`")
    }
}

/// SIM112
pub(crate) fn use_capital_environment_variables(checker: &mut Checker, expr: &Expr) {
    // Ex) `os.environ['foo']`
    if let Expr::Subscript(_) = expr {
        check_os_environ_subscript(checker, expr);
        return;
    }

    // Ex) `os.environ.get('foo')`, `os.getenv('foo')`
    let Expr::Call(ast::ExprCall { func, args, .. }) = expr else {
        return;
    };
    let Some(arg) = args.get(0) else {
        return;
    };
    let Expr::Constant(ast::ExprConstant { value: Constant::Str(env_var), .. }) = arg else {
        return;
    };
    if !checker
        .semantic_model()
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
        arg.range(),
    ));
}

fn check_os_environ_subscript(checker: &mut Checker, expr: &Expr) {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return;
    };
    let Expr::Attribute(ast::ExprAttribute { value: attr_value, attr, .. }) = value.as_ref() else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = attr_value.as_ref() else {
        return;
    };
    if id != "os" || attr != "environ" {
        return;
    }
    let Expr::Constant(ast::ExprConstant { value: Constant::Str(env_var), kind, range: _ }) = slice.as_ref() else {
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
        slice.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let node = ast::ExprConstant {
            value: capital_env_var.into(),
            kind: kind.clone(),
            range: TextRange::default(),
        };
        let new_env_var = node.into();
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            checker.generator().expr(&new_env_var),
            slice.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM910
pub(crate) fn dict_get_with_none_default(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall { func, args, keywords, range: _ }) = expr else {
        return;
    };
    if !keywords.is_empty() {
        return;
    }
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. } )= func.as_ref() else {
        return;
    };
    if !value.is_dict_expr() {
        return;
    }
    if attr != "get" {
        return;
    }
    let Some(key) = args.get(0) else {
        return;
    };
    if !matches!(key, Expr::Constant(_) | Expr::Name(_)) {
        return;
    }
    let Some(default) = args.get(1) else {
        return;
    };
    if !matches!(
        default,
        Expr::Constant(ast::ExprConstant {
            value: Constant::None,
            ..
        })
    ) {
        return;
    };

    let expected = format!(
        "{}({})",
        checker.locator.slice(func.range()),
        checker.locator.slice(key.range())
    );
    let original = checker.locator.slice(expr.range()).to_string();

    let mut diagnostic = Diagnostic::new(
        DictGetWithNoneDefault {
            expected: expected.clone(),
            original,
        },
        expr.range(),
    );

    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            expected,
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
