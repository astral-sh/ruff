use crate::parser::Parser;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TokenId(u32);

impl TokenId {
    /// Increments the value of the token ID.
    pub(super) fn increment(&mut self) {
        // It's fine to just wrap around because the main purpose is to check whether
        // the previous token ID is different from the current token ID.
        self.0 = self.0.wrapping_add(1);
    }
}

/// Captures the progress of the parser and allows to test if the parsing is still making progress
#[derive(Debug, Copy, Clone, Default)]
pub(super) struct ParserProgress(Option<TokenId>);

impl ParserProgress {
    /// Returns true if the parser has passed this position
    #[inline]
    fn has_progressed(self, p: &Parser) -> bool {
        match self.0 {
            None => true,
            Some(prev_token_id) => prev_token_id != p.current_token_id(),
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
            p.src_text(p.current_token_range()),
            p.current_token_kind(),
            p.current_token_range(),
        );

        self.0 = Some(p.current_token_id());
    }
}
