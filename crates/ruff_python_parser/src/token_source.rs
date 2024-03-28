use std::iter::FusedIterator;

use ruff_text_size::{TextRange, TextSize};

use crate::lexer::{LexResult, LexicalError, Spanned};
use crate::{Tok, TokenKind};

#[derive(Clone, Debug)]
pub(crate) struct TokenSource {
    tokens: std::vec::IntoIter<LexResult>,
    errors: Vec<LexicalError>,
}

impl TokenSource {
    pub(crate) fn new(tokens: Vec<LexResult>) -> Self {
        Self {
            tokens: tokens.into_iter(),
            errors: Vec::new(),
        }
    }

    /// Returns the position of the current token.
    ///
    /// This is the position before any whitespace or comments.
    pub(crate) fn position(&self) -> Option<TextSize> {
        let first = self.tokens.as_slice().first()?;

        let range = match first {
            Ok((_, range)) => *range,
            Err(error) => error.location(),
        };

        Some(range.start())
    }

    /// Returns the end of the last token
    pub(crate) fn end(&self) -> Option<TextSize> {
        let last = self.tokens.as_slice().last()?;

        let range = match last {
            Ok((_, range)) => *range,
            Err(error) => error.location(),
        };

        Some(range.end())
    }

    /// Returns the next token kind and its range without consuming it.
    pub(crate) fn peek(&self) -> Option<(TokenKind, TextRange)> {
        let mut iter = self.tokens.as_slice().iter();

        loop {
            let next = iter.next()?;

            if next.as_ref().is_ok_and(is_trivia) {
                continue;
            }

            break Some(match next {
                Ok((token, range)) => (TokenKind::from_token(token), *range),
                Err(error) => (TokenKind::Unknown, error.location()),
            });
        }
    }

    pub(crate) fn finish(self) -> Vec<LexicalError> {
        assert_eq!(
            self.tokens.as_slice(),
            &[],
            "TokenSource was not fully consumed."
        );

        self.errors
    }
}

impl FromIterator<LexResult> for TokenSource {
    #[inline]
    fn from_iter<T: IntoIterator<Item = LexResult>>(iter: T) -> Self {
        Self::new(Vec::from_iter(iter))
    }
}

impl Iterator for TokenSource {
    type Item = Spanned;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.tokens.next()?;

            match next {
                Ok(token) => {
                    if is_trivia(&token) {
                        continue;
                    }

                    break Some(token);
                }

                Err(error) => {
                    let location = error.location();
                    self.errors.push(error);
                    break Some((Tok::Unknown, location));
                }
            }
        }
    }
}

impl FusedIterator for TokenSource {}

const fn is_trivia(result: &Spanned) -> bool {
    matches!(result, (Tok::Comment(_) | Tok::NonLogicalNewline, _))
}
