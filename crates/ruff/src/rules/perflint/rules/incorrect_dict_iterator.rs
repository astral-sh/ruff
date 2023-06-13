use std::fmt;

use ruff_text_size::TextRange;
use rustpython_parser::ast;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `dict.items()` that discard either the key or the value
/// when iterating over the dictionary.
///
/// ## Why is this bad?
///
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
    subset: DictSubset,
}

impl AlwaysAutofixableViolation for IncorrectDictIterator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectDictIterator { subset } = self;
        format!("When using only the {subset} of a dict use the `{subset}()` method")
    }

    fn autofix_title(&self) -> String {
        let IncorrectDictIterator { subset } = self;
        format!("Replace `.items()` with `.{subset}()`.")
    }
}

#[derive(Debug, PartialEq, Eq)]
enum DictSubset {
    Keys,
    Values,
}

impl fmt::Display for DictSubset {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DictSubset::Keys => fmt.write_str("keys"),
            DictSubset::Values => fmt.write_str("values"),
        }
    }
}

fn is_ignored_tuple_or_name(expr: &Expr) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = expr {
        return id == "_";
    }
    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = expr {
        return elts.iter().all(is_ignored_tuple_or_name);
    }
    false
}

/// PERF102
pub(crate) fn incorrect_dict_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    let Expr::Call(ast::ExprCall { func, args, .. }) = iter else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { attr, value, ..  }) = func.as_ref() else {
        return;
    };

    if attr != "items" {
        return;
    }
    if !args.is_empty() {
        return;
    }
    let Expr::Name(ast::ExprName { range: dict_range, .. }) = value.as_ref() else {
        return;
    };

    let method_range = TextRange::new(dict_range.end(), func.range().end());

    let Expr::Tuple(ast::ExprTuple {
        elts,
        range: tuple_range,
        ..
    }) = target
    else {
        return
    };
    if elts.len() != 2 {
        return;
    }
    match &elts[0] {
        Expr::Name(ast::ExprName { id, .. }) => {
            if id == "_" {
                let mut diagnostic = Diagnostic::new(
                    IncorrectDictIterator {
                        subset: DictSubset::Values,
                    },
                    method_range,
                );
                if checker.patch(diagnostic.kind.rule()) {
                    let mut fix_val: &str = "";
                    if let Expr::Name(ast::ExprName { id, .. }) = &elts[1] {
                        fix_val = id.as_str();
                    }
                    if let Expr::Tuple(ast::ExprTuple {
                        range: val_range, ..
                    }) = &elts[1]
                    {
                        fix_val = checker.locator.slice(*val_range);
                    }
                    diagnostic.set_fix(Fix::automatic_edits(
                        Edit::range_replacement(".values".to_string(), method_range),
                        [Edit::range_replacement(fix_val.to_string(), *tuple_range)],
                    ));
                }
                checker.diagnostics.push(diagnostic);
                return;
            }
        }
        Expr::Tuple(ast::ExprTuple { elts: sub_elts, .. }) => {
            if sub_elts.iter().all(is_ignored_tuple_or_name) {
                let mut diagnostic = Diagnostic::new(
                    IncorrectDictIterator {
                        subset: DictSubset::Values,
                    },
                    method_range,
                );
                if checker.patch(diagnostic.kind.rule()) {
                    let mut fix_val: &str = "";
                    if let Expr::Name(ast::ExprName { id, .. }) = &elts[1] {
                        fix_val = id.as_str();
                    }
                    if let Expr::Tuple(ast::ExprTuple {
                        range: val_range, ..
                    }) = &elts[1]
                    {
                        fix_val = checker.locator.slice(*val_range);
                    }
                    diagnostic.set_fix(Fix::automatic_edits(
                        Edit::range_replacement(".values".to_string(), method_range),
                        [Edit::range_replacement(fix_val.to_string(), *tuple_range)],
                    ));
                }
                checker.diagnostics.push(diagnostic);
                return;
            }
        }
        _ => (),
    }
    match &elts[1] {
        Expr::Name(ast::ExprName { id, .. }) => {
            if id == "_" {
                let mut diagnostic = Diagnostic::new(
                    IncorrectDictIterator {
                        subset: DictSubset::Keys,
                    },
                    method_range,
                );
                if checker.patch(diagnostic.kind.rule()) {
                    let mut fix_val: &str = "";
                    if let Expr::Name(ast::ExprName { id, .. }) = &elts[0] {
                        fix_val = id.as_str();
                    }
                    if let Expr::Tuple(ast::ExprTuple {
                        range: val_range, ..
                    }) = &elts[0]
                    {
                        fix_val = checker.locator.slice(*val_range);
                    }
                    diagnostic.set_fix(Fix::automatic_edits(
                        Edit::range_replacement(".keys".to_string(), method_range),
                        [Edit::range_replacement(fix_val.to_string(), *tuple_range)],
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        Expr::Tuple(ast::ExprTuple { elts: sub_elts, .. }) => {
            if sub_elts.iter().all(is_ignored_tuple_or_name) {
                let mut diagnostic = Diagnostic::new(
                    IncorrectDictIterator {
                        subset: DictSubset::Keys,
                    },
                    method_range,
                );
                if checker.patch(diagnostic.kind.rule()) {
                    let mut fix_val: &str = "";
                    if let Expr::Name(ast::ExprName { id, .. }) = &elts[0] {
                        fix_val = id.as_str();
                    }
                    if let Expr::Tuple(ast::ExprTuple {
                        range: val_range, ..
                    }) = &elts[0]
                    {
                        fix_val = checker.locator.slice(*val_range);
                    }
                    diagnostic.set_fix(Fix::automatic_edits(
                        Edit::range_replacement(".keys".to_string(), method_range),
                        [Edit::range_replacement(fix_val.to_string(), *tuple_range)],
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        _ => (),
    }
}
