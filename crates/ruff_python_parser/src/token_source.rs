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

    #[cfg(target_arch = "aarch64")]
    source: &'src str,
    #[cfg(target_arch = "aarch64")]
    mode: Mode,
    #[cfg(target_arch = "aarch64")]
    start_offset: TextSize,

    /// Kept when error recovery temporarily switches to the streaming lexer inside a parser
    /// checkpoint. Rewinding that checkpoint restores the pre-lexed stream.
    #[cfg(target_arch = "aarch64")]
    two_pass_backup: Option<TwoPassTokens>,

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
        position: Option<usize>,
        nesting: u32,
    },
}

impl<'src> TokenSource<'src> {
    /// Create a new token source for the given lexer.
    pub(crate) fn new(
        lexer: Lexer<'src>,
        source: &'src str,
        mode: Mode,
        start_offset: TextSize,
    ) -> Self {
        #[cfg(not(target_arch = "aarch64"))]
        let _ = mode;

        TokenSource {
            lexer: LexerSource::Streaming(lexer),
            #[cfg(target_arch = "aarch64")]
            source,
            #[cfg(target_arch = "aarch64")]
            mode,
            #[cfg(target_arch = "aarch64")]
            start_offset,
            #[cfg(target_arch = "aarch64")]
            two_pass_backup: None,
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
                    position: None,
                    nesting: 0,
                },
                source,
                mode,
                start_offset,
                two_pass_backup: None,
                tokens: Vec::new(),
            };
            source.do_bump();
            return source;
        }

        let lexer = Lexer::new(source, mode, start_offset);
        let mut source = TokenSource::new(lexer, source, mode, start_offset);

        // Initialize the token source so that the current token is set correctly.
        source.do_bump();
        source
    }

    /// Returns the kind of the current token.
    #[inline]
    pub(crate) fn current_kind(&self) -> TokenKind {
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_kind(),
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => tokens.tokens[position.unwrap_or(0)].kind(),
        }
    }

    /// Returns the range of the current token.
    #[inline]
    pub(crate) fn current_range(&self) -> TextRange {
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_range(),
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => tokens.tokens[position.unwrap_or(0)].range(),
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
        match &self.lexer {
            LexerSource::Streaming(lexer) => lexer.current_flags(),
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => tokens.tokens[position.unwrap_or(0)].flags(),
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
        self.replay_with_streaming_lexer();

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
            if matches!(self.lexer, LexerSource::TwoPass { .. })
                && (self.current_kind() != TokenKind::String
                    || !self.current_flags().intersects(TokenFlags::UNCLOSED_STRING))
            {
                return;
            }
            self.replay_with_streaming_lexer();
        }

        if let LexerSource::Streaming(lexer) = &mut self.lexer {
            lexer.re_lex_string_token_in_interpolation_element(kind);
        }
    }

    #[cfg_attr(
        not(target_arch = "aarch64"),
        expect(irrefutable_let_patterns, reason = "the two-pass variant is ARM-only")
    )]
    pub(crate) fn re_lex_raw_string_in_format_spec(&mut self) {
        #[cfg(target_arch = "aarch64")]
        {
            if matches!(self.lexer, LexerSource::TwoPass { .. })
                && (self.current_kind() != TokenKind::String
                    || !self
                        .current_flags()
                        .contains(TokenFlags::UNCLOSED_STRING | TokenFlags::RAW_STRING_LOWERCASE))
            {
                return;
            }
            self.replay_with_streaming_lexer();
        }

        if let LexerSource::Streaming(lexer) = &mut self.lexer {
            lexer.re_lex_raw_string_in_format_spec();
        }
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
            } => next_pre_lexed_token(tokens, position.unwrap_or(0) + 1).0,
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
                let (first, position) = next_pre_lexed_token(tokens, position.unwrap_or(0) + 1);
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
                tokens, position, ..
            } => {
                let position = position.unwrap_or(0);
                let token = tokens.tokens[position];
                if token.kind() != kind {
                    tokens.rewrites.push((position, token));
                    tokens.tokens[position] = Token::new(kind, token.range(), token.flags());
                }
            }
        }
        self.do_bump();
    }

    /// Bumps the token source to the next non-trivia token without adding the current token to the
    /// token vector. It does add the trivia tokens to the token vector.
    #[inline]
    fn do_bump(&mut self) {
        match &mut self.lexer {
            LexerSource::Streaming(lexer) => loop {
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
            },
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens,
                position,
                nesting,
            } => {
                let mut next = position
                    .map_or(0, |position| position + 1)
                    .min(tokens.tokens.len() - 1);
                loop {
                    let kind = tokens.tokens[next].kind();
                    match kind {
                        TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => *nesting += 1,
                        TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                            *nesting = nesting.saturating_sub(1);
                        }
                        _ => {}
                    }
                    *position = Some(next);
                    if kind.is_trivia() {
                        next += 1;
                        continue;
                    }
                    break;
                }
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
                } else if let Some(mut tokens) = self.two_pass_backup.take() {
                    rewind_rewrites(&mut tokens, rewrites_position);
                    self.lexer = LexerSource::TwoPass {
                        tokens,
                        position,
                        nesting,
                    };
                }
            }
        }
        self.tokens.truncate(tokens_position);
    }

    /// Returns a slice of [`Token`] that are within the given `range`.
    pub(crate) fn in_range(&self, range: TextRange) -> &[Token] {
        let tokens = match &self.lexer {
            LexerSource::Streaming(_) => &self.tokens,
            #[cfg(target_arch = "aarch64")]
            LexerSource::TwoPass {
                tokens, position, ..
            } => &tokens.tokens[..=position.unwrap_or(0)],
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
    #[cold]
    fn replay_with_streaming_lexer(&mut self) {
        let LexerSource::TwoPass {
            tokens, position, ..
        } = &self.lexer
        else {
            return;
        };
        let Some(position) = *position else {
            return;
        };

        let mut lexer = Lexer::new(self.source, self.mode, self.start_offset);
        for index in 0..=position {
            let kind = lexer.next_token();
            let token = tokens.tokens[index];
            if index == position {
                debug_assert_eq!(kind, token.kind());
            }
            debug_assert_eq!(lexer.current_range(), token.range());
            debug_assert_eq!(lexer.current_flags(), token.flags());
        }
        let previous = std::mem::replace(&mut self.lexer, LexerSource::Streaming(lexer));
        if let LexerSource::TwoPass { tokens, .. } = previous {
            self.tokens.extend_from_slice(&tokens.tokens[..position]);
            self.two_pass_backup = Some(tokens);
        }
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
        position: Option<usize>,
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
