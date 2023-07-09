use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{self, Constant, ExceptHandler, Expr, Ranged, Stmt};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::has_comments;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `try`-`except`-`pass` blocks that can be replaced with the
/// `contextlib.suppress` context manager.
///
/// ## Why is this bad?
/// Using `contextlib.suppress` is more concise and directly communicates the
/// intent of the code: to suppress a given exception.
///
/// Note that `contextlib.suppress` is slower than using `try`-`except`-`pass`
/// directly. For performance-critical code, consider retaining the
/// `try`-`except`-`pass` pattern.
///
/// ## Example
/// ```python
/// try:
///     1 / 0
/// except ZeroDivisionError:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import contextlib
///
/// with contextlib.suppress(ZeroDivisionError):
///     1 / 0
/// ```
///
/// ## References
/// - [Python documentation: `contextlib.suppress`](https://docs.python.org/3/library/contextlib.html#contextlib.suppress)
/// - [Python documentation: `try` statement](https://docs.python.org/3/reference/compound_stmts.html#the-try-statement)
/// - [a simpler `try`/`except` (and why maybe shouldn't)](https://www.youtube.com/watch?v=MZAJ8qnC7mk)
#[violation]
pub struct SuppressibleException {
    exception: String,
}

impl Violation for SuppressibleException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SuppressibleException { exception } = self;
        format!("Use `contextlib.suppress({exception})` instead of `try`-`except`-`pass`")
    }

    fn autofix_title(&self) -> Option<String> {
        let SuppressibleException { exception } = self;
        Some(format!("Replace with `contextlib.suppress({exception})`"))
    }
}

/// SIM105
pub(crate) fn suppressible_exception(
    checker: &mut Checker,
    stmt: &Stmt,
    try_body: &[Stmt],
    handlers: &[ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
) {
    if !matches!(
        try_body,
        [Stmt::Delete(_)
            | Stmt::Assign(_)
            | Stmt::AugAssign(_)
            | Stmt::AnnAssign(_)
            | Stmt::Assert(_)
            | Stmt::Import(_)
            | Stmt::ImportFrom(_)
            | Stmt::Expr(_)
            | Stmt::Pass(_)]
    ) || handlers.len() != 1
        || !orelse.is_empty()
        || !finalbody.is_empty()
    {
        return;
    }
    let handler = &handlers[0];
    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, .. }) = handler;
    if body.len() == 1 {
        let node = &body[0];
        if node.is_pass_stmt()
            || (matches!(
            node,
            Stmt::Expr(ast::StmtExpr { value, range: _ })
            if matches!(**value, Expr::Constant(ast::ExprConstant { value: Constant::Ellipsis, .. }))
            ))
        {
            let handler_names: Vec<String> = helpers::extract_handled_exceptions(handlers)
                .into_iter()
                .filter_map(compose_call_path)
                .collect();
            let exception = if handler_names.is_empty() {
                "Exception".to_string()
            } else {
                handler_names.join(", ")
            };

            let mut diagnostic = Diagnostic::new(
                SuppressibleException {
                    exception: exception.clone(),
                },
                stmt.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                if !has_comments(stmt, checker.locator) {
                    diagnostic.try_set_fix(|| {
                        let (import_edit, binding) = checker.importer.get_or_import_symbol(
                            &ImportRequest::import("contextlib", "suppress"),
                            stmt.start(),
                            checker.semantic(),
                        )?;
                        let replace_try = Edit::range_replacement(
                            format!("with {binding}({exception})"),
                            TextRange::at(stmt.start(), "try".text_len()),
                        );
                        let handler_line_begin = checker.locator.line_start(handler.start());
                        let remove_handler = Edit::deletion(handler_line_begin, handler.end());
                        Ok(Fix::suggested_edits(
                            import_edit,
                            [replace_try, remove_handler],
                        ))
                    });
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
