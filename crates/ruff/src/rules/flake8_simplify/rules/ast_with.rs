use log::error;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Ranged, Stmt, Withitem};

use ruff_diagnostics::{AutofixKind, Violation};
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_newlines::StrExt;
use ruff_python_ast::helpers::{first_colon_range, has_comments_in};

use crate::checkers::ast::Checker;
use crate::line_width::LineWidth;
use crate::registry::AsRule;

use super::fix_with;

/// ## What it does
/// Checks for the unnecessary nesting of multiple consecutive context
/// managers.
///
/// ## Why is this bad?
/// In Python 3, a single `with` block can include multiple context
/// managers.
///
/// Combining multiple context managers into a single `with` statement
/// will minimize the indentation depth of the code, making it more
/// readable.
///
/// ## Example
/// ```python
/// with A() as a:
///     with B() as b:
///         pass
/// ```
///
/// Use instead:
/// ```python
/// with A() as a, B() as b:
///     pass
/// ```
///
/// ## References
/// - [Python: "The with statement"](https://docs.python.org/3/reference/compound_stmts.html#the-with-statement)
#[violation]
pub struct MultipleWithStatements;

impl Violation for MultipleWithStatements {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use a single `with` statement with multiple contexts instead of nested `with` \
             statements"
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Combine `with` statements".to_string())
    }
}

fn find_last_with(body: &[Stmt]) -> Option<(&[Withitem], &[Stmt])> {
    let [Stmt::With(ast::StmtWith { items, body, .. })] = body else { return None };
    find_last_with(body).or(Some((items, body)))
}

/// SIM117
pub(crate) fn multiple_with_statements(
    checker: &mut Checker,
    with_stmt: &Stmt,
    with_body: &[Stmt],
    with_parent: Option<&Stmt>,
) {
    if let Some(Stmt::With(ast::StmtWith { body, .. })) = with_parent {
        if body.len() == 1 {
            return;
        }
    }
    if let Some((items, body)) = find_last_with(with_body) {
        let last_item = items.last().expect("Expected items to be non-empty");
        let colon = first_colon_range(
            TextRange::new(
                last_item
                    .optional_vars
                    .as_ref()
                    .map_or(last_item.context_expr.end(), |v| v.end()),
                body.first().expect("Expected body to be non-empty").start(),
            ),
            checker.locator,
        );

        let mut diagnostic = Diagnostic::new(
            MultipleWithStatements,
            colon.map_or_else(
                || with_stmt.range(),
                |colon| TextRange::new(with_stmt.start(), colon.end()),
            ),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if !has_comments_in(
                TextRange::new(with_stmt.start(), with_body[0].start()),
                checker.locator,
            ) {
                match fix_with::fix_multiple_with_statements(
                    checker.locator,
                    checker.stylist,
                    with_stmt,
                ) {
                    Ok(edit) => {
                        if edit
                            .content()
                            .unwrap_or_default()
                            .universal_newlines()
                            .all(|line| {
                                LineWidth::new(checker.settings.tab_size).add_str(&line)
                                    <= checker.settings.line_length
                            })
                        {
                            #[allow(deprecated)]
                            diagnostic.set_fix(Fix::unspecified(edit));
                        }
                    }
                    Err(err) => error!("Failed to fix nested with: {err}"),
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
