use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use crate::registry::AsRule;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::{Identifier, Ranged};
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

/// Custom iterator to collect all the identifiers of `,` separated values in a (nested) tuple
struct W8102TupleIterator<'a> {
    stack: Vec<&'a Identifier>,
}

impl<'a> W8102TupleIterator<'a> {
    fn new(expr: &'a Identifier) -> Self {
        Self { stack: vec![expr] }
    }
}

impl<'a> Iterator for W8102TupleIterator<'a> {
    type Item = &'a Expr;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(expr) = self.stack.pop() {
            match expr {
                Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                    for el in elts {
                        self.stack.push(el);
                    }
                }
                Expr::Name(ast::ExprName {id, ..}) => self.stack.push(id)
                _ => return Some(expr),
            }
        }
        None
    }
}

/// W8102
pub(crate) fn incorrect_dict_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    if let Expr::Call(ast::ExprCall { func, args, .. }) = iter {
        let Expr::Attribute(ast::ExprAttribute { attr, value, ..  }) = func.as_ref() else {
            return;
        };
        if attr != "items" {
            return;
        }
        if !args.is_empty() {
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

            match &elts[0] {
                Expr::Name(ast::ExprName { id, range, .. }) => {
                    if id == "_" {
                        let mut diagnostic = Diagnostic::new(
                            IncorrectDictIterator {
                                subset: "values".to_string(),
                            },
                            *range,
                        );
                        if let Expr::Name(ast::ExprName {
                            range: dict_range, ..
                        }) = value.as_ref()
                        {
                            if checker.patch(diagnostic.kind.rule()) {
                                let mut fix_val: &str = "";
                                if let Expr::Name(ast::ExprName {id, .. }) = &elts[1] {
                                    fix_val = id.as_str();
                                }
                                if let Expr::Tuple(ast::ExprTuple { range: val_range, .. }) = &elts[1] {
                                    fix_val = checker.locator.slice(*val_range);
                                }
                                diagnostic.set_fix(Fix::automatic_edits(
                                    Edit::range_replacement(
                                        ".values".to_string(),
                                        TextRange::new(dict_range.end(), func.range().end()),
                                    ),
                                    [Edit::range_replacement(fix_val.to_string(), *tup_range)],
                                ));
                            }
                            checker.diagnostics.push(diagnostic);
                            return;
                        }
                    }
                }
                Expr::Tuple(_) => {}
                _ => (),
            }
            match &elts[1] {
                Expr::Name(ast::ExprName { id, range, .. }) => {
                    if id == "_" {
                        let mut diagnostic = Diagnostic::new(
                            IncorrectDictIterator {
                                subset: "keys".to_string(),
                            },
                            *range,
                        );
                        if let Expr::Name(ast::ExprName {
                            range: dict_range, ..
                        }) = value.as_ref()
                        {
                            if checker.patch(diagnostic.kind.rule()) {
                                let mut fix_val: &str = "";
                                if let Expr::Name(ast::ExprName {id, .. }) = &elts[0] {
                                    fix_val = id.as_str();
                                }
                                if let Expr::Tuple(ast::ExprTuple { range: val_range, .. }) = &elts[0] {
                                    fix_val = checker.locator.slice(*val_range);
                                }
                                diagnostic.set_fix(Fix::automatic_edits(
                                    Edit::range_replacement(
                                        ".keys".to_string(),
                                        TextRange::new(dict_range.end(), func.range().end()),
                                    ),
                                    [Edit::range_replacement(fix_val.to_string(), *tup_range)],
                                ));
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
                Expr::Tuple(_) => {}
                _ => (),
            }
        }
    }
}
