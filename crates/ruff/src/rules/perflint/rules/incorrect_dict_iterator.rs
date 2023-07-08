use std::fmt;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Expr;
use rustpython_parser::{ast, lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use rustpython_parser::ast::Ranged;

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
    if elts.len() != 2 {
        return;
    }
    let Expr::Call(ast::ExprCall { func, args, .. }) = iter else {
        return;
    };
    if !args.is_empty() {
        return;
    }
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };
    if attr != "items" {
        return;
    }

    let unused_key = is_ignored_tuple_or_name(&elts[0]);
    let unused_value = is_ignored_tuple_or_name(&elts[1]);

    match (unused_key, unused_value) {
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
                if let Some(range) = attribute_range(value.end(), checker.locator) {
                    let replace_attribute = Edit::range_replacement("values".to_string(), range);
                    let replace_target = Edit::range_replacement(
                        checker.locator.slice(elts[1].range()).to_string(),
                        target.range(),
                    );
                    diagnostic.set_fix(Fix::suggested_edits(replace_attribute, [replace_target]));
                }
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
                if let Some(range) = attribute_range(value.end(), checker.locator) {
                    let replace_attribute = Edit::range_replacement("keys".to_string(), range);
                    let replace_target = Edit::range_replacement(
                        checker.locator.slice(elts[0].range()).to_string(),
                        target.range(),
                    );
                    diagnostic.set_fix(Fix::suggested_edits(replace_attribute, [replace_target]));
                }
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
fn is_ignored_tuple_or_name(expr: &Expr) -> bool {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().all(is_ignored_tuple_or_name),
        Expr::Name(ast::ExprName { id, .. }) => id == "_",
        _ => false,
    }
}

/// Returns the range of the attribute identifier after the given location, if any.
fn attribute_range(at: TextSize, locator: &Locator) -> Option<TextRange> {
    lexer::lex_starts_at(locator.after(at), Mode::Expression, at)
        .flatten()
        .find_map(|(tok, range)| {
            if matches!(tok, Tok::Name { .. }) {
                Some(range)
            } else {
                None
            }
        })
}
