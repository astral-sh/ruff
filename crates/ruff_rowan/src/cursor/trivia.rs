use crate::cursor::SyntaxToken;
use crate::green::GreenTrivia;
use crate::TriviaPiece;
use ruff_text_size::{TextRange, TextSize};
use std::fmt;
use std::iter::FusedIterator;

#[derive(PartialEq, Eq, Clone, Hash)]
pub(crate) struct SyntaxTrivia {
    token: SyntaxToken,
    is_leading: bool,
}

impl SyntaxTrivia {
    pub(super) fn leading(token: SyntaxToken) -> Self {
        Self {
            token,
            is_leading: true,
        }
    }

    pub(super) fn trailing(token: SyntaxToken) -> Self {
        Self {
            token,
            is_leading: false,
        }
    }

    pub(crate) fn text(&self) -> &str {
        let trivia_range = self.text_range();

        let relative_range = TextRange::at(
            trivia_range.start() - self.token.data().offset,
            trivia_range.len(),
        );

        &self.token.text()[relative_range]
    }

    pub(crate) fn token(&self) -> &SyntaxToken {
        &self.token
    }

    pub(crate) fn text_range(&self) -> TextRange {
        let length = self.green_trivia().text_len();
        let token_range = self.token.text_range();

        match self.is_leading {
            true => TextRange::at(token_range.start(), length),
            false => TextRange::at(token_range.end() - length, length),
        }
    }

    /// Get the number of TriviaPiece inside this trivia
    pub(crate) fn len(&self) -> usize {
        self.green_trivia().len()
    }

    /// Gets index-th trivia piece when the token associated with this trivia was created.
    /// See [SyntaxTriviaPiece].
    pub(crate) fn get_piece(&self, index: usize) -> Option<&TriviaPiece> {
        self.green_trivia().get_piece(index)
    }

    fn green_trivia(&self) -> &GreenTrivia {
        match self.is_leading {
            true => self.token.green().leading_trivia(),
            false => self.token.green().trailing_trivia(),
        }
    }

    /// Returns the last trivia piece element
    pub(crate) fn last(&self) -> Option<&TriviaPiece> {
        self.green_trivia().pieces().last()
    }

    /// Returns the first trivia piece element
    pub(crate) fn first(&self) -> Option<&TriviaPiece> {
        self.green_trivia().pieces().first()
    }

    /// Iterate over all pieces of the trivia. The iterator returns the offset
    /// of the trivia as [TextSize] and its data as [Trivia], which contains its length.
    /// See [SyntaxTriviaPiece].
    pub(crate) fn pieces(&self) -> SyntaxTriviaPiecesIterator {
        let range = self.text_range();
        SyntaxTriviaPiecesIterator {
            raw: self.clone(),
            next_index: 0,
            next_offset: range.start(),
            end_index: self.len(),
            end_offset: range.end(),
        }
    }
}

impl fmt::Debug for SyntaxTrivia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("SyntaxTrivia");
        f.field("text_range", &self.text_range());
        f.finish()
    }
}

impl fmt::Display for SyntaxTrivia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.text(), f)
    }
}

#[derive(Clone)]
pub struct SyntaxTriviaPiecesIterator {
    pub(crate) raw: SyntaxTrivia,
    pub(crate) next_index: usize,
    pub(crate) next_offset: TextSize,
    pub(crate) end_index: usize,
    pub(crate) end_offset: TextSize,
}

impl Iterator for SyntaxTriviaPiecesIterator {
    type Item = (TextSize, TriviaPiece);

    fn next(&mut self) -> Option<Self::Item> {
        let trivia = self.raw.get_piece(self.next_index)?;
        let piece = (self.next_offset, *trivia);

        self.next_index += 1;
        self.next_offset += trivia.text_len();

        Some(piece)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end_index.saturating_sub(self.next_index);
        (len, Some(len))
    }
}

impl FusedIterator for SyntaxTriviaPiecesIterator {}

impl DoubleEndedIterator for SyntaxTriviaPiecesIterator {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.end_index == self.next_index {
            return None;
        }

        self.end_index -= 1;

        let trivia = self.raw.get_piece(self.end_index)?;
        self.end_offset -= trivia.text_len();

        Some((self.end_offset, *trivia))
    }
}

impl ExactSizeIterator for SyntaxTriviaPiecesIterator {}

#[cfg(test)]
mod tests {
    use crate::raw_language::{RawLanguage, RawLanguageKind, RawSyntaxTreeBuilder};
    use crate::{SyntaxNode, TriviaPiece, TriviaPieceKind};

    #[test]
    fn trivia_text() {
        let mut builder = RawSyntaxTreeBuilder::new();
        builder.start_node(RawLanguageKind::ROOT);
        builder.token_with_trivia(
            RawLanguageKind::WHITESPACE,
            "\t let \t\t",
            &[TriviaPiece::new(TriviaPieceKind::Whitespace, 2)],
            &[TriviaPiece::new(TriviaPieceKind::Whitespace, 3)],
        );
        builder.finish_node();

        let root = builder.finish_green();
        let syntax: SyntaxNode<RawLanguage> = SyntaxNode::new_root(root);

        let token = syntax.first_token().unwrap();
        assert_eq!(token.leading_trivia().text(), "\t ");
        assert_eq!(token.trailing_trivia().text(), " \t\t");
    }
}
