use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, Stmt, StmtFor};
use ruff_python_semantic::analyze::typing;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks use of `IOBase.write` in a for loop.
///
/// ## Why is this bad?
/// When writing a batch of elements, it's more idiomatic to use a single method call,
/// `IOBase.writelines`, rather than write elements one by one.
///
/// ## Example
/// ```python
/// with Path("file").open("w") as f:
///     for line in lines:
///         f.write(line)
///
/// with Path("file").open("wb") as f:
///     for line in lines:
///         f.write(line.encode())
/// ```
///
/// Use instead:
/// ```python
/// with Path("file").open("w") as f:
///     f.writelines(lines)
///
/// with Path("file").open("wb") as f:
///     f.writelines(line.encode() for line in lines)
/// ```
///
/// ## References
/// - [Python documentation: `io.IOBase.writelines`](https://docs.python.org/3/library/io.html#io.IOBase.writelines)
#[violation]
pub struct ForLoopWrites {
    name: String,
}

impl AlwaysFixableViolation for ForLoopWrites {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `{}.write` in a for loop", self.name)
    }

    fn fix_title(&self) -> String {
        format!("Replace with `{}.writelines`", self.name)
    }
}

pub(crate) fn for_loop_writes(checker: &mut Checker, for_stmt: &StmtFor) {
    if !for_stmt.orelse.is_empty() {
        return;
    }
    let [Stmt::Expr(stmt_expr)] = for_stmt.body.as_slice() else {
        return;
    };
    let Expr::Call(call_expr) = stmt_expr.value.as_ref() else {
        return;
    };
    let Expr::Attribute(expr_attr) = call_expr.func.as_ref() else {
        return;
    };
    if expr_attr.attr.as_str() != "write" {
        return;
    }
    if !call_expr.arguments.keywords.is_empty() {
        return;
    }
    let [write_arg] = call_expr.arguments.args.as_ref() else {
        return;
    };

    let Expr::Name(io_object_name) = expr_attr.value.as_ref() else {
        return;
    };

    // Determine whether `f` in `f.write()` was bound to a file object.
    if !checker
        .semantic()
        .resolve_name(io_object_name)
        .map(|id| checker.semantic().binding(id))
        .is_some_and(|binding| typing::is_io_base(binding, checker.semantic()))
    {
        return;
    }

    let content = match (for_stmt.target.as_ref(), write_arg) {
        (Expr::Name(for_target), Expr::Name(write_arg)) if for_target.id == write_arg.id => {
            format!(
                "{}.writelines({})",
                checker.locator().slice(io_object_name),
                checker.locator().slice(for_stmt.iter.as_ref()),
            )
        }
        (for_target, write_arg) => {
            format!(
                "{}.writelines({} for {} in {})",
                checker.locator().slice(io_object_name),
                checker.locator().slice(write_arg),
                checker.locator().slice(for_target),
                checker.locator().slice(for_stmt.iter.as_ref()),
            )
        }
    };

    checker.diagnostics.push(
        Diagnostic::new(
            ForLoopWrites {
                name: io_object_name.id.clone(),
            },
            for_stmt.range,
        )
        .with_fix(Fix::safe_edit(Edit::range_replacement(
            content,
            for_stmt.range,
        ))),
    );
}
