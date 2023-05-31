use ruff_text_size::TextSize;
use rustpython_parser::ast::{Ranged, Stmt};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::source_code::{Locator, Stylist};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Insertion {
    /// The content to add before the insertion.
    prefix: &'static str,
    /// The location at which to insert.
    location: TextSize,
    /// The content to add after the insertion.
    suffix: &'static str,
}

impl Insertion {
    /// Create an [`Insertion`] to insert (e.g.) an import after the end of the given [`Stmt`],
    /// along with a prefix and suffix to use for the insertion.
    ///
    /// For example, given the following code:
    ///
    /// ```python
    /// """Hello, world!"""
    ///
    /// import os
    /// import math
    ///
    ///
    /// def foo():
    ///     pass
    /// ```
    ///
    /// The insertion returned will begin after the newline after the last import statement, which
    /// in this case is the line after `import math`, and will include a trailing newline suffix.
    pub(super) fn end_of_statement(stmt: &Stmt, locator: &Locator, stylist: &Stylist) -> Insertion {
        let location = stmt.end();
        let mut tokens =
            lexer::lex_starts_at(locator.after(location), Mode::Module, location).flatten();
        if let Some((Tok::Semi, range)) = tokens.next() {
            // If the first token after the docstring is a semicolon, insert after the semicolon as an
            // inline statement;
            Insertion::new(" ", range.end(), ";")
        } else {
            // Otherwise, insert on the next line.
            Insertion::new(
                "",
                locator.full_line_end(location),
                stylist.line_ending().as_str(),
            )
        }
    }

    /// Create an [`Insertion`] to insert (e.g.) an import statement at the "top" of a given file,
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
    /// The insertion returned will begin at the start of the `import os` statement, and will
    /// include a trailing newline suffix.
    pub(super) fn start_of_file(body: &[Stmt], locator: &Locator, stylist: &Stylist) -> Insertion {
        // Skip over any docstrings.
        let mut location = if let Some(location) = match_docstring_end(body) {
            // If the first token after the docstring is a semicolon, insert after the semicolon as an
            // inline statement;
            let first_token = lexer::lex_starts_at(locator.after(location), Mode::Module, location)
                .flatten()
                .next();
            if let Some((Tok::Semi, range)) = first_token {
                return Insertion::new(" ", range.end(), ";");
            }

            // Otherwise, advance to the next row.
            locator.full_line_end(location)
        } else {
            TextSize::default()
        };

        // Skip over any comments and empty lines.
        for (tok, range) in
            lexer::lex_starts_at(locator.after(location), Mode::Module, location).flatten()
        {
            if matches!(tok, Tok::Comment(..) | Tok::Newline) {
                location = locator.full_line_end(range.end());
            } else {
                break;
            }
        }

        Insertion::new("", location, stylist.line_ending().as_str())
    }

    fn new(prefix: &'static str, location: TextSize, suffix: &'static str) -> Self {
        Self {
            prefix,
            location,
            suffix,
        }
    }

    /// Convert this [`Insertion`] into an [`Edit`] that inserts the given content.
    pub(super) fn into_edit(self, content: &str) -> Edit {
        let Insertion {
            prefix,
            location,
            suffix,
        } = self;
        Edit::insertion(format!("{prefix}{content}{suffix}"), location)
    }
}

/// Find the end of the last docstring.
fn match_docstring_end(body: &[Stmt]) -> Option<TextSize> {
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
    Some(stmt.end())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::TextSize;
    use rustpython_parser::ast::Suite;
    use rustpython_parser::lexer::LexResult;
    use rustpython_parser::Parse;

    use ruff_newlines::LineEnding;
    use ruff_python_ast::source_code::{Locator, Stylist};

    use super::Insertion;

    fn insert(contents: &str) -> Result<Insertion> {
        let program = Suite::parse(contents, "<filename>")?;
        let tokens: Vec<LexResult> = ruff_rustpython::tokenize(contents);
        let locator = Locator::new(contents);
        let stylist = Stylist::from_tokens(&tokens, &locator);
        Ok(Insertion::start_of_file(&program, &locator, &stylist))
    }

    #[test]
    fn start_of_file() -> Result<()> {
        let contents = "";
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!""""#
            .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(19), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(20), "\n")
        );

        let contents = r#"
"""Hello, world!"""
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(40), "\n")
        );

        let contents = r#"
x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(23), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(43), "\n")
        );

        let contents = r#"
"""Hello, world!"""
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(43), "\n")
        );

        let contents = r#"
"""%s""" % "Hello, world!"
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new("", TextSize::from(0), "\n")
        );

        let contents = r#"
"""Hello, world!"""; x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", TextSize::from(20), ";")
        );

        let contents = r#"
"""Hello, world!"""; x = 1; y = \
    2
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::new(" ", TextSize::from(20), ";")
        );

        Ok(())
    }
}
