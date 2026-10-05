//! Shared module, suite, and statement inference ordering.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::Statement;

use super::TypeInferenceBuilder;
use crate::types::bool::BoolError;
use crate::types::{Truthiness, Type, TypeContext};

pub(super) struct OrdinaryStatementEffects;
pub(super) struct StatementFacts;

pub(super) enum StatementWork<'stmt> {
    Suite(&'stmt [ast::Stmt], usize),
    MaybeStandalone(&'stmt ast::Stmt),
    Statement(&'stmt ast::Stmt),
    If(&'stmt ast::StmtIf),
    Clauses(&'stmt [ast::ElifElseClause], usize),
    UnusedAwaitable(&'stmt ast::Expr),
}

shared_semantic_family! {
    #[synchronous(SynchronousStatementEffects)]
    pub(super) trait StatementEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn infer_body(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, suite: &[ast::Stmt]) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn traverse(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, initial: StatementWork<'_>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn frames<'stmt>(&self) -> Result<Vec<StatementWork<'stmt>>, Self::Error>;
        #[operation(local)]
        async fn push<'stmt>(&self, frames: &mut Vec<StatementWork<'stmt>>, work: StatementWork<'stmt>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn pop<'stmt>(&self, frames: &mut Vec<StatementWork<'stmt>>) -> Result<Option<StatementWork<'stmt>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_statement<'suite>(&self, suite: &'suite [ast::Stmt], cursor: &mut usize) -> Result<Option<&'suite ast::Stmt>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_clause<'stmt>(&self, clauses: &'stmt [ast::ElifElseClause], cursor: &mut usize) -> Result<Option<&'stmt ast::ElifElseClause>, Self::Error>;
        #[operation(local)]
        async fn standalone_statement(&self, builder: &TypeInferenceBuilder<'db, 'ast>, statement: &ast::Stmt) -> Result<Option<Statement<'db>>, Self::Error>;
        #[operation(child)]
        async fn infer_standalone_statement(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: Statement<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_condition(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn standalone_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, tcx: TypeContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn try_bool(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Result<Truthiness, BoolError<'db>>, Self::Error>;
        #[operation(source)]
        async fn report_bool_error(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, error: &BoolError<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn check_unused_awaitable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn check_redundant_conditions(&self, builder: &TypeInferenceBuilder<'db, 'ast>, suite: &[ast::Stmt]) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_function_definition(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, function: &ast::StmtFunctionDef) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_class_definition(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, class: &ast::StmtClassDef) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, tcx: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_if(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtIf) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_try(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtTry) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_with(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtWith) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_match(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtMatch) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_assignment(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtAssign) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_annotated_assignment(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtAnnAssign) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_augmented_assignment(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtAugAssign) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_type_alias(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtTypeAlias) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_for(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtFor) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_while(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtWhile) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_import(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtImport) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_import_from(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtImportFrom) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_assert(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtAssert) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_raise(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtRaise) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_return(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtReturn) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_delete(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtDelete) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn infer_global(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, statement: &ast::StmtGlobal) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl StatementFacts {
        fn module_body<'ast>(&self, module: &'ast ast::ModModule) -> &'ast [ast::Stmt] {
            &module.body
        }
        fn default_context<'db>(&self) -> TypeContext<'db> {
            TypeContext::default()
        }
        fn if_test<'stmt>(&self, statement: &'stmt ast::StmtIf) -> &'stmt ast::Expr {
            &statement.test
        }
        fn if_body<'stmt>(&self, statement: &'stmt ast::StmtIf) -> &'stmt [ast::Stmt] {
            &statement.body
        }
        fn clauses<'stmt>(&self, statement: &'stmt ast::StmtIf) -> &'stmt [ast::ElifElseClause] {
            &statement.elif_else_clauses
        }
        fn clause_test<'stmt>(&self, clause: &'stmt ast::ElifElseClause) -> Option<&'stmt ast::Expr> {
            clause.test.as_ref()
        }
        fn clause_body<'stmt>(&self, clause: &'stmt ast::ElifElseClause) -> &'stmt [ast::Stmt] {
            &clause.body
        }
    }

    #[synchronous(infer_module_sync)]
    #[capabilities(effects = StatementEffects, facts = StatementFacts)]
    #[passive_values()]
    pub(super) async fn infer_module_with<'db, 'ast, E: StatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        module: &ast::ModModule,
        facts: StatementFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.infer_body(builder, facts.module_body(module)).await
    }

    #[synchronous(infer_body_sync)]
    #[capabilities(effects = StatementEffects)]
    #[passive_values(StatementWork::Suite)]
    pub(super) async fn infer_body_with<'db, 'ast, E: StatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        suite: &[ast::Stmt],
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.traverse(builder, StatementWork::Suite(suite, 0)).await
    }

    #[synchronous(infer_statement_sync)]
    #[capabilities(effects = StatementEffects)]
    #[passive_values(StatementWork::Statement)]
    pub(super) async fn infer_statement_with<'db, 'ast, E: StatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::Stmt,
        effects: &E,
    ) -> Result<(), E::Error> {
        if let ast::Stmt::If(statement) = statement {
            effects.infer_if(builder, statement).await
        } else {
            effects.traverse(builder, StatementWork::Statement(statement)).await
        }
    }

    #[synchronous(infer_if_sync)]
    #[capabilities(effects = StatementEffects)]
    #[passive_values(StatementWork::If)]
    pub(super) async fn infer_if_with<'db, 'ast, E: StatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtIf,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.traverse(builder, StatementWork::If(statement)).await
    }

    #[synchronous(infer_condition_sync)]
    #[capabilities(effects = StatementEffects, facts = StatementFacts)]
    #[passive_values()]
    pub(super) async fn infer_condition_with<'db, 'ast, E: StatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        facts: StatementFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let ty = effects.standalone_expression(builder, expression, facts.default_context()).await?;
        if let Err(error) = effects.try_bool(builder, ty).await? {
            effects.report_bool_error(builder, expression, &error).await?;
        }
        Ok(())
    }

    #[synchronous(traverse_sync)]
    #[capabilities(effects = StatementEffects, facts = StatementFacts)]
    #[passive_values(StatementWork::Suite, StatementWork::MaybeStandalone, StatementWork::Statement, StatementWork::If, StatementWork::Clauses, StatementWork::UnusedAwaitable)]
    pub(super) async fn traverse_with<'db, 'ast, E: StatementEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        initial: StatementWork<'_>,
        facts: StatementFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let mut frames = effects.frames().await?;
        effects.push(&mut frames, initial).await?;
        #[cursor_loop]
        while let Some(work) = effects.pop(&mut frames).await? {
            match work {
                StatementWork::Suite(suite, mut cursor) => {
                    if let Some(statement) = effects.next_statement(suite, &mut cursor).await? {
                        effects.push(&mut frames, StatementWork::Suite(suite, cursor)).await?;
                        if let ast::Stmt::Expr(ast::StmtExpr { value, .. }) = statement {
                            effects.push(&mut frames, StatementWork::UnusedAwaitable(value)).await?;
                        }
                        effects.push(&mut frames, StatementWork::MaybeStandalone(statement)).await?;
                    } else {
                        effects.check_redundant_conditions(builder, suite).await?;
                    }
                }
                StatementWork::MaybeStandalone(statement) => {
                    if let Some(standalone) = effects.standalone_statement(builder, statement).await? {
                        effects.infer_standalone_statement(builder, standalone).await?;
                    } else {
                        effects.push(&mut frames, StatementWork::Statement(statement)).await?;
                    }
                }
                StatementWork::If(statement) => {
                    effects.infer_condition(builder, facts.if_test(statement)).await?;
                    effects.push(&mut frames, StatementWork::Clauses(facts.clauses(statement), 0)).await?;
                    effects.push(&mut frames, StatementWork::Suite(facts.if_body(statement), 0)).await?;
                }
                StatementWork::Clauses(clauses, mut cursor) => {
                    if let Some(clause) = effects.next_clause(clauses, &mut cursor).await? {
                        if let Some(test) = facts.clause_test(clause) {
                            effects.infer_condition(builder, test).await?;
                        }
                        effects.push(&mut frames, StatementWork::Clauses(clauses, cursor)).await?;
                        effects.push(&mut frames, StatementWork::Suite(facts.clause_body(clause), 0)).await?;
                    }
                }
                StatementWork::UnusedAwaitable(expression) => effects.check_unused_awaitable(builder, expression).await?,
                StatementWork::Statement(statement) => {
        match statement {
            ast::Stmt::FunctionDef(function) => effects.infer_function_definition(builder, function).await?,
            ast::Stmt::ClassDef(class) => effects.infer_class_definition(builder, class).await?,
            ast::Stmt::Expr(ast::StmtExpr {
                range: _,
                node_index: _,
                value,
            }) => {
                // If this is a call expression, we would have added an `IsNonTerminalCall`
                // constraint, meaning this will be a standalone expression.
                effects.infer_expression(builder, value, facts.default_context()).await?;
            }
            ast::Stmt::If(if_statement) => effects.push(&mut frames, StatementWork::If(if_statement)).await?,
            ast::Stmt::Try(try_statement) => effects.infer_try(builder, try_statement).await?,
            ast::Stmt::With(with_statement) => effects.infer_with(builder, with_statement).await?,
            ast::Stmt::Match(match_statement) => effects.infer_match(builder, match_statement).await?,
            ast::Stmt::Assign(assign) => effects.infer_assignment(builder, assign).await?,
            ast::Stmt::AnnAssign(assign) => effects.infer_annotated_assignment(builder, assign).await?,
            ast::Stmt::AugAssign(aug_assign) => effects.infer_augmented_assignment(builder, aug_assign).await?,
            ast::Stmt::TypeAlias(type_statement) => effects.infer_type_alias(builder, type_statement).await?,
            ast::Stmt::For(for_statement) => effects.infer_for(builder, for_statement).await?,
            ast::Stmt::While(while_statement) => effects.infer_while(builder, while_statement).await?,
            ast::Stmt::Import(import) => effects.infer_import(builder, import).await?,
            ast::Stmt::ImportFrom(import) => effects.infer_import_from(builder, import).await?,
            ast::Stmt::Assert(assert_statement) => effects.infer_assert(builder, assert_statement).await?,
            ast::Stmt::Raise(raise) => effects.infer_raise(builder, raise).await?,
            ast::Stmt::Return(ret) => effects.infer_return(builder, ret).await?,
            ast::Stmt::Delete(delete) => effects.infer_delete(builder, delete).await?,
            ast::Stmt::Global(global) => effects.infer_global(builder, global).await?,
            ast::Stmt::Nonlocal(_)
            | ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::IpyEscapeCommand(_) => {
                // No-op
            }
        }
                }
            }
        }
        Ok(())
    }
}

impl<'db, 'ast> SynchronousStatementEffects<'db, 'ast> for OrdinaryStatementEffects {
    type Error = Infallible;

    fn traverse(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        initial: StatementWork<'_>,
    ) -> Result<(), Self::Error> {
        traverse_sync(builder, initial, StatementFacts, self)
    }

    fn frames<'stmt>(&self) -> Result<Vec<StatementWork<'stmt>>, Self::Error> {
        Ok(Vec::new())
    }

    fn push<'stmt>(
        &self,
        frames: &mut Vec<StatementWork<'stmt>>,
        work: StatementWork<'stmt>,
    ) -> Result<(), Self::Error> {
        frames.push(work);
        Ok(())
    }

    fn pop<'stmt>(
        &self,
        frames: &mut Vec<StatementWork<'stmt>>,
    ) -> Result<Option<StatementWork<'stmt>>, Self::Error> {
        Ok(frames.pop())
    }

    fn infer_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        suite: &[ast::Stmt],
    ) -> Result<(), Self::Error> {
        builder.infer_body(suite);
        Ok(())
    }

    fn next_statement<'suite>(
        &self,
        suite: &'suite [ast::Stmt],
        cursor: &mut usize,
    ) -> Result<Option<&'suite ast::Stmt>, Self::Error> {
        let next = suite.get(*cursor);
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn next_clause<'stmt>(
        &self,
        clauses: &'stmt [ast::ElifElseClause],
        cursor: &mut usize,
    ) -> Result<Option<&'stmt ast::ElifElseClause>, Self::Error> {
        let next = clauses.get(*cursor);
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn standalone_statement(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::Stmt,
    ) -> Result<Option<Statement<'db>>, Self::Error> {
        Ok(builder.index.try_statement(statement))
    }

    fn infer_standalone_statement(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: Statement<'db>,
    ) -> Result<(), Self::Error> {
        builder.infer_standalone_statement_impl(statement);
        Ok(())
    }

    fn infer_condition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), Self::Error> {
        infer_condition_sync(builder, expression, StatementFacts, self)
    }

    fn standalone_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.infer_standalone_expression(expression, tcx))
    }

    fn try_bool(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Result<Truthiness, BoolError<'db>>, Self::Error> {
        Ok(ty.try_bool(builder.db(), builder.program_environment()))
    }

    fn report_bool_error(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        error: &BoolError<'db>,
    ) -> Result<(), Self::Error> {
        error.report_diagnostic(&builder.context, expression);
        Ok(())
    }

    fn check_unused_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), Self::Error> {
        builder.check_unused_awaitable(expression);
        Ok(())
    }

    fn check_redundant_conditions(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        suite: &[ast::Stmt],
    ) -> Result<(), Self::Error> {
        builder.check_suite_for_redundant_conditions(suite);
        Ok(())
    }

    fn infer_function_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &ast::StmtFunctionDef,
    ) -> Result<(), Self::Error> {
        builder.infer_function_definition_statement(function);
        Ok(())
    }

    fn infer_class_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        class: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        builder.infer_class_definition_statement(class);
        Ok(())
    }

    fn infer_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<(), Self::Error> {
        builder.infer_maybe_standalone_expression(expression, tcx);
        Ok(())
    }

    fn infer_if(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtIf,
    ) -> Result<(), Self::Error> {
        builder.infer_if_statement(statement);
        Ok(())
    }

    fn infer_try(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtTry,
    ) -> Result<(), Self::Error> {
        builder.infer_try_statement(statement);
        Ok(())
    }

    fn infer_with(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtWith,
    ) -> Result<(), Self::Error> {
        builder.infer_with_statement(statement);
        Ok(())
    }

    fn infer_match(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtMatch,
    ) -> Result<(), Self::Error> {
        builder.infer_match_statement(statement);
        Ok(())
    }

    fn infer_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtAssign,
    ) -> Result<(), Self::Error> {
        builder.infer_assignment_statement(statement);
        Ok(())
    }

    fn infer_annotated_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtAnnAssign,
    ) -> Result<(), Self::Error> {
        builder.infer_annotated_assignment_statement(statement);
        Ok(())
    }

    fn infer_augmented_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtAugAssign,
    ) -> Result<(), Self::Error> {
        builder.infer_augmented_assignment_statement(statement);
        Ok(())
    }

    fn infer_type_alias(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtTypeAlias,
    ) -> Result<(), Self::Error> {
        builder.infer_type_alias_statement(statement);
        Ok(())
    }

    fn infer_for(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtFor,
    ) -> Result<(), Self::Error> {
        builder.infer_for_statement(statement);
        Ok(())
    }

    fn infer_while(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtWhile,
    ) -> Result<(), Self::Error> {
        builder.infer_while_statement(statement);
        Ok(())
    }

    fn infer_import(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtImport,
    ) -> Result<(), Self::Error> {
        builder.infer_import_statement(statement);
        Ok(())
    }

    fn infer_import_from(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtImportFrom,
    ) -> Result<(), Self::Error> {
        builder.infer_import_from_statement(statement);
        Ok(())
    }

    fn infer_assert(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtAssert,
    ) -> Result<(), Self::Error> {
        builder.infer_assert_statement(statement);
        Ok(())
    }

    fn infer_raise(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtRaise,
    ) -> Result<(), Self::Error> {
        builder.infer_raise_statement(statement);
        Ok(())
    }

    fn infer_return(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtReturn,
    ) -> Result<(), Self::Error> {
        builder.infer_return_statement(statement);
        Ok(())
    }

    fn infer_delete(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtDelete,
    ) -> Result<(), Self::Error> {
        builder.infer_delete_statement(statement);
        Ok(())
    }

    fn infer_global(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtGlobal,
    ) -> Result<(), Self::Error> {
        builder.infer_global_statement(statement);
        Ok(())
    }
}
