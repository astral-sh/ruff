use std::cmp::Ordering;

use bitflags::bitflags;
use drop_bomb::DebugDropBomb;

use ast::Mod;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::lexer::lex;
use crate::parser::progress::ParserProgress;
use crate::{
    lexer::{LexResult, Spanned},
    token_set::TokenSet,
    token_source::TokenSource,
    Mode, ParseError, ParseErrorType, Tok, TokenKind,
};

mod expression;
mod helpers;
mod pattern;
mod progress;
mod statement;
#[cfg(test)]
mod tests;

pub(crate) fn parse_tokens(
    tokens: Vec<LexResult>,
    source: &str,
    mode: Mode,
) -> Result<Mod, ParseError> {
    let program = Parser::new(source, mode, TokenSource::new(tokens)).parse_program();
    if program.parse_errors.is_empty() {
        Ok(program.ast)
    } else {
        Err(program.parse_errors.into_iter().next().unwrap())
    }
}

#[derive(Debug)]
pub struct Program {
    pub ast: ast::Mod,
    pub parse_errors: Vec<ParseError>,
}

impl Program {
    pub fn parse_str(source: &str, mode: Mode) -> Program {
        let tokens = lex(source, mode);
        Self::parse_tokens(source, tokens.collect(), mode)
    }

    pub fn parse_tokens(source: &str, tokens: Vec<LexResult>, mode: Mode) -> Program {
        Parser::new(source, mode, TokenSource::new(tokens)).parse_program()
    }
}

pub(crate) struct Parser<'src> {
    source: &'src str,
    tokens: TokenSource,

    /// Stores all the syntax errors found during the parsing.
    errors: Vec<ParseError>,

    /// This tracks the current expression or statement being parsed. For example,
    /// if we're parsing a tuple expression, e.g. `(1, 2)`, `ctx` has the value
    /// `ParserCtxFlags::TUPLE_EXPR`.
    ///
    /// The `ctx` is also used to create custom error messages and forbid certain
    /// expressions or statements of being parsed. The `ctx` should be empty after
    /// an expression or statement is done parsing.
    ctx: ParserCtxFlags,

    /// Specify the mode in which the code will be parsed.
    mode: Mode,

    current: Spanned,

    /// The end of the last processed. Used to determine a node's end.
    last_token_end: TextSize,

    /// The range of the tokens to parse.
    ///
    /// The range is equal to [0; source.len()) when parsing an entire file.
    /// The range can be different when parsing only a part of a file using the `lex_starts_at` and `parse_expression_starts_at` APIs
    /// in which case the the range is equal to [offset; subrange.len()).
    tokens_range: TextRange,

    recovery_context: RecoveryContext,
}

const NEWLINE_EOF_SET: TokenSet = TokenSet::new([TokenKind::Newline, TokenKind::EndOfFile]);
const LITERAL_SET: TokenSet = TokenSet::new([
    TokenKind::Int,
    TokenKind::Float,
    TokenKind::Complex,
    TokenKind::String,
    TokenKind::Ellipsis,
    TokenKind::True,
    TokenKind::False,
    TokenKind::None,
]);
/// Tokens that are usually an expression or the start of one.
const EXPR_SET: TokenSet = TokenSet::new([
    TokenKind::Name,
    TokenKind::Minus,
    TokenKind::Plus,
    TokenKind::Tilde,
    TokenKind::Star,
    TokenKind::DoubleStar,
    TokenKind::Lpar,
    TokenKind::Lbrace,
    TokenKind::Lsqb,
    TokenKind::Lambda,
    TokenKind::Await,
    TokenKind::Not,
    TokenKind::Yield,
    TokenKind::FStringStart,
])
.union(LITERAL_SET);

impl<'src> Parser<'src> {
    pub(crate) fn new(source: &'src str, mode: Mode, mut tokens: TokenSource) -> Parser<'src> {
        let tokens_range = TextRange::new(
            tokens.position().unwrap_or_default(),
            tokens.end().unwrap_or_default(),
        );

        let current = tokens
            .next()
            .unwrap_or_else(|| (Tok::EndOfFile, TextRange::empty(tokens_range.end())));

        Parser {
            mode,
            source,
            errors: Vec::new(),
            ctx: ParserCtxFlags::empty(),
            tokens,
            recovery_context: RecoveryContext::empty(),
            last_token_end: tokens_range.start(),
            current,
            tokens_range,
        }
    }

    pub(crate) fn parse_program(mut self) -> Program {
        let ast = if self.mode == Mode::Expression {
            let start = self.node_start();
            let parsed_expr = self.parse_expression();
            let mut progress = ParserProgress::default();

            // TODO: How should error recovery work here? Just truncate after the expression?
            loop {
                progress.assert_progressing(&self);
                if !self.eat(TokenKind::Newline) {
                    break;
                }
            }
            self.bump(TokenKind::EndOfFile);

            Mod::Expression(ast::ModExpression {
                body: Box::new(parsed_expr.expr),
                range: self.node_range(start),
            })
        } else {
            let body = self.parse_list(
                RecoveryContextKind::ModuleStatements,
                Parser::parse_statement,
            );

            self.bump(TokenKind::EndOfFile);

            Mod::Module(ast::ModModule {
                body,
                range: self.tokens_range,
            })
        };

        Program {
            ast,
            parse_errors: self.finish(),
        }
    }

    fn finish(self) -> Vec<ParseError> {
        // After parsing, the `ctx` and `ctx_stack` should be empty.
        // If it's not, you probably forgot to call `clear_ctx` somewhere.
        assert_eq!(self.ctx, ParserCtxFlags::empty());
        assert_eq!(
            self.current_kind(),
            TokenKind::EndOfFile,
            "Parser should be at the end of the file."
        );

        // TODO consider re-integrating lexical error handling into the parser?
        let parse_errors = self.errors;
        let lex_errors = self.tokens.finish();

        // Fast path for when there are no lex errors.
        // There's no fast path for when there are no parse errors because a lex error
        // always results in a parse error.
        if lex_errors.is_empty() {
            return parse_errors;
        }

        let mut merged = Vec::with_capacity(parse_errors.len().saturating_add(lex_errors.len()));

        let mut parse_errors = parse_errors.into_iter().peekable();
        let mut lex_errors = lex_errors.into_iter().peekable();

        while let (Some(parse_error), Some(lex_error)) = (parse_errors.peek(), lex_errors.peek()) {
            match parse_error
                .location
                .start()
                .cmp(&lex_error.location().start())
            {
                Ordering::Less => merged.push(parse_errors.next().unwrap()),
                Ordering::Equal => {
                    // Skip the parse error if we already have a lex error at the same location..
                    parse_errors.next().unwrap();
                    merged.push(lex_errors.next().unwrap().into());
                }
                Ordering::Greater => merged.push(lex_errors.next().unwrap().into()),
            }
        }

        merged.extend(parse_errors);
        merged.extend(lex_errors.map(ParseError::from));

        merged
    }

    #[inline]
    #[must_use]
    fn set_ctx(&mut self, ctx: ParserCtxFlags) -> SavedParserContext {
        SavedParserContext {
            flags: std::mem::replace(&mut self.ctx, ctx),
            bomb: DebugDropBomb::new(
                "You must restore the old parser context explicit by calling `clear_ctx`.",
            ),
        }
    }

    #[inline]
    fn restore_ctx(&mut self, current: ParserCtxFlags, mut saved_context: SavedParserContext) {
        assert_eq!(self.ctx, current);
        saved_context.bomb.defuse();
        self.ctx = saved_context.flags;
    }

    #[inline]
    fn has_ctx(&self, ctx: ParserCtxFlags) -> bool {
        self.ctx.intersects(ctx)
    }

    /// Returns the start position for a node that starts at the current token.
    fn node_start(&self) -> TextSize {
        self.current_range().start()
    }

    fn node_range(&self, start: TextSize) -> TextRange {
        // It's possible during error recovery that the parsing didn't consume any tokens. In that case,
        // `last_token_end` still points to the end of the previous token but `start` is the start of the current token.
        // Calling `TextRange::new(start, self.last_token_end)` would panic in that case because `start > end`.
        // This path "detects" this case and creates an empty range instead.
        if self.node_start() == start {
            TextRange::empty(start)
        } else {
            TextRange::new(start, self.last_token_end)
        }
    }

    fn missing_node_range(&self) -> TextRange {
        TextRange::empty(self.last_token_end)
    }

    /// Moves the parser to the next token. Returns the old current token as an owned value.
    /// FIXME(micha): Using `next_token` is almost always incorrect if there's a case where the current token is not of the expected type.
    fn next_token(&mut self) -> Spanned {
        let next = self
            .tokens
            .next()
            .unwrap_or_else(|| (Tok::EndOfFile, TextRange::empty(self.tokens_range.end())));

        let current = std::mem::replace(&mut self.current, next);

        if !matches!(
            current.0,
            // TODO explore including everything up to the dedent as part of the body.
            Tok::Dedent
            // Don't include newlines in the body
            | Tok::Newline
            // TODO(micha): Including the semi feels more correct but it isn't compatible with lalrpop and breaks the
            // formatters semicolon detection. Exclude it for now
            | Tok::Semi
        ) {
            self.last_token_end = current.1.end();
        }

        current
    }

    fn peek_nth(&self, offset: usize) -> TokenKind {
        if offset == 0 {
            self.current_kind()
        } else {
            self.tokens
                .peek_nth(offset - 1)
                .map_or(TokenKind::EndOfFile, |spanned| spanned.0)
        }
    }

    #[inline]
    fn current_token(&self) -> (TokenKind, TextRange) {
        (self.current_kind(), self.current_range())
    }

    #[inline]
    fn current_kind(&self) -> TokenKind {
        // TODO: Converting the token kind over and over again can be expensive.
        TokenKind::from_token(&self.current.0)
    }

    #[inline]
    fn current_range(&self) -> TextRange {
        self.current.1
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if !self.at(kind) {
            return false;
        }

        self.next_token();
        true
    }

    /// Bumps the current token assuming it is of the given kind.
    ///
    /// # Panics
    /// If the current token is not of the given kind.
    ///
    /// # Returns
    /// The current token
    fn bump(&mut self, kind: TokenKind) -> (Tok, TextRange) {
        assert_eq!(self.current_kind(), kind);

        self.next_token()
    }

    fn expect(&mut self, expected: TokenKind) -> bool {
        if self.eat(expected) {
            return true;
        }

        let (found, range) = self.current_token();
        self.add_error(ParseErrorType::ExpectedToken { found, expected }, range);
        false
    }

    fn add_error<T>(&mut self, error: ParseErrorType, ranged: T)
    where
        T: Ranged,
    {
        fn inner(errors: &mut Vec<ParseError>, error: ParseErrorType, range: TextRange) {
            // Avoid flagging multiple errors at the same location
            let is_same_location = errors
                .last()
                .is_some_and(|last| last.location.start() == range.start());

            if !is_same_location {
                errors.push(ParseError {
                    error,
                    location: range,
                });
            }
        }

        inner(&mut self.errors, error, ranged.range());
    }

    /// Skip tokens until [`TokenSet`]. Returns the range of the skipped tokens.

    #[deprecated(note = "We should not perform error recovery outside of lists. Remove")]
    fn skip_until(&mut self, token_set: TokenSet) {
        let mut progress = ParserProgress::default();
        while !self.at_ts(token_set) {
            progress.assert_progressing(self);
            self.next_token();
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    fn at_ts(&self, ts: TokenSet) -> bool {
        ts.contains(self.current_kind())
    }

    fn src_text<T>(&self, ranged: T) -> &'src str
    where
        T: Ranged,
    {
        let range = ranged.range();
        // `ranged` uses absolute ranges to the source text of an entire file.
        // Fix the source by subtracting the start offset when parsing only a part of a file (when parsing the tokens from `lex_starts_at`).
        &self.source[range - self.tokens_range.start()]
    }

    fn parse_list<T>(
        &mut self,
        kind: RecoveryContextKind,
        parse_element: impl Fn(&mut Parser<'src>) -> T,
    ) -> Vec<T> {
        let mut elements = Vec::new();

        self.parse_sequence(kind, |p| elements.push(parse_element(p)));

        elements
    }

    fn parse_sequence(
        &mut self,
        kind: RecoveryContextKind,
        mut parse_element: impl FnMut(&mut Parser<'src>),
    ) {
        let mut progress = ParserProgress::default();

        let saved_context = self.recovery_context;
        self.recovery_context = self
            .recovery_context
            .union(RecoveryContext::from_kind(kind));

        while !kind.is_list_terminator(self) {
            progress.assert_progressing(self);

            // The end of file marker ends all lists.
            if self.at(TokenKind::EndOfFile) {
                break;
            }

            if kind.is_list_element(self) {
                parse_element(self);
            } else {
                let should_recover = self.is_enclosing_list_element_or_terminator();

                // Not a recognised element. Add an error and either skip the token or break parsing the list
                // if the token is recognised as an element or terminator of an enclosing list.
                let error = kind.create_error(self);
                self.add_error(error, self.current_range());

                if should_recover {
                    break;
                }
                self.next_token();
            }
        }

        self.recovery_context = saved_context;
    }

    fn parse_delimited_list<T>(
        &mut self,
        kind: RecoveryContextKind,
        parse_element: impl Fn(&mut Parser<'src>) -> T,
        allow_trailing_comma: bool,
    ) -> Vec<T> {
        let mut progress = ParserProgress::default();
        let mut elements = Vec::new();

        let saved_context = self.recovery_context;
        self.recovery_context = self
            .recovery_context
            .union(RecoveryContext::from_kind(kind));

        let mut trailing_comma_range: Option<TextRange> = None;

        loop {
            progress.assert_progressing(self);

            self.current_kind();

            if kind.is_list_element(self) {
                elements.push(parse_element(self));

                let maybe_comma_range = self.current_range();
                if self.eat(TokenKind::Comma) {
                    trailing_comma_range = Some(maybe_comma_range);
                    continue;
                } else {
                    trailing_comma_range = None;
                }

                if kind.is_list_terminator(self) {
                    break;
                }

                self.expect(TokenKind::Comma);
            } else if kind.is_list_terminator(self) {
                break;
            } else {
                // Run the error recovery: This also handles the case when an element is missing between two commas: `a,,b`
                let should_recover = self.is_enclosing_list_element_or_terminator();

                // Not a recognised element. Add an error and either skip the token or break parsing the list
                // if the token is recognised as an element or terminator of an enclosing list.
                let error = kind.create_error(self);
                self.add_error(error, self.current_range());

                if should_recover {
                    break;
                }

                if self.at(TokenKind::Comma) {
                    trailing_comma_range = Some(self.current_range());
                } else {
                    trailing_comma_range = None;
                }

                self.next_token();
            }
        }

        if let Some(trailing_comma) = trailing_comma_range {
            if !allow_trailing_comma {
                self.add_error(
                    ParseErrorType::OtherError("Trailing comma not allowed".to_string()),
                    trailing_comma,
                );
            }
        }

        self.recovery_context = saved_context;

        elements
    }

    #[cold]
    fn is_enclosing_list_element_or_terminator(&self) -> bool {
        for context in self.recovery_context.kind_iter() {
            if context.is_list_terminator(self) || context.is_list_element(self) {
                return true;
            }
        }

        false
    }

    /// Parses elements enclosed within a delimiter pair, such as parentheses, brackets,
    /// or braces.
    #[deprecated(note = "Use `parse_delimited_list` instead.")]
    fn parse_delimited(
        &mut self,
        allow_trailing_delim: bool,
        opening: TokenKind,
        delim: TokenKind,
        closing: TokenKind,
        func: impl FnMut(&mut Parser<'src>),
    ) {
        self.bump(opening);

        #[allow(deprecated)]
        self.parse_separated(allow_trailing_delim, delim, [closing], func);

        self.expect(closing);
    }

    /// Parses a sequence of elements separated by a delimiter. This function stops
    /// parsing upon encountering any of the tokens in `ending_set`, if it doesn't
    /// encounter the tokens in `ending_set` it stops parsing when seeing the `EOF`
    /// or `Newline` token.
    #[deprecated(note = "Use `parse_delimited_list` instead.")]
    fn parse_separated(
        &mut self,
        allow_trailing_delim: bool,
        delim: TokenKind,
        ending_set: impl Into<TokenSet>,
        mut func: impl FnMut(&mut Parser<'src>),
    ) {
        let ending_set = NEWLINE_EOF_SET.union(ending_set.into());
        let mut progress = ParserProgress::default();

        while !self.at_ts(ending_set) {
            progress.assert_progressing(self);
            func(self);

            // exit the loop if a trailing `delim` is not allowed
            if !allow_trailing_delim && ending_set.contains(self.peek_nth(1)) {
                break;
            }

            if !self.eat(delim) {
                if self.at_expr() {
                    self.expect(delim);
                } else {
                    break;
                }
            }
        }
    }

    fn is_current_token_postfix(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Dot
        )
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SequenceMatchPatternParentheses {
    Tuple,
    List,
}

impl SequenceMatchPatternParentheses {
    fn closing_kind(self) -> TokenKind {
        match self {
            SequenceMatchPatternParentheses::Tuple => TokenKind::Rpar,
            SequenceMatchPatternParentheses::List => TokenKind::Rsqb,
        }
    }

    const fn is_list(self) -> bool {
        matches!(self, SequenceMatchPatternParentheses::List)
    }
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    struct ParserCtxFlags: u8 {
        const PARENTHESIZED_EXPR = 1 << 0;

        // NOTE: `ARGUMENTS` can be removed once the heuristic in `parse_with_items`
        // is improved.
        const ARGUMENTS = 1 << 1;
        const FOR_TARGET = 1 << 2;
    }
}

#[derive(PartialEq, Copy, Clone)]
enum FunctionKind {
    Lambda,
    FunctionDef,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
enum RecoveryContextKind {
    ModuleStatements,
    BlockStatements,

    /// The `elif` clauses of an `if` statement
    Elif,

    /// The `except` clauses of a `try` statement
    Except,

    /// When parsing a list of assignment targets
    AssignmentTargets,

    TypeParams,

    ImportNames,
}

impl RecoveryContextKind {
    fn is_list_terminator(self, p: &Parser) -> bool {
        match self {
            // The program must consume all tokens until the end
            RecoveryContextKind::ModuleStatements => false,
            RecoveryContextKind::BlockStatements => p.at(TokenKind::Dedent),

            RecoveryContextKind::Elif => p.at(TokenKind::Else),
            RecoveryContextKind::Except => {
                matches!(p.current_kind(), TokenKind::Finally | TokenKind::Else)
            }

            // TODO: Should `semi` be part of the simple statement recovery set instead?
            RecoveryContextKind::AssignmentTargets => {
                matches!(p.current_kind(), TokenKind::Newline | TokenKind::Semi)
            }

            // Tokens other than `]` are for better error recovery: For example, recover when we find the `:` of a clause header or
            // the equal of a type assignment.
            RecoveryContextKind::TypeParams => {
                matches!(
                    p.current_kind(),
                    TokenKind::Rsqb
                        | TokenKind::Newline
                        | TokenKind::Colon
                        | TokenKind::Equal
                        | TokenKind::Lpar
                )
            }
            RecoveryContextKind::ImportNames => {
                matches!(p.current_kind(), TokenKind::Rpar | TokenKind::Newline)
            }
        }
    }

    fn is_list_element(self, p: &Parser) -> bool {
        match self {
            RecoveryContextKind::ModuleStatements => p.is_at_stmt(),
            RecoveryContextKind::BlockStatements => p.is_at_stmt(),
            RecoveryContextKind::Elif => p.at(TokenKind::Elif),
            RecoveryContextKind::Except => p.at(TokenKind::Except),
            RecoveryContextKind::AssignmentTargets => p.at(TokenKind::Equal),
            RecoveryContextKind::TypeParams => p.is_at_type_param(),
            RecoveryContextKind::ImportNames => {
                matches!(p.current_kind(), TokenKind::Star | TokenKind::Name)
            }
        }
    }

    fn create_error(self, p: &Parser) -> ParseErrorType {
        match self {
            RecoveryContextKind::ModuleStatements | RecoveryContextKind::BlockStatements => {
                if p.at(TokenKind::Indent) {
                    ParseErrorType::UnexpectedIndentation
                } else {
                    ParseErrorType::OtherError("Expected a statement".to_string())
                }
            }
            RecoveryContextKind::Elif => ParseErrorType::OtherError(
                "Expected an `elif` or `else` clause, or the end of the `if` statement."
                    .to_string(),
            ),
            RecoveryContextKind::Except => ParseErrorType::OtherError(
                "An `except` or `finally` clause or the end of the `try` statement expected."
                    .to_string(),
            ),
            RecoveryContextKind::AssignmentTargets => {
                if p.current_kind().is_keyword() {
                    ParseErrorType::OtherError(
                        "The keyword is not allowed as a variable declaration name".to_string(),
                    )
                } else {
                    ParseErrorType::OtherError("Assignment target expected".to_string())
                }
            }
            RecoveryContextKind::TypeParams => ParseErrorType::OtherError(
                "Expected a type parameter or the end of the type parameter list".to_string(),
            ),
            RecoveryContextKind::ImportNames => {
                ParseErrorType::OtherError("Expected an import name or a ')'".to_string())
            }
        }
    }
}

#[derive(Copy, Clone, Default, PartialEq, Eq)]
struct RecoveryContext(u8);

bitflags! {
    impl RecoveryContext: u8 {
        const MODULE_STATEMENTS = 1 << 0;
        const BLOCK_STATEMENTS = 1 << 1;
        const ELIF = 1 << 2;
        const EXCEPT = 1 << 3;

        const ASSIGNMENT_TARGETS = 1 << 4;
        const TYPE_PARAMS = 1 << 5;

        const IMPORT_NAMES = 1 << 6;
    }
}

impl RecoveryContext {
    const fn from_kind(kind: RecoveryContextKind) -> Self {
        match kind {
            RecoveryContextKind::ModuleStatements => RecoveryContext::MODULE_STATEMENTS,
            RecoveryContextKind::BlockStatements => RecoveryContext::BLOCK_STATEMENTS,
            RecoveryContextKind::Elif => RecoveryContext::ELIF,
            RecoveryContextKind::Except => RecoveryContext::EXCEPT,
            RecoveryContextKind::AssignmentTargets => RecoveryContext::ASSIGNMENT_TARGETS,
            RecoveryContextKind::TypeParams => RecoveryContext::TYPE_PARAMS,
            RecoveryContextKind::ImportNames => RecoveryContext::IMPORT_NAMES,
        }
    }

    /// Safe conversion to the corresponding [`RecoveryContextKind`] (inverse of [`Self::from_kind`]).
    ///
    /// Returns `None` if the `RecoveryContext` is empty or has multiple flags set.
    const fn to_kind(self) -> Option<RecoveryContextKind> {
        Some(match self {
            RecoveryContext::MODULE_STATEMENTS => RecoveryContextKind::ModuleStatements,
            RecoveryContext::BLOCK_STATEMENTS => RecoveryContextKind::BlockStatements,
            RecoveryContext::ELIF => RecoveryContextKind::Elif,
            RecoveryContext::ASSIGNMENT_TARGETS => RecoveryContextKind::AssignmentTargets,
            RecoveryContext::TYPE_PARAMS => RecoveryContextKind::TypeParams,
            RecoveryContext::IMPORT_NAMES => RecoveryContextKind::ImportNames,
            _ => return None,
        })
    }

    fn kind_iter(self) -> impl Iterator<Item = RecoveryContextKind> {
        self.iter().map(|context| {
            context
                .to_kind()
                .expect("Expected context to be of a single kind.")
        })
    }
}

#[derive(Debug)]
struct SavedParserContext {
    flags: ParserCtxFlags,
    bomb: DebugDropBomb,
}
