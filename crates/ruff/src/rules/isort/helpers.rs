use rustpython_parser::ast::{Location, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::Locator;

use super::types::TrailingComma;

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
    for line in locator.take(stmt.location).universal_newlines().rev() {
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

/// Find the end of the last docstring.
fn match_docstring_end(body: &[Stmt]) -> Option<Location> {
    let mut iter = body.iter();
    let Some(mut stmt) = iter.next() else {
        return None;
    };
    if !is_docstring_stmt(stmt) {
        return None;
    }
    for next in iter {
        if !is_docstring_stmt(next) {
            break;
        }
        stmt = next;
    }
    Some(stmt.end_location.unwrap())
}

/// Find the end of the first token that isn't a docstring, comment, or
/// whitespace.
pub fn find_splice_location(body: &[Stmt], locator: &Locator) -> Location {
    // Find the first AST node that isn't a docstring.
    let mut splice = match_docstring_end(body).unwrap_or_default();

    // Find the first token that isn't a comment or whitespace.
    let contents = locator.skip(splice);
    for (.., tok, end) in lexer::lex_located(contents, Mode::Module, splice).flatten() {
        if matches!(tok, Tok::Comment(..) | Tok::Newline) {
            splice = end;
        } else {
            break;
        }
    }

    splice
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser as parser;
    use rustpython_parser::ast::Location;

    use ruff_python_ast::source_code::Locator;

    use super::find_splice_location;

    fn splice_contents(contents: &str) -> Result<Location> {
        let program = parser::parse_program(contents, "<filename>")?;
        let locator = Locator::new(contents);
        Ok(find_splice_location(&program, &locator))
    }

    #[test]
    fn splice() -> Result<()> {
        let contents = "";
        assert_eq!(splice_contents(contents)?, Location::new(1, 0));

        let contents = r#"
"""Hello, world!"""
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(1, 19));

        let contents = r#"
"""Hello, world!"""
"""Hello, world!"""
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(2, 19));

        let contents = r#"
x = 1
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(1, 0));

        let contents = r#"
#!/usr/bin/env python3
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(1, 22));

        let contents = r#"
#!/usr/bin/env python3
"""Hello, world!"""
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(2, 19));

        let contents = r#"
"""Hello, world!"""
#!/usr/bin/env python3
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(2, 22));

        let contents = r#"
"""%s""" % "Hello, world!"
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(1, 0));

        let contents = r#"
"""Hello, world!"""; x = 1
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(1, 19));

        let contents = r#"
"""Hello, world!"""; x = 1; y = \
    2
"#
        .trim();
        assert_eq!(splice_contents(contents)?, Location::new(1, 19));

        Ok(())
    }
}
