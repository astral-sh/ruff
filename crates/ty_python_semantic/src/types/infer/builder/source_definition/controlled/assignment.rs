//! Assignment definitions retain canonical expression results and their binding ownership.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::{BindingsOwner, Definition};
use ty_python_core::expression::Expression;
use ty_python_core::unpack::Unpack;

use super::storage::sequence_merge;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::assignment::statement::{
    AssignmentStatementEffects, AssignmentStatementFacts, infer_assignment_statement_with,
};
use crate::types::infer::builder::assignment::{AssignmentDefinitionEffects, sealed};
use crate::types::infer::builder::local;
use crate::types::{SpecialFormType, Type, TypeContext};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn infer_assignment_statement_source(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        assignment: &ast::StmtAssign,
    ) -> RunResult<()> {
        infer_assignment_statement_with(builder, assignment, AssignmentStatementFacts, self).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AssignmentStatementEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn infer_name(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: &ast::ExprName,
    ) -> RunResult<()> {
        let work = self
            .local(1, 0, || builder.index.definition_lookup_work())
            .await?;
        let definition = self
            .local(work, 0, || builder.index.expect_single_definition(name))
            .await?;
        let inference = self.access.definition(definition).await?;
        self.merge_definition(builder, definition, inference).await
    }

    async fn shared_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        value: &ast::Expr,
    ) -> RunResult<Expression<'db>> {
        let work = self
            .local(1, 0, || builder.index.expression_lookup_work())
            .await?;
        self.local(Self::checked(work)?, 0, || builder.index.expression(value))
            .await
    }

    async fn retain_value_bindings(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: Expression<'db>,
    ) -> RunResult<()> {
        let inference = self
            .canonical_expression(expression, TypeContext::default())
            .await?;
        self.work(1).await?;
        let Some(extra) = &inference.extra else {
            return Ok(());
        };
        let mut quote = sequence_merge::<(Definition<'db>, Type<'db>)>(
            builder.bindings.0.len(),
            builder.bindings.0.capacity(),
            extra.bindings.len(),
        )
        .ok_or(RunError::Contract("assignment bindings quotation overflow"))?;
        // VecMap checks uniqueness against earlier entries in debug builds.
        quote.work = Self::checked(
            builder
                .bindings
                .0
                .len()
                .checked_add(extra.bindings.len())
                .and_then(|n| n.checked_mul(extra.bindings.len()))
                .and_then(|n| quote.work.checked_add(n)),
        )?;
        self.local(quote.work, quote.bytes, || {
            builder.bindings.0.reserve_exact(extra.bindings.len());
            builder.bindings.extend(extra.bindings.iter().copied());
        })
        .await
    }

    async fn next_target<'target>(
        &self,
        targets: &'target [ast::Expr],
        cursor: &mut usize,
    ) -> RunResult<Option<&'target ast::Expr>> {
        self.local(1, 0, || {
            let next = targets.get(*cursor);
            if next.is_some() {
                *cursor += 1;
            }
            next
        })
        .await
    }

    async fn unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
    ) -> RunResult<Option<Unpack<'db>>> {
        let work = self
            .local(1, 0, || builder.index.unpack_lookup_work())
            .await?;
        self.local(work, 0, || builder.index.try_unpack(target))
            .await
    }

    async fn infer_unpacked_target(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: Expression<'db>,
        _unpack: Unpack<'db>,
        _target: &ast::Expr,
        _value: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AssignmentUnpack).await
    }

    async fn infer_target(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: Expression<'db>,
        _target: &ast::Expr,
        _value: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::StatementAssignment).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> AssignmentDefinitionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type BindingEffects = Self;
    type ExpressionEffects = Self;

    fn binding_effects(&self) -> &Self::BindingEffects {
        self
    }

    fn expression_effects(&self) -> &Self::ExpressionEffects {
        self
    }

    async fn checkpoint(&self, builder: &TypeInferenceBuilder<'db, '_>) -> RunResult<()> {
        let path = self
            .field(builder.file().read_fields(self.db()).path())
            .await?;
        self.work(Self::checked(path.as_str().len().checked_add(16))?)
            .await
    }

    async fn unpack(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _unpack: Unpack<'db>,
        _target: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::AssignmentUnpack).await
    }

    async fn standalone_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: Expression<'db>,
        value: &ast::Expr,
        owner: BindingsOwner,
        tcx: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        let inference = self.canonical_expression(expression, tcx).await?;
        match owner {
            BindingsOwner::Definition => builder.extend_expression_with(inference, self).await?,
            BindingsOwner::Statement => {
                builder
                    .extend_expression_unchecked_with(inference, false, self)
                    .await?;
            }
        }
        let work = Self::checked(inference.expressions.iter().len().checked_add(1))?;
        self.local(work, 0, || inference.expression_type(value))
            .await
    }

    async fn local_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        local::source::expression(builder, value, tcx, self).await
    }

    async fn call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        target: &ast::Expr,
        call: &ast::ExprCall,
        definition: Definition<'db>,
        tcx: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        local::source::assignment_call(builder, target, call, definition, tcx, self).await
    }

    async fn invalid_type_checking(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _target: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AssignmentDiagnostic)
            .await
    }

    async fn special_form(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        name: &str,
    ) -> RunResult<Option<SpecialFormType>> {
        let candidates = self
            .local(Self::checked(name.len().checked_add(1))?, 0, || {
                SpecialFormType::candidates_from_name(name)
            })
            .await?;
        if candidates.is_empty() {
            return Ok(None);
        }

        let file = self.local(1, 0, || builder.program_file()).await?;
        self.check_file_program(file).await?;
        let prepared = self.access.prepare_existing(file).await?;
        if prepared.file != file {
            return Err(RunError::Contract("prepared assignment file is foreign"));
        }
        let Some(module) = self.access.known_module(prepared.file).await? else {
            return Ok(None);
        };
        self.local(Self::checked(candidates.len().checked_add(2))?, 0, || {
            candidates
                .iter()
                .find(|candidate| candidate.check_module(module))
                .copied()
        })
        .await
    }
}
