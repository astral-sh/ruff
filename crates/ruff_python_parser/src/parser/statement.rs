use crate::parser::syntax::{self as id, Construct, IntoList};
use ruff_python_ast::AstNode as _;
use ruff_python_ast::{AstNodeId, ExprKind, ListId, build};
use std::fmt::{Display, Write};

use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, ExprContext, IpyEscapeKind, Operator, PythonVersion};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::error::StarTupleKind;
use crate::parser::expression::{ArgumentsContext, EXPR_SET, ParsedExpr};
use crate::parser::progress::ParserProgress;
use crate::parser::{
    FunctionKind, IpyEscapeContext, Parser, RecoveryContext, RecoveryContextKind, WithItemKind,
    helpers,
};
use crate::token_set::TokenSet;
use crate::{Mode, ParseErrorType, UnsupportedSyntaxErrorKind};

use super::Parenthesized;
use super::expression::ExpressionContext;

/// Tokens that represent compound statements.
const COMPOUND_STMT_SET: TokenSet = TokenSet::new([
    TokenKind::Match,
    TokenKind::If,
    TokenKind::With,
    TokenKind::While,
    TokenKind::For,
    TokenKind::Try,
    TokenKind::Def,
    TokenKind::Class,
    TokenKind::Async,
    TokenKind::At,
]);

/// Tokens that represent simple statements, but doesn't include expressions.
const SIMPLE_STMT_SET: TokenSet = TokenSet::new([
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
    TokenKind::IpyEscapeCommand,
]);

/// Tokens that represent simple statements, including expressions.
const SIMPLE_STMT_WITH_EXPR_SET: TokenSet = SIMPLE_STMT_SET.union(EXPR_SET);

/// Tokens that represents all possible statements, including simple, compound,
/// and expression statements.
const STMTS_SET: TokenSet = SIMPLE_STMT_WITH_EXPR_SET.union(COMPOUND_STMT_SET);

/// Tokens that represent operators that can be used in augmented assignments.
const AUGMENTED_ASSIGN_SET: TokenSet = TokenSet::new([
    TokenKind::PlusEqual,
    TokenKind::MinusEqual,
    TokenKind::StarEqual,
    TokenKind::DoubleStarEqual,
    TokenKind::SlashEqual,
    TokenKind::DoubleSlashEqual,
    TokenKind::PercentEqual,
    TokenKind::AtEqual,
    TokenKind::AmperEqual,
    TokenKind::VbarEqual,
    TokenKind::CircumflexEqual,
    TokenKind::LeftShiftEqual,
    TokenKind::RightShiftEqual,
]);

impl<'src> Parser<'src> {
    /// Returns `true` if the current token is the start of a compound statement.
    pub(super) fn at_compound_stmt(&self) -> bool {
        self.at_ts(COMPOUND_STMT_SET)
    }

    /// Returns `true` if the current token is the start of a simple statement,
    /// including expressions.
    fn at_simple_stmt(&self) -> bool {
        self.at_ts(SIMPLE_STMT_WITH_EXPR_SET) || self.at_soft_keyword()
    }

    /// Returns `true` if the current token is the start of a simple, compound or expression
    /// statement.
    pub(super) fn at_stmt(&self) -> bool {
        self.at_ts(STMTS_SET) || self.at_soft_keyword()
    }

    /// Checks if the parser is currently positioned at the start of a type parameter.
    pub(super) fn at_type_param(&self) -> bool {
        let token = self.current_token_kind();
        matches!(
            token,
            TokenKind::Star | TokenKind::DoubleStar | TokenKind::Identifier
        ) || token.is_keyword()
    }

    /// Parses a compound or a single simple statement.
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/compound_stmts.html>
    /// - <https://docs.python.org/3/reference/simple_stmts.html>
    pub(super) fn parse_statement(&mut self) -> id::Stmt {
        let start = self.node_start();

        match self.current_token_kind() {
            TokenKind::If => id::Stmt::from(self.parse_if_statement()),
            TokenKind::For => id::Stmt::from(self.parse_for_statement(start, false)),
            TokenKind::While => id::Stmt::from(self.parse_while_statement()),
            TokenKind::Def => {
                id::Stmt::from(self.parse_function_definition(Vec::new(), start, false))
            }
            TokenKind::Class => id::Stmt::from(self.parse_class_definition(Vec::new(), start)),
            TokenKind::Try => id::Stmt::from(self.parse_try_statement()),
            TokenKind::With => id::Stmt::from(self.parse_with_statement(start, false)),
            TokenKind::At => self.parse_decorators(),
            TokenKind::Async => self.parse_async_statement(),
            token => {
                if token == TokenKind::Match {
                    // Match is considered a soft keyword, so we will treat it as an identifier if
                    // it's followed by an unexpected token.

                    match self.classify_match_token() {
                        MatchTokenKind::Keyword => {
                            return id::Stmt::from(self.parse_match_statement());
                        }
                        MatchTokenKind::KeywordOrIdentifier => {
                            if let Some(match_stmt) = self.try_parse_match_statement() {
                                return id::Stmt::from(match_stmt);
                            }
                        }
                        MatchTokenKind::Identifier => {}
                    }
                }

                self.parse_single_simple_statement()
            }
        }
    }

    /// Parses a single simple statement.
    ///
    /// This statement must be terminated by a newline or semicolon.
    ///
    /// Use [`Parser::parse_simple_statements`] to parse a sequence of simple statements.
    fn parse_single_simple_statement(&mut self) -> id::Stmt {
        let stmt = self.parse_simple_statement();

        // The order of the token is important here.
        let has_eaten_semicolon = self.eat(TokenKind::Semi);
        let has_eaten_newline = self.eat(TokenKind::Newline);

        if !has_eaten_newline {
            if !has_eaten_semicolon && self.at_simple_stmt() {
                // test_err simple_stmts_on_same_line
                // a b
                // a + b c + d
                // break; continue pass; continue break
                self.add_error(
                    ParseErrorType::SimpleStatementsOnSameLine,
                    self.current_token_range(),
                );
            } else if self.at_compound_stmt() && !self.at(TokenKind::Match) {
                // `match` can also be an identifier. Report this error when parsing it as a keyword.

                // test_err simple_and_compound_stmt_on_same_line
                // a; if b: pass; b
                self.add_error(
                    ParseErrorType::SimpleAndCompoundStatementOnSameLine,
                    self.current_token_range(),
                );
            }
        }

        stmt
    }

    /// Parses a sequence of simple statements.
    ///
    /// If there is more than one statement in this sequence, it is expected to be separated by a
    /// semicolon. The sequence can optionally end with a semicolon, but regardless of whether
    /// a semicolon is present or not, it is expected to end with a newline.
    ///
    /// Matches the `simple_stmts` rule in the [Python grammar].
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_simple_statements(&mut self) -> ListId<AstNodeId<ast::kind::Stmt>> {
        let stmts_snapshot = self.stmt_scratch.snapshot();
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            let stmt = self.parse_simple_statement();
            self.stmt_scratch.push(stmt.id);

            if !self.eat(TokenKind::Semi) {
                if self.at_simple_stmt() {
                    // test_err simple_stmts_on_same_line_in_block
                    // if True: break; continue pass; continue break
                    self.add_error(
                        ParseErrorType::SimpleStatementsOnSameLine,
                        self.current_token_range(),
                    );
                } else {
                    // test_ok simple_stmts_in_block
                    // if True: pass
                    // if True: pass;
                    // if True: pass; continue
                    // if True: pass; continue;
                    // x = 1
                    break;
                }
            }

            if !self.at_simple_stmt() {
                break;
            }
        }

        // Ideally, we should use `expect` here but we use `eat` for better error message. Later,
        // if the parser isn't at the start of a compound statement, we'd `expect` a newline.
        if !self.eat(TokenKind::Newline) {
            if self.at_compound_stmt() {
                // test_err simple_and_compound_stmt_on_same_line_in_block
                // if True: pass if False: pass
                // if True: pass; if False: pass
                self.add_error(
                    ParseErrorType::SimpleAndCompoundStatementOnSameLine,
                    self.current_token_range(),
                );
            } else {
                // test_err multiple_clauses_on_same_line
                // if True: pass elif False: pass else: pass
                // if True: pass; elif False: pass; else: pass
                // for x in iter: break else: pass
                // for x in iter: break; else: pass
                // try: pass except exc: pass else: pass finally: pass
                // try: pass; except exc: pass; else: pass; finally: pass
                self.add_error(
                    ParseErrorType::ExpectedToken {
                        found: self.current_token_kind(),
                        expected: TokenKind::Newline,
                    },
                    self.current_token_range(),
                );
            }
        }

        // test_ok simple_stmts_with_semicolons
        // return; import a; from x import y; z; type T = int
        self.stmt_scratch
            .take_range(stmts_snapshot, &mut self.storage)
    }

    /// Parses a simple statement.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html>
    fn parse_simple_statement(&mut self) -> id::Stmt {
        match self.current_token_kind() {
            TokenKind::Return => id::Stmt::from(self.parse_return_statement()),
            TokenKind::Import => {
                let start = self.node_start();
                id::Stmt::from(self.parse_import_statement(start, false))
            }
            TokenKind::From => {
                let start = self.node_start();
                id::Stmt::from(self.parse_from_import_statement(start, false))
            }
            TokenKind::Pass => id::Stmt::from(self.parse_pass_statement()),
            TokenKind::Continue => id::Stmt::from(self.parse_continue_statement()),
            TokenKind::Break => id::Stmt::from(self.parse_break_statement()),
            TokenKind::Raise => id::Stmt::from(self.parse_raise_statement()),
            TokenKind::Del => id::Stmt::from(self.parse_delete_statement()),
            TokenKind::Assert => id::Stmt::from(self.parse_assert_statement()),
            TokenKind::Global => id::Stmt::from(self.parse_global_statement()),
            TokenKind::Nonlocal => id::Stmt::from(self.parse_nonlocal_statement()),
            TokenKind::IpyEscapeCommand => {
                id::Stmt::from(self.parse_ipython_escape_command_statement())
            }
            token => {
                if token == TokenKind::Lazy {
                    let start = self.node_start();
                    let lazy_range = self.current_token_range();

                    match self.peek() {
                        // test_ok lazy_import_stmt_py315
                        // # parse_options: {"target-version": "3.15"}
                        // lazy import foo
                        // lazy import foo as bar
                        // lazy from bar import baz
                        // lazy from sys import x as y
                        // lazy = 1
                        // import foo as lazy
                        // from lazy import qux

                        // test_ok lazy_import_relative_py315
                        // # parse_options: {"target-version": "3.15"}
                        // lazy from . import basic2
                        // lazy from .basic2 import x, f
                        // lazy from . import b, x

                        // test_ok lazy_import_soft_keyword_split_py315
                        // # parse_options: {"target-version": "3.15"}
                        // lazy
                        // import os
                        //
                        // lazy  # comment
                        // from sys import path

                        // test_err lazy_import_stmt_py314
                        // # parse_options: {"target-version": "3.14"}
                        // lazy import foo
                        // lazy from bar import baz
                        TokenKind::Import => {
                            self.bump(TokenKind::Lazy);
                            self.add_unsupported_syntax_error(
                                UnsupportedSyntaxErrorKind::LazyImportStatement,
                                lazy_range,
                            );
                            return id::Stmt::from(self.parse_import_statement(start, true));
                        }
                        TokenKind::From => {
                            self.bump(TokenKind::Lazy);
                            self.add_unsupported_syntax_error(
                                UnsupportedSyntaxErrorKind::LazyImportStatement,
                                lazy_range,
                            );
                            return id::Stmt::from(self.parse_from_import_statement(start, true));
                        }
                        _ => {}
                    }
                }

                if token == TokenKind::Type {
                    // Type is considered a soft keyword, so we will treat it as an identifier if
                    // it's followed by an unexpected token.
                    let (first, second) = self.peek2();

                    if (first == TokenKind::Identifier || first.is_soft_keyword())
                        && matches!(second, TokenKind::Lsqb | TokenKind::Equal)
                    {
                        return id::Stmt::from(self.parse_type_alias_statement());
                    }
                }

                let start = self.node_start();

                // test_err yield_after_comma
                // def f(): 1, yield 1

                // test_ok yield_after_comma_parenthesized
                // def f(): 1, (yield 1)

                // simple_stmt: `... | yield_stmt | star_expressions | ...`
                let parsed_expr =
                    self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

                if self.at(TokenKind::Equal) {
                    id::Stmt::from(self.parse_assign_statement(parsed_expr, start))
                } else if self.at(TokenKind::Colon) {
                    id::Stmt::from(self.parse_annotated_assignment_statement(parsed_expr, start))
                } else if let Some(op) = self.current_token_kind().as_augmented_assign_operator() {
                    id::Stmt::from(self.parse_augmented_assignment_statement(
                        parsed_expr,
                        op,
                        start,
                    ))
                } else if self.options.mode == Mode::Ipython && self.at(TokenKind::Question) {
                    id::Stmt::from(
                        self.parse_ipython_help_end_escape_command_statement(&parsed_expr),
                    )
                } else {
                    id::Stmt::from(
                        build::StmtExpr {
                            range: self.node_range(start),
                            value: parsed_expr.expr.id,
                        }
                        .parsed(&mut self.storage),
                    )
                }
            }
        }
    }

    /// Parses a delete statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `del` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-del_stmt>
    fn parse_delete_statement(&mut self) -> id::StmtDelete {
        let start = self.node_start();
        self.bump(TokenKind::Del);

        // test_err del_incomplete_target
        // del x, y.
        // z
        // del x, y[
        // z
        let targets = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::DeleteTargets,
            |parser| {
                // Allow starred expression to raise a better error message for
                // an invalid delete target later.
                let target = parser.parse_conditional_expression_or_higher_impl(
                    ExpressionContext::starred_conditional(),
                );
                parser
                    .storage
                    .set_expr_context(target.expr.id, ExprContext::Del);

                // test_err invalid_del_target
                // del x + 1
                // del {'x': 1}
                // del {'x', 'y'}
                // del None, True, False, 1, 1.0, "abc"
                parser.validate_delete_target(&target.expr);

                target.expr
            },
        );

        if targets.is_empty() {
            // test_err del_stmt_empty
            // del
            self.add_error(
                ParseErrorType::EmptyDeleteTargets,
                self.current_token_range(),
            );
        }

        build::StmtDelete {
            targets: self.storage.list(targets).into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `return` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `return` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-return_stmt>
    fn parse_return_statement(&mut self) -> id::StmtReturn {
        let start = self.node_start();
        self.bump(TokenKind::Return);

        // test_err return_stmt_invalid_expr
        // return *
        // return yield x
        // return yield from x
        // return x := 1
        // return *x and y
        let value = self.at_expr().then(|| {
            let parsed_expr = self.parse_expression_list(ExpressionContext::starred_bitwise_or());

            // test_ok iter_unpack_return_py37
            // # parse_options: {"target-version": "3.7"}
            // rest = (4, 5, 6)
            // def f(): return (1, 2, 3, *rest)

            // test_ok iter_unpack_return_py38
            // # parse_options: {"target-version": "3.8"}
            // rest = (4, 5, 6)
            // def f(): return 1, 2, 3, *rest

            // test_err iter_unpack_return_py37
            // # parse_options: {"target-version": "3.7"}
            // rest = (4, 5, 6)
            // def f(): return 1, 2, 3, *rest
            self.check_tuple_unpacking(
                &parsed_expr,
                UnsupportedSyntaxErrorKind::StarTuple(StarTupleKind::Return),
            );

            parsed_expr.expr
        });

        build::StmtReturn {
            range: self.node_range(start),
            value: value.map(|node| node.id),
        }
        .parsed(&mut self.storage)
    }

    /// Report [`UnsupportedSyntaxError`]s for each starred element in `expr` if it is an
    /// unparenthesized tuple.
    ///
    /// This method can be used to check for tuple unpacking in `return`, `yield`, and `for`
    /// statements, which are only allowed after [Python 3.8] and [Python 3.9], respectively.
    ///
    /// [Python 3.8]: https://github.com/python/cpython/issues/76298
    /// [Python 3.9]: https://github.com/python/cpython/issues/90881
    pub(super) fn check_tuple_unpacking(
        &mut self,
        expr: &id::Expr,
        kind: UnsupportedSyntaxErrorKind,
    ) {
        if kind.is_supported(self.options.target_version) {
            return;
        }

        let Some(tuple) = expr.view(&self.storage).cast::<ast::ExprTuple>() else {
            return;
        };
        if tuple.parenthesized() {
            return;
        }
        let elements = tuple.elts().id();
        for i in 0..elements.len() {
            if let Some(element) = self.storage.get_list(elements).get(i)
                && element.node_kind() == ast::NodeKind::ExprStarred
            {
                self.add_unsupported_syntax_error(kind, element.range());
            }
        }
    }

    /// Parses a `raise` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `raise` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-raise_stmt>
    fn parse_raise_statement(&mut self) -> id::StmtRaise {
        let start = self.node_start();
        self.bump(TokenKind::Raise);

        let exc = match self.current_token_kind() {
            // test_ok bare_raise_before_semicolon
            // try: work()
            // except ValueError: raise; report()
            TokenKind::Newline | TokenKind::Semi => None,
            TokenKind::From => {
                // test_err raise_stmt_from_without_exc
                // raise from exc
                // raise from None
                self.add_error(
                    ParseErrorType::OtherError(
                        "Exception missing in `raise` statement with cause".to_string(),
                    ),
                    self.current_token_range(),
                );
                None
            }
            _ => {
                // test_err raise_stmt_invalid_exc
                // raise *x
                // raise yield x
                // raise x := 1
                let exc = self.parse_expression_list(ExpressionContext::default());

                if exc
                    .expr
                    .view(&self.storage)
                    .cast::<ast::ExprTuple>()
                    .is_some_and(|tuple| !tuple.parenthesized())
                {
                    // test_err raise_stmt_unparenthesized_tuple_exc
                    // raise x,
                    // raise x, y
                    // raise x, y from z
                    self.add_error(ParseErrorType::UnparenthesizedTupleExpression, exc);
                }

                Some(exc.expr)
            }
        };

        let cause = self.eat(TokenKind::From).then(|| {
            // test_err raise_stmt_invalid_cause
            // raise x from *y
            // raise x from yield y
            // raise x from y := 1
            let cause = self.parse_expression_list(ExpressionContext::default());

            if cause
                .expr
                .view(&self.storage)
                .cast::<ast::ExprTuple>()
                .is_some_and(|tuple| !tuple.parenthesized())
            {
                // test_err raise_stmt_unparenthesized_tuple_cause
                // raise x from y,
                // raise x from y, z
                self.add_error(ParseErrorType::UnparenthesizedTupleExpression, cause);
            }

            cause.expr
        });

        build::StmtRaise {
            range: self.node_range(start),
            exc: exc.map(|node| node.id),
            cause: cause.map(|node| node.id),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an import statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `import` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#the-import-statement>
    fn parse_import_statement(&mut self, start: TextSize, is_lazy: bool) -> id::StmtImport {
        self.bump(TokenKind::Import);

        // test_err import_stmt_parenthesized_names
        // import (a)
        // import (a, b)

        // test_err import_stmt_star_import
        // import *
        // import x, *, y

        // test_err import_stmt_trailing_comma
        // import ,
        // import x, y,

        let names_snapshot = self.alias_scratch.snapshot();
        self.parse_comma_separated_list(RecoveryContextKind::ImportNames, |parser| {
            let alias = parser.parse_alias(ImportStyle::Import);
            parser.alias_scratch.push(alias.id);
        });
        let names = self
            .alias_scratch
            .take_range(names_snapshot, &mut self.storage);

        if names.is_empty() {
            // test_err import_stmt_empty
            // import
            self.add_error(ParseErrorType::EmptyImportNames, self.current_token_range());
        }

        build::StmtImport {
            names,
            is_lazy,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `from` import statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `from` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-import_stmt>
    fn parse_from_import_statement(
        &mut self,
        start: TextSize,
        is_lazy: bool,
    ) -> id::StmtImportFrom {
        self.bump(TokenKind::From);

        let mut leading_dots = 0;
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            if self.eat(TokenKind::Dot) {
                leading_dots += 1;
            } else if self.eat(TokenKind::Ellipsis) {
                leading_dots += 3;
            } else {
                break;
            }
        }

        let module = if self.at_identifier_or_soft_keyword() {
            // test_ok from_import_soft_keyword_module_name
            // from match import pattern
            // from type import bar
            // from case import pattern
            // from lazy import qux
            // from match.type.case import foo
            Some(self.parse_dotted_name())
        } else {
            if leading_dots == 0 {
                // test_err from_import_missing_module
                // from
                // from import x
                self.add_error(
                    ParseErrorType::OtherError("Expected a module name".to_string()),
                    self.current_token_range(),
                );
            }
            None
        };

        // test_ok from_import_no_space
        // from.import x
        // from...import x
        self.expect(TokenKind::Import);

        let names_start = self.node_start();
        let names_snapshot = self.alias_scratch.snapshot();
        let mut seen_star_import = false;

        let parenthesized = Parenthesized::from(self.eat(TokenKind::Lpar));

        // test_err from_import_unparenthesized_trailing_comma
        // from a import b,
        // from a import b as c,
        // from a import b, c,
        self.parse_comma_separated_list(
            RecoveryContextKind::ImportFromAsNames(parenthesized),
            |parser| {
                // test_err from_import_dotted_names
                // from x import a.
                // from x import a.b
                // from x import a, b.c, d, e.f, g
                let alias = parser.parse_alias(ImportStyle::ImportFrom);
                seen_star_import |= alias.view(&parser.storage).name().id() == "*";
                parser.alias_scratch.push(alias.id);
            },
        );
        let names = self
            .alias_scratch
            .take_range(names_snapshot, &mut self.storage);

        if names.is_empty() {
            // test_err from_import_empty_names
            // from x import
            // from x import ()
            // from x import ,,
            self.add_error(ParseErrorType::EmptyImportNames, self.current_token_range());
        }

        if seen_star_import && parenthesized.is_yes() {
            // test_err from_import_parenthesized_star
            // from x import (*)
            self.add_error(
                ParseErrorType::OtherError("Star import cannot be parenthesized".to_string()),
                self.node_range(names_start),
            );
        }

        if seen_star_import && names.len() > 1 {
            // test_err from_import_star_with_other_names
            // from x import *, a
            // from x import a, *, b
            // from x import *, a as b
            // from x import *, *, a
            self.add_error(
                ParseErrorType::OtherError("Star import must be the only import".to_string()),
                self.node_range(names_start),
            );
        }

        if parenthesized.is_yes() {
            // test_err from_import_missing_rpar
            // from x import (a, b
            // 1 + 1
            // from x import (a, b,
            // 2 + 2
            self.expect(TokenKind::Rpar);
        }

        build::StmtImportFrom {
            module: module.map(|node| node.id),
            names,
            level: leading_dots,
            is_lazy,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `import` or `from` import name.
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/simple_stmts.html#the-import-statement>
    /// - <https://docs.python.org/3/library/ast.html#ast.alias>
    fn parse_alias(&mut self, style: ImportStyle) -> id::Alias {
        let start = self.node_start();
        if self.eat(TokenKind::Star) {
            let range = self.node_range(start);
            return build::Alias {
                name: build::Identifier {
                    id: self.storage.intern_name("*"),
                    range,
                }
                .parsed(&mut self.storage)
                .id,
                asname: None,
                range,
            }
            .parsed(&mut self.storage);
        }

        let name = match style {
            ImportStyle::Import => self.parse_dotted_name(),
            ImportStyle::ImportFrom => self.parse_identifier(),
        };

        let asname = if self.eat(TokenKind::As) {
            if self.at_identifier_or_soft_keyword() {
                // test_ok import_as_name_soft_keyword
                // import foo as match
                // import bar as case
                // import baz as type
                // import qux as lazy
                Some(self.parse_identifier())
            } else {
                // test_err import_alias_missing_asname
                // import x as
                self.add_error(
                    ParseErrorType::OtherError("Expected symbol after `as`".to_string()),
                    self.current_token_range(),
                );
                None
            }
        } else {
            None
        };

        build::Alias {
            range: self.node_range(start),
            name: name.id,
            asname: asname.map(|node| node.id),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a dotted name.
    ///
    /// A dotted name is a sequence of identifiers separated by a single dot.
    fn parse_dotted_name(&mut self) -> id::Identifier {
        let start = self.node_start();

        let first = self.parse_identifier_with_context(ExpressionContext::default());
        if !self.at(TokenKind::Dot) {
            return first.parsed(&mut self.storage);
        }

        let snapshot = self.name_buffer.len();
        self.name_buffer.push_str(self.storage.name(first.id));
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::Dot) {
            progress.assert_progressing(self);

            // test_err dotted_name_multiple_dots
            // import a..b
            // import a...b
            self.name_buffer.push('.');
            let identifier = self.parse_identifier_with_context(ExpressionContext::default());
            self.name_buffer.push_str(self.storage.name(identifier.id));
        }

        let id = self.storage.intern_name(&self.name_buffer[snapshot..]);
        self.name_buffer.truncate(snapshot);

        // test_ok dotted_name_normalized_spaces
        // import a.b.c
        // import a .  b  . c
        build::Identifier {
            id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `pass` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `pass` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-pass_stmt>
    fn parse_pass_statement(&mut self) -> id::StmtPass {
        let start = self.node_start();
        self.bump(TokenKind::Pass);
        build::StmtPass {
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `continue` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `continue` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-continue_stmt>
    fn parse_continue_statement(&mut self) -> id::StmtContinue {
        let start = self.node_start();
        self.bump(TokenKind::Continue);
        build::StmtContinue {
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `break` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `break` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-break_stmt>
    fn parse_break_statement(&mut self) -> id::StmtBreak {
        let start = self.node_start();
        self.bump(TokenKind::Break);
        build::StmtBreak {
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `assert` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `assert` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement>
    fn parse_assert_statement(&mut self) -> id::StmtAssert {
        let start = self.node_start();
        self.bump(TokenKind::Assert);

        // test_err assert_empty_test
        // assert

        // test_err assert_invalid_test_expr
        // assert *x
        // assert assert x
        // assert yield x
        // assert x := 1
        let test = self.parse_conditional_expression_or_higher();

        let msg = if self.eat(TokenKind::Comma) {
            if self.at_expr() {
                // test_err assert_invalid_msg_expr
                // assert False, *x
                // assert False, assert x
                // assert False, yield x
                // assert False, x := 1
                Some(self.parse_conditional_expression_or_higher().expr)
            } else {
                // test_err assert_empty_msg
                // assert x,
                self.add_error(
                    ParseErrorType::ExpectedExpression,
                    self.current_token_range(),
                );
                None
            }
        } else {
            None
        };

        build::StmtAssert {
            test: test.expr.id,
            msg: msg.map(|node| node.id),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a global statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `global` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-global_stmt>
    fn parse_global_statement(&mut self) -> id::StmtGlobal {
        let start = self.node_start();
        self.bump(TokenKind::Global);

        // test_err global_stmt_trailing_comma
        // global ,
        // global x,
        // global x, y,

        // test_err global_stmt_expression
        // global x + 1
        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::Identifiers,
            Parser::parse_identifier,
        );

        if names.is_empty() {
            // test_err global_stmt_empty
            // global
            self.add_error(ParseErrorType::EmptyGlobalNames, self.current_token_range());
        }

        // test_ok global_stmt
        // global x
        // global x, y, z
        build::StmtGlobal {
            range: self.node_range(start),
            names: names.into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a nonlocal statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `nonlocal` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-nonlocal_stmt>
    fn parse_nonlocal_statement(&mut self) -> id::StmtNonlocal {
        let start = self.node_start();
        self.bump(TokenKind::Nonlocal);

        // test_err nonlocal_stmt_trailing_comma
        // def _():
        //     nonlocal ,
        //     nonlocal x,
        //     nonlocal x, y,

        // test_err nonlocal_stmt_expression
        // def _():
        //     nonlocal x + 1
        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::Identifiers,
            Parser::parse_identifier,
        );

        if names.is_empty() {
            // test_err nonlocal_stmt_empty
            // def _():
            //     nonlocal
            self.add_error(
                ParseErrorType::EmptyNonlocalNames,
                self.current_token_range(),
            );
        }

        // test_ok nonlocal_stmt
        // def _():
        //     nonlocal x
        //     nonlocal x, y, z
        build::StmtNonlocal {
            range: self.node_range(start),
            names: names.into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a type alias statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `type` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#the-type-statement>
    fn parse_type_alias_statement(&mut self) -> id::StmtTypeAlias {
        let start = self.node_start();
        let type_range = self.current_token_range();
        self.bump(TokenKind::Type);

        // test_ok type_stmt_py312
        // # parse_options: {"target-version": "3.12"}
        // type x = int

        // test_err type_stmt_py311
        // # parse_options: {"target-version": "3.11"}
        // type x = int

        self.add_unsupported_syntax_error(
            UnsupportedSyntaxErrorKind::TypeAliasStatement,
            type_range,
        );

        let name = id::Expr::from(self.parse_name(ExpressionContext::default()));
        self.storage.set_expr_context(name.id, ExprContext::Store);

        let type_params = self.try_parse_type_params();

        self.expect(TokenKind::Equal);

        // test_err type_alias_incomplete_stmt
        // type
        // type x
        // type x =

        // test_err type_alias_invalid_value_expr
        // type x = *y
        // type x = yield y
        // type x = yield from y
        // type x = x := 1
        let value = self.parse_conditional_expression_or_higher();

        build::StmtTypeAlias {
            name: name.id,
            type_params: type_params.map(|node| node.id),
            value: value.expr.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an IPython escape command at the statement level.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `IpyEscapeCommand` token.
    fn parse_ipython_escape_command_statement(&mut self) -> id::StmtIpyEscapeCommand {
        let start = self.node_start();

        let (value, kind) = self.bump_ipython_escape_command(IpyEscapeContext::LogicalLineStart);

        let range = self.node_range(start);
        if self.options.mode != Mode::Ipython {
            self.add_error(ParseErrorType::UnexpectedIpythonEscapeCommand, range);
        }

        build::StmtIpyEscapeCommand { range, kind, value }.parsed(&mut self.storage)
    }

    /// Parses an IPython help end escape command at the statement level.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `?` token.
    fn parse_ipython_help_end_escape_command_statement(
        &mut self,
        parsed_expr: &ParsedExpr,
    ) -> id::StmtIpyEscapeCommand {
        // We are permissive than the original implementation because we would allow whitespace
        // between the expression and the suffix while the IPython implementation doesn't allow it.
        // For example, `foo ?` would be valid in our case but invalid for IPython.
        fn unparse_expr(parser: &mut Parser, expr: id::Expr, buffer: &mut String) {
            match expr.view(&parser.storage).classify() {
                ExprKind::Name(name) => {
                    buffer.push_str(name.id().as_str());
                }
                ExprKind::Subscript(subscript) => {
                    let value = id::Node::from_view(subscript.value());
                    let slice = id::Node::from_view(subscript.slice());
                    unparse_expr(parser, value, buffer);
                    buffer.push('[');

                    if let Some(number) = slice.view(&parser.storage).as_number_literal_expr()
                        && let ast::Number::Int(integer) = number.value()
                    {
                        let _ = write!(buffer, "{integer}");
                    } else {
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Only integer literals are allowed in subscript expressions \
                                    in help end escape command"
                                    .to_string(),
                            ),
                            slice.range(),
                        );
                        buffer.push_str(parser.src_text(slice.range()));
                    }

                    buffer.push(']');
                }
                ExprKind::Attribute(attribute) => {
                    let value = id::Node::from_view(attribute.value());
                    let attr = id::Node::from_view(attribute.attr());
                    unparse_expr(parser, value, buffer);
                    buffer.push('.');
                    buffer.push_str(attr.view(&parser.storage).as_str());
                }
                _ => {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "Expected name, subscript or attribute expression \
                                in help end escape command"
                                .to_string(),
                        ),
                        expr,
                    );
                }
            }
        }

        let start = self.node_start();
        self.bump(TokenKind::Question);

        let kind = if self.eat(TokenKind::Question) {
            IpyEscapeKind::Help2
        } else {
            IpyEscapeKind::Help
        };

        if parsed_expr.is_parenthesized {
            let token_range = self.node_range(start);
            self.add_error(
                ParseErrorType::OtherError(
                    "Help end escape command cannot be applied on a parenthesized expression"
                        .to_string(),
                ),
                token_range,
            );
        }

        if self.at(TokenKind::Question) {
            self.add_error(
                ParseErrorType::OtherError(
                    "Maximum of 2 `?` tokens are allowed in help end escape command".to_string(),
                ),
                self.current_token_range(),
            );
        }

        let mut value = String::new();
        unparse_expr(self, parsed_expr.expr, &mut value);

        build::StmtIpyEscapeCommand {
            value: value.into_boxed_str(),
            kind,
            range: self.node_range(parsed_expr.start()),
        }
        .parsed(&mut self.storage)
    }

    /// Parse an assignment statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `=` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#assignment-statements>
    fn parse_assign_statement(&mut self, target: ParsedExpr, start: TextSize) -> id::StmtAssign {
        self.bump(TokenKind::Equal);

        let mut targets = vec![target.expr];

        // test_err assign_stmt_missing_rhs
        // x =
        // 1 + 1
        // x = y =
        // 2 + 2
        // x = = y
        // 3 + 3

        // test_err assign_stmt_keyword_target
        // a = pass = c
        // a + b
        // a = b = pass = c
        // a + b

        // test_err assign_stmt_invalid_value_expr
        // x = (*a and b,)
        // x = (42, *yield x)
        // x = (42, *yield from x)
        // x = (*lambda x: x,)
        // x = x := 1

        let mut value =
            self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

        if self.at(TokenKind::Equal) {
            // This path is only taken when there are more than one assignment targets.
            self.parse_list(RecoveryContextKind::AssignmentTargets, |parser| {
                parser.bump(TokenKind::Equal);

                let mut parsed_expr =
                    parser.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

                std::mem::swap(&mut value, &mut parsed_expr);

                targets.push(parsed_expr.expr);
            });
        }

        for target in &mut targets {
            self.storage.set_expr_context(target.id, ExprContext::Store);
            // test_err assign_stmt_invalid_target
            // 1 = 1
            // x = 1 = 2
            // x = 1 = y = 2 = z
            // ["a", "b"] = ["a", "b"]
            self.validate_assignment_target(target);
        }

        build::StmtAssign {
            targets: self.storage.list(targets).into_list(&mut self.storage),
            value: value.expr.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an annotated assignment statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `:` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#annotated-assignment-statements>
    fn parse_annotated_assignment_statement(
        &mut self,
        target: ParsedExpr,
        start: TextSize,
    ) -> id::StmtAnnAssign {
        self.bump(TokenKind::Colon);

        // test_err ann_assign_stmt_invalid_target
        // "abc": str = "def"
        // call(): str = "no"
        // *x: int = 1, 2
        // # Tuple assignment
        // x,: int = 1
        // x, y: int = 1, 2
        // (x, y): int = 1, 2
        // # List assignment
        // [x]: int = 1
        // [x, y]: int = 1, 2
        self.validate_annotated_assignment_target(&target.expr);

        self.storage
            .set_expr_context(target.expr.id, ExprContext::Store);

        // test_ok ann_assign_stmt_simple_target
        // a: int  # simple
        // (a): int
        // a.b: int
        // a[0]: int
        let simple = target.is_name_expr() && !target.is_parenthesized;

        // test_err ann_assign_stmt_invalid_annotation
        // x: *int = 1
        // x: yield a = 1
        // x: yield from b = 1
        // x: y := int = 1

        // test_err ann_assign_stmt_type_alias_annotation
        // a: type X = int
        // lambda: type X = int
        let annotation = self.parse_conditional_expression_or_higher();

        let value = if self.eat(TokenKind::Equal) {
            if self.at_expr() {
                // test_err ann_assign_stmt_invalid_value
                // x: Any = *a and b
                // x: Any = x := 1
                // x: list = [x, *a | b, *a or b]
                Some(
                    self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or())
                        .expr,
                )
            } else {
                // test_err ann_assign_stmt_missing_rhs
                // x: int =
                self.add_error(
                    ParseErrorType::ExpectedExpression,
                    self.current_token_range(),
                );
                None
            }
        } else {
            None
        };

        build::StmtAnnAssign {
            target: target.expr.id,
            annotation: annotation.expr.id,
            value: value.map(|node| node.id),
            simple,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an augmented assignment statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an augmented assignment token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#augmented-assignment-statements>
    fn parse_augmented_assignment_statement(
        &mut self,
        target: ParsedExpr,
        op: Operator,
        start: TextSize,
    ) -> id::StmtAugAssign {
        // Consume the operator
        self.bump_ts(AUGMENTED_ASSIGN_SET);

        if !matches!(
            &target.expr.kind,
            ast::NodeKind::ExprName | ast::NodeKind::ExprAttribute | ast::NodeKind::ExprSubscript
        ) {
            // test_err aug_assign_stmt_invalid_target
            // 1 += 1
            // "a" += "b"
            // *x += 1
            // pass += 1
            // x += pass
            // (x + y) += 1
            self.add_error(ParseErrorType::InvalidAugmentedAssignmentTarget, target);
        }

        self.storage
            .set_expr_context(target.expr.id, ExprContext::Store);

        // test_err aug_assign_stmt_missing_rhs
        // x +=
        // 1 + 1
        // x += y +=
        // 2 + 2

        // test_err aug_assign_stmt_invalid_value
        // x += *a and b
        // x += *yield x
        // x += *yield from x
        // x += *lambda x: x
        // x += y := 1
        let value = self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

        build::StmtAugAssign {
            target: target.expr.id,
            op,
            value: value.expr.id,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `if` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `if` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-if-statement>
    fn parse_if_statement(&mut self) -> id::StmtIf {
        let start = self.node_start();
        self.bump(TokenKind::If);

        // test_err if_stmt_invalid_test_expr
        // if *x: ...
        // if yield x: ...
        // if yield from x: ...

        // test_err if_stmt_missing_test
        // if : ...
        let test = self.parse_named_expression_or_higher(ExpressionContext::default());

        // test_err if_stmt_missing_colon
        // if x
        // if x
        //     pass
        // a = 1
        self.expect(TokenKind::Colon);

        // test_err if_stmt_empty_body
        // if True:
        // 1 + 1
        let body = self.parse_body(Clause::If);

        // test_err if_stmt_misspelled_elif
        // if True:
        //     pass
        // elf:
        //     pass
        // else:
        //     pass
        let elif_else_snapshot = self.elif_else_scratch.snapshot();
        self.parse_clauses(Clause::ElIf, |parser| {
            let clause = parser.parse_elif_or_else_clause(ElifOrElse::Elif);
            parser.elif_else_scratch.push(clause.id);
        });

        if self.at(TokenKind::Else) {
            let clause = self.parse_elif_or_else_clause(ElifOrElse::Else);
            self.elif_else_scratch.push(clause.id);
        }

        build::StmtIf {
            test: test.expr.id,
            body: body.into_list(&mut self.storage),
            elif_else_clauses: self
                .elif_else_scratch
                .take_range(elif_else_snapshot, &mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `elif` or `else` clause.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `elif` or `else` token.
    fn parse_elif_or_else_clause(&mut self, kind: ElifOrElse) -> id::ElifElseClause {
        let start = self.node_start();
        self.bump(kind.as_token_kind());

        let test = if kind.is_elif() {
            // test_err if_stmt_invalid_elif_test_expr
            // if x:
            //     pass
            // elif *x:
            //     pass
            // elif yield x:
            //     pass
            Some(
                self.parse_named_expression_or_higher(ExpressionContext::default())
                    .expr,
            )
        } else {
            None
        };

        // test_err if_stmt_elif_missing_colon
        // if x:
        //     pass
        // elif y
        //     pass
        // else:
        //     pass
        self.expect(TokenKind::Colon);

        let body = self.parse_body(kind.as_clause());

        build::ElifElseClause {
            test: test.map(|node| node.id),
            body: body.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `try` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `try` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-try-statement>
    fn parse_try_statement(&mut self) -> id::StmtTry {
        let try_start = self.node_start();
        self.bump(TokenKind::Try);
        self.expect(TokenKind::Colon);

        let mut is_star: Option<bool> = None;

        let try_body = self.parse_body(Clause::Try);

        let has_except = self.at(TokenKind::Except);

        // test_err try_stmt_mixed_except_kind
        // try:
        //     pass
        // except:
        //     pass
        // except* ExceptionGroup:
        //     pass
        // try:
        //     pass
        // except* ExceptionGroup:
        //     pass
        // except:
        //     pass
        // try:
        //     pass
        // except:
        //     pass
        // except:
        //     pass
        // except* ExceptionGroup:
        //     pass
        // except* ExceptionGroup:
        //     pass
        let mut mixed_except_ranges = Vec::new();
        let mut handlers = Vec::new();
        self.parse_clauses(Clause::Except, |p| {
            let (handler, kind) = p.parse_except_clause();
            if let ExceptClauseKind::Star(range) = kind {
                p.add_unsupported_syntax_error(UnsupportedSyntaxErrorKind::ExceptStar, range);
            }
            if is_star.is_none() {
                is_star = Some(kind.is_star());
            } else if is_star != Some(kind.is_star()) {
                mixed_except_ranges.push(handler.range());
            }
            if handlers.is_empty() {
                handlers.reserve_exact(1);
            }
            handlers.push(handler);
        });
        handlers.shrink_to_fit();

        // Empty handler has `is_star` false.
        let is_star = is_star.unwrap_or_default();
        for handler_err_range in mixed_except_ranges {
            self.add_error(
                ParseErrorType::OtherError(
                    "Cannot have both 'except' and 'except*' on the same 'try'".to_string(),
                ),
                handler_err_range,
            );
        }

        // test_err try_stmt_misspelled_except
        // try:
        //     pass
        // exept:  # spellchecker:disable-line
        //     pass
        // finally:
        //     pass
        // a = 1
        // try:
        //     pass
        // except:
        //     pass
        // exept:  # spellchecker:disable-line
        //     pass
        // b = 1

        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon);
            self.parse_body(Clause::Else)
        } else {
            ListId::default()
        };

        let (finalbody, has_finally) = if self.eat(TokenKind::Finally) {
            self.expect(TokenKind::Colon);
            (self.parse_body(Clause::Finally), true)
        } else {
            (ListId::default(), false)
        };

        if !has_except && !has_finally {
            // test_err try_stmt_missing_except_finally
            // try:
            //     pass
            // try:
            //     pass
            // else:
            //     pass
            self.add_error(
                ParseErrorType::OtherError(
                    "Expected `except` or `finally` after `try` block".to_string(),
                ),
                self.current_token_range(),
            );
        }

        if has_finally && self.at(TokenKind::Else) {
            // test_err try_stmt_invalid_order
            // try:
            //     pass
            // finally:
            //     pass
            // else:
            //     pass
            self.add_error(
                ParseErrorType::OtherError(
                    "`else` block must come before `finally` block".to_string(),
                ),
                self.current_token_range(),
            );
        }

        // test_ok except_star_py311
        // # parse_options: {"target-version": "3.11"}
        // try: ...
        // except* ValueError: ...

        // test_err except_star_py310
        // # parse_options: {"target-version": "3.10"}
        // try: ...
        // except* ValueError: ...
        // except* KeyError: ...
        // except    *     Error: ...

        build::StmtTry {
            body: try_body.into_list(&mut self.storage),
            handlers: handlers.into_list(&mut self.storage),
            orelse: orelse.into_list(&mut self.storage),
            finalbody: finalbody.into_list(&mut self.storage),
            is_star,
            range: self.node_range(try_start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses an `except` clause of a `try` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `except` token.
    fn parse_except_clause(&mut self) -> (id::ExceptHandler, ExceptClauseKind) {
        let start = self.node_start();
        self.bump(TokenKind::Except);

        let star_token_range = self.current_token_range();
        let block_kind = if self.eat(TokenKind::Star) {
            ExceptClauseKind::Star(star_token_range)
        } else {
            ExceptClauseKind::Normal
        };

        let type_ = if self.at_expr() {
            // test_err except_stmt_invalid_expression
            // try:
            //     pass
            // except yield x:
            //     pass
            // try:
            //     pass
            // except* *x:
            //     pass
            let parsed_expr = self.parse_expression_list(ExpressionContext::default());
            if parsed_expr
                .expr
                .view(&self.storage)
                .cast::<ast::ExprTuple>()
                .is_some_and(|tuple| !tuple.parenthesized())
            {
                if self.at(TokenKind::As) {
                    // test_err except_stmt_unparenthesized_tuple_as
                    // try:
                    //     pass
                    // except x, y as exc:
                    //     pass
                    // try:
                    //     pass
                    // except* x, y as eg:
                    //     pass
                    self.add_error(
                        ParseErrorType::OtherError(
                            "Multiple exception types must be parenthesized when using `as`"
                                .to_string(),
                        ),
                        parsed_expr,
                    );
                } else {
                    // test_err except_stmt_unparenthesized_tuple_no_as_py313
                    // # parse_options: {"target-version": "3.13"}
                    // try:
                    //     pass
                    // except x, y:
                    //     pass
                    // try:
                    //     pass
                    // except* x, y:
                    //     pass

                    // test_ok except_stmt_unparenthesized_tuple_no_as_py314
                    // # parse_options: {"target-version": "3.14"}
                    // try:
                    //     pass
                    // except x, y:
                    //     pass
                    // try:
                    //     pass
                    // except* x, y:
                    //     pass
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::UnparenthesizedExceptionTypes,
                        parsed_expr.range(),
                    );
                }
            }
            Some(parsed_expr.expr)
        } else {
            if block_kind.is_star() || self.at(TokenKind::As) {
                // test_err except_stmt_missing_exception
                // try:
                //     pass
                // except as exc:
                //     pass
                // # If a '*' is present then exception type is required
                // try:
                //     pass
                // except*:
                //     pass
                // except*
                //     pass
                // except* as exc:
                //     pass
                self.add_error(
                    ParseErrorType::OtherError("Expected one or more exception types".to_string()),
                    self.current_token_range(),
                );
            }
            None
        };

        let name = if self.eat(TokenKind::As) {
            if self.at_identifier_or_soft_keyword() {
                // test_ok except_stmt_as_name_soft_keyword
                // try: ...
                // except Exception as match: ...
                // except Exception as case: ...
                // except Exception as type: ...
                Some(self.parse_identifier())
            } else {
                // test_err except_stmt_missing_as_name
                // try:
                //     pass
                // except Exception as:
                //     pass
                // except Exception as
                //     pass
                self.add_error(
                    ParseErrorType::OtherError("Expected name after `as`".to_string()),
                    self.current_token_range(),
                );
                None
            }
        } else {
            None
        };

        // test_err except_stmt_missing_exception_and_as_name
        // try:
        //     pass
        // except as:
        //     pass

        self.expect(TokenKind::Colon);

        let except_body = self.parse_body(Clause::Except);

        (
            id::ExceptHandler::from(
                build::ExceptHandlerExceptHandler {
                    type_: type_.map(|node| node.id),
                    name: name.map(|node| node.id),
                    body: except_body.into_list(&mut self.storage),
                    range: self.node_range(start),
                }
                .parsed(&mut self.storage),
            ),
            block_kind,
        )
    }

    /// Parses a `for` statement.
    ///
    /// The given `start` offset is the start of either the `for` token or the
    /// `async` token if it's an async for statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `for` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-for-statement>
    fn parse_for_statement(&mut self, start: TextSize, is_async: bool) -> id::StmtFor {
        self.bump(TokenKind::For);

        // test_err for_stmt_missing_target
        // for in x: ...

        // test_ok for_in_target_valid_expr
        // for d[x in y] in target: ...
        // for (x in y)[0] in iter: ...
        // for (x in y).attr in iter: ...

        // test_err for_stmt_invalid_target_in_keyword
        // for d(x in y) in target: ...
        // for (x in y)() in iter: ...
        // for (x in y) in iter: ...
        // for (x in y, z) in iter: ...
        // for [x in y, z] in iter: ...
        // for {x in y, z} in iter: ...

        // test_err for_stmt_invalid_target_binary_expr
        // for x not in y in z: ...
        // for x == y in z: ...
        // for x or y in z: ...
        // for -x in y: ...
        // for not x in y: ...
        // for x | y in z: ...
        let target =
            self.parse_expression_list(ExpressionContext::starred_conditional().with_in_excluded());

        self.storage
            .set_expr_context(target.expr.id, ExprContext::Store);

        // test_err for_stmt_invalid_target
        // for 1 in x: ...
        // for "a" in x: ...
        // for *x and y in z: ...
        // for *x | y in z: ...
        // for await x in z: ...
        // for yield x in y: ...
        // for [x, 1, y, *["a"]] in z: ...
        self.validate_assignment_target(&target.expr);

        // test_err for_stmt_missing_in_keyword
        // for a b: ...
        // for a: ...
        self.expect(TokenKind::In);

        // test_err for_stmt_missing_iter
        // for x in:
        //     a = 1

        // test_err for_stmt_invalid_iter_expr
        // for x in *a and b: ...
        // for x in yield a: ...
        // for target in x := 1: ...
        let iter = self.parse_expression_list(ExpressionContext::starred_bitwise_or());

        // test_ok for_iter_unpack_py39
        // # parse_options: {"target-version": "3.9"}
        // for x in *a,  b: ...
        // for x in  a, *b: ...
        // for x in *a, *b: ...

        // test_ok for_iter_unpack_py38
        // # parse_options: {"target-version": "3.8"}
        // for x in (*a,  b): ...
        // for x in ( a, *b): ...
        // for x in (*a, *b): ...

        // test_err for_iter_unpack_py38
        // # parse_options: {"target-version": "3.8"}
        // for x in *a,  b: ...
        // for x in  a, *b: ...
        // for x in *a, *b: ...
        self.check_tuple_unpacking(
            &iter,
            UnsupportedSyntaxErrorKind::UnparenthesizedUnpackInFor,
        );

        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::For);

        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon);
            self.parse_body(Clause::Else)
        } else {
            ListId::default()
        };

        build::StmtFor {
            target: target.expr.id,
            iter: iter.expr.id,
            is_async,
            body: body.into_list(&mut self.storage),
            orelse: orelse.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `while` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `while` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-while-statement>
    fn parse_while_statement(&mut self) -> id::StmtWhile {
        let start = self.node_start();
        self.bump(TokenKind::While);

        // test_err while_stmt_missing_test
        // while : ...
        // while :
        //     a = 1

        // test_err while_stmt_invalid_test_expr
        // while *x: ...
        // while yield x: ...
        // while a, b: ...
        // while a := 1, b: ...
        let test = self.parse_named_expression_or_higher(ExpressionContext::default());

        // test_err while_stmt_missing_colon
        // while (
        //     a < 30 # comment
        // )
        //     pass
        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::While);

        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon);
            self.parse_body(Clause::Else)
        } else {
            ListId::default()
        };

        build::StmtWhile {
            test: test.expr.id,
            body: body.into_list(&mut self.storage),
            orelse: orelse.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a function definition.
    ///
    /// The given `start` offset is the start of either of the following:
    /// - `def` token
    /// - `async` token if it's an asynchronous function definition with no decorators
    /// - `@` token if the function definition has decorators
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `def` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#function-definitions>
    fn parse_function_definition(
        &mut self,
        decorator_list: Vec<id::Node<ast::kind::Decorator>>,
        start: TextSize,
        is_async: bool,
    ) -> id::StmtFunctionDef {
        self.bump(TokenKind::Def);

        // test_err function_def_missing_identifier
        // def (): ...
        // def () -> int: ...
        let name = self.parse_identifier();

        // test_err function_def_unclosed_type_param_list
        // def foo[T1, *T2(a, b):
        //     return a + b
        // x = 10
        let type_params = self.try_parse_type_params();

        // test_ok function_type_params_py312
        // # parse_options: {"target-version": "3.12"}
        // def foo[T](): ...

        // test_err function_type_params_py311
        // # parse_options: {"target-version": "3.11"}
        // def foo[T](): ...
        // def foo[](): ...
        if let Some(params) = &type_params {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::TypeParameterList,
                params.range(),
            );
        }

        // test_ok function_def_parameter_range
        // def foo(
        //     first: int,
        //     second: int,
        // ) -> int: ...

        // test_err function_def_unclosed_parameter_list
        // def foo(a: int, b:
        // def foo():
        //     return 42
        // def foo(a: int, b: str
        // x = 10
        let parameters = self.parse_parameters(FunctionKind::FunctionDef);

        let returns = if self.eat(TokenKind::Rarrow) {
            if self.at_expr() {
                // test_ok function_def_valid_return_expr
                // def foo() -> int | str: ...
                // def foo() -> lambda x: x: ...
                // def foo() -> int if True else str: ...

                // test_err function_def_invalid_return_expr
                // def foo() -> *int: ...
                // def foo() -> (*int): ...
                // def foo() -> yield x: ...
                let returns = self.parse_expression_list(ExpressionContext::default());

                if returns
                    .expr
                    .view(&self.storage)
                    .cast::<ast::ExprTuple>()
                    .is_some_and(|tuple| !tuple.parenthesized())
                {
                    // test_ok function_def_parenthesized_return_types
                    // def foo() -> (int,): ...
                    // def foo() -> (int, str): ...

                    // test_err function_def_unparenthesized_return_types
                    // def foo() -> int,: ...
                    // def foo() -> int, str: ...
                    self.add_error(
                        ParseErrorType::OtherError(
                            "Multiple return types must be parenthesized".to_string(),
                        ),
                        returns.range(),
                    );
                }

                Some(returns.expr)
            } else {
                // test_err function_def_missing_return_type
                // def foo() -> : ...
                self.add_error(
                    ParseErrorType::ExpectedExpression,
                    self.current_token_range(),
                );

                None
            }
        } else {
            None
        };

        self.expect(TokenKind::Colon);

        // test_err function_def_empty_body
        // def foo():
        // def foo() -> int:
        // x = 42
        let body = self.parse_body(Clause::FunctionDef);

        build::StmtFunctionDef {
            name: name.id,
            type_params: type_params.map(|node| node.id),
            parameters: parameters.id,
            body: body.into_list(&mut self.storage),
            decorator_list: decorator_list.into_list(&mut self.storage),
            is_async,
            returns: returns.map(|node| node.id),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a class definition.
    ///
    /// The given `start` offset is the start of either the `def` token or the
    /// `@` token if the class definition has decorators.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `class` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-classdef>
    fn parse_class_definition(
        &mut self,
        decorator_list: Vec<id::Node<ast::kind::Decorator>>,
        start: TextSize,
    ) -> id::StmtClassDef {
        self.bump(TokenKind::Class);

        // test_err class_def_missing_name
        // class : ...
        // class (): ...
        // class (metaclass=ABC): ...
        let name = self.parse_identifier();

        // test_err class_def_unclosed_type_param_list
        // class Foo[T1, *T2(a, b):
        //     pass
        // x = 10
        let type_params = self.try_parse_type_params();

        // test_ok class_type_params_py312
        // # parse_options: {"target-version": "3.12"}
        // class Foo[S: (str, bytes), T: float, *Ts, **P]: ...

        // test_err class_type_params_py311
        // # parse_options: {"target-version": "3.11"}
        // class Foo[S: (str, bytes), T: float, *Ts, **P]: ...
        // class Foo[]: ...
        if let Some(params) = &type_params {
            self.add_unsupported_syntax_error(
                UnsupportedSyntaxErrorKind::TypeParameterList,
                params.range(),
            );
        }

        // test_ok class_def_arguments
        // class Foo: ...
        // class Foo(): ...
        // class Foo((base for base in bases)): ...
        // class Foo(*(base for base in bases)): ...

        // test_err class_def_unparenthesized_generator_argument
        // class Foo(base for base in bases): ...
        let arguments = self
            .at(TokenKind::Lpar)
            .then(|| self.parse_arguments(ArgumentsContext::ClassDefinition));

        self.expect(TokenKind::Colon);

        // test_err class_def_empty_body
        // class Foo:
        // class Foo():
        // x = 42
        let body = self.parse_body(Clause::Class);

        build::StmtClassDef {
            range: self.node_range(start),
            decorator_list: decorator_list.into_list(&mut self.storage),
            name: name.id,
            type_params: type_params.map(|node| node.id),
            arguments: arguments.map(|node| node.id),
            body: body.into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a `with` statement
    ///
    /// The given `start` offset is the start of either the `with` token or the
    /// `async` token if it's an async with statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `with` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-with-statement>
    fn parse_with_statement(&mut self, start: TextSize, is_async: bool) -> id::StmtWith {
        self.bump(TokenKind::With);

        let mut items = self.parse_with_items();
        items.shrink_to_fit();

        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::With);

        build::StmtWith {
            items: items.into_list(&mut self.storage),
            body: body.into_list(&mut self.storage),
            is_async,
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a list of with items.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-with-statement>
    fn parse_with_items(&mut self) -> Vec<id::WithItem> {
        if !self.at_expr() {
            self.add_error(
                ParseErrorType::OtherError(
                    "Expected the start of an expression after `with` keyword".to_string(),
                ),
                self.current_token_range(),
            );
            return vec![];
        }

        let open_paren_range = self.current_token_range();

        if self.at(TokenKind::Lpar) {
            if let (Some(items), has_trailing_comma) = self.try_parse_parenthesized_with_items() {
                // test_ok tuple_context_manager_py38
                // # parse_options: {"target-version": "3.8"}
                // with (
                //   foo,
                //   bar,
                //   baz,
                // ) as tup: ...

                // test_ok single_parenthesized_item_context_manager_py38
                // # parse_options: {"target-version": "3.8"}
                // with (
                //   open('foo.txt')) as foo: ...
                // with (
                //   open('foo.txt')): ...

                // test_err tuple_context_manager_py38
                // # parse_options: {"target-version": "3.8"}
                // # these cases are _syntactically_ valid before Python 3.9 because the `with` item
                // # is parsed as a tuple, but this will always cause a runtime error, so we flag it
                // # anyway
                // with (foo, bar): ...
                // with (
                //   foo,
                //   bar,
                //   baz,
                // ): ...
                // with (foo,): ...

                // test_ok parenthesized_context_manager_py39
                // # parse_options: {"target-version": "3.9"}
                // with (foo as x, bar as y): ...
                // with (foo, bar as y): ...
                // with (foo as x, bar): ...

                // test_err parenthesized_context_manager_py38
                // # parse_options: {"target-version": "3.8"}
                // with (foo as x, bar as y): ...
                // with (foo, bar as y): ...
                // with (foo as x, bar): ...
                if items.len() > 1 || has_trailing_comma {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::ParenthesizedContextManager,
                        open_paren_range,
                    );
                }

                self.expect(TokenKind::Rpar);
                items
            } else {
                // test_ok ambiguous_lpar_with_items_if_expr
                // with (x) if True else y: ...
                // with (x for x in iter) if True else y: ...
                // with (x async for x in iter) if True else y: ...
                // with (x)[0] if True else y: ...

                // test_ok ambiguous_lpar_with_items_binary_expr
                // # It doesn't matter what's inside the parentheses, these tests need to make sure
                // # all binary expressions parses correctly.
                // with (a) and b: ...
                // with (a) is not b: ...
                // # Make sure precedence works
                // with (a) or b and c: ...
                // with (a) and b or c: ...
                // with (a | b) << c | d: ...
                // # Postfix should still be parsed first
                // with (a)[0] + b * c: ...
                self.parse_comma_separated_list_into_vec_with_capacity(
                    RecoveryContextKind::WithItems(WithItemKind::ParenthesizedExpression),
                    |p| p.parse_with_item(WithItemParsingState::Regular).item,
                    1,
                )
            }
        } else {
            self.parse_comma_separated_list_into_vec_with_capacity(
                RecoveryContextKind::WithItems(WithItemKind::Unparenthesized),
                |p| p.parse_with_item(WithItemParsingState::Regular).item,
                1,
            )
        }
    }

    /// Try parsing with-items coming after an ambiguous `(` token.
    ///
    /// To understand the ambiguity, consider the following example:
    ///
    /// ```python
    /// with (item1, item2): ...       # Parenthesized with items
    /// with (item1, item2) as f: ...  # Parenthesized expression
    /// ```
    ///
    /// When the parser is at the `(` token after the `with` keyword, it doesn't know if `(` is
    /// used to parenthesize the with items or if it's part of a parenthesized expression of the
    /// first with item. The challenge here is that until the parser sees the matching `)` token,
    /// it can't resolve the ambiguity.
    ///
    /// This method resolves the ambiguity using speculative parsing. It starts with an assumption
    /// that it's a parenthesized with items. Then, once it finds the matching `)`, it checks if
    /// the assumption still holds true. If the initial assumption was correct, this will return
    /// the parsed with items. Otherwise, rewind the parser back to the starting `(` token,
    /// returning [`None`].
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `(` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-with_stmt_contents>
    fn try_parse_parenthesized_with_items(&mut self) -> (Option<Vec<id::WithItem>>, bool) {
        let checkpoint = self.checkpoint();

        // We'll start with the assumption that the with items are parenthesized.
        let mut with_item_kind = WithItemKind::Parenthesized;

        self.bump(TokenKind::Lpar);

        let mut parsed_with_items = Vec::with_capacity(1);
        let mut has_optional_vars = false;

        // test_err with_items_parenthesized_missing_comma
        // with (item1 item2): ...
        // with (item1 as f1 item2): ...
        // with (item1, item2 item3, item4): ...
        // with (item1, item2 as f1 item3, item4): ...
        // with (item1, item2: ...
        let has_trailing_comma =
            self.parse_comma_separated_list(RecoveryContextKind::WithItems(with_item_kind), |p| {
                let parsed_with_item = p.parse_with_item(WithItemParsingState::Speculative);
                has_optional_vars |= parsed_with_item
                    .item
                    .view(&p.storage)
                    .optional_vars()
                    .is_some();
                parsed_with_items.push(parsed_with_item);
            });

        // Check if our assumption is incorrect and it's actually a parenthesized expression.
        if has_optional_vars {
            // If any of the with item has optional variables, then our assumption is correct
            // and it is a parenthesized with items. Now, we need to restrict the grammar for a
            // with item's context expression which is:
            //
            //     with_item: expression ...
            //
            // So, named, starred and yield expressions not allowed.
            for parsed_with_item in &parsed_with_items {
                if parsed_with_item.is_parenthesized {
                    // Parentheses resets the precedence.
                    continue;
                }
                let error = match parsed_with_item
                    .item
                    .view(&self.storage)
                    .context_expr()
                    .classify()
                {
                    ExprKind::Named(_) => ParseErrorType::UnparenthesizedNamedExpression,
                    ExprKind::Starred(_) => ParseErrorType::InvalidStarredExpressionUsage,
                    ExprKind::Yield(_) | ExprKind::YieldFrom(_) => {
                        ParseErrorType::InvalidYieldExpressionUsage
                    }
                    _ => continue,
                };
                self.add_error(
                    error,
                    parsed_with_item
                        .item
                        .view(&self.storage)
                        .context_expr()
                        .range(),
                );
            }
        } else if self.at(TokenKind::Rpar)
            && (
                // test_err with_items_parenthesized_missing_colon
                // # `)` followed by a newline
                // with (item1, item2)
                //     pass
                matches!(self.peek(), TokenKind::Colon | TokenKind::Newline)
            )
        {
            if parsed_with_items.is_empty() {
                // No with items, treat it as a parenthesized expression to create an empty
                // tuple expression.
                with_item_kind = WithItemKind::ParenthesizedExpression;
            } else {
                // These expressions, if unparenthesized, are only allowed if it's a
                // parenthesized expression and none of the with items have an optional
                // variable.
                if parsed_with_items.iter().any(|parsed_with_item| {
                    !parsed_with_item.is_parenthesized
                        && matches!(
                            parsed_with_item
                                .item
                                .view(&self.storage)
                                .context_expr()
                                .classify(),
                            ExprKind::Named(_)
                                | ExprKind::Starred(_)
                                | ExprKind::Yield(_)
                                | ExprKind::YieldFrom(_)
                        )
                }) {
                    with_item_kind = WithItemKind::ParenthesizedExpression;
                }
            }
        } else {
            // For any other token followed by `)`, if any of the items has an optional
            // variables (`as ...`), then our assumption is correct. Otherwise, treat
            // it as a parenthesized expression. For example:
            //
            // ```python
            // with (item1, item2 as f): ...
            // ```
            //
            // This also helps in raising the correct syntax error for the following
            // case:
            // ```python
            // with (item1, item2 as f) as x: ...
            // #                        ^^
            // #                        Expecting `:` but got `as`
            // ```
            with_item_kind = WithItemKind::ParenthesizedExpression;
        }

        let with_items = if with_item_kind.is_parenthesized() {
            let with_items: Vec<_> = parsed_with_items
                .into_iter()
                .map(|parsed_with_item| parsed_with_item.item)
                .collect();

            Some(with_items)
        } else {
            self.rewind(checkpoint);

            None
        };

        (with_items, has_trailing_comma)
    }

    /// Parses a single `with` item.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-with_item>
    fn parse_with_item(&mut self, state: WithItemParsingState) -> ParsedWithItem {
        let start = self.node_start();

        // The grammar for the context expression of a with item depends on the state
        // of with item parsing.
        let context_expr = match state {
            WithItemParsingState::Speculative => {
                // If it's in a speculative state, the parenthesis (`(`) could be part of any of the
                // following expression:
                //
                // Tuple expression          -  star_named_expression
                // Generator expression      -  named_expression
                // Parenthesized expression  -  (yield_expr | named_expression)
                // Parenthesized with items  -  expression
                //
                // Here, the right side specifies the grammar for an element corresponding to the
                // expression mentioned in the left side.
                //
                // So, the grammar used should be able to parse an element belonging to any of the
                // above expression. At a later point, once the parser understands where the
                // parenthesis belongs to, it'll validate and report errors for any invalid expression
                // usage.
                //
                // Thus, we can conclude that the grammar used should be:
                //      (yield_expr | star_named_expression)
                self.parse_named_expression_or_higher(
                    ExpressionContext::yield_or_starred_bitwise_or(),
                )
            }
            WithItemParsingState::Regular => self.parse_conditional_expression_or_higher(),
        };

        let optional_vars = self
            .at(TokenKind::As)
            .then(|| self.parse_with_item_optional_vars().expr);

        ParsedWithItem {
            is_parenthesized: context_expr.is_parenthesized,
            item: build::WithItem {
                range: self.node_range(start),
                context_expr: context_expr.expr.id,
                optional_vars: optional_vars.map(|node| node.id),
            }
            .parsed(&mut self.storage),
        }
    }

    /// Parses the optional variables in a `with` item.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `as` token.
    fn parse_with_item_optional_vars(&mut self) -> ParsedExpr {
        self.bump(TokenKind::As);

        let target = self
            .parse_conditional_expression_or_higher_impl(ExpressionContext::starred_conditional());

        // This has the same semantics as an assignment target.
        self.validate_assignment_target(&target.expr);

        self.storage
            .set_expr_context(target.expr.id, ExprContext::Store);

        target
    }

    /// Consume `match` as a keyword and report an error if the compound statement follows
    /// a semicolon. If `match` turns out to be an identifier, rewinding discards the error.
    fn bump_match_statement_keyword(&mut self) {
        // test_ok semicolon_before_match_identifier
        // first(); match = value
        // first(); match(value)
        // first(); match[0]: int
        // first();
        // match (value):
        //     case _: pass

        // test_err semicolon_before_match_statement
        // first(); match value:
        //     case _: pass
        // first(); match (value):
        //     case _: pass
        if self.tokens.previous_kind() == Some(TokenKind::Semi) {
            self.add_error(
                ParseErrorType::SimpleAndCompoundStatementOnSameLine,
                self.current_token_range(),
            );
        }
        self.bump(TokenKind::Match);
    }

    /// Try parsing a `match` statement.
    ///
    /// This uses speculative parsing to remove the ambiguity of whether the `match` token is used
    /// as a keyword or an identifier. This ambiguity arises only in if the `match` token is
    /// followed by certain tokens. For example, if `match` is followed by `[`, we can't know if
    /// it's used in the context of a subscript expression or as a list expression:
    ///
    /// ```python
    /// # Subscript expression; `match` is an identifier
    /// match[x]
    ///
    /// # List expression; `match` is a keyword
    /// match [x, y]:
    ///     case [1, 2]:
    ///         pass
    /// ```
    ///
    /// This is done by parsing the subject expression considering `match` as a keyword token.
    /// Then, based on certain heuristics we'll determine if our assumption is true. If so, we'll
    /// continue parsing the entire match statement. Otherwise, return `None`.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `match` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-match-statement>
    fn try_parse_match_statement(&mut self) -> Option<id::StmtMatch> {
        let checkpoint = self.checkpoint();

        let start = self.node_start();
        self.bump_match_statement_keyword();

        let subject = self.parse_match_subject_expression();

        match self.current_token_kind() {
            // test_ok match_annotated_assignment
            // match[0]: int
            // match [x, y, z]: dict
            TokenKind::Colon if self.peek() == TokenKind::Newline => {
                // `match` is a keyword — colon followed by newline confirms
                // this is a match statement, not an annotated assignment like
                // `match [x, y, z]: {dict}` or `match[0]: int`.
                self.bump(TokenKind::Colon);

                let cases = self.parse_match_body();

                Some(
                    build::StmtMatch {
                        subject: subject.id,
                        cases: cases.into_list(&mut self.storage),
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            TokenKind::Newline if matches!(self.peek2(), (TokenKind::Indent, TokenKind::Case)) => {
                // `match` is a keyword

                // test_err match_expected_colon
                // match [1, 2]
                //     case _: ...
                self.add_error(
                    ParseErrorType::ExpectedToken {
                        found: self.current_token_kind(),
                        expected: TokenKind::Colon,
                    },
                    self.current_token_range(),
                );

                let cases = self.parse_match_body();

                Some(
                    build::StmtMatch {
                        subject: subject.id,
                        cases: cases.into_list(&mut self.storage),
                        range: self.node_range(start),
                    }
                    .parsed(&mut self.storage),
                )
            }
            _ => {
                // `match` is an identifier
                self.rewind(checkpoint);

                None
            }
        }
    }

    /// Parses a match statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `match` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-match-statement>
    fn parse_match_statement(&mut self) -> id::StmtMatch {
        let start = self.node_start();
        self.bump_match_statement_keyword();

        let match_range = self.node_range(start);

        let subject = self.parse_match_subject_expression();
        self.expect(TokenKind::Colon);

        let cases = self.parse_match_body();

        // test_err match_before_py310
        // # parse_options: { "target-version": "3.9" }
        // match 2:
        //     case 1:
        //         pass

        // test_ok match_after_py310
        // # parse_options: { "target-version": "3.10" }
        // match 2:
        //     case 1:
        //         pass

        self.add_unsupported_syntax_error(UnsupportedSyntaxErrorKind::Match, match_range);

        build::StmtMatch {
            subject: subject.id,
            cases: cases.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses the subject expression for a `match` statement.
    fn parse_match_subject_expression(&mut self) -> id::Expr {
        let start = self.node_start();

        // Subject expression grammar is:
        //
        //     subject_expr:
        //         | star_named_expression ',' star_named_expressions?
        //         | named_expression
        //
        // First try with `star_named_expression`, then if there's no comma,
        // we'll restrict it to `named_expression`.
        let subject =
            self.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or());

        // test_ok match_stmt_subject_expr
        // match x := 1:
        //     case _: ...
        // match (x := 1):
        //     case _: ...
        // # Starred expressions are only allowed in tuple expression
        // match *x | y, z:
        //     case _: ...
        // match await x:
        //     case _: ...

        // test_err match_stmt_invalid_subject_expr
        // match (*x):
        //     case _: ...
        // # Starred expression precedence test
        // match *x and y, z:
        //     case _: ...
        // match yield x:
        //     case _: ...
        if self.at(TokenKind::Comma) {
            let tuple = self.parse_tuple_expression(subject.expr, start, Parenthesized::No, |p| {
                p.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
            });

            id::Expr::from(tuple)
        } else {
            if subject.is_unparenthesized_starred_expr() {
                // test_err match_stmt_single_starred_subject
                // match *foo:
                //     case _: ...
                self.add_error(ParseErrorType::InvalidStarredExpressionUsage, subject);
            }
            subject.expr
        }
    }

    /// Parses the body of a `match` statement.
    ///
    /// This method expects that the parser is positioned at a `Newline` token. If not, it adds a
    /// syntax error and continues parsing.
    fn parse_match_body(&mut self) -> Vec<id::MatchCase> {
        // test_err match_stmt_no_newline_before_case
        // match foo: case _: ...
        self.expect(TokenKind::Newline);

        // Use `eat` instead of `expect` for better error message.
        if !self.eat(TokenKind::Indent) {
            // test_err match_stmt_expect_indented_block
            // match foo:
            // case _: ...
            self.add_error(
                ParseErrorType::OtherError(
                    "Expected an indented block after `match` statement".to_string(),
                ),
                self.current_token_range(),
            );
        }

        let cases = self.parse_match_case_blocks();

        // TODO(dhruvmanila): Should we expect `Dedent` only if there was an `Indent` present?
        self.expect(TokenKind::Dedent);

        cases
    }

    /// Parses a list of match case blocks.
    fn parse_match_case_blocks(&mut self) -> Vec<id::MatchCase> {
        let mut cases = vec![];

        if !self.at(TokenKind::Case) {
            // test_err match_stmt_expected_case_block
            // match x:
            //     x = 1
            // match x:
            //     match y:
            //         case _: ...
            self.add_error(
                ParseErrorType::OtherError("Expected `case` block".to_string()),
                self.current_token_range(),
            );
            return cases;
        }

        let mut progress = ParserProgress::default();

        while self.at(TokenKind::Case) {
            progress.assert_progressing(self);
            cases.push(self.parse_match_case());
        }

        cases
    }

    /// Parses a single match case block.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `case` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-case_block>
    fn parse_match_case(&mut self) -> id::MatchCase {
        let start = self.node_start();
        self.bump(TokenKind::Case);

        // test_err match_stmt_missing_pattern
        // match x:
        //     case : ...
        let pattern = self.parse_match_patterns();

        let guard = if self.eat(TokenKind::If) {
            if self.at_expr() {
                // test_ok match_stmt_valid_guard_expr
                // match x:
                //     case y if a := 1: ...
                // match x:
                //     case y if a if True else b: ...
                // match x:
                //     case y if lambda a: b: ...
                // match x:
                //     case y if (yield x): ...

                // test_err match_stmt_invalid_guard_expr
                // match x:
                //     case y if *a: ...
                // match x:
                //     case y if (*a): ...
                // match x:
                //     case y if yield x: ...
                Some(
                    self.parse_named_expression_or_higher(ExpressionContext::default())
                        .expr,
                )
            } else {
                // test_err match_stmt_missing_guard_expr
                // match x:
                //     case y if: ...
                self.add_error(
                    ParseErrorType::ExpectedExpression,
                    self.current_token_range(),
                );
                None
            }
        } else {
            None
        };

        self.expect(TokenKind::Colon);

        // test_err case_expect_indented_block
        // match subject:
        //     case 1:
        //     case 2: ...
        let body = self.parse_body(Clause::Case);

        build::MatchCase {
            pattern: pattern.id,
            guard: guard.map(|node| node.id),
            body: body.into_list(&mut self.storage),
            range: self.node_range(start),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a statement that is valid after an `async` token.
    ///
    /// If the statement is not a valid `async` statement, an error will be reported
    /// and it will be parsed as a statement.
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/compound_stmts.html#the-async-with-statement>
    /// - <https://docs.python.org/3/reference/compound_stmts.html#the-async-for-statement>
    /// - <https://docs.python.org/3/reference/compound_stmts.html#coroutine-function-definition>
    fn parse_async_statement(&mut self) -> id::Stmt {
        let async_start = self.node_start();
        self.bump(TokenKind::Async);

        match self.current_token_kind() {
            // test_ok async_function_definition
            // async def foo(): ...
            TokenKind::Def => {
                id::Stmt::from(self.parse_function_definition(Vec::new(), async_start, true))
            }

            // test_ok async_with_statement
            // async with item: ...
            TokenKind::With => id::Stmt::from(self.parse_with_statement(async_start, true)),

            // test_ok async_for_statement
            // async for target in iter: ...
            TokenKind::For => id::Stmt::from(self.parse_for_statement(async_start, true)),

            kind => {
                // test_err async_unexpected_token
                // async class Foo: ...
                // async while test: ...
                // async x = 1
                // async async def foo(): ...
                // async match test:
                //     case _: ...
                self.add_error(
                    ParseErrorType::UnexpectedTokenAfterAsync(kind),
                    self.current_token_range(),
                );

                // Although this statement is not a valid `async` statement,
                // we still parse it. Guard the recursive recovery path so
                // `async async async ...` cannot overflow the parser stack.
                self.with_recursion(Self::parse_statement)
            }
        }
    }

    /// Parses a decorator list followed by a class, function or async function definition.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-decorators>
    fn parse_decorators(&mut self) -> id::Stmt {
        let start = self.node_start();

        let mut decorators = Vec::new();
        let mut progress = ParserProgress::default();

        // test_err decorator_missing_expression
        // @def foo(): ...
        // @
        // def foo(): ...
        // @@
        // def foo(): ...
        // @test
        // @
        // class Test
        while self.at(TokenKind::At) {
            progress.assert_progressing(self);

            let decorator_start = self.node_start();
            self.bump(TokenKind::At);

            let parsed_expr = if self.at(TokenKind::Def) || self.at(TokenKind::Class) {
                id::Expr::from(self.parse_missing_name()).into()
            } else {
                self.parse_named_expression_or_higher(ExpressionContext::default())
            };

            if self.options.target_version < PythonVersion::PY39 {
                // test_ok decorator_expression_dotted_ident_py38
                // # parse_options: { "target-version": "3.8" }
                // @buttons.clicked.connect
                // def spam(): ...

                // test_ok decorator_expression_identity_hack_py38
                // # parse_options: { "target-version": "3.8" }
                // def _(x): return x
                // @_(buttons[0].clicked.connect)
                // def spam(): ...

                // test_ok decorator_expression_eval_hack_py38
                // # parse_options: { "target-version": "3.8" }
                // @eval("buttons[0].clicked.connect")
                // def spam(): ...

                // test_ok decorator_expression_py39
                // # parse_options: { "target-version": "3.9" }
                // @buttons[0].clicked.connect
                // def spam(): ...
                // @(x := lambda x: x)(foo)
                // def bar(): ...

                // test_err decorator_expression_py38
                // # parse_options: { "target-version": "3.8" }
                // @buttons[0].clicked.connect
                // def spam(): ...

                // test_err decorator_named_expression_py37
                // # parse_options: { "target-version": "3.7" }
                // @(x := lambda x: x)(foo)
                // def bar(): ...

                // test_err decorator_dict_literal_py38
                // # parse_options: { "target-version": "3.8" }
                // @{3: 3}
                // def bar(): ...

                // test_err decorator_float_literal_py38
                // # parse_options: { "target-version": "3.8" }
                // @3.14
                // def bar(): ...

                // test_ok decorator_await_expression_py39
                // # parse_options: { "target-version": "3.9" }
                // async def foo():
                //     @await bar
                //     def baz(): ...

                // test_err decorator_await_expression_py38
                // # parse_options: { "target-version": "3.8" }
                // async def foo():
                //     @await bar
                //     def baz(): ...

                // test_err decorator_non_toplevel_call_expression_py38
                // # parse_options: { "target-version": "3.8" }
                // @foo().bar()
                // def baz(): ...

                let value = parsed_expr.expr.view(&self.storage);
                let target = value
                    .cast::<ast::ExprCall>()
                    .map_or(value, ast::ExprCall::func);
                let relaxed_decorator_error =
                    helpers::detect_invalid_pre_py39_decorator_node(target);

                if let Some((error, range)) = relaxed_decorator_error {
                    self.add_unsupported_syntax_error(
                        UnsupportedSyntaxErrorKind::RelaxedDecorator(error),
                        range,
                    );
                }
            }

            // test_err decorator_invalid_expression
            // @*x
            // @(*x)
            // @((*x))
            // @yield x
            // @yield from x
            // def foo(): ...

            decorators.push(
                build::Decorator {
                    expression: parsed_expr.expr.id,
                    range: self.node_range(decorator_start),
                }
                .parsed(&mut self.storage),
            );

            // test_err decorator_missing_newline
            // @x def foo(): ...
            // @x async def foo(): ...
            // @x class Foo: ...
            self.expect(TokenKind::Newline);
        }

        decorators.shrink_to_fit();

        match self.current_token_kind() {
            TokenKind::Def => {
                id::Stmt::from(self.parse_function_definition(decorators, start, false))
            }
            TokenKind::Class => id::Stmt::from(self.parse_class_definition(decorators, start)),
            TokenKind::Async if self.peek() == TokenKind::Def => {
                self.bump(TokenKind::Async);

                // test_ok decorator_async_function
                // @decorator
                // async def foo(): ...
                id::Stmt::from(self.parse_function_definition(decorators, start, true))
            }
            _ => {
                // test_err decorator_unexpected_token
                // @foo
                // async with x: ...
                // @foo
                // x = 1
                self.add_error(
                    ParseErrorType::OtherError(
                        "Expected class, function definition or async function definition \
                            after decorator"
                            .to_string(),
                    ),
                    self.current_token_range(),
                );

                let range = self.node_range(start);

                build::StmtFunctionDef {
                    range,
                    is_async: false,
                    decorator_list: decorators.into_list(&mut self.storage),
                    name: build::Identifier {
                        id: self.storage.intern_name(""),
                        range: self.missing_node_range(),
                    }
                    .parsed(&mut self.storage)
                    .id,
                    type_params: None,
                    parameters: build::Parameters {
                        range: self.missing_node_range(),
                        ..build::Parameters::default()
                    }
                    .parsed(&mut self.storage)
                    .id,
                    returns: None,
                    body: ListId::default(),
                }
                .parsed(&mut self.storage)
                .into()
            }
        }
    }

    /// Parses the body of the given [`Clause`].
    ///
    /// This could either be a single statement that's on the same line as the
    /// clause header or an indented block.
    fn parse_body(&mut self, parent_clause: Clause) -> ListId<AstNodeId<ast::kind::Stmt>> {
        // Note: The test cases in this method chooses a clause at random to test
        // the error logic.

        let newline_range = self.current_token_range();
        if self.eat(TokenKind::Newline) {
            if self.at(TokenKind::Indent) {
                return self.parse_block();
            }
            // test_err clause_expect_indented_block
            // # Here, the error is highlighted at the `pass` token
            // if True:
            // pass
            // # The parser is at the end of the program, so let's highlight
            // # at the newline token after `:`
            // if True:
            self.add_error(
                ParseErrorType::OtherError(format!(
                    "Expected an indented block after {parent_clause}"
                )),
                if self.current_token_range().is_empty() {
                    newline_range
                } else {
                    self.current_token_range()
                },
            );
        } else {
            if self.at_simple_stmt() {
                return self.parse_simple_statements();
            }
            // test_err clause_expect_single_statement
            // if True: if True: pass
            self.add_error(
                ParseErrorType::OtherError("Expected a simple statement".to_string()),
                self.current_token_range(),
            );
        }

        ListId::default()
    }

    /// Parses a block of statements.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `Indent` token.
    fn parse_block(&mut self) -> ListId<AstNodeId<ast::kind::Stmt>> {
        self.bump(TokenKind::Indent);

        let statements = self.with_recursion(|parser| {
            let snapshot = parser.stmt_scratch.snapshot();
            parser.parse_list(RecoveryContextKind::BlockStatements, |parser| {
                let statement = parser.parse_statement();
                parser.stmt_scratch.push(statement.id);
            });

            parser
                .stmt_scratch
                .take_range(snapshot, &mut parser.storage)
        });

        self.expect(TokenKind::Dedent);

        statements
    }

    /// Parses a single parameter for the given function kind.
    ///
    /// Matches either the `param_no_default_star_annotation` or `param_no_default`
    /// rule in the [Python grammar] depending on whether star annotation is allowed
    /// or not.
    ///
    /// Use [`Parser::parse_parameter_with_default`] to allow parameter with default
    /// values.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_parameter(
        &mut self,
        start: TextSize,
        function_kind: FunctionKind,
        allow_star_annotation: AllowStarAnnotation,
    ) -> id::Parameter {
        let name = self.parse_identifier();

        // Annotations are only allowed for function definition. For lambda expression,
        // the `:` token would indicate its body.
        let annotation = match function_kind {
            FunctionKind::FunctionDef if self.eat(TokenKind::Colon) => {
                if self.at_expr() {
                    let parsed_expr = match allow_star_annotation {
                        AllowStarAnnotation::Yes => {
                            // test_ok param_with_star_annotation
                            // def foo(*args: *int | str): ...
                            // def foo(*args: *(int or str)): ...

                            // test_err param_with_invalid_star_annotation
                            // def foo(*args: *): ...
                            // def foo(*args: (*tuple[int])): ...
                            // def foo(*args: *int or str): ...
                            // def foo(*args: *yield x): ...
                            // # def foo(*args: **int): ...
                            let parsed_expr = self.parse_conditional_expression_or_higher_impl(
                                ExpressionContext::starred_bitwise_or(),
                            );

                            // test_ok param_with_star_annotation_py311
                            // # parse_options: {"target-version": "3.11"}
                            // def foo(*args: *Ts): ...

                            // test_ok param_with_star_annotation_py310
                            // # parse_options: {"target-version": "3.10"}
                            // # regression tests for https://github.com/astral-sh/ruff/issues/16874
                            // # starred parameters are fine, just not the annotation
                            // from typing import Annotated, Literal
                            // def foo(*args: Ts): ...
                            // def foo(*x: Literal["this should allow arbitrary strings"]): ...
                            // def foo(*x: Annotated[str, "this should allow arbitrary strings"]): ...
                            // def foo(*args: str, **kwds: int): ...
                            // def union(*x: A | B): ...

                            // test_err param_with_star_annotation_py310
                            // # parse_options: {"target-version": "3.10"}
                            // def foo(*args: *Ts): ...
                            if parsed_expr.is_starred_expr() {
                                self.add_unsupported_syntax_error(
                                    UnsupportedSyntaxErrorKind::StarAnnotation,
                                    parsed_expr.range(),
                                );
                            }

                            parsed_expr
                        }
                        AllowStarAnnotation::No => {
                            // test_ok param_with_annotation
                            // def foo(arg: int): ...
                            // def foo(arg: lambda x: x): ...

                            // test_err param_with_invalid_annotation
                            // def foo(arg: *int): ...
                            // def foo(arg: yield int): ...
                            // def foo(arg: x := int): ...
                            self.parse_conditional_expression_or_higher()
                        }
                    };
                    Some(parsed_expr.expr)
                } else {
                    // test_err param_missing_annotation
                    // def foo(x:): ...
                    // def foo(x:,): ...
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    None
                }
            }
            _ => None,
        };

        build::Parameter {
            range: self.node_range(start),
            name: name.id,
            annotation: annotation.map(|node| node.id),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a parameter with an optional default expression.
    ///
    /// Matches the `param_maybe_default` rule in the [Python grammar].
    ///
    /// This method doesn't allow star annotation. Use [`Parser::parse_parameter`]
    /// instead.
    ///
    /// [Python grammar]: https://docs.python.org/3/reference/grammar.html
    fn parse_parameter_with_default(
        &mut self,
        start: TextSize,
        function_kind: FunctionKind,
    ) -> id::ParameterWithDefault {
        let parameter = self.parse_parameter(start, function_kind, AllowStarAnnotation::No);

        let default = if self.eat(TokenKind::Equal) {
            if self.at_expr() {
                // test_ok param_with_default
                // def foo(x=lambda y: y): ...
                // def foo(x=1 if True else 2): ...
                // def foo(x=await y): ...
                // def foo(x=(yield y)): ...

                // test_err param_with_invalid_default
                // def foo(x=*int): ...
                // def foo(x=(*int)): ...
                // def foo(x=yield y): ...
                Some(self.parse_conditional_expression_or_higher().expr)
            } else {
                // test_err param_missing_default
                // def foo(x=): ...
                // def foo(x: int = ): ...
                self.add_error(
                    ParseErrorType::ExpectedExpression,
                    self.current_token_range(),
                );
                None
            }
        } else {
            None
        };

        build::ParameterWithDefault {
            range: self.node_range(start),
            parameter: parameter.id,
            default: default.map(|node| node.id),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a parameter list for the given function kind.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-parameter_list>
    pub(super) fn parse_parameters(&mut self, function_kind: FunctionKind) -> id::Parameters {
        let start = self.node_start();

        if matches!(function_kind, FunctionKind::FunctionDef) {
            self.expect(TokenKind::Lpar);
        }

        // TODO(dhruvmanila): This has the same problem as `parse_match_pattern_mapping`
        // has where if there are multiple kwarg or vararg, the last one will win and
        // the parser will drop the previous ones. Another thing is the vararg and kwarg
        // uses `Parameter` (not `ParameterWithDefault`) which means that the parser cannot
        // recover well from `*args=(1, 2)`.
        let mut parameters = build::Parameters::default();
        let parameters_snapshot = self.parameter_scratch.snapshot();
        let mut args_snapshot = None;
        let mut kwonlyargs_snapshot = None;

        let mut seen_default_param = false; // `a=10`
        let mut seen_positional_only_separator = false; // `/`
        let mut seen_keyword_only_separator = false; // `*`
        let mut seen_keyword_only_param_after_separator = false;

        // Range of the keyword only separator if it's the last parameter in the list.
        let mut last_keyword_only_separator_range = None;

        self.parse_comma_separated_list(RecoveryContextKind::Parameters(function_kind), |parser| {
            let param_start = parser.node_start();

            if parameters.kwarg.is_some() {
                // test_err params_follows_var_keyword_param
                // def foo(**kwargs, a, /, b=10, *, *args): ...
                parser.add_error(
                    ParseErrorType::ParamAfterVarKeywordParam,
                    parser.current_token_range(),
                );
            }

            match parser.current_token_kind() {
                TokenKind::Star => {
                    let star_range = parser.current_token_range();
                    parser.bump(TokenKind::Star);

                    kwonlyargs_snapshot.get_or_insert_with(|| parser.parameter_scratch.snapshot());

                    if parser.at_identifier_or_soft_keyword() {
                        let param = parser.parse_parameter(
                            param_start,
                            function_kind,
                            AllowStarAnnotation::Yes,
                        );
                        let param_star_range = parser.node_range(star_range.start());

                        if parser.at(TokenKind::Equal) {
                            // test_err params_var_positional_with_default
                            // def foo(a, *args=(1, 2)): ...
                            parser.add_error(
                                ParseErrorType::VarParameterWithDefault,
                                parser.current_token_range(),
                            );
                        }

                        if seen_keyword_only_separator || parameters.vararg.is_some() {
                            // test_err params_multiple_varargs
                            // def foo(a, *, *args, b): ...
                            // # def foo(a, *, b, c, *args): ...
                            // def foo(a, *args1, *args2, b): ...
                            // def foo(a, *args1, b, c, *args2): ...
                            parser.add_error(
                                ParseErrorType::OtherError(
                                    "Only one '*' parameter allowed".to_string(),
                                ),
                                param_star_range,
                            );
                        }

                        // TODO(dhruvmanila): The AST doesn't allow multiple `vararg`, so let's
                        // choose to keep the first one so that the parameters remain in preorder.
                        if parameters.vararg.is_none() {
                            parameters.vararg = Some(param.id);
                        }

                        last_keyword_only_separator_range = None;
                    } else {
                        if seen_keyword_only_separator {
                            // test_err params_multiple_star_separator
                            // def foo(a, *, *, b): ...
                            // def foo(a, *, b, c, *): ...
                            parser.add_error(
                                ParseErrorType::OtherError(
                                    "Only one '*' separator allowed".to_string(),
                                ),
                                star_range,
                            );
                        }

                        if parameters.vararg.is_some() {
                            // test_err params_star_separator_after_star_param
                            // def foo(a, *args, *, b): ...
                            // def foo(a, *args, b, c, *): ...
                            parser.add_error(
                                ParseErrorType::OtherError(
                                    "Keyword-only parameter separator not allowed \
                                        after '*' parameter"
                                        .to_string(),
                                ),
                                star_range,
                            );
                        }

                        seen_keyword_only_separator = true;
                        last_keyword_only_separator_range = Some(star_range);
                    }
                }
                TokenKind::DoubleStar => {
                    let double_star_range = parser.current_token_range();
                    parser.bump(TokenKind::DoubleStar);

                    let param =
                        parser.parse_parameter(param_start, function_kind, AllowStarAnnotation::No);
                    let param_double_star_range = parser.node_range(double_star_range.start());

                    if parameters.kwarg.is_some() {
                        // test_err params_multiple_kwargs
                        // def foo(a, **kwargs1, **kwargs2): ...
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Only one '**' parameter allowed".to_string(),
                            ),
                            param_double_star_range,
                        );
                    }

                    if parser.at(TokenKind::Equal) {
                        // test_err params_var_keyword_with_default
                        // def foo(a, **kwargs={'b': 1, 'c': 2}): ...
                        parser.add_error(
                            ParseErrorType::VarParameterWithDefault,
                            parser.current_token_range(),
                        );
                    }

                    if seen_keyword_only_separator && !seen_keyword_only_param_after_separator {
                        // test_ok params_seen_keyword_only_param_after_star
                        // def foo(*, a, **kwargs): ...
                        // def foo(*, a=10, **kwargs): ...

                        // test_err params_kwarg_after_star_separator
                        // def foo(*, **kwargs): ...
                        parser.add_error(
                            ParseErrorType::ExpectedKeywordParam,
                            param_double_star_range,
                        );
                    }

                    parameters.kwarg = Some(param.id);
                    last_keyword_only_separator_range = None;
                }
                TokenKind::Slash => {
                    let slash_range = parser.current_token_range();
                    parser.bump(TokenKind::Slash);

                    if parser.parameter_scratch.is_empty(&parameters_snapshot)
                        && parameters.vararg.is_none()
                        && parameters.kwarg.is_none()
                    {
                        // test_err params_no_arg_before_slash
                        // def foo(/): ...
                        // def foo(/, a): ...
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Position-only parameter separator not allowed as first parameter"
                                    .to_string(),
                            ),
                            slash_range,
                        );
                    }

                    if seen_positional_only_separator {
                        // test_err params_multiple_slash_separator
                        // def foo(a, /, /, b): ...
                        // def foo(a, /, b, c, /): ...
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Only one '/' separator allowed".to_string(),
                            ),
                            slash_range,
                        );
                    }

                    if seen_keyword_only_separator || parameters.vararg.is_some() {
                        // test_err params_star_after_slash
                        // def foo(*a, /): ...
                        // def foo(a, *args, b, /): ...
                        // def foo(a, *, /, b): ...
                        // def foo(a, *, b, c, /, d): ...
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "'/' parameter must appear before '*' parameter".to_string(),
                            ),
                            slash_range,
                        );
                    }

                    if !seen_positional_only_separator {
                        // We should only split if we're seeing the separator for the
                        // first time, otherwise it's a user error.
                        if kwonlyargs_snapshot.is_none() {
                            args_snapshot = Some(parser.parameter_scratch.snapshot());
                        }
                        seen_positional_only_separator = true;

                        // test_ok pos_only_py38
                        // # parse_options: {"target-version": "3.8"}
                        // def foo(a, /): ...

                        // test_err pos_only_py37
                        // # parse_options: {"target-version": "3.7"}
                        // def foo(a, /): ...
                        // def foo(a, /, b, /): ...
                        // def foo(a, *args, /, b): ...
                        // def foo(a, //): ...
                        parser.add_unsupported_syntax_error(
                            UnsupportedSyntaxErrorKind::PositionalOnlyParameter,
                            slash_range,
                        );
                    }

                    last_keyword_only_separator_range = None;
                }
                _ if parser.at_identifier_or_soft_keyword() => {
                    let param = parser.parse_parameter_with_default(param_start, function_kind);

                    // TODO(dhruvmanila): Pyright seems to only highlight the first non-default argument
                    // https://github.com/microsoft/pyright/blob/3b70417dd549f6663b8f86a76f75d8dfd450f4a8/packages/pyright-internal/src/parser/parser.ts#L2038-L2042
                    if param.view(&parser.storage).default().is_none()
                        && seen_default_param
                        && !seen_keyword_only_separator
                        && parameters.vararg.is_none()
                    {
                        // test_ok params_non_default_after_star
                        // def foo(a=10, *, b, c=11, d): ...
                        // def foo(a=10, *args, b, c=11, d): ...

                        // test_err params_non_default_after_default
                        // def foo(a=10, b, c: int): ...
                        parser.add_error(ParseErrorType::NonDefaultParamAfterDefaultParam, param);
                    }

                    seen_default_param |= param.view(&parser.storage).default().is_some();

                    if seen_keyword_only_separator {
                        seen_keyword_only_param_after_separator = true;
                    }

                    parser.parameter_scratch.push(param.id);
                    last_keyword_only_separator_range = None;
                }
                _ => {
                    // This corresponds to the expected token kinds for `is_list_element`.
                    unreachable!("Expected identifier, '*', '**', or '/'");
                }
            }
        });

        if let Some(star_range) = last_keyword_only_separator_range {
            // test_err params_expected_after_star_separator
            // def foo(*): ...
            // def foo(*,): ...
            // def foo(a, *): ...
            // def foo(a, *,): ...
            // def foo(*, **kwargs): ...
            self.add_error(ParseErrorType::ExpectedKeywordParam, star_range);
        }

        if matches!(function_kind, FunctionKind::FunctionDef) {
            self.expect(TokenKind::Rpar);
        }

        if let Some(kwonlyargs_snapshot) = kwonlyargs_snapshot {
            parameters.kwonlyargs = self
                .parameter_scratch
                .take_range(kwonlyargs_snapshot, &mut self.storage);
        }
        if let Some(args_snapshot) = args_snapshot {
            parameters.args = self
                .parameter_scratch
                .take_range(args_snapshot, &mut self.storage);
            parameters.posonlyargs = self
                .parameter_scratch
                .take_range(parameters_snapshot, &mut self.storage);
        } else if seen_positional_only_separator {
            parameters.posonlyargs = self
                .parameter_scratch
                .take_range(parameters_snapshot, &mut self.storage);
        } else {
            parameters.args = self
                .parameter_scratch
                .take_range(parameters_snapshot, &mut self.storage);
        }

        parameters.range = self.node_range(start);

        parameters.parsed(&mut self.storage)
    }

    /// Try to parse a type parameter list. If the parser is not at the start of a
    /// type parameter list, return `None`.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#type-parameter-lists>
    fn try_parse_type_params(&mut self) -> Option<id::TypeParams> {
        self.at(TokenKind::Lsqb).then(|| self.parse_type_params())
    }

    /// Parses a type parameter list.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#type-parameter-lists>
    fn parse_type_params(&mut self) -> id::TypeParams {
        let start = self.node_start();
        self.bump(TokenKind::Lsqb);

        let type_params = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::TypeParams,
            Parser::parse_type_param,
        );

        if type_params.is_empty() {
            // test_err type_params_empty
            // def foo[]():
            //     pass
            // type ListOrSet[] = list | set
            self.add_error(ParseErrorType::EmptyTypeParams, self.current_token_range());
        }

        self.expect(TokenKind::Rsqb);

        build::TypeParams {
            range: self.node_range(start),
            type_params: type_params.into_list(&mut self.storage),
        }
        .parsed(&mut self.storage)
    }

    /// Parses a type parameter.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-type_param>
    fn parse_type_param(&mut self) -> id::TypeParam {
        let start = self.node_start();

        // TODO(dhruvmanila): CPython throws an error if `TypeVarTuple` or `ParamSpec`
        // has bounds:
        //
        //    type X[*T: int] = int
        //             ^^^^^
        // SyntaxError: cannot use bound with TypeVarTuple
        //
        // We should do the same but currently we can't without throwing away the parsed
        // expression because the AST can't contain it.

        // test_ok type_param_type_var_tuple
        // type X[*Ts] = int
        // type X[*Ts = int] = int
        // type X[*Ts = *int] = int
        // type X[T, *Ts] = int
        // type X[T, *Ts = int] = int
        if self.eat(TokenKind::Star) {
            let name = self.parse_identifier();

            let default = if self.eat(TokenKind::Equal) {
                if self.at_expr() {
                    // test_err type_param_type_var_tuple_invalid_default_expr
                    // type X[*Ts = *int] = int
                    // type X[*Ts = *int or str] = int
                    // type X[*Ts = yield x] = int
                    // type X[*Ts = yield from x] = int
                    // type X[*Ts = x := int] = int
                    Some(
                        self.parse_conditional_expression_or_higher_impl(
                            ExpressionContext::starred_bitwise_or(),
                        )
                        .expr,
                    )
                } else {
                    // test_err type_param_type_var_tuple_missing_default
                    // type X[*Ts =] = int
                    // type X[*Ts =, T2] = int
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    None
                }
            } else {
                None
            };

            // test_err type_param_type_var_tuple_bound
            // type X[*T: int] = int
            id::TypeParam::from(
                build::TypeParamTypeVarTuple {
                    range: self.node_range(start),
                    name: name.id,
                    default: default.map(|node| node.id),
                }
                .parsed(&mut self.storage),
            )

        // test_ok type_param_param_spec
        // type X[**P] = int
        // type X[**P = int] = int
        // type X[T, **P] = int
        // type X[T, **P = int] = int
        } else if self.eat(TokenKind::DoubleStar) {
            let name = self.parse_identifier();

            let default = if self.eat(TokenKind::Equal) {
                if self.at_expr() {
                    // test_err type_param_param_spec_invalid_default_expr
                    // type X[**P = *int] = int
                    // type X[**P = yield x] = int
                    // type X[**P = yield from x] = int
                    // type X[**P = x := int] = int
                    // type X[**P = *int] = int
                    Some(self.parse_conditional_expression_or_higher().expr)
                } else {
                    // test_err type_param_param_spec_missing_default
                    // type X[**P =] = int
                    // type X[**P =, T2] = int
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    None
                }
            } else {
                None
            };

            // test_err type_param_param_spec_bound
            // type X[**T: int] = int
            id::TypeParam::from(
                build::TypeParamParamSpec {
                    range: self.node_range(start),
                    name: name.id,
                    default: default.map(|node| node.id),
                }
                .parsed(&mut self.storage),
            )
            // test_ok type_param_type_var
            // type X[T] = int
            // type X[T = int] = int
            // type X[T: int = int] = int
            // type X[T: (int, int) = int] = int
            // type X[T: int = int, U: (int, int) = int] = int
        } else {
            let name = self.parse_identifier();

            let bound = if self.eat(TokenKind::Colon) {
                if self.at_expr() {
                    // test_err type_param_invalid_bound_expr
                    // type X[T: *int] = int
                    // type X[T: yield x] = int
                    // type X[T: yield from x] = int
                    // type X[T: x := int] = int
                    Some(self.parse_conditional_expression_or_higher().expr)
                } else {
                    // test_err type_param_missing_bound
                    // type X[T: ] = int
                    // type X[T1: , T2] = int
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    None
                }
            } else {
                None
            };

            let equal_token_start = self.node_start();
            let default = if self.eat(TokenKind::Equal) {
                if self.at_expr() {
                    // test_err type_param_type_var_invalid_default_expr
                    // type X[T = *int] = int
                    // type X[T = yield x] = int
                    // type X[T = (yield x)] = int
                    // type X[T = yield from x] = int
                    // type X[T = x := int] = int
                    // type X[T: int = *int] = int
                    Some(self.parse_conditional_expression_or_higher().expr)
                } else {
                    // test_err type_param_type_var_missing_default
                    // type X[T =] = int
                    // type X[T: int =] = int
                    // type X[T1 =, T2] = int
                    self.add_error(
                        ParseErrorType::ExpectedExpression,
                        self.current_token_range(),
                    );
                    None
                }
            } else {
                None
            };

            // test_ok type_param_default_py313
            // # parse_options: {"target-version": "3.13"}
            // type X[T = int] = int
            // def f[T = int](): ...
            // class C[T = int](): ...

            // test_err type_param_default_py312
            // # parse_options: {"target-version": "3.12"}
            // type X[T = int] = int
            // def f[T = int](): ...
            // class C[T = int](): ...
            // class D[S, T = int, U = uint](): ...

            if default.is_some() {
                self.add_unsupported_syntax_error(
                    UnsupportedSyntaxErrorKind::TypeParamDefault,
                    self.node_range(equal_token_start),
                );
            }

            id::TypeParam::from(
                build::TypeParamTypeVar {
                    range: self.node_range(start),
                    name: name.id,
                    bound: bound.map(|node| node.id),
                    default: default.map(|node| node.id),
                }
                .parsed(&mut self.storage),
            )
        }
    }

    /// Validate that the given expression is a valid assignment target.
    ///
    /// If the expression is a list or tuple, then validate each element in the list.
    /// If it's a starred expression, then validate the value of the starred expression.
    ///
    /// Report an error for each invalid assignment expression found.
    pub(super) fn validate_assignment_target(&mut self, expr: &id::Expr) {
        let elements = match expr.view(&self.storage).classify() {
            ExprKind::Starred(starred) => {
                let child = id::Node::from_view(starred.value());
                return self.validate_assignment_target(&child);
            }
            ExprKind::List(list) => list.elts().id(),
            ExprKind::Tuple(tuple) => tuple.elts().id(),
            ExprKind::Name(_) | ExprKind::Attribute(_) | ExprKind::Subscript(_) => return,
            _ => return self.add_error(ParseErrorType::InvalidAssignmentTarget, expr.range()),
        };
        for i in 0..elements.len() {
            if let Some(child) = self
                .storage
                .get_list(elements)
                .get(i)
                .map(id::Node::from_view)
            {
                self.validate_assignment_target(&child);
            }
        }
    }

    /// Validate that the given expression is a valid annotated assignment target.
    ///
    /// Unlike [`Parser::validate_assignment_target`], starred, list and tuple
    /// expressions aren't allowed here.
    fn validate_annotated_assignment_target(&mut self, expr: &id::Expr) {
        match expr.view(&self.storage).classify() {
            ExprKind::List(_) => self.add_error(
                ParseErrorType::OtherError(
                    "Only single target (not list) can be annotated".to_string(),
                ),
                expr,
            ),
            ExprKind::Tuple(_) => self.add_error(
                ParseErrorType::OtherError(
                    "Only single target (not tuple) can be annotated".to_string(),
                ),
                expr,
            ),
            ExprKind::Name(_) | ExprKind::Attribute(_) | ExprKind::Subscript(_) => {}
            _ => self.add_error(ParseErrorType::InvalidAnnotatedAssignmentTarget, expr),
        }
    }

    /// Validate that the given expression is a valid delete target.
    ///
    /// If the expression is a list or tuple, then validate each element in the list.
    ///
    /// See: <https://github.com/python/cpython/blob/d864b0094f9875c5613cbb0b7f7f3ca8f1c6b606/Parser/action_helpers.c#L1150-L1180>
    fn validate_delete_target(&mut self, expr: &id::Expr) {
        let elements = match expr.view(&self.storage).classify() {
            ExprKind::List(list) => list.elts().id(),
            ExprKind::Tuple(tuple) => tuple.elts().id(),
            ExprKind::Name(_) | ExprKind::Attribute(_) | ExprKind::Subscript(_) => return,
            _ => return self.add_error(ParseErrorType::InvalidDeleteTarget, expr.range()),
        };
        for i in 0..elements.len() {
            if let Some(child) = self
                .storage
                .get_list(elements)
                .get(i)
                .map(id::Node::from_view)
            {
                self.validate_delete_target(&child);
            }
        }
    }

    /// Classify the `match` soft keyword token.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `match` token.
    fn classify_match_token(&mut self) -> MatchTokenKind {
        assert_eq!(self.current_token_kind(), TokenKind::Match);

        let (first, second) = self.peek2();

        match first {
            // test_ok match_classify_as_identifier_1
            // match not in case
            TokenKind::Not if second == TokenKind::In => MatchTokenKind::Identifier,

            // test_ok match_classify_as_keyword_1
            // match foo:
            //     case _: ...
            // match 1:
            //     case _: ...
            // match 1.0:
            //     case _: ...
            // match 1j:
            //     case _: ...
            // match "foo":
            //     case _: ...
            // match f"foo {x}":
            //     case _: ...
            // match {1, 2}:
            //     case _: ...
            // match ~foo:
            //     case _: ...
            // match ...:
            //     case _: ...
            // match not foo:
            //     case _: ...
            // match await foo():
            //     case _: ...
            // match lambda foo: foo:
            //     case _: ...

            // test_err match_classify_as_keyword
            // match yield foo:
            //     case _: ...
            TokenKind::Identifier
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Complex
            | TokenKind::String
            | TokenKind::FStringStart
            | TokenKind::TStringStart
            | TokenKind::Lbrace
            | TokenKind::Tilde
            | TokenKind::Ellipsis
            | TokenKind::Not
            | TokenKind::Await
            | TokenKind::Yield
            | TokenKind::Lambda => MatchTokenKind::Keyword,

            // test_ok match_classify_as_keyword_or_identifier
            // match (1, 2)  # Identifier
            // match (1, 2):  # Keyword
            //     case _: ...
            // match [1:]  # Identifier
            // match [1, 2]:  # Keyword
            //     case _: ...
            // match * foo  # Identifier
            // match - foo  # Identifier
            // match -foo:  # Keyword
            //     case _: ...

            // test_err match_classify_as_keyword_or_identifier
            // match *foo:  # Keyword
            //     case _: ...
            TokenKind::Lpar
            | TokenKind::Lsqb
            | TokenKind::Star
            | TokenKind::Plus
            | TokenKind::Minus => MatchTokenKind::KeywordOrIdentifier,

            _ => {
                if first.is_soft_keyword() || first.is_singleton() {
                    // test_ok match_classify_as_keyword_2
                    // match match:
                    //     case _: ...
                    // match case:
                    //     case _: ...
                    // match type:
                    //     case _: ...
                    // match None:
                    //     case _: ...
                    // match True:
                    //     case _: ...
                    // match False:
                    //     case _: ...
                    MatchTokenKind::Keyword
                } else {
                    // test_ok match_classify_as_identifier_2
                    // match
                    // match != foo
                    // (foo, match)
                    // [foo, match]
                    // {foo, match}
                    // match;
                    // match: int
                    // match,
                    // match.foo
                    // match / foo
                    // match << foo
                    // match and foo
                    // match is not foo
                    MatchTokenKind::Identifier
                }
            }
        }
    }

    /// Parses a sequence of clauses.
    ///
    /// The parser only continues for as long as it sees the token indicating the start of the
    /// specific clause. Unlike [`Parser::parse_list`], this method does not perform error recovery
    /// when the next token is not a list terminator or the start of a list element.
    ///
    /// The special method is necessary because Python uses indentation over explicit delimiters to
    /// indicate the end of a clause.
    ///
    /// ```python
    /// if True: ...
    /// elif False: ...
    /// elf x: ....
    /// else: ...
    /// ```
    ///
    /// It would be nice if the above example would recover and either skip over the `elf x: ...`
    /// or parse it as a nested statement so that the parser recognises the `else` clause. But
    /// Python makes this hard (without writing custom error recovery logic) because `elf x: `
    /// could also be an annotated assignment that went wrong ;)
    ///
    /// For now, don't recover when parsing clause headers, but add the terminator tokens (e.g.
    /// `Else`) to the recovery context so that expression recovery stops when it encounters an
    /// `else` token.
    fn parse_clauses(&mut self, clause: Clause, mut parse_clause: impl FnMut(&mut Parser<'src>)) {
        let mut progress = ParserProgress::default();

        let recovery_kind = match clause {
            Clause::ElIf => RecoveryContextKind::Elif,
            Clause::Except => RecoveryContextKind::Except,
            _ => unreachable!("Clause is not supported"),
        };

        let saved_context = self.recovery_context;
        self.recovery_context = self
            .recovery_context
            .union(RecoveryContext::from_kind(recovery_kind));

        while recovery_kind.is_list_element(self) {
            progress.assert_progressing(self);

            parse_clause(self);
        }

        self.recovery_context = saved_context;
    }
}

#[derive(Copy, Clone)]
enum Clause {
    If,
    Else,
    ElIf,
    For,
    With,
    Class,
    While,
    FunctionDef,
    Case,
    Try,
    Except,
    Finally,
}

impl Display for Clause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Clause::If => write!(f, "`if` statement"),
            Clause::Else => write!(f, "`else` clause"),
            Clause::ElIf => write!(f, "`elif` clause"),
            Clause::For => write!(f, "`for` statement"),
            Clause::With => write!(f, "`with` statement"),
            Clause::Class => write!(f, "`class` definition"),
            Clause::While => write!(f, "`while` statement"),
            Clause::FunctionDef => write!(f, "function definition"),
            Clause::Case => write!(f, "`case` block"),
            Clause::Try => write!(f, "`try` statement"),
            Clause::Except => write!(f, "`except` clause"),
            Clause::Finally => write!(f, "`finally` clause"),
        }
    }
}

/// The classification of the `match` token.
///
/// The `match` token is a soft keyword which means, depending on the context, it can be used as a
/// keyword or an identifier.
#[derive(Debug, Clone, Copy)]
enum MatchTokenKind {
    /// The `match` token is used as a keyword.
    ///
    /// For example:
    /// ```python
    /// match foo:
    ///     case _:
    ///         pass
    /// ```
    Keyword,

    /// The `match` token is used as an identifier.
    ///
    /// For example:
    /// ```python
    /// match.values()
    /// match is None
    /// ````
    Identifier,

    /// The `match` token is used as either a keyword or an identifier.
    ///
    /// For example:
    /// ```python
    /// # Used as a keyword
    /// match [x, y]:
    ///     case [1, 2]:
    ///         pass
    ///
    /// # Used as an identifier
    /// match[x]
    /// ```
    KeywordOrIdentifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WithItemParsingState {
    /// Parsing the with items without any ambiguity.
    Regular,

    /// Parsing the with items in a speculative mode.
    Speculative,
}

#[derive(Debug)]
struct ParsedWithItem {
    /// The contained with item.
    item: id::WithItem,
    /// If the context expression of the item is parenthesized.
    is_parenthesized: bool,
}

#[derive(Debug, Copy, Clone)]
enum ElifOrElse {
    Elif,
    Else,
}

impl ElifOrElse {
    const fn is_elif(self) -> bool {
        matches!(self, ElifOrElse::Elif)
    }

    const fn as_token_kind(self) -> TokenKind {
        match self {
            ElifOrElse::Elif => TokenKind::Elif,
            ElifOrElse::Else => TokenKind::Else,
        }
    }

    const fn as_clause(self) -> Clause {
        match self {
            ElifOrElse::Elif => Clause::ElIf,
            ElifOrElse::Else => Clause::Else,
        }
    }
}

/// The kind of the except clause.
#[derive(Debug, Copy, Clone)]
enum ExceptClauseKind {
    /// A normal except clause e.g., `except Exception as e: ...`.
    Normal,
    /// An except clause with a star e.g., `except *: ...`.
    ///
    /// Contains the star's [`TextRange`] for error reporting.
    Star(TextRange),
}

impl ExceptClauseKind {
    const fn is_star(self) -> bool {
        matches!(self, ExceptClauseKind::Star(..))
    }
}

#[derive(Debug, Copy, Clone)]
enum AllowStarAnnotation {
    Yes,
    No,
}

#[derive(Debug, Copy, Clone)]
enum ImportStyle {
    /// E.g., `import foo, bar`
    Import,
    /// E.g., `from foo import bar, baz`
    ImportFrom,
}
