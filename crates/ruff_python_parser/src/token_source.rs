use ruff_python_ast::token::{Token, TokenFlags, TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Mode;
use crate::error::LexicalError;
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
use crate::lexer::chunked::ChunkedLexer;
use crate::lexer::{Lexer, LexerCheckpoint};
use crate::string::InterpolatedStringKind;

/// Token source for the parser that skips over any trivia tokens.
#[derive(Debug)]
pub(crate) struct TokenSource<'src> {
    /// The underlying source for the tokens.
    lexer: LexerSource<'src>,

    /// The current non-trivia token. Keeping this separate from the buffered stream avoids an
    /// enum match and an indexed load for every parser predicate.
    #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
    current: Token,

    /// Whether error recovery requested a logical-token re-lex that requires the streaming lexer.
    #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
    needs_legacy_reparse: bool,

    /// A vector containing all the tokens emitted by the lexer. This is returned when the parser
    /// is finished consuming all the tokens. Note that unlike the emitted tokens, this vector
    /// holds both the trivia and non-trivia tokens.
    tokens: Vec<Token>,
}

#[derive(Debug)]
enum LexerSource<'src> {
    Streaming(Lexer<'src>),
    #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
    Chunked {
        lexer: ChunkedLexer<'src>,
        position: usize,
        nesting: u32,
    },
}

impl<'src> TokenSource<'src> {
    /// Creates a token source, using chunked lexing for complete modules when the first batch is
    /// supported and the streaming lexer otherwise.
    pub(crate) fn from_source(source: &'src str, mode: Mode, start_offset: TextSize) -> Self {
        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        if mode == Mode::Module
            && start_offset == TextSize::new(0)
            && let Some(mut lexer) = ChunkedLexer::new(source)
            && lexer.fill().is_some()
        {
            let mut source = TokenSource {
                lexer: LexerSource::Chunked {
                    lexer,
                    position: usize::MAX,
                    nesting: 0,
                },
                current: Token::new(
                    TokenKind::EndOfFile,
                    TextRange::empty(start_offset),
                    TokenFlags::empty(),
                ),
                needs_legacy_reparse: false,
                tokens: Vec::new(),
            };
            source.do_bump();
            return source;
        }

        Self::from_streaming_source(source, mode, start_offset)
    }

    /// Creates a token source that always uses the streaming lexer, including when reparsing a
    /// module after a late chunked-lexer failure.
    pub(crate) fn from_streaming_source(
        source: &'src str,
        mode: Mode,
        start_offset: TextSize,
    ) -> Self {
        let mut source = TokenSource {
            lexer: LexerSource::Streaming(Lexer::new(source, mode, start_offset)),
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            current: Token::new(
                TokenKind::EndOfFile,
                TextRange::empty(start_offset),
                TokenFlags::empty(),
            ),
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            needs_legacy_reparse: false,
            tokens: allocate_tokens_vec(&source[start_offset.to_usize()..]),
        };

        // Initialize the token source so that the current token is set correctly.
        source.do_bump();
        source
    }

    /// Returns the kind of the current token.
    #[inline]
    pub(crate) fn current_kind(&self) -> TokenKind {
        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        return self.current.kind();

        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_kind(),
        }
    }

    /// Returns the range of the current token.
    #[inline]
    pub(crate) fn current_range(&self) -> TextRange {
        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        return self.current.range();

        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_range(),
        }
    }

    /// Returns the current parenthesis, bracket, and brace nesting level.
    #[inline]
    pub(crate) fn nesting(&self) -> u32 {
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.nesting(),
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked { nesting, .. } => *nesting,
        }
    }

    /// Returns the flags for the current token.
    #[inline]
    pub(crate) fn current_flags(&self) -> TokenFlags {
        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        return self.current.flags();

        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_flags(),
        }
    }

    /// Calls the underlying [`re_lex_logical_token`] method on the lexer with the new lexer
    /// position and updates the token vector accordingly.
    ///
    /// [`re_lex_logical_token`]: Lexer::re_lex_logical_token
    #[cfg_attr(
        not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")),
        expect(irrefutable_let_patterns, reason = "the chunked variant is SIMD-only")
    )]
    pub(crate) fn re_lex_logical_token(&mut self) {
        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        if matches!(self.lexer, LexerSource::Chunked { .. }) {
            self.needs_legacy_reparse = true;
            return;
        }

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

        let LexerSource::Streaming(lexer) = &mut self.lexer else {
            return;
        };
        if !lexer.re_lex_logical_token(non_logical_newline.map(|(_, start)| start)) {
            return;
        }

        let non_logical_line_index = non_logical_newline
            .expect(
                "`re_lex_logical_token` should only return `true` if `non_logical_line` is `Some`",
            )
            .0;

        // Trim the already bumped logical line token (and comments coming after it) as it might now have become a logical line token
        self.tokens.truncate(non_logical_line_index);

        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        self.refresh_current();

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

    /// Re-lexes a string token in an interpolation element for the streaming lexer. Chunked
    /// f/t-strings are emitted as complete token sequences and need no adjustment.
    pub(crate) fn re_lex_string_token_in_interpolation_element(
        &mut self,
        kind: InterpolatedStringKind,
    ) {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => {
                lexer.re_lex_string_token_in_interpolation_element(kind);
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked { .. } => return,
        }

        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        self.refresh_current();
    }

    /// Re-lexes a raw string in a format spec for the streaming lexer; this is already resolved
    /// when the chunked lexer appends an interpolated string.
    pub(crate) fn re_lex_raw_string_in_format_spec(&mut self) {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => lexer.re_lex_raw_string_in_format_spec(),
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked { .. } => return,
        }

        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        self.refresh_current();
    }

    /// Returns the next non-trivia token without consuming it.
    ///
    /// Use [`peek2`] to get the next two tokens.
    ///
    /// [`peek2`]: TokenSource::peek2
    #[inline]
    pub(crate) fn peek(&mut self) -> TokenKind {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => {
                let checkpoint = lexer.checkpoint();
                let next = next_non_trivia_token(lexer);
                lexer.rewind(checkpoint);
                next
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked {
                lexer, position, ..
            } => next_buffered_token(lexer, *position + 1, &mut self.needs_legacy_reparse).0,
        }
    }

    /// Returns the next two non-trivia tokens without consuming it.
    ///
    /// Use [`peek`] to only get the next token.
    ///
    /// [`peek`]: TokenSource::peek
    #[inline]
    pub(crate) fn peek2(&mut self) -> (TokenKind, TokenKind) {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => {
                let checkpoint = lexer.checkpoint();
                let first = next_non_trivia_token(lexer);
                let second = next_non_trivia_token(lexer);
                lexer.rewind(checkpoint);
                (first, second)
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked {
                lexer, position, ..
            } => {
                let (first, position) =
                    next_buffered_token(lexer, *position + 1, &mut self.needs_legacy_reparse);
                let (second, _) =
                    next_buffered_token(lexer, position + 1, &mut self.needs_legacy_reparse);
                (first, second)
            }
        }
    }

    /// Bumps the token source to the next non-trivia token.
    ///
    /// It pushes the given kind to the token vector with the current token range.
    #[inline]
    pub(crate) fn bump(&mut self, kind: TokenKind) {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => {
                self.tokens.push(Token::new(
                    kind,
                    lexer.current_range(),
                    lexer.current_flags(),
                ));
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked {
                lexer,
                position,
                nesting,
            } => {
                let current_position = *position;
                let token = self.current;
                if token.kind() != kind {
                    lexer.tokens.rewrites.push((current_position, token));
                    lexer.tokens.tokens[current_position] =
                        Token::new(kind, token.range(), token.flags());
                }
                self.current =
                    bump_buffered_token(lexer, position, nesting, &mut self.needs_legacy_reparse);
                return;
            }
        }
        self.do_bump();
    }

    /// Bumps the token source to the next non-trivia token without adding the current token to the
    /// token vector. It does add the trivia tokens to the token vector.
    #[inline]
    fn do_bump(&mut self) {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => {
                loop {
                    let kind = lexer.next_token();
                    if kind.is_trivia() {
                        self.tokens.push(Token::new(
                            kind,
                            lexer.current_range(),
                            lexer.current_flags(),
                        ));
                        continue;
                    }
                    break;
                }

                #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
                {
                    self.current = Token::new(
                        lexer.current_kind(),
                        lexer.current_range(),
                        lexer.current_flags(),
                    );
                }
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked {
                lexer,
                position,
                nesting,
            } => {
                self.current =
                    bump_buffered_token(lexer, position, nesting, &mut self.needs_legacy_reparse);
            }
        }
    }

    /// Creates a checkpoint to which the token source can later return to using [`Self::rewind`].
    pub(crate) fn checkpoint(&self) -> TokenSourceCheckpoint {
        TokenSourceCheckpoint {
            lexer_checkpoint: match &self.lexer {
                LexerSource::Streaming(lexer) => {
                    LexerSourceCheckpoint::Streaming(lexer.checkpoint())
                }
                #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
                LexerSource::Chunked {
                    lexer,
                    position,
                    nesting,
                    ..
                } => LexerSourceCheckpoint::Chunked {
                    position: *position,
                    nesting: *nesting,
                    rewrites_position: lexer.tokens.rewrites.len(),
                },
            },
            tokens_position: self.tokens.len(),
        }
    }

    /// Restore the token source to the given checkpoint.
    #[cfg_attr(
        not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")),
        expect(irrefutable_let_patterns, reason = "the chunked variant is SIMD-only")
    )]
    pub(crate) fn rewind(&mut self, checkpoint: TokenSourceCheckpoint) {
        let TokenSourceCheckpoint {
            lexer_checkpoint,
            tokens_position,
        } = checkpoint;

        match lexer_checkpoint {
            LexerSourceCheckpoint::Streaming(checkpoint) => {
                if let LexerSource::Streaming(lexer) = &mut self.lexer {
                    lexer.rewind(checkpoint);
                }
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSourceCheckpoint::Chunked {
                position,
                nesting,
                rewrites_position,
            } => {
                if let LexerSource::Chunked {
                    lexer,
                    position: current_position,
                    nesting: current_nesting,
                    ..
                } = &mut self.lexer
                {
                    rewind_rewrites(lexer, rewrites_position);
                    *current_position = position;
                    *current_nesting = nesting;
                }
            }
        }
        self.tokens.truncate(tokens_position);

        #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
        self.refresh_current();
    }

    /// Returns a slice of [`Token`] that are within the given `range`.
    pub(crate) fn in_range(&self, range: TextRange) -> &[Token] {
        let tokens = match &self.lexer {
            LexerSource::Streaming(_) => &self.tokens,
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked {
                lexer, position, ..
            } => &lexer.tokens.tokens[..=*position],
        };
        let start = tokens.iter().rposition(|tok| tok.start() == range.start());
        let end = tokens.iter().rposition(|tok| tok.end() == range.end());

        let (Some(start), Some(end)) = (start, end) else {
            return tokens;
        };

        &tokens[start..=end]
    }

    /// Consumes the token source, returning all trivia and non-trivia tokens plus lexical errors.
    pub(crate) fn finish(self) -> (Vec<Token>, Vec<LexicalError>) {
        assert_eq!(
            self.current_kind(),
            TokenKind::EndOfFile,
            "TokenSource was not fully consumed"
        );

        // The `EndOfFile` token shouldn't be included in the token stream, it's mainly to signal
        // the parser to stop. This isn't in `do_bump` because it only needs to be done once.
        let (mut tokens, errors) = match self.lexer {
            LexerSource::Streaming(lexer) => (self.tokens, lexer.finish()),
            #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
            LexerSource::Chunked { lexer, .. } => (lexer.tokens.tokens, Vec::new()),
        };

        if let Some(last) = tokens.pop() {
            assert_eq!(last.kind(), TokenKind::EndOfFile);
        }
        tokens.shrink_to_fit();

        (tokens, errors)
    }

    /// Returns whether chunked lexing or parser-directed re-lexing requires a streaming reparse.
    #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
    pub(crate) fn should_reparse_with_legacy_lexer(&self) -> bool {
        self.needs_legacy_reparse
    }

    #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
    #[inline]
    fn refresh_current(&mut self) {
        self.current = match &self.lexer {
            LexerSource::Streaming(lexer) => Token::new(
                lexer.current_kind(),
                lexer.current_range(),
                lexer.current_flags(),
            ),
            LexerSource::Chunked {
                lexer, position, ..
            } => lexer.tokens.tokens[*position],
        };
    }
}

pub(crate) struct TokenSourceCheckpoint {
    lexer_checkpoint: LexerSourceCheckpoint,
    tokens_position: usize,
}

enum LexerSourceCheckpoint {
    Streaming(LexerCheckpoint),
    #[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
    Chunked {
        position: usize,
        nesting: u32,
        rewrites_position: usize,
    },
}

fn next_non_trivia_token(lexer: &mut Lexer) -> TokenKind {
    loop {
        let kind = lexer.next_token();
        if !kind.is_trivia() {
            break kind;
        }
    }
}

/// Finds the next non-trivia buffered token, refilling on demand without advancing the parser.
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
fn next_buffered_token(
    lexer: &mut ChunkedLexer,
    mut position: usize,
    needs_legacy_reparse: &mut bool,
) -> (TokenKind, usize) {
    loop {
        position = ensure_buffered_token(lexer, position, needs_legacy_reparse);
        let token = lexer.tokens.tokens[position];
        if !token.kind().is_trivia() {
            return (token.kind(), position);
        }
        position += 1;
    }
}

/// Advances to the next non-trivia buffered token and maintains delimiter nesting for the parser.
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
#[inline]
fn bump_buffered_token(
    lexer: &mut ChunkedLexer,
    position: &mut usize,
    nesting: &mut u32,
    needs_legacy_reparse: &mut bool,
) -> Token {
    let mut next = position.wrapping_add(1);
    loop {
        if let Some(token) = lexer.tokens.tokens.get(next) {
            match token.kind() {
                TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => *nesting += 1,
                TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                    *nesting = nesting.saturating_sub(1);
                }
                _ => {}
            }
            if !token.kind().is_trivia() {
                *position = next;
                return *token;
            }
            next += 1;
            continue;
        }

        next = ensure_buffered_token(lexer, next, needs_legacy_reparse);
    }
}

#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
#[inline]
fn ensure_buffered_token(
    lexer: &mut ChunkedLexer,
    position: usize,
    needs_legacy_reparse: &mut bool,
) -> usize {
    if position < lexer.tokens.tokens.len() {
        return position;
    }

    refill_buffered_tokens(lexer, position, needs_legacy_reparse);
    position.min(lexer.tokens.tokens.len() - 1)
}

/// Refills through `position`. A failed batch appends a synthetic EOF and requests one streaming
/// reparse, preventing repeated failed refills during error recovery.
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
#[cold]
fn refill_buffered_tokens(
    lexer: &mut ChunkedLexer,
    position: usize,
    needs_legacy_reparse: &mut bool,
) {
    while position >= lexer.tokens.tokens.len()
        && !lexer.is_finished()
        && lexer
            .tokens
            .tokens
            .last()
            .is_none_or(|token| token.kind() != TokenKind::EndOfFile)
    {
        if lexer.fill().is_none() {
            *needs_legacy_reparse = true;
            let end = lexer
                .tokens
                .tokens
                .last()
                .map_or(TextSize::default(), Token::end);
            lexer.tokens.tokens.push(Token::new(
                TokenKind::EndOfFile,
                TextRange::empty(end),
                TokenFlags::empty(),
            ));
            break;
        }
    }
}

/// Restores contextual token-kind rewrites made after a parser checkpoint.
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
fn rewind_rewrites(lexer: &mut ChunkedLexer, position: usize) {
    for (index, token) in lexer.tokens.rewrites.drain(position..).rev() {
        lexer.tokens.tokens[index] = token;
    }
}

/// Allocates a token buffer with a capacity intended to skip early grows.
fn allocate_tokens_vec(contents: &str) -> Vec<Token> {
    // In sampled ruff-ecosystem projects, about three quarters of Python files contain at least
    // one token per eight source bytes. Intentionally underestimate the final token count to avoid
    // over-reserving for token-sparse files.
    const BYTES_PER_PREALLOCATED_TOKEN: usize = 8;
    const MIN_INITIAL_CAPACITY: usize = 128;

    let capacity_hint = contents.len() / BYTES_PER_PREALLOCATED_TOKEN;
    if capacity_hint < MIN_INITIAL_CAPACITY {
        return Vec::new();
    }

    // Stay on a power-of-two bucket so that later geometric growth does not preserve an
    // arbitrary capacity offset.
    Vec::with_capacity(1 << capacity_hint.ilog2())
}

#[cfg(all(
    test,
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
))]
mod tests {
    use ruff_text_size::TextSize;

    use crate::Mode;
    use crate::token_source::{LexerSource, TokenSource};

    #[test]
    fn module_uses_chunked_lexer() {
        let source = TokenSource::from_source("value = 1\n", Mode::Module, TextSize::new(0));
        assert!(matches!(source.lexer, LexerSource::Chunked { .. }));
    }
}
