use ruff_python_ast::token::{Token, TokenFlags, TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Mode;
use crate::error::LexicalError;
use crate::lexer::{Lexer, LexerCheckpoint};
#[cfg(target_arch = "aarch64")]
use crate::lexer::{two_pass, two_pass::TwoPassTokens};
use crate::string::InterpolatedStringKind;

/// Token source for the parser that skips over any trivia tokens.
#[derive(Debug)]
pub(crate) struct TokenSource<'src> {
    /// The underlying source for the tokens.
    lexer: LexerSource<'src>,

    /// The current non-trivia token. Keeping this separate from the pre-lexed stream avoids an
    /// enum match and an indexed load for every parser predicate.
    #[cfg(target_arch = "aarch64")]
    current: Token,

    /// Whether error recovery requested a logical-token re-lex that requires the streaming lexer.
    #[cfg(target_arch = "aarch64")]
    needs_legacy_reparse: bool,

    /// A vector containing all the tokens emitted by the lexer. This is returned when the parser
    /// is finished consuming all the tokens. Note that unlike the emitted tokens, this vector
    /// holds both the trivia and non-trivia tokens.
    tokens: Vec<Token>,
}

#[derive(Debug)]
enum LexerSource<'src> {
    Streaming(Lexer<'src>),
    #[cfg(target_arch = "aarch64")]
    TwoPass {
        tokens: TwoPassTokens,
        position: usize,
        nesting: u32,
    },
}

impl<'src> TokenSource<'src> {
    /// Create a new token source for the given lexer.
    pub(crate) fn new(lexer: Lexer<'src>, source: &'src str, start_offset: TextSize) -> Self {
        TokenSource {
            lexer: LexerSource::Streaming(lexer),
            #[cfg(target_arch = "aarch64")]
            current: Token::new(
                TokenKind::EndOfFile,
                TextRange::empty(start_offset),
                TokenFlags::empty(),
            ),
            #[cfg(target_arch = "aarch64")]
            needs_legacy_reparse: false,
            tokens: allocate_tokens_vec(&source[start_offset.to_usize()..]),
        }
    }

    /// Create a new token source from the given source code which starts at the given offset.
    pub(crate) fn from_source(
        source: &'src str,
        mode: Mode,
        start_offset: TextSize,
        two_pass_lexer: bool,
    ) -> Self {
        #[cfg(not(target_arch = "aarch64"))]
        let _ = two_pass_lexer;

        #[cfg(target_arch = "aarch64")]
        if two_pass_lexer
            && mode == Mode::Module
            && start_offset == TextSize::new(0)
            && let Some(tokens) = two_pass::lex(source)
        {
            let mut source = TokenSource {
                lexer: LexerSource::TwoPass {
                    tokens,
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

        let lexer = Lexer::new(source, mode, start_offset);
        let mut source = TokenSource::new(lexer, source, start_offset);

        // Initialize the token source so that the current token is set correctly.
        source.do_bump();
        source
    }

    /// Returns the kind of the current token.
    #[inline]
    pub(crate) fn current_kind(&self) -> TokenKind {
        #[cfg(target_arch = "aarch64")]
        return self.current.kind();

        #[cfg(not(target_arch = "aarch64"))]
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_kind(),
        }
    }

    /// Returns the range of the current token.
    #[inline]
    pub(crate) fn current_range(&self) -> TextRange {
        #[cfg(target_arch = "aarch64")]
        return self.current.range();

        #[cfg(not(target_arch = "aarch64"))]
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_range(),
        }
    }

    /// Returns the current parenthesis, bracket, and brace nesting level.
    #[inline]
    pub(crate) fn nesting(&self) -> u32 {
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.nesting(),
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass { nesting, .. } => *nesting,
        }
    }

    /// Returns the flags for the current token.
    #[inline]
    pub(crate) fn current_flags(&self) -> TokenFlags {
        #[cfg(target_arch = "aarch64")]
        return self.current.flags();

        #[cfg(not(target_arch = "aarch64"))]
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_flags(),
        }
    }

    /// Calls the underlying [`re_lex_logical_token`] method on the lexer with the new lexer
    /// position and updates the token vector accordingly.
    ///
    /// [`re_lex_logical_token`]: Lexer::re_lex_logical_token
    #[cfg_attr(
        not(target_arch = "aarch64"),
        expect(irrefutable_let_patterns, reason = "the two-pass variant is ARM-only")
    )]
    pub(crate) fn re_lex_logical_token(&mut self) {
        #[cfg(target_arch = "aarch64")]
        if matches!(self.lexer, LexerSource::TwoPass { .. }) {
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

        #[cfg(target_arch = "aarch64")]
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

    #[cfg_attr(
        not(target_arch = "aarch64"),
        expect(irrefutable_let_patterns, reason = "the two-pass variant is ARM-only")
    )]
    pub(crate) fn re_lex_string_token_in_interpolation_element(
        &mut self,
        kind: InterpolatedStringKind,
    ) {
        #[cfg(target_arch = "aarch64")]
        {
            if matches!(self.lexer, LexerSource::TwoPass { .. }) {
                return;
            }
        }

        if let LexerSource::Streaming(lexer) = &mut self.lexer {
            lexer.re_lex_string_token_in_interpolation_element(kind);
        }

        #[cfg(target_arch = "aarch64")]
        self.refresh_current();
    }

    #[cfg_attr(
        not(target_arch = "aarch64"),
        expect(irrefutable_let_patterns, reason = "the two-pass variant is ARM-only")
    )]
    pub(crate) fn re_lex_raw_string_in_format_spec(&mut self) {
        #[cfg(target_arch = "aarch64")]
        {
            if matches!(self.lexer, LexerSource::TwoPass { .. }) {
                return;
            }
        }

        if let LexerSource::Streaming(lexer) = &mut self.lexer {
            lexer.re_lex_raw_string_in_format_spec();
        }

        #[cfg(target_arch = "aarch64")]
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
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => next_pre_lexed_token(tokens, *position + 1).0,
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
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => {
                let (first, position) = next_pre_lexed_token(tokens, *position + 1);
                let (second, _) = next_pre_lexed_token(tokens, position + 1);
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
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens,
                position,
                nesting,
            } => {
                let current_position = *position;
                let token = self.current;
                if token.kind() != kind {
                    tokens.rewrites.push((current_position, token));
                    tokens.tokens[current_position] =
                        Token::new(kind, token.range(), token.flags());
                }
                self.current = bump_pre_lexed_token(tokens, position, nesting);
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

                #[cfg(target_arch = "aarch64")]
                {
                    self.current = Token::new(
                        lexer.current_kind(),
                        lexer.current_range(),
                        lexer.current_flags(),
                    );
                }
            }
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens,
                position,
                nesting,
            } => {
                self.current = bump_pre_lexed_token(tokens, position, nesting);
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
                #[cfg(target_arch = "aarch64")]
                LexerSource::TwoPass {
                    tokens,
                    position,
                    nesting,
                } => LexerSourceCheckpoint::TwoPass {
                    position: *position,
                    nesting: *nesting,
                    rewrites_position: tokens.rewrites.len(),
                },
            },
            tokens_position: self.tokens.len(),
        }
    }

    /// Restore the token source to the given checkpoint.
    #[cfg_attr(
        not(target_arch = "aarch64"),
        expect(irrefutable_let_patterns, reason = "the two-pass variant is ARM-only")
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
            #[cfg(target_arch = "aarch64")]
            LexerSourceCheckpoint::TwoPass {
                position,
                nesting,
                rewrites_position,
            } => {
                if let LexerSource::TwoPass {
                    tokens,
                    position: current_position,
                    nesting: current_nesting,
                } = &mut self.lexer
                {
                    rewind_rewrites(tokens, rewrites_position);
                    *current_position = position;
                    *current_nesting = nesting;
                }
            }
        }
        self.tokens.truncate(tokens_position);

        #[cfg(target_arch = "aarch64")]
        self.refresh_current();
    }

    /// Returns a slice of [`Token`] that are within the given `range`.
    pub(crate) fn in_range(&self, range: TextRange) -> &[Token] {
        let tokens = match &self.lexer {
            LexerSource::Streaming(_) => &self.tokens,
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => &tokens.tokens[..=*position],
        };
        let start = tokens.iter().rposition(|tok| tok.start() == range.start());
        let end = tokens.iter().rposition(|tok| tok.end() == range.end());

        let (Some(start), Some(end)) = (start, end) else {
            return tokens;
        };

        &tokens[start..=end]
    }

    /// Consumes the token source, returning the collected tokens, comment ranges, and any errors
    /// encountered during lexing. The token collection includes both the trivia and non-trivia
    /// tokens.
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
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass { tokens, .. } => (tokens.tokens, Vec::new()),
        };

        if let Some(last) = tokens.pop() {
            assert_eq!(last.kind(), TokenKind::EndOfFile);
        }
        tokens.shrink_to_fit();

        (tokens, errors)
    }

    #[cfg(target_arch = "aarch64")]
    pub(crate) fn should_reparse_with_legacy_lexer(&self) -> bool {
        self.needs_legacy_reparse
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn refresh_current(&mut self) {
        self.current = match &self.lexer {
            LexerSource::Streaming(lexer) => Token::new(
                lexer.current_kind(),
                lexer.current_range(),
                lexer.current_flags(),
            ),
            LexerSource::TwoPass {
                tokens, position, ..
            } => tokens.tokens[*position],
        };
    }
}

pub(crate) struct TokenSourceCheckpoint {
    lexer_checkpoint: LexerSourceCheckpoint,
    tokens_position: usize,
}

enum LexerSourceCheckpoint {
    Streaming(LexerCheckpoint),
    #[cfg(target_arch = "aarch64")]
    TwoPass {
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

#[cfg(target_arch = "aarch64")]
fn next_pre_lexed_token(tokens: &TwoPassTokens, mut position: usize) -> (TokenKind, usize) {
    position = position.min(tokens.tokens.len() - 1);
    while tokens.tokens[position].kind().is_trivia() {
        position += 1;
    }
    (tokens.tokens[position].kind(), position)
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn bump_pre_lexed_token(tokens: &TwoPassTokens, position: &mut usize, nesting: &mut u32) -> Token {
    let mut next = position.wrapping_add(1).min(tokens.tokens.len() - 1);
    loop {
        let token = tokens.tokens[next];
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => *nesting += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                *nesting = nesting.saturating_sub(1);
            }
            _ => {}
        }
        if !token.kind().is_trivia() {
            *position = next;
            return token;
        }
        next += 1;
    }
}

#[cfg(target_arch = "aarch64")]
fn rewind_rewrites(tokens: &mut TwoPassTokens, position: usize) {
    for (index, token) in tokens.rewrites.drain(position..).rev() {
        tokens.tokens[index] = token;
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
