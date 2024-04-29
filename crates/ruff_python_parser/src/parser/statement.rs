use std::fmt::Display;
use std::hash::BuildHasherDefault;

use rustc_hash::FxHashSet;

use ruff_python_ast::{
    self as ast, ExceptHandler, Expr, ExprContext, IpyEscapeKind, Operator, Stmt, WithItem,
};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::expression::{GeneratorExpressionInParentheses, ParsedExpr, EXPR_SET};
use crate::parser::progress::ParserProgress;
use crate::parser::{
    helpers, FunctionKind, Parser, RecoveryContext, RecoveryContextKind, WithItemKind,
};
use crate::token_set::TokenSet;
use crate::{Mode, ParseErrorType, Tok, TokenKind};

use super::expression::{ExpressionContext, OperatorPrecedence};
use super::Parenthesized;

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
        self.at_ts(SIMPLE_STMT_WITH_EXPR_SET)
    }

    /// Returns `true` if the current token is the start of a simple, compound or expression
    /// statement.
    pub(super) fn at_stmt(&self) -> bool {
        self.at_ts(STMTS_SET)
    }

    /// Checks if the parser is currently positioned at the start of a type parameter.
    pub(super) fn at_type_param(&self) -> bool {
        let token = self.current_token_kind();
        matches!(
            token,
            TokenKind::Star | TokenKind::DoubleStar | TokenKind::Name
        ) || token.is_keyword()
    }

    /// Parses a compound or a single simple statement.
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/compound_stmts.html>
    /// - <https://docs.python.org/3/reference/simple_stmts.html>
    pub(super) fn parse_statement(&mut self) -> Stmt {
        let start = self.node_start();

        match self.current_token_kind() {
            TokenKind::If => Stmt::If(self.parse_if_statement()),
            TokenKind::For => Stmt::For(self.parse_for_statement(start)),
            TokenKind::While => Stmt::While(self.parse_while_statement()),
            TokenKind::Def => Stmt::FunctionDef(self.parse_function_definition(vec![], start)),
            TokenKind::Class => Stmt::ClassDef(self.parse_class_definition(vec![], start)),
            TokenKind::Try => Stmt::Try(self.parse_try_statement()),
            TokenKind::With => Stmt::With(self.parse_with_statement(start)),
            TokenKind::At => self.parse_decorators(),
            TokenKind::Async => self.parse_async_statement(),
            TokenKind::Match => Stmt::Match(self.parse_match_statement()),
            _ => self.parse_single_simple_statement(),
        }
    }

    /// Parses a single simple statement.
    ///
    /// This statement must be terminated by a newline or semicolon.
    ///
    /// Use [`Parser::parse_simple_statements`] to parse a sequence of simple statements.
    fn parse_single_simple_statement(&mut self) -> Stmt {
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
            } else if self.at_compound_stmt() {
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
    fn parse_simple_statements(&mut self) -> Vec<Stmt> {
        let mut stmts = vec![];
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            stmts.push(self.parse_simple_statement());

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
        stmts
    }

    /// Parses a simple statement.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html>
    fn parse_simple_statement(&mut self) -> Stmt {
        match self.current_token_kind() {
            TokenKind::Return => Stmt::Return(self.parse_return_statement()),
            TokenKind::Import => Stmt::Import(self.parse_import_statement()),
            TokenKind::From => Stmt::ImportFrom(self.parse_from_import_statement()),
            TokenKind::Pass => Stmt::Pass(self.parse_pass_statement()),
            TokenKind::Continue => Stmt::Continue(self.parse_continue_statement()),
            TokenKind::Break => Stmt::Break(self.parse_break_statement()),
            TokenKind::Raise => Stmt::Raise(self.parse_raise_statement()),
            TokenKind::Del => Stmt::Delete(self.parse_delete_statement()),
            TokenKind::Assert => Stmt::Assert(self.parse_assert_statement()),
            TokenKind::Global => Stmt::Global(self.parse_global_statement()),
            TokenKind::Nonlocal => Stmt::Nonlocal(self.parse_nonlocal_statement()),
            TokenKind::Type => Stmt::TypeAlias(self.parse_type_alias_statement()),
            TokenKind::IpyEscapeCommand => {
                Stmt::IpyEscapeCommand(self.parse_ipython_escape_command_statement())
            }
            _ => {
                let start = self.node_start();

                // simple_stmt: `... | yield_stmt | star_expressions | ...`
                let parsed_expr =
                    self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or());

                if self.at(TokenKind::Equal) {
                    Stmt::Assign(self.parse_assign_statement(parsed_expr, start))
                } else if self.at(TokenKind::Colon) {
                    Stmt::AnnAssign(self.parse_annotated_assignment_statement(parsed_expr, start))
                } else if let Some(op) = self.current_token_kind().as_augmented_assign_operator() {
                    Stmt::AugAssign(self.parse_augmented_assignment_statement(
                        parsed_expr,
                        op,
                        start,
                    ))
                } else if self.mode == Mode::Ipython && self.at(TokenKind::Question) {
                    Stmt::IpyEscapeCommand(
                        self.parse_ipython_help_end_escape_command_statement(&parsed_expr),
                    )
                } else {
                    Stmt::Expr(ast::StmtExpr {
                        range: self.node_range(start),
                        value: Box::new(parsed_expr.expr),
                    })
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
    fn parse_delete_statement(&mut self) -> ast::StmtDelete {
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
                let mut target = parser.parse_conditional_expression_or_higher_impl(
                    ExpressionContext::starred_conditional(),
                );
                helpers::set_expr_ctx(&mut target.expr, ExprContext::Del);

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

        ast::StmtDelete {
            targets,
            range: self.node_range(start),
        }
    }

    /// Parses a `return` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `return` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-return_stmt>
    fn parse_return_statement(&mut self) -> ast::StmtReturn {
        let start = self.node_start();
        self.bump(TokenKind::Return);

        // test_err return_stmt_invalid_expr
        // return *
        // return yield x
        // return yield from x
        // return x := 1
        // return *x and y
        let value = self.at_expr().then(|| {
            Box::new(
                self.parse_expression_list(ExpressionContext::starred_bitwise_or())
                    .expr,
            )
        });

        ast::StmtReturn {
            range: self.node_range(start),
            value,
        }
    }

    /// Parses a `raise` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `raise` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-raise_stmt>
    fn parse_raise_statement(&mut self) -> ast::StmtRaise {
        let start = self.node_start();
        self.bump(TokenKind::Raise);

        let exc = if self.at(TokenKind::Newline) {
            None
        } else {
            // test_err raise_stmt_invalid_exc
            // raise *x
            // raise yield x
            // raise x := 1
            let exc = self.parse_expression_list(ExpressionContext::default());

            if let Some(ast::ExprTuple {
                parenthesized: false,
                ..
            }) = exc.as_tuple_expr()
            {
                // test_err raise_stmt_unparenthesized_tuple_exc
                // raise x,
                // raise x, y
                // raise x, y from z
                self.add_error(ParseErrorType::UnparenthesizedTupleExpression, &exc);
            }

            Some(Box::new(exc.expr))
        };

        let cause = (exc.is_some() && self.eat(TokenKind::From)).then(|| {
            // test_err raise_stmt_invalid_cause
            // raise x from *y
            // raise x from yield y
            // raise x from y := 1
            let cause = self.parse_expression_list(ExpressionContext::default());

            if let Some(ast::ExprTuple {
                parenthesized: false,
                ..
            }) = cause.as_tuple_expr()
            {
                // test_err raise_stmt_unparenthesized_tuple_cause
                // raise x from y,
                // raise x from y, z
                self.add_error(ParseErrorType::UnparenthesizedTupleExpression, &cause);
            }

            Box::new(cause.expr)
        });

        ast::StmtRaise {
            range: self.node_range(start),
            exc,
            cause,
        }
    }

    /// Parses an import statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `import` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#the-import-statement>
    fn parse_import_statement(&mut self) -> ast::StmtImport {
        let start = self.node_start();
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

        let names = self
            .parse_comma_separated_list_into_vec(RecoveryContextKind::ImportNames, |p| {
                p.parse_alias(ImportStyle::Import)
            });

        if names.is_empty() {
            // test_err import_stmt_empty
            // import
            self.add_error(ParseErrorType::EmptyImportNames, self.current_token_range());
        }

        ast::StmtImport {
            range: self.node_range(start),
            names,
        }
    }

    /// Parses a `from` import statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `from` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-import_stmt>
    fn parse_from_import_statement(&mut self) -> ast::StmtImportFrom {
        let start = self.node_start();
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

        let module = if self.at(TokenKind::Name) {
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
        let mut names = vec![];
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
                seen_star_import |= alias.name.id == "*";
                names.push(alias);
            },
        );

        if names.is_empty() {
            // test_err from_import_empty_names
            // from x import
            // from x import ()
            // from x import ,,
            self.add_error(ParseErrorType::EmptyImportNames, self.current_token_range());
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

        ast::StmtImportFrom {
            module,
            names,
            level: leading_dots,
            range: self.node_range(start),
        }
    }

    /// Parses an `import` or `from` import name.
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/simple_stmts.html#the-import-statement>
    /// - <https://docs.python.org/3/library/ast.html#ast.alias>
    fn parse_alias(&mut self, style: ImportStyle) -> ast::Alias {
        let start = self.node_start();
        if self.eat(TokenKind::Star) {
            let range = self.node_range(start);
            return ast::Alias {
                name: ast::Identifier {
                    id: "*".into(),
                    range,
                },
                asname: None,
                range,
            };
        }

        let name = match style {
            ImportStyle::Import => self.parse_dotted_name(),
            ImportStyle::ImportFrom => self.parse_identifier(),
        };

        let asname = if self.eat(TokenKind::As) {
            if self.at(TokenKind::Name) {
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

        ast::Alias {
            range: self.node_range(start),
            name,
            asname,
        }
    }

    /// Parses a dotted name.
    ///
    /// A dotted name is a sequence of identifiers separated by a single dot.
    fn parse_dotted_name(&mut self) -> ast::Identifier {
        let start = self.node_start();

        let mut dotted_name = self.parse_identifier().id;
        let mut progress = ParserProgress::default();

        while self.eat(TokenKind::Dot) {
            progress.assert_progressing(self);

            // test_err dotted_name_multiple_dots
            // import a..b
            // import a...b
            dotted_name.push('.');
            dotted_name.push_str(&self.parse_identifier());
        }

        // test_ok dotted_name_normalized_spaces
        // import a.b.c
        // import a .  b  . c
        ast::Identifier {
            id: dotted_name,
            range: self.node_range(start),
        }
    }

    /// Parses a `pass` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `pass` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-pass_stmt>
    fn parse_pass_statement(&mut self) -> ast::StmtPass {
        let start = self.node_start();
        self.bump(TokenKind::Pass);
        ast::StmtPass {
            range: self.node_range(start),
        }
    }

    /// Parses a `continue` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `continue` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-continue_stmt>
    fn parse_continue_statement(&mut self) -> ast::StmtContinue {
        let start = self.node_start();
        self.bump(TokenKind::Continue);
        ast::StmtContinue {
            range: self.node_range(start),
        }
    }

    /// Parses a `break` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `break` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-break_stmt>
    fn parse_break_statement(&mut self) -> ast::StmtBreak {
        let start = self.node_start();
        self.bump(TokenKind::Break);
        ast::StmtBreak {
            range: self.node_range(start),
        }
    }

    /// Parses an `assert` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `assert` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement>
    fn parse_assert_statement(&mut self) -> ast::StmtAssert {
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
                Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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

        ast::StmtAssert {
            test: Box::new(test.expr),
            msg,
            range: self.node_range(start),
        }
    }

    /// Parses a global statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `global` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-global_stmt>
    fn parse_global_statement(&mut self) -> ast::StmtGlobal {
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
        ast::StmtGlobal {
            range: self.node_range(start),
            names,
        }
    }

    /// Parses a nonlocal statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `nonlocal` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-nonlocal_stmt>
    fn parse_nonlocal_statement(&mut self) -> ast::StmtNonlocal {
        let start = self.node_start();
        self.bump(TokenKind::Nonlocal);

        // test_err nonlocal_stmt_trailing_comma
        // nonlocal ,
        // nonlocal x,
        // nonlocal x, y,

        // test_err nonlocal_stmt_expression
        // nonlocal x + 1
        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::Identifiers,
            Parser::parse_identifier,
        );

        if names.is_empty() {
            // test_err nonlocal_stmt_empty
            // nonlocal
            self.add_error(
                ParseErrorType::EmptyNonlocalNames,
                self.current_token_range(),
            );
        }

        // test_ok nonlocal_stmt
        // nonlocal x
        // nonlocal x, y, z
        ast::StmtNonlocal {
            range: self.node_range(start),
            names,
        }
    }

    /// Parses a type alias statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `type` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#the-type-statement>
    fn parse_type_alias_statement(&mut self) -> ast::StmtTypeAlias {
        let start = self.node_start();
        self.bump(TokenKind::Type);

        let mut name = Expr::Name(self.parse_name());
        helpers::set_expr_ctx(&mut name, ExprContext::Store);

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

        ast::StmtTypeAlias {
            name: Box::new(name),
            type_params,
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    /// Parses an IPython escape command at the statement level.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `IpyEscapeCommand` token.
    fn parse_ipython_escape_command_statement(&mut self) -> ast::StmtIpyEscapeCommand {
        let start = self.node_start();

        let (Tok::IpyEscapeCommand { value, kind }, _) = self.bump(TokenKind::IpyEscapeCommand)
        else {
            unreachable!()
        };

        let range = self.node_range(start);
        if self.mode != Mode::Ipython {
            self.add_error(ParseErrorType::UnexpectedIpythonEscapeCommand, range);
        }

        ast::StmtIpyEscapeCommand { range, kind, value }
    }

    /// Parses an IPython help end escape command at the statement level.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `?` token.
    fn parse_ipython_help_end_escape_command_statement(
        &mut self,
        parsed_expr: &ParsedExpr,
    ) -> ast::StmtIpyEscapeCommand {
        // We are permissive than the original implementation because we would allow whitespace
        // between the expression and the suffix while the IPython implementation doesn't allow it.
        // For example, `foo ?` would be valid in our case but invalid for IPython.
        fn unparse_expr(parser: &mut Parser, expr: &Expr, buffer: &mut String) {
            match expr {
                Expr::Name(ast::ExprName { id, .. }) => {
                    buffer.push_str(id.as_str());
                }
                Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                    unparse_expr(parser, value, buffer);
                    buffer.push('[');

                    if let Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: ast::Number::Int(integer),
                        ..
                    }) = &**slice
                    {
                        buffer.push_str(&format!("{integer}"));
                    } else {
                        parser.add_error(
                            ParseErrorType::OtherError(
                                "Only integer literals are allowed in subscript expressions in help end escape command"
                                    .to_string()
                            ),
                            slice.range(),
                        );
                        buffer.push_str(parser.src_text(slice.range()));
                    }

                    buffer.push(']');
                }
                Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                    unparse_expr(parser, value, buffer);
                    buffer.push('.');
                    buffer.push_str(attr.as_str());
                }
                _ => {
                    parser.add_error(
                        ParseErrorType::OtherError(
                            "Expected name, subscript or attribute expression in help end escape command"
                                .to_string()
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
        unparse_expr(self, &parsed_expr.expr, &mut value);

        ast::StmtIpyEscapeCommand {
            value: value.into_boxed_str(),
            kind,
            range: self.node_range(parsed_expr.start()),
        }
    }

    /// Parse an assignment statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `=` token.
    ///
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#assignment-statements>
    fn parse_assign_statement(&mut self, target: ParsedExpr, start: TextSize) -> ast::StmtAssign {
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
        // x = *a and b
        // x = *yield x
        // x = *yield from x
        // x = *lambda x: x
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
            helpers::set_expr_ctx(target, ExprContext::Store);
            // test_err assign_stmt_invalid_target
            // 1 = 1
            // x = 1 = 2
            // x = 1 = y = 2 = z
            // ["a", "b"] = ["a", "b"]
            self.validate_assignment_target(target);
        }

        ast::StmtAssign {
            targets,
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
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
        mut target: ParsedExpr,
        start: TextSize,
    ) -> ast::StmtAnnAssign {
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

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

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
                Some(Box::new(
                    self.parse_expression_list(ExpressionContext::yield_or_starred_bitwise_or())
                        .expr,
                ))
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

        ast::StmtAnnAssign {
            target: Box::new(target.expr),
            annotation: Box::new(annotation.expr),
            value,
            simple,
            range: self.node_range(start),
        }
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
        mut target: ParsedExpr,
        op: Operator,
        start: TextSize,
    ) -> ast::StmtAugAssign {
        // Consume the operator
        self.bump_ts(AUGMENTED_ASSIGN_SET);

        if !matches!(
            &target.expr,
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_)
        ) {
            // test_err aug_assign_stmt_invalid_target
            // 1 += 1
            // "a" += "b"
            // *x += 1
            // pass += 1
            // x += pass
            // (x + y) += 1
            self.add_error(ParseErrorType::InvalidAugmentedAssignmentTarget, &target);
        }

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

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

        ast::StmtAugAssign {
            target: Box::new(target.expr),
            op,
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    /// Parses an `if` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `if` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-if-statement>
    fn parse_if_statement(&mut self) -> ast::StmtIf {
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
        let mut elif_else_clauses = self.parse_clauses(Clause::ElIf, |p| {
            p.parse_elif_or_else_clause(ElifOrElse::Elif)
        });

        if self.at(TokenKind::Else) {
            elif_else_clauses.push(self.parse_elif_or_else_clause(ElifOrElse::Else));
        }

        ast::StmtIf {
            test: Box::new(test.expr),
            body,
            elif_else_clauses,
            range: self.node_range(start),
        }
    }

    /// Parses an `elif` or `else` clause.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `elif` or `else` token.
    fn parse_elif_or_else_clause(&mut self, kind: ElifOrElse) -> ast::ElifElseClause {
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

        ast::ElifElseClause {
            test,
            body,
            range: self.node_range(start),
        }
    }

    /// Parses a `try` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `try` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-try-statement>
    fn parse_try_statement(&mut self) -> ast::StmtTry {
        let try_start = self.node_start();
        self.bump(TokenKind::Try);
        self.expect(TokenKind::Colon);

        let mut is_star = false;

        let try_body = self.parse_body(Clause::Try);

        let has_except = self.at(TokenKind::Except);

        // TODO(dhruvmanila): Raise syntax error if there are both 'except' and 'except*'
        // on the same 'try'
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
        let handlers = self.parse_clauses(Clause::Except, |p| {
            let (handler, kind) = p.parse_except_clause();
            is_star |= kind.is_star();
            handler
        });

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
            vec![]
        };

        let (finalbody, has_finally) = if self.eat(TokenKind::Finally) {
            self.expect(TokenKind::Colon);
            (self.parse_body(Clause::Finally), true)
        } else {
            (vec![], false)
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

        ast::StmtTry {
            body: try_body,
            handlers,
            orelse,
            finalbody,
            is_star,
            range: self.node_range(try_start),
        }
    }

    /// Parses an `except` clause of a `try` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `except` token.
    fn parse_except_clause(&mut self) -> (ExceptHandler, ExceptClauseKind) {
        let start = self.node_start();
        self.bump(TokenKind::Except);

        let block_kind = if self.eat(TokenKind::Star) {
            ExceptClauseKind::Star
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
            if matches!(
                parsed_expr.expr,
                Expr::Tuple(ast::ExprTuple {
                    parenthesized: false,
                    ..
                })
            ) {
                // test_err except_stmt_unparenthesized_tuple
                // try:
                //     pass
                // except x, y:
                //     pass
                // except x, y as exc:
                //     pass
                // try:
                //     pass
                // except* x, y:
                //     pass
                // except* x, y as eg:
                //     pass
                self.add_error(
                    ParseErrorType::OtherError(
                        "Multiple exception types must be parenthesized".to_string(),
                    ),
                    &parsed_expr,
                );
            }
            Some(Box::new(parsed_expr.expr))
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
            if self.at(TokenKind::Name) {
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
            ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                type_,
                name,
                body: except_body,
                range: self.node_range(start),
            }),
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
    fn parse_for_statement(&mut self, start: TextSize) -> ast::StmtFor {
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
        let mut target =
            self.parse_expression_list(ExpressionContext::starred_conditional().with_in_excluded());

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

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

        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::For);

        let orelse = if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon);
            self.parse_body(Clause::Else)
        } else {
            vec![]
        };

        ast::StmtFor {
            target: Box::new(target.expr),
            iter: Box::new(iter.expr),
            is_async: false,
            body,
            orelse,
            range: self.node_range(start),
        }
    }

    /// Parses a `while` statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `while` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-while-statement>
    fn parse_while_statement(&mut self) -> ast::StmtWhile {
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
            vec![]
        };

        ast::StmtWhile {
            test: Box::new(test.expr),
            body,
            orelse,
            range: self.node_range(start),
        }
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
        decorator_list: Vec<ast::Decorator>,
        start: TextSize,
    ) -> ast::StmtFunctionDef {
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
                // def foo() -> (yield x): ...
                // def foo() -> int if True else str: ...

                // test_err function_def_invalid_return_expr
                // def foo() -> *int: ...
                // def foo() -> (*int): ...
                // def foo() -> yield x: ...
                let returns = self.parse_expression_list(ExpressionContext::default());

                if matches!(
                    returns.expr,
                    Expr::Tuple(ast::ExprTuple {
                        parenthesized: false,
                        ..
                    })
                ) {
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

                Some(Box::new(returns.expr))
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

        ast::StmtFunctionDef {
            name,
            type_params: type_params.map(Box::new),
            parameters: Box::new(parameters),
            body,
            decorator_list,
            is_async: false,
            returns,
            range: self.node_range(start),
        }
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
        decorator_list: Vec<ast::Decorator>,
        start: TextSize,
    ) -> ast::StmtClassDef {
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

        // test_ok class_def_arguments
        // class Foo: ...
        // class Foo(): ...
        let arguments = self
            .at(TokenKind::Lpar)
            .then(|| Box::new(self.parse_arguments()));

        self.expect(TokenKind::Colon);

        // test_err class_def_empty_body
        // class Foo:
        // class Foo():
        // x = 42
        let body = self.parse_body(Clause::Class);

        ast::StmtClassDef {
            range: self.node_range(start),
            decorator_list,
            name,
            type_params: type_params.map(Box::new),
            arguments,
            body,
        }
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
    fn parse_with_statement(&mut self, start: TextSize) -> ast::StmtWith {
        self.bump(TokenKind::With);

        let items = self.parse_with_items();
        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::With);

        ast::StmtWith {
            items,
            body,
            is_async: false,
            range: self.node_range(start),
        }
    }

    /// Parses a list of with items.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-with_stmt_contents>
    fn parse_with_items(&mut self) -> Vec<WithItem> {
        let start = self.node_start();
        let mut items = vec![];

        if !self.at_expr() {
            self.add_error(
                ParseErrorType::OtherError(
                    "Expected the start of an expression after `with` keyword".to_string(),
                ),
                self.current_token_range(),
            );
            return items;
        }

        let with_item_kind = if self.eat(TokenKind::Lpar) {
            self.parse_parenthesized_with_items(start, &mut items)
        } else {
            WithItemKind::Unparenthesized
        };

        if with_item_kind.is_parenthesized_expression() {
            // The trailing comma is optional because (1) they aren't allowed in parenthesized
            // expression context and, (2) We need to raise the correct error if they're present.
            //
            // Consider the following three examples:
            //
            // ```python
            // with (item1, item2): ...  # (1)
            // with (item1, item2),: ...  # (2)
            // with (item1, item2), item3,: ...  # (3)
            // ```
            //
            // Here, (1) is valid and represents a parenthesized with items while (2) and (3)
            // are invalid as they are parenthesized expression. Example (3) will raise an error
            // stating that a trailing comma isn't allowed, while (2) will raise an "expected an
            // expression" error.
            //
            // The reason that (2) expects an expression is because if it raised an error
            // similar to (3), we would be suggesting to remove the trailing comma, which would
            // make it a parenthesized with items. This would contradict our original assumption
            // that it's a parenthesized expression.
            //
            // However, for (3), the error is being raised by the list parsing logic and if the
            // trailing comma is removed, it still remains a parenthesized expression, so it's
            // fine to raise the error.
            if self.eat(TokenKind::Comma) && !self.at_expr() {
                self.add_error(
                    ParseErrorType::ExpectedExpression,
                    self.current_token_range(),
                );
            }
        }

        // This call is a no-op if the with items are parenthesized as all of them
        // have already been parsed.
        self.parse_comma_separated_list(RecoveryContextKind::WithItems(with_item_kind), |parser| {
            items.push(parser.parse_with_item(WithItemParsingState::Regular).item);
        });

        if with_item_kind == WithItemKind::Parenthesized {
            self.expect(TokenKind::Rpar);
        }

        items
    }

    /// Parse the with items coming after an ambiguous `(` token.
    ///
    /// To understand the ambiguity, consider the following example:
    ///
    /// ```python
    /// with (item1, item2): ...       # (1)
    /// with (item1, item2) as f: ...  # (2)
    /// ```
    ///
    /// When the parser is at the `(` token after the `with` keyword, it doesn't
    /// know if it's used to parenthesize the with items or if it's part of a
    /// parenthesized expression of the first with item. The challenge here is
    /// that until the parser sees the matching `)` token, it can't resolve the
    /// ambiguity. This requires infinite lookahead.
    ///
    /// This method resolves the ambiguity by parsing the with items assuming that
    /// it's a parenthesized with items. Then, once it finds the matching `)`, it
    /// checks if the assumption still holds true. If it doesn't, then it combines
    /// the parsed with items into a single with item with an appropriate expression.
    ///
    /// The return value is the kind of with items parsed. Note that there could
    /// still be other with items which needs to be parsed as this method stops
    /// when the matching `)` is found.
    fn parse_parenthesized_with_items(
        &mut self,
        start: TextSize,
        items: &mut Vec<WithItem>,
    ) -> WithItemKind {
        // We'll start with the assumption that the with items are parenthesized.
        let mut with_item_kind = WithItemKind::Parenthesized;

        // Keep track of certain properties to determine if the with items are
        // parenthesized or if it's a parenthesized expression. Refer to their
        // usage for examples and explanation.
        let mut has_trailing_comma = false;
        let mut has_optional_vars = false;

        // Start with parsing the first with item after an ambiguous `(` token
        // with the start offset.
        let mut state = WithItemParsingState::AmbiguousLparFirstItem(start);

        let mut parsed_with_items = vec![];
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);

            // We stop at the first `)` found. Any nested parentheses will be
            // consumed by the with item parsing. This check needs to be done
            // first in case there are no with items. For example,
            //
            // ```python
            // with (): ...
            // with () as x: ...
            // ```
            if self.at(TokenKind::Rpar) {
                break;
            }

            let parsed_with_item = self.parse_with_item(state);

            if parsed_with_item.item.context_expr.is_generator_expr()
                && parsed_with_item.used_ambiguous_lpar
            {
                // For generator expressions, it's a bit tricky. We need to check if parsing
                // a generator expression has used the ambiguous `(` token. This is the case
                // for a parenthesized generator expression which is using the ambiguous `(`
                // as the start of the generator expression. For example:
                //
                // ```python
                // with (x for x in range(10)): ...
                // #                         ^
                // #                         Consumed by `parse_with_item`
                // ```
                //
                // This is only allowed if it's the first with item which is made sure by the
                // `with_item_parsing` state.
                with_item_kind = WithItemKind::SingleParenthesizedGeneratorExpression;
                parsed_with_items.push(parsed_with_item);
                break;
            }

            has_optional_vars |= parsed_with_item.item.optional_vars.is_some();

            parsed_with_items.push(parsed_with_item);

            has_trailing_comma = self.eat(TokenKind::Comma);
            if !has_trailing_comma {
                break;
            }

            // Update the with item parsing to indicate that we're no longer
            // parsing the first with item, but we haven't yet found the `)` to
            // the corresponding ambiguous `(`.
            state = WithItemParsingState::AmbiguousLparRest;
        }

        // Check if our assumption is incorrect and it's actually a parenthesized
        // expression.
        if !with_item_kind.is_parenthesized_expression() && self.at(TokenKind::Rpar) {
            if has_optional_vars {
                // If any of the with item has optional variables, then our assumption is
                // correct and it is a parenthesized with items. Now, we need to restrict
                // the grammar for a with item's context expression which is:
                //
                //     with_item: expression ...
                //
                // So, named, starred and yield expressions not allowed.
                for parsed_with_item in &parsed_with_items {
                    // Parentheses resets the precedence.
                    if parsed_with_item.is_parenthesized {
                        continue;
                    }
                    let err = match parsed_with_item.item.context_expr {
                        Expr::Named(_) => ParseErrorType::UnparenthesizedNamedExpression,
                        Expr::Starred(_) => ParseErrorType::InvalidStarredExpressionUsage,
                        Expr::Yield(_) | Expr::YieldFrom(_) => {
                            ParseErrorType::InvalidYieldExpressionUsage
                        }
                        _ => continue,
                    };
                    self.add_error(err, &parsed_with_item.item.context_expr);
                }
            } else if self.peek() == TokenKind::Colon {
                // Here, the parser is at a `)` followed by a `:`.
                if parsed_with_items.is_empty() {
                    // No with items, treat it as a parenthesized expression to
                    // create an empty tuple expression.
                    with_item_kind = WithItemKind::ParenthesizedExpression;
                } else {
                    // These expressions, if unparenthesized, are only allowed if it's
                    // a parenthesized expression and none of the with items have an
                    // optional variable.
                    if parsed_with_items.iter().any(|parsed_with_item| {
                        !parsed_with_item.is_parenthesized
                            && matches!(
                                parsed_with_item.item.context_expr,
                                Expr::Named(_)
                                    | Expr::Starred(_)
                                    | Expr::Yield(_)
                                    | Expr::YieldFrom(_)
                            )
                    }) {
                        with_item_kind = WithItemKind::ParenthesizedExpression;
                    }
                }
            } else {
                // For any other token followed by `)`, if any of the items has
                // an optional variables (`as ...`), then our assumption is correct.
                // Otherwise, treat it as a parenthesized expression. For example:
                //
                // ```python
                // with (item1, item2 as f): ...
                // ```
                //
                // This also helps in raising the correct syntax error for the
                // following case:
                // ```python
                // with (item1, item2 as f) as x: ...
                // #                        ^^
                // #                        Expecting `:` but got `as`
                // ```
                with_item_kind = WithItemKind::ParenthesizedExpression;
            }
        }

        if with_item_kind == WithItemKind::Parenthesized && !self.at(TokenKind::Rpar) {
            // test_err with_items_parenthesized_missing_comma
            // with (item1 item2): ...
            // with (item1 as f1 item2): ...
            // with (item1, item2 item3, item4): ...
            // with (item1, item2 as f1 item3, item4): ...
            // with (item1, item2: ...
            self.expect(TokenKind::Comma);
        }

        // Transform the items if it's a parenthesized expression.
        if with_item_kind.is_parenthesized_expression() {
            // The generator expression has already consumed the `)`, so avoid
            // expecting it again.
            if with_item_kind != WithItemKind::SingleParenthesizedGeneratorExpression {
                self.expect(TokenKind::Rpar);
            }

            let mut lhs = if parsed_with_items.len() == 1 && !has_trailing_comma {
                // SAFETY: We've checked that `items` has only one item.
                let expr = parsed_with_items.pop().unwrap().item.context_expr;

                // Here, we know that it's a parenthesized expression so the expression
                // should be checked against the grammar rule which is:
                //
                //     group: (yield_expr | named_expression)
                //
                // So, no starred expression allowed.
                if expr.is_starred_expr() {
                    self.add_error(ParseErrorType::InvalidStarredExpressionUsage, &expr);
                }
                expr
            } else {
                let mut elts = Vec::with_capacity(parsed_with_items.len());

                // Here, we know that it's a tuple expression so each expression should
                // be checked against the tuple element grammar rule which:
                //
                //     tuple: '(' [ star_named_expression ',' [star_named_expressions] ] ')'
                //
                // So, no yield expressions allowed.
                for expr in parsed_with_items
                    .drain(..)
                    .map(|parsed_with_item| parsed_with_item.item.context_expr)
                {
                    if matches!(expr, Expr::Yield(_) | Expr::YieldFrom(_)) {
                        self.add_error(ParseErrorType::InvalidYieldExpressionUsage, &expr);
                    }
                    elts.push(expr);
                }

                Expr::Tuple(ast::ExprTuple {
                    range: self.node_range(start),
                    elts,
                    ctx: ExprContext::Load,
                    parenthesized: true,
                })
            };

            // Remember that the expression is parenthesized and the parser has just
            // consumed the `)` token. We need to check for any possible postfix
            // expressions. For example:
            //
            // ```python
            // with (foo)(): ...
            // #         ^
            //
            // with (1, 2)[0]: ...
            // #          ^
            //
            // with (foo.bar).baz: ...
            // #             ^
            // ```
            //
            // The reason being that the opening parenthesis is ambiguous and isn't
            // considered when parsing the with item in the case. So, the parser
            // stops when it sees the `)` token and doesn't check for any postfix
            // expressions.
            lhs = self.parse_postfix_expression(lhs, start);

            let context_expr = if self.at(TokenKind::If) {
                // test_ok ambiguous_lpar_with_items_if_expr
                // with (x) if True else y: ...
                // with (x for x in iter) if True else y: ...
                // with (x async for x in iter) if True else y: ...
                // with (x)[0] if True else y: ...
                Expr::If(self.parse_if_expression(lhs, start))
            } else {
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
                self.parse_binary_expression_or_higher_recursive(
                    lhs.into(),
                    OperatorPrecedence::Initial,
                    ExpressionContext::default(),
                    start,
                )
                .expr
            };

            let optional_vars = self
                .at(TokenKind::As)
                .then(|| Box::new(self.parse_with_item_optional_vars().expr));

            items.push(ast::WithItem {
                range: self.node_range(start),
                context_expr,
                optional_vars,
            });
        } else {
            items.extend(parsed_with_items.drain(..).map(|item| item.item));
        }

        with_item_kind
    }

    /// Parses a single `with` item.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-with_item>
    fn parse_with_item(&mut self, state: WithItemParsingState) -> ParsedWithItem {
        let start = self.node_start();

        let mut used_ambiguous_lpar = false;

        // The grammar for the context expression of a with item depends on the state
        // of with item parsing.
        let context_expr = if state.is_ambiguous_lpar() {
            // If it's in an ambiguous state, the parenthesis (`(`) could be part of any
            // of the following expression:
            //
            // Tuple expression          -  star_named_expression
            // Generator expression      -  named_expression
            // Parenthesized expression  -  (yield_expr | named_expression)
            // Parenthesized with items  -  expression
            //
            // Here, the right side specifies the grammar for an element corresponding
            // to the expression mentioned in the left side.
            //
            // So, the grammar used should be able to parse an element belonging to any
            // of the above expression. At a later point, once the parser understands
            // where the parenthesis belongs to, it'll validate and report errors for
            // any invalid expression usage.
            //
            // Thus, we can conclude that the grammar used should be:
            //      (yield_expr | star_named_expression)
            let parsed_expr = self
                .parse_named_expression_or_higher(ExpressionContext::yield_or_starred_bitwise_or());

            if matches!(self.current_token_kind(), TokenKind::Async | TokenKind::For) {
                if parsed_expr.is_unparenthesized_starred_expr() {
                    self.add_error(
                        ParseErrorType::IterableUnpackingInComprehension,
                        &parsed_expr,
                    );
                }

                let generator_expr =
                    if let WithItemParsingState::AmbiguousLparFirstItem(lpar_start) = state {
                        // The parser is at the first with item after the ambiguous `(` token.
                        // For example:
                        //
                        // ```python
                        // with (x for x in range(10)): ...
                        // with (x for x in range(10)), item: ...
                        // ```
                        let generator_expr = self.parse_generator_expression(
                            parsed_expr.expr,
                            GeneratorExpressionInParentheses::Maybe {
                                lpar_start,
                                expr_start: start,
                            },
                        );
                        used_ambiguous_lpar = generator_expr.parenthesized;
                        generator_expr
                    } else {
                        // For better error recovery. We would not take this path if the
                        // expression was parenthesized as it would be parsed as a generator
                        // expression by `parse_conditional_expression_or_higher`.
                        //
                        // ```python
                        // # This path will be taken for
                        // with (item, x for x in range(10)): ...
                        //
                        // # This path will not be taken for
                        // with (item, (x for x in range(10))): ...
                        // ```
                        self.parse_generator_expression(
                            parsed_expr.expr,
                            GeneratorExpressionInParentheses::No(start),
                        )
                    };

                if !generator_expr.parenthesized {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "Unparenthesized generator expression cannot be used here".to_string(),
                        ),
                        generator_expr.range(),
                    );
                }

                Expr::Generator(generator_expr).into()
            } else {
                parsed_expr
            }
        } else {
            // If it's not in an ambiguous state, then the grammar of the with item
            // should be used which is `expression`.
            self.parse_conditional_expression_or_higher()
        };

        let optional_vars = self
            .at(TokenKind::As)
            .then(|| Box::new(self.parse_with_item_optional_vars().expr));

        ParsedWithItem {
            is_parenthesized: context_expr.is_parenthesized,
            used_ambiguous_lpar,
            item: ast::WithItem {
                range: self.node_range(start),
                context_expr: context_expr.expr,
                optional_vars,
            },
        }
    }

    /// Parses the optional variables in a `with` item.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `as` token.
    fn parse_with_item_optional_vars(&mut self) -> ParsedExpr {
        self.bump(TokenKind::As);

        let mut target = self
            .parse_conditional_expression_or_higher_impl(ExpressionContext::starred_conditional());

        // This has the same semantics as an assignment target.
        self.validate_assignment_target(&target.expr);

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        target
    }

    /// Parses a match statement.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `match` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#the-match-statement>
    fn parse_match_statement(&mut self) -> ast::StmtMatch {
        let start = self.node_start();
        self.bump(TokenKind::Match);

        let subject_start = self.node_start();

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
        let subject = if self.at(TokenKind::Comma) {
            let tuple =
                self.parse_tuple_expression(subject.expr, subject_start, Parenthesized::No, |p| {
                    p.parse_named_expression_or_higher(ExpressionContext::starred_bitwise_or())
                });

            Expr::Tuple(tuple).into()
        } else {
            if subject.is_unparenthesized_starred_expr() {
                // test_err match_stmt_single_starred_subject
                // match *foo:
                //     case _: ...
                self.add_error(ParseErrorType::InvalidStarredExpressionUsage, &subject);
            }
            subject
        };

        self.expect(TokenKind::Colon);

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

        ast::StmtMatch {
            subject: Box::new(subject.expr),
            cases,
            range: self.node_range(start),
        }
    }

    /// Parses a list of match case blocks.
    fn parse_match_case_blocks(&mut self) -> Vec<ast::MatchCase> {
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
    fn parse_match_case(&mut self) -> ast::MatchCase {
        let start = self.node_start();
        self.bump(TokenKind::Case);

        // test_err match_stmt_missing_pattern
        // # TODO(dhruvmanila): Here, `case` is a name token because of soft keyword transformer
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
                Some(Box::new(
                    self.parse_named_expression_or_higher(ExpressionContext::default())
                        .expr,
                ))
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

        ast::MatchCase {
            pattern,
            guard,
            body,
            range: self.node_range(start),
        }
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
    fn parse_async_statement(&mut self) -> Stmt {
        let async_start = self.node_start();
        self.bump(TokenKind::Async);

        match self.current_token_kind() {
            // test_ok async_function_definition
            // async def foo(): ...
            TokenKind::Def => Stmt::FunctionDef(ast::StmtFunctionDef {
                is_async: true,
                ..self.parse_function_definition(vec![], async_start)
            }),

            // test_ok async_with_statement
            // async with item: ...
            TokenKind::With => Stmt::With(ast::StmtWith {
                is_async: true,
                ..self.parse_with_statement(async_start)
            }),

            // test_ok async_for_statement
            // async for target in iter: ...
            TokenKind::For => Stmt::For(ast::StmtFor {
                is_async: true,
                ..self.parse_for_statement(async_start)
            }),

            kind => {
                // test_err async_unexpected_token
                // async class Foo: ...
                // async while test: ...
                // async x = 1
                // async async def foo(): ...
                // # TODO(dhruvmanila): Here, `match` is actually a Name token because
                // # of the soft keyword # transformer
                // async match test:
                //     case _: ...
                self.add_error(
                    ParseErrorType::UnexpectedTokenAfterAsync(kind),
                    self.current_token_range(),
                );

                // Although this statement is not a valid `async` statement,
                // we still parse it.
                self.parse_statement()
            }
        }
    }

    /// Parses a decorator list followed by a class, function or async function definition.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-decorators>
    fn parse_decorators(&mut self) -> Stmt {
        let start = self.node_start();

        let mut decorators = vec![];
        let mut progress = ParserProgress::default();

        // test_err decorator_missing_expression
        // @def foo(): ...
        // @
        // def foo(): ...
        // @@
        // def foo(): ...
        while self.at(TokenKind::At) {
            progress.assert_progressing(self);

            let decorator_start = self.node_start();
            self.bump(TokenKind::At);

            // test_err decorator_invalid_expression
            // @*x
            // @(*x)
            // @((*x))
            // @yield x
            // @yield from x
            // def foo(): ...
            let parsed_expr = self.parse_named_expression_or_higher(ExpressionContext::default());

            decorators.push(ast::Decorator {
                expression: parsed_expr.expr,
                range: self.node_range(decorator_start),
            });

            // test_err decorator_missing_newline
            // @x def foo(): ...
            // @x async def foo(): ...
            // @x class Foo: ...
            self.expect(TokenKind::Newline);
        }

        match self.current_token_kind() {
            TokenKind::Def => Stmt::FunctionDef(self.parse_function_definition(decorators, start)),
            TokenKind::Class => Stmt::ClassDef(self.parse_class_definition(decorators, start)),
            TokenKind::Async if self.peek() == TokenKind::Def => {
                self.bump(TokenKind::Async);

                // test_ok decorator_async_function
                // @decorator
                // async def foo(): ...
                Stmt::FunctionDef(ast::StmtFunctionDef {
                    is_async: true,
                    ..self.parse_function_definition(decorators, start)
                })
            }
            _ => {
                // test_err decorator_unexpected_token
                // @foo
                // async with x: ...
                // @foo
                // x = 1
                self.add_error(
                    ParseErrorType::OtherError(
                        "Expected class, function definition or async function definition after decorator".to_string(),
                    ),
                    self.current_token_range(),
                );

                // TODO(dhruvmanila): It seems that this recovery drops all the parsed
                // decorators. Maybe we could convert them into statement expression
                // with a flag indicating that this expression is part of a decorator.
                // It's only possible to keep them if it's a function or class definition.
                // We could possibly keep them if there's indentation error:
                //
                // ```python
                // @decorator
                //   @decorator
                // def foo(): ...
                // ```
                //
                // Or, parse it as a binary expression where the left side is missing.
                // We would need to convert each decorator into a binary expression.
                self.parse_statement()
            }
        }
    }

    /// Parses the body of the given [`Clause`].
    ///
    /// This could either be a single statement that's on the same line as the
    /// clause header or an indented block.
    fn parse_body(&mut self, parent_clause: Clause) -> Vec<Stmt> {
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

        Vec::new()
    }

    /// Parses a block of statements.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at an `Indent` token.
    fn parse_block(&mut self) -> Vec<Stmt> {
        self.bump(TokenKind::Indent);

        let statements =
            self.parse_list_into_vec(RecoveryContextKind::BlockStatements, Self::parse_statement);

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
    ) -> ast::Parameter {
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
                            self.parse_conditional_expression_or_higher_impl(
                                ExpressionContext::starred_bitwise_or(),
                            )
                        }
                        AllowStarAnnotation::No => {
                            // test_ok param_with_annotation
                            // def foo(arg: int): ...
                            // def foo(arg: lambda x: x): ...
                            // def foo(arg: (yield x)): ...
                            // def foo(arg: (x := int)): ...

                            // test_err param_with_invalid_annotation
                            // def foo(arg: *int): ...
                            // def foo(arg: yield int): ...
                            // def foo(arg: x := int): ...
                            self.parse_conditional_expression_or_higher()
                        }
                    };
                    Some(Box::new(parsed_expr.expr))
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

        ast::Parameter {
            range: self.node_range(start),
            name,
            annotation,
        }
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
    ) -> ast::ParameterWithDefault {
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
                Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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

        ast::ParameterWithDefault {
            range: self.node_range(start),
            parameter,
            default,
        }
    }

    /// Parses a parameter list for the given function kind.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-parameter_list>
    pub(super) fn parse_parameters(&mut self, function_kind: FunctionKind) -> ast::Parameters {
        let start = self.node_start();

        if matches!(function_kind, FunctionKind::FunctionDef) {
            self.expect(TokenKind::Lpar);
        }

        // TODO(dhruvmanila): This has the same problem as `parse_match_pattern_mapping`
        // has where if there are multiple kwarg or vararg, the last one will win and
        // the parser will drop the previous ones. Another thing is the vararg and kwarg
        // uses `Parameter` (not `ParameterWithDefault`) which means that the parser cannot
        // recover well from `*args=(1, 2)`.
        let mut parameters = ast::Parameters::default();

        let mut seen_default_param = false; // `a=10`
        let mut seen_positional_only_separator = false; // `/`
        let mut seen_keyword_only_separator = false; // `*`
        let mut seen_keyword_only_param_after_separator = false;

        // Range of the keyword only separator if it's the last parameter in the list.
        let mut last_keyword_only_separator_range = None;

        self.parse_comma_separated_list(RecoveryContextKind::Parameters(function_kind), |parser| {
            let param_start = parser.node_start();

            if parameters.kwarg.is_some() {
                // TODO(dhruvmanila): This fails AST validation in tests because
                // of the pre-order visit
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

                    if parser.at(TokenKind::Name) {
                        let param = parser.parse_parameter(param_start, function_kind, AllowStarAnnotation::Yes);
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
                            parameters.vararg = Some(Box::new(param));
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
                                    "Keyword-only parameter separator not allowed after '*' parameter"
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

                    let param = parser.parse_parameter(param_start, function_kind, AllowStarAnnotation::No);
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

                    parameters.kwarg = Some(Box::new(param));
                    last_keyword_only_separator_range = None;
                }
                TokenKind::Slash => {
                    let slash_range = parser.current_token_range();
                    parser.bump(TokenKind::Slash);

                    if parameters.is_empty() {
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
                        // We should only swap if we're seeing the separator for the
                        // first time, otherwise it's a user error.
                        std::mem::swap(&mut parameters.args, &mut parameters.posonlyargs);
                        seen_positional_only_separator = true;
                    }

                    last_keyword_only_separator_range = None;
                }
                TokenKind::Name => {
                    let param = parser.parse_parameter_with_default(param_start, function_kind);

                    // TODO(dhruvmanila): Pyright seems to only highlight the first non-default argument
                    // https://github.com/microsoft/pyright/blob/3b70417dd549f6663b8f86a76f75d8dfd450f4a8/packages/pyright-internal/src/parser/parser.ts#L2038-L2042
                    if param.default.is_none()
                        && seen_default_param
                        && !seen_keyword_only_separator
                        && parameters.vararg.is_none()
                    {
                        // test_ok params_non_default_after_star
                        // def foo(a=10, *, b, c=11, d): ...
                        // def foo(a=10, *args, b, c=11, d): ...

                        // test_err params_non_default_after_default
                        // def foo(a=10, b, c: int): ...
                        parser
                            .add_error(ParseErrorType::NonDefaultParamAfterDefaultParam, &param);
                    }

                    seen_default_param |= param.default.is_some();

                    if seen_keyword_only_separator {
                        seen_keyword_only_param_after_separator = true;
                    }

                    if seen_keyword_only_separator || parameters.vararg.is_some() {
                        parameters.kwonlyargs.push(param);
                    } else {
                        parameters.args.push(param);
                    }
                    last_keyword_only_separator_range = None;
                }
                _ => {
                    // This corresponds to the expected token kinds for `is_list_element`.
                    unreachable!("Expected Name, '*', '**', or '/'");
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

        parameters.range = self.node_range(start);

        // test_err params_duplicate_names
        // def foo(a, a=10, *a, a, a: str, **a): ...
        self.validate_parameters(&parameters);

        parameters
    }

    /// Try to parse a type parameter list. If the parser is not at the start of a
    /// type parameter list, return `None`.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#type-parameter-lists>
    fn try_parse_type_params(&mut self) -> Option<ast::TypeParams> {
        self.at(TokenKind::Lsqb).then(|| self.parse_type_params())
    }

    /// Parses a type parameter list.
    ///
    /// # Panics
    ///
    /// If the parser isn't positioned at a `[` token.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#type-parameter-lists>
    fn parse_type_params(&mut self) -> ast::TypeParams {
        let start = self.node_start();
        self.bump(TokenKind::Lsqb);

        let type_params = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::TypeParams,
            Parser::parse_type_param,
        );

        self.expect(TokenKind::Rsqb);

        ast::TypeParams {
            range: self.node_range(start),
            type_params,
        }
    }

    /// Parses a type parameter.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-type_param>
    fn parse_type_param(&mut self) -> ast::TypeParam {
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
                    Some(Box::new(
                        self.parse_conditional_expression_or_higher_impl(
                            ExpressionContext::starred_bitwise_or(),
                        )
                        .expr,
                    ))
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
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                range: self.node_range(start),
                name,
                default,
            })

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
                    Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                range: self.node_range(start),
                name,
                default,
            })
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
                    Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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

            let default = if self.eat(TokenKind::Equal) {
                if self.at_expr() {
                    // test_err type_param_type_var_invalid_default_expr
                    // type X[T = *int] = int
                    // type X[T = yield x] = int
                    // type X[T = (yield x)] = int
                    // type X[T = yield from x] = int
                    // type X[T = x := int] = int
                    // type X[T: int = *int] = int
                    Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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

            ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                range: self.node_range(start),
                name,
                bound,
                default,
            })
        }
    }

    /// Validate that the given expression is a valid assignment target.
    ///
    /// If the expression is a list or tuple, then validate each element in the list.
    /// If it's a starred expression, then validate the value of the starred expression.
    ///
    /// Report an error for each invalid assignment expression found.
    pub(super) fn validate_assignment_target(&mut self, expr: &Expr) {
        match expr {
            Expr::Starred(ast::ExprStarred { value, .. }) => self.validate_assignment_target(value),
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for expr in elts {
                    self.validate_assignment_target(expr);
                }
            }
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => {}
            _ => self.add_error(ParseErrorType::InvalidAssignmentTarget, expr.range()),
        }
    }

    /// Validate that the given expression is a valid annotated assignment target.
    ///
    /// Unlike [`Parser::validate_assignment_target`], starred, list and tuple
    /// expressions aren't allowed here.
    fn validate_annotated_assignment_target(&mut self, expr: &Expr) {
        match expr {
            Expr::List(_) => self.add_error(
                ParseErrorType::OtherError(
                    "Only single target (not list) can be annotated".to_string(),
                ),
                expr,
            ),
            Expr::Tuple(_) => self.add_error(
                ParseErrorType::OtherError(
                    "Only single target (not tuple) can be annotated".to_string(),
                ),
                expr,
            ),
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => {}
            _ => self.add_error(ParseErrorType::InvalidAnnotatedAssignmentTarget, expr),
        }
    }

    /// Validate that the given expression is a valid delete target.
    ///
    /// If the expression is a list or tuple, then validate each element in the list.
    ///
    /// See: <https://github.com/python/cpython/blob/d864b0094f9875c5613cbb0b7f7f3ca8f1c6b606/Parser/action_helpers.c#L1150-L1180>
    fn validate_delete_target(&mut self, expr: &Expr) {
        match expr {
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for expr in elts {
                    self.validate_delete_target(expr);
                }
            }
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => {}
            _ => self.add_error(ParseErrorType::InvalidDeleteTarget, expr),
        }
    }

    /// Validate that the given parameters doesn't have any duplicate names.
    ///
    /// Report errors for all the duplicate names found.
    fn validate_parameters(&mut self, parameters: &ast::Parameters) {
        let mut all_arg_names =
            FxHashSet::with_capacity_and_hasher(parameters.len(), BuildHasherDefault::default());

        for parameter in parameters {
            let range = parameter.name().range();
            let param_name = parameter.name().as_str();
            if !all_arg_names.insert(param_name) {
                self.add_error(
                    ParseErrorType::DuplicateParameter(param_name.to_string()),
                    range,
                );
            }
        }
    }

    /// Specialized [`Parser::parse_list_into_vec`] for parsing a sequence of clauses.
    ///
    /// The difference is that the parser only continues parsing for as long as it sees the token
    /// indicating the start of the specific clause. This is different from
    /// [`Parser::parse_list_into_vec`] that performs error recovery when the next token is not a
    /// list terminator or the start of a list element.
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
    fn parse_clauses<T>(
        &mut self,
        clause: Clause,
        mut parse_clause: impl FnMut(&mut Parser<'src>) -> T,
    ) -> Vec<T> {
        let mut clauses = Vec::new();
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

            clauses.push(parse_clause(self));
        }

        self.recovery_context = saved_context;

        clauses
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WithItemParsingState {
    /// The parser is currently parsing a with item without any ambiguity.
    Regular,

    /// The parser is currently parsing the first with item after an ambiguous
    /// left parenthesis. The contained offset is the start of the left parenthesis.
    ///
    /// ```python
    /// with (item1, item2): ...
    /// ```
    ///
    /// The parser is at the start of `item1`.
    AmbiguousLparFirstItem(TextSize),

    /// The parser is currently parsing one of the with items after an ambiguous
    /// left parenthesis, but not the first one.
    ///
    /// ```python
    /// with (item1, item2, item3): ...
    /// ```
    ///
    /// The parser could be at the start of `item2` or `item3`, but not `item1`.
    AmbiguousLparRest,
}

impl WithItemParsingState {
    const fn is_ambiguous_lpar(self) -> bool {
        matches!(
            self,
            Self::AmbiguousLparFirstItem(_) | Self::AmbiguousLparRest
        )
    }
}

struct ParsedWithItem {
    /// The contained with item.
    item: WithItem,
    /// If the context expression of the item is parenthesized.
    is_parenthesized: bool,
    /// If the parsing used the ambiguous left parenthesis.
    used_ambiguous_lpar: bool,
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
    Star,
}

impl ExceptClauseKind {
    const fn is_star(self) -> bool {
        matches!(self, ExceptClauseKind::Star)
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
