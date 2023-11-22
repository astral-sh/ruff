use ruff_python_ast::{PySourceType, Stmt};
use ruff_python_parser::{lexer, AsMode, Tok};
use ruff_python_trivia::PythonWhitespace;
use ruff_source_file::{Locator, UniversalNewlines};
use ruff_text_size::Ranged;

use crate::rules::isort::types::TrailingComma;

/// Return `true` if a `Stmt::ImportFrom` statement ends with a magic
/// trailing comma.
pub(super) fn trailing_comma(
    stmt: &Stmt,
    locator: &Locator,
    source_type: PySourceType,
) -> TrailingComma {
    let contents = locator.slice(stmt);
    let mut count = 0u32;
    let mut trailing_comma = TrailingComma::Absent;
    for (tok, _) in lexer::lex_starts_at(contents, source_type.as_mode(), stmt.start()).flatten() {
        if matches!(tok, Tok::Lpar) {
            count = count.saturating_add(1);
        }
        if matches!(tok, Tok::Rpar) {
            count = count.saturating_sub(1);
        }
        if count == 1 {
            if matches!(
                tok,
                Tok::NonLogicalNewline | Tok::Indent | Tok::Dedent | Tok::Comment(_)
            ) {
                continue;
            } else if matches!(tok, Tok::Comma) {
                trailing_comma = TrailingComma::Present;
            } else {
                trailing_comma = TrailingComma::Absent;
            }
        }
    }
    trailing_comma
}

/// Return `true` if a [`Stmt`] is preceded by a "comment break"
pub(super) fn has_comment_break(stmt: &Stmt, locator: &Locator) -> bool {
    // Starting from the `Stmt` (`def f(): pass`), we want to detect patterns like
    // this:
    //
    //   import os
    //
    //   # Detached comment.
    //
    //   def f(): pass

    // This should also be detected:
    //
    //   import os
    //
    //   # Detached comment.
    //
    //   # Direct comment.
    //   def f(): pass

    // But this should not:
    //
    //   import os
    //
    //   # Direct comment.
    //   def f(): pass
    let mut seen_blank = false;
    for line in locator.up_to(stmt.start()).universal_newlines().rev() {
        let line = line.trim_whitespace();
        if seen_blank {
            if line.starts_with('#') {
                return true;
            } else if !line.is_empty() {
                break;
            }
        } else {
            if line.is_empty() {
                seen_blank = true;
            } else if line.starts_with('#') || line.starts_with('@') {
                continue;
            } else {
                break;
            }
        }
    }
    false
}
