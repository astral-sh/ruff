use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, Stmt, StmtFor};
use ruff_python_semantic::analyze::typing;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for code that updates a set with the contents of an iterable by
/// using a `for` loop to call `.add()` or `.discard()` on each element
/// separately.
///
/// ## Why is this bad?
/// When adding or removing a batch of elements to or from a set, it's more
/// idiomatic to use a single method call rather than adding or removing
/// elements one by one.
///
/// ## Example
/// ```python
/// s = set()
///
/// for x in (1, 2, 3):
///     s.add(x)
///
/// for x in (1, 2, 3):
///     s.discard(x)
/// ```
///
/// Use instead:
/// ```python
/// s = set()
///
/// s.update((1, 2, 3))
/// s.difference_update((1, 2, 3))
/// ```
///
/// ## References
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[violation]
pub struct ForLoopSetMutations {
    method_name: &'static str,
    batch_method_name: &'static str,
}

impl AlwaysFixableViolation for ForLoopSetMutations {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `set.{}()` in a for loop", self.method_name)
    }

    fn fix_title(&self) -> String {
        format!("Replace with `.{}()`", self.batch_method_name)
    }
}

// FURB142
pub(crate) fn for_loop_set_mutations(checker: &mut Checker, for_stmt: &StmtFor) {
    if !for_stmt.orelse.is_empty() {
        return;
    }
    let [Stmt::Expr(stmt_expr)] = for_stmt.body.as_slice() else {
        return;
    };
    let Expr::Call(expr_call) = stmt_expr.value.as_ref() else {
        return;
    };
    let Expr::Attribute(expr_attr) = expr_call.func.as_ref() else {
        return;
    };
    if !expr_call.arguments.keywords.is_empty() {
        return;
    }

    let (method_name, batch_method_name) = match expr_attr.attr.as_str() {
        "add" => ("add", "update"),
        "discard" => ("discard", "difference_update"),
        _ => {
            return;
        }
    };

    let Expr::Name(set) = expr_attr.value.as_ref() else {
        return;
    };

    if !checker
        .semantic()
        .resolve_name(set)
        .is_some_and(|s| typing::is_set(checker.semantic().binding(s), checker.semantic()))
    {
        return;
    }
    let [arg] = expr_call.arguments.args.as_ref() else {
        return;
    };

    let content = match (for_stmt.target.as_ref(), arg) {
        (Expr::Name(for_target), Expr::Name(arg)) if for_target.id == arg.id => {
            format!(
                "{}.{batch_method_name}({})",
                set.id,
                checker.locator().slice(for_stmt.iter.as_ref())
            )
        }
        (for_target, arg) => format!(
            "{}.{batch_method_name}({} for {} in {})",
            set.id,
            checker.locator().slice(arg),
            checker.locator().slice(for_target),
            checker.locator().slice(for_stmt.iter.as_ref())
        ),
    };

    checker.diagnostics.push(
        Diagnostic::new(
            ForLoopSetMutations {
                method_name,
                batch_method_name,
            },
            for_stmt.range,
        )
        .with_fix(Fix::safe_edit(Edit::range_replacement(
            content,
            for_stmt.range,
        ))),
    );
}
