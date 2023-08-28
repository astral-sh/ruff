use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Stmt;
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for explicit casts to `list` on for-loop iterables.
///
/// ## Why is this bad?
/// Using a `list()` call to eagerly iterate over an already-iterable type
/// (like a tuple, list, or set) is inefficient, as it forces Python to create
/// a new list unnecessarily.
///
/// Removing the `list()` call will not change the behavior of the code, but
/// may improve performance.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// items = (1, 2, 3)
/// for i in list(items):
///     print(i)
/// ```
///
/// Use instead:
/// ```python
/// items = (1, 2, 3)
/// for i in items:
///     print(i)
/// ```
#[violation]
pub struct UnnecessaryListCast;

impl AlwaysAutofixableViolation for UnnecessaryListCast {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not cast an iterable to `list` before iterating over it")
    }

    fn autofix_title(&self) -> String {
        format!("Remove `list()` cast")
    }
}

/// PERF101
pub(crate) fn unnecessary_list_cast(checker: &mut Checker, iter: &Expr) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments:
            Arguments {
                args,
                keywords: _,
                range: _,
            },
        range: list_range,
    }) = iter
    else {
        return;
    };

    let [arg] = args.as_slice() else {
        return;
    };

    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return;
    };

    if !(id == "list" && checker.semantic().is_builtin("list")) {
        return;
    }

    match arg {
        Expr::Tuple(ast::ExprTuple {
            range: iterable_range,
            ..
        })
        | Expr::List(ast::ExprList {
            range: iterable_range,
            ..
        })
        | Expr::Set(ast::ExprSet {
            range: iterable_range,
            ..
        }) => {
            let mut diagnostic = Diagnostic::new(UnnecessaryListCast, *list_range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(remove_cast(*list_range, *iterable_range));
            }
            checker.diagnostics.push(diagnostic);
        }
        Expr::Name(ast::ExprName {
            id,
            range: iterable_range,
            ..
        }) => {
            let scope = checker.semantic().current_scope();
            if let Some(binding_id) = scope.get(id) {
                let binding = checker.semantic().binding(binding_id);
                if binding.kind.is_assignment() || binding.kind.is_named_expr_assignment() {
                    if let Some(parent_id) = binding.source {
                        let parent = checker.semantic().statement(parent_id);
                        if let Stmt::Assign(ast::StmtAssign { value, .. })
                        | Stmt::AnnAssign(ast::StmtAnnAssign {
                            value: Some(value), ..
                        })
                        | Stmt::AugAssign(ast::StmtAugAssign { value, .. }) = parent
                        {
                            if matches!(
                                value.as_ref(),
                                Expr::Tuple(_) | Expr::List(_) | Expr::Set(_)
                            ) {
                                let mut diagnostic =
                                    Diagnostic::new(UnnecessaryListCast, *list_range);
                                if checker.patch(diagnostic.kind.rule()) {
                                    diagnostic.set_fix(remove_cast(*list_range, *iterable_range));
                                }
                                checker.diagnostics.push(diagnostic);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Generate a [`Fix`] to remove a `list` cast from an expression.
fn remove_cast(list_range: TextRange, iterable_range: TextRange) -> Fix {
    Fix::automatic_edits(
        Edit::deletion(list_range.start(), iterable_range.start()),
        [Edit::deletion(iterable_range.end(), list_range.end())],
    )
}
