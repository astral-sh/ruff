use std::fmt;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, Expr};
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;

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
///
/// ## Fix safety
/// The fix does not perform any type analysis and, as such, may suggest an
/// incorrect fix if the object in question does not duck-type as a mapping
/// (e.g., if it is missing a `.keys()` or `.values()` method, or if those
/// methods behave differently than they do on standard mapping types).
#[violation]
pub struct IncorrectDictIterator {
    subset: DictSubset,
}

impl AlwaysFixableViolation for IncorrectDictIterator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectDictIterator { subset } = self;
        format!("When using only the {subset} of a dict use the `{subset}()` method")
    }

    fn fix_title(&self) -> String {
        let IncorrectDictIterator { subset } = self;
        format!("Replace `.items()` with `.{subset}()`")
    }
}

/// PERF102
pub(crate) fn incorrect_dict_iterator(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = stmt_for.target.as_ref() else {
        return;
    };
    let [key, value] = elts.as_slice() else {
        return;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    }) = stmt_for.iter.as_ref()
    else {
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
        is_unused(key, checker.semantic()),
        is_unused(value, checker.semantic()),
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
            let replace_attribute = Edit::range_replacement("values".to_string(), attr.range());
            let replace_target = Edit::range_replacement(
                pad(
                    checker.locator().slice(value).to_string(),
                    stmt_for.target.range(),
                    checker.locator(),
                ),
                stmt_for.target.range(),
            );
            diagnostic.set_fix(Fix::unsafe_edits(replace_attribute, [replace_target]));
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
            let replace_attribute = Edit::range_replacement("keys".to_string(), attr.range());
            let replace_target = Edit::range_replacement(
                pad(
                    checker.locator().slice(key).to_string(),
                    stmt_for.target.range(),
                    checker.locator(),
                ),
                stmt_for.target.range(),
            );
            diagnostic.set_fix(Fix::unsafe_edits(replace_attribute, [replace_target]));
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Returns `true` if the given expression is unused.
/// An expression is considered unused if it is not referenced after its
/// definition. This way we can avoid false positives for bindings that are
/// declared before the expression in question. For example, in the following
/// code, `value` is declared before `for statement`, but it is not used after it
/// and should be considered unused from the `ForStatement` perspective:
/// ```python
/// def f():
///     value = 2
///     print(value)
///
///     for key, value in some_dict.items():  # PERF102
///         print(key)
/// ```
fn is_unused(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            elts.iter().all(|expr| is_unused(expr, semantic))
        }
        Expr::Name(ast::ExprName { id, .. }) => semantic
            .current_scope()
            .get_all(id)
            .filter(|binding_id| {
                has_references_after_expr(semantic.binding(*binding_id), expr, semantic)
            })
            .peekable()
            .peek()
            .is_none(),
        _ => false,
    }
}

/// Returns `true` if the given binding is referenced after the given expression.   
fn has_references_after_expr(binding: &Binding, expr: &Expr, semantic: &SemanticModel) -> bool {
    let ref_count_before_expr = binding
        .references
        .len()
        .saturating_sub(binding_count_before_expr(expr, semantic));
    binding.start() >= expr.start() && ref_count_before_expr > 0
}

/// Returns the number of references to the given binding that occur before the
/// given expression.
fn binding_count_before_expr(expr: &Expr, semantic: &SemanticModel) -> usize {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => semantic
            .current_scope()
            .get_all(id)
            .map(|binding_id| semantic.binding(binding_id))
            .filter(|binding| binding.start() < expr.start())
            .count(),
        _ => 0,
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
