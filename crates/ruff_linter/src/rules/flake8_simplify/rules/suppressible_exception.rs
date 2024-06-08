use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, ExceptHandler, Stmt};
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SuppressibleException { exception } = self;
        format!("Use `contextlib.suppress({exception})` instead of `try`-`except`-`pass`")
    }

    fn fix_title(&self) -> Option<String> {
        let SuppressibleException { exception } = self;
        Some(format!("Replace with `contextlib.suppress({exception})`"))
    }
}

fn is_empty(body: &[Stmt]) -> bool {
    match body {
        [Stmt::Pass(_)] => true,
        [Stmt::Expr(ast::StmtExpr { value, range: _ })] => value.is_ellipsis_literal_expr(),
        _ => false,
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
    ) || !orelse.is_empty()
        || !finalbody.is_empty()
    {
        return;
    }

    let [ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, range, .. })] =
        handlers
    else {
        return;
    };

    if !is_empty(body) {
        return;
    }

    let Some(handler_names) = helpers::extract_handled_exceptions(handlers)
        .into_iter()
        .map(|expr| UnqualifiedName::from_expr(expr).map(|name| name.to_string()))
        .collect::<Option<Vec<String>>>()
    else {
        return;
    };

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
    if !checker
        .comment_ranges()
        .has_comments(stmt, checker.locator())
    {
        diagnostic.try_set_fix(|| {
            // let range = statement_range(stmt, checker.locator(), checker.indexer());

            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("contextlib", "suppress"),
                stmt.start(),
                checker.semantic(),
            )?;
            let replace_try = Edit::range_replacement(
                format!("with {binding}({exception})"),
                TextRange::at(stmt.start(), "try".text_len()),
            );
            let remove_handler = Edit::range_deletion(checker.locator().full_lines_range(*range));
            Ok(Fix::unsafe_edits(
                import_edit,
                [replace_try, remove_handler],
            ))
        });
    }
    checker.diagnostics.push(diagnostic);
}
