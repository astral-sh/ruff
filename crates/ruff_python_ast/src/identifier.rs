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
use rustpython_ast::{Alias, Arg, ArgWithDefault, Pattern};
use rustpython_parser::ast::{self, ExceptHandler, Ranged, Stmt};

use ruff_python_whitespace::is_python_whitespace;

use crate::source_code::Locator;

pub trait Identifier {
    /// Return the [`TextRange`] of the identifier in the given AST node.
    fn identifier(&self) -> TextRange;
}

pub trait TryIdentifier {
    /// Return the [`TextRange`] of the identifier in the given AST node, or `None` if
    /// the node does not have an identifier.
    fn try_identifier(&self) -> Option<TextRange>;
}

impl Identifier for Stmt {
    /// Return the [`TextRange`] of the identifier in the given statement.
    ///
    /// For example, return the range of `f` in:
    /// ```python
    /// def f():
    ///     ...
    /// ```
    fn identifier(&self) -> TextRange {
        match self {
            Stmt::ClassDef(ast::StmtClassDef { name, .. })
            | Stmt::FunctionDef(ast::StmtFunctionDef { name, .. })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { name, .. }) => name.range(),
            _ => self.range(),
        }
    }
}

impl Identifier for Arg {
    /// Return the [`TextRange`] for the identifier defining an [`Arg`].
    ///
    /// For example, return the range of `x` in:
    /// ```python
    /// def f(x: int):
    ///     ...
    /// ```
    fn identifier(&self) -> TextRange {
        self.arg.range()
    }
}

impl Identifier for ArgWithDefault {
    /// Return the [`TextRange`] for the identifier defining an [`ArgWithDefault`].
    ///
    /// For example, return the range of `x` in:
    /// ```python
    /// def f(x: int = 0):
    ///     ...
    /// ```
    fn identifier(&self) -> TextRange {
        self.def.identifier()
    }
}

impl Identifier for Alias {
    /// Return the [`TextRange`] for the identifier defining an [`Alias`].
    ///
    /// For example, return the range of `x` in:
    /// ```python
    /// from foo import bar as x
    /// ```
    fn identifier(&self) -> TextRange {
        self.asname
            .as_ref()
            .map_or_else(|| self.name.range(), Ranged::range)
    }
}

impl TryIdentifier for Pattern {
    /// Return the [`TextRange`] of the identifier in the given pattern.
    ///
    /// For example, return the range of `z` in:
    /// ```python
    /// match x:
    ///     # Pattern::MatchAs
    ///     case z:
    ///         ...
    /// ```
    ///
    /// Or:
    /// ```python
    /// match x:
    ///     # Pattern::MatchAs
    ///     case y as z:
    ///         ...
    /// ```
    ///
    /// Or :
    /// ```python
    /// match x:
    ///     # Pattern::MatchMapping
    ///     case {"a": 1, **z}
    ///         ...
    /// ```
    ///
    /// Or :
    /// ```python
    /// match x:
    ///     # Pattern::MatchStar
    ///     case *z:
    ///         ...
    /// ```
    fn try_identifier(&self) -> Option<TextRange> {
        let name = match self {
            Pattern::MatchAs(ast::PatternMatchAs {
                name: Some(name), ..
            }) => Some(name),
            Pattern::MatchMapping(ast::PatternMatchMapping {
                rest: Some(rest), ..
            }) => Some(rest),
            Pattern::MatchStar(ast::PatternMatchStar {
                name: Some(name), ..
            }) => Some(name),
            _ => None,
        };
        name.map(Ranged::range)
    }
}

impl TryIdentifier for ExceptHandler {
    /// Return the [`TextRange`] of a named exception in an [`ExceptHandler`].
    ///
    /// For example, return the range of `e` in:
    /// ```python
    /// try:
    ///     ...
    /// except ValueError as e:
    ///     ...
    /// ```
    fn try_identifier(&self) -> Option<TextRange> {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { name, .. }) = self;
        name.as_ref().map(Ranged::range)
    }
}

/// Return the [`TextRange`] for every name in a [`Stmt`].
///
/// Intended to be used for `global` and `nonlocal` statements.
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

/// Return the [`TextRange`] of the `except` token in an [`ExceptHandler`].
pub fn except(handler: &ExceptHandler, locator: &Locator) -> TextRange {
    IdentifierTokenizer::new(locator.contents(), handler.range())
        .next()
        .expect("Failed to find `except` token in `ExceptHandler`")
}

/// Return the [`TextRange`] of the `else` token in a `For`, `AsyncFor`, or `While` statement.
pub fn else_(stmt: &Stmt, locator: &Locator) -> Option<TextRange> {
    let (Stmt::For(ast::StmtFor { body, orelse, .. })
    | Stmt::AsyncFor(ast::StmtAsyncFor { body, orelse, .. })
    | Stmt::While(ast::StmtWhile { body, orelse, .. })) = stmt
    else {
        return None;
    };

    if orelse.is_empty() {
        return None;
    }

    IdentifierTokenizer::starts_at(
        body.last().expect("Expected body to be non-empty").end(),
        locator.contents(),
    )
    .next()
}

/// Return `true` if the given character starts a valid Python identifier.
///
/// Python identifiers must start with an alphabetic character or an underscore.
fn is_python_identifier_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Return `true` if the given character is a valid Python identifier continuation character.
///
/// Python identifiers can contain alphanumeric characters and underscores, but cannot start with a
/// number.
fn is_python_identifier_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Simple zero allocation tokenizer for Python identifiers.
///
/// The tokenizer must operate over a range that can only contain identifiers, keywords, and
/// comments (along with whitespace and continuation characters). It does not support other tokens,
/// like operators, literals, or delimiters. It also does not differentiate between keywords and
/// identifiers, treating every valid token as an "identifier".
///
/// This is useful for cases like, e.g., identifying the alias name in an aliased import (`bar` in
/// `import foo as bar`), where we're guaranteed to only have identifiers and keywords in the
/// relevant range.
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
                c if is_python_identifier_start(c) => {
                    let start = self.offset.add(self.cursor.offset()).sub(c.text_len());
                    self.cursor.eat_while(is_python_identifier_continue);
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
    use ruff_text_size::{TextRange, TextSize};
    use rustpython_ast::Stmt;
    use rustpython_parser::Parse;

    use crate::identifier;
    use crate::source_code::Locator;

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
        let range = identifier::else_(&stmt, &locator).unwrap();
        assert_eq!(&contents[range], "else");
        assert_eq!(
            range,
            TextRange::new(TextSize::from(21), TextSize::from(25))
        );
        Ok(())
    }
}
