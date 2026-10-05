//! Tuple literals retain the source expression owner while their children and shape are inferred.

use std::borrow::Cow;
use std::{slice, vec};

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::generics::Specialization;
use crate::types::infer::builder::local::tuple_expression::{Phase, Prepared, State};
use crate::types::infer::builder::local::{BuilderId, BuilderStore};
use crate::types::infer::builder::number_literal::NumberLiteralEffects;
use crate::types::infer::builder::source_expression::SourceExpressionOperation;
use crate::types::infer::builder::tuple_expression::{
    TupleExpressionEffects, TupleExpressionFacts, prepare_tuple_expression_with,
};
use crate::types::infer::builder::{TypeInferenceBuilder, local};
use crate::types::tuple::construction::{
    TupleConstructionEffects, fixed_without_variable, tuple_type,
};
use crate::types::tuple::{TupleSpec, TupleType, VariableLengthTuple, VariableSegment};
use crate::types::{KnownClass, Type, TypeContext};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> NumberLiteralEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn value<'expr>(
        &self,
        literal: &'expr ast::ExprNumberLiteral,
    ) -> RunResult<&'expr ast::Number> {
        self.local_with_fixed_transfers(1, 0, || &literal.value)
            .await
    }

    async fn integer(&self, value: &ast::Int) -> RunResult<Option<i64>> {
        self.local_with_fixed_transfers(2, 0, || value.as_i64())
            .await
    }

    async fn instance(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Expression(
            SourceExpressionOperation::ExpressionKind,
        ))
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> TupleExpressionEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn narrow_targets(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: Type<'db>,
    ) -> RunResult<Option<Cow<'db, [Type<'db>]>>> {
        self.contextual_tuple_targets(builder.program_environment(), annotation)
            .await
    }

    async fn setup_cache(&self, _builder: &mut TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn teardown_cache(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }

    async fn specialization(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: Type<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        self.contextual_tuple_specialization(builder.program_environment(), annotation)
            .await
    }

    async fn assignable(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _inferred: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::Expression(
            SourceExpressionOperation::ContextualClassSpecialization,
        ))
        .await
    }
    async fn infer_impl(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        tuple: &ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        local::source::tuple_value(builder, tuple, context, self).await
    }

    async fn filter_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.contextual_tuple_filter_annotation(builder.program_environment(), annotation)
            .await
    }

    async fn annotation_tuple(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db TupleSpec<'db>> {
        self.contextual_tuple_spec(specialization).await
    }

    async fn resize_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        spec: &TupleSpec<'db>,
        length: usize,
    ) -> RunResult<Option<TupleSpec<'db>>> {
        self.resize_tuple_to_fixed(builder.program_environment(), spec, length)
            .await
    }

    async fn elements<'expr>(
        &self,
        tuple: &'expr ast::ExprTuple,
    ) -> RunResult<slice::Iter<'expr, ast::Expr>> {
        self.local_with_fixed_transfers(2, 0, || tuple.elts.iter())
            .await
    }
    async fn next_element<'expr>(
        &self,
        elements: &mut slice::Iter<'expr, ast::Expr>,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local_with_fixed_transfers(2, 0, || elements.next())
            .await
    }
    async fn annotation_elements(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: &TupleSpec<'db>,
    ) -> RunResult<vec::IntoIter<Type<'db>>> {
        self.tuple_annotation_elements(tuple).await
    }

    async fn empty_annotation_elements(&self) -> RunResult<vec::IntoIter<Type<'db>>> {
        // An absent annotation retains no element buffer; include iterator construction and drop.
        self.local_with_fixed_transfers(3, 0, || Vec::new().into_iter())
            .await
    }

    async fn iterable_context(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _element: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Expression(
            SourceExpressionOperation::ContextualClassSpecialization,
        ))
        .await
    }

    async fn needs_promotion(&self, tuple: &ast::ExprTuple) -> RunResult<bool> {
        self.tuple_literal_needs_promotion(&tuple.elts).await
    }
    async fn sequence(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: &ast::ExprTuple,
        promote: bool,
    ) -> RunResult<TupleSpec<'db>> {
        self.sequence_from_literal_elements(builder, &tuple.elts, promote)
            .await
    }
    async fn construct(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        spec: &TupleSpec<'db>,
    ) -> RunResult<Type<'db>> {
        #[cfg(all(test, feature = "experimental-analysis"))]
        crate::types::infer::source_runtime::tests::contextual_tuple::observe_boundary(
            self.db(),
            crate::types::infer::source_runtime::tests::contextual_tuple::Stage::BeforeTupleTransfer,
        );
        let tuple = tuple_type(builder.db(), builder.program_environment(), spec, self).await?;
        #[cfg(all(test, feature = "experimental-analysis"))]
        crate::types::infer::source_runtime::tests::contextual_tuple::observe_boundary(
            self.db(),
            crate::types::infer::source_runtime::tests::contextual_tuple::Stage::AfterTupleTransfer,
        );
        self.local_with_fixed_transfers(1, 0, || Type::tuple(tuple))
            .await
    }
    async fn initialize_targets<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        targets: Option<Cow<'db, [Type<'db>]>>,
    ) -> RunResult<()> {
        let mut targets = Some(targets);
        self.local_with_fixed_transfers(
            4,
            size_of::<Option<Cow<'db, [Type<'db>]>>>() * 2 + size_of::<Phase<'db>>(),
            || {
                let targets = targets.take().flatten();
                state.targets = targets;
            },
        )
        .await
    }
    async fn select_phase<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        phase: Phase<'db>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(2, size_of::<Phase<'db>>() * 2, || state.phase = phase)
            .await
    }
    async fn cache_ready<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        teardown: bool,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(3, size_of::<bool>() + size_of::<Phase<'db>>(), || {
            state.teardown_cache = teardown;
            state.phase = Phase::Select;
        })
        .await
    }
    async fn next_state_target<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(
            5,
            size_of::<Option<Type<'db>>>() + size_of::<usize>(),
            || {
                let target = state
                    .targets
                    .as_deref()
                    .and_then(|targets| targets.get(state.target_index))
                    .copied();
                if target.is_some() {
                    state.target_index += 1;
                }
                target
            },
        )
        .await
    }
    async fn speculate_store(
        &self,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
        _parent: BuilderId,
    ) -> RunResult<BuilderId> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn finish_speculation(
        &self,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
        _parent: BuilderId,
        _child: BuilderId,
        _keep: bool,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn select_context<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        builder: BuilderId,
        context: TypeContext<'db>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(
            4,
            size_of::<BuilderId>() + size_of::<TypeContext<'db>>() + size_of::<Phase<'db>>(),
            || {
                state.active = builder;
                state.context = context;
                state.phase = Phase::Prepare;
            },
        )
        .await
    }
    async fn prepare_context(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        tuple: &ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> RunResult<Prepared<'db>> {
        self.local_with_fixed_transfers(3, size_of::<Prepared<'db>>() * 2, || ())
            .await?;
        let prepared =
            prepare_tuple_expression_with(builder, tuple, context, TupleExpressionFacts, self)
                .await?;
        #[cfg(all(test, feature = "experimental-analysis"))]
        self.local_with_fixed_transfers(1, 0, || {
            crate::types::infer::source_runtime::tests::contextual_tuple::observe_prepared(
                builder.db(),
                prepared._specification.as_ref(),
                prepared.annotations.as_slice(),
                prepared.can_use_type_context,
            );
        })
        .await?;
        Ok(prepared)
    }
    async fn install_prepared<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        prepared: Prepared<'db>,
    ) -> RunResult<()> {
        let mut prepared = Some(prepared);
        self.local_with_fixed_transfers(
            5,
            size_of::<Prepared<'db>>() * 2 + size_of::<&[ast::Expr]>() + size_of::<Phase<'db>>(),
            || {
                let prepared = prepared.take().ok_or(RunError::Contract(
                    "tuple preparation was already installed",
                ))?;
                state.prepared = prepared;
                state.remaining = &state.tuple.elts;
                state.phase = Phase::Elements;
                Ok(())
            },
        )
        .await?
    }
    async fn next_state_element<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local_with_fixed_transfers(
            3,
            size_of::<Option<&ast::Expr>>() + size_of::<&[ast::Expr]>(),
            || {
                let (next, rest) = state.remaining.split_first()?;
                state.remaining = rest;
                Some(next)
            },
        )
        .await
    }
    async fn state_annotation<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(2, size_of::<Option<Type<'db>>>(), || {
            state.prepared.annotations.next()
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleConstructionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn fixed_without_variable(
        &self,
        tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
    ) -> RunResult<TupleSpec<'db>> {
        let count = Self::checked(
            tuple
                .prefix_elements()
                .len()
                .checked_add(tuple.suffix_elements().len()),
        )?;
        let work = Self::checked(count.checked_mul(2).and_then(|n| n.checked_add(4)))?;
        let bytes = Self::checked(
            count
                .max(4)
                .checked_mul(2)
                .and_then(|n| n.checked_mul(size_of::<Type<'db>>())),
        )?;
        self.local_with_fixed_transfers(
            work,
            Self::checked(bytes.checked_add(size_of::<TupleSpec<'db>>()))?,
            || fixed_without_variable(tuple),
        )
        .await
    }

    async fn intern_borrowed(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        let program = self.environment_program(env).await?;
        let scan = Self::checked(spec.storage_len().checked_add(2))?;
        let (retirement, bytes) = self
            .local_with_fixed_transfers(scan, size_of::<(Option<usize>, Option<usize>)>(), || {
                (spec.retirement_work(), spec.clone_requested_bytes())
            })
            .await?;
        let retirement = Self::checked(retirement)?;
        let bytes = Self::checked(Self::checked(bytes)?.checked_add(size_of::<TupleSpec<'db>>()))?;
        let work = Self::checked(retirement.checked_mul(2).and_then(|n| n.checked_add(4)))?;
        let owned = self
            .local_with_fixed_transfers(work, bytes, || spec.clone())
            .await?;
        self.access.intern_tuple(program, owned).await
    }

    async fn intern_owned(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        let program = self.environment_program(env).await?;
        self.access.intern_tuple(program, spec).await
    }
}
