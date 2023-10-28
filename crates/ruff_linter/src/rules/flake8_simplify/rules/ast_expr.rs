use ruff_python_ast::{self as ast, Arguments, Constant, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::fix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for environment variables that are not capitalized.
///
/// ## Why is this bad?
/// By convention, environment variables should be capitalized.
///
/// On Windows, environment variables are case-insensitive and are converted to
/// uppercase, so using lowercase environment variables can lead to subtle bugs.
///
/// ## Example
/// ```python
/// import os
///
/// os.environ["foo"]
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// os.environ["FOO"]
/// ```
///
/// ## References
/// - [Python documentation: `os.environ`](https://docs.python.org/3/library/os.html#os.environ)
#[violation]
pub struct UncapitalizedEnvironmentVariables {
    expected: SourceCodeSnippet,
    actual: SourceCodeSnippet,
}

impl Violation for UncapitalizedEnvironmentVariables {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UncapitalizedEnvironmentVariables { expected, actual } = self;
        if let (Some(expected), Some(actual)) = (expected.full_display(), actual.full_display()) {
            format!("Use capitalized environment variable `{expected}` instead of `{actual}`")
        } else {
            format!("Use capitalized environment variable")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UncapitalizedEnvironmentVariables { expected, actual } = self;
        if let (Some(expected), Some(actual)) = (expected.full_display(), actual.full_display()) {
            Some(format!("Replace `{actual}` with `{expected}`"))
        } else {
            Some("Capitalize environment variable".to_string())
        }
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
/// - [Python documentation: `dict.get`](https://docs.python.org/3/library/stdtypes.html#dict.get)
#[violation]
pub struct DictGetWithNoneDefault {
    expected: SourceCodeSnippet,
    actual: SourceCodeSnippet,
}

impl AlwaysFixableViolation for DictGetWithNoneDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DictGetWithNoneDefault { expected, actual } = self;
        if let (Some(expected), Some(actual)) = (expected.full_display(), actual.full_display()) {
            format!("Use `{expected}` instead of `{actual}`")
        } else {
            format!("Use `dict.get()` without default value")
        }
    }

    fn fix_title(&self) -> String {
        let DictGetWithNoneDefault { expected, actual } = self;
        if let (Some(expected), Some(actual)) = (expected.full_display(), actual.full_display()) {
            format!("Replace `{actual}` with `{expected}`")
        } else {
            "Remove default value".to_string()
        }
    }
}

/// Returns whether the given environment variable is allowed to be lowercase.
///
/// References:
/// - <https://unix.stackexchange.com/a/212972/>
/// - <https://about.gitlab.com/blog/2021/01/27/we-need-to-talk-no-proxy/#http_proxy-and-https_proxy/>
fn is_lowercase_allowed(env_var: &str) -> bool {
    matches!(env_var, "https_proxy" | "http_proxy" | "no_proxy")
}

/// SIM112
pub(crate) fn use_capital_environment_variables(checker: &mut Checker, expr: &Expr) {
    // Ex) `os.environ['foo']`
    if let Expr::Subscript(_) = expr {
        check_os_environ_subscript(checker, expr);
        return;
    }

    // Ex) `os.environ.get('foo')`, `os.getenv('foo')`
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    }) = expr
    else {
        return;
    };
    let Some(arg) = args.get(0) else {
        return;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(ast::StringConstant { value: env_var, .. }),
        ..
    }) = arg
    else {
        return;
    };
    if !checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["os", "environ", "get"] | ["os", "getenv"]
            )
        })
    {
        return;
    }

    if is_lowercase_allowed(env_var) {
        return;
    }

    let capital_env_var = env_var.to_ascii_uppercase();
    if &capital_env_var == env_var {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        UncapitalizedEnvironmentVariables {
            expected: SourceCodeSnippet::new(capital_env_var),
            actual: SourceCodeSnippet::new(env_var.clone()),
        },
        arg.range(),
    ));
}

fn check_os_environ_subscript(checker: &mut Checker, expr: &Expr) {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return;
    };
    let Expr::Attribute(ast::ExprAttribute {
        value: attr_value,
        attr,
        ..
    }) = value.as_ref()
    else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = attr_value.as_ref() else {
        return;
    };
    if id != "os" || attr != "environ" {
        return;
    }
    let Expr::Constant(ast::ExprConstant {
        value:
            Constant::Str(ast::StringConstant {
                value: env_var,
                unicode,
                ..
            }),
        range: _,
    }) = slice.as_ref()
    else {
        return;
    };

    if is_lowercase_allowed(env_var) {
        return;
    }

    let capital_env_var = env_var.to_ascii_uppercase();
    if &capital_env_var == env_var {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UncapitalizedEnvironmentVariables {
            expected: SourceCodeSnippet::new(capital_env_var.clone()),
            actual: SourceCodeSnippet::new(env_var.clone()),
        },
        slice.range(),
    );
    let node = ast::ExprConstant {
        value: ast::Constant::Str(ast::StringConstant {
            value: capital_env_var,
            unicode: *unicode,
            implicit_concatenated: false,
        }),
        range: TextRange::default(),
    };
    let new_env_var = node.into();
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(&new_env_var),
        slice.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// SIM910
pub(crate) fn dict_get_with_none_default(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        range: _,
    }) = expr
    else {
        return;
    };
    if !keywords.is_empty() {
        return;
    }
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
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
    if !is_const_none(default) {
        return;
    }

    let expected = format!(
        "{}({})",
        checker.locator().slice(func.as_ref()),
        checker.locator().slice(key)
    );
    let actual = checker.locator().slice(expr);

    let mut diagnostic = Diagnostic::new(
        DictGetWithNoneDefault {
            expected: SourceCodeSnippet::new(expected.clone()),
            actual: SourceCodeSnippet::from_str(actual),
        },
        expr.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        expected,
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
