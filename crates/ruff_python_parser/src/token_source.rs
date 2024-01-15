use crate::lexer::{LexResult, LexicalError, Spanned};
use crate::{Tok, TokenKind};
use ruff_text_size::TextRange;
use std::iter::FusedIterator;

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

    pub(crate) fn peek_nth(&self, mut n: usize) -> Option<(TokenKind, TextRange)> {
        let mut iter = self.tokens.as_slice().iter();

        loop {
            let next = iter.next()?;

            if next.as_ref().is_ok_and(is_trivia) {
                continue;
            }

            if n == 0 {
                break Some(match next {
                    Ok((token, range)) => (TokenKind::from_token(token), *range),
                    Err(LexicalError { location, .. }) => (TokenKind::Unknown, *location),
                });
            }

            n -= 1;
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
                    let location = error.location;
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
