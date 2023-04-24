use rustpython_parser::ast::Stmt;
use rustpython_parser::{lexer, Mode, Tok};

use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::Locator;

use crate::rules::isort::types::TrailingComma;

/// Return `true` if a `StmtKind::ImportFrom` statement ends with a magic
/// trailing comma.
pub fn trailing_comma(stmt: &Stmt, locator: &Locator) -> TrailingComma {
    let contents = locator.slice(stmt);
    let mut count: usize = 0;
    let mut trailing_comma = TrailingComma::Absent;
    for (_, tok, _) in lexer::lex_located(contents, Mode::Module, stmt.location).flatten() {
        if matches!(tok, Tok::Lpar) {
            count += 1;
        }
        if matches!(tok, Tok::Rpar) {
            count -= 1;
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
pub fn has_comment_break(stmt: &Stmt, locator: &Locator) -> bool {
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
    for line in locator.up_to(stmt.location).universal_newlines().rev() {
        let line = line.trim();
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
