use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{DictItem, Expr, ExprAttribute, ExprCall, ExprDict};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// Real type: [`Expr::Name`]
type DictRef = Expr;
type Argument = Expr;
type Key = Expr;
type Value = Expr;

/// ## What it does
/// Checks for `dictionary.update({single: item})`.
///
/// ## Why is this bad?
/// Calling `dict.update()` with a single-item dictionary
/// is equivalent to setting the item directly,
/// which is simpler and more concise.
///
/// ## Example
///
/// ```python
/// def foo(d: dict[str, int]) -> None:
///     d.update({"lorem": 42})
/// ```
///
/// Use instead:
///
/// ```python
/// def foo(d: dict[str, int]) -> None:
///     d["lorem"] = 42
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct DictUpdateSingleItemDict;

impl AlwaysFixableViolation for DictUpdateSingleItemDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`dict.update` with single dictionary argument".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with assign statement".to_string()
    }
}

/// RUF042
pub(crate) fn dict_update_single(checker: &mut Checker, expr: &Expr) {
    let semantic = checker.semantic();

    let Some((dict_ref, argument)) = dict_update_call_with_one_argument(semantic, expr) else {
        return;
    };
    let Some((key, value)) = single_item_of_literal_dict(argument) else {
        return;
    };

    let fix = replace_with_assignment_fix(checker, expr, dict_ref, key, value);
    let diagnostic = Diagnostic::new(DictUpdateSingleItemDict, expr.range());

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn dict_update_call_with_one_argument<'a>(
    semantic: &SemanticModel,
    expr: &'a Expr,
) -> Option<(&'a DictRef, &'a Argument)> {
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return None;
    };
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };

    if attr != "update" || !is_known_to_be_of_type_dict(semantic, value) {
        return None;
    }

    if !arguments.keywords.is_empty() {
        return None;
    }

    match &arguments.args[..] {
        [argument] => Some((value, argument)),
        _ => None,
    }
}

fn single_item_of_literal_dict(dict: &Expr) -> Option<(&Key, &Value)> {
    let Expr::Dict(ExprDict { items, .. }) = dict else {
        return None;
    };
    let [DictItem {
        key: Some(key),
        value,
    }] = &items[..]
    else {
        return None;
    };

    Some((key, value))
}

// FIXME: Use function with same name from RUF041
fn is_known_to_be_of_type_dict(semantic: &SemanticModel, expr: &Expr) -> bool {
    expr.as_name_expr().is_some_and(|name| {
        let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
            return false;
        };
        typing::is_dict(binding, semantic)
    })
}

fn replace_with_assignment_fix(
    checker: &Checker,
    expr: &Expr,
    dict_ref: &DictRef,
    key: &Key,
    value: &Value,
) -> Fix {
    let locator = checker.locator();
    let dict_ref_expr = locator.slice(dict_ref);
    let key_expr = locator.slice(key);
    let value_expr = locator.slice(value);

    let new_content = format!("{dict_ref_expr}[{key_expr}] = {value_expr}");
    let edit = Edit::range_replacement(new_content, expr.range());

    let comment_ranges = checker.comment_ranges();
    let applicability = if comment_ranges.has_comments(expr, checker.source()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edit(edit, applicability)
}
