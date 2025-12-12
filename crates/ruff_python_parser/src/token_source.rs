use ruff_python_ast::token::{Token, TokenFlags, TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Mode;
use crate::error::LexicalError;
use crate::lexer::{Lexer, LexerCheckpoint};
use crate::string::InterpolatedStringKind;
use crate::token::TokenValue;

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
        // TODO(dhruvmanila): Use `allocate_tokens_vec`
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
        source.do_bump();
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

    /// Returns the flags for the current token.
    pub(crate) fn current_flags(&self) -> TokenFlags {
        self.lexer.current_flags()
    }

    /// Calls the underlying [`take_value`] method on the lexer. Refer to its documentation
    /// for more info.
    ///
    /// [`take_value`]: Lexer::take_value
    pub(crate) fn take_value(&mut self) -> TokenValue {
        self.lexer.take_value()
    }

    /// Calls the underlying [`re_lex_logical_token`] method on the lexer with the new lexer
    /// position and updates the token vector accordingly.
    ///
    /// [`re_lex_logical_token`]: Lexer::re_lex_logical_token
    pub(crate) fn re_lex_logical_token(&mut self) {
        let mut non_logical_newline = None;

        #[cfg(debug_assertions)]
        let last_non_trivia_end_before = {
            self.tokens
                .iter()
                .rev()
                .find(|tok| !tok.kind().is_trivia())
                .map(ruff_text_size::Ranged::end)
        };

        for (index, token) in self.tokens.iter().enumerate().rev() {
            match token.kind() {
                TokenKind::NonLogicalNewline => {
                    non_logical_newline = Some((index, token.start()));
                }
                TokenKind::Comment => continue,
                _ => break,
            }
        }

        if !self
            .lexer
            .re_lex_logical_token(non_logical_newline.map(|(_, start)| start))
        {
            return;
        }

        let non_logical_line_index = non_logical_newline
            .expect(
                "`re_lex_logical_token` should only return `true` if `non_logical_line` is `Some`",
            )
            .0;

        // Trim the already bumped logical line token (and comments coming after it) as it might now have become a logical line token
        self.tokens.truncate(non_logical_line_index);

        #[cfg(debug_assertions)]
        {
            let last_non_trivia_end_now = {
                self.tokens
                    .iter()
                    .rev()
                    .find(|tok| !tok.kind().is_trivia())
                    .map(ruff_text_size::Ranged::end)
            };

            assert_eq!(last_non_trivia_end_before, last_non_trivia_end_now);
        }

        // Ensure `current` is positioned at a non-trivia token.
        if self.current_kind().is_trivia() {
            self.bump(self.current_kind());
        }
    }

    pub(crate) fn re_lex_string_token_in_interpolation_element(
        &mut self,
        kind: InterpolatedStringKind,
    ) {
        self.lexer
            .re_lex_string_token_in_interpolation_element(kind);
    }

    pub(crate) fn re_lex_raw_string_in_format_spec(&mut self) {
        self.lexer.re_lex_raw_string_in_format_spec();
    }

    /// Returns the next non-trivia token without consuming it.
    ///
    /// Use [`peek2`] to get the next two tokens.
    ///
    /// [`peek2`]: TokenSource::peek2
    pub(crate) fn peek(&mut self) -> TokenKind {
        let checkpoint = self.lexer.checkpoint();
        let next = self.next_non_trivia_token();
        self.lexer.rewind(checkpoint);
        next
    }

    /// Returns the next two non-trivia tokens without consuming it.
    ///
    /// Use [`peek`] to only get the next token.
    ///
    /// [`peek`]: TokenSource::peek
    pub(crate) fn peek2(&mut self) -> (TokenKind, TokenKind) {
        let checkpoint = self.lexer.checkpoint();
        let first = self.next_non_trivia_token();
        let second = self.next_non_trivia_token();
        self.lexer.rewind(checkpoint);
        (first, second)
    }

    /// Bumps the token source to the next non-trivia token.
    ///
    /// It pushes the given kind to the token vector with the current token range.
    pub(crate) fn bump(&mut self, kind: TokenKind) {
        self.tokens
            .push(Token::new(kind, self.current_range(), self.current_flags()));
        self.do_bump();
    }

    /// Bumps the token source to the next non-trivia token without adding the current token to the
    /// token vector. It does add the trivia tokens to the token vector.
    fn do_bump(&mut self) {
        loop {
            let kind = self.lexer.next_token();
            if kind.is_trivia() {
                self.tokens
                    .push(Token::new(kind, self.current_range(), self.current_flags()));
                continue;
            }
            break;
        }
    }

    /// Returns the next non-trivia token without adding it to the token vector.
    fn next_non_trivia_token(&mut self) -> TokenKind {
        loop {
            let kind = self.lexer.next_token();
            if kind.is_trivia() {
                continue;
            }
            break kind;
        }
    }

    /// Creates a checkpoint to which the token source can later return to using [`Self::rewind`].
    pub(crate) fn checkpoint(&self) -> TokenSourceCheckpoint {
        TokenSourceCheckpoint {
            lexer_checkpoint: self.lexer.checkpoint(),
            tokens_position: self.tokens.len(),
        }
    }

    /// Restore the token source to the given checkpoint.
    pub(crate) fn rewind(&mut self, checkpoint: TokenSourceCheckpoint) {
        let TokenSourceCheckpoint {
            lexer_checkpoint,
            tokens_position,
        } = checkpoint;

        self.lexer.rewind(lexer_checkpoint);
        self.tokens.truncate(tokens_position);
    }

    /// Returns a slice of [`Token`] that are within the given `range`.
    pub(crate) fn in_range(&self, range: TextRange) -> &[Token] {
        let start = self
            .tokens
            .iter()
            .rposition(|tok| tok.start() == range.start());
        let end = self.tokens.iter().rposition(|tok| tok.end() == range.end());

        let (Some(start), Some(end)) = (start, end) else {
            return &self.tokens;
        };

        &self.tokens[start..=end]
    }

    /// Consumes the token source, returning the collected tokens, comment ranges, and any errors
    /// encountered during lexing. The token collection includes both the trivia and non-trivia
    /// tokens.
    pub(crate) fn finish(mut self) -> (Vec<Token>, Vec<LexicalError>) {
        assert_eq!(
            self.current_kind(),
            TokenKind::EndOfFile,
            "TokenSource was not fully consumed"
        );

        // The `EndOfFile` token shouldn't be included in the token stream, it's mainly to signal
        // the parser to stop. This isn't in `do_bump` because it only needs to be done once.
        if let Some(last) = self.tokens.pop() {
            assert_eq!(last.kind(), TokenKind::EndOfFile);
        }

        (self.tokens, self.lexer.finish())
    }
}

pub(crate) struct TokenSourceCheckpoint {
    lexer_checkpoint: LexerCheckpoint,
    tokens_position: usize,
}

/// Allocates a [`Vec`] with an approximated capacity to fit all tokens
/// of `contents`.
///
/// See [#9546](https://github.com/astral-sh/ruff/pull/9546) for a more detailed explanation.
#[expect(dead_code)]
fn allocate_tokens_vec(contents: &str) -> Vec<Token> {
    let lower_bound = contents.len().saturating_mul(15) / 100;
    Vec::with_capacity(lower_bound)
}
