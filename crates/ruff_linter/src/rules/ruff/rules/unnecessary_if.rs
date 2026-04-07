use std::cmp::Ordering;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::{
    any_over_expr, comment_indentation_after, contains_effect, is_stub_body,
};
use ruff_python_ast::token::{TokenKind, Tokens, parenthesized_range};
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{Expr, StmtIf};
use ruff_python_semantic::analyze::typing;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix, fix};

/// ## What it does
/// Checks for `if` statements (without `elif` or `else` branches) where the
/// body contains only `pass` or `...`
///
/// ## Why is this bad?
/// An `if` statement with an empty body either does nothing (when the
/// condition is side-effect-free) or could be replaced with just the
/// condition expression (when it has side effects). This pattern commonly
/// arises when auto-fixers remove unused imports from conditional blocks
/// (e.g., version-dependent imports), leaving behind an empty skeleton.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info >= (3, 11):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import sys
/// ```
///
/// ## Fix safety
/// When the condition is side-effect-free, the fix removes the entire `if`
/// statement.
///
/// When the condition has side effects (e.g., a function call), the fix
/// replaces the `if` statement with just the condition as an expression
/// statement, preserving the side effects.
///
/// Note: conditions consisting solely of a name expression (like
/// `if x: pass`) are treated as side-effect-free, even though `if x`
/// implicitly calls `x.__bool__()` (or `x.__len__()`), which could have
/// side effects if overridden. In practice this is very rare, but if you
/// rely on this behavior, suppress the diagnostic with `# noqa: RUF050`.
///
/// ## Related rules
/// - [`needless-else (RUF047)`]: Detects empty `else` clauses. For `if`/`else`
///   statements where all branches are empty, `RUF047` first removes the empty
///   `else`, and then this rule catches the remaining empty `if`.
/// - [`empty-type-checking-block (TC005)`]: Detects empty `if TYPE_CHECKING`
///   blocks specifically.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.8")]
pub(crate) struct UnnecessaryIf;

impl AlwaysFixableViolation for UnnecessaryIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Empty `if` statement".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove the `if` statement".to_string()
    }
}

/// RUF050
pub(crate) fn unnecessary_if(checker: &Checker, stmt: &StmtIf) {
    let StmtIf {
        test,
        body,
        elif_else_clauses,
        ..
    } = stmt;

    // Only handle bare `if` blocks — `elif`/`else` branches are handled by
    // RUF047 (needless-else)
    if !elif_else_clauses.is_empty() {
        return;
    }

    if !is_stub_body(body) {
        return;
    }

    // Skip `if TYPE_CHECKING` blocks — handled by TC005
    if typing::is_type_checking_block(stmt, checker.semantic()) {
        return;
    }

    // Skip if the body contains a comment
    if if_contains_comments(stmt, checker) {
        return;
    }

    let has_side_effects = contains_effect(test, |id| checker.semantic().has_builtin_binding(id))
        || any_over_expr(test, |expr| matches!(expr, Expr::Named(_)));

    let mut diagnostic = checker.report_diagnostic(UnnecessaryIf, stmt.range());

    if has_side_effects {
        // Replace `if cond: pass` with `cond` as an expression statement.
        let replacement = condition_as_expression_statement(test, stmt, checker);
        let edit = Edit::range_replacement(replacement, stmt.range());
        diagnostic.set_fix(Fix::safe_edit(edit));
    } else {
        let stmt_ref = checker.semantic().current_statement();
        let parent = checker.semantic().current_statement_parent();
        let edit = fix::edits::delete_stmt(stmt_ref, parent, checker.locator(), checker.indexer());
        let fix = Fix::safe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_parent_id(),
        ));
        diagnostic.set_fix(fix);
    }
}

/// Return the `if` condition in a form that remains a single valid expression statement.
fn condition_as_expression_statement(test: &Expr, stmt: &StmtIf, checker: &Checker) -> String {
    let has_top_level_line_break = has_top_level_line_break(test.range(), checker.tokens());

    if has_top_level_line_break
        && let Some(range) = parenthesized_range(test.into(), stmt.into(), checker.tokens())
    {
        return checker.locator().slice(range).to_string();
    }

    let condition_text = checker.locator().slice(test.range());
    if test.is_named_expr() || has_top_level_line_break {
        format!("({condition_text})")
    } else {
        condition_text.to_string()
    }
}

/// Returns `true` if an expression contains a line break at the top level.
///
/// Such expressions need parentheses to remain a single expression statement when extracted from
/// an `if` condition.
fn has_top_level_line_break(range: TextRange, tokens: &Tokens) -> bool {
    let mut nesting = 0u32;

    for token in tokens.in_range(range) {
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => nesting += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                nesting = nesting.saturating_sub(1);
            }
            TokenKind::Newline | TokenKind::NonLogicalNewline if nesting == 0 => return true,
            _ => {}
        }
    }

    false
}

/// Returns `true` if the `if` statement contains a comment
fn if_contains_comments(stmt: &StmtIf, checker: &Checker) -> bool {
    let source = checker.source();

    // Use `line_end` (before the newline) instead of `full_line_end` (after
    // the newline) to avoid touching the range of a comment on the next line.
    // `TextRange::intersect` considers touching ranges as intersecting.
    let stmt_line_end = source.line_end(stmt.end());
    let check_range = TextRange::new(stmt.start(), stmt_line_end);

    let Some(last_stmt) = stmt.body.last() else {
        return false;
    };

    let stmt_full_end = source.full_line_end(stmt.end());

    checker.comment_ranges().intersects(check_range)
        || if_has_trailing_comment(stmt, last_stmt, stmt_full_end, checker)
}

/// Returns `true` if the `if` branch has a trailing own-line comment
fn if_has_trailing_comment(
    stmt: &StmtIf,
    last_body_stmt: &ruff_python_ast::Stmt,
    stmt_full_end: TextSize,
    checker: &Checker,
) -> bool {
    let (tokens, source) = (checker.tokens(), checker.source());

    // Compare against the `if` keyword indentation rather than the body
    // statement — handles single-line forms like `if True: pass`
    let stmt_indentation = indentation(source, stmt).unwrap_or_default().text_len();

    for token in tokens.after(stmt_full_end) {
        match token.kind() {
            TokenKind::Comment => {
                let comment_indentation =
                    comment_indentation_after(last_body_stmt.into(), token.range(), source);

                match comment_indentation.cmp(&stmt_indentation) {
                    Ordering::Greater => return true,
                    Ordering::Equal | Ordering::Less => break,
                }
            }

            TokenKind::NonLogicalNewline
            | TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent => {}

            _ => break,
        }
    }

    false
}
