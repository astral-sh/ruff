use ast::{ExprAttribute, ExprName, Identifier};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_text_size::Ranged;

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
/// ## References
/// - [Python documentation: `dict.items`](https://docs.python.org/3/library/stdtypes.html#dict.items)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0")]
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
pub(crate) fn zip_dict_keys_and_values(checker: &Checker, expr: &ast::ExprCall) {
    let ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        ..
    } = expr;
    match &keywords[..] {
        [] => {}
        [
            ast::Keyword {
                arg: Some(name), ..
            },
        ] if name.as_str() == "strict" => {}
        _ => return,
    }
    let [arg1, arg2] = &args[..] else {
        return;
    };
    let Some((var1, attr1, args1)) = get_var_attr_args(arg1) else {
        return;
    };
    let Some((var2, attr2, args2)) = get_var_attr_args(arg2) else {
        return;
    };
    if var1.id != var2.id
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
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        expected,
        expr.range(),
    )));
}

fn get_var_attr_args(expr: &Expr) -> Option<(&ExprName, &Identifier, &Arguments)> {
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return None;
    };
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };
    let Expr::Name(var_name) = value.as_ref() else {
        return None;
    };
    Some((var_name, attr, arguments))
}
