use ruff_diagnostics::Applicability;
use ruff_python_ast::flat;
use ruff_python_ast::{self as ast, Expr, str_prefix::StringLiteralPrefix};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::Modules;
use ruff_python_semantic::analyze::typing::is_dict;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{AlwaysFixableViolation, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Check for environment variables that are not capitalized.
///
/// ## Why is this bad?
/// By convention, environment variables should be capitalized.
///
/// Furthermore, `os.environ` behaves differently across platforms. On Windows,
/// `os.environ` automatically converts environment variable names to uppercase.
/// This means that if you define a lowercase environment variable (e.g., `foo=1`),
/// iterating over `os.environ` will yield `FOO` on Windows, but
/// `foo` on Linux and macOS. This can lead to subtle bugs in cross-platform code
/// if it assumes environment variables preserve their original case.
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
/// ## Fix safety
///
/// This fix is always marked as unsafe because automatically capitalizing environment variable names
/// can change program behavior in environments where the variable names are case-sensitive, such as most
/// Unix-like systems.
///
/// ## References
/// - [Python documentation: `os.environ`](https://docs.python.org/3/library/os.html#os.environ)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.218", category = Category::Pedantic)]
pub(crate) struct UncapitalizedEnvironmentVariables {
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
            "Use capitalized environment variable".to_string()
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
/// Checks for `dict.get()` calls that pass `None` as the default value.
///
/// ## Why is this bad?
/// `None` is the default value for `dict.get()`, so it is redundant to pass it
/// explicitly.
///
/// ## Example
/// ```python
/// ages = {"Tom": 23, "Maria": 23, "Dog": 11}
/// age = ages.get("Cat", None)
/// ```
///
/// Use instead:
/// ```python
/// ages = {"Tom": 23, "Maria": 23, "Dog": 11}
/// age = ages.get("Cat")
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the expression contains comments.
///
/// ## References
/// - [Python documentation: `dict.get`](https://docs.python.org/3/library/stdtypes.html#dict.get)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.261", category = Category::Pedantic)]
pub(crate) struct DictGetWithNoneDefault {
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
            "Use `dict.get()` without default value".to_string()
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
pub(crate) fn use_capital_environment_variables(checker: &Checker, &expr: &flat::View<'_, Expr>) {
    if !checker.semantic().seen_module(Modules::OS) {
        return;
    }

    // Ex) `os.environ['foo']`
    if let flat::ExprKind::Subscript(_) = expr.classify() {
        check_os_environ_subscript(checker, expr);
        return;
    }

    // Ex) `os.environ.get('foo')`, `os.getenv('foo')`
    let flat::ExprKind::Call(call) = expr.classify() else {
        return;
    };
    let func = call.func();
    let arguments = call.arguments();
    let args = arguments.args();
    let Some(arg) = args.first() else {
        return;
    };
    let flat::ExprKind::StringLiteral(string_literal) = arg.classify() else {
        return;
    };
    let env_var = string_literal.value();
    if !checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["os", "environ", "get"] | ["os", "getenv"]
            )
        })
    {
        return;
    }

    if is_lowercase_allowed(env_var.to_str()) {
        return;
    }

    let capital_env_var = env_var.to_str().to_ascii_uppercase();
    if capital_env_var == env_var.to_str() {
        return;
    }

    checker.report_diagnostic(
        UncapitalizedEnvironmentVariables {
            expected: SourceCodeSnippet::new(capital_env_var),
            actual: SourceCodeSnippet::new(env_var.to_string()),
        },
        arg.range(),
    );
}

fn check_os_environ_subscript(checker: &Checker, expr: flat::View<'_, Expr>) {
    let flat::ExprKind::Subscript(subscript) = expr.classify() else {
        return;
    };
    let value = subscript.value();
    let slice = subscript.slice();
    let flat::ExprKind::Attribute(attribute) = value.classify() else {
        return;
    };
    let attr_value = attribute.value();
    let attr = attribute.attr();
    let flat::ExprKind::Name(name_node) = attr_value.classify() else {
        return;
    };
    let id = name_node.id();
    if id != "os" || attr != "environ" {
        return;
    }
    let flat::ExprKind::StringLiteral(string_literal) = slice.classify() else {
        return;
    };
    let env_var = string_literal.value();

    if is_lowercase_allowed(env_var.to_str()) {
        return;
    }

    let capital_env_var = env_var.to_str().to_ascii_uppercase();
    if capital_env_var == env_var.to_str() {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        UncapitalizedEnvironmentVariables {
            expected: SourceCodeSnippet::new(capital_env_var.clone()),
            actual: SourceCodeSnippet::new(env_var.to_string()),
        },
        slice.range(),
    );
    let node = ast::StringLiteral {
        value: capital_env_var.into_boxed_str(),
        flags: checker.default_string_flags().with_prefix({
            if env_var.is_unicode() {
                StringLiteralPrefix::Unicode
            } else {
                StringLiteralPrefix::Empty
            }
        }),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    };
    let new_env_var = node.into();
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(&new_env_var),
        slice.range(),
    )));
}

/// SIM910
pub(crate) fn dict_get_with_none_default(checker: &Checker, &expr: &flat::View<'_, Expr>) {
    let flat::ExprKind::Call(call) = expr.classify() else {
        return;
    };
    let func = call.func();
    let arguments = call.arguments();
    let args = arguments.args();
    let keywords = arguments.keywords();
    if !keywords.is_empty() {
        return;
    }
    let flat::ExprKind::Attribute(attribute) = func.classify() else {
        return;
    };
    let value = attribute.value();
    let attr = attribute.attr();
    if attr != "get" {
        return;
    }
    let Some(key) = args.first() else {
        return;
    };
    let Some(default) = args.get(1) else {
        return;
    };
    if !default.is_none_literal_expr() {
        return;
    }

    // Check if the value is a dictionary.
    match value.classify() {
        flat::ExprKind::Dict(_) | flat::ExprKind::DictComp(_) => {}
        flat::ExprKind::Name(name) => {
            let Some(binding) = checker
                .semantic()
                .resolve_name(name)
                .map(|id| checker.semantic().binding(id))
            else {
                return;
            };
            if !is_dict(binding, checker.semantic()) {
                return;
            }
        }
        _ => return,
    }

    let expected = format!(
        "{}({})",
        checker.locator().slice(func),
        checker.locator().slice(key)
    );
    let actual = checker.locator().slice(expr);

    let mut diagnostic = checker.report_diagnostic(
        DictGetWithNoneDefault {
            expected: SourceCodeSnippet::new(expected.clone()),
            actual: SourceCodeSnippet::from_str(actual),
        },
        expr.range(),
    );

    let applicability = if checker.comment_ranges().intersects(expr.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    diagnostic.set_fix(Fix::applicable_edit(
        Edit::range_replacement(expected, expr.range()),
        applicability,
    ));
}
