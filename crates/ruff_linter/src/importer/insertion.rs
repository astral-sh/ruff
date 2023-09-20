//! Insert statements into Python code.
use std::ops::Add;

use ruff_python_ast::{PySourceType, Stmt};
use ruff_python_parser::{lexer, AsMode, Tok};
use ruff_text_size::{Ranged, TextSize};

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_codegen::Stylist;
use ruff_python_trivia::{textwrap::indent, PythonWhitespace};
use ruff_source_file::{Locator, UniversalNewlineIterator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Placement<'a> {
    /// The content will be inserted inline with the existing code (i.e., within semicolon-delimited
    /// statements).
    Inline,
    /// The content will be inserted on its own line.
    OwnLine,
    /// The content will be inserted as an indented block.
    Indented(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Insertion<'a> {
    /// The content to add before the insertion.
    prefix: &'a str,
    /// The location at which to insert.
    location: TextSize,
    /// The content to add after the insertion.
    suffix: &'a str,
    /// The line placement of insertion.
    placement: Placement<'a>,
}

impl<'a> Insertion<'a> {
    /// Create an [`Insertion`] to insert (e.g.) an import statement at the start of a given
    /// file, along with a prefix and suffix to use for the insertion.
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
    /// include a trailing newline.
    pub(super) fn start_of_file(
        body: &[Stmt],
        locator: &Locator,
        stylist: &Stylist,
    ) -> Insertion<'static> {
        // Skip over any docstrings.
        let mut location = if let Some(location) = match_docstring_end(body) {
            // If the first token after the docstring is a semicolon, insert after the semicolon as
            // an inline statement.
            if let Some(offset) = match_leading_semicolon(locator.after(location)) {
                return Insertion::inline(" ", location.add(offset).add(TextSize::of(';')), ";");
            }

            // Otherwise, advance to the next row.
            locator.full_line_end(location)
        } else {
            TextSize::default()
        };

        // Skip over commented lines, with whitespace separation.
        for line in UniversalNewlineIterator::with_offset(locator.after(location), location) {
            let trimmed_line = line.trim_whitespace_start();
            if trimmed_line.is_empty() {
                continue;
            }
            if trimmed_line.starts_with('#') {
                location = line.full_end();
            } else {
                break;
            }
        }

        Insertion::own_line("", location, stylist.line_ending().as_str())
    }

    /// Create an [`Insertion`] to insert (e.g.) an import after the end of the given
    /// [`Stmt`], along with a prefix and suffix to use for the insertion.
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
    /// in this case is the line after `import math`, and will include a trailing newline.
    ///
    /// The statement itself is assumed to be at the top-level of the module.
    pub(super) fn end_of_statement(
        stmt: &Stmt,
        locator: &Locator,
        stylist: &Stylist,
    ) -> Insertion<'static> {
        let location = stmt.end();
        if let Some(offset) = match_leading_semicolon(locator.after(location)) {
            // If the first token after the statement is a semicolon, insert after the semicolon as
            // an inline statement.
            Insertion::inline(" ", location.add(offset).add(TextSize::of(';')), ";")
        } else {
            // Otherwise, insert on the next line.
            Insertion::own_line(
                "",
                locator.full_line_end(location),
                stylist.line_ending().as_str(),
            )
        }
    }

    /// Create an [`Insertion`] to insert (e.g.) an import statement at the start of a given
    /// block, along with a prefix and suffix to use for the insertion.
    ///
    /// For example, given the following code:
    ///
    /// ```python
    /// if TYPE_CHECKING:
    ///     import os
    /// ```
    ///
    /// The insertion returned will begin at the start of the `import os` statement, and will
    /// include a trailing newline.
    ///
    /// The block itself is assumed to be at the top-level of the module.
    pub(super) fn start_of_block(
        mut location: TextSize,
        locator: &Locator<'a>,
        stylist: &Stylist,
        source_type: PySourceType,
    ) -> Insertion<'a> {
        enum Awaiting {
            Colon(u32),
            Newline,
            Indent,
        }

        let mut state = Awaiting::Colon(0);
        for (tok, range) in
            lexer::lex_starts_at(locator.after(location), source_type.as_mode(), location).flatten()
        {
            match state {
                // Iterate until we find the colon indicating the start of the block body.
                Awaiting::Colon(depth) => match tok {
                    Tok::Colon if depth == 0 => {
                        state = Awaiting::Newline;
                    }
                    Tok::Lpar | Tok::Lbrace | Tok::Lsqb => {
                        state = Awaiting::Colon(depth.saturating_add(1));
                    }
                    Tok::Rpar | Tok::Rbrace | Tok::Rsqb => {
                        state = Awaiting::Colon(depth.saturating_sub(1));
                    }
                    _ => {}
                },
                // Once we've seen the colon, we're looking for a newline; otherwise, there's no
                // block body (e.g. `if True: pass`).
                Awaiting::Newline => match tok {
                    Tok::Comment(..) => {}
                    Tok::Newline => {
                        state = Awaiting::Indent;
                    }
                    _ => {
                        location = range.start();
                        break;
                    }
                },
                // Once we've seen the newline, we're looking for the indentation of the block body.
                Awaiting::Indent => match tok {
                    Tok::Comment(..) => {}
                    Tok::NonLogicalNewline => {}
                    Tok::Indent => {
                        // This is like:
                        // ```py
                        // if True:
                        //     pass
                        // ```
                        // Where `range` is the indentation before the `pass` token.
                        return Insertion::indented(
                            "",
                            range.start(),
                            stylist.line_ending().as_str(),
                            locator.slice(range),
                        );
                    }
                    _ => {
                        location = range.start();
                        break;
                    }
                },
            }
        }

        // This is like: `if True: pass`, where `location` is the start of the `pass` token.
        Insertion::inline("", location, "; ")
    }

    /// Convert this [`Insertion`] into an [`Edit`] that inserts the given content.
    pub(super) fn into_edit(self, content: &str) -> Edit {
        let Insertion {
            prefix,
            location,
            suffix,
            placement,
        } = self;
        let content = format!("{prefix}{content}{suffix}");
        Edit::insertion(
            match placement {
                Placement::Indented(indentation) if !indentation.is_empty() => {
                    indent(&content, indentation).to_string()
                }
                _ => content,
            },
            location,
        )
    }

    /// Returns `true` if this [`Insertion`] is inline.
    pub(super) fn is_inline(&self) -> bool {
        matches!(self.placement, Placement::Inline)
    }

    /// Create an [`Insertion`] that inserts content inline (i.e., within semicolon-delimited
    /// statements).
    fn inline(prefix: &'a str, location: TextSize, suffix: &'a str) -> Self {
        Self {
            prefix,
            location,
            suffix,
            placement: Placement::Inline,
        }
    }

    /// Create an [`Insertion`] that starts on its own line.
    fn own_line(prefix: &'a str, location: TextSize, suffix: &'a str) -> Self {
        Self {
            prefix,
            location,
            suffix,
            placement: Placement::OwnLine,
        }
    }

    /// Create an [`Insertion`] that starts on its own line, with the given indentation.
    fn indented(
        prefix: &'a str,
        location: TextSize,
        suffix: &'a str,
        indentation: &'a str,
    ) -> Self {
        Self {
            prefix,
            location,
            suffix,
            placement: Placement::Indented(indentation),
        }
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

/// If a line starts with a semicolon, return its offset.
fn match_leading_semicolon(s: &str) -> Option<TextSize> {
    for (offset, c) in s.char_indices() {
        match c {
            ' ' | '\t' => continue,
            ';' => return Some(TextSize::try_from(offset).unwrap()),
            _ => break,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_ast::PySourceType;
    use ruff_python_codegen::Stylist;
    use ruff_python_parser::lexer::LexResult;
    use ruff_python_parser::{parse_suite, Mode};
    use ruff_source_file::{LineEnding, Locator};
    use ruff_text_size::TextSize;

    use super::Insertion;

    #[test]
    fn start_of_file() -> Result<()> {
        fn insert(contents: &str) -> Result<Insertion> {
            let program = parse_suite(contents, "<filename>")?;
            let tokens: Vec<LexResult> = ruff_python_parser::tokenize(contents, Mode::Module);
            let locator = Locator::new(contents);
            let stylist = Stylist::from_tokens(&tokens, &locator);
            Ok(Insertion::start_of_file(&program, &locator, &stylist))
        }

        let contents = "";
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(0), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!""""#
            .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(19), LineEnding::default().as_str())
        );

        let contents = r#"
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(20), "\n")
        );

        let contents = r#"
"""Hello, world!"""
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(40), "\n")
        );

        let contents = r#"
x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(0), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(23), "\n")
        );

        let contents = r#"
#!/usr/bin/env python3
"""Hello, world!"""
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(43), "\n")
        );

        let contents = r#"
"""Hello, world!"""
#!/usr/bin/env python3
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(43), "\n")
        );

        let contents = r#"
"""%s""" % "Hello, world!"
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::own_line("", TextSize::from(0), "\n")
        );

        let contents = r#"
"""Hello, world!"""; x = 1
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::inline(" ", TextSize::from(20), ";")
        );

        let contents = r#"
"""Hello, world!"""; x = 1; y = \
    2
"#
        .trim_start();
        assert_eq!(
            insert(contents)?,
            Insertion::inline(" ", TextSize::from(20), ";")
        );

        Ok(())
    }

    #[test]
    fn start_of_block() {
        fn insert(contents: &str, offset: TextSize) -> Insertion {
            let tokens: Vec<LexResult> = ruff_python_parser::tokenize(contents, Mode::Module);
            let locator = Locator::new(contents);
            let stylist = Stylist::from_tokens(&tokens, &locator);
            Insertion::start_of_block(offset, &locator, &stylist, PySourceType::default())
        }

        let contents = "if True: pass";
        assert_eq!(
            insert(contents, TextSize::from(0)),
            Insertion::inline("", TextSize::from(9), "; ")
        );

        let contents = r#"
if True:
    pass
"#
        .trim_start();
        assert_eq!(
            insert(contents, TextSize::from(0)),
            Insertion::indented("", TextSize::from(9), "\n", "    ")
        );
    }
}
