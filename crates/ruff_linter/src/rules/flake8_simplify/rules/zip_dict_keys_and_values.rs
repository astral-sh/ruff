use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::{Arguments, Expr};
use ruff_python_ast::{ExprCall, ExprName, Identifier};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_text_size::Ranged;

use crate::codes::Category;
use crate::fix::edits;
use crate::{AlwaysFixableViolation, Edit, Fix};
use crate::{checkers::ast::Checker, fix::snippet::SourceCodeSnippet};

/// ## What it does
/// Checks for use of `zip()` to iterate over keys and values of a dictionary at once.
///
/// ## Why is this bad?
/// The `dict` type provides an `.items()` method which is faster and more readable.
///
/// ## Example
/// ```python
/// flag_stars = {"USA": 50, "Slovenia": 3, "Panama": 2, "Australia": 6}
///
/// for country, stars in zip(flag_stars.keys(), flag_stars.values()):
///     print(f"{country}'s flag has {stars} stars.")
/// ```
///
/// Use instead:
/// ```python
/// flag_stars = {"USA": 50, "Slovenia": 3, "Panama": 2, "Australia": 6}
///
/// for country, stars in flag_stars.items():
///     print(f"{country}'s flag has {stars} stars.")
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the expression contains comments.
///
/// ## References
/// - [Python documentation: `dict.items`](https://docs.python.org/3/library/stdtypes.html#dict.items)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0", category = Category::Complexity)]
pub(crate) struct ZipDictKeysAndValues {
    expected: SourceCodeSnippet,
    actual: SourceCodeSnippet,
}

impl AlwaysFixableViolation for ZipDictKeysAndValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ZipDictKeysAndValues { expected, actual } = self;
        if let (Some(expected), Some(actual)) = (expected.full_display(), actual.full_display()) {
            format!("Use `{expected}` instead of `{actual}`")
        } else {
            "Use `dict.items()` instead of `zip(dict.keys(), dict.values())`".to_string()
        }
    }

    fn fix_title(&self) -> String {
        let ZipDictKeysAndValues { expected, actual } = self;
        if let (Some(expected), Some(actual)) = (expected.full_display(), actual.full_display()) {
            format!("Replace `{actual}` with `{expected}`")
        } else {
            "Replace `zip(dict.keys(), dict.values())` with `dict.items()`".to_string()
        }
    }
}

/// SIM911
pub(crate) fn zip_dict_keys_and_values(checker: &Checker, expr: flat::View<'_, ExprCall>) {
    let func = expr.func();
    let arguments = expr.arguments();
    let args = arguments.args();
    let keywords = arguments.keywords();
    match keywords.slice(..) {
        keywords if keywords.is_empty() => {}
        keywords
            if let Some(
                [
                    ast::Keyword {
                        arg: Some(name), ..
                    },
                ],
            ) = keywords.as_array().map(|[keyword]| [keyword.fields()])
                && name.as_str() == "strict" => {}
        _ => return,
    }
    let Some([arg1, arg2]) = args.slice(..).as_array() else {
        return;
    };
    let Some((var1, attr1, args1)) = get_var_attr_args(arg1) else {
        return;
    };
    let Some((var2, attr2, args2)) = get_var_attr_args(arg2) else {
        return;
    };
    if var1.id() != var2.id()
        || attr1 != "keys"
        || attr2 != "values"
        || !args1.is_empty()
        || !args2.is_empty()
    {
        return;
    }
    if !checker.semantic().match_builtin_expr(func, "zip") {
        return;
    }

    let Some(binding) = checker
        .semantic()
        .resolve_name(var1)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_dict(binding, checker.semantic()) {
        return;
    }

    let expected = edits::pad(
        format!("{}.items()", checker.locator().slice(var1)),
        expr.range(),
        checker.locator(),
    );
    let actual = checker.locator().slice(expr);

    let mut diagnostic = checker.report_diagnostic(
        ZipDictKeysAndValues {
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

fn get_var_attr_args(
    expr: flat::View<'_, Expr>,
) -> Option<(
    flat::View<'_, ExprName>,
    flat::View<'_, Identifier>,
    flat::View<'_, Arguments>,
)> {
    let flat::ExprKind::Call(call) = expr.classify() else {
        return None;
    };
    let func = call.func();
    let arguments = call.arguments();
    let flat::ExprKind::Attribute(attribute) = func.classify() else {
        return None;
    };
    let value = attribute.value();
    let attr = attribute.attr();
    let flat::ExprKind::Name(var_name) = value.classify() else {
        return None;
    };
    Some((var_name, attr, arguments))
}
