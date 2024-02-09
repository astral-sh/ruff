use std::fmt::Display;

use ruff_python_ast::{
    self as ast, ExceptHandler, Expr, ExprContext, IpyEscapeKind, Operator, Stmt, WithItem,
};
use ruff_text_size::{Ranged, TextSize};

use crate::parser::expression::ParsedExpr;
use crate::parser::progress::ParserProgress;
use crate::parser::{
    helpers, FunctionKind, Parser, ParserCtxFlags, RecoveryContext, RecoveryContextKind, EXPR_SET,
    LITERAL_SET,
};
use crate::token_set::TokenSet;
use crate::{Mode, ParseErrorType, Tok, TokenKind};

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

impl<'src> Parser<'src> {
    fn at_compound_stmt(&self) -> bool {
        self.at_ts(COMPOUND_STMT_SET)
    }

    fn at_simple_stmt(&self) -> bool {
        self.at_ts(SIMPLE_STMT_SET2)
    }

    pub(super) fn is_at_stmt(&self) -> bool {
        self.at_ts(STMTS_SET)
    }

    /// Parses a compound or a simple statement.
    pub(super) fn parse_statement(&mut self) -> Stmt {
        let start_offset = self.node_start();
        match self.current_kind() {
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
            let range = self.current_range();
            self.add_error(
                ParseErrorType::SimpleStmtsInSameLine,
                stmt.range().cover(range),
            );
        }

        if !has_eaten_newline && self.at_compound_stmt() {
            // Avoid create `SimpleStmtAndCompoundStmtInSameLine` error when the
            // current node is `Expr::Invalid`. Example of when this may happen:
            // ```python
            // ! def x(): ...
            // ```
            // The `!` (an unexpected token) will be parsed as `Expr::Invalid`.
            if let Stmt::Expr(expr) = &stmt {
                #[allow(deprecated)]
                if let Expr::Invalid(_) = expr.value.as_ref() {
                    return stmt;
                }
            }

            self.add_error(
                ParseErrorType::SimpleStmtAndCompoundStmtInSameLine,
                stmt.range().cover(self.current_range()),
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
        match self.current_kind() {
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
                } else if let Ok(op) = Operator::try_from(self.current_kind()) {
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
    /// If the parser isn't positioned at a `del` token.
    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-del_stmt>
    fn parse_delete_statement(&mut self) -> ast::StmtDelete {
        let start = self.node_start();

        self.bump(TokenKind::Del);
        let mut targets = vec![];

        #[allow(deprecated)]
        self.parse_separated(true, TokenKind::Comma, [TokenKind::Newline], |parser| {
            let mut target = parser.parse_conditional_expression_or_higher();
            helpers::set_expr_ctx(&mut target.expr, ExprContext::Del);

            if matches!(target.expr, Expr::BoolOp(_) | Expr::Compare(_)) {
                // Should we make `target` an `Expr::Invalid` here?
                parser.add_error(
                    ParseErrorType::OtherError(format!(
                        "`{}` not allowed in `del` statement",
                        parser.src_text(&target.expr)
                    )),
                    &target.expr,
                );
            }
            targets.push(target.expr);
        });

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

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-import_stmt>
    fn parse_import_statement(&mut self) -> ast::StmtImport {
        let start = self.node_start();
        self.bump(TokenKind::Import);

        let mut names = vec![];
        #[allow(deprecated)]
        self.parse_separated(false, TokenKind::Comma, [TokenKind::Newline], |parser| {
            names.push(parser.parse_alias());
        });

        ast::StmtImport {
            range: self.node_range(start),
            names,
        }
    }

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
            let range = self.current_range();
            self.add_error(
                ParseErrorType::OtherError("missing module name".to_string()),
                range,
            );
        }

        self.expect(TokenKind::Import);

        let parenthesized = self.eat(TokenKind::Lpar);
        let names =
            self.parse_delimited_list(RecoveryContextKind::ImportNames, |p| p.parse_alias(), true);

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

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-global_stmt>
    fn parse_global_statement(&mut self) -> ast::StmtGlobal {
        let start = self.node_start();
        self.bump(TokenKind::Global);

        let mut names = vec![];
        #[allow(deprecated)]
        self.parse_separated(false, TokenKind::Comma, [TokenKind::Newline], |parser| {
            names.push(parser.parse_identifier());
        });

        ast::StmtGlobal {
            range: self.node_range(start),
            names,
        }
    }

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-nonlocal_stmt>
    fn parse_nonlocal_statement(&mut self) -> ast::StmtNonlocal {
        let start = self.node_start();
        self.bump(TokenKind::Nonlocal);

        let mut names = vec![];
        #[allow(deprecated)]
        self.parse_separated(false, TokenKind::Comma, [TokenKind::Newline], |parser| {
            names.push(parser.parse_identifier());
        });

        ast::StmtNonlocal {
            range: self.node_range(start),
            names,
        }
    }

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
            self.add_error(
                ParseErrorType::OtherError(format!("expecting identifier, got {tok}")),
                tok_range,
            );
            #[allow(deprecated)]
            Expr::Invalid(ast::ExprInvalid {
                value: self.src_text(tok_range).into(),
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
            self.parse_sequence(RecoveryContextKind::AssignmentTargets, |p| {
                p.bump(TokenKind::Equal);

                let mut parsed_expr = p.parse_expression();

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
                .for_each(|target| self.add_error(ParseErrorType::AssignmentError, target.range()));
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
            self.add_error(ParseErrorType::AssignmentError, target.range());
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

    /// See: <https://docs.python.org/3/reference/simple_stmts.html#grammar-token-python-grammar-augmented_assignment_stmt>
    fn parse_augmented_assignment_statement(
        &mut self,
        mut target: ParsedExpr,
        op: Operator,
        start: TextSize,
    ) -> ast::StmtAugAssign {
        // Consume the operator
        // FIXME(micha): assert that it is an augmented assign token
        self.next_token();

        if !helpers::is_valid_aug_assignment_target(&target.expr) {
            self.add_error(ParseErrorType::AugAssignmentError, target.range());
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
            let range = self.current_range();
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
                match self.peek_nth(index) {
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
                prev_token = self.peek_nth(index);
            }
        }

        if !treat_it_as_expr && has_seen_lpar {
            self.bump(TokenKind::Lpar);
        }

        let ending = if has_seen_lpar && treat_it_as_expr {
            [TokenKind::Colon]
        } else {
            [TokenKind::Rpar]
        };

        let mut is_last_parenthesized = false;
        #[allow(deprecated)]
        self.parse_separated(
            // Only allow a trailing delimiter if we've seen a `(`.
            has_seen_lpar,
            TokenKind::Comma,
            ending,
            |parser| {
                let parsed_with_item = parser.parse_with_item();
                is_last_parenthesized = parsed_with_item.is_parenthesized;
                items.push(parsed_with_item.item);
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
        if let [with_item] = items.as_mut_slice() {
            if treat_it_as_expr
                && with_item.optional_vars.is_none()
                && is_last_parenthesized
                && !matches!(with_item.context_expr, Expr::Tuple(_))
            {
                with_item.range = with_item.range.add_start(1.into()).sub_end(1.into());
            }
        }

        if !treat_it_as_expr && has_seen_lpar {
            self.expect(TokenKind::Rpar);
        }

        items
    }

    fn parse_with_item(&mut self) -> ParsedWithItem {
        let start = self.node_start();

        let context_expr = self.parse_conditional_expression_or_higher();
        match context_expr.expr {
            Expr::Starred(_) => {
                self.add_error(
                    ParseErrorType::OtherError("starred expression not allowed".into()),
                    context_expr.range(),
                );
            }
            Expr::NamedExpr(_) if !context_expr.is_parenthesized => {
                self.add_error(
                    ParseErrorType::OtherError(
                        "unparenthesized named expression not allowed".into(),
                    ),
                    context_expr.range(),
                );
            }
            _ => {}
        }

        let optional_vars = if self.eat(TokenKind::As) {
            let mut target = self.parse_conditional_expression_or_higher();

            if matches!(target.expr, Expr::BoolOp(_) | Expr::Compare(_)) {
                // Should we make `target` an `Expr::Invalid` here?
                self.add_error(
                    ParseErrorType::OtherError(
                        "expression not allowed in `with` statement".to_string(),
                    ),
                    target.range(),
                );
            }

            helpers::set_expr_ctx(&mut target.expr, ExprContext::Store);

            Some(Box::new(target.expr))
        } else {
            None
        };

        ParsedWithItem {
            is_parenthesized: context_expr.is_parenthesized,
            item: ast::WithItem {
                range: self.node_range(start),
                context_expr: context_expr.expr,
                optional_vars,
            },
        }
    }

    /// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-match_stmt>
    fn parse_match_statement(&mut self) -> ast::StmtMatch {
        let start_offset = self.node_start();

        self.bump(TokenKind::Match);

        let subject_start = self.node_start();
        let subject = self.parse_named_expression_or_higher();
        let subject = if self.at(TokenKind::Comma) {
            let tuple = self.parse_tuple_expression(
                subject.expr,
                subject_start,
                false,
                Parser::parse_named_expression_or_higher,
            );

            Expr::Tuple(tuple).into()
        } else {
            subject
        };

        self.expect(TokenKind::Colon);

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

        let cases = self.parse_match_cases();

        self.eat(TokenKind::Dedent);

        ast::StmtMatch {
            subject: Box::new(subject.expr),
            cases,
            range: self.node_range(start_offset),
        }
    }

    fn parse_match_cases(&mut self) -> Vec<ast::MatchCase> {
        if !self.at(TokenKind::Case) {
            self.add_error(
                ParseErrorType::OtherError("expecting `case` block after `match`".to_string()),
                self.current_range(),
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

        match self.current_kind() {
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
                self.add_error(ParseErrorType::StmtIsNotAsync(kind), self.current_range());
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

        match self.current_kind() {
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
                    self.current_range(),
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
            self.current_range(),
        );

        Vec::new()
    }

    fn parse_block(&mut self) -> Vec<Stmt> {
        self.bump(TokenKind::Indent);

        let statements =
            self.parse_list(RecoveryContextKind::BlockStatements, Self::parse_statement);

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

    pub(super) fn parse_parameters(&mut self, function_kind: FunctionKind) -> ast::Parameters {
        let mut args = vec![];
        let mut posonlyargs = vec![];
        let mut kwonlyargs = vec![];
        let mut kwarg = None;
        let mut vararg = None;

        let mut has_seen_asterisk = false;
        let mut has_seen_vararg = false;
        let mut has_seen_default_param = false;

        let ending = match function_kind {
            FunctionKind::Lambda => TokenKind::Colon,
            FunctionKind::FunctionDef => TokenKind::Rpar,
        };

        let ending_set = TokenSet::new([TokenKind::Rarrow, ending]).union(COMPOUND_STMT_SET);
        let start = self.node_start();

        #[allow(deprecated)]
        self.parse_separated(true, TokenKind::Comma, ending_set, |parser| {
            // Don't allow any parameter after we have seen a vararg `**kwargs`
            if has_seen_vararg {
                parser.add_error(
                    ParseErrorType::ParamFollowsVarKeywordParam,
                    parser.current_range(),
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
                        parser.current_range(),
                    );
                }
                std::mem::swap(&mut args, &mut posonlyargs);
            } else if parser.at(TokenKind::Name) {
                let param = parser.parse_parameter_with_default(function_kind);
                // Don't allow non-default parameters after default parameters e.g. `a=1, b`,
                // can't place `b` after `a=1`. Non-default parameters are only allowed after
                // default parameters if we have a `*` before them, e.g. `a=1, *, b`.
                if param.default.is_none() && has_seen_default_param && !has_seen_asterisk {
                    parser.add_error(ParseErrorType::DefaultArgumentError, parser.current_range());
                }
                has_seen_default_param = param.default.is_some();

                if has_seen_asterisk {
                    kwonlyargs.push(param);
                } else {
                    args.push(param);
                }
            } else {
                if parser.at_ts(SIMPLE_STMT_SET) {
                    return;
                }

                let range = parser.current_range();
                #[allow(deprecated)]
                parser.skip_until(
                    ending_set.union(TokenSet::new([TokenKind::Comma, TokenKind::Colon])),
                );
                parser.add_error(
                    ParseErrorType::OtherError("expected parameter".to_string()),
                    range.cover(parser.current_range()), // TODO(micha): This goes one token too far?
                );
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

    fn parse_type_params(&mut self) -> ast::TypeParams {
        let start = self.node_start();

        self.bump(TokenKind::Lsqb);

        let type_params = self.parse_delimited_list(
            RecoveryContextKind::TypeParams,
            |p| p.parse_type_param(),
            true,
        );

        self.expect(TokenKind::Rsqb);

        ast::TypeParams {
            range: self.node_range(start),
            type_params,
        }
    }

    fn try_parse_type_params(&mut self) -> Option<ast::TypeParams> {
        self.at(TokenKind::Lsqb).then(|| self.parse_type_params())
    }

    pub(super) fn is_at_type_param(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Star | TokenKind::DoubleStar | TokenKind::Name
        ) || self.current_kind().is_keyword()
    }

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

struct ParsedWithItem {
    item: WithItem,
    is_parenthesized: bool,
}
