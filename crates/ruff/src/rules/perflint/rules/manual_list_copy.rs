use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `for` loops that can be replaced by a making a copy of a list.
///
/// ## Why is this bad?
/// When creating a copy of an existing list using a for-loop, prefer
/// `list` or `list.copy` instead. Making a direct copy is more readable and
/// more performant.
///
/// Using the below as an example, the `list`-based copy is ~2x faster on
/// Python 3.11.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// original = list(range(10000))
/// filtered = []
/// for i in original:
///     filtered.append(i)
/// ```
///
/// Use instead:
/// ```python
/// original = list(range(10000))
/// filtered = list(original)
/// ```
#[violation]
pub struct ManualListCopy;

impl Violation for ManualListCopy {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `list` or `list.copy` to create a copy of a list")
    }
}

/// PERF402
pub(crate) fn manual_list_copy(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
    let [stmt] = body else {
        return;
    };

    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return;
    };

    let Expr::Call(ast::ExprCall {
        func,
        range,
        args,
        keywords,
    }) = value.as_ref()
    else {
        return;
    };

    if !keywords.is_empty() {
        return;
    }

    let [arg] = args.as_slice() else {
        return;
    };

    // Only flag direct list copies (e.g., `for x in y: filtered.append(x)`).
    if !arg.as_name_expr().map_or(false, |arg| {
        target
            .as_name_expr()
            .map_or(false, |target| arg.id == target.id)
    }) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return;
    };

    if matches!(attr.as_str(), "append" | "insert") {
        checker
            .diagnostics
            .push(Diagnostic::new(ManualListCopy, *range));
    }
}
