use std::fmt;

use regex::Regex;
use rustpython_parser::ast;
use rustpython_parser::ast::Expr;
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `dict.items()` that discard either the key or the value
/// when iterating over the dictionary.
///
/// ## Why is this bad?
/// If you only need the keys or values of a dictionary, you should use
/// `dict.keys()` or `dict.values()` respectively, instead of `dict.items()`.
/// These specialized methods are more efficient than `dict.items()`, as they
/// avoid allocating tuples for every item in the dictionary. They also
/// communicate the intent of the code more clearly.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
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
        format!("Replace `.items()` with `.{subset}()`")
    }
}

/// PERF102
pub(crate) fn incorrect_dict_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = target else {
        return;
    };
    let [key, value] = elts.as_slice() else {
        return;
    };
    let Expr::Call(ast::ExprCall { func, args, .. }) = iter else {
        return;
    };
    if !args.is_empty() {
        return;
    }
    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return;
    };
    if attr != "items" {
        return;
    }

    match (
        is_ignored_tuple_or_name(key, &checker.settings.dummy_variable_rgx),
        is_ignored_tuple_or_name(value, &checker.settings.dummy_variable_rgx),
    ) {
        (true, true) => {
            // Both the key and the value are unused.
        }
        (false, false) => {
            // Neither the key nor the value are unused.
        }
        (true, false) => {
            // The key is unused, so replace with `dict.values()`.
            let mut diagnostic = Diagnostic::new(
                IncorrectDictIterator {
                    subset: DictSubset::Values,
                },
                func.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let replace_attribute = Edit::range_replacement("values".to_string(), attr.range());
                let replace_target = Edit::range_replacement(
                    checker.locator.slice(value.range()).to_string(),
                    target.range(),
                );
                diagnostic.set_fix(Fix::suggested_edits(replace_attribute, [replace_target]));
            }
            checker.diagnostics.push(diagnostic);
        }
        (false, true) => {
            // The value is unused, so replace with `dict.keys()`.
            let mut diagnostic = Diagnostic::new(
                IncorrectDictIterator {
                    subset: DictSubset::Keys,
                },
                func.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let replace_attribute = Edit::range_replacement("keys".to_string(), attr.range());
                let replace_target = Edit::range_replacement(
                    checker.locator.slice(key.range()).to_string(),
                    target.range(),
                );
                diagnostic.set_fix(Fix::suggested_edits(replace_attribute, [replace_target]));
            }
            checker.diagnostics.push(diagnostic);
        }
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

/// Returns `true` if the given expression is either an ignored value or a tuple of ignored values.
fn is_ignored_tuple_or_name(expr: &Expr, dummy_variable_rgx: &Regex) -> bool {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts
            .iter()
            .all(|expr| is_ignored_tuple_or_name(expr, dummy_variable_rgx)),
        Expr::Name(ast::ExprName { id, .. }) => dummy_variable_rgx.is_match(id.as_str()),
        _ => false,
    }
}
