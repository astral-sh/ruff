use std::fmt;

use ast::{ExprContext, Identifier};
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Arguments, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

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
/// obj = {"a": 1, "b": 2}
/// for key, value in obj.items():
///     print(value)
/// ```
///
/// Use instead:
/// ```python
/// obj = {"a": 1, "b": 2}
/// for value in obj.values():
///     print(value)
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

fn unpacked_items<'a>(
    target: &'a Expr,
    iter: &'a Expr,
) -> Option<(&'a Expr, &'a Identifier, &'a Expr, &'a Expr)> {
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = target else {
        return None;
    };
    let [key, value] = elts.as_slice() else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    }) = iter
    else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return None;
    };
    if attr != "items" {
        return None;
    }
    Some((func, attr, key, value))
}

pub(crate) fn incorrect_dict_iterator(
    checker: &mut Checker,
    target: &Expr,
    func: &Expr,
    attr: &Identifier,
    key: &Expr,
    value: &Expr,
    key_unused: bool,
    value_unused: bool,
) {
    match (key_unused, value_unused) {
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
                    checker.locator().slice(value.range()).to_string(),
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
                    checker.locator().slice(key.range()).to_string(),
                    target.range(),
                );
                diagnostic.set_fix(Fix::suggested_edits(replace_attribute, [replace_target]));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PERF102
pub(crate) fn incorrect_dict_iterator_for(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    let ast::StmtFor { target, iter, .. } = stmt_for;
    let Some((func, attr, key, value)) = unpacked_items(target, iter) else {
        return;
    };
    incorrect_dict_iterator(
        checker,
        target,
        func,
        attr,
        key,
        value,
        is_unused(key, checker.semantic()),
        is_unused(value, checker.semantic()),
    );
}

#[derive(Default)]
struct LoadedNamesVisitor<'a> {
    loaded: Vec<&'a ast::ExprName>,
    stored: Vec<&'a ast::ExprName>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoadedNamesVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match &name.ctx {
                ExprContext::Load => self.loaded.push(name),
                ExprContext::Store => self.stored.push(name),
                ExprContext::Del => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

pub(crate) fn incorrect_dict_iterator_seq(
    checker: &mut Checker,
    elt: &Expr,
    generators: &[ast::Comprehension],
) {
    let mut visitor = LoadedNamesVisitor::default();
    visitor.visit_expr(elt);
    for generator in generators.iter().rev() {
        let ast::Comprehension { target, iter, .. } = generator;
        let Some((func, attr, key, value)) = unpacked_items(target, iter) else {
            visitor.visit_expr(iter);
            continue;
        };
        let key_unused = match key {
            Expr::Name(name) => !visitor.loaded.iter().any(|n| n.id == name.id),
            _ => false,
        };
        let value_unused = match value {
            Expr::Name(name) => !visitor.loaded.iter().any(|n| n.id == name.id),
            _ => false,
        };
        incorrect_dict_iterator(
            checker,
            target,
            func,
            attr,
            key,
            value,
            key_unused,
            value_unused,
        );
        visitor.visit_expr(iter);
    }
}

pub(crate) fn incorrect_dict_iterator_dict(
    checker: &mut Checker,
    key: &Expr,
    value: &Expr,
    generators: &[ast::Comprehension],
) {
    let mut visitor = LoadedNamesVisitor::default();
    visitor.visit_expr(key);
    visitor.visit_expr(value);
    for generator in generators.iter().rev() {
        let ast::Comprehension { target, iter, .. } = generator;
        let Some((func, attr, key, value)) = unpacked_items(target, iter) else {
            visitor.visit_expr(iter);
            continue;
        };

        let key_unused = match key {
            Expr::Name(name) => !visitor.loaded.iter().any(|n| n.id == name.id),
            _ => false,
        };
        let value_unused = match value {
            Expr::Name(name) => !visitor.loaded.iter().any(|n| n.id == name.id),
            _ => false,
        };
        incorrect_dict_iterator(
            checker,
            target,
            func,
            attr,
            key,
            value,
            key_unused,
            value_unused,
        );
        visitor.visit_expr(iter);
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

/// Returns `true` if the given expression is either an unused value or a tuple of unused values.
fn is_unused(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            elts.iter().all(|expr| is_unused(expr, semantic))
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            // Treat a variable as used if it has any usages, _or_ it's shadowed by another variable
            // with usages.
            //
            // If we don't respect shadowing, we'll incorrectly flag `bar` as unused in:
            // ```python
            // from random import random
            //
            // for bar in range(10):
            //     if random() > 0.5:
            //         break
            // else:
            //     bar = 1
            //
            // print(bar)
            // ```
            let scope = semantic.current_scope();
            scope
                .get_all(id)
                .map(|binding_id| semantic.binding(binding_id))
                .filter(|binding| binding.start() >= expr.start())
                .all(|binding| !binding.is_used())
        }
        _ => false,
    }
}
