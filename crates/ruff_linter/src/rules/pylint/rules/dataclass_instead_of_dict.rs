use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for dictionaries which could be replaced with a dataclass or namedtuple.
///
/// ## Why is this bad?
/// Replacing the dictionary with a dataclass or namedtuple makes the code more readable.
///
/// ## Example
/// ```python
/// MAPPING = {
///     "entry_1": {"key_1": 11, "key_2": 21, "key_diff_1": 31},
///     "entry_2": {"key_1": 12, "key_2": 22, "key_diff_2": 32},
/// }
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass
///
///
/// @dataclasses
/// class MyData:
///     key_1: int
///     key_2: int
///     key_optional: tuple(str, int)
///
///
/// MAPPING = {
///     "entry_1": MyData(11, 21, ("key_diff_1", 31)),
///     "entry_2": MyData(12, 22, ("key_diff_1", 32)),
/// }
/// ```
///
/// ## References
/// - [Python documentation: `dataclasses` module](https://docs.python.org/3/library/dataclasses.html#module-dataclasses)
/// - [Python documentation: `collections.namedtuple`](https://docs.python.org/3/library/collections.html#collections.namedtuple)
#[violation]
pub struct DataclassInsteadOfDict;

impl Violation for DataclassInsteadOfDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using dataclass or namedtuple instead of dict")
    }
}

/// PLR6101
pub(crate) fn dataclass_instead_of_if(checker: &mut Checker, dict: &ast::ExprDict) {
    if dict.keys.is_empty() {
        return;
    }

    if !is_module_level_dict(checker) && !is_final_dict(checker) {
        return;
    }

    if !is_dict_nodes(checker, dict) && !is_list_or_tuple_nodes(dict) {
        return;
    }

    let diagnostic = Diagnostic::new(DataclassInsteadOfDict, dict.range);
    checker.diagnostics.push(diagnostic);
}

fn is_module_level_dict(checker: &Checker) -> bool {
    if !checker.semantic().current_scope().kind.is_module() {
        return false;
    }
    // No nested statements
    if checker.semantic().current_statements().nth(1).is_some() {
        return false;
    }
    // The parent statement is an (annotated) assignment
    let Some(parent_stmt) = checker.semantic().current_statements().next() else {
        return false;
    };
    if !matches!(
        parent_stmt,
        Stmt::Assign(ast::StmtAssign { .. }) | Stmt::AnnAssign(ast::StmtAnnAssign { .. })
    ) {
        return false;
    }
    // No nested expressions
    if checker.semantic().current_expression_parent().is_some() {
        return false;
    }
    true
}

fn is_final_dict(checker: &Checker) -> bool {
    // The parent statement is an annotated assignment marked with typing.Final
    let Some(parent_stmt) = checker.semantic().current_statements().next() else {
        return false;
    };
    let Stmt::AnnAssign(ast::StmtAnnAssign {
        target, annotation, ..
    }) = parent_stmt
    else {
        return false;
    };
    if !matches!(target.as_ref(), Expr::Name(ast::ExprName { .. })) {
        return false;
    }
    if !checker.semantic().match_typing_expr(annotation, "Final") {
        return false;
    }
    true
}

fn is_list_or_tuple_nodes(dict: &ast::ExprDict) -> bool {
    // Make sure all values are lists or tuples, and their length is the same and >0
    let mut list_length = None;
    for value in &dict.values {
        match value {
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                if list_length.is_none() {
                    list_length = Some(elts.len());
                    if list_length == Some(0) {
                        return false;
                    }
                } else if list_length != Some(elts.len()) {
                    return false;
                }

                // Make sure at least one list entry isn't a dict
                if elts.iter().all(|elt| matches!(elt, Expr::Dict(_))) {
                    return false;
                }
            }
            _ => {
                return false;
            }
        }
    }
    true
}

fn is_dict_nodes(checker: &Checker, dict: &ast::ExprDict) -> bool {
    // Make sure all values are dict, and all these have at least 1 common key
    let mut keys_intersection = FxHashSet::<&str>::default();

    for value in &dict.values {
        match value {
            Expr::Dict(ast::ExprDict { keys, .. }) => {
                let mut keys_current = FxHashSet::<&str>::default();

                for key in keys {
                    let Some(expr) = key else {
                        return false;
                    };
                    let Some(string_value) = get_value(checker, expr) else {
                        return false;
                    };
                    keys_current.insert(string_value);
                }

                if keys_intersection.is_empty() {
                    keys_intersection = keys_current;
                } else {
                    keys_intersection = keys_intersection
                        .intersection(&keys_current)
                        .copied()
                        .collect();
                    if keys_intersection.is_empty() {
                        return false;
                    }
                }
            }
            _ => {
                return false;
            }
        }
    }
    true
}

fn get_value<'a>(checker: &'a Checker, expr: &'a Expr) -> Option<&'a str> {
    // Extract the underlying string constant if possible.
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = expr {
        return Some(value.to_str());
    }
    let Some(attribute) = checker.semantic().lookup_attribute(expr) else {
        return None;
    };
    let Some(node_id) = checker.semantic().binding(attribute).source else {
        return None;
    };
    let stmt = checker.semantic().statement(node_id);
    match stmt {
        Stmt::Assign(ast::StmtAssign {
            value: value_expr, ..
        })
        | Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value_expr),
            ..
        }) => match value_expr.as_ref() {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                return Some(value.to_str())
            }
            _ => None,
        },
        _ => None,
    }
}
