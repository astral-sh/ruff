use crate::parser::Parser;
use crate::TokenKind;
use ruff_text_size::TextSize;

/// Captures the progress of the parser and allows to test if the parsing is still making progress
#[derive(Debug, Copy, Clone, Default)]
pub(super) struct ParserProgress(Option<(TokenKind, TextSize)>);

impl ParserProgress {
    /// Returns true if the parser has passed this position
    #[inline]
    fn has_progressed(self, p: &Parser) -> bool {
        match self.0 {
            None => true,
            Some(snapshot) => snapshot != (p.current_kind(), p.current_range().start()),
        }
    }

    /// Asserts that the parsing is still making progress.
    ///
    /// # Panics
    ///
    /// Panics if the parser hasn't progressed since the last call.
    #[inline]
    pub(super) fn assert_progressing(&mut self, p: &Parser) {
        assert!(
            self.has_progressed(p),
            "The parser is no longer progressing. Stuck at '{}' {:?}:{:?}",
            p.src_text(p.current_range()),
            p.current_kind(),
            p.current_range(),
        );

        self.0 = Some((p.current_kind(), p.current_range().start()));
    }
}
