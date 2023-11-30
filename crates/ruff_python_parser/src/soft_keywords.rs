use itertools::{Itertools, MultiPeek};

use crate::{lexer::LexResult, token::Tok, Mode};

/// An [`Iterator`] that transforms a token stream to accommodate soft keywords (namely, `match`
/// `case`, and `type`).
///
/// [PEP 634](https://www.python.org/dev/peps/pep-0634/) introduced the `match` and `case` keywords
/// as soft keywords, meaning that they can be used as identifiers (e.g., variable names) in certain
/// contexts.
///
/// Later, [PEP 695](https://peps.python.org/pep-0695/#generic-type-alias) introduced the `type`
/// soft keyword.
///
/// This function modifies a token stream to accommodate this change. In particular, it replaces
/// soft keyword tokens with `identifier` tokens if they are used as identifiers.
///
/// Handling soft keywords in this intermediary pass allows us to simplify both the lexer and
/// `ruff_python_parser`, as neither of them need to be aware of soft keywords.
pub struct SoftKeywordTransformer<I>
where
    I: Iterator<Item = LexResult>,
{
    underlying: MultiPeek<I>,
    position: Position,
}

impl<I> SoftKeywordTransformer<I>
where
    I: Iterator<Item = LexResult>,
{
    pub fn new(lexer: I, mode: Mode) -> Self {
        Self {
            underlying: lexer.multipeek(), // spell-checker:ignore multipeek
            position: if mode == Mode::Expression {
                Position::Other
            } else {
                Position::Statement
            },
        }
    }
}

impl<I> Iterator for SoftKeywordTransformer<I>
where
    I: Iterator<Item = LexResult>,
{
    type Item = LexResult;

    #[inline]
    fn next(&mut self) -> Option<LexResult> {
        let mut next = self.underlying.next();
        if let Some(Ok((tok, range))) = next.as_ref() {
            // If the token is a soft keyword e.g. `type`, `match`, or `case`, check if it's
            // used as an identifier. We assume every soft keyword use is an identifier unless
            // a heuristic is met.
            match tok {
                // For `match` and `case`, all of the following conditions must be met:
                // 1. The token is at the start of a logical line.
                // 2. The logical line contains a top-level colon (that is, a colon that is not nested
                //    inside a parenthesized expression, list, or dictionary).
                // 3. The top-level colon is not the immediate sibling of a `match` or `case` token.
                //    (This is to avoid treating `match` or `case` as identifiers when annotated with
                //    type hints.)
                Tok::Match | Tok::Case => {
                    if matches!(self.position, Position::Statement) {
                        let mut nesting = 0;
                        let mut first = true;
                        let mut seen_colon = false;
                        let mut seen_lambda = false;
                        while let Some(Ok((tok, _))) = self.underlying.peek() {
                            match tok {
                                Tok::Newline => break,
                                Tok::Lambda if nesting == 0 => seen_lambda = true,
                                Tok::Colon if nesting == 0 => {
                                    if seen_lambda {
                                        seen_lambda = false;
                                    } else if !first {
                                        seen_colon = true;
                                    }
                                }
                                Tok::Lpar | Tok::Lsqb | Tok::Lbrace => nesting += 1,
                                Tok::Rpar | Tok::Rsqb | Tok::Rbrace => nesting -= 1,
                                _ => {}
                            }
                            first = false;
                        }
                        if !seen_colon {
                            next = Some(Ok((soft_to_name(tok), *range)));
                        }
                    } else {
                        next = Some(Ok((soft_to_name(tok), *range)));
                    }
                }
                // For `type` all of the following conditions must be met:
                // 1. The token is at the start of a logical line.
                // 2. The type token is immediately followed by a name token.
                // 3. The name token is eventually followed by an equality token.
                Tok::Type => {
                    if matches!(
                        self.position,
                        Position::Statement | Position::SimpleStatement
                    ) {
                        let mut is_type_alias = false;
                        if let Some(Ok((tok, _))) = self.underlying.peek() {
                            if matches!(
                                tok,
                                Tok::Name { .. } |
                                // We treat a soft keyword token following a type token as a
                                // name to support cases like `type type = int` or `type match = int`
                                Tok::Type | Tok::Match | Tok::Case
                            ) {
                                let mut nesting = 0;
                                while let Some(Ok((tok, _))) = self.underlying.peek() {
                                    match tok {
                                        Tok::Newline => break,
                                        Tok::Equal if nesting == 0 => {
                                            is_type_alias = true;
                                            break;
                                        }
                                        Tok::Lsqb => nesting += 1,
                                        Tok::Rsqb => nesting -= 1,
                                        // Allow arbitrary content within brackets for now
                                        _ if nesting > 0 => {}
                                        // Exit if unexpected tokens are seen
                                        _ => break,
                                    }
                                }
                            }
                        }
                        if !is_type_alias {
                            next = Some(Ok((soft_to_name(tok), *range)));
                        }
                    } else {
                        next = Some(Ok((soft_to_name(tok), *range)));
                    }
                }
                _ => (), // Not a soft keyword token
            }
        }

        // Update the position, to track whether we're at the start of a logical line.
        if let Some(lex_result) = next.as_ref() {
            if let Ok((tok, _)) = lex_result.as_ref() {
                match tok {
                    Tok::NonLogicalNewline | Tok::Comment { .. } => {
                        // Nothing to do.
                    }
                    Tok::StartModule | Tok::Newline | Tok::Indent | Tok::Dedent => {
                        self.position = Position::Statement;
                    }
                    // If we see a semicolon, assume we're at the start of a simple statement, as in:
                    // ```python
                    // type X = int; type Y = float
                    // ```
                    Tok::Semi => {
                        self.position = Position::SimpleStatement;
                    }
                    // If we see a colon, and we're not in a nested context, assume we're at the
                    // start of a simple statement, as in:
                    // ```python
                    // class Class: type X = int
                    // ```
                    Tok::Colon if self.position == Position::Other => {
                        self.position = Position::SimpleStatement;
                    }
                    Tok::Lpar | Tok::Lsqb | Tok::Lbrace => {
                        self.position = if let Position::Nested(depth) = self.position {
                            Position::Nested(depth.saturating_add(1))
                        } else {
                            Position::Nested(1)
                        };
                    }
                    Tok::Rpar | Tok::Rsqb | Tok::Rbrace => {
                        self.position = if let Position::Nested(depth) = self.position {
                            let depth = depth.saturating_sub(1);
                            if depth > 0 {
                                Position::Nested(depth)
                            } else {
                                Position::Other
                            }
                        } else {
                            Position::Other
                        };
                    }
                    _ => {
                        self.position = Position::Other;
                    }
                }
            }
        }

        next
    }
}

#[inline]
fn soft_to_name(tok: &Tok) -> Tok {
    let name = match tok {
        Tok::Match => "match",
        Tok::Case => "case",
        Tok::Type => "type",
        _ => unreachable!("other tokens never reach here"),
    };
    Tok::Name {
        name: name.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    /// The lexer is at the start of a logical line, i.e., the start of a simple or compound statement.
    Statement,
    /// The lexer is at the start of a simple statement, e.g., a statement following a semicolon
    /// or colon, as in:
    /// ```python
    /// class Class: type X = int
    /// ```
    SimpleStatement,
    /// The lexer is within brackets, with the given bracket nesting depth.
    Nested(u32),
    /// The lexer is some other location.
    Other,
}
