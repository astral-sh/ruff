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

use itertools::Itertools;
use log::error;
use ruff_text_size::{TextRange, TextSize};
use rustpython_ast::{Alias, Arg, Expr};
use rustpython_parser::ast::{self, Cmpop, Excepthandler, Ranged, Stmt};
use rustpython_parser::{lexer, Tok};

use crate::prelude::Mode;
use crate::source_code::Locator;

/// Return `true` if the given character is a valid identifier character.
fn is_identifier(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub struct IdentifierIterator<'a> {
    /// The locator to use for lexical analysis.
    locator: &'a Locator<'a>,
    /// The current state of the iterator.
    state: IdentifierIteratorState,
}

impl<'a> IdentifierIterator<'a> {
    pub fn new(locator: &'a Locator<'a>) -> Self {
        IdentifierIterator {
            locator,
            state: IdentifierIteratorState::AwaitingIdentifier { index: 0 },
        }
    }
}

impl<'a> Iterator for IdentifierIterator<'a> {
    type Item = TextRange;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.state {
                IdentifierIteratorState::AwaitingIdentifier { index } => {
                    let mut start = None;
                    let mut end = None;
                    for (char_index, char) in self.locator.slice().char_indices() {
                        if is_identifier(char) {
                            if start.is_none() {
                                start = Some(char_index);
                            }
                            end = Some(char_index + char.len_utf8());
                        } else if start.is_some() {
                            // We've found the end of the identifier.
                            let start = start.unwrap();
                            let end = end.unwrap();
                            self.state = IdentifierIteratorState::InIdentifier {
                                index: *index,
                                start: self.locator.offset(start),
                                end: self.locator.offset(end),
                            };
                            return Some(TextRange::new(
                                self.locator.offset(start),
                                self.locator.offset(end),
                            ));
                        }
                    }

                    // We've reached the end of the file.
                    return None;
                }
                IdentifierIteratorState::InIdentifier { index, start, end } => {
                    // We've found the identifier we're looking for.
                    self.state = IdentifierIteratorState::AwaitingIdentifier { index: index + 1 };
                    return Some(TextRange::new(*start, *end));
                }
            }
        }
    }
}

/// Return the [`TextRange`] of the identifier in the given statement.
///
/// For example, return the range of `f` in:
/// ```python
/// def f():
///     ...
/// ```
pub fn identifier_range(stmt: &Stmt, locator: &Locator) -> TextRange {
    #[derive(Debug)]
    enum IdentifierState {
        /// We're in a comment, awaiting the identifier at the given index.
        InComment { index: usize },
        /// We're looking for the identifier at the given index.
        AwaitingIdentifier { index: usize },
        /// We're in the identifier at the given index, starting at the given character.
        InIdentifier { index: usize, start: TextSize },
    }

    // STOPSHIP: We can generalize this... we can make an identifier iterator, that skips comments,
    // and then we can just take the nth identifier.

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
        })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
            decorator_list,
            range,
            ..
        }) => {
            let header_range = decorator_list.last().map_or(*range, |last_decorator| {
                TextRange::new(last_decorator.end(), range.end())
            });

            // If the statement is an async function, we're looking for the third
            // keyword-or-identifier (`foo` in `async def foo()`). Otherwise, it's the
            // second keyword-or-identifier (`foo` in `def foo()` or `Foo` in `class Foo`).
            let name_index = if stmt.is_async_function_def_stmt() {
                2
            } else {
                1
            };

            let mut state = IdentifierState::AwaitingIdentifier { index: 0 };
            for (char_index, char) in locator.slice(header_range).char_indices() {
                match state {
                    IdentifierState::AwaitingIdentifier { index } => match char {
                        // Read until we hit an identifier.
                        '#' => {
                            state = IdentifierState::InComment { index };
                        }
                        c if is_identifier(c) => {
                            state = IdentifierState::InIdentifier {
                                index,
                                start: TextSize::try_from(char_index).unwrap(),
                            };
                        }
                        _ => {}
                    },
                    IdentifierState::InComment { index } => match char {
                        // Read until the end of the comment.
                        '\r' | '\n' => {
                            state = IdentifierState::AwaitingIdentifier { index };
                        }
                        _ => {}
                    },
                    IdentifierState::InIdentifier { index, start } => {
                        // We've reached the end of the identifier.
                        if !is_identifier(char) {
                            if index == name_index {
                                // We've found the identifier we're looking for.
                                let end = TextSize::try_from(char_index).unwrap();
                                return TextRange::new(
                                    header_range.start().add(start),
                                    header_range.start().add(end),
                                );
                            }

                            // We're looking for a different identifier.
                            state = IdentifierState::AwaitingIdentifier { index: index + 1 };
                        }
                    }
                }
            }

            error!("Failed to find identifier for {:?}", stmt);
            header_range
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
pub fn arg_range(arg: &Arg, locator: &Locator) -> TextRange {
    // Return the first identifier token.
    let contents = locator.slice(arg.range());
    let size = contents.chars().take_while(|c| is_identifier(*c)).count();
    TextRange::new(arg.start(), arg.start() + TextSize::try_from(size).unwrap())
}

/// Return the [`TextRange`] for the identifier defining an [`Alias`].
///
/// For example, return the range of `x` in:
/// ```python
/// from foo import bar as x
/// ```
pub fn alias_range(alias: &Alias, locator: &Locator) -> TextRange {
    // The identifier is the first token in the argument, so continue until we hit
    // the first non-identifier character.
    let contents = locator.slice(alias.range());

    if alias.asname.is_none() {
        // Return the first identifier token (the name).
        alias.name.range()
    } else {
        // Return the third identifier token (the alias).
    }
    let size = contents.chars().take_while(|c| is_identifier(*c)).count();
    TextRange::new(
        alias.start(),
        alias.start() + TextSize::try_from(size).unwrap(),
    )
}

/// Return the ranges of [`Tok::Name`] tokens within a specified node.
///
/// For example, return the ranges of `x` and `y` in:
/// ```python
/// global x, y
/// ```
pub fn find_names<'a, T>(
    located: &'a T,
    locator: &'a Locator,
) -> impl Iterator<Item = TextRange> + 'a
where
    T: Ranged,
{
    // These names have to be comma-separated.
    let contents = locator.slice(located.range());

    // Skip the keyword (`global` or `nonlocal`).

    // Skip the

    lexer::lex_starts_at(contents, Mode::Module, located.start())
        .flatten()
        .filter(|(tok, _)| tok.is_name())
        .map(|(_, range)| range)
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
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler {
        name,
        type_,
        body,
        range: _range,
    }) = handler;

    match (name, type_) {
        (Some(_), Some(type_)) => {
            let contents = &locator.contents()[TextRange::new(type_.end(), body[0].start())];

            lexer::lex_starts_at(contents, Mode::Module, type_.end())
                .flatten()
                .tuple_windows()
                .find(|(tok, next_tok)| {
                    matches!(tok.0, Tok::As) && matches!(next_tok.0, Tok::Name { .. })
                })
                .map(|((..), (_, range))| range)
        }
        _ => None,
    }
}

/// Return the [`TextRange`] of the `except` token in an [`Excepthandler`].
pub fn except_range(handler: &Excepthandler, locator: &Locator) -> TextRange {
    let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { body, type_, .. }) = handler;
    let end = if let Some(type_) = type_ {
        type_.end()
    } else {
        body.first().expect("Expected body to be non-empty").start()
    };
    let contents = &locator.contents()[TextRange::new(handler.start(), end)];

    lexer::lex_starts_at(contents, Mode::Module, handler.start())
        .flatten()
        .find(|(tok, _)| tok.is_except())
        .map(|(_, range)| range)
        .expect("Failed to find `except` range")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedCmpop {
    pub range: TextRange,
    pub op: Cmpop,
}

impl LocatedCmpop {
    fn new<T: Into<TextRange>>(range: T, op: Cmpop) -> Self {
        Self {
            range: range.into(),
            op,
        }
    }
}

/// Extract all [`Cmpop`] operators from an expression snippet, with appropriate
/// ranges.
///
/// `RustPython` doesn't include line and column information on [`Cmpop`] nodes.
/// `CPython` doesn't either. This method iterates over the token stream and
/// re-identifies [`Cmpop`] nodes, annotating them with valid ranges.
pub fn locate_cmpops(expr: &Expr, locator: &Locator) -> Vec<LocatedCmpop> {
    // If `Expr` is a multi-line expression, we need to parenthesize it to
    // ensure that it's lexed correctly.
    let contents = locator.slice(expr.range());
    let parenthesized_contents = format!("({contents})");
    let mut tok_iter = lexer::lex(&parenthesized_contents, Mode::Expression)
        .flatten()
        .skip(1)
        .map(|(tok, range)| (tok, range.sub(TextSize::from(1))))
        .filter(|(tok, _)| !matches!(tok, Tok::NonLogicalNewline | Tok::Comment(_)))
        .peekable();

    let mut ops: Vec<LocatedCmpop> = vec![];
    let mut count = 0u32;
    loop {
        let Some((tok, range)) = tok_iter.next() else {
            break;
        };
        if matches!(tok, Tok::Lpar) {
            count = count.saturating_add(1);
            continue;
        } else if matches!(tok, Tok::Rpar) {
            count = count.saturating_sub(1);
            continue;
        }
        if count == 0 {
            match tok {
                Tok::Not => {
                    if let Some((_, next_range)) =
                        tok_iter.next_if(|(tok, _)| matches!(tok, Tok::In))
                    {
                        ops.push(LocatedCmpop::new(
                            TextRange::new(range.start(), next_range.end()),
                            Cmpop::NotIn,
                        ));
                    }
                }
                Tok::In => {
                    ops.push(LocatedCmpop::new(range, Cmpop::In));
                }
                Tok::Is => {
                    let op = if let Some((_, next_range)) =
                        tok_iter.next_if(|(tok, _)| matches!(tok, Tok::Not))
                    {
                        LocatedCmpop::new(
                            TextRange::new(range.start(), next_range.end()),
                            Cmpop::IsNot,
                        )
                    } else {
                        LocatedCmpop::new(range, Cmpop::Is)
                    };
                    ops.push(op);
                }
                Tok::NotEqual => {
                    ops.push(LocatedCmpop::new(range, Cmpop::NotEq));
                }
                Tok::EqEqual => {
                    ops.push(LocatedCmpop::new(range, Cmpop::Eq));
                }
                Tok::GreaterEqual => {
                    ops.push(LocatedCmpop::new(range, Cmpop::GtE));
                }
                Tok::Greater => {
                    ops.push(LocatedCmpop::new(range, Cmpop::Gt));
                }
                Tok::LessEqual => {
                    ops.push(LocatedCmpop::new(range, Cmpop::LtE));
                }
                Tok::Less => {
                    ops.push(LocatedCmpop::new(range, Cmpop::Lt));
                }
                _ => {}
            }
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_text_size::{TextLen, TextRange, TextSize};
    use rustpython_ast::{Expr, Stmt};
    use rustpython_parser::ast::Cmpop;
    use rustpython_parser::Parse;

    use crate::helpers::{elif_else_range, else_range, first_colon_range};
    use crate::ranges::{arg_range, identifier_range, locate_cmpops, LocatedCmpop};
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
            arg_range(arg, &locator),
            TextRange::new(TextSize::from(6), TextSize::from(7))
        );

        let contents = "def f(x: int): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let function_def = stmt.as_function_def_stmt().unwrap();
        let args = &function_def.args.args;
        let arg = &args[0];
        let locator = Locator::new(contents);
        assert_eq!(
            arg_range(arg, &locator),
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
            arg_range(arg, &locator),
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
            identifier_range(&stmt, &locator),
            TextRange::new(TextSize::from(4), TextSize::from(5))
        );

        let contents = "async def f(): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(&stmt, &locator),
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
            identifier_range(&stmt, &locator),
            TextRange::new(TextSize::from(8), TextSize::from(9))
        );

        let contents = "class Class(): pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(&stmt, &locator),
            TextRange::new(TextSize::from(6), TextSize::from(11))
        );

        let contents = "class Class: pass".trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(&stmt, &locator),
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
            identifier_range(&stmt, &locator),
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
            identifier_range(&stmt, &locator),
            TextRange::new(TextSize::from(30), TextSize::from(35))
        );

        let contents = r#"x = y + 1"#.trim();
        let stmt = Stmt::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(&stmt, &locator),
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

    #[test]
    fn extract_cmpop_location() -> Result<()> {
        let contents = "x == 1";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(4),
                Cmpop::Eq
            )]
        );

        let contents = "x != 1";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(4),
                Cmpop::NotEq
            )]
        );

        let contents = "x is 1";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(4),
                Cmpop::Is
            )]
        );

        let contents = "x is not 1";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(8),
                Cmpop::IsNot
            )]
        );

        let contents = "x in 1";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(4),
                Cmpop::In
            )]
        );

        let contents = "x not in 1";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(8),
                Cmpop::NotIn
            )]
        );

        let contents = "x != (1 is not 2)";
        let expr = Expr::parse(contents, "<filename>")?;
        let locator = Locator::new(contents);
        assert_eq!(
            locate_cmpops(&expr, &locator),
            vec![LocatedCmpop::new(
                TextSize::from(2)..TextSize::from(4),
                Cmpop::NotEq
            )]
        );

        Ok(())
    }
}
