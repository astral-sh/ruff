use rustpython_parser::ast::{Location, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::{Locator, Stylist};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Insertion {
    /// The content to add before the insertion.
    pub prefix: &'static str,
    /// The location at which to insert.
    pub location: Location,
    /// The content to add after the insertion.
    pub suffix: &'static str,
}

impl Insertion {
    pub fn new(prefix: &'static str, location: Location, suffix: &'static str) -> Self {
        Self {
            prefix,
            location,
            suffix,
        }
    }
}

/// Find the location at which a "top-of-file" import should be inserted,
/// along with a prefix and suffix to use for the insertion.
///
/// For example, given the following code:
///
/// ```python
/// """Hello, world!"""
///
/// import os
/// ```
///
/// The location returned will be the start of the `import os` statement,
/// along with a trailing newline suffix.
pub(super) fn top_of_file_insertion(
    body: &[Stmt],
    locator: &Locator,
    stylist: &Stylist,
) -> Insertion {
    // Skip over any docstrings.
    let mut location = if let Some(location) = match_docstring_end(body) {
        // If the first token after the docstring is a semicolon, insert after the semicolon as an
        // inline statement;
        let first_token = lexer::lex_located(locator.skip(location), Mode::Module, location)
            .flatten()
            .next();
        if let Some((.., Tok::Semi, end)) = first_token {
            return Insertion::new(" ", end, ";");
        }

        // Otherwise, advance to the next row.
        Location::new(location.row() + 1, 0)
    } else {
        Location::default()
    };

    // Skip over any comments and empty lines.
    for (.., tok, end) in
        lexer::lex_located(locator.skip(location), Mode::Module, location).flatten()
    {
        if matches!(tok, Tok::Comment(..) | Tok::Newline) {
            location = Location::new(end.row() + 1, 0);
        } else {
            break;
        }
    }

    return Insertion::new("", location, stylist.line_ending().as_str());
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser as parser;
    use rustpython_parser::ast::Location;
    use rustpython_parser::lexer::LexResult;

    use ruff_python_ast::source_code::{LineEnding, Locator, Stylist};

    use crate::rules::isort::helpers::{top_of_file_insertion, Insertion};

    fn insert(contents: &str) -> Result<Insertion> {
        let program = parser::parse_program(contents, "<filename>")?;
        let tokens: Vec<LexResult> = ruff_rustpython::tokenize(contents);
        let locator = Locator::new(contents);
        let stylist = Stylist::from_tokens(&tokens, &locator);
        Ok(top_of_file_insertion(&program, &locator, &stylist))
    }

    #[test]
    fn top_of_file_insertions() -> Result<()> {
        let contents = "";
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(1, 0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!""""#
            .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(2, 0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(2, 0), "\n")
        );

        let contents = r#"
"""Hello, world!"""
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(3, 0), "\n")
        );

        let contents = r#"
x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(1, 0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(2, 0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(3, 0), "\n")
        );

        let contents = r#"
"""Hello, world!"""
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(3, 0), "\n")
        );

        let contents = r#"
"""%s""" % "Hello, world!"
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", Location::new(1, 0), "\n")
        );

        let contents = r#"
"""Hello, world!"""; x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", Location::new(1, 20), ";")
        );

        let contents = r#"
"""Hello, world!"""; x = 1; y = \
    2
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", Location::new(1, 20), ";")
        );

        Ok(())
    }
}
