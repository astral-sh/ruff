//! Extract [`TextRange`] information from AST nodes.
//!
//! For example, given:
//! ```python
//! try:
//!     ...
//! except Exception as e:
//!     ...
//! ```
//!
//! This module can be used to identify the [`TextRange`] of the `except` token.

use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_ast::{Alias, Arg, ArgWithDefault};
use rustpython_parser::ast::{self, ExceptHandler, Ranged, Stmt};

use ruff_python_trivia::{is_python_whitespace, Cursor};

use crate::source_code::Locator;

pub trait Identifier {
    /// Return the [`TextRange`] of the identifier in the given AST node.
    fn identifier(&self) -> TextRange;
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
        while let Some(c) = {
            self.offset += self.cursor.token_len();
            self.cursor.start_token();
            self.cursor.bump()
        } {
            match c {
                c if is_python_identifier_start(c) => {
                    self.cursor.eat_while(is_python_identifier_continue);
                    return Some(TextRange::at(self.offset, self.cursor.token_len()));
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::{TextRange, TextSize};
    use rustpython_ast::{Ranged, Stmt};
    use rustpython_parser::Parse;

    use crate::identifier;
    use crate::identifier::IdentifierTokenizer;
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

    #[test]
    fn extract_global_names() -> Result<()> {
        let contents = r#"global X,Y, Z"#.trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);

        let mut names = IdentifierTokenizer::new(locator.contents(), stmt.range());

        let range = names.next_token().unwrap();
        assert_eq!(&contents[range], "global");
        assert_eq!(range, TextRange::new(TextSize::from(0), TextSize::from(6)));

        let range = names.next_token().unwrap();
        assert_eq!(&contents[range], "X");
        assert_eq!(range, TextRange::new(TextSize::from(7), TextSize::from(8)));

        let range = names.next_token().unwrap();
        assert_eq!(&contents[range], "Y");
        assert_eq!(range, TextRange::new(TextSize::from(9), TextSize::from(10)));

        let range = names.next_token().unwrap();
        assert_eq!(&contents[range], "Z");
        assert_eq!(
            range,
            TextRange::new(TextSize::from(12), TextSize::from(13))
        );
        Ok(())
    }
}
