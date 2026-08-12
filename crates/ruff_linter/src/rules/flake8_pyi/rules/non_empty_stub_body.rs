use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_trivia::has_leading_content;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::delete_stmt;
use crate::preview::is_non_empty_stub_body_multiple_statements_enabled;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

/// ## What it does
/// Checks for non-empty function stub bodies.
///
/// ## Why is this bad?
/// Stub files are never executed at runtime; they should be thought of as
/// "data files" for type checkers or IDEs. Function bodies are redundant
/// for this purpose.
///
/// ## Example
/// ```pyi
/// def double(x: int) -> int:
///     return x * 2
/// ```
///
/// Use instead:
/// ```pyi
/// def double(x: int) -> int: ...
/// ```
///
/// ## Preview
/// Outside of [preview], only a body made up of a single statement is flagged. A longer body
/// is left to [`stub-body-multiple-statements` (`PYI048`)][PYI048], which has no fix and isn't
/// scoped to specifically handle non-empty statements. When preview is enabled, every statement in
/// the body is flagged on its own, however many the body holds:
///
/// ```pyi
/// def double(x: int) -> int:
///     doubled = x * 2
///     return doubled
/// ```
///
/// ## See also
/// Statements that are already empty are left to the rules that own them: `pass` to
/// [`pass-statement-stub-body` (`PYI009`)][PYI009] and docstrings to
/// [`docstring-in-stub` (`PYI021`)][PYI021]. A body holding several such empty statements
/// is left to [`stub-body-multiple-statements` (`PYI048`)][PYI048] and
/// [`unnecessary-placeholder` (`PIE790`)][PIE790].
///
/// ## Fix safety
/// The fix removes each offending statement whole, including any comment nested inside it,
/// since such a comment describes the statement being removed. A comment on a line of its
/// own is kept, because it may be about the function rather than the statement surrounding it.
///
/// Deleting a statement deletes the line it sits on, so a comment trailing that statement
/// goes with it. The fix is marked unsafe only in that case: replacing a
/// statement with `...` keeps the trailing comment and stays safe.
///
/// ## References
/// - [Typing documentation - Writing and Maintaining Stub Files](https://typing.python.org/en/latest/guides/writing_stubs.html)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
/// [PIE790]: https://docs.astral.sh/ruff/rules/unnecessary-placeholder/
/// [PYI009]: https://docs.astral.sh/ruff/rules/pass-statement-stub-body/
/// [PYI021]: https://docs.astral.sh/ruff/rules/docstring-in-stub/
/// [PYI048]: https://docs.astral.sh/ruff/rules/stub-body-multiple-statements/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.253")]
pub(crate) struct NonEmptyStubBody {
    fix_kind: FixKind,
}

impl AlwaysFixableViolation for NonEmptyStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Function body must contain only `...`".to_string()
    }

    fn fix_title(&self) -> String {
        match self.fix_kind {
            FixKind::Replace => "Replace function body with `...`".to_string(),
            FixKind::Remove => "Remove statement from function body".to_string(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FixKind {
    /// The statement is replaced by `...`, because nothing else in the body would
    /// be left behind to stand in for it.
    Replace,
    /// The statement is deleted, because the body retains another statement (a
    /// `pass`, a `...`, or a docstring).
    Remove,
}

/// PYI010
pub(crate) fn non_empty_stub_body(checker: &Checker, body: &[Stmt]) {
    // Outside of preview, only single-statement bodies are flagged; anything longer is
    // left to `stub-body-multiple-statements` (PYI048).
    if !is_non_empty_stub_body_multiple_statements_enabled(checker.settings()) && body.len() > 1 {
        return;
    }

    // Whether the body already contains a statement that will survive the fix. If so,
    // every offending statement can simply be deleted; otherwise, the first one has to
    // be replaced by `...` to keep the body non-empty.
    let mut has_surviving_stmt = body
        .iter()
        .enumerate()
        .any(|(index, stmt)| is_permitted_stub_stmt(stmt, index == 0));

    for (index, stmt) in body.iter().enumerate() {
        if is_permitted_stub_stmt(stmt, index == 0) {
            continue;
        }

        let previous = body[..index].last();

        let fix_kind = if has_surviving_stmt {
            FixKind::Remove
        } else {
            has_surviving_stmt = true;
            FixKind::Replace
        };

        let edit = match fix_kind {
            FixKind::Replace => Edit::range_replacement("...".to_string(), stmt.range()),

            // A statement that shares a line with the statement before it is separated from
            // it by a semicolon, as in `def f(): x = 1; print(x)`. Delete back to the end of
            // that statement so that the semicolon goes too, rather than leaving the stray
            // `def f(): x = 1; ` that deleting the statement alone would produce.
            FixKind::Remove
                if let Some(previous) = previous
                    && has_leading_content(stmt.start(), checker.source()) =>
            {
                Edit::deletion(previous.end(), stmt.end())
            }

            // Passing `None` as the parent is safe here: `has_surviving_stmt` guarantees
            // that this statement is not the only one in the body, so deleting it cannot
            // leave behind an empty block.
            FixKind::Remove => delete_stmt(stmt, None, checker.locator(), checker.indexer()),
        };

        // Deleting a statement deletes the lines it sits on, which
        // takes a comment trailing the statement with it. A comment within the statement's
        // own range describes the statement and is meant to go with it, but one after the
        // statement ends may be about something else, so losing it makes the fix unsafe.
        let removes_trailing_comment = edit.end() > stmt.end()
            && checker
                .comment_ranges()
                .intersects(TextRange::new(stmt.end(), edit.end()));

        let mut diagnostic = checker.report_diagnostic(NonEmptyStubBody { fix_kind }, stmt.range());
        diagnostic.set_fix(Fix::applicable_edit(
            edit,
            if removes_trailing_comment {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
    }
}

/// Returns `true` if the statement is one that a stub body may keep: `...`, `pass`, or a
/// docstring. See the "See also" section of [`NonEmptyStubBody`] for the rules that own the
/// latter two.
///
/// Only the first statement can be a docstring, hence `is_first`; a later string is dead
/// weight and is flagged. Implicit concatenation (`"""doc1.""" """doc2."""`) is one statement.
fn is_permitted_stub_stmt(stmt: &Stmt, is_first: bool) -> bool {
    match stmt {
        Stmt::Pass(_) => true,
        Stmt::Expr(ast::StmtExpr { value, .. }) => {
            value.is_ellipsis_literal_expr() || (is_first && value.is_string_literal_expr())
        }
        _ => false,
    }
}
