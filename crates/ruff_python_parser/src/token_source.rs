use crate::lexer::LexResult;
use crate::Tok;
use std::iter::FusedIterator;

#[derive(Clone, Debug)]
pub(crate) struct TokenSource {
    tokens: std::vec::IntoIter<LexResult>,
}

impl TokenSource {
    pub(crate) fn new(tokens: Vec<LexResult>) -> Self {
        Self {
            tokens: tokens.into_iter(),
        }
    }
}

impl FromIterator<LexResult> for TokenSource {
    #[inline]
    fn from_iter<T: IntoIterator<Item = LexResult>>(iter: T) -> Self {
        Self::new(Vec::from_iter(iter))
    }
}

impl Iterator for TokenSource {
    type Item = LexResult;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = self.tokens.next()?;

            if is_trivia(&next) {
                continue;
            }

            break Some(next);
        }
    }
}

impl FusedIterator for TokenSource {}

const fn is_trivia(result: &LexResult) -> bool {
    matches!(result, Ok((Tok::Comment(_) | Tok::NonLogicalNewline, _)))
}
