use ruff_text_size::TextRange;
use rustpython_parser::ast;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::Stmt;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for explicit usages of `list()` on an iterable to iterate over them in a for-loop
///
/// ## Why is this bad?
/// Using a `list()` call to eagerly iterate over an already iterable type is inefficient as a
/// second list iterator is created, after first iterating the value:
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
    let Expr::Call(ast::ExprCall{ func, args, range: list_range, ..}) = iter else {
        return;
    };

    if args.is_empty() {
        return;
    }

    if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
        if id != "list" || !checker.semantic().is_builtin("list") {
            return;
        }
    };

    match &args[0] {
        Expr::Tuple(ast::ExprTuple {
            range: iter_range, ..
        })
        | Expr::List(ast::ExprList {
            range: iter_range, ..
        })
        | Expr::Set(ast::ExprSet {
            range: iter_range, ..
        }) => fix_incorrect_list_cast(checker, *list_range, *iter_range),
        Expr::Name(ast::ExprName {
            id,
            range: iterable_range,
            ..
        }) => {
            let scope = checker.semantic().scope();
            if let Some(binding_id) = scope.get(id) {
                let binding = &checker.semantic().bindings[binding_id];
                if binding.kind.is_assignment() || binding.kind.is_named_expr_assignment() {
                    if let Some(parent_id) = binding.source {
                        let parent = checker.semantic().stmts[parent_id];
                        match parent {
                            Stmt::Assign(ast::StmtAssign { value, .. })
                            | Stmt::AnnAssign(ast::StmtAnnAssign {
                                value: Some(value), ..
                            }) => match value.as_ref() {
                                Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) => {
                                    fix_incorrect_list_cast(checker, *list_range, *iterable_range);
                                }
                                _ => {}
                            },
                            _ => (),
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn fix_incorrect_list_cast(
    checker: &mut Checker,
    list_range: TextRange,
    iterable_range: TextRange,
) {
    let mut diagnostic = Diagnostic::new(UnnecessaryListCast, list_range);
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::automatic_edits(
            Edit::deletion(list_range.start(), iterable_range.start()),
            [Edit::deletion(iterable_range.end(), list_range.end())],
        ));
    }
    checker.diagnostics.push(diagnostic);
}
