use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks if a call to `dict.items()` uses only the keys or values
///
/// ## Why is this bad?
/// Python dictionaries store keys and values in two separate tables. They can be individually
/// iterated. Using .items() and discarding either the key or the value using _ is inefficient,
/// when .keys() or .values() can be used instead:
///
/// ## Example
/// ```python
/// some_dict = {"a": 1, "b": 2}
/// for _, val in some_dict.items():
///     print(val)
/// ```
///
/// Use instead:
/// ```python
/// some_dict = {"a": 1, "b": 2}
/// for val in some_dict.values():
///     print(val)
/// ```
#[violation]
pub struct IncorrectDictIterator {
    // TODO: Come up with a better name
    method: String,
}

impl Violation for IncorrectDictIterator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectDictIterator { method } = self;
        format!("When using only the {method} of a dict use the {method}() method")
    }
}

/// PER8102
pub(crate) fn incorrect_dict_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    if let Expr::Call(ast::ExprCall { func, .. }) = iter {
        let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
            return;
        };
        if attr.to_string() != "items" {
            return;
        }
        // TODO: Add check that `items()` is being called on a dict and is not a custom implementation
    }
    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = target {
        if elts.len() != 2 {
            return;
        }

        if let Expr::Name(ast::ExprName { id, range,.. }) = &elts[0] {
            if id.to_string() == "_" {
                checker.diagnostics.push(Diagnostic::new(
                    IncorrectDictIterator {
                        method: "values".to_string(),
                    },
                    *range,
                ));
                return;
            }
        }
        if let Expr::Name(ast::ExprName { id, range, .. }) = &elts[1] {
            if id.to_string() == "_" {
                checker.diagnostics.push(Diagnostic::new(
                    IncorrectDictIterator {
                        method: "keys".to_string(),
                    },
                    *range,
                ));
                return;
            }
        }
        // TODO: Add autofix
    }
}
