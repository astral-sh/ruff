use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    DictItem, Expr, ExprAttribute, ExprCall, ExprDict, ExprList, ExprSet, ExprTuple,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

/// Real type: [`Expr::Name`]
type ListRef = Expr;
type Argument = Expr;
type Item = Expr;

/// ## What it does
/// Checks for `list.extend(["single item"])`.
///
/// ## Why is this bad?
/// Calling `list.extend()` with a single-item iterable
/// is equivalent to appending the item directly.
///
/// ## Example
///
/// ```python
/// def foo(l: list[str]) -> None:
///     l.extend(["lorem"])
/// ```
///
/// Use instead:
///
/// ```python
/// def foo(l: list[str]) -> None:
///     l.append("lorem")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ListExtendSingleItemIterable;

impl AlwaysFixableViolation for ListExtendSingleItemIterable {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Literal iterable with single item in `list.extend()` call".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `.append()`".to_string()
    }
}

/// RUF043
pub(crate) fn list_extend_single(checker: &mut Checker, expr: &Expr) {
    let semantic = checker.semantic();

    let Some((list_ref, argument)) = list_extend_call_with_one_argument(semantic, expr) else {
        return;
    };
    let Some(item) = single_item_of_literal_iterable(argument) else {
        return;
    };

    let fix = replace_with_append_fix(checker, expr, list_ref, item);
    let diagnostic = Diagnostic::new(ListExtendSingleItemIterable, expr.range());

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn list_extend_call_with_one_argument<'a>(
    semantic: &SemanticModel,
    expr: &'a Expr,
) -> Option<(&'a ListRef, &'a Argument)> {
    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return None;
    };
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };

    if attr != "extend" || !is_known_to_be_of_type_list(semantic, value) {
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

fn single_item_of_literal_iterable(iterable: &Expr) -> Option<&Item> {
    match iterable {
        Expr::Dict(ExprDict { items, .. }) => match &items[..] {
            [DictItem { key: Some(key), .. }] => Some(key),
            _ => None,
        },

        Expr::Set(ExprSet { elts, .. })
        | Expr::List(ExprList { elts, .. })
        | Expr::Tuple(ExprTuple { elts, .. }) => match &elts[..] {
            [Expr::Starred(..)] => None,
            [item] => Some(item),
            _ => None,
        },

        _ => None,
    }
}

fn is_known_to_be_of_type_list(semantic: &SemanticModel, expr: &Expr) -> bool {
    expr.as_name_expr().is_some_and(|name| {
        let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
            return false;
        };
        typing::is_list(binding, semantic)
    })
}

fn replace_with_append_fix(
    checker: &mut Checker,
    expr: &Expr,
    list_ref: &ListRef,
    item: &Item,
) -> Fix {
    let locator = checker.locator();
    let list_ref_expr = locator.slice(list_ref);
    let item_expr = locator.slice(item);

    let new_content = format!("{list_ref_expr}.append({item_expr})");
    let edit = Edit::range_replacement(new_content, expr.range());

    let comment_ranges = checker.comment_ranges();
    let applicability = if comment_ranges.has_comments(expr, checker.source()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edit(edit, applicability)
}
