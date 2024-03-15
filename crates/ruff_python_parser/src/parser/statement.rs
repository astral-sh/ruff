use std::fmt::Display;

use ruff_python_ast::{
    self as ast, ExceptHandler, Expr, ExprContext, IpyEscapeKind, Operator, Stmt, WithItem,
};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::expression::ParsedExpr;
use crate::parser::progress::ParserProgress;
use crate::parser::{
    helpers, FunctionKind, Parser, ParserCtxFlags, RecoveryContext, RecoveryContextKind,
    WithItemKind, EXPR_SET,
};
use crate::token_set::TokenSet;
use crate::{Mode, ParseErrorType, Tok, TokenKind};

use super::TupleParenthesized;

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
                let parsed_expr = self.parse_expression();

                if self.eat(TokenKind::Equal) {
                    Stmt::Assign(self.parse_assign_statement(parsed_expr, start))
                } else if self.eat(TokenKind::Colon) {
                    Stmt::AnnAssign(self.parse_annotated_assignment_statement(parsed_expr, start))
                } else if let Ok(op) = Operator::try_from(self.current_token_kind()) {
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

        let targets = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::DeleteTargets,
            |parser| {
                let mut target = parser.parse_conditional_expression_or_higher();
                helpers::set_expr_ctx(&mut target.expr, ExprContext::Del);

                if !helpers::is_valid_del_target(&target.expr) {
                    parser.add_error(ParseErrorType::InvalidDeleteTarget, &target.expr);
                }
                target.expr
            },
        );

        ast::StmtDelete {
            targets,
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-return_stmt>
    fn parse_return_statement(&mut self) -> ast::StmtReturn {
        let start = self.node_start();
        self.bump(TokenKind::Return);

        let value = self
            .at_expr()
            .then(|| Box::new(self.parse_expression().expr));

        ast::StmtReturn {
            range: self.node_range(start),
            value,
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-raise_stmt>
    fn parse_raise_statement(&mut self) -> ast::StmtRaise {
        let start = self.node_start();
        self.bump(TokenKind::Raise);

        let exc = if self.at(TokenKind::Newline) {
            None
        } else {
            let exc = self.parse_expression();

            if let Expr::Tuple(node) = &exc.expr {
                if !node.parenthesized {
                    self.add_error(
                        ParseErrorType::OtherError(
                            "unparenthesized tuple not allowed in `raise` statement".to_string(),
                        ),
                        node.range,
                    );
                }
            }

            Some(Box::new(exc.expr))
        };

        let cause = (exc.is_some() && self.eat(TokenKind::From)).then(|| {
            let cause = self.parse_expression();

            if let Expr::Tuple(ast::ExprTuple {
                parenthesized: false,
                range: tuple_range,
                ..
            }) = &cause.expr
            {
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized tuple not allowed in `raise from` statement".to_string(),
                    ),
                    tuple_range,
                );
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
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-import_stmt>
    fn parse_import_statement(&mut self) -> ast::StmtImport {
        let start = self.node_start();
        self.bump(TokenKind::Import);

        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::ImportNames,
            Parser::parse_alias,
        );

        // TODO(dhruvmanila): Error when `*` is used

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
        const DOT_ELLIPSIS_SET: TokenSet = TokenSet::new([TokenKind::Dot, TokenKind::Ellipsis]);

        let start = self.node_start();
        self.bump(TokenKind::From);

        let mut module = None;
        let mut level = if self.eat(TokenKind::Ellipsis) { 3 } else { 0 };
        let mut progress = ParserProgress::default();

        while self.at_ts(DOT_ELLIPSIS_SET) {
            progress.assert_progressing(self);

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
            let range = self.current_token_range();
            self.add_error(
                ParseErrorType::OtherError("missing module name".to_string()),
                range,
            );
        }

        self.expect(TokenKind::Import);

        let parenthesized = self.eat(TokenKind::Lpar);

        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::ImportFromAsNames,
            Parser::parse_alias,
        );

        // TODO(dhruvmanila): Error when `*` is mixed with other names.

        if parenthesized {
            self.expect(TokenKind::Rpar);
        }

        ast::StmtImportFrom {
            module,
            names,
            level: Some(level),
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-pass_stmt>
    fn parse_pass_statement(&mut self) -> ast::StmtPass {
        let start = self.node_start();
        self.bump(TokenKind::Pass);
        ast::StmtPass {
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-continue_stmt>
    fn parse_continue_statement(&mut self) -> ast::StmtContinue {
        let start = self.node_start();
        self.bump(TokenKind::Continue);
        ast::StmtContinue {
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-break_stmt>
    fn parse_break_statement(&mut self) -> ast::StmtBreak {
        let start = self.node_start();
        self.bump(TokenKind::Break);
        ast::StmtBreak {
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-assert_stmt>
    fn parse_assert_statement(&mut self) -> ast::StmtAssert {
        let start = self.node_start();
        self.bump(TokenKind::Assert);

        let test = self.parse_conditional_expression_or_higher();

        let msg = self
            .eat(TokenKind::Comma)
            .then(|| Box::new(self.parse_conditional_expression_or_higher().expr));

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

        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::Identifiers,
            Parser::parse_identifier,
        );

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

        let names = self.parse_comma_separated_list_into_vec(
            RecoveryContextKind::Identifiers,
            Parser::parse_identifier,
        );

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
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-type_stmt>
    fn parse_type_alias_statement(&mut self) -> ast::StmtTypeAlias {
        let start = self.node_start();
        self.bump(TokenKind::Type);

        let (tok, tok_range) = self.next_token();
        let name = if let Tok::Name { name } = tok {
            Expr::Name(ast::ExprName {
                id: name.to_string(),
                ctx: ExprContext::Store,
                range: tok_range,
            })
        } else {
            // TODO(dhruvmanila): This recovery isn't possible currently because the soft keyword
            // transformer will always convert the `type` token to a `Name` token if it's not
            // followed by a `Name` token.
            self.add_error(
                ParseErrorType::OtherError(format!("expecting identifier, got {tok}")),
                tok_range,
            );
            Expr::Name(ast::ExprName {
                id: String::new(),
                ctx: ExprContext::Invalid,
                range: tok_range,
            })
        };
        let type_params = self.try_parse_type_params();

        self.expect(TokenKind::Equal);

        let value = self.parse_conditional_expression_or_higher();

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

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-assignment_stmt>
    fn parse_assign_statement(&mut self, target: ParsedExpr, start: TextSize) -> ast::StmtAssign {
        let mut targets = vec![target.expr];
        let mut value = self.parse_expression();

        if self.at(TokenKind::Equal) {
            self.parse_list(RecoveryContextKind::AssignmentTargets, |parser| {
                parser.bump(TokenKind::Equal);

                let mut parsed_expr = parser.parse_expression();

                std::mem::swap(&mut value, &mut parsed_expr);

                targets.push(parsed_expr.expr);
            });
        }

        targets
            .iter_mut()
            .for_each(|target| helpers::set_expr_ctx(target, ExprContext::Store));

        if !targets.iter().all(helpers::is_valid_assignment_target) {
            targets
                .iter()
                .filter(|target| !helpers::is_valid_assignment_target(target))
                .for_each(|target| {
                    self.add_error(ParseErrorType::InvalidAssignmentTarget, target.range());
                });
        }

        ast::StmtAssign {
            targets,
            value: Box::new(value.expr),
            range: self.node_range(start),
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-annotated_assignment_stmt>
    fn parse_annotated_assignment_statement(
        &mut self,
        mut target: ParsedExpr,
        start: TextSize,
    ) -> ast::StmtAnnAssign {
        if !helpers::is_valid_assignment_target(&target.expr) {
            self.add_error(ParseErrorType::InvalidAssignmentTarget, target.range());
        }

        if matches!(target.expr, Expr::Tuple(_)) {
            self.add_error(
                ParseErrorType::OtherError(
                    "only single target (not tuple) can be annotated".into(),
                ),
                target.range(),
            );
        }

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        let simple = target.expr.is_name_expr() && !target.is_parenthesized;
        let annotation = self.parse_expression();

        if matches!(
            annotation.expr,
            Expr::Tuple(ast::ExprTuple {
                parenthesized: false,
                ..
            })
        ) {
            self.add_error(
                ParseErrorType::OtherError("annotation cannot be unparenthesized".into()),
                annotation.range(),
            );
        }

        let value = self
            .eat(TokenKind::Equal)
            .then(|| Box::new(self.parse_expression().expr));

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
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-augmented_assignment_stmt>
    fn parse_augmented_assignment_statement(
        &mut self,
        mut target: ParsedExpr,
        op: Operator,
        start: TextSize,
    ) -> ast::StmtAugAssign {
        // Consume the operator
        self.bump_ts(AUGMENTED_ASSIGN_SET);

        if !helpers::is_valid_aug_assignment_target(&target.expr) {
            self.add_error(
                ParseErrorType::InvalidAugmentedAssignmentTarget,
                target.range(),
            );
        }

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        let value = self.parse_expression();

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

        let test = self.parse_named_expression_or_higher();
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

                let test = p.parse_named_expression_or_higher();
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
                let parsed_expr = p.parse_expression();
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
        let mut target = self.parse_expression();
        self.restore_ctx(ParserCtxFlags::FOR_TARGET, saved_context);

        helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

        self.expect(TokenKind::In);

        let iter = self.parse_expression();

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

        let test = self.parse_named_expression_or_higher();
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
            let returns = self.parse_expression();
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
            // stating that a trailing comma isn't allowed, while (2) will raise the following
            // error.
            //
            // The reason that (2) expects an expression is because if it raised an error
            // similar to (3), we would be suggesting to remove the trailing comma, which would
            // make it a parenthesized with items. This would contradict our original assumption.
            // However, for (3), if the trailing comma is removed, it still remains a parenthesized
            // expression.
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
    /// This method is used to parse the with items when the parser has seen an
    /// ambiguous `(` token. It's used to determine if the with items are
    /// parenthesized or it's a parenthesized expression.
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

        // Keep track of any trailing comma. This is used to determine if it's a
        // tuple expression or not in the case of a single with item.
        let mut has_trailing_comma = false;

        // Start with parsing the first with item after an ambiguous `(` token
        // with the start offset.
        let mut state = WithItemParsingState::AmbiguousLparFirstItem(start);

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

            match parsed_with_item.item.context_expr {
                Expr::Named(_) if !parsed_with_item.is_parenthesized => {
                    // If the named expression isn't parenthesized, then:
                    //
                    // 1. It has either used the ambiguous `(` token e.g., `with (item := 10): ...` or
                    //    `with (item := 10) as f: ...`.
                    // 2. It's a tuple element e.g., `with (item1, item2 := 10): ...`.
                    //
                    // In either case, our assumption is incorrect as it's a parenthesized expression.
                    with_item_kind = WithItemKind::ParenthesizedExpression;
                }
                Expr::Generator(_) if parsed_with_item.used_ambiguous_lpar => {
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
                    items.push(parsed_with_item.item);
                    break;
                }
                _ => {}
            }

            items.push(parsed_with_item.item);

            has_trailing_comma = false;
            if !self.eat(TokenKind::Comma) {
                break;
            }
            has_trailing_comma = true;

            // Update the with item parsing to indicate that we're no longer
            // parsing the first with item, but we haven't yet found the `)` to
            // the corresponding ambiguous `(`.
            state = WithItemParsingState::AmbiguousLparRest;
        }

        // Check if our assumption is incorrect and it's actually a parenthesized
        // expression.
        if self.at(TokenKind::Rpar) {
            if self.peek() == TokenKind::Colon {
                // Here, the parser is at a `)` followed by a `:`.
                match items.as_slice() {
                    // No with items, treat it as a parenthesized expression to
                    // create an empty tuple expression.
                    [] => with_item_kind = WithItemKind::ParenthesizedExpression,

                    // If there's only one with item and it's a starred expression,
                    // then it's only allowed if there's a trailing comma which makes
                    // it a tuple expression. A bare starred expression is not allowed.
                    // For example:
                    //
                    // ```python
                    // # Syntax error
                    // with (*item): ...
                    //
                    // # Tuple expression
                    // with (*item,): ...
                    // ```
                    [item] if item.context_expr.is_starred_expr() => {
                        if !has_trailing_comma {
                            self.add_error(
                                ParseErrorType::OtherError(
                                    "cannot use starred expression here".to_string(),
                                ),
                                item.range(),
                            );
                        }
                        with_item_kind = WithItemKind::ParenthesizedExpression;
                    }

                    // If there are multiple items, then our assumption is correct.
                    // For example, `with (item1, item2): ...`
                    _ => {}
                }
            } else {
                // For any other token followed by `)`, if either of the items has
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
                if items.iter().all(|item| item.optional_vars.is_none()) {
                    with_item_kind = WithItemKind::ParenthesizedExpression;
                }
            }
        }

        // Transform the items if it's a parenthesized expression.
        if with_item_kind.is_parenthesized_expression() {
            // The generator expression has already consumed the `)`, so avoid
            // expecting it again.
            if with_item_kind != WithItemKind::SingleParenthesizedGeneratorExpression {
                self.expect(TokenKind::Rpar);
            }

            let lhs = if items.len() == 1 && !has_trailing_comma {
                // SAFETY: We've checked that `items` has only one item.
                items.pop().unwrap().context_expr
            } else {
                Expr::Tuple(ast::ExprTuple {
                    range: self.node_range(start),
                    elts: items
                        .drain(..)
                        .map(|item| item.context_expr)
                        .collect::<Vec<_>>(),
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
            let context_expr = if self.is_current_token_postfix() {
                self.parse_postfix_expression(lhs, start)
            } else {
                lhs
            };

            let optional_vars = self
                .at(TokenKind::As)
                .then(|| Box::new(self.parse_with_item_optional_vars().expr));

            items.push(ast::WithItem {
                range: self.node_range(start),
                context_expr,
                optional_vars,
            });
        }

        with_item_kind
    }

    /// Parses a single `with` item.
    ///
    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-with_item>
    fn parse_with_item(&mut self, state: WithItemParsingState) -> ParsedWithItem {
        let start = self.node_start();

        let parsed_expr = self.parse_conditional_expression_or_higher();
        let mut used_ambiguous_lpar = false;

        // While parsing a with item after an ambiguous `(` token, we need to check
        // for any additional expressions that can be parsed as the above parse function
        // doesn't do that.
        let context_expr = if state.is_ambiguous_lpar() {
            match self.current_token_kind() {
                // Named expressions can come at any position after the ambiguous `(` token.
                // For example:
                //
                // ```python
                // # Only item
                // with (item := 10): ...
                //
                // # Multiple items
                // with (item1, item2 := 10): ...
                // ```
                TokenKind::ColonEqual => {
                    let named_expr = self.parse_named_expression(parsed_expr.expr, start);

                    // For example: `with (item := 10 as foo): ...`
                    if self.at(TokenKind::As) {
                        self.add_error(
                            ParseErrorType::OtherError(
                                "unparenthesized named expression cannot be used here".to_string(),
                            ),
                            named_expr.range(),
                        );
                    }

                    Expr::Named(named_expr).into()
                }
                TokenKind::Async | TokenKind::For => {
                    let generator_expr =
                        if let WithItemParsingState::AmbiguousLparFirstItem(lpar_start) = state {
                            // The parser is at the first with item after the ambiguous `(` token.
                            // For example:
                            //
                            // ```python
                            // with (x for x in range(10)): ...
                            // with (x for x in range(10)), item: ...
                            // ```
                            used_ambiguous_lpar = true;
                            self.parse_generator_expression(parsed_expr.expr, lpar_start, true)
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
                            self.parse_generator_expression(parsed_expr.expr, start, false)
                        };

                    if !generator_expr.parenthesized {
                        self.add_error(
                            ParseErrorType::OtherError(
                                "unparenthesized generator expression cannot be used here"
                                    .to_string(),
                            ),
                            generator_expr.range(),
                        );
                    }

                    Expr::Generator(generator_expr).into()
                }
                _ => parsed_expr,
            }
        } else {
            parsed_expr
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

        let mut target = self.parse_conditional_expression_or_higher();

        // This has the same semantics as an assignment target.
        if !helpers::is_valid_assignment_target(&target.expr) {
            self.add_error(ParseErrorType::InvalidAssignmentTarget, target.range());
        }

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
        let subject = self.parse_named_expression_or_higher();
        let subject = if self.at(TokenKind::Comma) {
            let tuple = self.parse_tuple_expression(
                subject.expr,
                subject_start,
                TupleParenthesized::No,
                Parser::parse_named_expression_or_higher,
            );

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

        let guard = self
            .eat(TokenKind::If)
            .then(|| Box::new(self.parse_named_expression_or_higher().expr));

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

            let parsed_expr = self.parse_named_expression_or_higher();
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
            TokenKind::Async if self.peek_nth(1) == TokenKind::Def => {
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
            Some(Box::new(self.parse_conditional_expression_or_higher().expr))
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

        let default = self
            .eat(TokenKind::Equal)
            .then(|| Box::new(self.parse_conditional_expression_or_higher().expr));

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

        if self.eat(TokenKind::Star) {
            let name = self.parse_identifier();
            ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple {
                range: self.node_range(start),
                name,
            })
        } else if self.eat(TokenKind::DoubleStar) {
            let name = self.parse_identifier();
            ast::TypeParam::ParamSpec(ast::TypeParamParamSpec {
                range: self.node_range(start),
                name,
            })
        } else {
            let name = self.parse_identifier();
            let bound = self
                .eat(TokenKind::Colon)
                .then(|| Box::new(self.parse_conditional_expression_or_higher().expr));

            ast::TypeParam::TypeVar(ast::TypeParamTypeVar {
                range: self.node_range(start),
                name,
                bound,
            })
        }
    }

    fn parse_dotted_name(&mut self) -> ast::Identifier {
        let start = self.node_start();

        self.parse_identifier();

        let mut progress = ParserProgress::default();
        while self.eat(TokenKind::Dot) {
            progress.assert_progressing(self);

            let id = self.parse_identifier();
            if !id.is_valid() {
                self.add_error(
                    ParseErrorType::OtherError("invalid identifier".into()),
                    id.range,
                );
            }
        }

        let range = self.node_range(start);

        ast::Identifier {
            id: self.src_text(range).into(),
            range,
        }
    }

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
        let asname = self.eat(TokenKind::As).then(|| self.parse_identifier());

        ast::Alias {
            range: self.node_range(start),
            name,
            asname,
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
