use itertools::{Itertools, MultiPeek};

use crate::lexer::{LexResult, Tok};
pub use crate::mode::Mode;

/// An [`Iterator`] that transforms a token stream to accommodate soft keywords (namely, `match`
/// and `case`).
///
/// [PEP 634](https://www.python.org/dev/peps/pep-0634/) introduced the `match` and `case` keywords
/// as soft keywords, meaning that they can be used as identifiers (e.g., variable names) in certain
/// contexts.
///
/// This function modifies a token stream to accommodate this change. In particular, it replaces
/// `match` and `case` tokens with `identifier` tokens if they are used as identifiers.
///
/// Handling soft keywords in this intermediary pass allows us to simplify both the lexer and
/// parser, as neither of them need to be aware of soft keywords.
pub struct SoftKeywordTransformer<I>
where
    I: Iterator<Item = LexResult>,
{
    underlying: MultiPeek<I>,
    start_of_line: bool,
}

impl<I> SoftKeywordTransformer<I>
where
    I: Iterator<Item = LexResult>,
{
    pub fn new(lexer: I, mode: Mode) -> Self {
        Self {
            underlying: lexer.multipeek(),
            start_of_line: matches!(mode, Mode::Interactive | Mode::Module),
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
        if let Some(Ok((start, tok, end))) = next.as_ref() {
            // If the token is a `match` or `case` token, check if it's used as an identifier.
            // We assume every `match` or `case` is an identifier unless both of the following
            // conditions are met:
            // 1. The token is at the start of a logical line.
            // 2. The logical line contains a top-level colon (that is, a colon that is not nested
            //    inside a parenthesized expression, list, or dictionary).
            // 3. The top-level colon is not the immediate sibling of a `match` or `case` token.
            //    (This is to avoid treating `match` and `case` as identifiers when annotated with
            //    type hints.)
            if matches!(tok, Tok::Match | Tok::Case) {
                if !self.start_of_line {
                    next = Some(Ok((*start, soft_to_name(tok), *end)));
                } else {
                    let mut nesting = 0;
                    let mut first = true;
                    let mut seen_colon = false;
                    while let Some(Ok((_, tok, _))) = self.underlying.peek() {
                        match tok {
                            Tok::Newline => break,
                            Tok::Colon if nesting == 0 => {
                                if !first {
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
                        next = Some(Ok((*start, soft_to_name(tok), *end)));
                    }
                }
            }
        }

        self.start_of_line = next.as_ref().map_or(false, |lex_result| {
            lex_result.as_ref().map_or(false, |(_, tok, _)| {
                if matches!(tok, Tok::NonLogicalNewline | Tok::Comment { .. }) {
                    self.start_of_line
                } else {
                    matches!(
                        tok,
                        Tok::StartModule
                            | Tok::StartInteractive
                            | Tok::Newline
                            | Tok::Indent
                            | Tok::Dedent
                    )
                }
            })
        });

        next
    }
}

#[inline]
fn soft_to_name(tok: &Tok) -> Tok {
    let name = match tok {
        Tok::Match => "match",
        Tok::Case => "case",
        _ => unreachable!("other tokens never reach here"),
    };
    Tok::Name {
        name: name.to_owned(),
    }
}
