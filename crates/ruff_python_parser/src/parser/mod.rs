use std::fmt::Display;

use ast::{
    BoolOp, CmpOp, ConversionFlag, ExceptHandler, ExprContext, FStringElement, IpyEscapeKind,
    Number, Operator, Pattern, Singleton, UnaryOp,
};
use bitflags::bitflags;

use crate::{
    error::FStringErrorType,
    helpers::{self, token_kind_to_cmp_op},
    lexer::{LexResult, Spanned},
    string::{
        concatenated_strings, parse_fstring_literal_element, parse_string_literal, StringType,
    },
    token_set::TokenSet,
    Mode, ParseError, ParseErrorType, Tok, TokenKind,
};
use itertools::PeekNth;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextLen, TextRange};

mod functions;
mod tests;
pub(super) use functions::ParenthesizedExpr;
pub use functions::{
    parse, parse_expression, parse_expression_starts_at, parse_ok_tokens, parse_ok_tokens_lalrpop,
    parse_ok_tokens_new, parse_program, parse_starts_at, parse_suite, parse_tokens,
};

#[derive(Debug)]
pub struct ParsedFile {
    pub ast: ast::Mod,
    pub parse_errors: Vec<ParseError>,
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    struct ParserCtxFlags: u16 {
        const TUPLE_EXPR = 1 << 0;
        const PARENTHESIZED_EXPR = 1 << 1;
        const BRACKETSIZED_EXPR = 1 << 2;
        const BRACESIZED_EXPR = 1 << 3;
        const LAMBDA_EXPR = 1 << 4;

        const IF_STMT = 1 << 5;
        const FUNC_DEF_STMT = 1 << 6;
        const CLASS_DEF_STMT = 1 << 7;
        const WITH_STMT = 1 << 8;
        const FOR_STMT = 1 << 9;
        const WHILE_STMT  = 1 << 10;
        const MATCH_STMT  = 1 << 11;

        const ARGUMENTS = 1 << 12;
        const FOR_TARGET = 1 << 13;
    }
}

type ExprWithRange = (Expr, TextRange);
type StmtWithRange = (Stmt, TextRange);

/// Binding power associativity
enum Associativity {
    Left,
    Right,
}

pub(crate) struct Parser<'src, 'src_path, I>
where
    I: Iterator<Item = LexResult>,
{
    source: &'src str,
    source_path: &'src_path str,
    lexer: PeekNth<I>,
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
    /// During the parsing of expression or statement, multiple `ctx`s can be created.
    /// `ctx_stack` stores the previous `ctx`s that were created during the parsing. For example,
    /// when parsing a tuple expression, e.g. `(1, 2, 3)`, two [`ParserCtxFlags`] will be
    /// created `ParserCtxFlags::PARENTHESIZED_EXPR` and `ParserCtxFlags::TUPLE_EXPR`.
    ///
    /// When parsing a tuple the first context created is `ParserCtxFlags::PARENTHESIZED_EXPR`.
    /// Afterwards, the `ParserCtxFlags::TUPLE_EXPR` is created and `ParserCtxFlags::PARENTHESIZED_EXPR`
    /// is pushed onto the `ctx_stack`.
    /// `ParserCtxFlags::PARENTHESIZED_EXPR` is removed from the stack and set to be the current `ctx`,
    /// after we parsed all elements in the tuple.
    ///
    /// The end of the vector is the top of the stack.
    ctx_stack: Vec<ParserCtxFlags>,
    /// Stores the last `ctx` of an expression or statement that was parsed.
    last_ctx: ParserCtxFlags,
    /// Specify the mode in which the code will be parsed.
    mode: Mode,
}

const NEWLINE_EOF_SET: TokenSet = TokenSet::new(&[TokenKind::Newline, TokenKind::EndOfFile]);
const LITERAL_SET: TokenSet = TokenSet::new(&[
    TokenKind::Name,
    TokenKind::Int,
    TokenKind::Float,
    TokenKind::Complex,
    TokenKind::Plus,
    TokenKind::String,
    TokenKind::Ellipsis,
    TokenKind::True,
    TokenKind::False,
    TokenKind::None,
]);
/// Tokens that are usually an expression or the start of one.
const EXPR_SET: TokenSet = TokenSet::new(&[
    TokenKind::Minus,
    TokenKind::Tilde,
    TokenKind::Star,
    TokenKind::DoubleStar,
    TokenKind::Vbar,
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
/// Tokens that can appear after an expression.
const END_EXPR_SET: TokenSet = TokenSet::new(&[
    TokenKind::Newline,
    TokenKind::Semi,
    TokenKind::Colon,
    TokenKind::EndOfFile,
    TokenKind::Rbrace,
    TokenKind::Rsqb,
    TokenKind::Rpar,
    TokenKind::Comma,
    TokenKind::Dedent,
    TokenKind::Else,
    TokenKind::As,
    TokenKind::From,
    TokenKind::For,
    TokenKind::Async,
]);
/// Tokens that represent compound statements.
const COMPOUND_STMT_SET: TokenSet = TokenSet::new(&[
    TokenKind::Match,
    TokenKind::If,
    TokenKind::Else,
    TokenKind::Elif,
    TokenKind::With,
    TokenKind::While,
    TokenKind::For,
    TokenKind::Try,
    TokenKind::Def,
    TokenKind::Class,
    TokenKind::Async,
]);
/// Tokens that represent simple statements, but doesn't include expressions.
const SIMPLE_STMT_SET: TokenSet = TokenSet::new(&[
    TokenKind::Pass,
    TokenKind::Return,
    TokenKind::Break,
    TokenKind::Continue,
    TokenKind::Global,
    TokenKind::Nonlocal,
    TokenKind::Assert,
    TokenKind::Yield,
    TokenKind::Del,
    TokenKind::Raise,
    TokenKind::Import,
    TokenKind::From,
    TokenKind::Type,
]);
/// Tokens that represent simple statements, including expressions.
const SIMPLE_STMT_SET2: TokenSet = SIMPLE_STMT_SET.union(EXPR_SET);

impl<'src, 'src_path, I> Parser<'src, 'src_path, I>
where
    I: Iterator<Item = LexResult>,
{
    pub(crate) fn new(
        source: &'src str,
        source_path: &'src_path str,
        mode: Mode,
        lexer: I,
    ) -> Parser<'src, 'src_path, impl Iterator<Item = LexResult>> {
        Parser {
            mode,
            source,
            source_path,
            errors: Vec::new(),
            ctx_stack: Vec::new(),
            ctx: ParserCtxFlags::empty(),
            last_ctx: ParserCtxFlags::empty(),
            lexer: itertools::peek_nth(lexer),
        }
    }

    pub(crate) fn parse(mut self) -> ParsedFile {
        let mut body = vec![];

        let ast = if self.mode == Mode::Expression {
            let (expr, range) = self.parse_exprs();
            loop {
                if !self.eat(TokenKind::Newline) {
                    break;
                }
            }
            self.expect(TokenKind::EndOfFile);

            ast::Mod::Expression(ast::ModExpression {
                body: Box::new(expr),
                range,
            })
        } else {
            let is_src_empty = self.at(TokenKind::EndOfFile);
            while !self.at(TokenKind::EndOfFile) {
                if self.at(TokenKind::Indent) {
                    self.handle_unexpected_indentation(&mut body, "unexpected indentation");
                    continue;
                }
                let (stmt, _) = self.parse_statement();
                body.push(stmt);
            }
            ast::Mod::Module(ast::ModModule {
                body,
                // If the `source` only contains comments or empty spaces, return
                // an empty range.
                range: if is_src_empty {
                    TextRange::default()
                } else {
                    TextRange::new(
                        0.into(),
                        self.source
                            .len()
                            .try_into()
                            .expect("source length is  bigger than u32 max"),
                    )
                },
            })
        };

        // After parsing, the `ctx` and `ctx_stack` should be empty.
        // If it's not, you probably forgot to call `clear_ctx` somewhere.
        assert!(self.ctx.is_empty() && self.ctx_stack.is_empty());

        ParsedFile {
            ast,
            parse_errors: self.errors,
        }
    }

    #[inline]
    fn set_ctx(&mut self, ctx: ParserCtxFlags) {
        self.ctx_stack.push(self.ctx);
        self.ctx = ctx;
    }

    #[inline]
    fn clear_ctx(&mut self, ctx: ParserCtxFlags) {
        assert_eq!(self.ctx, ctx);
        self.last_ctx = ctx;
        if let Some(top) = self.ctx_stack.pop() {
            self.ctx = top;
        }
    }

    #[inline]
    fn has_ctx(&self, ctx: ParserCtxFlags) -> bool {
        self.ctx.intersects(ctx)
    }

    #[inline]
    fn has_in_curr_or_parent_ctx(&self, ctx: ParserCtxFlags) -> bool {
        self.has_ctx(ctx) || self.parent_ctx().intersects(ctx)
    }

    #[inline]
    fn parent_ctx(&self) -> ParserCtxFlags {
        self.ctx_stack
            .last()
            .copied()
            .unwrap_or(ParserCtxFlags::empty())
    }

    fn next_token(&mut self) -> Spanned {
        self.lexer
            .next()
            .map(|result| match result {
                Ok(token) => token,
                Err(lex_error) => {
                    self.add_error(ParseErrorType::Lexical(lex_error.error), lex_error.location);

                    // Return a `Invalid` token when encountering an error
                    (Tok::Invalid, lex_error.location)
                }
            })
            .unwrap_or((
                Tok::EndOfFile,
                TextRange::empty(
                    self.source
                        .len()
                        .try_into()
                        .expect("source length is bigger than u32 max"),
                ),
            ))
    }

    fn lookahead(&mut self, offset: usize) -> (TokenKind, TextRange) {
        self.lexer.peek_nth(offset).map_or(
            (
                TokenKind::EndOfFile,
                TextRange::empty(
                    self.source
                        .len()
                        .try_into()
                        .expect("source length is  bigger than u32 max"),
                ),
            ),
            |result| match result {
                Ok((tok, range)) => (tok.into(), *range),
                // Return a `Invalid` token when encountering an error
                Err(err) => (TokenKind::Invalid, err.location),
            },
        )
    }

    #[inline]
    fn current_token(&mut self) -> (TokenKind, TextRange) {
        self.lookahead(0)
    }

    #[inline]
    fn current_kind(&mut self) -> TokenKind {
        self.lookahead(0).0
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if !self.at(kind) {
            return false;
        }

        self.next_token();
        true
    }

    fn expect(&mut self, expected: TokenKind) -> bool {
        if self.eat(expected) {
            return true;
        }

        let (found, range) = self.current_token();
        self.add_error(ParseErrorType::ExpectedToken { found, expected }, range);
        false
    }

    /// Expects a specific token kind, skipping leading unexpected tokens if needed.
    fn expect_and_recover(&mut self, expected: TokenKind, recover_set: TokenSet) {
        if !self.expect(expected) {
            let expected_set = NEWLINE_EOF_SET
                .union(recover_set)
                .union([expected].as_slice().into());
            // Skip leading unexpected tokens
            self.skip_until(expected_set);

            self.eat(expected);
        }
    }

    fn add_error(&mut self, error: ParseErrorType, range: TextRange) {
        self.errors.push(ParseError {
            error,
            location: range,
            source_path: self.source_path.to_string(),
        });
    }

    fn skip_until(&mut self, token_set: TokenSet) {
        while !self.at_ts(token_set) {
            self.next_token();
        }
    }

    fn at(&mut self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    fn at_ts(&mut self, ts: TokenSet) -> bool {
        ts.contains(self.current_kind())
    }

    fn at_expr(&mut self) -> bool {
        self.at_ts(EXPR_SET)
    }

    fn at_simple_stmt(&mut self) -> bool {
        self.at_ts(SIMPLE_STMT_SET2)
    }

    fn at_compound_stmt(&mut self) -> bool {
        self.at_ts(COMPOUND_STMT_SET)
    }

    fn src_text(&self, mut range: TextRange) -> &'src str {
        // This check is to prevent the parser from panicking when using the
        // `parse_expression_starts_at` function with an offset bigger than zero.
        //
        // The parser assumes that the token's range values are smaller than
        // the source length. But, with an offset bigger than zero, it can
        // happen that the token's range values are bigger than the source
        // length, causing the parser to panic when calling this function
        // with such ranges.
        //
        // Therefore, we fix this by creating a new range starting at 0 up to
        // the source length - 1.
        //
        // TODO: Create the proper range here.
        let src_len = self.source.len();
        if range.start().to_usize() > src_len || range.end().to_usize() > src_len {
            range = TextRange::new(
                0.into(),
                (self.source.len() - 1)
                    .try_into()
                    .expect("source length is bigger than u32 max"),
            );
        }
        &self.source[range]
    }

    fn current_range(&mut self) -> TextRange {
        self.lookahead(0).1
    }

    /// Parses elements enclosed within a delimiter pair, such as parentheses, brackets,
    /// or braces.
    ///
    /// Returns the [`TextRange`] of the parsed enclosed elements.
    fn parse_delimited(
        &mut self,
        allow_trailing_delim: bool,
        opening: TokenKind,
        delim: TokenKind,
        closing: TokenKind,
        mut func: impl FnMut(&mut Parser<'src, 'src_path, I>),
    ) -> TextRange {
        let start_range = self.current_range();
        assert!(self.eat(opening));

        self.parse_separated(
            allow_trailing_delim,
            delim,
            [closing].as_slice(),
            |parser| {
                func(parser);
                // Doesn't matter what range we return here
                TextRange::default()
            },
        );

        let end_range = self.current_range();
        self.expect_and_recover(closing, TokenSet::EMPTY);

        start_range.cover(end_range)
    }

    /// Parses a sequence of elements separated by a delimiter. This function stops
    /// parsing upon encountering any of the tokens in `ending_set`, if it doesn't
    /// encounter the tokens in `ending_set` it stops parsing when seeing the `EOF`
    /// or `Newline` token.
    ///
    /// Returns the last [`TextRange`] of the parsed elements. If none elements are
    /// parsed it returns `None`.
    fn parse_separated(
        &mut self,
        allow_trailing_delim: bool,
        delim: TokenKind,
        ending_set: impl Into<TokenSet>,
        mut func: impl FnMut(&mut Parser<'src, 'src_path, I>) -> TextRange,
    ) -> Option<TextRange> {
        let ending_set = NEWLINE_EOF_SET.union(ending_set.into());
        let mut final_range = None;

        while !self.at_ts(ending_set) {
            final_range = Some(func(self));

            // exit the loop if a trailing `delim` is not allowed
            if !allow_trailing_delim && ending_set.contains(self.lookahead(1).0) {
                break;
            }

            if self.at(delim) {
                final_range = Some(self.current_range());
                self.eat(delim);
            } else {
                if self.at_expr() {
                    self.expect(delim);
                } else {
                    break;
                }
            }
        }

        final_range
    }

    fn is_current_token_postfix(&mut self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Dot | TokenKind::Async | TokenKind::For
        )
    }

    fn handle_unexpected_indentation(
        &mut self,
        stmts: &mut Vec<Stmt>,
        error_msg: &str,
    ) -> TextRange {
        self.eat(TokenKind::Indent);

        let mut range = self.current_range();
        self.add_error(ParseErrorType::OtherError(error_msg.to_string()), range);

        while !self.at(TokenKind::Dedent) {
            let (stmt, stmt_range) = self.parse_statement();
            stmts.push(stmt);
            range = stmt_range;
        }
        assert!(self.eat(TokenKind::Dedent));

        range
    }

    fn parse_statement(&mut self) -> StmtWithRange {
        let (kind, range) = self.current_token();
        match kind {
            TokenKind::If => self.parse_if_stmt(),
            TokenKind::Try => self.parse_try_stmt(),
            TokenKind::For => self.parse_for_stmt(),
            TokenKind::With => self.parse_with_stmt(),
            TokenKind::At => self.parse_decorators(),
            TokenKind::Async => self.parse_async_stmt(),
            TokenKind::While => self.parse_while_stmt(),
            TokenKind::Def => self.parse_func_def_stmt(vec![], range),
            TokenKind::Class => self.parse_class_def_stmt(vec![], range),
            TokenKind::Match => self.parse_match_stmt(),
            _ => self.parse_simple_stmt_newline(),
        }
    }

    fn parse_match_stmt(&mut self) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::MATCH_STMT);
        let mut range = self.current_range();

        self.eat(TokenKind::Match);
        let (subject, _) = self.parse_expr_with_recovery(
            |parser| {
                let (expr, expr_range) = parser.parse_expr2();
                if parser.at(TokenKind::Comma) {
                    return parser.parse_tuple_expr(expr, expr_range, Parser::parse_expr2);
                }
                (expr, expr_range)
            },
            [TokenKind::Colon].as_slice(),
            "expecting expression after `match` keyword",
        );
        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        self.eat(TokenKind::Newline);
        if !self.eat(TokenKind::Indent) {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError(
                    "expected an indented block after `match` statement".to_string(),
                ),
                range,
            );
        }

        let (cases, cases_range) = self.parse_match_cases();
        range = range.cover(cases_range);

        self.eat(TokenKind::Dedent);

        self.clear_ctx(ParserCtxFlags::MATCH_STMT);
        (
            Stmt::Match(ast::StmtMatch {
                subject: Box::new(subject),
                cases,
                range,
            }),
            range,
        )
    }

    fn parse_match_case(&mut self) -> ast::MatchCase {
        let mut range = self.current_range();

        self.eat(TokenKind::Case);
        let (pattern, _) = self.parse_match_patterns();

        let guard = if self.eat(TokenKind::If) {
            let (expr, _) = self.parse_expr2();
            Some(Box::new(expr))
        } else {
            None
        };

        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);
        let (body, body_range) = self.parse_body();
        range = range.cover(body_range);

        ast::MatchCase {
            pattern,
            guard,
            body,
            range: range.cover(range),
        }
    }

    fn parse_match_cases(&mut self) -> (Vec<ast::MatchCase>, TextRange) {
        let mut range = self.current_range();

        if !self.at(TokenKind::Case) {
            self.add_error(
                ParseErrorType::OtherError("expecting `case` block after `match`".to_string()),
                range,
            );
        }

        let mut cases = vec![];
        while self.at(TokenKind::Case) {
            let case = self.parse_match_case();
            range = range.cover(case.range);

            cases.push(case);
        }

        (cases, range)
    }

    fn parse_attr_expr_for_match_pattern(
        &mut self,
        mut lhs: Expr,
        mut lhs_range: TextRange,
    ) -> ExprWithRange {
        loop {
            (lhs, lhs_range) = match self.current_kind() {
                TokenKind::Dot => self.parse_attribute_expr(lhs, lhs_range),
                _ => break,
            }
        }

        (lhs, lhs_range)
    }

    fn parse_match_pattern_literal(&mut self) -> (Pattern, TextRange) {
        let (tok, range) = self.next_token();
        match tok {
            Tok::None => (
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::None,
                    range,
                }),
                range,
            ),
            Tok::True => (
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::True,
                    range,
                }),
                range,
            ),
            Tok::False => (
                Pattern::MatchSingleton(ast::PatternMatchSingleton {
                    value: Singleton::False,
                    range,
                }),
                range,
            ),
            tok @ Tok::String { .. } => {
                let (str, str_range) = self.parse_string_expr(tok, range);
                (
                    Pattern::MatchValue(ast::PatternMatchValue {
                        value: Box::new(str),
                        range: str_range,
                    }),
                    str_range,
                )
            }
            Tok::Complex { real, imag } => (
                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: Number::Complex { real, imag },
                        range,
                    })),
                    range,
                }),
                range,
            ),
            Tok::Int { value } => (
                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: Number::Int(value),
                        range,
                    })),
                    range,
                }),
                range,
            ),
            Tok::Float { value } => (
                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: Number::Float(value),
                        range,
                    })),
                    range,
                }),
                range,
            ),
            Tok::Name { name } if self.at(TokenKind::Dot) => {
                let id = Expr::Name(ast::ExprName {
                    id: name,
                    ctx: ExprContext::Load,
                    range,
                });
                let (expr, range) = self.parse_attr_expr_for_match_pattern(id, range);
                (
                    Pattern::MatchValue(ast::PatternMatchValue {
                        value: Box::new(expr),
                        range,
                    }),
                    range,
                )
            }
            Tok::Name { name } => (
                Pattern::MatchAs(ast::PatternMatchAs {
                    range,
                    pattern: None,
                    name: if name == "_" {
                        None
                    } else {
                        Some(ast::Identifier { id: name, range })
                    },
                }),
                range,
            ),
            Tok::Minus
                if matches!(
                    self.current_kind(),
                    TokenKind::Int | TokenKind::Float | TokenKind::Complex
                ) =>
            {
                // Since the `Minus` token was consumed `parse_lhs` will not
                // be able to parse an `UnaryOp`, therefore we create the node
                // manually.
                let (expr, expr_range) = self.parse_lhs();
                let range = range.cover(expr_range);
                (
                    Pattern::MatchValue(ast::PatternMatchValue {
                        value: Box::new(Expr::UnaryOp(ast::ExprUnaryOp {
                            range,
                            op: UnaryOp::USub,
                            operand: Box::new(expr),
                        })),
                        range,
                    }),
                    range,
                )
            }
            kind => {
                const RECOVERY_SET: TokenSet =
                    TokenSet::new(&[TokenKind::Colon]).union(NEWLINE_EOF_SET);
                self.add_error(
                    ParseErrorType::InvalidMatchPatternLiteral {
                        pattern: kind.into(),
                    },
                    range,
                );
                self.skip_until(RECOVERY_SET);
                (
                    Pattern::Invalid(ast::PatternMatchInvalid {
                        value: self.src_text(range).into(),
                        range,
                    }),
                    range.cover_offset(self.current_range().start()),
                )
            }
        }
    }

    fn parse_delimited_match_pattern(&mut self) -> (Pattern, TextRange) {
        let mut range = self.current_range();

        let is_paren = self.at(TokenKind::Lpar);
        let is_bracket = self.at(TokenKind::Lsqb);

        let closing = if is_paren {
            self.eat(TokenKind::Lpar);
            TokenKind::Rpar
        } else {
            self.eat(TokenKind::Lsqb);
            TokenKind::Rsqb
        };

        if matches!(self.current_kind(), TokenKind::Newline | TokenKind::Colon) {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "missing `{}`",
                    if is_paren { ')' } else { ']' }
                )),
                range,
            );
        }

        if self.at(closing) {
            range = range.cover(self.current_range());
            self.eat(closing);

            return (
                Pattern::MatchSequence(ast::PatternMatchSequence {
                    patterns: vec![],
                    range,
                }),
                range,
            );
        }

        let (mut pattern, pattern_range) = self.parse_match_pattern();

        if is_bracket || self.at(TokenKind::Comma) {
            (pattern, _) = self.parse_sequence_match_pattern(pattern, pattern_range, closing);
        }

        range = range.cover(self.current_range());
        self.expect_and_recover(closing, TokenSet::EMPTY);

        if let Pattern::MatchSequence(mut sequence) = pattern {
            // Update the range to include the parenthesis or brackets
            sequence.range = range;
            (Pattern::MatchSequence(sequence), range)
        } else {
            (pattern, range)
        }
    }

    fn parse_sequence_match_pattern(
        &mut self,
        first_elt: Pattern,
        elt_range: TextRange,
        ending: TokenKind,
    ) -> (Pattern, TextRange) {
        // In case of the match sequence only having one element, we need to cover
        // the range of the comma.
        let mut final_range = elt_range.cover(self.current_range());
        self.eat(TokenKind::Comma);
        let mut patterns = vec![first_elt];

        let range = self.parse_separated(true, TokenKind::Comma, [ending].as_slice(), |parser| {
            let (pattern, pattern_range) = parser.parse_match_pattern();
            patterns.push(pattern);
            pattern_range
        });
        final_range = final_range.cover(range.unwrap_or(final_range));

        (
            Pattern::MatchSequence(ast::PatternMatchSequence {
                patterns,
                range: final_range,
            }),
            final_range,
        )
    }

    fn parse_match_pattern_lhs(&mut self) -> (Pattern, TextRange) {
        let (mut lhs, mut range) = match self.current_kind() {
            TokenKind::Lbrace => self.parse_match_pattern_mapping(),
            TokenKind::Star => self.parse_match_pattern_star(),
            TokenKind::Lpar | TokenKind::Lsqb => self.parse_delimited_match_pattern(),
            _ => self.parse_match_pattern_literal(),
        };

        if self.at(TokenKind::Lpar) {
            (lhs, range) = self.parse_match_pattern_class(lhs, range);
        }

        if self.at(TokenKind::Plus) || self.at(TokenKind::Minus) {
            let (op_kind, _) = self.next_token();

            let (lhs_value, lhs_range) = if let Pattern::MatchValue(lhs) = lhs {
                if !lhs.value.is_literal_expr() && !matches!(lhs.value.as_ref(), Expr::UnaryOp(_)) {
                    self.add_error(
                        ParseErrorType::OtherError(format!(
                            "invalid `{}` expression for match pattern",
                            self.src_text(lhs.range)
                        )),
                        lhs.range,
                    );
                }
                (lhs.value, lhs.range)
            } else {
                self.add_error(
                    ParseErrorType::OtherError("invalid lhs pattern".to_string()),
                    range,
                );
                (
                    Box::new(Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(range).into(),
                        range,
                    })),
                    range,
                )
            };

            let (rhs_pattern, rhs_range) = self.parse_match_pattern_lhs();
            let (rhs_value, rhs_range) = if let Pattern::MatchValue(rhs) = rhs_pattern {
                if !rhs.value.is_literal_expr() {
                    self.add_error(
                        ParseErrorType::OtherError(format!(
                            "invalid `{}` expression for match pattern",
                            self.src_text(rhs_range)
                        )),
                        rhs_range,
                    );
                }
                (rhs.value, rhs.range)
            } else {
                self.add_error(
                    ParseErrorType::OtherError("invalid rhs pattern".to_string()),
                    rhs_range,
                );
                (
                    Box::new(Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(rhs_range).into(),
                        range: rhs_range,
                    })),
                    rhs_range,
                )
            };

            if matches!(
                rhs_value.as_ref(),
                Expr::UnaryOp(ast::ExprUnaryOp {
                    op: UnaryOp::USub,
                    ..
                })
            ) {
                self.add_error(
                    ParseErrorType::OtherError(
                        "`-` not allowed in rhs of match pattern".to_string(),
                    ),
                    rhs_range,
                );
            }

            let op = if matches!(op_kind, Tok::Plus) {
                Operator::Add
            } else {
                Operator::Sub
            };
            let range = lhs_range.cover(rhs_range);
            return (
                Pattern::MatchValue(ast::PatternMatchValue {
                    value: Box::new(Expr::BinOp(ast::ExprBinOp {
                        left: lhs_value,
                        op,
                        right: rhs_value,
                        range,
                    })),
                    range,
                }),
                range,
            );
        }

        (lhs, range)
    }

    fn parse_match_pattern(&mut self) -> (Pattern, TextRange) {
        let (mut lhs, mut range) = self.parse_match_pattern_lhs();

        if self.at(TokenKind::Vbar) {
            let mut patterns = vec![lhs];

            while self.eat(TokenKind::Vbar) {
                let (pattern, pattern_range) = self.parse_match_pattern_lhs();
                range = range.cover(pattern_range);
                patterns.push(pattern);
            }

            lhs = Pattern::MatchOr(ast::PatternMatchOr { range, patterns });
        }

        if self.eat(TokenKind::As) {
            let ident = self.parse_identifier();
            range = range.cover(ident.range);
            lhs = Pattern::MatchAs(ast::PatternMatchAs {
                range,
                name: Some(ident),
                pattern: Some(Box::new(lhs)),
            });
        }

        (lhs, range)
    }

    fn parse_match_patterns(&mut self) -> (Pattern, TextRange) {
        let (pattern, range) = self.parse_match_pattern();

        if self.at(TokenKind::Comma) {
            return self.parse_sequence_match_pattern(pattern, range, TokenKind::Colon);
        }

        (pattern, range)
    }

    fn parse_match_pattern_star(&mut self) -> (Pattern, TextRange) {
        let mut range = self.current_range();
        self.eat(TokenKind::Star);

        let ident = self.parse_identifier();

        range = range.cover(ident.range);
        (
            Pattern::MatchStar(ast::PatternMatchStar {
                range,
                name: if ident.is_valid() && ident.id == "_" {
                    None
                } else {
                    Some(ident)
                },
            }),
            range,
        )
    }

    fn parse_match_pattern_class(
        &mut self,
        cls: Pattern,
        mut cls_range: TextRange,
    ) -> (Pattern, TextRange) {
        let mut patterns = vec![];
        let mut keywords = vec![];
        let mut has_seen_pattern = false;
        let mut has_seen_keyword_pattern = false;

        let args_range = self.parse_delimited(
            true,
            TokenKind::Lpar,
            TokenKind::Comma,
            TokenKind::Rpar,
            |parser| {
                let (pattern, pattern_range) = parser.parse_match_pattern();

                if parser.eat(TokenKind::Equal) {
                    has_seen_pattern = false;
                    has_seen_keyword_pattern = true;

                    if let Pattern::MatchAs(ast::PatternMatchAs {
                        name: Some(attr),
                        range,
                        ..
                    }) = pattern
                    {
                        let (pattern, _) = parser.parse_match_pattern();

                        keywords.push(ast::PatternKeyword {
                            attr,
                            pattern,
                            range: range.cover_offset(parser.current_range().start()),
                        });
                    } else {
                        parser.skip_until(END_EXPR_SET);
                        parser.add_error(
                            ParseErrorType::OtherError(format!(
                                "`{}` not valid keyword pattern",
                                parser.src_text(pattern_range)
                            )),
                            pattern_range,
                        );
                    }
                } else {
                    has_seen_pattern = true;
                    patterns.push(pattern);
                }

                if has_seen_keyword_pattern && has_seen_pattern {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "pattern not allowed after keyword pattern".to_string(),
                        ),
                        pattern_range,
                    );
                }
            },
        );

        let cls = match cls {
            Pattern::MatchAs(ast::PatternMatchAs {
                name: Some(ident), ..
            }) => {
                cls_range = ident.range;
                if ident.is_valid() {
                    Box::new(Expr::Name(ast::ExprName {
                        id: ident.id,
                        ctx: ExprContext::Load,
                        range: cls_range,
                    }))
                } else {
                    Box::new(Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(cls_range).into(),
                        range: cls_range,
                    }))
                }
            }
            Pattern::MatchValue(ast::PatternMatchValue {
                value,
                range: value_range,
            }) if matches!(value.as_ref(), Expr::Attribute(_)) => {
                cls_range = value_range;
                value
            }
            _ => {
                self.add_error(
                    ParseErrorType::OtherError(format!(
                        "`{}` invalid pattern match class",
                        self.src_text(cls_range)
                    )),
                    cls_range,
                );
                Box::new(Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(cls_range).into(),
                    range: cls_range,
                }))
            }
        };

        let range = cls_range.cover(args_range);
        (
            Pattern::MatchClass(ast::PatternMatchClass {
                cls,
                arguments: ast::PatternArguments {
                    patterns,
                    keywords,
                    range: args_range,
                },
                range,
            }),
            range,
        )
    }

    fn parse_match_pattern_mapping(&mut self) -> (Pattern, TextRange) {
        let mut keys = vec![];
        let mut patterns = vec![];
        let mut rest = None;

        let range = self.parse_delimited(
            true,
            TokenKind::Lbrace,
            TokenKind::Comma,
            TokenKind::Rbrace,
            |parser| {
                if parser.eat(TokenKind::DoubleStar) {
                    rest = Some(parser.parse_identifier());
                } else {
                    let (pattern, pattern_range) = parser.parse_match_pattern_lhs();
                    let key = match pattern {
                        Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => *value,
                        Pattern::MatchSingleton(ast::PatternMatchSingleton { value, range }) => {
                            match value {
                                Singleton::None => {
                                    Expr::NoneLiteral(ast::ExprNoneLiteral { range })
                                }
                                Singleton::True => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                                    value: true,
                                    range,
                                }),
                                Singleton::False => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                                    value: false,
                                    range,
                                }),
                            }
                        }
                        _ => {
                            parser.add_error(
                                ParseErrorType::OtherError(format!(
                                    "invalid mapping pattern key `{}`",
                                    parser.src_text(pattern_range)
                                )),
                                pattern_range,
                            );
                            Expr::Invalid(ast::ExprInvalid {
                                value: parser.src_text(pattern_range).into(),
                                range: pattern_range,
                            })
                        }
                    };
                    keys.push(key);

                    parser.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

                    let (pattern, _) = parser.parse_match_pattern();
                    patterns.push(pattern);
                }
            },
        );

        (
            Pattern::MatchMapping(ast::PatternMatchMapping {
                range,
                keys,
                patterns,
                rest,
            }),
            range,
        )
    }

    fn parse_async_stmt(&mut self) -> StmtWithRange {
        let mut range = self.current_range();
        self.eat(TokenKind::Async);

        let (kind, kind_range) = self.current_token();
        let (mut stmt, stmt_range) = match kind {
            TokenKind::Def => self.parse_func_def_stmt(vec![], kind_range),
            TokenKind::With => self.parse_with_stmt(),
            TokenKind::For => self.parse_for_stmt(),
            kind => {
                // Although this statement is not a valid `async` statement,
                // we still parse it.
                self.add_error(ParseErrorType::StmtIsNotAsync(kind), kind_range);
                self.parse_statement()
            }
        };
        range = range.cover(stmt_range);

        match stmt {
            Stmt::FunctionDef(ref mut func) => {
                func.range = range;
                func.is_async = true;
            }
            Stmt::For(ref mut for_stmt) => {
                for_stmt.range = range;
                for_stmt.is_async = true;
            }
            Stmt::With(ref mut with_stmt) => {
                with_stmt.range = range;
                with_stmt.is_async = true;
            }
            _ => {}
        };

        (stmt, range)
    }

    fn parse_while_stmt(&mut self) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::WHILE_STMT);
        let mut range = self.current_range();
        self.eat(TokenKind::While);

        let (test, _) = self.parse_expr_with_recovery(
            Parser::parse_expr2,
            [TokenKind::Colon].as_slice(),
            "expecting expression after `while` keyword",
        );
        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        let (body, body_range) = self.parse_body();
        range = range.cover(body_range);

        let orelse = if self.eat(TokenKind::Else) {
            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (else_body, else_body_range) = self.parse_body();
            range = range.cover(else_body_range);
            else_body
        } else {
            vec![]
        };

        self.clear_ctx(ParserCtxFlags::WHILE_STMT);
        (
            Stmt::While(ast::StmtWhile {
                test: Box::new(test),
                body,
                orelse,
                range,
            }),
            range,
        )
    }

    fn parse_for_stmt(&mut self) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::FOR_STMT);

        let mut range = self.current_range();
        self.eat(TokenKind::For);

        self.set_ctx(ParserCtxFlags::FOR_TARGET);
        let (mut target, _) = self.parse_expr_with_recovery(
            Parser::parse_exprs,
            [TokenKind::In, TokenKind::Colon].as_slice(),
            "expecting expression after `for` keyword",
        );
        self.clear_ctx(ParserCtxFlags::FOR_TARGET);

        helpers::set_expr_ctx(&mut target, ExprContext::Store);

        self.expect_and_recover(TokenKind::In, TokenSet::new(&[TokenKind::Colon]));

        let (iter, _) = self.parse_expr_with_recovery(
            Parser::parse_exprs,
            EXPR_SET.union([TokenKind::Colon, TokenKind::Indent].as_slice().into()),
            "expecting an expression after `in` keyword",
        );
        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        let (body, body_range) = self.parse_body();
        range = range.cover(body_range);

        let orelse = if self.eat(TokenKind::Else) {
            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (else_body, else_body_range) = self.parse_body();
            range = range.cover(else_body_range);
            else_body
        } else {
            vec![]
        };

        self.clear_ctx(ParserCtxFlags::FOR_STMT);
        (
            Stmt::For(ast::StmtFor {
                target: Box::new(target),
                iter: Box::new(iter),
                is_async: false,
                body,
                orelse,
                range,
            }),
            range,
        )
    }

    fn parse_try_stmt(&mut self) -> StmtWithRange {
        let mut range = self.current_range();
        self.eat(TokenKind::Try);
        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        let mut is_star = false;
        let mut has_except = false;
        let mut has_finally = false;

        let (try_body, _) = self.parse_body();

        let mut handlers = vec![];
        loop {
            let mut except_range = self.current_range();
            if self.eat(TokenKind::Except) {
                has_except = true;
            } else {
                break;
            }

            is_star = self.eat(TokenKind::Star);

            let type_ = if self.at(TokenKind::Colon) && !is_star {
                None
            } else {
                let (expr, expr_range) = self.parse_exprs();
                if !self.last_ctx.contains(ParserCtxFlags::PARENTHESIZED_EXPR)
                    && matches!(expr, Expr::Tuple(_))
                {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "multiple exception types must be parenthesized".to_string(),
                        ),
                        expr_range,
                    );
                }
                Some(Box::new(expr))
            };

            let name = if self.eat(TokenKind::As) {
                Some(self.parse_identifier())
            } else {
                None
            };

            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (except_body, except_body_range) = self.parse_body();

            except_range = except_range.cover(except_body_range);
            range = range.cover(except_range);

            handlers.push(ExceptHandler::ExceptHandler(
                ast::ExceptHandlerExceptHandler {
                    type_,
                    name,
                    body: except_body,
                    range: except_range,
                },
            ));

            if !self.at(TokenKind::Except) {
                break;
            }
        }

        let orelse = if self.eat(TokenKind::Else) {
            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (else_body, else_body_range) = self.parse_body();
            range = range.cover(else_body_range);
            else_body
        } else {
            vec![]
        };

        let finalbody = if self.eat(TokenKind::Finally) {
            has_finally = true;
            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (finally_body, finally_body_range) = self.parse_body();
            range = range.cover(finally_body_range);
            finally_body
        } else {
            vec![]
        };

        if !has_except && !has_finally {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError(
                    "expecting `except` or `finally` after `try` block".to_string(),
                ),
                range,
            );
        }

        (
            Stmt::Try(ast::StmtTry {
                body: try_body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range,
            }),
            range,
        )
    }

    fn parse_decorators(&mut self) -> StmtWithRange {
        let range = self.current_range();
        let mut decorators = vec![];

        while self.at(TokenKind::At) {
            let range = self.current_range();
            self.eat(TokenKind::At);

            let (expression, expr_range) = self.parse_expr2();
            decorators.push(ast::Decorator {
                expression,
                range: range.cover(expr_range),
            });
            self.eat(TokenKind::Newline);
        }

        let (kind, kind_range) = self.current_token();
        match kind {
            TokenKind::Def => self.parse_func_def_stmt(decorators, range),
            TokenKind::Class => self.parse_class_def_stmt(decorators, range),
            TokenKind::Async if self.lookahead(1).0 == TokenKind::Def => {
                let mut async_range = self.current_range();
                self.eat(TokenKind::Async);

                let (Stmt::FunctionDef(mut func), stmt_range) =
                    self.parse_func_def_stmt(decorators, range)
                else {
                    unreachable!()
                };

                async_range = async_range.cover(stmt_range);
                func.range = async_range;
                func.is_async = true;

                (Stmt::FunctionDef(func), async_range)
            }
            _ => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "expected class, function definition or async function definition after decorator".to_string(),
                    ),
                    kind_range,
                );
                self.parse_statement()
            }
        }
    }

    fn parse_func_def_stmt(
        &mut self,
        decorator_list: Vec<ast::Decorator>,
        func_range: TextRange,
    ) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::FUNC_DEF_STMT);

        self.eat(TokenKind::Def);
        let name = self.parse_identifier();
        let type_params = if self.at(TokenKind::Lsqb) {
            Some(self.parse_type_params())
        } else {
            None
        };

        let lpar_range = self.current_range();
        self.expect_and_recover(
            TokenKind::Lpar,
            EXPR_SET.union(
                [TokenKind::Colon, TokenKind::Rarrow, TokenKind::Comma]
                    .as_slice()
                    .into(),
            ),
        );

        let mut parameters = self.parse_parameters();

        let rpar_range = self.current_range();

        self.expect_and_recover(
            TokenKind::Rpar,
            SIMPLE_STMT_SET
                .union(COMPOUND_STMT_SET)
                .union([TokenKind::Colon, TokenKind::Rarrow].as_slice().into()),
        );

        parameters.range = lpar_range.cover(rpar_range);

        let returns = if self.eat(TokenKind::Rarrow) {
            let (returns, range) = self.parse_exprs();
            if self.last_ctx.contains(ParserCtxFlags::TUPLE_EXPR)
                && matches!(returns, Expr::Tuple(_))
            {
                self.add_error(
                    ParseErrorType::OtherError(
                        "multiple return types must be parenthesized".to_string(),
                    ),
                    range,
                );
            }
            Some(Box::new(returns))
        } else {
            None
        };

        self.expect_and_recover(
            TokenKind::Colon,
            SIMPLE_STMT_SET
                .union(COMPOUND_STMT_SET)
                .union([TokenKind::Rarrow].as_slice().into()),
        );

        let (body, body_range) = self.parse_body();
        let range = func_range.cover(body_range);

        self.clear_ctx(ParserCtxFlags::FUNC_DEF_STMT);

        (
            Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                type_params,
                parameters: Box::new(parameters),
                body,
                decorator_list,
                is_async: false,
                returns,
                range,
            }),
            range,
        )
    }

    fn parse_class_def_stmt(
        &mut self,
        decorator_list: Vec<ast::Decorator>,
        class_range: TextRange,
    ) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::CLASS_DEF_STMT);

        self.eat(TokenKind::Class);

        let name = self.parse_identifier();
        let type_params = if self.at(TokenKind::Lsqb) {
            Some(Box::new(self.parse_type_params()))
        } else {
            None
        };
        let arguments = if self.at(TokenKind::Lpar) {
            Some(Box::new(self.parse_arguments()))
        } else {
            None
        };

        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        let (body, body_range) = self.parse_body();
        let range = class_range.cover(body_range);

        self.clear_ctx(ParserCtxFlags::CLASS_DEF_STMT);

        (
            Stmt::ClassDef(ast::StmtClassDef {
                range,
                decorator_list,
                name,
                type_params,
                arguments,
                body,
            }),
            range,
        )
    }

    fn parse_with_item(&mut self) -> ast::WithItem {
        let (context_expr, mut range) = self.parse_expr();
        match context_expr {
            Expr::Starred(_) => {
                self.add_error(
                    ParseErrorType::OtherError("starred expression not allowed".into()),
                    range,
                );
            }
            Expr::NamedExpr(_) if !self.last_ctx.contains(ParserCtxFlags::PARENTHESIZED_EXPR) => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized named expression not allowed".into(),
                    ),
                    range,
                );
            }
            _ => {}
        }

        let optional_vars = if self.eat(TokenKind::As) {
            let (mut target, target_range) = self.parse_expr();
            range = range.cover(target_range);

            if matches!(target, Expr::BoolOp(_) | Expr::Compare(_)) {
                // Should we make `target` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(format!(
                        "expression `{target:?}` not allowed in `with` statement"
                    )),
                    target_range,
                );
            }

            helpers::set_expr_ctx(&mut target, ExprContext::Store);

            Some(Box::new(target))
        } else {
            None
        };

        ast::WithItem {
            range,
            context_expr,
            optional_vars,
        }
    }

    fn parse_with_items(&mut self) -> Vec<ast::WithItem> {
        let mut items = vec![];

        if !self.at_expr() {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("expecting expression after `with` keyword".to_string()),
                range,
            );
            return items;
        }

        let has_seen_lpar = self.at(TokenKind::Lpar);

        // Consider the two `WithItem` examples below:
        //      1) `(a) as A`
        //      2) `(a)`
        //
        // In the first example, the `item` contains a parenthesized expression,
        // while the second example is a parenthesized `WithItem`. This situation
        // introduces ambiguity during parsing. When encountering an opening parenthesis
        // `(,` the parser may initially assume it's parsing a parenthesized `WithItem`.
        // However, this assumption doesn't hold for the first case, `(a) as A`, where
        // `(a)` represents a parenthesized expression.
        //
        // To disambiguate, the following heuristic was created. First, assume we're
        // parsing an expression, then we look for the following tokens:
        //      i) `as` keyword outside parenthesis
        //      ii) `,` outside or inside parenthesis
        //      iii) `:=` inside an 1-level nested parenthesis
        //      iv) `*` inside an 1-level nested parenthesis, representing a starred
        //         expression
        //
        // If we find case i we treat it as in case 1. For case ii, we only treat it as in
        // case 1 if the comma is outside of parenthesis and we've seen an `Rpar` or `Lpar`
        // before the comma.
        // Cases iii and iv are special cases, when we find them, we treat it as in case 2.
        // The reason for this is that the resulting AST node needs to be a tuple for cases
        // iii and iv instead of multiple `WithItem`s. For example, `with (a, b := 0, c): ...`
        // will be parsed as one `WithItem` containing a tuple, instead of three different `WithItem`s.
        let mut treat_it_as_expr = true;
        if has_seen_lpar {
            let mut index = 1;
            let mut paren_nesting = 1;
            let mut ignore_comma_check = false;
            let mut has_seen_rpar = false;
            let mut has_seen_colon_equal = false;
            let mut has_seen_star = false;
            let mut prev_token = self.current_kind();
            loop {
                let (kind, _) = self.lookahead(index);
                match kind {
                    TokenKind::Lpar => {
                        paren_nesting += 1;
                    }
                    TokenKind::Rpar => {
                        paren_nesting -= 1;
                        has_seen_rpar = true;
                    }
                    // Check for `:=` inside an 1-level nested parens, e.g. `with (a, b := c): ...`
                    TokenKind::ColonEqual if paren_nesting == 1 => {
                        treat_it_as_expr = true;
                        ignore_comma_check = true;
                        has_seen_colon_equal = true;
                    }
                    // Check for starred expressions inside an 1-level nested parens,
                    // e.g. `with (a, *b): ...`
                    TokenKind::Star if paren_nesting == 1 && !LITERAL_SET.contains(prev_token) => {
                        treat_it_as_expr = true;
                        ignore_comma_check = true;
                        has_seen_star = true;
                    }
                    // Check for `as` keyword outside parens
                    TokenKind::As => {
                        treat_it_as_expr = paren_nesting == 0;
                        ignore_comma_check = true;
                    }
                    TokenKind::Comma if !ignore_comma_check => {
                        // If the comma is outside of parens, treat it as an expression
                        // if we've seen `(` and `)`.
                        if paren_nesting == 0 {
                            treat_it_as_expr = has_seen_lpar && has_seen_rpar;
                        } else if !has_seen_star && !has_seen_colon_equal {
                            treat_it_as_expr = false;
                        }
                    }
                    TokenKind::Colon | TokenKind::Newline => break,
                    _ => {}
                }

                index += 1;
                prev_token = kind;
            }
        }

        if !treat_it_as_expr && has_seen_lpar {
            self.eat(TokenKind::Lpar);
        }

        let ending = if has_seen_lpar && treat_it_as_expr {
            [TokenKind::Colon]
        } else {
            [TokenKind::Rpar]
        };
        self.parse_separated(
            // Only allow a trailing delimiter if we've seen a `(`.
            has_seen_lpar,
            TokenKind::Comma,
            ending.as_slice(),
            |parser| {
                let item = parser.parse_with_item();
                let range = item.range;
                items.push(item);
                range
            },
        );
        // Special-case: if we have a parenthesized `WithItem` that was parsed as
        // an expression, then the item should _exclude_ the outer parentheses in
        // its range. For example:
        // ```python
        // with (a := 0): pass
        // with (*a): pass
        // with (a): pass
        // with (1 + 2): pass
        // ```
        // In this case, the `(` and `)` are part of the `with` statement.
        // The exception is when `WithItem` is an `()` (empty tuple).
        if items.len() == 1 {
            let with_item = items.last_mut().unwrap();
            if treat_it_as_expr
                && with_item.optional_vars.is_none()
                && self.last_ctx.contains(ParserCtxFlags::PARENTHESIZED_EXPR)
                && !matches!(with_item.context_expr, Expr::Tuple(_))
            {
                with_item.range = with_item.range.add_start(1.into()).sub_end(1.into());
            }
        }

        if !treat_it_as_expr && has_seen_lpar {
            self.expect_and_recover(TokenKind::Rpar, TokenSet::new(&[TokenKind::Colon]));
        }

        items
    }

    fn parse_with_stmt(&mut self) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::WITH_STMT);
        let mut range = self.current_range();

        self.eat(TokenKind::With);

        let items = self.parse_with_items();
        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        let (body, body_range) = self.parse_body();
        range = range.cover(body_range);

        self.clear_ctx(ParserCtxFlags::WITH_STMT);

        (
            Stmt::With(ast::StmtWith {
                items,
                body,
                is_async: false,
                range,
            }),
            range,
        )
    }

    fn parse_assign_stmt(&mut self, target_stmt: Stmt, mut range: TextRange) -> StmtWithRange {
        let Stmt::Expr(target) = target_stmt else {
            unreachable!()
        };

        let mut targets = vec![*target.value];
        let (mut value, value_range) = self.parse_exprs();
        range = range.cover(value_range);

        while self.eat(TokenKind::Equal) {
            let (mut expr, expr_range) = self.parse_exprs();

            std::mem::swap(&mut value, &mut expr);

            range = range.cover(expr_range);
            targets.push(expr);
        }

        targets
            .iter_mut()
            .for_each(|target| helpers::set_expr_ctx(target, ExprContext::Store));

        if !targets.iter().all(helpers::is_valid_assignment_target) {
            targets
                .iter()
                .filter(|target| !helpers::is_valid_assignment_target(target))
                .for_each(|target| self.add_error(ParseErrorType::AssignmentError, target.range()));
        }

        (
            Stmt::Assign(ast::StmtAssign {
                targets,
                value: Box::new(value),
                range,
            }),
            range,
        )
    }

    fn parse_ann_assign_stmt(&mut self, target: Stmt, mut range: TextRange) -> StmtWithRange {
        let Stmt::Expr(mut target) = target else {
            unreachable!()
        };

        if !helpers::is_valid_assignment_target(&target.value) {
            self.add_error(ParseErrorType::AssignmentError, target.range);
        }

        if self.last_ctx.intersects(ParserCtxFlags::TUPLE_EXPR) {
            // Should we make `target` an `Expr::Invalid` here?
            self.add_error(
                ParseErrorType::OtherError(
                    "unparenthesized tuple cannot have type annotation".to_string(),
                ),
                range,
            );
        }

        helpers::set_expr_ctx(&mut target.value, ExprContext::Store);

        let simple = matches!(target.value.as_ref(), Expr::Name(_))
            && !self.last_ctx.intersects(ParserCtxFlags::PARENTHESIZED_EXPR);
        let (annotation, ann_range) = self.parse_expr();
        range = range.cover(ann_range);

        let value = if self.eat(TokenKind::Equal) {
            let (value, value_range) = self.parse_exprs();
            range = range.cover(value_range);

            Some(Box::new(value))
        } else {
            None
        };

        (
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target: target.value,
                annotation: Box::new(annotation),
                value,
                simple,
                range,
            }),
            range,
        )
    }

    fn parse_aug_assign_stmt(
        &mut self,
        target: Stmt,
        op: Operator,
        mut range: TextRange,
    ) -> StmtWithRange {
        // Consume the operator
        self.next_token();
        let Stmt::Expr(mut target) = target else {
            unreachable!()
        };

        if !helpers::is_valid_aug_assignment_target(&target.value) {
            self.add_error(ParseErrorType::AugAssignmentError, target.range);
        }

        helpers::set_expr_ctx(&mut target.value, ExprContext::Store);

        let (value, value_range) = self.parse_exprs();
        range = range.cover(value_range);

        (
            Stmt::AugAssign(ast::StmtAugAssign {
                target: target.value,
                op,
                value: Box::new(value),
                range,
            }),
            range,
        )
    }

    fn parse_simple_stmt_newline(&mut self) -> StmtWithRange {
        let stmt = self.parse_simple_stmt();

        self.last_ctx = ParserCtxFlags::empty();
        let has_eaten_semicolon = self.eat(TokenKind::Semi);
        let has_eaten_newline = self.eat(TokenKind::Newline);

        if !has_eaten_newline && !has_eaten_semicolon && self.at_simple_stmt() {
            let range = self.current_range();
            self.add_error(ParseErrorType::SimpleStmtsInSameLine, stmt.1.cover(range));
        }

        if !has_eaten_newline && self.at_compound_stmt() {
            // Avoid create `SimpleStmtAndCompoundStmtInSameLine` error when the
            // current node is `Expr::Invalid`. Example of when this may happen:
            // ```python
            // ! def x(): ...
            // ```
            // The `!` (an unexpected token) will be parsed as `Expr::Invalid`.
            if let Stmt::Expr(expr) = &stmt.0 {
                if let Expr::Invalid(_) = expr.value.as_ref() {
                    return stmt;
                }
            }
            let range = self.current_range();
            self.add_error(
                ParseErrorType::SimpleStmtAndCompoundStmtInSameLine,
                stmt.1.cover(range),
            );
        }

        stmt
    }

    fn parse_simple_stmts(&mut self) -> (Vec<Stmt>, TextRange) {
        let mut range;
        let mut stmts = vec![];

        loop {
            let (stmt, stmt_range) = self.parse_simple_stmt();
            stmts.push(stmt);
            range = stmt_range;

            if !self.eat(TokenKind::Semi) {
                if self.at_simple_stmt() {
                    for stmt in &stmts {
                        self.add_error(ParseErrorType::SimpleStmtsInSameLine, stmt.range());
                    }
                } else {
                    break;
                }
            }

            if !self.at_simple_stmt() {
                break;
            }
        }

        if !self.eat(TokenKind::Newline) && self.at_compound_stmt() {
            self.add_error(ParseErrorType::SimpleStmtAndCompoundStmtInSameLine, range);
        }

        (stmts, range)
    }

    fn parse_simple_stmt(&mut self) -> StmtWithRange {
        let (kind, range) = self.current_token();
        match kind {
            TokenKind::Del => self.parse_del_stmt(range),
            TokenKind::Pass => self.parse_pass_stmt(range),
            TokenKind::Break => self.parse_break_stmt(range),
            TokenKind::Raise => self.parse_raise_stmt(range),
            TokenKind::Assert => self.parse_assert_stmt(range),
            TokenKind::Global => self.parse_global_stmt(range),
            TokenKind::Import => self.parse_import_stmt(range),
            TokenKind::Return => self.parse_return_stmt(range),
            TokenKind::From => self.parse_import_from_stmt(range),
            TokenKind::Continue => self.parse_continue_stmt(range),
            TokenKind::Nonlocal => self.parse_nonlocal_stmt(range),
            TokenKind::Type => self.parse_type_stmt(range),
            TokenKind::EscapeCommand if self.mode == Mode::Ipython => {
                self.parse_ipython_escape_command_stmt()
            }
            _ => {
                let (expr, expr_range) = self.parse_expr_stmt();

                if self.eat(TokenKind::Equal) {
                    self.parse_assign_stmt(expr, expr_range)
                } else if self.eat(TokenKind::Colon) {
                    self.parse_ann_assign_stmt(expr, expr_range)
                } else if let Ok(op) = Operator::try_from(self.current_kind()) {
                    self.parse_aug_assign_stmt(expr, op, expr_range)
                } else if self.mode == Mode::Ipython && self.at(TokenKind::Question) {
                    let mut kind = IpyEscapeKind::Help;
                    let mut ipy_range = expr_range.cover(self.current_range());

                    self.eat(TokenKind::Question);
                    if self.at(TokenKind::Question) {
                        kind = IpyEscapeKind::Help2;
                        ipy_range = ipy_range.cover(self.current_range());
                        self.eat(TokenKind::Question);
                    }

                    (
                        Stmt::IpyEscapeCommand(ast::StmtIpyEscapeCommand {
                            value: self.src_text(expr_range).to_string(),
                            kind,
                            range: ipy_range,
                        }),
                        ipy_range,
                    )
                } else {
                    (expr, expr_range)
                }
            }
        }
    }

    fn parse_ipython_escape_command_stmt(&mut self) -> StmtWithRange {
        let (Tok::IpyEscapeCommand { value, kind }, range) = self.next_token() else {
            unreachable!()
        };

        (
            Stmt::IpyEscapeCommand(ast::StmtIpyEscapeCommand { range, kind, value }),
            range,
        )
    }

    #[inline]
    fn parse_pass_stmt(&mut self, range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Pass);
        (Stmt::Pass(ast::StmtPass { range }), range)
    }

    #[inline]
    fn parse_continue_stmt(&mut self, range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Continue);
        (Stmt::Continue(ast::StmtContinue { range }), range)
    }

    #[inline]
    fn parse_break_stmt(&mut self, range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Break);
        (Stmt::Break(ast::StmtBreak { range }), range)
    }

    fn parse_del_stmt(&mut self, mut del_range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Del);
        let mut targets = vec![];

        let range = self.parse_separated(
            true,
            TokenKind::Comma,
            [TokenKind::Newline].as_slice(),
            |parser| {
                let (mut target, target_range) = parser.parse_expr();
                helpers::set_expr_ctx(&mut target, ExprContext::Del);

                if matches!(target, Expr::BoolOp(_) | Expr::Compare(_)) {
                    // Should we make `target` an `Expr::Invalid` here?
                    parser.add_error(
                        ParseErrorType::OtherError(format!(
                            "`{}` not allowed in `del` statement",
                            parser.src_text(target_range)
                        )),
                        target_range,
                    );
                }
                targets.push(target);
                target_range
            },
        );
        del_range = del_range.cover(range.unwrap_or(del_range));

        (
            Stmt::Delete(ast::StmtDelete {
                targets,
                range: del_range,
            }),
            del_range,
        )
    }

    fn parse_assert_stmt(&mut self, mut range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Assert);

        let (test, test_range) = self.parse_expr();
        range = range.cover(test_range);

        let msg = if self.eat(TokenKind::Comma) {
            let (msg, msg_range) = self.parse_expr();
            range = range.cover(msg_range);

            Some(Box::new(msg))
        } else {
            None
        };

        (
            Stmt::Assert(ast::StmtAssert {
                test: Box::new(test),
                msg,
                range,
            }),
            range,
        )
    }

    fn parse_global_stmt(&mut self, global_range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Global);

        let mut names = vec![];
        let range = self.parse_separated(
            false,
            TokenKind::Comma,
            [TokenKind::Newline].as_slice(),
            |parser| {
                let ident = parser.parse_identifier();
                let range = ident.range;
                names.push(ident);
                range
            },
        );
        let range = global_range.cover(range.unwrap_or(global_range));

        (Stmt::Global(ast::StmtGlobal { range, names }), range)
    }

    fn parse_nonlocal_stmt(&mut self, nonlocal_range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Nonlocal);

        let mut names = vec![];

        let range = self
            .parse_separated(
                false,
                TokenKind::Comma,
                [TokenKind::Newline].as_slice(),
                |parser| {
                    let ident = parser.parse_identifier();
                    let range = ident.range;
                    names.push(ident);
                    range
                },
            )
            .map_or(nonlocal_range, |range| nonlocal_range.cover(range));

        (Stmt::Nonlocal(ast::StmtNonlocal { range, names }), range)
    }

    fn parse_return_stmt(&mut self, mut range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Return);

        let value = if self.at_expr() {
            let (value, value_range) = self.parse_exprs();
            range = range.cover(value_range);
            Some(Box::new(value))
        } else {
            None
        };

        (Stmt::Return(ast::StmtReturn { range, value }), range)
    }

    fn parse_raise_stmt(&mut self, mut range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Raise);

        let exc = if self.at(TokenKind::Newline) {
            None
        } else {
            let (exc, exc_range) = self.parse_exprs();
            range = range.cover(exc_range);

            Some(Box::new(exc))
        };

        if let Some(Expr::Tuple(node)) = exc.as_deref() {
            if !self.last_ctx.intersects(ParserCtxFlags::PARENTHESIZED_EXPR) {
                // Should we make `exc` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized tuple not allowed in `raise` statement".to_string(),
                    ),
                    node.range,
                );
            }
        }

        let cause = if exc.is_some() && self.eat(TokenKind::From) {
            let (cause, cause_range) = self.parse_exprs();
            range = range.cover(cause_range);

            if let Expr::Tuple(node) = &cause {
                if !self.last_ctx.intersects(ParserCtxFlags::PARENTHESIZED_EXPR) {
                    // Should we make `exc` an `Expr::Invalid` here?
                    self.add_error(
                        ParseErrorType::OtherError(
                            "unparenthesized tuple not allowed in `raise from` statement"
                                .to_string(),
                        ),
                        node.range,
                    );
                }
            }

            Some(Box::new(cause))
        } else {
            None
        };

        (Stmt::Raise(ast::StmtRaise { range, exc, cause }), range)
    }

    fn parse_type_stmt(&mut self, range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Type);

        let (tok, tok_range) = self.next_token();
        let name = if let Tok::Name { name } = tok {
            Expr::Name(ast::ExprName {
                id: name,
                ctx: ExprContext::Store,
                range: tok_range,
            })
        } else {
            self.add_error(
                ParseErrorType::OtherError(format!("expecting identifier, got {tok}")),
                tok_range,
            );
            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(tok_range).into(),
                range: tok_range,
            })
        };
        let type_params = if self.at(TokenKind::Lsqb) {
            Some(self.parse_type_params())
        } else {
            None
        };
        self.expect_and_recover(TokenKind::Equal, EXPR_SET);

        let (value, value_range) = self.parse_expr();
        let range = range.cover(value_range);

        (
            Stmt::TypeAlias(ast::StmtTypeAlias {
                name: Box::new(name),
                type_params,
                value: Box::new(value),
                range,
            }),
            range,
        )
    }

    fn parse_type_params(&mut self) -> ast::TypeParams {
        let mut type_params = vec![];
        let range = self.parse_delimited(
            true,
            TokenKind::Lsqb,
            TokenKind::Comma,
            TokenKind::Rsqb,
            |parser| {
                type_params.push(parser.parse_type_param());
            },
        );

        ast::TypeParams { range, type_params }
    }

    fn parse_type_param(&mut self) -> ast::TypeParam {
        let mut range = self.current_range();
        if self.eat(TokenKind::Star) {
            let name = self.parse_identifier();
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                range: range.cover(name.range),
                name,
            })
        } else if self.eat(TokenKind::DoubleStar) {
            let name = self.parse_identifier();
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                range: range.cover(name.range),
                name,
            })
        } else {
            let name = self.parse_identifier();
            let bound = if self.eat(TokenKind::Colon) {
                let (bound, bound_range) = self.parse_expr();
                range = range.cover(bound_range);
                Some(Box::new(bound))
            } else {
                None
            };
            ast::TypeParam::TypeVar(ast::TypeParamTypeVar { range, name, bound })
        }
    }

    fn parse_dotted_name(&mut self) -> ast::Identifier {
        let id = self.parse_identifier();
        let mut range = id.range;

        while self.eat(TokenKind::Dot) {
            let id = self.parse_identifier();
            if !id.is_valid() {
                self.add_error(
                    ParseErrorType::OtherError("invalid identifier".into()),
                    id.range,
                );
            }
            range = range.cover(id.range);
        }

        ast::Identifier {
            id: self.src_text(range).into(),
            range,
        }
    }

    fn parse_alias(&mut self) -> ast::Alias {
        let (kind, mut range) = self.current_token();
        if kind == TokenKind::Star {
            self.eat(TokenKind::Star);
            return ast::Alias {
                name: ast::Identifier {
                    id: "*".into(),
                    range,
                },
                asname: None,
                range,
            };
        }

        let name = self.parse_dotted_name();
        range = range.cover(name.range);

        let asname = if self.eat(TokenKind::As) {
            let id = self.parse_identifier();
            range = range.cover(id.range);
            Some(id)
        } else {
            None
        };

        ast::Alias {
            range,
            name,
            asname,
        }
    }

    fn parse_import_stmt(&mut self, import_range: TextRange) -> StmtWithRange {
        self.eat(TokenKind::Import);

        let mut names = vec![];
        let range = self
            .parse_separated(
                false,
                TokenKind::Comma,
                [TokenKind::Newline].as_slice(),
                |parser| {
                    let alias = parser.parse_alias();
                    let range = alias.range;
                    names.push(alias);
                    range
                },
            )
            .map_or(import_range, |range| import_range.cover(range));

        (Stmt::Import(ast::StmtImport { range, names }), range)
    }

    fn parse_import_from_stmt(&mut self, from_range: TextRange) -> StmtWithRange {
        const DOT_ELLIPSIS_SET: TokenSet = TokenSet::new(&[TokenKind::Dot, TokenKind::Ellipsis]);
        self.eat(TokenKind::From);

        let mut module = None;
        let mut level = if self.eat(TokenKind::Ellipsis) { 3 } else { 0 };

        while self.at_ts(DOT_ELLIPSIS_SET) {
            if self.eat(TokenKind::Dot) {
                level += 1;
            }

            if self.eat(TokenKind::Ellipsis) {
                level += 3;
            }
        }

        if self.at(TokenKind::Name) {
            module = Some(self.parse_dotted_name());
        };

        if level == 0 && module.is_none() {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("missing module name".to_string()),
                range,
            );
        }

        self.expect_and_recover(TokenKind::Import, TokenSet::EMPTY);

        let mut names = vec![];
        let range = if self.at(TokenKind::Lpar) {
            let delim_range = self.parse_delimited(
                true,
                TokenKind::Lpar,
                TokenKind::Comma,
                TokenKind::Rpar,
                |parser| {
                    names.push(parser.parse_alias());
                },
            );
            from_range.cover(delim_range)
        } else {
            self.parse_separated(
                false,
                TokenKind::Comma,
                [TokenKind::Newline].as_slice(),
                |parser| {
                    let alias = parser.parse_alias();
                    let range = alias.range;
                    names.push(alias);
                    range
                },
            )
            .map_or(from_range, |range| from_range.cover(range))
        };

        (
            Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level: Some(level),
                range,
            }),
            range,
        )
    }

    const ELSE_ELIF_SET: TokenSet = TokenSet::new(&[TokenKind::Else, TokenKind::Elif]);
    fn parse_if_stmt(&mut self) -> StmtWithRange {
        self.set_ctx(ParserCtxFlags::IF_STMT);
        let mut if_range = self.current_range();
        assert!(self.eat(TokenKind::If));

        let (test, _) = self.parse_expr_with_recovery(
            Parser::parse_expr2,
            [TokenKind::Colon].as_slice(),
            "expecting expression after `if` keyword",
        );
        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        let (body, body_range) = self.parse_body();
        if_range = if_range.cover(body_range);

        let elif_else_clauses = if self.at_ts(Self::ELSE_ELIF_SET) {
            let (elif_else_clauses, range) = self.parse_elif_else_clauses();
            if_range = if_range.cover(range);

            elif_else_clauses
        } else {
            vec![]
        };

        self.clear_ctx(ParserCtxFlags::IF_STMT);
        (
            Stmt::If(ast::StmtIf {
                test: Box::new(test),
                body,
                elif_else_clauses,
                range: if_range,
            }),
            if_range,
        )
    }

    fn parse_elif_else_clauses(&mut self) -> (Vec<ast::ElifElseClause>, TextRange) {
        let mut elif_else_stmts = vec![];
        let mut range = self.current_range();
        while self.at(TokenKind::Elif) {
            let elif_range = self.current_range();
            self.eat(TokenKind::Elif);

            let (test, _) = self.parse_expr_with_recovery(
                Parser::parse_expr2,
                [TokenKind::Colon].as_slice(),
                "expecting expression after `elif` keyword",
            );
            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (body, body_range) = self.parse_body();
            range = body_range;
            elif_else_stmts.push(ast::ElifElseClause {
                test: Some(test),
                body,
                range: elif_range.cover(body_range),
            });
        }

        if self.at(TokenKind::Else) {
            let else_range = self.current_range();
            self.eat(TokenKind::Else);
            self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

            let (body, body_range) = self.parse_body();
            range = body_range;
            elif_else_stmts.push(ast::ElifElseClause {
                test: None,
                body,
                range: else_range.cover(body_range),
            });
        }

        (elif_else_stmts, range)
    }

    fn parse_body(&mut self) -> (Vec<Stmt>, TextRange) {
        let mut last_stmt_range = TextRange::default();
        let mut stmts = vec![];

        // Check if we are currently at a simple statement
        if !self.eat(TokenKind::Newline) && self.at_simple_stmt() {
            return self.parse_simple_stmts();
        }

        if self.eat(TokenKind::Indent) {
            const BODY_END_SET: TokenSet =
                TokenSet::new(&[TokenKind::Dedent]).union(NEWLINE_EOF_SET);
            while !self.at_ts(BODY_END_SET) {
                if self.at(TokenKind::Indent) {
                    last_stmt_range = self.handle_unexpected_indentation(
                        &mut stmts,
                        "indentation doesn't match previous indentation",
                    );
                    continue;
                }
                let (stmt, stmt_range) = self.parse_statement();
                last_stmt_range = stmt_range;
                stmts.push(stmt);
            }

            self.eat(TokenKind::Dedent);
        } else {
            let ctx_str = match self.ctx {
                ParserCtxFlags::IF_STMT => Some("`if` statement"),
                ParserCtxFlags::FOR_STMT => Some("`for` statement"),
                ParserCtxFlags::WITH_STMT => Some("`with` statement"),
                ParserCtxFlags::WHILE_STMT => Some("`while` statement"),
                ParserCtxFlags::CLASS_DEF_STMT => Some("`class` definition"),
                ParserCtxFlags::FUNC_DEF_STMT => Some("function definition"),
                _ => None,
            };
            if let Some(ctx_str) = ctx_str {
                let range = self.current_range();
                self.add_error(
                    ParseErrorType::OtherError(format!(
                        "expected an indented block after {ctx_str}"
                    )),
                    range,
                );
            }
        }

        (stmts, last_stmt_range)
    }

    fn parse_expr_stmt(&mut self) -> StmtWithRange {
        let (expr, range) = self.parse_exprs();

        (
            Stmt::Expr(ast::StmtExpr {
                value: Box::new(expr),
                range,
            }),
            range,
        )
    }

    /// Parses every Python expression.
    fn parse_exprs(&mut self) -> ExprWithRange {
        let (expr, expr_range) = self.parse_expr();

        if self.at(TokenKind::Comma) {
            return self.parse_tuple_expr(expr, expr_range, Parser::parse_expr);
        }
        (expr, expr_range)
    }

    /// Parses every Python expression except unparenthesized tuple and named expressions.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    fn parse_expr(&mut self) -> ExprWithRange {
        let (expr, expr_range) = self.parse_expr_simple();

        if self.at(TokenKind::If) {
            return self.parse_if_expr(expr, expr_range);
        }

        (expr, expr_range)
    }

    /// Parses every Python expression except unparenthesized tuple.
    ///
    /// NOTE: If you have expressions separated by commas and want to parse them individually,
    /// instead of a tuple, use this function!
    fn parse_expr2(&mut self) -> ExprWithRange {
        let (expr, expr_range) = self.parse_expr();

        if self.at(TokenKind::ColonEqual) {
            return self.parse_named_expr(expr, expr_range);
        }

        (expr, expr_range)
    }

    /// Parses every Python expression except unparenthesized tuple and `if` expression.
    fn parse_expr_simple(&mut self) -> ExprWithRange {
        self.expr_bp(1)
    }

    /// Tries to parse an expression (using `parse_func`), and recovers from
    /// errors by skipping until a specified set of tokens.
    ///
    /// If the current token is not part of an expression, adds the `error_msg`
    /// to the list of errors and returns an `Expr::Invalid`.
    fn parse_expr_with_recovery(
        &mut self,
        mut parse_func: impl FnMut(&mut Parser<'src, 'src_path, I>) -> ExprWithRange,
        recover_set: impl Into<TokenSet>,
        error_msg: impl Display,
    ) -> ExprWithRange {
        if self.at_expr() {
            parse_func(self)
        } else {
            let range = self.current_range();
            self.add_error(ParseErrorType::OtherError(error_msg.to_string()), range);
            self.skip_until(NEWLINE_EOF_SET.union(recover_set.into()));

            (
                Expr::Invalid(ast::ExprInvalid {
                    value: self.src_text(range).into(),
                    range,
                }),
                range,
            )
        }
    }

    /// Binding powers of operators for a Pratt parser.
    ///
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    fn current_op(&mut self) -> (u8, TokenKind, Associativity) {
        const NOT_AN_OP: (u8, TokenKind, Associativity) =
            (0, TokenKind::Invalid, Associativity::Left);
        let kind = self.current_kind();

        match kind {
            TokenKind::Or => (4, kind, Associativity::Left),
            TokenKind::And => (5, kind, Associativity::Left),
            TokenKind::Not if self.lookahead(1).0 == TokenKind::In => {
                (7, kind, Associativity::Left)
            }
            TokenKind::Is
            | TokenKind::In
            | TokenKind::EqEqual
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual => (7, kind, Associativity::Left),
            TokenKind::Vbar => (8, kind, Associativity::Left),
            TokenKind::CircumFlex => (9, kind, Associativity::Left),
            TokenKind::Amper => (10, kind, Associativity::Left),
            TokenKind::LeftShift | TokenKind::RightShift => (11, kind, Associativity::Left),
            TokenKind::Plus | TokenKind::Minus => (12, kind, Associativity::Left),
            TokenKind::Star
            | TokenKind::Slash
            | TokenKind::DoubleSlash
            | TokenKind::Percent
            | TokenKind::At => (14, kind, Associativity::Left),
            TokenKind::DoubleStar => (18, kind, Associativity::Right),
            _ => NOT_AN_OP,
        }
    }

    /// Parses expression with binding power of at least bp.
    ///
    /// Uses the Pratt parser algorithm.
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>
    fn expr_bp(&mut self, bp: u8) -> ExprWithRange {
        let (mut lhs, mut lhs_range) = self.parse_lhs();

        loop {
            let (op_bp, op, associativity) = self.current_op();
            if op_bp < bp {
                break;
            }

            // Don't parse a `CompareExpr` if we are parsing a `Comprehension` or `ForStmt`
            if op.is_compare_operator()
                && self.has_in_curr_or_parent_ctx(ParserCtxFlags::FOR_TARGET)
            {
                break;
            }

            let op_bp = match associativity {
                Associativity::Left => op_bp + 1,
                Associativity::Right => op_bp,
            };

            self.eat(op);

            // We need to create a dedicated node for boolean operations,
            // even though boolean operations are infix.
            if op.is_bool_operator() {
                (lhs, lhs_range) = self.parse_bool_op_expr(lhs, lhs_range, op, op_bp);
                continue;
            }

            // Same here as well
            if op.is_compare_operator() {
                (lhs, lhs_range) = self.parse_compare_op_expr(lhs, lhs_range, op, op_bp);
                continue;
            }

            let (rhs, rhs_range) = if self.at_expr() {
                self.expr_bp(op_bp)
            } else {
                let rhs_range = self.current_range();
                self.add_error(
                    ParseErrorType::OtherError("expecting an expression after operand".into()),
                    rhs_range,
                );
                (
                    Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(rhs_range).into(),
                        range: rhs_range,
                    }),
                    rhs_range,
                )
            };
            lhs_range = lhs_range.cover(rhs_range);
            lhs = Expr::BinOp(ast::ExprBinOp {
                left: Box::new(lhs),
                op: Operator::try_from(op).unwrap(),
                right: Box::new(rhs),
                range: lhs_range,
            });
        }

        (lhs, lhs_range)
    }

    fn parse_lhs(&mut self) -> ExprWithRange {
        let token = self.next_token();
        let (mut lhs, mut lhs_range) = match token.0 {
            Tok::Plus | Tok::Minus | Tok::Not | Tok::Tilde => self.parse_unary_expr(token),
            Tok::Star => self.parse_starred_expr(token),
            Tok::Await => self.parse_await_expr(token.1),
            Tok::Lambda => self.parse_lambda_expr(token.1),
            _ => self.parse_atom(token),
        };

        if self.is_current_token_postfix() {
            (lhs, lhs_range) = self.parse_postfix_expr(lhs, lhs_range);
        }

        (lhs, lhs_range)
    }

    #[inline]
    fn parse_identifier(&mut self) -> ast::Identifier {
        let (tok, range) = self.next_token();
        if let Tok::Name { name } = tok {
            ast::Identifier { id: name, range }
        } else {
            ast::Identifier {
                id: String::new(),
                range,
            }
        }
    }

    fn parse_atom(&mut self, token: Spanned) -> ExprWithRange {
        let (tok, mut range) = token;
        let lhs = match tok {
            Tok::Float { value } => Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Float(value),
                range,
            }),
            Tok::Complex { real, imag } => Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Complex { real, imag },
                range,
            }),
            Tok::Int { value } => Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Int(value),
                range,
            }),
            Tok::True => Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, range }),
            Tok::False => Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: false,
                range,
            }),
            Tok::None => Expr::NoneLiteral(ast::ExprNoneLiteral { range }),
            Tok::Ellipsis => Expr::EllipsisLiteral(ast::ExprEllipsisLiteral { range }),
            Tok::Name { name } => Expr::Name(ast::ExprName {
                id: name,
                ctx: ExprContext::Load,
                range,
            }),
            Tok::IpyEscapeCommand { value, kind } if self.mode == Mode::Ipython => {
                Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand { range, kind, value })
            }
            tok @ Tok::String { .. } => return self.parse_string_expr(tok, range),
            Tok::FStringStart => return self.parse_fstring_expr(range),
            Tok::Lpar => return self.parse_parenthesized_expr(range),
            Tok::Lsqb => return self.parse_bracketsized_expr(range),
            Tok::Lbrace => return self.parse_bracesized_expr(range),
            Tok::Yield => return self.parse_yield_expr(range),
            // `Invalid` tokens are created when there's a lexical error, to
            // avoid creating an "unexpected token" error for `Tok::Invalid`
            // we handle it here. We try to parse an expression to avoid
            // creating "statements in the same line" error in some cases.
            Tok::Invalid => {
                if self.at_expr() {
                    let (expr, expr_range) = self.parse_exprs();
                    range = expr_range;
                    expr
                } else {
                    Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(range).into(),
                        range,
                    })
                }
            }
            // Handle unexpected token
            tok => {
                // Try to parse an expression after seeing an unexpected token
                let lhs = if self.at_expr() {
                    let (expr, expr_range) = self.parse_exprs();
                    range = expr_range;
                    expr
                } else {
                    Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(range).into(),
                        range,
                    })
                };

                if matches!(tok, Tok::IpyEscapeCommand { .. }) {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "IPython escape commands are only allowed in `Mode::Ipython`".into(),
                        ),
                        range,
                    );
                } else {
                    self.add_error(
                        ParseErrorType::OtherError(format!("unexpected token `{tok}`")),
                        range,
                    );
                }
                lhs
            }
        };

        (lhs, range)
    }

    fn parse_postfix_expr(&mut self, mut lhs: Expr, mut lhs_range: TextRange) -> ExprWithRange {
        loop {
            (lhs, lhs_range) = match self.current_kind() {
                TokenKind::Lpar => self.parse_call_expr(lhs, lhs_range),
                TokenKind::Lsqb => self.parse_subscript_expr(lhs, lhs_range),
                TokenKind::Dot => self.parse_attribute_expr(lhs, lhs_range),
                _ => break,
            };
        }

        (lhs, lhs_range)
    }

    fn parse_call_expr(&mut self, lhs: Expr, lhs_range: TextRange) -> ExprWithRange {
        assert!(self.at(TokenKind::Lpar));
        let arguments = self.parse_arguments();
        let range = lhs_range.cover(arguments.range);

        (
            Expr::Call(ast::ExprCall {
                func: Box::new(lhs),
                arguments,
                range,
            }),
            range,
        )
    }

    fn parse_arguments(&mut self) -> ast::Arguments {
        self.set_ctx(ParserCtxFlags::ARGUMENTS);

        let mut args: Vec<Expr> = vec![];
        let mut keywords: Vec<ast::Keyword> = vec![];
        let mut has_seen_kw_arg = false;
        let mut has_seen_kw_unpack = false;

        let range = self.parse_delimited(
            true,
            TokenKind::Lpar,
            TokenKind::Comma,
            TokenKind::Rpar,
            |parser| {
                if parser.at(TokenKind::DoubleStar) {
                    let range = parser.current_range();
                    parser.eat(TokenKind::DoubleStar);

                    let (expr, expr_range) = parser.parse_expr();
                    keywords.push(ast::Keyword {
                        arg: None,
                        value: expr,
                        range: range.cover(expr_range),
                    });

                    has_seen_kw_unpack = true;
                } else {
                    let (mut expr, expr_range) = parser.parse_expr2();

                    match parser.current_kind() {
                        TokenKind::Async | TokenKind::For => {
                            (expr, _) = parser.parse_generator_expr(expr, expr_range);
                        }
                        _ => {}
                    }

                    if has_seen_kw_unpack && matches!(expr, Expr::Starred(_)) {
                        parser.add_error(ParseErrorType::UnpackedArgumentError, expr_range);
                    }

                    if parser.eat(TokenKind::Equal) {
                        has_seen_kw_arg = true;
                        let arg = if let Expr::Name(ident_expr) = expr {
                            ast::Identifier {
                                id: ident_expr.id,
                                range: ident_expr.range,
                            }
                        } else {
                            parser.add_error(
                                ParseErrorType::OtherError(format!(
                                    "`{}` cannot be used as a keyword argument!",
                                    parser.src_text(expr_range)
                                )),
                                expr_range,
                            );
                            ast::Identifier {
                                id: String::new(),
                                range: expr_range,
                            }
                        };

                        let (value, value_range) = parser.parse_expr();

                        keywords.push(ast::Keyword {
                            arg: Some(arg),
                            value,
                            range: expr_range.cover(value_range),
                        });
                    } else {
                        if has_seen_kw_arg
                            && !(has_seen_kw_unpack || matches!(expr, Expr::Starred(_)))
                        {
                            parser.add_error(ParseErrorType::PositionalArgumentError, expr_range);
                        }
                        args.push(expr);
                    }
                }
            },
        );
        self.clear_ctx(ParserCtxFlags::ARGUMENTS);

        let arguments = ast::Arguments {
            range,
            args,
            keywords,
        };

        if let Err(error) = helpers::validate_arguments(&arguments, self.source_path) {
            self.errors.push(error);
        }

        arguments
    }

    fn parse_subscript_expr(&mut self, mut value: Expr, value_range: TextRange) -> ExprWithRange {
        assert!(self.eat(TokenKind::Lsqb));

        // To prevent the `value` context from being `Del` within a `del` statement,
        // we set the context as `Load` here.
        helpers::set_expr_ctx(&mut value, ExprContext::Load);

        // Create an error when receiving a empty slice to parse, e.g. `l[]`
        if !self.at(TokenKind::Colon) && !self.at_expr() {
            let close_bracket_range = self.current_range();
            self.expect_and_recover(TokenKind::Rsqb, TokenSet::EMPTY);

            let range = value_range.cover(close_bracket_range);
            let slice_range = close_bracket_range.sub_start(1.into());
            self.add_error(ParseErrorType::EmptySlice, range);
            return (
                Expr::Subscript(ast::ExprSubscript {
                    value: Box::new(value),
                    slice: Box::new(Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(slice_range).into(),
                        range: slice_range,
                    })),
                    ctx: ExprContext::Load,
                    range,
                }),
                range,
            );
        }

        let (mut slice, slice_range) = self.parse_slice();

        if self.at(TokenKind::Comma) {
            let (_, comma_range) = self.next_token();
            let mut slices = vec![slice];
            let slices_range = self
                .parse_separated(
                    true,
                    TokenKind::Comma,
                    TokenSet::new(&[TokenKind::Rsqb]),
                    |parser| {
                        let (slice, slice_range) = parser.parse_slice();
                        slices.push(slice);
                        slice_range
                    },
                )
                .unwrap_or(comma_range);

            slice = Expr::Tuple(ast::ExprTuple {
                elts: slices,
                ctx: ExprContext::Load,
                range: slice_range.cover(slices_range),
            });
        }

        let end_range = self.current_range();
        self.expect_and_recover(TokenKind::Rsqb, TokenSet::EMPTY);

        let range = value_range.cover(end_range);
        (
            Expr::Subscript(ast::ExprSubscript {
                value: Box::new(value),
                slice: Box::new(slice),
                ctx: ExprContext::Load,
                range,
            }),
            range,
        )
    }

    const UPPER_END_SET: TokenSet =
        TokenSet::new(&[TokenKind::Comma, TokenKind::Colon, TokenKind::Rsqb])
            .union(NEWLINE_EOF_SET);
    const STEP_END_SET: TokenSet =
        TokenSet::new(&[TokenKind::Comma, TokenKind::Rsqb]).union(NEWLINE_EOF_SET);
    fn parse_slice(&mut self) -> ExprWithRange {
        let mut range = self.current_range();
        let lower = if self.at_expr() {
            let (expr, expr_range) = self.parse_expr2();
            range = range.cover(expr_range);
            Some(expr)
        } else {
            None
        };

        if self.at(TokenKind::Colon)
            && (lower.is_none()
                || lower
                    .as_ref()
                    .is_some_and(|expr| !matches!(expr, Expr::NamedExpr(_))))
        {
            let (_, colon_range) = self.next_token();
            range = range.cover(colon_range);
            let lower = lower.map(Box::new);
            let upper = if self.at_ts(Self::UPPER_END_SET) {
                None
            } else {
                let (upper, upper_range) = self.parse_expr();
                range = range.cover(upper_range);
                Some(Box::new(upper))
            };

            let colon_range = self.current_range();
            let step = if self.eat(TokenKind::Colon) {
                range = range.cover(colon_range);
                if self.at_ts(Self::STEP_END_SET) {
                    None
                } else {
                    let (step, step_range) = self.parse_expr();
                    range = range.cover(step_range);
                    Some(Box::new(step))
                }
            } else {
                None
            };

            (
                Expr::Slice(ast::ExprSlice {
                    range,
                    lower,
                    upper,
                    step,
                }),
                range,
            )
        } else {
            (lower.unwrap(), range)
        }
    }

    fn parse_unary_expr(&mut self, (op_tok, range): Spanned) -> ExprWithRange {
        let (rhs, rhs_range) = if matches!(op_tok, Tok::Not) {
            self.expr_bp(6)
        } else {
            // plus, minus and tilde
            self.expr_bp(17)
        };
        let new_range = range.cover(rhs_range);

        (
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: UnaryOp::try_from(op_tok).unwrap(),
                operand: Box::new(rhs),
                range: new_range,
            }),
            new_range,
        )
    }

    fn parse_attribute_expr(&mut self, value: Expr, lhs_range: TextRange) -> ExprWithRange {
        assert!(self.eat(TokenKind::Dot));

        let attr = self.parse_identifier();
        let range = lhs_range.cover(attr.range);

        (
            Expr::Attribute(ast::ExprAttribute {
                value: Box::new(value),
                attr,
                ctx: ExprContext::Load,
                range,
            }),
            range,
        )
    }

    fn parse_bool_op_expr(
        &mut self,
        lhs: Expr,
        mut lhs_range: TextRange,
        op: TokenKind,
        op_bp: u8,
    ) -> ExprWithRange {
        let mut values = vec![lhs];

        // Keep adding `expr` to `values` until we see a different
        // boolean operation than `op`.
        loop {
            let (expr, expr_range) = self.expr_bp(op_bp);
            lhs_range = lhs_range.cover(expr_range);
            values.push(expr);

            if self.current_kind() != op {
                break;
            }

            self.next_token();
        }

        (
            Expr::BoolOp(ast::ExprBoolOp {
                values,
                op: BoolOp::try_from(op).unwrap(),
                range: lhs_range,
            }),
            lhs_range,
        )
    }

    fn parse_compare_op_expr(
        &mut self,
        lhs: Expr,
        mut lhs_range: TextRange,
        op: TokenKind,
        op_bp: u8,
    ) -> ExprWithRange {
        let mut comparators = vec![];
        let op = token_kind_to_cmp_op([op, self.current_kind()]).unwrap();
        let mut ops = vec![op];

        if matches!(op, CmpOp::IsNot | CmpOp::NotIn) {
            self.next_token();
        }

        loop {
            let (expr, expr_range) = self.expr_bp(op_bp);
            lhs_range = lhs_range.cover(expr_range);
            comparators.push(expr);

            if let Ok(op) = token_kind_to_cmp_op([self.current_kind(), self.lookahead(1).0]) {
                if matches!(op, CmpOp::IsNot | CmpOp::NotIn) {
                    self.next_token();
                }

                ops.push(op);
            } else {
                break;
            }

            self.next_token();
        }

        (
            Expr::Compare(ast::ExprCompare {
                left: Box::new(lhs),
                ops,
                comparators,
                range: lhs_range,
            }),
            lhs_range,
        )
    }

    fn parse_string_expr(&mut self, mut tok: Tok, mut str_range: TextRange) -> ExprWithRange {
        let mut final_range = str_range;
        let mut strings = vec![];
        while let Tok::String {
            value,
            kind,
            triple_quoted,
        } = tok
        {
            match parse_string_literal(&value, kind, triple_quoted, str_range) {
                Ok(string) => {
                    strings.push(string);
                }
                Err(error) => {
                    strings.push(StringType::Invalid(ast::StringLiteral {
                        value,
                        range: str_range,
                        unicode: kind.is_unicode(),
                    }));
                    self.add_error(ParseErrorType::Lexical(error.error), error.location);
                }
            }

            if !self.at(TokenKind::String) {
                break;
            }

            (tok, str_range) = self.next_token();
            final_range = final_range.cover(str_range);
        }

        // This handles the case where the string is implicit concatenated with
        // a fstring, e.g., `"hello " f"{x}"`.
        if self.at(TokenKind::FStringStart) {
            let mut fstring_range = self.current_range();
            self.handle_implicit_concatenated_strings(&mut fstring_range, &mut strings);
            final_range = final_range.cover(fstring_range);
        }

        if strings.len() == 1 {
            return match strings.pop().unwrap() {
                StringType::Str(string) => {
                    let range = string.range;
                    (
                        Expr::StringLiteral(ast::ExprStringLiteral {
                            value: ast::StringLiteralValue::single(string),
                            range,
                        }),
                        range,
                    )
                }
                StringType::Bytes(bytes) => {
                    let range = bytes.range;
                    (
                        Expr::BytesLiteral(ast::ExprBytesLiteral {
                            value: ast::BytesLiteralValue::single(bytes),
                            range,
                        }),
                        range,
                    )
                }
                StringType::Invalid(invalid) => (
                    Expr::Invalid(ast::ExprInvalid {
                        value: invalid.value,
                        range: invalid.range,
                    }),
                    invalid.range,
                ),
                StringType::FString(_) => unreachable!(),
            };
        }

        match concatenated_strings(strings, final_range) {
            Ok(string) => (string, final_range),
            Err(error) => {
                self.add_error(ParseErrorType::Lexical(error.error), error.location);
                (
                    Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(error.location).into(),
                        range: error.location,
                    }),
                    error.location,
                )
            }
        }
    }

    const FSTRING_SET: TokenSet = TokenSet::new(&[TokenKind::FStringStart, TokenKind::String]);
    /// Handles implicit concatenated f-strings, e.g. `f"{x}" f"hello"`, and
    /// implicit concatenated f-strings with strings, e.g. `f"{x}" "xyz" f"{x}"`.
    fn handle_implicit_concatenated_strings(
        &mut self,
        fstring_range: &mut TextRange,
        strings: &mut Vec<StringType>,
    ) {
        while self.at_ts(Self::FSTRING_SET) {
            if self.at(TokenKind::FStringStart) {
                let (_, range) = self.next_token();
                let fstring = self.parse_fstring(range);
                *fstring_range = fstring_range.cover(fstring.range);
                strings.push(StringType::FString(fstring));
            } else {
                let (
                    Tok::String {
                        value,
                        kind,
                        triple_quoted,
                    },
                    str_range,
                ) = self.next_token()
                else {
                    unreachable!()
                };

                match parse_string_literal(&value, kind, triple_quoted, str_range) {
                    Ok(string) => {
                        *fstring_range = fstring_range.cover(str_range);
                        strings.push(string);
                    }
                    Err(error) => {
                        strings.push(StringType::Invalid(ast::StringLiteral {
                            value,
                            range: str_range,
                            unicode: kind.is_unicode(),
                        }));
                        self.add_error(ParseErrorType::Lexical(error.error), error.location);
                    }
                }
            }
        }
    }

    fn parse_fstring_expr(&mut self, mut fstring_range: TextRange) -> ExprWithRange {
        let fstring = self.parse_fstring(fstring_range);

        if !self.at_ts(Self::FSTRING_SET) {
            let range = fstring.range;
            return (
                Expr::FString(ast::ExprFString {
                    value: ast::FStringValue::single(fstring),
                    range,
                }),
                range,
            );
        }

        let mut strings = vec![StringType::FString(fstring)];
        self.handle_implicit_concatenated_strings(&mut fstring_range, &mut strings);

        match concatenated_strings(strings, fstring_range) {
            Ok(string) => (string, fstring_range),
            Err(error) => {
                self.add_error(ParseErrorType::Lexical(error.error), error.location);
                (
                    Expr::Invalid(ast::ExprInvalid {
                        value: self.src_text(error.location).into(),
                        range: error.location,
                    }),
                    error.location,
                )
            }
        }
    }

    fn parse_fstring(&mut self, mut fstring_range: TextRange) -> ast::FString {
        let (elements, _) = self.parse_fstring_elements();

        fstring_range = fstring_range.cover(self.current_range());
        self.eat(TokenKind::FStringEnd);

        ast::FString {
            elements,
            range: fstring_range,
        }
    }

    const FSTRING_END_SET: TokenSet =
        TokenSet::new(&[TokenKind::FStringEnd, TokenKind::Rbrace]).union(NEWLINE_EOF_SET);
    fn parse_fstring_elements(&mut self) -> (Vec<FStringElement>, TextRange) {
        let mut elements = vec![];
        let mut final_range: Option<TextRange> = None;
        while !self.at_ts(Self::FSTRING_END_SET) {
            let element = match self.current_kind() {
                TokenKind::Lbrace => {
                    let fstring_expr = self.parse_fstring_expr_element();
                    let range = final_range.get_or_insert(fstring_expr.range);
                    *range = range.cover(fstring_expr.range);
                    FStringElement::Expression(fstring_expr)
                }
                TokenKind::FStringMiddle => {
                    let (Tok::FStringMiddle { value, is_raw }, range) = self.next_token() else {
                        unreachable!()
                    };
                    let (fstring_literal, fstring_range) =
                        match parse_fstring_literal_element(&value, is_raw, range) {
                            Ok(fstring) => {
                                let range = fstring.range();
                                (fstring, range)
                            }
                            Err(lex_error) => {
                                self.add_error(
                                    ParseErrorType::Lexical(lex_error.error),
                                    lex_error.location,
                                );
                                (
                                    ast::FStringElement::Invalid(ast::FStringInvalidElement {
                                        value: self.src_text(lex_error.location).into(),
                                        range: lex_error.location,
                                    }),
                                    lex_error.location,
                                )
                            }
                        };
                    let range = final_range.get_or_insert(fstring_range);
                    *range = range.cover(fstring_range);
                    fstring_literal
                }
                // `Invalid` tokens are created when there's a lexical error, so
                // we ignore it here to avoid creating unexpected token errors
                TokenKind::Invalid => {
                    self.next_token();
                    continue;
                }
                // Handle an unexpected token
                _ => {
                    let (tok, range) = self.next_token();
                    self.add_error(
                        ParseErrorType::OtherError(format!("f-string: unexpected token `{tok:?}`")),
                        range,
                    );
                    continue;
                }
            };
            elements.push(element);
        }

        (elements, final_range.unwrap_or_default())
    }

    fn parse_fstring_expr_element(&mut self) -> ast::FStringExpressionElement {
        let range = self.current_range();

        let has_open_brace = self.eat(TokenKind::Lbrace);
        let (value, value_range) = self.parse_expr_with_recovery(
            Parser::parse_exprs,
            [
                TokenKind::Exclamation,
                TokenKind::Colon,
                TokenKind::Rbrace,
                TokenKind::FStringEnd,
            ]
            .as_slice(),
            "f-string: expecting expression",
        );
        if !self.last_ctx.contains(ParserCtxFlags::PARENTHESIZED_EXPR)
            && matches!(value, Expr::Lambda(_))
        {
            self.add_error(
                ParseErrorType::FStringError(FStringErrorType::LambdaWithoutParentheses),
                value_range,
            );
        }
        let debug_text = if self.eat(TokenKind::Equal) {
            let leading_range = range
                .add_start("{".text_len())
                .cover_offset(value_range.start());
            let trailing_range = TextRange::new(value_range.end(), self.current_range().start());
            Some(ast::DebugText {
                leading: self.src_text(leading_range).to_string(),
                trailing: self.src_text(trailing_range).to_string(),
            })
        } else {
            None
        };

        let conversion = if self.eat(TokenKind::Exclamation) {
            let (_, range) = self.next_token();
            match self.src_text(range) {
                "s" => ConversionFlag::Str,
                "r" => ConversionFlag::Repr,
                "a" => ConversionFlag::Ascii,
                _ => {
                    self.add_error(
                        ParseErrorType::FStringError(FStringErrorType::InvalidConversionFlag),
                        range,
                    );
                    ConversionFlag::None
                }
            }
        } else {
            ConversionFlag::None
        };

        let format_spec = if self.eat(TokenKind::Colon) {
            let (elements, mut range) = self.parse_fstring_elements();
            // Special case for when the f-string format spec is empty. We set the range
            // to an empty `TextRange`.
            if range.is_empty() {
                range = TextRange::empty(self.current_range().start());
            }
            Some(Box::new(ast::FStringFormatSpec { range, elements }))
        } else {
            None
        };

        let close_brace_range = self.current_range();
        if has_open_brace && !self.eat(TokenKind::Rbrace) {
            self.add_error(
                ParseErrorType::FStringError(FStringErrorType::UnclosedLbrace),
                close_brace_range,
            );
        }

        ast::FStringExpressionElement {
            expression: Box::new(value),
            debug_text,
            conversion,
            format_spec,
            range: range.cover(close_brace_range),
        }
    }

    fn parse_bracketsized_expr(&mut self, open_bracket_range: TextRange) -> ExprWithRange {
        self.set_ctx(ParserCtxFlags::BRACKETSIZED_EXPR);
        // Nice error message when having a unclosed open bracket `[`
        if self.at_ts(NEWLINE_EOF_SET) {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("missing closing bracket `]`".to_string()),
                range,
            );
        }

        // Return an empty `ListExpr` when finding a `]` right after the `[`
        if self.at(TokenKind::Rsqb) {
            self.clear_ctx(ParserCtxFlags::BRACKETSIZED_EXPR);
            let close_bracket_range = self.current_range();
            let range = open_bracket_range.cover(close_bracket_range);

            self.eat(TokenKind::Rsqb);
            return (
                Expr::List(ast::ExprList {
                    elts: vec![],
                    ctx: ExprContext::Load,
                    range,
                }),
                range,
            );
        }

        let (mut expr, expr_range) = self.parse_expr2();

        match self.current_kind() {
            TokenKind::Async | TokenKind::For => {
                (expr, _) = self.parse_list_comprehension_expr(expr, expr_range);
            }
            _ => {
                (expr, _) = self.parse_list_expr(expr);
            }
        }
        let close_bracket_range = self.current_range();
        self.expect_and_recover(TokenKind::Rsqb, TokenSet::EMPTY);

        let range = open_bracket_range.cover(close_bracket_range);

        // Update the range of `Expr::List` or `Expr::ListComp` to
        // include the parenthesis.
        if matches!(expr, Expr::List(_) | Expr::ListComp(_)) {
            helpers::set_expr_range(&mut expr, range);
        }
        self.clear_ctx(ParserCtxFlags::BRACKETSIZED_EXPR);

        (expr, range)
    }

    fn parse_bracesized_expr(&mut self, lbrace_range: TextRange) -> ExprWithRange {
        self.set_ctx(ParserCtxFlags::BRACESIZED_EXPR);
        // Nice error message when having a unclosed open brace `{`
        if self.at_ts(NEWLINE_EOF_SET) {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("missing closing brace `}`".to_string()),
                range,
            );
        }

        // Return an empty `DictExpr` when finding a `}` right after the `{`
        if self.at(TokenKind::Rbrace) {
            self.clear_ctx(ParserCtxFlags::BRACESIZED_EXPR);
            let close_brace_range = self.current_range();
            let range = lbrace_range.cover(close_brace_range);

            self.eat(TokenKind::Rbrace);
            return (
                Expr::Dict(ast::ExprDict {
                    keys: vec![],
                    values: vec![],
                    range,
                }),
                range,
            );
        }

        let (mut expr, mut expr_range) = if self.eat(TokenKind::DoubleStar) {
            // Handle dict unpack
            let (value, _) = self.parse_expr();
            self.parse_dict_expr(None, value)
        } else {
            self.parse_expr2()
        };

        match self.current_kind() {
            TokenKind::Async | TokenKind::For => {
                (expr, expr_range) = self.parse_set_comprehension_expr(expr, expr_range);
            }
            TokenKind::Colon => {
                self.next_token();
                let (value, value_range) = self.parse_expr();
                let range = expr_range.cover(value_range);

                (expr, expr_range) = match self.current_kind() {
                    TokenKind::Async | TokenKind::For => {
                        self.parse_dict_comprehension_expr(expr, value, range)
                    }
                    _ => self.parse_dict_expr(Some(expr), value),
                };
            }
            _ if !matches!(expr, Expr::Dict(_)) => {
                (expr, expr_range) = self.parse_set_expr(expr);
            }
            _ => {}
        }

        let rbrace_range = self.current_range();
        self.expect_and_recover(TokenKind::Rbrace, TokenSet::EMPTY);

        // Check for dict unpack used in a comprehension, e.g. `{**d for i in l}`
        if matches!(
            expr,
            Expr::SetComp(ast::ExprSetComp { ref elt, .. }) if matches!(elt.as_ref(), Expr::Dict(_))
        ) {
            self.add_error(
                ParseErrorType::OtherError(
                    "dict unpacking cannot be used in dict comprehension".into(),
                ),
                expr_range,
            );
        }

        let range = lbrace_range.cover(rbrace_range);
        // Update the range of `Expr::Set`, `Expr::Dict`, `Expr::DictComp` and
        // `Expr::SetComp` to include the parenthesis.
        if matches!(
            expr,
            Expr::Set(_) | Expr::Dict(_) | Expr::DictComp(_) | Expr::SetComp(_)
        ) {
            helpers::set_expr_range(&mut expr, range);
        }
        self.clear_ctx(ParserCtxFlags::BRACESIZED_EXPR);

        (expr, range)
    }

    fn parse_parenthesized_expr(&mut self, open_paren_range: TextRange) -> ExprWithRange {
        self.set_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

        // Nice error message when having a unclosed open parenthesis `(`
        if self.at_ts(NEWLINE_EOF_SET) {
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("missing closing parenthesis `)`".to_string()),
                range,
            );
        }

        // Return an empty `TupleExpr` when finding a `)` right after the `(`
        if self.at(TokenKind::Rpar) {
            let close_paren_range = self.current_range();
            let range = open_paren_range.cover(close_paren_range);

            self.eat(TokenKind::Rpar);
            self.clear_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

            return (
                Expr::Tuple(ast::ExprTuple {
                    elts: vec![],
                    ctx: ExprContext::Load,
                    range,
                }),
                range,
            );
        }

        let (mut expr, expr_range) = self.parse_expr2();

        match self.current_kind() {
            TokenKind::Comma => {
                (expr, _) = self.parse_tuple_expr(expr, expr_range, Parser::parse_expr2);
            }
            TokenKind::Async | TokenKind::For => {
                (expr, _) = self.parse_generator_expr(expr, expr_range);
            }
            _ => {}
        }
        let close_paren_range = self.current_range();
        self.expect_and_recover(TokenKind::Rpar, TokenSet::EMPTY);

        let range = open_paren_range.cover(close_paren_range);

        // Update the range of `Expr::Tuple` or `Expr::Generator` to
        // include the parenthesis.
        if matches!(expr, Expr::Tuple(_) | Expr::GeneratorExp(_))
            && !self.last_ctx.contains(ParserCtxFlags::PARENTHESIZED_EXPR)
        {
            helpers::set_expr_range(&mut expr, range);
        }

        self.clear_ctx(ParserCtxFlags::PARENTHESIZED_EXPR);

        (expr, range)
    }

    /// Parses multiple items separated by a comma into a `TupleExpr` node.
    /// Uses `parse_func` to parse each item.
    fn parse_tuple_expr(
        &mut self,
        first_element: Expr,
        first_element_range: TextRange,
        mut parse_func: impl FnMut(&mut Parser<'src, 'src_path, I>) -> ExprWithRange,
    ) -> ExprWithRange {
        self.set_ctx(ParserCtxFlags::TUPLE_EXPR);
        // In case of the tuple only having one element, we need to cover the
        // range of the comma.
        let mut final_range = first_element_range.cover(self.current_range());
        self.eat(TokenKind::Comma);

        let mut elts = vec![first_element];

        while self.at_expr() {
            let (expr, expr_range) = parse_func(self);
            elts.push(expr);
            final_range = final_range.cover(expr_range);

            if self.at(TokenKind::Comma) {
                final_range = final_range.cover(self.current_range());
                self.eat(TokenKind::Comma);
            } else {
                if self.at_expr() {
                    self.expect(TokenKind::Comma);
                } else {
                    break;
                }
            }
        }

        self.clear_ctx(ParserCtxFlags::TUPLE_EXPR);
        (
            Expr::Tuple(ast::ExprTuple {
                elts,
                ctx: ExprContext::Load,
                range: final_range,
            }),
            final_range,
        )
    }

    fn parse_list_expr(&mut self, first_element: Expr) -> ExprWithRange {
        self.eat(TokenKind::Comma);
        let mut elts = vec![first_element];

        let range = self
            .parse_separated(
                true,
                TokenKind::Comma,
                [TokenKind::Rsqb].as_slice(),
                |parser| {
                    let (expr, range) = parser.parse_expr2();
                    elts.push(expr);
                    range
                },
            )
            // Doesn't really matter what range we get here, since the range will
            // be modified later in `parse_bracketsized_expr`.
            .unwrap_or_default();

        (
            Expr::List(ast::ExprList {
                elts,
                ctx: ExprContext::Load,
                range,
            }),
            range,
        )
    }

    fn parse_set_expr(&mut self, first_element: Expr) -> ExprWithRange {
        self.eat(TokenKind::Comma);
        let mut elts = vec![first_element];

        let range = self
            .parse_separated(
                true,
                TokenKind::Comma,
                [TokenKind::Rbrace].as_slice(),
                |parser| {
                    let (expr, range) = parser.parse_expr2();
                    elts.push(expr);
                    range
                },
            )
            // Doesn't really matter what range we get here, since the range will
            // be modified later in `parse_bracesized_expr`.
            .unwrap_or_default();

        (Expr::Set(ast::ExprSet { range, elts }), range)
    }

    fn parse_dict_expr(&mut self, key: Option<Expr>, value: Expr) -> ExprWithRange {
        self.eat(TokenKind::Comma);

        let mut keys = vec![key];
        let mut values = vec![value];

        let range = self
            .parse_separated(
                true,
                TokenKind::Comma,
                [TokenKind::Rbrace, TokenKind::For, TokenKind::Async].as_slice(),
                |parser| {
                    if parser.eat(TokenKind::DoubleStar) {
                        keys.push(None);
                    } else {
                        let (key, _) = parser.parse_expr();
                        keys.push(Some(key));

                        parser.expect_and_recover(
                            TokenKind::Colon,
                            TokenSet::new(&[TokenKind::Comma]).union(EXPR_SET),
                        );
                    }
                    let (value, range) = parser.parse_expr();
                    values.push(value);
                    range
                },
            )
            // Doesn't really matter what range we get here, since the range will
            // be modified later in `parse_bracesized_expr`.
            .unwrap_or_default();

        (
            Expr::Dict(ast::ExprDict {
                range,
                keys,
                values,
            }),
            range,
        )
    }

    fn parse_comprehension(&mut self) -> ast::Comprehension {
        assert!(self.at(TokenKind::For) || self.at(TokenKind::Async));

        let mut range = self.current_range();

        let is_async = self.eat(TokenKind::Async);
        self.eat(TokenKind::For);

        self.set_ctx(ParserCtxFlags::FOR_TARGET);
        let (mut target, _) = self.parse_expr_with_recovery(
            Parser::parse_exprs,
            [TokenKind::In, TokenKind::Colon].as_slice(),
            "expecting expression after `for` keyword",
        );
        self.clear_ctx(ParserCtxFlags::FOR_TARGET);

        helpers::set_expr_ctx(&mut target, ExprContext::Store);

        self.expect_and_recover(TokenKind::In, TokenSet::new(&[TokenKind::Rsqb]));

        let (iter, iter_expr) = self.parse_expr_with_recovery(
            Parser::parse_expr_simple,
            EXPR_SET.union(
                [
                    TokenKind::Rpar,
                    TokenKind::Rsqb,
                    TokenKind::Rbrace,
                    TokenKind::If,
                    TokenKind::Async,
                    TokenKind::For,
                ]
                .as_slice()
                .into(),
            ),
            "expecting an expression after `in` keyword",
        );
        range = range.cover(iter_expr);

        let mut ifs = vec![];
        while self.eat(TokenKind::If) {
            let (if_expr, if_range) = self.parse_expr_simple();
            ifs.push(if_expr);
            range = range.cover(if_range);
        }

        ast::Comprehension {
            range,
            target,
            iter,
            ifs,
            is_async,
        }
    }

    fn parse_generators(&mut self, mut range: TextRange) -> (Vec<ast::Comprehension>, TextRange) {
        const GENERATOR_SET: TokenSet = TokenSet::new(&[TokenKind::For, TokenKind::Async]);
        let mut generators = vec![];
        while self.at_ts(GENERATOR_SET) {
            let comp = self.parse_comprehension();
            range = range.cover(comp.range);

            generators.push(comp);
        }

        (generators, range)
    }

    fn parse_generator_expr(&mut self, element: Expr, element_range: TextRange) -> ExprWithRange {
        let (generators, range) = self.parse_generators(element_range);

        (
            Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt: Box::new(element),
                generators,
                range,
            }),
            range,
        )
    }

    fn parse_list_comprehension_expr(
        &mut self,
        element: Expr,
        element_range: TextRange,
    ) -> ExprWithRange {
        let (generators, range) = self.parse_generators(element_range);

        (
            Expr::ListComp(ast::ExprListComp {
                elt: Box::new(element),
                generators,
                range,
            }),
            range,
        )
    }

    fn parse_dict_comprehension_expr(
        &mut self,
        key: Expr,
        value: Expr,
        range: TextRange,
    ) -> ExprWithRange {
        let (generators, range) = self.parse_generators(range);

        (
            Expr::DictComp(ast::ExprDictComp {
                key: Box::new(key),
                value: Box::new(value),
                generators,
                range,
            }),
            range,
        )
    }

    fn parse_set_comprehension_expr(
        &mut self,
        element: Expr,
        element_range: TextRange,
    ) -> ExprWithRange {
        let (generators, range) = self.parse_generators(element_range);

        (
            Expr::SetComp(ast::ExprSetComp {
                elt: Box::new(element),
                generators,
                range,
            }),
            range,
        )
    }

    fn parse_starred_expr(&mut self, (_, range): Spanned) -> ExprWithRange {
        let (expr, expr_range) = self.parse_expr();
        let star_range = range.cover(expr_range);

        (
            Expr::Starred(ast::ExprStarred {
                value: Box::new(expr),
                ctx: ExprContext::Load,
                range: star_range,
            }),
            star_range,
        )
    }

    fn parse_await_expr(&mut self, start_range: TextRange) -> ExprWithRange {
        let mut await_range = start_range;

        let (expr, expr_range) = self.expr_bp(19);
        await_range = await_range.cover(expr_range);

        if matches!(expr, Expr::Starred(_)) {
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "starred expression `{}` is not allowed in an `await` statement",
                    self.src_text(expr_range)
                )),
                expr_range,
            );
        }

        (
            Expr::Await(ast::ExprAwait {
                value: Box::new(expr),
                range: await_range,
            }),
            await_range,
        )
    }

    fn parse_yield_expr(&mut self, mut yield_range: TextRange) -> ExprWithRange {
        if self.eat(TokenKind::From) {
            return self.parse_yield_from_expr(yield_range);
        }

        let value = if self.at_expr() {
            let (expr, expr_range) = self.parse_exprs();
            yield_range = yield_range.cover(expr_range);

            Some(Box::new(expr))
        } else {
            None
        };

        (
            Expr::Yield(ast::ExprYield {
                value,
                range: yield_range,
            }),
            yield_range,
        )
    }

    fn parse_yield_from_expr(&mut self, mut yield_range: TextRange) -> ExprWithRange {
        let (expr, expr_range) = self.parse_exprs();
        yield_range = yield_range.cover(expr_range);

        match expr {
            Expr::Starred(_) => {
                // Should we make `expr` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(format!(
                        "starred expression `{}` is not allowed in a `yield from` statement",
                        self.src_text(expr_range)
                    )),
                    expr_range,
                );
            }
            Expr::Tuple(_) if !self.last_ctx.contains(ParserCtxFlags::PARENTHESIZED_EXPR) => {
                // Should we make `expr` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(format!(
                        "unparenthesized tuple `{}` is not allowed in a `yield from` statement",
                        self.src_text(expr_range)
                    )),
                    expr_range,
                );
            }
            _ => {}
        }

        (
            Expr::YieldFrom(ast::ExprYieldFrom {
                value: Box::new(expr),
                range: yield_range,
            }),
            yield_range,
        )
    }

    fn parse_if_expr(&mut self, body: Expr, body_range: TextRange) -> ExprWithRange {
        assert!(self.eat(TokenKind::If));

        let (test, _) = self.parse_expr_simple();

        self.expect_and_recover(TokenKind::Else, TokenSet::EMPTY);

        let (orelse, orelse_range) = self.parse_expr_with_recovery(
            Parser::parse_expr,
            TokenSet::EMPTY,
            "expecting expression after `else` keyword",
        );
        let if_range = body_range.cover(orelse_range);

        (
            Expr::IfExp(ast::ExprIfExp {
                body: Box::new(body),
                test: Box::new(test),
                orelse: Box::new(orelse),
                range: if_range,
            }),
            if_range,
        )
    }

    fn parse_lambda_expr(&mut self, start_range: TextRange) -> ExprWithRange {
        self.set_ctx(ParserCtxFlags::LAMBDA_EXPR);
        let mut lambda_range = start_range;

        let parameters: Option<Box<ast::Parameters>> = if self.at(TokenKind::Colon) {
            None
        } else {
            Some(Box::new(self.parse_parameters()))
        };

        self.expect_and_recover(TokenKind::Colon, TokenSet::EMPTY);

        // Check for forbidden tokens in the `lambda`'s body
        let (kind, range) = self.current_token();
        match kind {
            TokenKind::Yield => self.add_error(
                ParseErrorType::OtherError(
                    "`yield` not allowed in a `lambda` expression".to_string(),
                ),
                range,
            ),
            TokenKind::Star => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "starred expression not allowed in a `lambda` expression".to_string(),
                    ),
                    range,
                );
            }
            TokenKind::DoubleStar => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "double starred expression not allowed in a `lambda` expression"
                            .to_string(),
                    ),
                    range,
                );
            }
            _ => {}
        }

        let (body, body_range) = self.parse_expr();
        lambda_range = lambda_range.cover(body_range);

        self.clear_ctx(ParserCtxFlags::LAMBDA_EXPR);
        (
            Expr::Lambda(ast::ExprLambda {
                body: Box::new(body),
                parameters,
                range: lambda_range,
            }),
            lambda_range,
        )
    }

    fn parse_parameter(&mut self) -> ast::Parameter {
        let name = self.parse_identifier();
        let mut range = name.range;
        // If we are at a colon and we're currently parsing a `lambda` expression,
        // this is the `lambda`'s body, don't try to parse as an annotation.
        let annotation = if self.at(TokenKind::Colon)
            && !self.has_in_curr_or_parent_ctx(ParserCtxFlags::LAMBDA_EXPR)
        {
            self.eat(TokenKind::Colon);
            let (ann, ann_range) = self.parse_expr();
            range = range.cover(ann_range);
            Some(Box::new(ann))
        } else {
            None
        };

        ast::Parameter {
            range,
            name,
            annotation,
        }
    }

    fn parse_parameter_with_default(&mut self) -> ast::ParameterWithDefault {
        let parameter = self.parse_parameter();
        let mut range = parameter.range;

        let default = if self.eat(TokenKind::Equal) {
            let (expr, expr_range) = self.parse_expr();
            range = range.cover(expr_range);
            Some(Box::new(expr))
        } else {
            None
        };

        ast::ParameterWithDefault {
            range,
            parameter,
            default,
        }
    }

    fn parse_parameters(&mut self) -> ast::Parameters {
        let mut args = vec![];
        let mut posonlyargs = vec![];
        let mut kwonlyargs = vec![];
        let mut kwarg = None;
        let mut vararg = None;

        let mut has_seen_asterisk = false;
        let mut has_seen_vararg = false;
        let mut has_seen_default_param = false;

        let ending = if self.has_ctx(ParserCtxFlags::FUNC_DEF_STMT) {
            TokenKind::Rpar
        } else if self.has_ctx(ParserCtxFlags::LAMBDA_EXPR) {
            TokenKind::Colon
        } else {
            TokenKind::Newline
        };

        let ending_set = TokenSet::new(&[TokenKind::Rarrow, ending]).union(COMPOUND_STMT_SET);
        let first_param_range = self.current_range();
        let range = self
            .parse_separated(true, TokenKind::Comma, ending_set, |parser| {
                let mut range = parser.current_range();
                // Don't allow any parameter after we have seen a vararg `**kwargs`
                if has_seen_vararg {
                    parser.add_error(ParseErrorType::ParamFollowsVarKeywordParam, range);
                }

                if parser.eat(TokenKind::Star) {
                    has_seen_asterisk = true;
                    if parser.at(TokenKind::Comma) {
                        has_seen_default_param = false;
                    } else if parser.at_expr() {
                        let param = parser.parse_parameter();
                        range = param.range;
                        vararg = Some(Box::new(param));
                    }
                } else if parser.eat(TokenKind::DoubleStar) {
                    has_seen_vararg = true;
                    let param = parser.parse_parameter();
                    range = param.range;
                    kwarg = Some(Box::new(param));
                } else if parser.eat(TokenKind::Slash) {
                    // Don't allow `/` after a `*`
                    if has_seen_asterisk {
                        let range = parser.current_range();
                        parser.add_error(
                            ParseErrorType::OtherError("`/` must be ahead of `*`".to_string()),
                            range,
                        );
                    }
                    std::mem::swap(&mut args, &mut posonlyargs);
                } else if parser.at(TokenKind::Name) {
                    let param = parser.parse_parameter_with_default();
                    // Don't allow non-default parameters after default parameters e.g. `a=1, b`,
                    // can't place `b` after `a=1`. Non-default parameters are only allowed after
                    // default parameters if we have a `*` before them, e.g. `a=1, *, b`.
                    if param.default.is_none() && has_seen_default_param && !has_seen_asterisk {
                        let range = parser.current_range();
                        parser.add_error(ParseErrorType::DefaultArgumentError, range);
                    }
                    has_seen_default_param = param.default.is_some();

                    range = param.range;
                    if has_seen_asterisk {
                        kwonlyargs.push(param);
                    } else {
                        args.push(param);
                    }
                } else {
                    if parser.at_ts(SIMPLE_STMT_SET) {
                        return TextRange::default(); // We can return any range here
                    }

                    let mut range = parser.current_range();
                    parser.skip_until(
                        ending_set.union([TokenKind::Comma, TokenKind::Colon].as_slice().into()),
                    );
                    range = range.cover(parser.current_range());
                    parser.add_error(
                        ParseErrorType::OtherError("expected parameter".to_string()),
                        range,
                    );
                }

                range
            })
            .map_or(first_param_range, |range| first_param_range.cover(range));

        let parameters = ast::Parameters {
            range,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        };

        if let Err(error) = helpers::validate_parameters(&parameters, self.source_path) {
            self.errors.push(error);
        }

        parameters
    }

    fn parse_named_expr(&mut self, mut target: Expr, target_range: TextRange) -> ExprWithRange {
        assert!(self.eat(TokenKind::ColonEqual));

        if !helpers::is_valid_assignment_target(&target) {
            self.add_error(ParseErrorType::NamedAssignmentError, target_range);
        }
        helpers::set_expr_ctx(&mut target, ExprContext::Store);

        let (value, value_range) = self.parse_expr();
        let range = target_range.cover(value_range);

        (
            Expr::NamedExpr(ast::ExprNamedExpr {
                target: Box::new(target),
                value: Box::new(value),
                range,
            }),
            range,
        )
    }
}
