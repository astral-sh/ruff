use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{
    CmpOp, Expr, ExprCompare, ExprName, ExprSubscript, Stmt, StmtDelete, StmtIf,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

// Real type: Expr::Name;
type Key = Expr;
type Dict = Expr;

/// ## What it does
/// Checks for `if key in dictionary: del dictionary[key]`.
///
/// ## Why is this bad?
/// When removing a key from a dictionary, it is unnecessary to check for its existence.
/// `.pop(..., None)` is simpler and has the same semantic.
///
/// ## Example
///
/// ```python
/// if key in dictionary:
///     del dictionary[key]
/// ```
///
/// Use instead:
///
/// ```python
/// dictionary.pop(key, None)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct IfKeyInDictDel;

impl AlwaysFixableViolation for IfKeyInDictDel {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `pop` instead of `key in dict` followed by `delete dict[key]`".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `.pop(..., None)`".to_string()
    }
}

/// RUF051
pub(crate) fn if_key_in_dict_del(checker: &mut Checker, stmt: &StmtIf) {
    let Some((test_dict, test_key)) = extract_dict_and_key_from_test(&stmt.test) else {
        return;
    };
    let Some((del_dict, del_key)) = extract_dict_and_key_from_del(&stmt.body) else {
        return;
    };

    if !is_same_key(test_key, del_key) || !is_same_dict(test_dict, del_dict) {
        return;
    }

    if !is_known_to_be_of_type_dict(checker.semantic(), test_dict) {
        return;
    }

    let diagnostic = Diagnostic::new(IfKeyInDictDel, stmt.range);
    let Some(fix) = replace_with_dict_pop_fix(checker, stmt, test_dict, test_key) else {
        // This is only reached when the `if` body has no statement,
        // which is impossible as we have already checked for this above.
        return;
    };

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn extract_dict_and_key_from_test(test: &Expr) -> Option<(&Dict, &Key)> {
    let Expr::Compare(ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return None;
    };

    if !matches!(ops.as_ref(), [CmpOp::In]) {
        return None;
    }

    let [right] = comparators.as_ref() else {
        return None;
    };

    dict_and_key_verified(right, left.as_ref())
}

fn extract_dict_and_key_from_del(body: &[Stmt]) -> Option<(&Dict, &Key)> {
    let [Stmt::Delete(StmtDelete { targets, .. })] = body else {
        return None;
    };
    let [Expr::Subscript(ExprSubscript { value, slice, .. })] = &targets[..] else {
        return None;
    };

    dict_and_key_verified(value.as_ref(), slice.as_ref())
}

fn dict_and_key_verified<'d, 'k>(dict: &'d Dict, key: &'k Key) -> Option<(&'d Dict, &'k Key)> {
    if !key.is_name_expr() && !key.is_literal_expr() {
        return None;
    }

    if !dict.is_name_expr() {
        return None;
    }

    Some((dict, key))
}

fn is_same_key(test: &Expr, del: &Expr) -> bool {
    match (test, del) {
        (Expr::Name(..), Expr::Name(..))
        | (Expr::NoneLiteral(..), Expr::NoneLiteral(..))
        | (Expr::EllipsisLiteral(..), Expr::EllipsisLiteral(..))
        | (Expr::BooleanLiteral(..), Expr::BooleanLiteral(..))
        | (Expr::NumberLiteral(..), Expr::NumberLiteral(..))
        | (Expr::BytesLiteral(..), Expr::BytesLiteral(..))
        | (Expr::StringLiteral(..), Expr::StringLiteral(..)) => {
            ComparableExpr::from(test) == ComparableExpr::from(del)
        }

        _ => false,
    }
}

fn is_same_dict(test: &Expr, del: &Expr) -> bool {
    match (test, del) {
        (Expr::Name(ExprName { id: test, .. }), Expr::Name(ExprName { id: del, .. })) => {
            test.as_str() == del.as_str()
        }

        _ => false,
    }
}

fn is_known_to_be_of_type_dict(semantic: &SemanticModel, dict: &Expr) -> bool {
    dict.as_name_expr().is_some_and(|name| {
        let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
            return false;
        };
        typing::is_dict(binding, semantic)
    })
}

fn replace_with_dict_pop_fix(
    checker: &Checker,
    stmt: &StmtIf,
    dict: &Dict,
    key: &Key,
) -> Option<Fix> {
    let locator = checker.locator();
    let dict_expr = locator.slice(dict);
    let key_expr = locator.slice(key);

    let replacement = format!("{dict_expr}.pop({key_expr}, None)");
    let edit = Edit::range_replacement(replacement, stmt.range);

    let test_expr = &stmt.test;
    let del_stmt = stmt.body.first()?;
    let test_to_del = TextRange::new(test_expr.end(), del_stmt.start());

    let comment_ranges = checker.comment_ranges();
    let applicability = if comment_ranges.has_comments(&test_to_del, checker.source()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Some(Fix::applicable_edit(edit, applicability))
}
