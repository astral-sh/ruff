use std::fmt::Display;

use ruff_python_ast::{
    self as ast, ExceptHandler, Expr, ExprContext, IpyEscapeKind, Operator, Stmt, WithItem,
};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::expression::{GeneratorExpressionInParentheses, ParsedExpr};
use crate::parser::progress::ParserProgress;
use crate::parser::{
    helpers, FunctionKind, Parser, ParserCtxFlags, RecoveryContext, RecoveryContextKind,
    WithItemKind, EXPR_SET,
};
use crate::token_set::TokenSet;
use crate::{Mode, ParseErrorType, Tok, TokenKind};

use super::expression::{AllowNamedExpression, AllowStarredExpression};
use super::Parenthesized;

/// Tokens that can appear after an expression.
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
    TokenKind::EscapeCommand,
]);

/// Tokens that represent simple statements, including expressions.
const SIMPLE_STMT_SET2: TokenSet = SIMPLE_STMT_SET.union(EXPR_SET);

const STMTS_SET: TokenSet = SIMPLE_STMT_SET2.union(COMPOUND_STMT_SET);

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
    pub(super) fn at_compound_stmt(&self) -> bool {
        self.at_ts(COMPOUND_STMT_SET)
    }

    fn at_simple_stmt(&self) -> bool {
        self.at_ts(SIMPLE_STMT_SET2)
    }

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

    /// Parses a compound or a simple statement.
    pub(super) fn parse_statement(&mut self) -> Stmt {
        let start_offset = self.node_start();
        match self.current_token_kind() {
            TokenKind::If => Stmt::If(self.parse_if_statement()),
            TokenKind::For => Stmt::For(self.parse_for_statement(start_offset)),
            TokenKind::While => Stmt::While(self.parse_while_statement()),
            TokenKind::Def => {
                Stmt::FunctionDef(self.parse_function_definition(vec![], start_offset))
            }
            TokenKind::Class => Stmt::ClassDef(self.parse_class_definition(vec![], start_offset)),
            TokenKind::Try => Stmt::Try(self.parse_try_statement()),
            TokenKind::With => Stmt::With(self.parse_with_statement(start_offset)),
            TokenKind::At => self.parse_decorators(),
            TokenKind::Async => self.parse_async_statement(),
            TokenKind::Match => Stmt::Match(self.parse_match_statement()),
            _ => self.parse_single_simple_statement(),
        }
    }

    /// Parses a single simple statement, expecting it to be terminated by a newline or semicolon.
    /// TODO(micha): It's not entirely clear why this method is necessary. It is called from
    /// `parse_body` and it only reads out the first simple statement before calling `parse_statement` again.
    /// This makes me wonder if the parser incorrectly allows `a;if b: pass`
    fn parse_single_simple_statement(&mut self) -> Stmt {
        let stmt = self.parse_simple_statement();

        let has_eaten_semicolon = self.eat(TokenKind::Semi);
        let has_eaten_newline = self.eat(TokenKind::Newline);

        if !has_eaten_newline && !has_eaten_semicolon && self.at_simple_stmt() {
            let range = self.current_token_range();
            self.add_error(
                ParseErrorType::SimpleStmtsInSameLine,
                stmt.range().cover(range),
            );
        }

        if !has_eaten_newline && self.at_compound_stmt() {
            self.add_error(
                ParseErrorType::SimpleStmtAndCompoundStmtInSameLine,
                stmt.range().cover(self.current_token_range()),
            );
        }

        stmt
    }

    fn parse_simple_statements(&mut self) -> Vec<Stmt> {
        let mut stmts = vec![];
        let start = self.node_start();
        let mut progress = ParserProgress::default();

        loop {
            progress.assert_progressing(self);
            stmts.push(self.parse_simple_statement());

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
            self.add_error(
                ParseErrorType::SimpleStmtAndCompoundStmtInSameLine,
                self.node_range(start),
            );
        }

        stmts
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#simple-statements>
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
            TokenKind::EscapeCommand if self.mode == Mode::Ipython => {
                Stmt::IpyEscapeCommand(self.parse_ipython_escape_command_statement())
            }
            _ => {
                let start = self.node_start();
                let parsed_expr =
                    self.parse_yield_expression_or_else(Parser::parse_star_expression_list);

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
                } else if self.mode == Mode::Ipython && self.eat(TokenKind::Question) {
                    let mut kind = IpyEscapeKind::Help;

                    if self.eat(TokenKind::Question) {
                        kind = IpyEscapeKind::Help2;
                    }

                    // FIXME(micha): Is this range correct
                    let range = self.node_range(start);
                    Stmt::IpyEscapeCommand(ast::StmtIpyEscapeCommand {
                        value: self
                            .src_text(parsed_expr.range())
                            .to_string()
                            .into_boxed_str(),
                        kind,
                        range,
                    })
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
                let mut target =
                    parser.parse_conditional_expression_or_higher(AllowStarredExpression::Yes);
                helpers::set_expr_ctx(&mut target.expr, ExprContext::Del);

                if !helpers::is_valid_del_target(&target.expr) {
                    // test_err invalid_del_target
                    // del x + 1
                    // del {'x': 1}
                    // del {'x', 'y'}
                    // del None, True, False, 1, 1.0, "abc"
                    parser.add_error(ParseErrorType::InvalidDeleteTarget, &target.expr);
                }
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
        let value = self
            .at_expr()
            .then(|| Box::new(self.parse_star_expression_list().expr));

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
            // TODO(dhruvmanila): Disallow starred and yield expression
            // test_err raise_stmt_invalid_exc
            // raise *x
            // raise yield x
            // raise x := 1
            let exc = self.parse_expression_list(AllowStarredExpression::No);

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
            // TODO(dhruvmanila): Disallow starred and yield expression
            // test_err raise_stmt_invalid_cause
            // raise x from *y
            // raise x from yield y
            // raise x from y := 1
            let cause = self.parse_expression_list(AllowStarredExpression::No);

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

        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::ImportNames,
            Parser::parse_alias,
        );

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
                let alias = parser.parse_alias();
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
            level: Some(leading_dots),
            range: self.node_range(start),
        }
    }

    /// Parses an `import` or `from` import name.
    ///
    /// See:
    /// - <https://docs.python.org/3/reference/simple_stmts.html#the-import-statement>
    /// - <https://docs.python.org/3/library/ast.html#ast.alias>
    fn parse_alias(&mut self) -> ast::Alias {
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

        let name = self.parse_dotted_name();

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

        // TODO(dhruvmanila): Disallow starred and yield expression
        // test_err assert_invalid_test_expr
        // assert *x
        // assert assert x
        // assert yield x
        // assert x := 1
        let test = self.parse_conditional_expression_or_higher(AllowStarredExpression::No);

        let msg = if self.eat(TokenKind::Comma) {
            if self.at_expr() {
                // TODO(dhruvmanila): Disallow starred and yield expression
                // test_err assert_invalid_msg_expr
                // assert False, *x
                // assert False, assert x
                // assert False, yield x
                // assert False, x := 1
                Some(Box::new(
                    self.parse_conditional_expression_or_higher(AllowStarredExpression::No)
                        .expr,
                ))
            } else {
                // test_err assert_empty_msg
                // assert x,
                self.add_error(
                    ParseErrorType::OtherError("Expected an expression".to_string()),
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
        let value = self.parse_conditional_expression_or_higher(AllowStarredExpression::No);

        ast::StmtTypeAlias {
            name: Box::new(name),
            type_params,
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    fn parse_ipython_escape_command_statement(&mut self) -> ast::StmtIpyEscapeCommand {
        let start = self.node_start();
        let (Tok::IpyEscapeCommand { value, kind }, _) = self.bump(TokenKind::EscapeCommand) else {
            unreachable!()
        };

        ast::StmtIpyEscapeCommand {
            range: self.node_range(start),
            kind,
            value,
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

        let mut value = self.parse_yield_expression_or_else(Parser::parse_star_expression_list);

        if self.at(TokenKind::Equal) {
            // This path is only taken when there are more than one assignment targets.
            self.parse_list(RecoveryContextKind::AssignmentTargets, |parser| {
                parser.bump(TokenKind::Equal);

                let mut parsed_expr =
                    parser.parse_yield_expression_or_else(Parser::parse_star_expression_list);

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

        let simple = target.is_name_expr() && !target.is_parenthesized;

        // test_err ann_assign_stmt_invalid_annotation
        // x: *int = 1
        // x: yield a = 1
        // x: yield from b = 1
        // x: y := int = 1
        let annotation = self.parse_conditional_expression_or_higher(AllowStarredExpression::No);

        let value = if self.eat(TokenKind::Equal) {
            if self.at_expr() {
                // test_err ann_assign_stmt_invalid_value
                // x: Any = *a and b
                // x: Any = x := 1
                // x: list = [x, *a | b, *a or b]
                Some(Box::new(
                    self.parse_yield_expression_or_else(Parser::parse_star_expression_list)
                        .expr,
                ))
            } else {
                // test_err ann_assign_stmt_missing_rhs
                // x: int =
                self.add_error(
                    ParseErrorType::OtherError("Expected an expression".to_string()),
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
        let value = self.parse_yield_expression_or_else(Parser::parse_star_expression_list);

        ast::StmtAugAssign {
            target: Box::new(target.expr),
            op,
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-if_stmt>
    fn parse_if_statement(&mut self) -> ast::StmtIf {
        let if_start = self.node_start();
        self.bump(TokenKind::If);

        let test = self.parse_named_expression_or_higher(AllowStarredExpression::No);
        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::If);

        let elif_else_clauses = self.parse_elif_else_clauses();

        ast::StmtIf {
            test: Box::new(test.expr),
            body,
            elif_else_clauses,
            range: self.node_range(if_start),
        }
    }

    fn parse_elif_else_clauses(&mut self) -> Vec<ast::ElifElseClause> {
        let mut elif_else_clauses = if self.at(TokenKind::Elif) {
            self.parse_clauses(Clause::ElIf, |p| {
                let elif_start = p.node_start();
                p.bump(TokenKind::Elif);

                let test = p.parse_named_expression_or_higher(AllowStarredExpression::No);
                p.expect(TokenKind::Colon);

                let body = p.parse_body(Clause::ElIf);

                ast::ElifElseClause {
                    test: Some(test.expr),
                    body,
                    range: p.node_range(elif_start),
                }
            })
        } else {
            Vec::new()
        };

        let else_start = self.node_start();
        if self.eat(TokenKind::Else) {
            self.expect(TokenKind::Colon);

            let body = self.parse_body(Clause::Else);

            elif_else_clauses.push(ast::ElifElseClause {
                test: None,
                body,
                range: self.node_range(else_start),
            });
        }

        elif_else_clauses
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-try_stmt>
    fn parse_try_statement(&mut self) -> ast::StmtTry {
        let try_start = self.node_start();
        self.bump(TokenKind::Try);
        self.expect(TokenKind::Colon);

        let mut is_star = false;

        let try_body = self.parse_body(Clause::Try);

        let has_except = self.at(TokenKind::Except);
        let handlers = self.parse_clauses(Clause::Except, |p| {
            let except_start = p.node_start();
            p.bump(TokenKind::Except);

            // TODO(micha): Should this be local to the except block or global for the try statement or do we need to track both?
            is_star = p.eat(TokenKind::Star);

            let type_ = if p.at(TokenKind::Colon) && !is_star {
                None
            } else {
                let parsed_expr = p.parse_expression_list(AllowStarredExpression::No);
                if matches!(
                    parsed_expr.expr,
                    Expr::Tuple(ast::ExprTuple {
                        parenthesized: false,
                        ..
                    })
                ) {
                    p.add_error(
                        ParseErrorType::OtherError(
                            "multiple exception types must be parenthesized".to_string(),
                        ),
                        &parsed_expr,
                    );
                }
                Some(Box::new(parsed_expr.expr))
            };

            let name = p.eat(TokenKind::As).then(|| p.parse_identifier());

            p.expect(TokenKind::Colon);

            let except_body = p.parse_body(Clause::Except);

            ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                type_,
                name,
                body: except_body,
                range: p.node_range(except_start),
            })
        });

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
            let range = self.current_token_range();
            self.add_error(
                ParseErrorType::OtherError(
                    "expecting `except` or `finally` after `try` block".to_string(),
                ),
                range,
            );
        }

        let range = self.node_range(try_start);

        ast::StmtTry {
            body: try_body,
            handlers,
            orelse,
            finalbody,
            is_star,
            range,
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-for_stmt>
    fn parse_for_statement(&mut self, for_start: TextSize) -> ast::StmtFor {
        self.bump(TokenKind::For);

        let saved_context = self.set_ctx(ParserCtxFlags::FOR_TARGET);
        let mut target = self.parse_expression_list(AllowStarredExpression::Yes);
        self.restore_ctx(ParserCtxFlags::FOR_TARGET, saved_context);

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        self.expect(TokenKind::In);

        let iter = self.parse_expression_list(AllowStarredExpression::Yes);

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
            range: self.node_range(for_start),
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-while_stmt>
    fn parse_while_statement(&mut self) -> ast::StmtWhile {
        let while_start = self.node_start();
        self.bump(TokenKind::While);

        let test = self.parse_named_expression_or_higher(AllowStarredExpression::No);
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
            range: self.node_range(while_start),
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-funcdef>
    fn parse_function_definition(
        &mut self,
        decorator_list: Vec<ast::Decorator>,
        start_offset: TextSize,
    ) -> ast::StmtFunctionDef {
        self.bump(TokenKind::Def);
        let name = self.parse_identifier();
        let type_params = self.try_parse_type_params();

        let parameters_start = self.node_start();
        self.expect(TokenKind::Lpar);
        let mut parameters = self.parse_parameters(FunctionKind::FunctionDef);
        self.expect(TokenKind::Rpar);
        parameters.range = self.node_range(parameters_start);

        let returns = self.eat(TokenKind::Rarrow).then(|| {
            let returns = self.parse_expression_list(AllowStarredExpression::No);
            if !returns.is_parenthesized && matches!(returns.expr, Expr::Tuple(_)) {
                self.add_error(
                    ParseErrorType::OtherError(
                        "multiple return types must be parenthesized".to_string(),
                    ),
                    returns.range(),
                );
            }
            Box::new(returns.expr)
        });

        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::FunctionDef);

        ast::StmtFunctionDef {
            name,
            type_params,
            parameters: Box::new(parameters),
            body,
            decorator_list,
            is_async: false,
            returns,
            range: self.node_range(start_offset),
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-classdef>
    fn parse_class_definition(
        &mut self,
        decorator_list: Vec<ast::Decorator>,
        start_offset: TextSize,
    ) -> ast::StmtClassDef {
        self.bump(TokenKind::Class);

        let name = self.parse_identifier();
        let type_params = self.try_parse_type_params();
        let arguments = self
            .at(TokenKind::Lpar)
            .then(|| Box::new(self.parse_arguments()));

        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::Class);

        ast::StmtClassDef {
            range: self.node_range(start_offset),
            decorator_list,
            name,
            type_params: type_params.map(Box::new),
            arguments,
            body,
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-with_stmt>
    fn parse_with_statement(&mut self, start_offset: TextSize) -> ast::StmtWith {
        self.bump(TokenKind::With);

        let items = self.parse_with_items();
        self.expect(TokenKind::Colon);

        let body = self.parse_body(Clause::With);

        ast::StmtWith {
            items,
            body,
            is_async: false,
            range: self.node_range(start_offset),
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
                    ParseErrorType::OtherError("Expected an expression".to_string()),
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
                        Expr::Starred(_) => ParseErrorType::StarredExpressionUsage,
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

        // Transform the items if it's a parenthesized expression.
        if with_item_kind.is_parenthesized_expression() {
            // The generator expression has already consumed the `)`, so avoid
            // expecting it again.
            if with_item_kind != WithItemKind::SingleParenthesizedGeneratorExpression {
                self.expect(TokenKind::Rpar);
            }

            let lhs = if parsed_with_items.len() == 1 && !has_trailing_comma {
                // SAFETY: We've checked that `items` has only one item.
                let expr = parsed_with_items.pop().unwrap().item.context_expr;

                // Here, we know that it's a parenthesized expression so the expression
                // should be checked against the grammar rule which is:
                //
                //     group: (yield_expr | named_expression)
                //
                // So, no starred expression allowed.
                if expr.is_starred_expr() {
                    self.add_error(ParseErrorType::StarredExpressionUsage, &expr);
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
            let context_expr = self.parse_postfix_expression(lhs, start);

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
            let parsed_expr = self.parse_yield_expression_or_else(|p| {
                p.parse_star_expression_or_higher(AllowNamedExpression::Yes)
            });

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
                            "unparenthesized generator expression cannot be used here".to_string(),
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
            self.parse_conditional_expression_or_higher(AllowStarredExpression::No)
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

        let mut target = self.parse_conditional_expression_or_higher(AllowStarredExpression::Yes);

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
        let start_offset = self.node_start();

        self.bump(TokenKind::Match);

        let subject_start = self.node_start();
        let subject = self.parse_named_expression_or_higher(AllowStarredExpression::No);
        let subject = if self.at(TokenKind::Comma) {
            let tuple =
                self.parse_tuple_expression(subject.expr, subject_start, Parenthesized::No, |p| {
                    p.parse_named_expression_or_higher(AllowStarredExpression::No)
                });

            Expr::Tuple(tuple).into()
        } else {
            subject
        };

        self.expect(TokenKind::Colon);

        self.eat(TokenKind::Newline);
        if !self.eat(TokenKind::Indent) {
            let range = self.current_token_range();
            self.add_error(
                ParseErrorType::OtherError(
                    "expected an indented block after `match` statement".to_string(),
                ),
                range,
            );
        }

        let cases = self.parse_match_cases();

        self.eat(TokenKind::Dedent);

        ast::StmtMatch {
            subject: Box::new(subject.expr),
            cases,
            range: self.node_range(start_offset),
        }
    }

    /// Parses a list of match case blocks.
    fn parse_match_cases(&mut self) -> Vec<ast::MatchCase> {
        if !self.at(TokenKind::Case) {
            self.add_error(
                ParseErrorType::OtherError("expecting `case` block after `match`".to_string()),
                self.current_token_range(),
            );
        }

        let mut cases = vec![];
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
        let pattern = self.parse_match_patterns();

        let guard = self.eat(TokenKind::If).then(|| {
            Box::new(
                self.parse_named_expression_or_higher(AllowStarredExpression::No)
                    .expr,
            )
        });

        self.expect(TokenKind::Colon);
        let body = self.parse_body(Clause::Match);

        ast::MatchCase {
            pattern,
            guard,
            body,
            range: self.node_range(start),
        }
    }

    /// Parses any statement that is valid after an `async` token.
    /// See:
    ///  - <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-async_with_stmt>
    ///  - <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-async_for_stmt>
    ///  - <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-async_funcdef>
    fn parse_async_statement(&mut self) -> Stmt {
        let async_start = self.node_start();
        self.bump(TokenKind::Async);

        match self.current_token_kind() {
            TokenKind::Def => Stmt::FunctionDef(ast::StmtFunctionDef {
                is_async: true,
                ..self.parse_function_definition(vec![], async_start)
            }),
            TokenKind::With => Stmt::With(ast::StmtWith {
                is_async: true,
                ..self.parse_with_statement(async_start)
            }),
            TokenKind::For => Stmt::For(ast::StmtFor {
                is_async: true,
                ..self.parse_for_statement(async_start)
            }),
            kind => {
                // Although this statement is not a valid `async` statement,
                // we still parse it.
                self.add_error(
                    ParseErrorType::StmtIsNotAsync(kind),
                    self.current_token_range(),
                );
                self.parse_statement()
            }
        }
    }

    fn parse_decorators(&mut self) -> Stmt {
        let start_offset = self.node_start();

        let mut decorators = vec![];
        let mut progress = ParserProgress::default();

        while self.at(TokenKind::At) {
            progress.assert_progressing(self);
            let decorator_start = self.node_start();
            self.bump(TokenKind::At);

            let parsed_expr = self.parse_named_expression_or_higher(AllowStarredExpression::No);
            decorators.push(ast::Decorator {
                expression: parsed_expr.expr,
                range: self.node_range(decorator_start),
            });

            self.expect(TokenKind::Newline);
        }

        match self.current_token_kind() {
            TokenKind::Def => {
                Stmt::FunctionDef(self.parse_function_definition(decorators, start_offset))
            }
            TokenKind::Class => {
                Stmt::ClassDef(self.parse_class_definition(decorators, start_offset))
            }
            TokenKind::Async if self.peek() == TokenKind::Def => {
                self.bump(TokenKind::Async);

                Stmt::FunctionDef(ast::StmtFunctionDef {
                    is_async: true,
                    ..self.parse_function_definition(decorators, start_offset)
                })
            }
            _ => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "expected class, function definition or async function definition after decorator".to_string(),
                    ),
                    self.current_token_range(),
                );
                self.parse_statement()
            }
        }
    }

    /// Parses a single statement that's on the same line as the clause header or
    /// an indented block.
    fn parse_body(&mut self, parent_clause: Clause) -> Vec<Stmt> {
        if self.eat(TokenKind::Newline) {
            if self.at(TokenKind::Indent) {
                return self.parse_block();
            }
        } else if self.at_simple_stmt() {
            return self.parse_simple_statements();
        }

        self.add_error(
            ParseErrorType::OtherError(format!(
                "expected a single statement or an indented body after {parent_clause}"
            )),
            self.current_token_range(),
        );

        Vec::new()
    }

    fn parse_block(&mut self) -> Vec<Stmt> {
        self.bump(TokenKind::Indent);

        let statements =
            self.parse_list_into_vec(RecoveryContextKind::BlockStatements, Self::parse_statement);

        self.expect(TokenKind::Dedent);

        statements
    }

    fn parse_parameter(&mut self, function_kind: FunctionKind) -> ast::Parameter {
        let start = self.node_start();
        let name = self.parse_identifier();
        // If we are at a colon and we're currently parsing a `lambda` expression,
        // this is the `lambda`'s body, don't try to parse as an annotation.
        let annotation = if function_kind == FunctionKind::FunctionDef && self.eat(TokenKind::Colon)
        {
            Some(Box::new(
                self.parse_conditional_expression_or_higher(AllowStarredExpression::Yes)
                    .expr,
            ))
        } else {
            None
        };

        ast::Parameter {
            range: self.node_range(start),
            name,
            annotation,
        }
    }

    fn parse_parameter_with_default(
        &mut self,
        function_kind: FunctionKind,
    ) -> ast::ParameterWithDefault {
        let start = self.node_start();
        let parameter = self.parse_parameter(function_kind);

        let default = self.eat(TokenKind::Equal).then(|| {
            Box::new(
                self.parse_conditional_expression_or_higher(AllowStarredExpression::No)
                    .expr,
            )
        });

        ast::ParameterWithDefault {
            range: self.node_range(start),
            parameter,
            default,
        }
    }

    /// Parses a parameter list.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-parameter_list>
    pub(super) fn parse_parameters(&mut self, function_kind: FunctionKind) -> ast::Parameters {
        let mut args = vec![];
        let mut posonlyargs = vec![];
        let mut kwonlyargs = vec![];
        let mut kwarg = None;
        let mut vararg = None;

        let mut has_seen_asterisk = false;
        let mut has_seen_vararg = false;
        let mut has_seen_default_param = false;

        let start = self.node_start();

        self.parse_comma_separated_list(RecoveryContextKind::Parameters(function_kind), |parser| {
            // Don't allow any parameter after we have seen a vararg `**kwargs`
            if has_seen_vararg {
                parser.add_error(
                    ParseErrorType::ParamFollowsVarKeywordParam,
                    parser.current_token_range(),
                );
            }

            if parser.eat(TokenKind::Star) {
                has_seen_asterisk = true;
                if parser.at(TokenKind::Comma) {
                    has_seen_default_param = false;
                } else if parser.at_expr() {
                    let param = parser.parse_parameter(function_kind);
                    vararg = Some(Box::new(param));
                }
            } else if parser.eat(TokenKind::DoubleStar) {
                has_seen_vararg = true;
                let param = parser.parse_parameter(function_kind);
                kwarg = Some(Box::new(param));
            } else if parser.eat(TokenKind::Slash) {
                // Don't allow `/` after a `*`
                if has_seen_asterisk {
                    parser.add_error(
                        ParseErrorType::OtherError("`/` must be ahead of `*`".to_string()),
                        parser.current_token_range(),
                    );
                }
                std::mem::swap(&mut args, &mut posonlyargs);
            } else if parser.at(TokenKind::Name) {
                let param = parser.parse_parameter_with_default(function_kind);
                // Don't allow non-default parameters after default parameters e.g. `a=1, b`,
                // can't place `b` after `a=1`. Non-default parameters are only allowed after
                // default parameters if we have a `*` before them, e.g. `a=1, *, b`.
                if param.default.is_none() && has_seen_default_param && !has_seen_asterisk {
                    parser.add_error(
                        ParseErrorType::DefaultArgumentError,
                        parser.current_token_range(),
                    );
                }
                has_seen_default_param = param.default.is_some();

                if has_seen_asterisk {
                    kwonlyargs.push(param);
                } else {
                    args.push(param);
                }
            }
        });

        let parameters = ast::Parameters {
            range: self.node_range(start),
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        };

        if let Err(error) = helpers::validate_parameters(&parameters) {
            self.add_error(error.error, error.location);
        }

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

        if self.eat(TokenKind::Star) {
            let name = self.parse_identifier();

            // test_err type_param_type_var_tuple_bound
            // type X[*T: int] = int
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                range: self.node_range(start),
                name,
            })
        } else if self.eat(TokenKind::DoubleStar) {
            let name = self.parse_identifier();

            // test_err type_param_param_spec_bound
            // type X[**T: int] = int
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                range: self.node_range(start),
                name,
            })
        } else {
            let name = self.parse_identifier();

            let bound = if self.eat(TokenKind::Colon) {
                if self.at_expr() {
                    // test_err type_param_invalid_bound_expr
                    // type X[T: *int] = int
                    // type X[T: yield x] = int
                    // type X[T: yield from x] = int
                    // type X[T: x := int] = int
                    Some(Box::new(
                        self.parse_conditional_expression_or_higher(AllowStarredExpression::No)
                            .expr,
                    ))
                } else {
                    // test_err type_param_missing_bound
                    // type X[T: ] = int
                    // type X[T1: , T2] = int
                    self.add_error(
                        ParseErrorType::OtherError("Expected an expression".to_string()),
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
                    "only single target (not list) can be annotated".to_string(),
                ),
                expr,
            ),
            Expr::Tuple(_) => self.add_error(
                ParseErrorType::OtherError(
                    "only single target (not tuple) can be annotated".to_string(),
                ),
                expr,
            ),
            Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => {}
            _ => self.add_error(ParseErrorType::InvalidAnnotatedAssignmentTarget, expr),
        }
    }

    /// Specialized [`Parser::parse_sequence`] for parsing a sequence of clauses.
    ///
    /// The difference is that the parser only continues parsing for as long as it sees the token indicating the start
    /// of the specific clause. This is different from [`Parser::parse_sequence`] that performs error recovery when
    /// the next token is not a list terminator or the start of a list element.
    ///
    /// The special method is necessary because Python uses indentation over explicit delimiters to indicate the end of a clause.
    ///
    /// ```python
    /// if True: ...
    /// elif False: ...
    /// elf x: ....
    /// else: ...
    /// ```
    ///
    /// It would be nice if the above example would recover and either skip over the `elf x: ...` or parse it as a nested statement
    /// so that the parser recognises the `else` clause. But Python makes this hard (without writing custom error recovery logic)
    /// because `elf x: ` could also be an annotated assignment that went wrong ;)
    ///
    /// For now, don't recover when parsing clause headers, but add the terminator tokens (e.g. `Else`) to the recovery context
    /// so that expression recovery stops when it encounters an `else` token.
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
    Match,
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
            Clause::Match => write!(f, "`match` statement"),
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
