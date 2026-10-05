//! Statement traversal uses the same canonical children as ordinary inference.

use ruff_python_ast as ast;
#[cfg(test)]
use ruff_text_size::Ranged;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::Statement;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::bool::BoolError;
use crate::types::infer::builder::source_statement::{
    self, StatementEffects, StatementFacts, StatementWork,
};
use crate::types::infer::builder::{TypeInferenceBuilder, local};
#[cfg(test)]
use crate::types::infer::source_runtime::tests::statement_traversal as observations;
use crate::types::{Truthiness, Type, TypeContext};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn infer_module_source(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<()> {
        let module = builder.module().syntax();
        source_statement::infer_module_with(builder, module, StatementFacts, self).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> StatementEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn traverse(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        initial: StatementWork<'_>,
    ) -> RunResult<()> {
        #[cfg(test)]
        let _observer = self
            .local(1, 0, || observations::TraversalGuard::new(self.db()))
            .await?;
        self.allocate_future(|| {
            source_statement::traverse_with(builder, initial, StatementFacts, self)
        })
        .await?
        .await
    }

    async fn frames<'stmt>(&self) -> RunResult<Vec<StatementWork<'stmt>>> {
        self.local(1, 0, Vec::new).await
    }

    async fn push<'stmt>(
        &self,
        frames: &mut Vec<StatementWork<'stmt>>,
        work: StatementWork<'stmt>,
    ) -> RunResult<()> {
        let growth = frames.len() == frames.capacity();
        let bytes = if growth {
            Self::checked(
                frames
                    .len()
                    .checked_add(1)
                    .and_then(|count| count.checked_mul(size_of::<StatementWork<'stmt>>())),
            )?
        } else {
            0
        };
        let units = Self::checked(if growth {
            frames.len().checked_add(3)
        } else {
            Some(3)
        })?;
        let mut work = Some(work);
        self.local(units, bytes, || {
            if growth {
                frames.reserve_exact(1);
            }
            frames.extend(work.take());
            #[cfg(test)]
            observations::frame_depth(self.db(), frames.len());
        })
        .await
    }

    async fn pop<'stmt>(
        &self,
        frames: &mut Vec<StatementWork<'stmt>>,
    ) -> RunResult<Option<StatementWork<'stmt>>> {
        self.local(1, 0, || frames.pop()).await
    }

    async fn infer_body(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        suite: &[ast::Stmt],
    ) -> RunResult<()> {
        source_statement::infer_body_with(builder, suite, self).await
    }

    async fn next_statement<'suite>(
        &self,
        suite: &'suite [ast::Stmt],
        cursor: &mut usize,
    ) -> RunResult<Option<&'suite ast::Stmt>> {
        self.local(1, 0, || {
            let next = suite.get(*cursor);
            if next.is_some() {
                *cursor = Self::checked(cursor.checked_add(1))?;
            }
            #[cfg(test)]
            if let Some(statement) = next {
                observations::statement_entered(self.db(), statement.range());
            }
            Ok(next)
        })
        .await?
    }

    async fn next_clause<'stmt>(
        &self,
        clauses: &'stmt [ast::ElifElseClause],
        cursor: &mut usize,
    ) -> RunResult<Option<&'stmt ast::ElifElseClause>> {
        self.local(1, 0, || {
            let next = clauses.get(*cursor);
            if next.is_some() {
                *cursor = Self::checked(cursor.checked_add(1))?;
            }
            Ok(next)
        })
        .await?
    }

    async fn standalone_statement(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::Stmt,
    ) -> RunResult<Option<Statement<'db>>> {
        self.local(1, 0, || builder.index.try_statement(statement))
            .await
    }

    async fn infer_standalone_statement(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: Statement<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementQuery).await
    }

    async fn infer_condition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<()> {
        self.local(1, 0, || {
            #[cfg(test)]
            observations::condition_entered(self.db(), expression.range());
        })
        .await?;
        source_statement::infer_condition_with(builder, expression, StatementFacts, self).await?;
        self.local(1, 0, || {
            #[cfg(test)]
            observations::condition_completed(self.db(), expression.range());
        })
        .await
    }

    async fn standalone_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        context: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        local::source::standalone_expression(builder, expression, context, self).await
    }

    async fn try_bool(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> RunResult<Result<Truthiness, BoolError<'db>>> {
        self.try_type_truthiness(builder.program_environment(), ty)
            .await
    }

    async fn report_bool_error(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _error: &BoolError<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementConditionDiagnostic)
            .await
    }

    async fn check_unused_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<()> {
        self.check_unused_awaitable_source(builder, expression)
            .await
    }

    async fn check_redundant_conditions(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        suite: &[ast::Stmt],
    ) -> RunResult<()> {
        self.local(1, 0, || {
            #[cfg(test)]
            observations::suite_postcheck(self.db(), suite);
        })
        .await?;
        self.check_redundant_conditions_source(builder, suite).await
    }

    async fn infer_function_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        function: &ast::StmtFunctionDef,
    ) -> RunResult<()> {
        let definition = self
            .local(1, 0, || builder.index.expect_single_definition(function))
            .await?;
        let inferred = self.access.definition(definition).await?;
        self.merge_definition(builder, definition, inferred).await
    }

    async fn infer_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        context: TypeContext<'db>,
    ) -> RunResult<()> {
        local::source::maybe_standalone_expression(builder, expression, context, self).await?;
        Ok(())
    }

    async fn infer_class_definition(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtClassDef,
    ) -> RunResult<()> {
        let definition = self
            .local(1, 0, || builder.index.expect_single_definition(statement))
            .await?;
        let inferred = self.access.definition(definition).await?;
        self.merge_definition(builder, definition, inferred).await
    }

    async fn infer_if(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtIf,
    ) -> RunResult<()> {
        source_statement::infer_if_with(builder, statement, self).await
    }

    async fn infer_try(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtTry,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementTry).await
    }

    async fn infer_with(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtWith,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementWith).await
    }

    async fn infer_match(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtMatch,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementMatch).await
    }

    async fn infer_assignment(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtAssign,
    ) -> RunResult<()> {
        self.infer_assignment_statement_source(builder, statement)
            .await
    }

    async fn infer_annotated_assignment(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtAnnAssign,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementAnnotatedAssignment)
            .await
    }

    async fn infer_augmented_assignment(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtAugAssign,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementAugmentedAssignment)
            .await
    }

    async fn infer_type_alias(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtTypeAlias,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementTypeAlias).await
    }

    async fn infer_for(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtFor,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementFor).await
    }

    async fn infer_while(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtWhile,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementWhile).await
    }

    async fn infer_import(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtImport,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementImport).await
    }

    async fn infer_import_from(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtImportFrom,
    ) -> RunResult<()> {
        self.infer_import_from_statement_source(builder, statement)
            .await
    }

    async fn infer_assert(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtAssert,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementAssert).await
    }

    async fn infer_raise(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtRaise,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementRaise).await
    }

    async fn infer_return(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        statement: &ast::StmtReturn,
    ) -> RunResult<()> {
        self.infer_return_source(builder, statement).await
    }

    async fn infer_delete(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtDelete,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementDelete).await
    }

    async fn infer_global(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _statement: &ast::StmtGlobal,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementGlobal).await
    }
}
