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
    ast: ast::Mod,
    parse_errors: Vec<ParseError>,
}

impl Program {
    /// Returns the parsed AST.
    pub fn ast(&self) -> &ast::Mod {
        &self.ast
    }

    /// Returns a list of syntax errors found during parsing.
    pub fn errors(&self) -> &[ParseError] {
        &self.parse_errors
    }

    /// Consumes the `Program` and returns the parsed AST.
    pub fn into_ast(self) -> ast::Mod {
        self.ast
    }

    /// Consumes the `Program` and returns a list of syntax errors found during parsing.
    pub fn into_errors(self) -> Vec<ParseError> {
        self.parse_errors
    }

    /// Returns `true` if the program is valid i.e., it has no syntax errors.
    pub fn is_valid(&self) -> bool {
        self.parse_errors.is_empty()
    }

    pub fn parse_str(source: &str, mode: Mode) -> Program {
        let tokens = lex(source, mode);
        Self::parse_tokens(source, tokens.collect(), mode)
    }

    pub fn parse_tokens(source: &str, tokens: Vec<LexResult>, mode: Mode) -> Program {
        Parser::new(source, mode, TokenSource::new(tokens)).parse_program()
    }
}

#[derive(Debug)]
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
            let body = self.parse_list_into_vec(
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
            self.current_token_kind(),
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
        self.current_token_range().start()
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
            self.current_token_kind()
        } else {
            self.tokens
                .peek_nth(offset - 1)
                .map_or(TokenKind::EndOfFile, |spanned| spanned.0)
        }
    }

    /// Returns the current token kind along with its range.
    ///
    /// Use `current_token_kind` or `current_token_range` to only get the kind or range
    /// respectively.
    #[inline]
    fn current_token(&self) -> (TokenKind, TextRange) {
        (self.current_token_kind(), self.current_token_range())
    }

    /// Returns the current token kind.
    #[inline]
    fn current_token_kind(&self) -> TokenKind {
        // TODO: Converting the token kind over and over again can be expensive.
        TokenKind::from_token(&self.current.0)
    }

    /// Returns the range of the current token.
    #[inline]
    fn current_token_range(&self) -> TextRange {
        self.current.1
    }

    /// Eat the current token if it is of the given kind, returning `true` in
    /// that case. Otherwise, return `false`.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.next_token();
            true
        } else {
            false
        }
    }

    /// Bumps the current token assuming it is of the given kind.
    ///
    /// # Panics
    ///
    /// If the current token is not of the given kind.
    ///
    /// # Returns
    ///
    /// The current token
    fn bump(&mut self, kind: TokenKind) -> (Tok, TextRange) {
        assert_eq!(self.current_token_kind(), kind);

        self.next_token()
    }

    /// Bumps the current token assuming it is found in the given token set.
    ///
    /// # Panics
    ///
    /// If the current token is not found in the given token set.
    ///
    /// # Returns
    ///
    /// The current token.
    fn bump_ts(&mut self, ts: TokenSet) -> (Tok, TextRange) {
        assert!(ts.contains(self.current_token_kind()));

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

    /// Returns `true` if the current token is of the given kind.
    fn at(&self, kind: TokenKind) -> bool {
        self.current_token_kind() == kind
    }

    /// Returns `true` if the current token is found in the given token set.
    fn at_ts(&self, ts: TokenSet) -> bool {
        ts.contains(self.current_token_kind())
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

    /// Parses a list of elements into a vector where each element is parsed using
    /// the given `parse_element` function.
    fn parse_list_into_vec<T>(
        &mut self,
        recovery_context_kind: RecoveryContextKind,
        parse_element: impl Fn(&mut Parser<'src>) -> T,
    ) -> Vec<T> {
        let mut elements = Vec::new();
        self.parse_list(recovery_context_kind, |p| elements.push(parse_element(p)));
        elements
    }

    /// Parses a list of elements where each element is parsed using the given
    /// `parse_element` function.
    ///
    /// The difference between this function and `parse_list_into_vec` is that
    /// this function does not return the parsed elements. Instead, it is the
    /// caller's responsibility to handle the parsed elements. This is the reason
    /// that the `parse_element` parameter is bound to [`FnMut`] instead of [`Fn`].
    fn parse_list(
        &mut self,
        recovery_context_kind: RecoveryContextKind,
        mut parse_element: impl FnMut(&mut Parser<'src>),
    ) {
        let mut progress = ParserProgress::default();

        let saved_context = self.recovery_context;
        self.recovery_context = self
            .recovery_context
            .union(RecoveryContext::from_kind(recovery_context_kind));

        loop {
            progress.assert_progressing(self);

            // The end of file marker ends all lists.
            if self.at(TokenKind::EndOfFile) {
                break;
            }

            if recovery_context_kind.is_list_element(self) {
                parse_element(self);
            } else if recovery_context_kind.is_list_terminator(self) {
                break;
            } else {
                // Not a recognised element. Add an error and either skip the token or break parsing the list
                // if the token is recognised as an element or terminator of an enclosing list.
                let error = recovery_context_kind.create_error(self);
                self.add_error(error, self.current_token_range());

                // Run the error recovery: This also handles the case when an element is missing between two commas: `a,,b`
                if self.is_enclosing_list_element_or_terminator() {
                    break;
                }

                self.next_token();
            }
        }

        self.recovery_context = saved_context;
    }

    /// Parses a comma separated list of elements into a vector where each element
    /// is parsed using the given `parse_element` function.
    fn parse_comma_separated_list_into_vec<T>(
        &mut self,
        recovery_context_kind: RecoveryContextKind,
        parse_element: impl Fn(&mut Parser<'src>) -> T,
        allow_trailing_comma: bool,
    ) -> Vec<T> {
        let mut elements = Vec::new();
        self.parse_comma_separated_list(
            recovery_context_kind,
            |p| elements.push(parse_element(p)),
            allow_trailing_comma,
        );
        elements
    }

    /// Parses a comma separated list of elements where each element is parsed
    /// sing the given `parse_element` function.
    ///
    /// The difference between this function and `parse_comma_separated_list_into_vec`
    /// is that this function does not return the parsed elements. Instead, it is the
    /// caller's responsibility to handle the parsed elements. This is the reason
    /// that the `parse_element` parameter is bound to [`FnMut`] instead of [`Fn`].
    ///
    /// If `allow_trailing_comma` is `true`, the function will allow a trailing
    /// comma at the end of the list, otherwise it will add an error.
    fn parse_comma_separated_list(
        &mut self,
        recovery_context_kind: RecoveryContextKind,
        mut parse_element: impl FnMut(&mut Parser<'src>),
        allow_trailing_comma: bool,
    ) {
        let mut progress = ParserProgress::default();

        let saved_context = self.recovery_context;
        self.recovery_context = self
            .recovery_context
            .union(RecoveryContext::from_kind(recovery_context_kind));

        let mut trailing_comma_range: Option<TextRange> = None;

        loop {
            progress.assert_progressing(self);

            // The end of file marker ends all lists.
            if self.at(TokenKind::EndOfFile) {
                break;
            }

            if recovery_context_kind.is_list_element(self) {
                parse_element(self);

                let maybe_comma_range = self.current_token_range();
                if self.eat(TokenKind::Comma) {
                    trailing_comma_range = Some(maybe_comma_range);
                    continue;
                }
                trailing_comma_range = None;

                if recovery_context_kind.is_list_terminator(self) {
                    break;
                }

                self.expect(TokenKind::Comma);
            } else if recovery_context_kind.is_list_terminator(self) {
                break;
            } else {
                // Not a recognised element. Add an error and either skip the token or break parsing the list
                // if the token is recognised as an element or terminator of an enclosing list.
                let error = recovery_context_kind.create_error(self);
                self.add_error(error, self.current_token_range());

                // Run the error recovery: This also handles the case when an element is missing between two commas: `a,,b`
                if self.is_enclosing_list_element_or_terminator() {
                    break;
                }

                if self.at(TokenKind::Comma) {
                    trailing_comma_range = Some(self.current_token_range());
                } else {
                    trailing_comma_range = None;
                }

                self.next_token();
            }
        }

        if let Some(trailing_comma_range) = trailing_comma_range {
            if !allow_trailing_comma {
                self.add_error(
                    ParseErrorType::OtherError("Trailing comma not allowed".to_string()),
                    trailing_comma_range,
                );
            }
        }

        self.recovery_context = saved_context;
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

    fn is_current_token_postfix(&self) -> bool {
        matches!(
            self.current_token_kind(),
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
    /// Returns the token kind that closes the parentheses.
    const fn closing_kind(self) -> TokenKind {
        match self {
            SequenceMatchPatternParentheses::Tuple => TokenKind::Rpar,
            SequenceMatchPatternParentheses::List => TokenKind::Rsqb,
        }
    }

    /// Returns `true` if the parentheses are for a list pattern e.g., `case [a, b]: ...`.
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

#[derive(Debug, PartialEq, Copy, Clone)]
enum FunctionKind {
    /// A lambda expression, e.g., `lambda x: x`
    Lambda,
    /// A function definition, e.g., `def f(x): ...`
    FunctionDef,
}

impl FunctionKind {
    /// Returns the token that terminates a list of parameters.
    const fn list_terminator(self) -> TokenKind {
        match self {
            FunctionKind::Lambda => TokenKind::Colon,
            FunctionKind::FunctionDef => TokenKind::Rpar,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum WithItemKind {
    /// The `with` item is parenthesized, e.g., `with (a, b): ...`.
    Parenthesized,
    /// The `with` item has a parenthesized expression, e.g., `with (a) as b: ...`.
    ParenthesizedExpression,
    /// The `with` item isn't parenthesized in any way, e.g., `with a as b: ...`.
    Unparenthesized,
}

impl WithItemKind {
    /// Returns the token that terminates a list of `with` items.
    const fn list_terminator(self) -> TokenKind {
        match self {
            WithItemKind::Parenthesized => TokenKind::Rpar,
            WithItemKind::Unparenthesized | WithItemKind::ParenthesizedExpression => {
                TokenKind::Colon
            }
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum TupleParenthesized {
    /// The tuple is parenthesized, e.g., `(a, b)`.
    Yes,
    /// The tuple is not parenthesized, e.g., `a, b`.
    No,
}

impl TupleParenthesized {
    /// Returns `true` if the tuple is parenthesized.
    const fn is_yes(self) -> bool {
        matches!(self, TupleParenthesized::Yes)
    }
}

#[derive(Copy, Clone, Debug)]
enum RecoveryContextKind {
    /// When parsing a list of statements at the module level i.e., at the top level of a file.
    ModuleStatements,

    /// When parsing a list of statements in a block e.g., the body of a function or a class.
    BlockStatements,

    /// The `elif` clauses of an `if` statement
    Elif,

    /// The `except` clauses of a `try` statement
    Except,

    /// When parsing a list of assignment targets
    AssignmentTargets,

    /// When parsing a list of type parameters
    TypeParams,

    /// When parsing a list of names in a `from ... import ...` statement
    ImportFromAsNames,

    /// When parsing a list of names in an `import` statement
    ImportNames,

    /// When parsing a list of slice elements e.g., `data[1, 2]`.
    ///
    /// This is different from `ListElements` as the surrounding context is
    /// different in that the list is part of a subscript expression.
    Slices,

    /// When parsing a list of elements in a list expression e.g., `[1, 2]`
    ListElements,

    /// When parsing a list of elements in a set expression e.g., `{1, 2}`
    SetElements,

    /// When parsing a list of elements in a dictionary expression e.g., `{1: "a", **data}`
    DictElements,

    /// When parsing a list of elements in a tuple expression e.g., `(1, 2)`
    TupleElements(TupleParenthesized),

    /// When parsing a list of patterns in a match statement with an optional
    /// parentheses, e.g., `case a, b: ...`, `case (a, b): ...`, `case [a, b]: ...`
    SequenceMatchPattern(Option<SequenceMatchPatternParentheses>),

    /// When parsing a mapping pattern in a match statement
    MatchPatternMapping,

    /// When parsing a list of arguments in a class pattern for the match statement
    MatchPatternClassArguments,

    /// When parsing a list of arguments in a function call or a class definition
    Arguments,

    /// When parsing a `del` statement
    DeleteTargets,

    /// When parsing a list of identifiers
    Identifiers,

    /// When parsing a list of parameters in a function definition which can be
    /// either a function definition or a lambda expression.
    Parameters(FunctionKind),

    /// When parsing a list of items in a `with` statement
    WithItems(WithItemKind),

    FStringElements,
}

impl RecoveryContextKind {
    fn is_list_terminator(self, p: &Parser) -> bool {
        match self {
            // The program must consume all tokens until the end
            RecoveryContextKind::ModuleStatements => false,
            RecoveryContextKind::BlockStatements => p.at(TokenKind::Dedent),

            RecoveryContextKind::Elif => p.at(TokenKind::Else),
            RecoveryContextKind::Except => {
                matches!(p.current_token_kind(), TokenKind::Finally | TokenKind::Else)
            }

            // TODO: Should `semi` be part of the simple statement recovery set instead?
            RecoveryContextKind::AssignmentTargets => {
                matches!(p.current_token_kind(), TokenKind::Newline | TokenKind::Semi)
            }

            // Tokens other than `]` are for better error recovery. For example, recover when we
            // find the `:` of a clause header or the equal of a type assignment.
            RecoveryContextKind::TypeParams => {
                matches!(
                    p.current_token_kind(),
                    TokenKind::Rsqb
                        | TokenKind::Newline
                        | TokenKind::Colon
                        | TokenKind::Equal
                        | TokenKind::Lpar
                )
            }
            // The names of an import statement cannot be parenthesized, so it
            // always ends with a newline.
            RecoveryContextKind::ImportNames => p.at(TokenKind::Newline),
            RecoveryContextKind::ImportFromAsNames => {
                matches!(
                    p.current_token_kind(),
                    // `from a import (b, c)`
                    TokenKind::Rpar
                    // `from a import b, c`
                    | TokenKind::Newline
                )
            }
            // The elements in a container expression cannot end with a newline
            // as all of them are actually non-logical newlines.
            RecoveryContextKind::Slices | RecoveryContextKind::ListElements => {
                p.at(TokenKind::Rsqb)
            }
            RecoveryContextKind::SetElements | RecoveryContextKind::DictElements => {
                p.at(TokenKind::Rbrace)
            }
            RecoveryContextKind::TupleElements(parenthesized) => {
                if parenthesized.is_yes() {
                    // TODO(dhruvmanila): Confirm if this is ok
                    p.at(TokenKind::Rpar)
                } else {
                    p.at_sequence_end()
                }
            }
            RecoveryContextKind::SequenceMatchPattern(parentheses) => p.at(parentheses.map_or(
                TokenKind::Colon,
                SequenceMatchPatternParentheses::closing_kind,
            )),
            RecoveryContextKind::MatchPatternMapping => p.at(TokenKind::Rbrace),
            RecoveryContextKind::MatchPatternClassArguments => p.at(TokenKind::Rpar),
            RecoveryContextKind::Arguments => p.at(TokenKind::Rpar),
            RecoveryContextKind::DeleteTargets | RecoveryContextKind::Identifiers => {
                p.at(TokenKind::Newline)
            }
            RecoveryContextKind::Parameters(function_kind) => {
                // `lambda x, y: ...` or `def f(x, y): ...`
                p.at(function_kind.list_terminator())
                    // To recover from missing closing parentheses
                    || p.at(TokenKind::Rarrow)
                    || p.at_compound_stmt()
            }
            RecoveryContextKind::WithItems(with_item_kind) => {
                p.at(with_item_kind.list_terminator())
            }
            RecoveryContextKind::FStringElements => {
                p.at_ts(TokenSet::new([
                    TokenKind::FStringEnd,
                    // For better error recovery
                    //
                    // ```python
                    // # List terminates with a newline
                    // f"hello
                    //
                    // x = 1
                    // ```
                    TokenKind::Newline,
                    // ```python
                    // # List terminates with a `}`
                    // f"hello {x}
                    //
                    // x = 1
                    // ```
                    TokenKind::Rbrace,
                ]))
            }
        }
    }

    fn is_list_element(self, p: &Parser) -> bool {
        match self {
            RecoveryContextKind::ModuleStatements => p.at_stmt(),
            RecoveryContextKind::BlockStatements => p.at_stmt(),
            RecoveryContextKind::Elif => p.at(TokenKind::Elif),
            RecoveryContextKind::Except => p.at(TokenKind::Except),
            RecoveryContextKind::AssignmentTargets => p.at(TokenKind::Equal),
            RecoveryContextKind::TypeParams => p.at_type_param(),
            RecoveryContextKind::ImportNames => p.at(TokenKind::Name),
            RecoveryContextKind::ImportFromAsNames => {
                matches!(p.current_token_kind(), TokenKind::Star | TokenKind::Name)
            }
            RecoveryContextKind::Slices => p.at(TokenKind::Colon) || p.at_expr(),
            RecoveryContextKind::ListElements
            | RecoveryContextKind::SetElements
            | RecoveryContextKind::TupleElements(_) => p.at_expr(),
            RecoveryContextKind::DictElements => p.at(TokenKind::DoubleStar) || p.at_expr(),
            RecoveryContextKind::SequenceMatchPattern(_) => p.at_pattern_start(),
            RecoveryContextKind::MatchPatternMapping => p.at_mapping_pattern_start(),
            RecoveryContextKind::MatchPatternClassArguments => p.at_pattern_start(),
            RecoveryContextKind::Arguments => p.at_expr(),
            RecoveryContextKind::DeleteTargets => p.at_expr(),
            RecoveryContextKind::Identifiers => p.at(TokenKind::Name),
            RecoveryContextKind::Parameters(_) => matches!(
                p.current_token_kind(),
                TokenKind::Name | TokenKind::Star | TokenKind::DoubleStar | TokenKind::Slash
            ),
            RecoveryContextKind::WithItems(_) => p.at_expr(),
            RecoveryContextKind::FStringElements => matches!(
                p.current_token_kind(),
                // Literal element
                TokenKind::FStringMiddle
                // Expression element
                | TokenKind::Lbrace
            ),
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
                "Expected an `except` or `finally` clause or the end of the `try` statement."
                    .to_string(),
            ),
            RecoveryContextKind::AssignmentTargets => {
                if p.current_token_kind().is_keyword() {
                    ParseErrorType::OtherError(
                        "The keyword is not allowed as a variable declaration name".to_string(),
                    )
                } else {
                    ParseErrorType::OtherError("Expected an assignment target".to_string())
                }
            }
            RecoveryContextKind::TypeParams => ParseErrorType::OtherError(
                "Expected a type parameter or the end of the type parameter list".to_string(),
            ),
            RecoveryContextKind::ImportFromAsNames => {
                ParseErrorType::OtherError("Expected an import name or a ')'".to_string())
            }
            RecoveryContextKind::ImportNames => {
                ParseErrorType::OtherError("Expected an import name".to_string())
            }
            RecoveryContextKind::Slices => ParseErrorType::OtherError(
                "Expected an expression or the end of the slice list".to_string(),
            ),
            RecoveryContextKind::ListElements => {
                ParseErrorType::OtherError("Expected an expression or a ']'".to_string())
            }
            RecoveryContextKind::SetElements | RecoveryContextKind::DictElements => {
                ParseErrorType::OtherError("Expected an expression or a '}'".to_string())
            }
            RecoveryContextKind::TupleElements(parenthesized) => {
                if parenthesized.is_yes() {
                    ParseErrorType::OtherError("Expected an expression or a ')'".to_string())
                } else {
                    ParseErrorType::OtherError("Expected an expression".to_string())
                }
            }
            RecoveryContextKind::SequenceMatchPattern(_) => ParseErrorType::OtherError(
                "Expected a pattern or the end of the sequence pattern".to_string(),
            ),
            RecoveryContextKind::MatchPatternMapping => ParseErrorType::OtherError(
                "Expected a mapping pattern or the end of the mapping pattern".to_string(),
            ),
            RecoveryContextKind::MatchPatternClassArguments => {
                ParseErrorType::OtherError("Expected a pattern or a ')'".to_string())
            }
            RecoveryContextKind::Arguments => {
                ParseErrorType::OtherError("Expected an expression or a ')'".to_string())
            }
            RecoveryContextKind::DeleteTargets => {
                ParseErrorType::OtherError("Expected a delete target".to_string())
            }
            RecoveryContextKind::Identifiers => {
                ParseErrorType::OtherError("Expected an identifier".to_string())
            }
            RecoveryContextKind::Parameters(_) => ParseErrorType::OtherError(
                "Expected a parameter or the end of the parameter list".to_string(),
            ),
            RecoveryContextKind::WithItems(with_item_kind) => match with_item_kind {
                WithItemKind::Parenthesized => {
                    ParseErrorType::OtherError("Expected an expression or a ')'".to_string())
                }
                _ => ParseErrorType::OtherError(
                    "Expected an expression or the end of the with item list".to_string(),
                ),
            },
            RecoveryContextKind::FStringElements => ParseErrorType::OtherError(
                "Expected an f-string element or the end of the f-string".to_string(),
            ),
        }
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
struct RecoveryContext(u32);

bitflags! {
    impl RecoveryContext: u32 {
        const MODULE_STATEMENTS = 1 << 0;
        const BLOCK_STATEMENTS = 1 << 1;
        const ELIF = 1 << 2;
        const EXCEPT = 1 << 3;
        const ASSIGNMENT_TARGETS = 1 << 4;
        const TYPE_PARAMS = 1 << 5;
        const IMPORT_FROM_AS_NAMES = 1 << 6;
        const IMPORT_NAMES = 1 << 7;
        const SLICES = 1 << 8;
        const LIST_ELEMENTS = 1 << 9;
        const SET_ELEMENTS = 1 << 10;
        const DICT_ELEMENTS = 1 << 11;
        const TUPLE_ELEMENTS_PARENTHESIZED = 1 << 12;
        const TUPLE_ELEMENTS_UNPARENTHESIZED = 1 << 13;
        const SEQUENCE_MATCH_PATTERN = 1 << 14;
        const SEQUENCE_MATCH_PATTERN_LIST = 1 << 15;
        const SEQUENCE_MATCH_PATTERN_TUPLE = 1 << 16;
        const MATCH_PATTERN_MAPPING = 1 << 17;
        const MATCH_PATTERN_CLASS_ARGUMENTS = 1 << 18;
        const ARGUMENTS = 1 << 19;
        const DELETE = 1 << 20;
        const IDENTIFIERS = 1 << 21;
        const FUNCTION_PARAMETERS = 1 << 22;
        const LAMBDA_PARAMETERS = 1 << 23;
        const WITH_ITEMS_PARENTHESIZED = 1 << 24;
        const WITH_ITEMS_PARENTHESIZED_EXPRESSION = 1 << 25;
        const WITH_ITEMS_UNPARENTHESIZED = 1 << 26;
        const F_STRING_ELEMENTS = 1 << 27;
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
            RecoveryContextKind::ImportFromAsNames => RecoveryContext::IMPORT_FROM_AS_NAMES,
            RecoveryContextKind::ImportNames => RecoveryContext::IMPORT_NAMES,
            RecoveryContextKind::Slices => RecoveryContext::SLICES,
            RecoveryContextKind::ListElements => RecoveryContext::LIST_ELEMENTS,
            RecoveryContextKind::SetElements => RecoveryContext::SET_ELEMENTS,
            RecoveryContextKind::DictElements => RecoveryContext::DICT_ELEMENTS,
            RecoveryContextKind::TupleElements(parenthesized) => match parenthesized {
                TupleParenthesized::Yes => RecoveryContext::TUPLE_ELEMENTS_PARENTHESIZED,
                TupleParenthesized::No => RecoveryContext::TUPLE_ELEMENTS_UNPARENTHESIZED,
            },
            RecoveryContextKind::SequenceMatchPattern(parentheses) => match parentheses {
                None => RecoveryContext::SEQUENCE_MATCH_PATTERN,
                Some(SequenceMatchPatternParentheses::List) => {
                    RecoveryContext::SEQUENCE_MATCH_PATTERN_LIST
                }
                Some(SequenceMatchPatternParentheses::Tuple) => {
                    RecoveryContext::SEQUENCE_MATCH_PATTERN_TUPLE
                }
            },
            RecoveryContextKind::MatchPatternMapping => RecoveryContext::MATCH_PATTERN_MAPPING,
            RecoveryContextKind::MatchPatternClassArguments => {
                RecoveryContext::MATCH_PATTERN_CLASS_ARGUMENTS
            }
            RecoveryContextKind::Arguments => RecoveryContext::ARGUMENTS,
            RecoveryContextKind::DeleteTargets => RecoveryContext::DELETE,
            RecoveryContextKind::Identifiers => RecoveryContext::IDENTIFIERS,
            RecoveryContextKind::Parameters(function_kind) => match function_kind {
                FunctionKind::Lambda => RecoveryContext::LAMBDA_PARAMETERS,
                FunctionKind::FunctionDef => RecoveryContext::FUNCTION_PARAMETERS,
            },
            RecoveryContextKind::WithItems(with_item_kind) => match with_item_kind {
                WithItemKind::Parenthesized => RecoveryContext::WITH_ITEMS_PARENTHESIZED,
                WithItemKind::ParenthesizedExpression => {
                    RecoveryContext::WITH_ITEMS_PARENTHESIZED_EXPRESSION
                }
                WithItemKind::Unparenthesized => RecoveryContext::WITH_ITEMS_UNPARENTHESIZED,
            },
            RecoveryContextKind::FStringElements => RecoveryContext::F_STRING_ELEMENTS,
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
            RecoveryContext::EXCEPT => RecoveryContextKind::Except,
            RecoveryContext::ASSIGNMENT_TARGETS => RecoveryContextKind::AssignmentTargets,
            RecoveryContext::TYPE_PARAMS => RecoveryContextKind::TypeParams,
            RecoveryContext::IMPORT_FROM_AS_NAMES => RecoveryContextKind::ImportFromAsNames,
            RecoveryContext::IMPORT_NAMES => RecoveryContextKind::ImportNames,
            RecoveryContext::SLICES => RecoveryContextKind::Slices,
            RecoveryContext::LIST_ELEMENTS => RecoveryContextKind::ListElements,
            RecoveryContext::SET_ELEMENTS => RecoveryContextKind::SetElements,
            RecoveryContext::DICT_ELEMENTS => RecoveryContextKind::DictElements,
            RecoveryContext::TUPLE_ELEMENTS_PARENTHESIZED => {
                RecoveryContextKind::TupleElements(TupleParenthesized::Yes)
            }
            RecoveryContext::TUPLE_ELEMENTS_UNPARENTHESIZED => {
                RecoveryContextKind::TupleElements(TupleParenthesized::No)
            }
            RecoveryContext::SEQUENCE_MATCH_PATTERN => {
                RecoveryContextKind::SequenceMatchPattern(None)
            }
            RecoveryContext::SEQUENCE_MATCH_PATTERN_LIST => {
                RecoveryContextKind::SequenceMatchPattern(Some(
                    SequenceMatchPatternParentheses::List,
                ))
            }
            RecoveryContext::SEQUENCE_MATCH_PATTERN_TUPLE => {
                RecoveryContextKind::SequenceMatchPattern(Some(
                    SequenceMatchPatternParentheses::Tuple,
                ))
            }
            RecoveryContext::MATCH_PATTERN_MAPPING => RecoveryContextKind::MatchPatternMapping,
            RecoveryContext::MATCH_PATTERN_CLASS_ARGUMENTS => {
                RecoveryContextKind::MatchPatternClassArguments
            }
            RecoveryContext::ARGUMENTS => RecoveryContextKind::Arguments,
            RecoveryContext::DELETE => RecoveryContextKind::DeleteTargets,
            RecoveryContext::IDENTIFIERS => RecoveryContextKind::Identifiers,
            RecoveryContext::FUNCTION_PARAMETERS => {
                RecoveryContextKind::Parameters(FunctionKind::FunctionDef)
            }
            RecoveryContext::LAMBDA_PARAMETERS => {
                RecoveryContextKind::Parameters(FunctionKind::Lambda)
            }
            RecoveryContext::WITH_ITEMS_PARENTHESIZED => {
                RecoveryContextKind::WithItems(WithItemKind::Parenthesized)
            }
            RecoveryContext::WITH_ITEMS_PARENTHESIZED_EXPRESSION => {
                RecoveryContextKind::WithItems(WithItemKind::ParenthesizedExpression)
            }
            RecoveryContext::WITH_ITEMS_UNPARENTHESIZED => {
                RecoveryContextKind::WithItems(WithItemKind::Unparenthesized)
            }
            RecoveryContext::F_STRING_ELEMENTS => RecoveryContextKind::FStringElements,
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
