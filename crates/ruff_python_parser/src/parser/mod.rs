use std::cmp::Ordering;

use bitflags::bitflags;

use ruff_python_ast::{Mod, ModExpression, ModModule};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::parser::expression::ExpressionContext;
use crate::parser::progress::{ParserProgress, TokenId};
use crate::token::TokenValue;
use crate::token_set::TokenSet;
use crate::token_source::{TokenSource, TokenSourceCheckpoint};
use crate::{Mode, ParseError, ParseErrorType, TokenKind};
use crate::{Parsed, Tokens};

mod expression;
mod helpers;
mod pattern;
mod progress;
mod recovery;
mod statement;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct Parser<'src> {
    source: &'src str,

    /// Token source for the parser that skips over any non-trivia token.
    tokens: TokenSource<'src>,

    /// Stores all the syntax errors found during the parsing.
    errors: Vec<ParseError>,

    /// Specify the mode in which the code will be parsed.
    mode: Mode,

    /// The ID of the current token. This is used to track the progress of the parser
    /// to avoid infinite loops when the parser is stuck.
    current_token_id: TokenId,

    /// The end of the previous token processed. This is used to determine a node's end.
    prev_token_end: TextSize,

    /// The recovery context in which the parser is currently in.
    recovery_context: RecoveryContext,

    /// The start offset in the source code from which to start parsing at.
    start_offset: TextSize,
}

impl<'src> Parser<'src> {
    /// Create a new parser for the given source code.
    pub(crate) fn new(source: &'src str, mode: Mode) -> Self {
        Parser::new_starts_at(source, mode, TextSize::new(0))
    }

    /// Create a new parser for the given source code which starts parsing at the given offset.
    pub(crate) fn new_starts_at(source: &'src str, mode: Mode, start_offset: TextSize) -> Self {
        let tokens = TokenSource::from_source(source, mode, start_offset);

        Parser {
            mode,
            source,
            errors: Vec::new(),
            tokens,
            recovery_context: RecoveryContext::empty(),
            prev_token_end: TextSize::new(0),
            start_offset,
            current_token_id: TokenId::default(),
        }
    }

    /// Consumes the [`Parser`] and returns the parsed [`Parsed`].
    pub(crate) fn parse(mut self) -> Parsed<Mod> {
        let syntax = match self.mode {
            Mode::Expression => Mod::Expression(self.parse_single_expression()),
            Mode::Module | Mode::Ipython => Mod::Module(self.parse_module()),
        };

        self.finish(syntax)
    }

    /// Parses a single expression.
    ///
    /// This is to be used for [`Mode::Expression`].
    ///
    /// ## Recovery
    ///
    /// After parsing a single expression, an error is reported and all remaining tokens are
    /// dropped by the parser.
    fn parse_single_expression(&mut self) -> ModExpression {
        let start = self.node_start();
        let parsed_expr = self.parse_expression_list(ExpressionContext::default());

        // All remaining newlines are actually going to be non-logical newlines.
        self.eat(TokenKind::Newline);

        if !self.at(TokenKind::EndOfFile) {
            self.add_error(
                ParseErrorType::UnexpectedExpressionToken,
                self.current_token_range(),
            );

            // TODO(dhruvmanila): How should error recovery work here? Just truncate after the expression?
            let mut progress = ParserProgress::default();
            loop {
                progress.assert_progressing(self);
                if self.at(TokenKind::EndOfFile) {
                    break;
                }
                self.bump_any();
            }
        }

        self.bump(TokenKind::EndOfFile);

        ModExpression {
            body: Box::new(parsed_expr.expr),
            range: self.node_range(start),
        }
    }

    /// Parses a Python module.
    ///
    /// This is to be used for [`Mode::Module`] and [`Mode::Ipython`].
    fn parse_module(&mut self) -> ModModule {
        let body = self.parse_list_into_vec(
            RecoveryContextKind::ModuleStatements,
            Parser::parse_statement,
        );

        self.bump(TokenKind::EndOfFile);

        ModModule {
            body,
            range: TextRange::new(self.start_offset, self.current_token_range().end()),
        }
    }

    fn finish(self, syntax: Mod) -> Parsed<Mod> {
        assert_eq!(
            self.current_token_kind(),
            TokenKind::EndOfFile,
            "Parser should be at the end of the file."
        );

        // TODO consider re-integrating lexical error handling into the parser?
        let parse_errors = self.errors;
        let (tokens, lex_errors) = self.tokens.finish();

        // Fast path for when there are no lex errors.
        // There's no fast path for when there are no parse errors because a lex error
        // always results in a parse error.
        if lex_errors.is_empty() {
            return Parsed {
                syntax,
                tokens: Tokens::new(tokens),
                errors: parse_errors,
            };
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

        Parsed {
            syntax,
            tokens: Tokens::new(tokens),
            errors: merged,
        }
    }

    /// Returns the start position for a node that starts at the current token.
    fn node_start(&self) -> TextSize {
        self.current_token_range().start()
    }

    fn node_range(&self, start: TextSize) -> TextRange {
        // It's possible during error recovery that the parsing didn't consume any tokens. In that
        // case, `last_token_end` still points to the end of the previous token but `start` is the
        // start of the current token. Calling `TextRange::new(start, self.last_token_end)` would
        // panic in that case because `start > end`. This path "detects" this case and creates an
        // empty range instead.
        //
        // The reason it's `<=` instead of just `==` is because there could be whitespaces between
        // the two tokens. For example:
        //
        // ```python
        // #     last token end
        // #     | current token (newline) start
        // #     v v
        // def foo \n
        // #      ^
        // #      assume there's trailing whitespace here
        // ```
        //
        // Or, there could tokens that are considered "trivia" and thus aren't emitted by the token
        // source. These are comments and non-logical newlines. For example:
        //
        // ```python
        // #     last token end
        // #     v
        // def foo # comment\n
        // #                ^ current token (newline) start
        // ```
        //
        // In either of the above cases, there's a "gap" between the end of the last token and start
        // of the current token.
        if self.prev_token_end <= start {
            // We need to create an empty range at the last token end instead of the start because
            // otherwise this node range will fall outside the range of it's parent node. Taking
            // the above example:
            //
            // ```python
            // if True:
            // #   function start
            // #   |     function end
            // #   v     v
            //     def foo # comment
            // #                    ^ current token start
            // ```
            //
            // Here, the current token start is the start of parameter range but the function ends
            // at `foo`. Even if there's a function body, the range of parameters would still be
            // before the comment.

            // test_err node_range_with_gaps
            // def foo # comment
            // def bar(): ...
            // def baz
            TextRange::empty(self.prev_token_end)
        } else {
            TextRange::new(start, self.prev_token_end)
        }
    }

    fn missing_node_range(&self) -> TextRange {
        // TODO(dhruvmanila): This range depends on whether the missing node is
        // on the leftmost or the rightmost of the expression. It's incorrect for
        // the leftmost missing node because the range is outside the expression
        // range. For example,
        //
        // ```python
        // value = ** y
        // #       ^^^^ expression range
        // #      ^ last token end
        // ```
        TextRange::empty(self.prev_token_end)
    }

    /// Moves the parser to the next token.
    fn do_bump(&mut self, kind: TokenKind) {
        if !matches!(
            self.current_token_kind(),
            // TODO explore including everything up to the dedent as part of the body.
            TokenKind::Dedent
            // Don't include newlines in the body
            | TokenKind::Newline
            // TODO(micha): Including the semi feels more correct but it isn't compatible with lalrpop and breaks the
            // formatters semicolon detection. Exclude it for now
            | TokenKind::Semi
        ) {
            self.prev_token_end = self.current_token_range().end();
        }

        self.tokens.bump(kind);
        self.current_token_id.increment();
    }

    /// Returns the next token kind without consuming it.
    fn peek(&mut self) -> TokenKind {
        self.tokens.peek()
    }

    /// Returns the next two token kinds without consuming it.
    fn peek2(&mut self) -> (TokenKind, TokenKind) {
        self.tokens.peek2()
    }

    /// Returns the current token kind.
    #[inline]
    fn current_token_kind(&self) -> TokenKind {
        self.tokens.current_kind()
    }

    /// Returns the range of the current token.
    #[inline]
    fn current_token_range(&self) -> TextRange {
        self.tokens.current_range()
    }

    /// Returns the current token ID.
    #[inline]
    fn current_token_id(&self) -> TokenId {
        self.current_token_id
    }

    /// Bumps the current token assuming it is of the given kind.
    ///
    /// # Panics
    ///
    /// If the current token is not of the given kind.
    fn bump(&mut self, kind: TokenKind) {
        assert_eq!(self.current_token_kind(), kind);

        self.do_bump(kind);
    }

    /// Take the token value from the underlying token source and bump the current token.
    ///
    /// # Panics
    ///
    /// If the current token is not of the given kind.
    fn bump_value(&mut self, kind: TokenKind) -> TokenValue {
        let value = self.tokens.take_value();
        self.bump(kind);
        value
    }

    /// Bumps the current token assuming it is found in the given token set.
    ///
    /// # Panics
    ///
    /// If the current token is not found in the given token set.
    fn bump_ts(&mut self, ts: TokenSet) {
        let kind = self.current_token_kind();
        assert!(ts.contains(kind));

        self.do_bump(kind);
    }

    /// Bumps the current token regardless of its kind and advances to the next token.
    ///
    /// # Panics
    ///
    /// If the parser is at end of file.
    fn bump_any(&mut self) {
        let kind = self.current_token_kind();
        assert_ne!(kind, TokenKind::EndOfFile);

        self.do_bump(kind);
    }

    /// Bumps the soft keyword token as a `Name` token.
    ///
    /// # Panics
    ///
    /// If the current token is not a soft keyword.
    pub(crate) fn bump_soft_keyword_as_name(&mut self) {
        assert!(self.at_soft_keyword());

        self.do_bump(TokenKind::Name);
    }

    /// Consume the current token if it is of the given kind. Returns `true` if it matches, `false`
    /// otherwise.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.do_bump(kind);
            true
        } else {
            false
        }
    }

    /// Eat the current token if its of the expected kind, otherwise adds an appropriate error.
    fn expect(&mut self, expected: TokenKind) -> bool {
        if self.eat(expected) {
            return true;
        }

        self.add_error(
            ParseErrorType::ExpectedToken {
                found: self.current_token_kind(),
                expected,
            },
            self.current_token_range(),
        );

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
        &self.source[ranged.range()]
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

            if recovery_context_kind.is_list_element(self) {
                parse_element(self);
            } else if recovery_context_kind.is_regular_list_terminator(self) {
                break;
            } else {
                // Run the error recovery: If the token is recognised as an element or terminator
                // of an enclosing list, then we try to re-lex in the context of a logical line and
                // break out of list parsing.
                if self.is_enclosing_list_element_or_terminator() {
                    self.tokens.re_lex_logical_token();
                    break;
                }

                self.add_error(
                    recovery_context_kind.create_error(self),
                    self.current_token_range(),
                );

                self.bump_any();
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
    ) -> Vec<T> {
        let mut elements = Vec::new();
        self.parse_comma_separated_list(recovery_context_kind, |p| elements.push(parse_element(p)));
        elements
    }

    /// Parses a comma separated list of elements where each element is parsed
    /// sing the given `parse_element` function.
    ///
    /// The difference between this function and `parse_comma_separated_list_into_vec`
    /// is that this function does not return the parsed elements. Instead, it is the
    /// caller's responsibility to handle the parsed elements. This is the reason
    /// that the `parse_element` parameter is bound to [`FnMut`] instead of [`Fn`].
    fn parse_comma_separated_list(
        &mut self,
        recovery_context_kind: RecoveryContextKind,
        mut parse_element: impl FnMut(&mut Parser<'src>),
    ) {
        let mut progress = ParserProgress::default();

        let saved_context = self.recovery_context;
        self.recovery_context = self
            .recovery_context
            .union(RecoveryContext::from_kind(recovery_context_kind));

        let mut first_element = true;
        let mut trailing_comma_range: Option<TextRange> = None;

        loop {
            progress.assert_progressing(self);

            if recovery_context_kind.is_list_element(self) {
                parse_element(self);

                // Only unset this when we've completely parsed a single element. This is mainly to
                // raise the correct error in case the first element isn't valid and the current
                // token isn't a comma. Without this knowledge, the parser would later expect a
                // comma instead of raising the context error.
                first_element = false;

                let maybe_comma_range = self.current_token_range();
                if self.eat(TokenKind::Comma) {
                    trailing_comma_range = Some(maybe_comma_range);
                    continue;
                }
                trailing_comma_range = None;
            }

            // test_ok comma_separated_regular_list_terminator
            // # The first element is parsed by `parse_list_like_expression` and the comma after
            // # the first element is expected by `parse_list_expression`
            // [0]
            // [0, 1]
            // [0, 1,]
            // [0, 1, 2]
            // [0, 1, 2,]
            if recovery_context_kind.is_regular_list_terminator(self) {
                break;
            }

            // test_err comma_separated_missing_comma_between_elements
            // # The comma between the first two elements is expected in `parse_list_expression`.
            // [0, 1 2]
            if recovery_context_kind.is_list_element(self) {
                // This is a special case to expect a comma between two elements and should be
                // checked before running the error recovery. This is because the error recovery
                // will always run as the parser is currently at a list element.
                self.expect(TokenKind::Comma);
                continue;
            }

            // Run the error recovery: If the token is recognised as an element or terminator of an
            // enclosing list, then we try to re-lex in the context of a logical line and break out
            // of list parsing.
            if self.is_enclosing_list_element_or_terminator() {
                self.tokens.re_lex_logical_token();
                break;
            }

            if first_element || self.at(TokenKind::Comma) {
                // There are two conditions when we need to add the recovery context error:
                //
                // 1. If the parser is at a comma which means that there's a missing element
                //    otherwise the comma would've been consumed by the first `eat` call above.
                //    And, the parser doesn't take the re-lexing route on a comma token.
                // 2. If it's the first element and the current token is not a comma which means
                //    that it's an invalid element.

                // test_err comma_separated_missing_element_between_commas
                // [0, 1, , 2]

                // test_err comma_separated_missing_first_element
                // call(= 1)
                self.add_error(
                    recovery_context_kind.create_error(self),
                    self.current_token_range(),
                );

                trailing_comma_range = if self.at(TokenKind::Comma) {
                    Some(self.current_token_range())
                } else {
                    None
                };
            } else {
                // Otherwise, there should've been a comma at this position. This could be because
                // the element isn't consumed completely by `parse_element`.

                // test_err comma_separated_missing_comma
                // call(**x := 1)
                self.expect(TokenKind::Comma);

                trailing_comma_range = None;
            }

            self.bump_any();
        }

        if let Some(trailing_comma_range) = trailing_comma_range {
            if !recovery_context_kind.allow_trailing_comma() {
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

    /// Creates a checkpoint to which the parser can later return to using [`Self::rewind`].
    fn checkpoint(&self) -> ParserCheckpoint {
        ParserCheckpoint {
            tokens: self.tokens.checkpoint(),
            errors_position: self.errors.len(),
            current_token_id: self.current_token_id,
            prev_token_end: self.prev_token_end,
            recovery_context: self.recovery_context,
        }
    }

    /// Restore the parser to the given checkpoint.
    fn rewind(&mut self, checkpoint: ParserCheckpoint) {
        let ParserCheckpoint {
            tokens,
            errors_position,
            current_token_id,
            prev_token_end,
            recovery_context,
        } = checkpoint;

        self.tokens.rewind(tokens);
        self.errors.truncate(errors_position);
        self.current_token_id = current_token_id;
        self.prev_token_end = prev_token_end;
        self.recovery_context = recovery_context;
    }
}

struct ParserCheckpoint {
    tokens: TokenSourceCheckpoint,
    errors_position: usize,
    current_token_id: TokenId,
    prev_token_end: TextSize,
    recovery_context: RecoveryContext,
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
    /// A list of `with` items that are surrounded by parentheses.
    ///
    /// ```python
    /// with (item1, item2): ...
    /// with (item1, item2 as foo): ...
    /// ```
    ///
    /// The parentheses belongs to the `with` statement.
    Parenthesized,

    /// The `with` item has a parenthesized expression.
    ///
    /// ```python
    /// with (item) as foo: ...
    /// ```
    ///
    /// The parentheses belongs to the context expression.
    ParenthesizedExpression,

    /// The `with` items aren't parenthesized in any way.
    ///
    /// ```python
    /// with item: ...
    /// with item as foo: ...
    /// with item1, item2: ...
    /// ```
    ///
    /// There are no parentheses around the items.
    Unparenthesized,
}

impl WithItemKind {
    /// Returns `true` if the with items are parenthesized.
    const fn is_parenthesized(self) -> bool {
        matches!(self, WithItemKind::Parenthesized)
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum FStringElementsKind {
    /// The regular f-string elements.
    ///
    /// For example, the `"hello "`, `x`, and `" world"` elements in:
    /// ```py
    /// f"hello {x:.2f} world"
    /// ```
    Regular,

    /// The f-string elements are part of the format specifier.
    ///
    /// For example, the `.2f` in:
    /// ```py
    /// f"hello {x:.2f} world"
    /// ```
    FormatSpec,
}

impl FStringElementsKind {
    const fn list_terminator(self) -> TokenKind {
        match self {
            FStringElementsKind::Regular => TokenKind::FStringEnd,
            // test_ok fstring_format_spec_terminator
            // f"hello {x:} world"
            // f"hello {x:.3f} world"
            FStringElementsKind::FormatSpec => TokenKind::Rbrace,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum Parenthesized {
    /// The elements are parenthesized, e.g., `(a, b)`.
    Yes,
    /// The elements are not parenthesized, e.g., `a, b`.
    No,
}

impl From<bool> for Parenthesized {
    fn from(value: bool) -> Self {
        if value {
            Parenthesized::Yes
        } else {
            Parenthesized::No
        }
    }
}

impl Parenthesized {
    /// Returns `true` if the parenthesized value is `Yes`.
    const fn is_yes(self) -> bool {
        matches!(self, Parenthesized::Yes)
    }
}

#[derive(Copy, Clone, Debug)]
enum ListTerminatorKind {
    /// The current token terminates the list.
    Regular,
    /// The current token doesn't terminate the list, but is useful for better error recovery.
    ErrorRecovery,
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
    ImportFromAsNames(Parenthesized),

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
    TupleElements(Parenthesized),

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

    /// When parsing a list of f-string elements which are either literal elements
    /// or expressions.
    FStringElements(FStringElementsKind),
}

impl RecoveryContextKind {
    /// Returns `true` if a trailing comma is allowed in the current context.
    const fn allow_trailing_comma(self) -> bool {
        matches!(
            self,
            RecoveryContextKind::Slices
                | RecoveryContextKind::TupleElements(_)
                | RecoveryContextKind::SetElements
                | RecoveryContextKind::ListElements
                | RecoveryContextKind::DictElements
                | RecoveryContextKind::Arguments
                | RecoveryContextKind::MatchPatternMapping
                | RecoveryContextKind::SequenceMatchPattern(_)
                | RecoveryContextKind::MatchPatternClassArguments
                // Only allow a trailing comma if the with item itself is parenthesized
                | RecoveryContextKind::WithItems(WithItemKind::Parenthesized)
                | RecoveryContextKind::Parameters(_)
                | RecoveryContextKind::TypeParams
                | RecoveryContextKind::DeleteTargets
                | RecoveryContextKind::ImportFromAsNames(Parenthesized::Yes)
        )
    }

    /// Returns `true` if the parser is at a token that terminates the list as per the context.
    ///
    /// This token could either end the list or is only present for better error recovery. Refer to
    /// [`is_regular_list_terminator`] to only check against the former.
    ///
    /// [`is_regular_list_terminator`]: RecoveryContextKind::is_regular_list_terminator
    fn is_list_terminator(self, p: &Parser) -> bool {
        self.list_terminator_kind(p).is_some()
    }

    /// Returns `true` if the parser is at a token that terminates the list as per the context but
    /// the token isn't part of the error recovery set.
    fn is_regular_list_terminator(self, p: &Parser) -> bool {
        matches!(
            self.list_terminator_kind(p),
            Some(ListTerminatorKind::Regular)
        )
    }

    /// Checks the current token the parser is at and returns the list terminator kind if the token
    /// terminates the list as per the context.
    fn list_terminator_kind(self, p: &Parser) -> Option<ListTerminatorKind> {
        // The end of file marker ends all lists.
        if p.at(TokenKind::EndOfFile) {
            return Some(ListTerminatorKind::Regular);
        }

        match self {
            // The parser must consume all tokens until the end
            RecoveryContextKind::ModuleStatements => None,
            RecoveryContextKind::BlockStatements => p
                .at(TokenKind::Dedent)
                .then_some(ListTerminatorKind::Regular),

            RecoveryContextKind::Elif => {
                p.at(TokenKind::Else).then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::Except => {
                matches!(p.current_token_kind(), TokenKind::Finally | TokenKind::Else)
                    .then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::AssignmentTargets => {
                // test_ok assign_targets_terminator
                // x = y = z = 1; a, b
                // x = y = z = 1
                // a, b
                matches!(p.current_token_kind(), TokenKind::Newline | TokenKind::Semi)
                    .then_some(ListTerminatorKind::Regular)
            }

            // Tokens other than `]` are for better error recovery. For example, recover when we
            // find the `:` of a clause header or the equal of a type assignment.
            RecoveryContextKind::TypeParams => {
                if p.at(TokenKind::Rsqb) {
                    Some(ListTerminatorKind::Regular)
                } else {
                    matches!(
                        p.current_token_kind(),
                        TokenKind::Newline | TokenKind::Colon | TokenKind::Equal | TokenKind::Lpar
                    )
                    .then_some(ListTerminatorKind::ErrorRecovery)
                }
            }
            // The names of an import statement cannot be parenthesized, so `)` is not a
            // terminator.
            RecoveryContextKind::ImportNames => {
                // test_ok import_stmt_terminator
                // import a, b; import c, d
                // import a, b
                // c, d
                matches!(p.current_token_kind(), TokenKind::Semi | TokenKind::Newline)
                    .then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::ImportFromAsNames(_) => {
                // test_ok from_import_stmt_terminator
                // from a import (b, c)
                // from a import (b, c); x, y
                // from a import b, c; x, y
                // from a import b, c
                // x, y
                matches!(
                    p.current_token_kind(),
                    TokenKind::Rpar | TokenKind::Semi | TokenKind::Newline
                )
                .then_some(ListTerminatorKind::Regular)
            }
            // The elements in a container expression cannot end with a newline
            // as all of them are actually non-logical newlines.
            RecoveryContextKind::Slices | RecoveryContextKind::ListElements => {
                p.at(TokenKind::Rsqb).then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::SetElements | RecoveryContextKind::DictElements => p
                .at(TokenKind::Rbrace)
                .then_some(ListTerminatorKind::Regular),
            RecoveryContextKind::TupleElements(parenthesized) => {
                if parenthesized.is_yes() {
                    p.at(TokenKind::Rpar).then_some(ListTerminatorKind::Regular)
                } else {
                    p.at_sequence_end().then_some(ListTerminatorKind::Regular)
                }
            }
            RecoveryContextKind::SequenceMatchPattern(parentheses) => match parentheses {
                None => {
                    // test_ok match_sequence_pattern_terminator
                    // match subject:
                    //     case a: ...
                    //     case a if x: ...
                    //     case a, b: ...
                    //     case a, b if x: ...
                    matches!(p.current_token_kind(), TokenKind::Colon | TokenKind::If)
                        .then_some(ListTerminatorKind::Regular)
                }
                Some(parentheses) => {
                    // test_ok match_sequence_pattern_parentheses_terminator
                    // match subject:
                    //     case [a, b]: ...
                    //     case (a, b): ...
                    p.at(parentheses.closing_kind())
                        .then_some(ListTerminatorKind::Regular)
                }
            },
            RecoveryContextKind::MatchPatternMapping => p
                .at(TokenKind::Rbrace)
                .then_some(ListTerminatorKind::Regular),
            RecoveryContextKind::MatchPatternClassArguments => {
                p.at(TokenKind::Rpar).then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::Arguments => {
                p.at(TokenKind::Rpar).then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::DeleteTargets | RecoveryContextKind::Identifiers => {
                // test_ok del_targets_terminator
                // del a, b; c, d
                // del a, b
                // c, d
                matches!(p.current_token_kind(), TokenKind::Semi | TokenKind::Newline)
                    .then_some(ListTerminatorKind::Regular)
            }
            RecoveryContextKind::Parameters(function_kind) => {
                // `lambda x, y: ...` or `def f(x, y): ...`
                if p.at(function_kind.list_terminator()) {
                    Some(ListTerminatorKind::Regular)
                } else {
                    // To recover from missing closing parentheses
                    (p.at(TokenKind::Rarrow) || p.at_compound_stmt())
                        .then_some(ListTerminatorKind::ErrorRecovery)
                }
            }
            RecoveryContextKind::WithItems(with_item_kind) => match with_item_kind {
                WithItemKind::Parenthesized => match p.current_token_kind() {
                    TokenKind::Rpar => Some(ListTerminatorKind::Regular),
                    TokenKind::Colon => Some(ListTerminatorKind::ErrorRecovery),
                    _ => None,
                },
                WithItemKind::Unparenthesized | WithItemKind::ParenthesizedExpression => p
                    .at(TokenKind::Colon)
                    .then_some(ListTerminatorKind::Regular),
            },
            RecoveryContextKind::FStringElements(kind) => {
                if p.at(kind.list_terminator()) {
                    Some(ListTerminatorKind::Regular)
                } else {
                    // test_err unterminated_fstring_newline_recovery
                    // f"hello
                    // 1 + 1
                    // f"hello {x
                    // 2 + 2
                    // f"hello {x:
                    // 3 + 3
                    // f"hello {x}
                    // 4 + 4
                    p.at(TokenKind::Newline)
                        .then_some(ListTerminatorKind::ErrorRecovery)
                }
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
            RecoveryContextKind::ImportNames => p.at_name_or_soft_keyword(),
            RecoveryContextKind::ImportFromAsNames(_) => {
                p.at(TokenKind::Star) || p.at_name_or_soft_keyword()
            }
            RecoveryContextKind::Slices => p.at(TokenKind::Colon) || p.at_expr(),
            RecoveryContextKind::ListElements
            | RecoveryContextKind::SetElements
            | RecoveryContextKind::TupleElements(_) => p.at_expr(),
            RecoveryContextKind::DictElements => p.at(TokenKind::DoubleStar) || p.at_expr(),
            RecoveryContextKind::SequenceMatchPattern(_) => {
                // `+` doesn't start any pattern but is here for better error recovery.
                p.at(TokenKind::Plus) || p.at_pattern_start()
            }
            RecoveryContextKind::MatchPatternMapping => {
                // A star pattern is invalid as a mapping key and is here only for
                // better error recovery.
                p.at(TokenKind::Star) || p.at_mapping_pattern_start()
            }
            RecoveryContextKind::MatchPatternClassArguments => p.at_pattern_start(),
            RecoveryContextKind::Arguments => p.at_expr(),
            RecoveryContextKind::DeleteTargets => p.at_expr(),
            RecoveryContextKind::Identifiers => p.at_name_or_soft_keyword(),
            RecoveryContextKind::Parameters(_) => {
                matches!(
                    p.current_token_kind(),
                    TokenKind::Star | TokenKind::DoubleStar | TokenKind::Slash
                ) || p.at_name_or_soft_keyword()
            }
            RecoveryContextKind::WithItems(_) => p.at_expr(),
            RecoveryContextKind::FStringElements(_) => matches!(
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
            RecoveryContextKind::ImportFromAsNames(parenthesized) => {
                if parenthesized.is_yes() {
                    ParseErrorType::OtherError("Expected an import name or a ')'".to_string())
                } else {
                    ParseErrorType::OtherError("Expected an import name".to_string())
                }
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
            RecoveryContextKind::FStringElements(kind) => match kind {
                FStringElementsKind::Regular => ParseErrorType::OtherError(
                    "Expected an f-string element or the end of the f-string".to_string(),
                ),
                FStringElementsKind::FormatSpec => {
                    ParseErrorType::OtherError("Expected an f-string element or a '}'".to_string())
                }
            },
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
        const IMPORT_FROM_AS_NAMES_PARENTHESIZED = 1 << 6;
        const IMPORT_FROM_AS_NAMES_UNPARENTHESIZED = 1 << 7;
        const IMPORT_NAMES = 1 << 8;
        const SLICES = 1 << 9;
        const LIST_ELEMENTS = 1 << 10;
        const SET_ELEMENTS = 1 << 11;
        const DICT_ELEMENTS = 1 << 12;
        const TUPLE_ELEMENTS_PARENTHESIZED = 1 << 13;
        const TUPLE_ELEMENTS_UNPARENTHESIZED = 1 << 14;
        const SEQUENCE_MATCH_PATTERN = 1 << 15;
        const SEQUENCE_MATCH_PATTERN_LIST = 1 << 16;
        const SEQUENCE_MATCH_PATTERN_TUPLE = 1 << 17;
        const MATCH_PATTERN_MAPPING = 1 << 18;
        const MATCH_PATTERN_CLASS_ARGUMENTS = 1 << 19;
        const ARGUMENTS = 1 << 20;
        const DELETE = 1 << 21;
        const IDENTIFIERS = 1 << 22;
        const FUNCTION_PARAMETERS = 1 << 23;
        const LAMBDA_PARAMETERS = 1 << 24;
        const WITH_ITEMS_PARENTHESIZED = 1 << 25;
        const WITH_ITEMS_PARENTHESIZED_EXPRESSION = 1 << 26;
        const WITH_ITEMS_UNPARENTHESIZED = 1 << 28;
        const F_STRING_ELEMENTS = 1 << 29;
        const F_STRING_ELEMENTS_IN_FORMAT_SPEC = 1 << 30;
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
            RecoveryContextKind::ImportFromAsNames(parenthesized) => match parenthesized {
                Parenthesized::Yes => RecoveryContext::IMPORT_FROM_AS_NAMES_PARENTHESIZED,
                Parenthesized::No => RecoveryContext::IMPORT_FROM_AS_NAMES_UNPARENTHESIZED,
            },
            RecoveryContextKind::ImportNames => RecoveryContext::IMPORT_NAMES,
            RecoveryContextKind::Slices => RecoveryContext::SLICES,
            RecoveryContextKind::ListElements => RecoveryContext::LIST_ELEMENTS,
            RecoveryContextKind::SetElements => RecoveryContext::SET_ELEMENTS,
            RecoveryContextKind::DictElements => RecoveryContext::DICT_ELEMENTS,
            RecoveryContextKind::TupleElements(parenthesized) => match parenthesized {
                Parenthesized::Yes => RecoveryContext::TUPLE_ELEMENTS_PARENTHESIZED,
                Parenthesized::No => RecoveryContext::TUPLE_ELEMENTS_UNPARENTHESIZED,
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
            RecoveryContextKind::FStringElements(kind) => match kind {
                FStringElementsKind::Regular => RecoveryContext::F_STRING_ELEMENTS,
                FStringElementsKind::FormatSpec => {
                    RecoveryContext::F_STRING_ELEMENTS_IN_FORMAT_SPEC
                }
            },
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
            RecoveryContext::IMPORT_FROM_AS_NAMES_PARENTHESIZED => {
                RecoveryContextKind::ImportFromAsNames(Parenthesized::Yes)
            }
            RecoveryContext::IMPORT_FROM_AS_NAMES_UNPARENTHESIZED => {
                RecoveryContextKind::ImportFromAsNames(Parenthesized::No)
            }
            RecoveryContext::IMPORT_NAMES => RecoveryContextKind::ImportNames,
            RecoveryContext::SLICES => RecoveryContextKind::Slices,
            RecoveryContext::LIST_ELEMENTS => RecoveryContextKind::ListElements,
            RecoveryContext::SET_ELEMENTS => RecoveryContextKind::SetElements,
            RecoveryContext::DICT_ELEMENTS => RecoveryContextKind::DictElements,
            RecoveryContext::TUPLE_ELEMENTS_PARENTHESIZED => {
                RecoveryContextKind::TupleElements(Parenthesized::Yes)
            }
            RecoveryContext::TUPLE_ELEMENTS_UNPARENTHESIZED => {
                RecoveryContextKind::TupleElements(Parenthesized::No)
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
            RecoveryContext::F_STRING_ELEMENTS => {
                RecoveryContextKind::FStringElements(FStringElementsKind::Regular)
            }
            RecoveryContext::F_STRING_ELEMENTS_IN_FORMAT_SPEC => {
                RecoveryContextKind::FStringElements(FStringElementsKind::FormatSpec)
            }
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
