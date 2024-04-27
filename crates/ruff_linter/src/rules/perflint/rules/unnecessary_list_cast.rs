use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};
use ruff_python_semantic::analyze::typing::find_assigned_value;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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

impl AlwaysFixableViolation for UnnecessaryListCast {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not cast an iterable to `list` before iterating over it")
    }

    fn fix_title(&self) -> String {
        format!("Remove `list()` cast")
    }
}

/// PERF101
pub(crate) fn unnecessary_list_cast(checker: &mut Checker, iter: &Expr, body: &[Stmt]) {
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

    let [arg] = &**args else {
        return;
    };

    if !checker.semantic().match_builtin_expr(func, "list") {
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
            diagnostic.set_fix(remove_cast(*list_range, *iterable_range));
            checker.diagnostics.push(diagnostic);
        }
        Expr::Name(ast::ExprName {
            id,
            range: iterable_range,
            ..
        }) => {
            let Some(value) = find_assigned_value(id, checker.semantic()) else {
                return;
            };
            if matches!(value, Expr::Tuple(_) | Expr::List(_) | Expr::Set(_)) {
                // If the variable is being modified to, don't suggest removing the cast:
                //
                // ```python
                // items = ["foo", "bar"]
                // for item in list(items):
                //    items.append("baz")
                // ```
                //
                // Here, removing the `list()` cast would change the behavior of the code.
                let mut visitor = MutationVisitor::new(id);
                visitor.visit_body(body);
                if visitor.is_mutated {
                    return;
                }

                let mut diagnostic = Diagnostic::new(UnnecessaryListCast, *list_range);
                diagnostic.set_fix(remove_cast(*list_range, *iterable_range));
                checker.diagnostics.push(diagnostic);
            }
        }
        _ => {}
    }
}

/// Generate a [`Fix`] to remove a `list` cast from an expression.
fn remove_cast(list_range: TextRange, iterable_range: TextRange) -> Fix {
    Fix::safe_edits(
        Edit::deletion(list_range.start(), iterable_range.start()),
        [Edit::deletion(iterable_range.end(), list_range.end())],
    )
}

/// A [`StatementVisitor`] that (conservatively) identifies mutations to a variable.
#[derive(Default)]
pub(crate) struct MutationVisitor<'a> {
    pub(crate) target: &'a str,
    pub(crate) is_mutated: bool,
}

impl<'a> MutationVisitor<'a> {
    pub(crate) fn new(target: &'a str) -> Self {
        Self {
            target,
            is_mutated: false,
        }
    }
}

impl<'a> StatementVisitor<'a> for MutationVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if match_mutation(stmt, self.target) {
            self.is_mutated = true;
        } else {
            walk_stmt(self, stmt);
        }
    }
}

/// Check if a statement is (probably) a modification to the list assigned to the given identifier.
///
/// For example, `foo.append(bar)` would return `true` if `id` is `foo`.
fn match_mutation(stmt: &Stmt, id: &str) -> bool {
    match stmt {
        // Ex) `foo.append(bar)`
        Stmt::Expr(ast::StmtExpr { value, .. }) => {
            let Some(ast::ExprCall { func, .. }) = value.as_call_expr() else {
                return false;
            };
            let Some(ast::ExprAttribute { value, attr, .. }) = func.as_attribute_expr() else {
                return false;
            };
            if !matches!(
                attr.as_str(),
                "append" | "insert" | "extend" | "remove" | "pop" | "clear" | "reverse" | "sort"
            ) {
                return false;
            }
            let Some(ast::ExprName { id: target_id, .. }) = value.as_name_expr() else {
                return false;
            };
            target_id == id
        }
        // Ex) `foo[0] = bar`
        Stmt::Assign(ast::StmtAssign { targets, .. }) => targets.iter().any(|target| {
            if let Some(ast::ExprSubscript { value: target, .. }) = target.as_subscript_expr() {
                if let Some(ast::ExprName { id: target_id, .. }) = target.as_name_expr() {
                    return target_id == id;
                }
            }
            false
        }),
        // Ex) `foo += bar`
        Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
            if let Some(ast::ExprName { id: target_id, .. }) = target.as_name_expr() {
                target_id == id
            } else {
                false
            }
        }
        // Ex) `foo[0]: int = bar`
        Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
            if let Some(ast::ExprSubscript { value: target, .. }) = target.as_subscript_expr() {
                if let Some(ast::ExprName { id: target_id, .. }) = target.as_name_expr() {
                    return target_id == id;
                }
            }
            false
        }
        // Ex) `del foo[0]`
        Stmt::Delete(ast::StmtDelete { targets, .. }) => targets.iter().any(|target| {
            if let Some(ast::ExprSubscript { value: target, .. }) = target.as_subscript_expr() {
                if let Some(ast::ExprName { id: target_id, .. }) = target.as_name_expr() {
                    return target_id == id;
                }
            }
            false
        }),
        _ => false,
    }
}
