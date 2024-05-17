use ruff_text_size::{TextRange, TextSize};

use crate::lexer::{Lexer, LexerCheckpoint, LexicalError, Token, TokenValue};
use crate::{Mode, TokenKind};

/// Token source for the parser that skips over any trivia tokens.
#[derive(Debug)]
pub(crate) struct TokenSource<'src> {
    /// The underlying source for the tokens.
    lexer: Lexer<'src>,

    /// A vector containing all the tokens emitted by the lexer. This is returned when the parser
    /// is finished consuming all the tokens. Note that unlike the emitted tokens, this vector
    /// holds both the trivia and non-trivia tokens.
    tokens: Vec<Token>,
}

impl<'src> TokenSource<'src> {
    /// Create a new token source for the given lexer.
    pub(crate) fn new(lexer: Lexer<'src>) -> Self {
        TokenSource {
            lexer,
            tokens: vec![],
        }
    }

    /// Create a new token source from the given source code which starts at the given offset.
    pub(crate) fn from_source(source: &'src str, mode: Mode, start_offset: TextSize) -> Self {
        let lexer = Lexer::new(source, mode, start_offset);
        let mut source = TokenSource::new(lexer);

        // Initialize the token source so that the current token is set correctly.
        source.next_token();
        source
    }

    /// Returns the kind of the current token.
    pub(crate) fn current_kind(&self) -> TokenKind {
        self.lexer.current_kind()
    }

    /// Returns the range of the current token.
    pub(crate) fn current_range(&self) -> TextRange {
        self.lexer.current_range()
    }

    /// Calls the underlying [`Lexer::take_value`] method on the lexer. Refer to its documentation
    /// for more info.
    pub(crate) fn take_value(&mut self) -> TokenValue {
        self.lexer.take_value()
    }

    /// Returns the next non-trivia token without consuming it.
    pub(crate) fn peek(&mut self) -> TokenKind {
        let checkpoint = self.lexer.checkpoint();
        let next = loop {
            let next = self.lexer.next_token();
            if next.is_trivia() {
                continue;
            }
            break next.kind();
        };
        self.lexer.rewind(checkpoint);
        next
    }

    /// Moves the lexer to the next non-trivia token.
    pub(crate) fn next_token(&mut self) {
        loop {
            let next = self.lexer.next_token();
            self.tokens.push(next);
            if next.is_trivia() {
                continue;
            }
            break;
        }
    }

    /// Creates a checkpoint to which the token source can later return to using [`Self::rewind`].
    pub(crate) fn checkpoint(&self) -> TokenSourceCheckpoint<'src> {
        TokenSourceCheckpoint {
            lexer: self.lexer.checkpoint(),
            tokens_position: self.tokens.len(),
        }
    }

    /// Restore the token source to the given checkpoint.
    pub(crate) fn rewind(&mut self, checkpoint: TokenSourceCheckpoint<'src>) {
        self.lexer.rewind(checkpoint.lexer);
        self.tokens.truncate(checkpoint.tokens_position);
    }

    /// Consumes the token source, returning the collected tokens and any errors encountered during
    /// lexing. The token collection includes both the trivia and non-trivia tokens.
    pub(crate) fn finish(self) -> (Vec<Token>, Vec<LexicalError>) {
        assert_eq!(
            self.current_kind(),
            TokenKind::EndOfFile,
            "TokenSource was not fully consumed"
        );

        (self.tokens, self.lexer.finish())
    }
}

pub(crate) struct TokenSourceCheckpoint<'src> {
    lexer: LexerCheckpoint<'src>,
    tokens_position: usize,
}
