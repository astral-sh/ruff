use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, ExceptHandler, Stmt};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.211")]
pub(crate) struct SuppressibleException {
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
        Some(format!(
            "Replace `try`-`except`-`pass` with `with contextlib.suppress({exception}): ...`"
        ))
    }
}

fn is_empty(body: &[Stmt]) -> bool {
    match body {
        [Stmt::Pass(_)] => true,
        [
            Stmt::Expr(ast::StmtExpr {
                value,
                range: _,
                node_index: _,
            }),
        ] => value.is_ellipsis_literal_expr(),
        _ => false,
    }
}

/// SIM105
pub(crate) fn suppressible_exception(
    checker: &Checker,
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

    let [
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            body, range, type_, ..
        }),
    ] = handlers
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
        if type_.is_none() {
            // case where there are no handler names provided at all
            "BaseException".to_string()
        } else {
            // case where handler names is an empty tuple
            String::new()
        }
    } else {
        handler_names.join(", ")
    };

    let mut diagnostic = checker.report_diagnostic(
        SuppressibleException {
            exception: exception.clone(),
        },
        stmt.range(),
    );
    if !checker
        .comment_ranges()
        .has_comments(stmt, checker.source())
    {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("contextlib", "suppress"),
                stmt.start(),
                checker.semantic(),
            )?;
            let mut rest: Vec<Edit> = Vec::new();
            let content: String;
            if exception == "BaseException" && handler_names.is_empty() {
                let (import_exception, binding_exception) =
                    checker.importer().get_or_import_symbol(
                        &ImportRequest::import("builtins", &exception),
                        stmt.start(),
                        checker.semantic(),
                    )?;
                content = format!("with {binding}({binding_exception})");
                rest.push(import_exception);
            } else {
                content = format!("with {binding}({exception})");
            }
            rest.push(Edit::range_deletion(
                checker.locator().full_lines_range(*range),
            ));
            rest.push(Edit::range_replacement(
                content,
                TextRange::at(stmt.start(), "try".text_len()),
            ));

            Ok(Fix::unsafe_edits(import_edit, rest))
        });
    }
}
