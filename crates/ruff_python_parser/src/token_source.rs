use ruff_text_size::TextRange;

use crate::lexer::{Lexer, LexicalError, Token, TokenValue};
use crate::{Mode, TokenKind};

#[derive(Debug)]
pub(crate) struct TokenSource<'src> {
    lexer: Lexer<'src>,
    tokens: Vec<Token>,
}

impl<'src> TokenSource<'src> {
    pub(crate) fn new(lexer: Lexer<'src>) -> Self {
        Self {
            lexer,
            tokens: vec![],
        }
    }

    pub(crate) fn from_source(source: &'src str, mode: Mode) -> Self {
        Self::new(Lexer::new(source, mode))
    }

    /// Returns the kind of the current token.
    pub(crate) fn current_kind(&self) -> TokenKind {
        self.lexer.current_kind()
    }

    /// Returns the range of the current token.
    pub(crate) fn current_range(&self) -> TextRange {
        self.lexer.current_range()
    }

    pub(crate) fn take_value(&mut self) -> TokenValue {
        self.lexer.take_value()
    }

    /// Returns the next token kind and its range without consuming it.
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

    pub(crate) fn finish(mut self) -> (Vec<Token>, Vec<LexicalError>) {
        assert_eq!(
            self.peek(),
            TokenKind::EndOfFile,
            "TokenSource was not fully consumed"
        );

        (self.tokens, self.lexer.finish())
    }
}
