//! Extract [`TextRange`] information from AST nodes.
//!
//! In the `RustPython` AST, each node has a `range` field that contains the
//! start and end byte offsets of the node. However, attributes on those
//! nodes may not have their own ranges. In particular, identifiers are
//! not given their own ranges, unless they're part of a name expression.
//!
//! For example, given:
//! ```python
//! def f():
//!     ...
//! ```
//!
//! The statement defining `f` has a range, but the identifier `f` does not.
//!
//! This module assists with extracting [`TextRange`] ranges from AST nodes
//! via manual lexical analysis.

use std::ops::{Add, Sub};
use std::str::Chars;

use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_ast::{Alias, Arg};
use rustpython_parser::ast::{self, Excepthandler, Ranged, Stmt};

use ruff_python_whitespace::is_python_whitespace;

use crate::source_code::Locator;

/// Return the [`TextRange`] of the identifier in the given statement.
///
/// For example, return the range of `f` in:
/// ```python
/// def f():
///     ...
/// ```
pub fn statement(stmt: &Stmt, locator: &Locator) -> TextRange {
    match stmt {
        Stmt::ClassDef(ast::StmtClassDef {
            decorator_list,
            range,
            ..
        })
        | Stmt::FunctionDef(ast::StmtFunctionDef {
            decorator_list,
            range,
            ..
        }) => {
            let range = decorator_list.last().map_or(*range, |last_decorator| {
                TextRange::new(last_decorator.end(), range.end())
            });

            // The first "identifier" is the `def` or `class` keyword.
            // The second "identifier" is the function or class name.
            IdentifierTokenizer::starts_at(range.start(), locator.contents())
                .nth(1)
                .expect("Unable to identify identifier in function or class definition")
        }
        Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
            decorator_list,
            range,
            ..
        }) => {
            let range = decorator_list.last().map_or(*range, |last_decorator| {
                TextRange::new(last_decorator.end(), range.end())
            });

            // The first "identifier" is the `async` keyword.
            // The second "identifier" is the `def` or `class` keyword.
            // The third "identifier" is the function or class name.
            IdentifierTokenizer::starts_at(range.start(), locator.contents())
                .nth(2)
                .expect("Unable to identify identifier in function or class definition")
        }
        _ => stmt.range(),
    }
}

/// Return the [`TextRange`] for the identifier defining an [`Arg`].
///
/// For example, return the range of `x` in:
/// ```python
/// def f(x: int = 0):
///     ...
/// ```
pub fn arg(arg: &Arg, locator: &Locator) -> TextRange {
    IdentifierTokenizer::new(locator.contents(), arg.range())
        .next()
        .expect("Failed to find argument identifier")
}

/// Return the [`TextRange`] for the identifier defining an [`Alias`].
///
/// For example, return the range of `x` in:
/// ```python
/// from foo import bar as x
/// ```
pub fn alias(alias: &Alias, locator: &Locator) -> TextRange {
    if alias.asname.is_none() {
        // The first identifier is the module name.
        IdentifierTokenizer::new(locator.contents(), alias.range())
            .next()
            .expect("Failed to find alias identifier")
    } else {
        // The first identifier is the module name.
        // The second identifier is the "as" keyword.
        // The third identifier is the alias name.
        IdentifierTokenizer::new(locator.contents(), alias.range())
            .nth(2)
            .expect("Failed to find alias identifier")
    }
}

/// Return the ranges of [`Tok::Name`] tokens within a specified node.
///
/// For example, return the ranges of `x` and `y` in:
/// ```python
/// global x, y
/// ```
pub fn names<'a>(stmt: &Stmt, locator: &'a Locator<'a>) -> impl Iterator<Item = TextRange> + 'a {
    // Given `global x, y`, the first identifier is `global`, and the remaining identifiers are
    // the names.
    IdentifierTokenizer::new(locator.contents(), stmt.range()).skip(1)
}

/// Return the [`TextRange`] of a named exception in an [`Excepthandler`].
///
/// For example, return the range of `e` in:
/// ```python
/// try:
///     ...
/// except ValueError as e:
///     ...
/// ```
pub fn exception_range(handler: &Excepthandler, locator: &Locator) -> Option<TextRange> {
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = handler;

    let Some(type_) = type_ else {
        return None;
    };

    // The exception name is the first identifier token after the `as` keyword.
    IdentifierTokenizer::starts_at(type_.end(), locator.contents()).nth(1)
}

/// Return the [`TextRange`] of the `except` token in an [`Excepthandler`].
pub fn except_range(handler: &Excepthandler, locator: &Locator) -> TextRange {
    IdentifierTokenizer::new(locator.contents(), handler.range())
        .next()
        .expect("Failed to find `except` token in `Excepthandler`")
}

/// Return `true` if the given character is a valid identifier character.
fn is_python_identifier(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

/// Simple zero allocation tokenizer for tokenizing trivia (and some tokens).
///
/// The tokenizer must start at an offset that is trivia (e.g. not inside of a multiline string).
///
/// The tokenizer doesn't guarantee any correctness after it returned a [`TokenKind::Other`]. That's why it
/// will return [`TokenKind::Bogus`] for every character after until it reaches the end of the file.
pub(crate) struct IdentifierTokenizer<'a> {
    cursor: Cursor<'a>,
    offset: TextSize,
}

impl<'a> IdentifierTokenizer<'a> {
    pub(crate) fn new(source: &'a str, range: TextRange) -> Self {
        Self {
            cursor: Cursor::new(&source[range]),
            offset: range.start(),
        }
    }

    pub(crate) fn starts_at(offset: TextSize, source: &'a str) -> Self {
        let range = TextRange::new(offset, source.text_len());
        Self::new(source, range)
    }

    fn next_token(&mut self) -> Option<TextRange> {
        while let Some(c) = self.cursor.bump() {
            match c {
                c if is_python_identifier(c) => {
                    let start = self.offset.add(self.cursor.offset()).sub(c.text_len());
                    self.cursor.eat_while(is_python_identifier);
                    let end = self.offset.add(self.cursor.offset());
                    return Some(TextRange::new(start, end));
                }

                c if is_python_whitespace(c) => {
                    self.cursor.eat_while(is_python_whitespace);
                }

                '#' => {
                    self.cursor.eat_while(|c| !matches!(c, '\n' | '\r'));
                }

                '\r' => {
                    self.cursor.eat_char('\n');
                }

                '\n' => {
                    // Nothing to do.
                }

                '\\' => {
                    // Nothing to do.
                }

                _ => {
                    // Nothing to do.
                }
            };
        }

        None
    }
}

impl Iterator for IdentifierTokenizer<'_> {
    type Item = TextRange;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

const EOF_CHAR: char = '\0';

#[derive(Debug, Clone)]
struct Cursor<'a> {
    chars: Chars<'a>,
    offset: TextSize,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars(),
            offset: TextSize::from(0),
        }
    }

    const fn offset(&self) -> TextSize {
        self.offset
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the file is at the end of the file.
    fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Returns `true` if the file is at the end of the file.
    fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Consumes the next character.
    fn bump(&mut self) -> Option<char> {
        if let Some(char) = self.chars.next() {
            self.offset += char.text_len();
            Some(char)
        } else {
            None
        }
    }

    /// Eats the next character if it matches the given character.
    fn eat_char(&mut self, c: char) -> bool {
        if self.first() == c {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::{TextLen, TextRange, TextSize};
    use rustpython_ast::Stmt;
    use rustpython_parser::Parse;

    use crate::helpers::{elif_else_range, else_range, first_colon_range};
    use crate::identifier;
    use crate::source_code::Locator;

    #[test]
    fn extract_arg_range() -> Result<()> {
        let contents = "def f(x): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let function_def = stmt.as_function_def_stmt().unwrap();
        let args = &function_def.args.args;
        let arg = &args[0];
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::arg(arg, &locator),
            TextRange::new(TextSize::from(6), TextSize::from(7))
        );

        let contents = "def f(x: int): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let function_def = stmt.as_function_def_stmt().unwrap();
        let args = &function_def.args.args;
        let arg = &args[0];
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::arg(arg, &locator),
            TextRange::new(TextSize::from(6), TextSize::from(7))
        );

        let contents = r#"
def f(
    x: int,  # Comment
):
    pass
"#
        .trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let function_def = stmt.as_function_def_stmt().unwrap();
        let args = &function_def.args.args;
        let arg = &args[0];
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::arg(arg, &locator),
            TextRange::new(TextSize::from(11), TextSize::from(12))
        );

        Ok(())
    }

    #[test]
    fn extract_identifier_range() -> Result<()> {
        let contents = "def f(): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(4), TextSize::from(5))
        );

        let contents = "async def f(): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(10), TextSize::from(11))
        );

        let contents = r#"
def \
  f():
  pass
"#
        .trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(8), TextSize::from(9))
        );

        let contents = "class Class(): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(6), TextSize::from(11))
        );

        let contents = "class Class: pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(6), TextSize::from(11))
        );

        let contents = r#"
@decorator()
class Class():
  pass
"#
        .trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(19), TextSize::from(24))
        );

        let contents = r#"
@decorator()  # Comment
class Class():
  pass
"#
        .trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(30), TextSize::from(35))
        );

        let contents = r#"x = y + 1"#.trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier::statement(&stmt, &locator),
            TextRange::new(TextSize::from(0), TextSize::from(9))
        );

        Ok(())
    }

    #[test]
    fn extract_else_range() -> Result<()> {
        let contents = r#"
for x in y:
    pass
else:
    pass
"#
        .trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        let range = else_range(&stmt, &locator).unwrap();
        assert_eq!(&contents[range], "else");
        assert_eq!(
            range,
            TextRange::new(TextSize::from(21), TextSize::from(25))
        );
        Ok(())
    }

    #[test]
    fn extract_first_colon_range() {
        let contents = "with a: pass";
        let locator = Locator::new(contents);
        let range = first_colon_range(
            TextRange::new(TextSize::from(0), contents.text_len()),
            &locator,
        )
        .unwrap();
        assert_eq!(&contents[range], ":");
        assert_eq!(range, TextRange::new(TextSize::from(6), TextSize::from(7)));
    }

    #[test]
    fn extract_elif_else_range() -> Result<()> {
        let contents = "if a:
    ...
elif b:
    ...
";
        let stmt = Stmt::parse(contents, "<filename>")?;
        let stmt = Stmt::as_if_stmt(&stmt).unwrap();
        let locator = Locator::new(contents);
        let range = elif_else_range(stmt, &locator).unwrap();
        assert_eq!(range.start(), TextSize::from(14));
        assert_eq!(range.end(), TextSize::from(18));

        let contents = "if a:
    ...
else:
    ...
";
        let stmt = Stmt::parse(contents, "<filename>")?;
        let stmt = Stmt::as_if_stmt(&stmt).unwrap();
        let locator = Locator::new(contents);
        let range = elif_else_range(stmt, &locator).unwrap();
        assert_eq!(range.start(), TextSize::from(14));
        assert_eq!(range.end(), TextSize::from(18));

        Ok(())
    }
}
