use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, Expr, ExprKind, ExprName, NodeList, StmtIf, StmtKind};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

type Key<'a> = Expr<'a>;
type Dict<'a> = ExprName<'a>;

/// ## What it does
/// Checks for `if key in dictionary: del dictionary[key]`.
///
/// ## Why is this bad?
/// To remove a key-value pair from a dictionary, it's more concise to use `.pop(..., None)`.
///
/// ## Example
///
/// ```python
/// dictionary = {}
/// if key in dictionary:
///     del dictionary[key]
/// ```
///
/// Use instead:
///
/// ```python
/// dictionary = {}
/// dictionary.pop(key, None)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the if statement contains comments.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.10.0", category = Category::Complexity)]
pub(crate) struct IfKeyInDictDel;

impl AlwaysFixableViolation for IfKeyInDictDel {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `pop` instead of `key in dict` followed by `del dict[key]`".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace `if` statement with `.pop(..., None)`".to_string()
    }
}

/// RUF051
pub(crate) fn if_key_in_dict_del(checker: &Checker, stmt: StmtIf<'_>) {
    if !stmt.elif_else_clauses().is_empty() {
        return;
    }

    let Some([body_stmt]) = stmt.body().as_array() else {
        return;
    };
    let StmtKind::Delete(delete) = body_stmt.classify() else {
        return;
    };

    let Some((test_dict, test_key)) = extract_dict_and_key_from_test(stmt.test()) else {
        return;
    };
    let Some((del_dict, del_key)) = extract_dict_and_key_from_del(delete.targets()) else {
        return;
    };

    if !is_same_key(test_key, del_key) || !is_same_dict(test_dict, del_dict) {
        return;
    }

    if !typing::is_known_to_be_of_type_dict(checker.semantic(), test_dict) {
        return;
    }

    let fix = replace_with_dict_pop_fix(checker, stmt, test_dict, test_key);

    checker
        .report_diagnostic(IfKeyInDictDel, delete.range())
        .set_fix(fix);
}

fn extract_dict_and_key_from_test(test: Expr<'_>) -> Option<(Dict<'_>, Key<'_>)> {
    let ExprKind::Compare(comp) = test.classify() else {
        return None;
    };

    let Some([key, ExprKind::Name(dict)]) = comp
        .operands()
        .as_array()
        .map(|items| items.map(Expr::classify))
    else {
        return None;
    };

    if !matches!(comp.ops().as_array(), Some([CmpOp::In])) {
        return None;
    }

    Some((dict, key.view()))
}

fn extract_dict_and_key_from_del(
    targets: NodeList<'_, ruff_python_ast::kind::Expr>,
) -> Option<(Dict<'_>, Key<'_>)> {
    let Some([ast::ExprFields::Subscript(ast::ExprSubscriptFields { value, slice, .. })]) =
        targets.as_array().map(|items| items.map(Expr::fields))
    else {
        return None;
    };

    let ExprKind::Name(dict) = value.classify() else {
        return None;
    };

    Some((dict, slice))
}

fn is_same_key(test: Key<'_>, del: Key<'_>) -> bool {
    match (test.fields(), del.fields()) {
        (
            ast::ExprFields::Name(ast::ExprNameFields { id: test, .. }),
            ast::ExprFields::Name(ast::ExprNameFields { id: del, .. }),
        ) => test.as_str() == del.as_str(),

        (ast::ExprFields::NoneLiteral(..), ast::ExprFields::NoneLiteral(..)) => true,
        (ast::ExprFields::EllipsisLiteral(..), ast::ExprFields::EllipsisLiteral(..)) => true,

        (
            ast::ExprFields::BooleanLiteral(ast::ExprBooleanLiteralFields { node: test, .. }),
            ast::ExprFields::BooleanLiteral(ast::ExprBooleanLiteralFields { node: del, .. }),
        ) => test.value() == del.value(),
        (
            ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields { node: test, .. }),
            ast::ExprFields::NumberLiteral(ast::ExprNumberLiteralFields { node: del, .. }),
        ) => test.value() == del.value(),

        (
            ast::ExprFields::BytesLiteral(ast::ExprBytesLiteralFields { node: test, .. }),
            ast::ExprFields::BytesLiteral(ast::ExprBytesLiteralFields { node: del, .. }),
        ) => Iterator::eq(test.value().bytes(), del.value().bytes()),

        (
            ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields { node: test, .. }),
            ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields { node: del, .. }),
        ) => Iterator::eq(test.value().chars(), del.value().chars()),

        _ => false,
    }
}

fn is_same_dict(test: Dict<'_>, del: Dict<'_>) -> bool {
    test.id().as_str() == del.id().as_str()
}

fn replace_with_dict_pop_fix(
    checker: &Checker,
    stmt: StmtIf<'_>,
    dict: Dict<'_>,
    key: Key<'_>,
) -> Fix {
    let locator = checker.locator();
    let dict_expr = locator.slice(dict);
    let key_expr = locator.slice(key);

    let replacement = format!("{dict_expr}.pop({key_expr}, None)");
    let edit = Edit::range_replacement(replacement, stmt.range());

    let comment_ranges = checker.comment_ranges();
    let applicability = if comment_ranges.intersects(stmt.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edit(edit, applicability)
}
