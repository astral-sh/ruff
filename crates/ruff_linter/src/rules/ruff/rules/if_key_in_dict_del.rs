use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{CmpOp, Expr, ExprName, ExprSubscript, Stmt, StmtDelete, StmtIf};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

type Key = Expr;
type Dict = ExprName;

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
    let [Stmt::Delete(StmtDelete { targets, .. })] = &stmt.body[..] else {
        return;
    };

    let Some((test_dict, test_key)) = extract_dict_and_key_from_test(&stmt.test) else {
        return;
    };
    let Some((del_dict, del_key)) = extract_dict_and_key_from_del(targets) else {
        return;
    };

    if !is_same_key(test_key, del_key) || !is_same_dict(test_dict, del_dict) {
        return;
    }

    if !is_known_to_be_of_type_dict(checker.semantic(), test_dict) {
        return;
    }

    let test_to_del = TextRange::new(test_dict.end(), del_dict.start());
    let fix = replace_with_dict_pop_fix(checker, stmt, test_to_del, test_dict, test_key);

    let diagnostic = Diagnostic::new(IfKeyInDictDel, stmt.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn extract_dict_and_key_from_test(test: &Expr) -> Option<(&Dict, &Key)> {
    let Expr::Compare(comp) = test else {
        return None;
    };

    let [Expr::Name(dict)] = comp.comparators.as_ref() else {
        return None;
    };

    if !matches!(comp.ops.as_ref(), [CmpOp::In]) {
        return None;
    }

    Some((dict, &comp.left))
}

fn extract_dict_and_key_from_del(targets: &[Expr]) -> Option<(&Dict, &Key)> {
    let [Expr::Subscript(ExprSubscript { value, slice, .. })] = targets else {
        return None;
    };

    let Expr::Name(dict) = value.as_ref() else {
        return None;
    };

    Some((dict, slice))
}

fn is_same_key(test: &Key, del: &Key) -> bool {
    match (test, del) {
        (Expr::Name(ExprName { id: test, .. }), Expr::Name(ExprName { id: del, .. })) => {
            test.as_str() == del.as_str()
        }

        (Expr::NoneLiteral(..), Expr::NoneLiteral(..)) => true,
        (Expr::EllipsisLiteral(..), Expr::EllipsisLiteral(..)) => true,

        (Expr::BooleanLiteral(test), Expr::BooleanLiteral(del)) => test.value == del.value,
        (Expr::NumberLiteral(test), Expr::NumberLiteral(del)) => test.value == del.value,

        (Expr::BytesLiteral(test), Expr::BytesLiteral(del)) => {
            Iterator::eq(test.value.bytes(), del.value.bytes())
        }

        (Expr::StringLiteral(test), Expr::StringLiteral(del)) => {
            Iterator::eq(test.value.chars(), del.value.chars())
        }

        _ => false,
    }
}

fn is_same_dict(test: &Dict, del: &Dict) -> bool {
    test.id.as_str() == del.id.as_str()
}

fn is_known_to_be_of_type_dict(semantic: &SemanticModel, dict: &Dict) -> bool {
    let Some(binding) = semantic.only_binding(dict).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_dict(binding, semantic)
}

fn replace_with_dict_pop_fix(
    checker: &Checker,
    stmt: &StmtIf,
    test_to_del: TextRange,
    dict: &Dict,
    key: &Key,
) -> Fix {
    let locator = checker.locator();
    let dict_expr = locator.slice(dict);
    let key_expr = locator.slice(key);

    let replacement = format!("{dict_expr}.pop({key_expr}, None)");
    let edit = Edit::range_replacement(replacement, stmt.range);

    let comment_ranges = checker.comment_ranges();
    let applicability = if comment_ranges.has_comments(&test_to_del, checker.source()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edit(edit, applicability)
}
