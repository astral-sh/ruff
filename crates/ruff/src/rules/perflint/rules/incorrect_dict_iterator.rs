use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use crate::registry::AsRule;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::Ranged;
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
    subset: String,
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

/// PER8102
pub(crate) fn incorrect_dict_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    if let Expr::Call(ast::ExprCall { func, args, .. }) = iter {
        let Expr::Attribute(ast::ExprAttribute { attr, value, ..  }) = func.as_ref() else {
            return;
        };
        if attr.to_string() != "items" {
            return;
        }
        if args.len() != 0 {
            return;
        }
        if let Expr::Tuple(ast::ExprTuple {
            elts,
            range: tup_range,
            ..
        }) = target
        {
            if elts.len() != 2 {
                return;
            }

            if let [Expr::Name(ast::ExprName {
                id,
                range: var_range,
                ..
            }), Expr::Name(ast::ExprName {
                id: id2,
                range: var_range2, ..                                          })] = &elts.as_slice()
            {
                if id.to_string() == "_" {
                    let mut diagnostic = Diagnostic::new(
                        IncorrectDictIterator {
                            subset: "values".to_string(),
                        },
                        *var_range,
                    );
                    if let Expr::Name(ast::ExprName {
                        range: dict_range, ..
                    }) = value.as_ref()
                    {
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Fix::automatic_edits(
                                Edit::range_replacement(
                                    ".values".to_string(),
                                    TextRange::new(dict_range.end(), func.range().end()),
                                ),
                                [Edit::range_replacement(id2.to_string(), *tup_range)],
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                        return;
                    }
                }
                if id2.to_string() == "_" {
                    let mut diagnostic = Diagnostic::new(
                        IncorrectDictIterator {
                            subset: "keys".to_string(),
                        },
                        *var_range2,
                    );
                    if let Expr::Name(ast::ExprName {
                        range: dict_range, ..
                    }) = value.as_ref()
                    {
                        if checker.patch(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Fix::automatic_edits(
                                Edit::range_replacement(
                                    ".keys".to_string(),
                                    TextRange::new(dict_range.end(), func.range().end()),
                                ),
                                [Edit::range_replacement(id.to_string(), *tup_range)],
                            ));
                        }
                        checker.diagnostics.push(diagnostic);
                        return;
                    }
                }
            }
        }
    }
}
