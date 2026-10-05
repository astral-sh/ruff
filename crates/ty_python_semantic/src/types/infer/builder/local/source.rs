//! Local Call frames retain the same source owner while canonical callees suspend.

#[path = "source/annotation_expression.rs"]
mod annotation_effects;
#[path = "source/string_annotation.rs"]
mod string_annotation_effects;
#[path = "source/call.rs"]
mod call_effects;
#[path = "source/contextual.rs"]
mod contextual_effects;
#[path = "source/legacy_context.rs"]
mod legacy_context_effects;
#[path = "source/matching.rs"]
mod matching;
#[path = "source/owned_arguments.rs"]
mod owned_argument_effects;
#[path = "source/owned_specialization.rs"]
mod owned_specialization_effects;
#[path = "source/owned_callable_annotation.rs"]
mod owned_callable_annotation;
#[path = "source/owned_tuple_annotation.rs"]
mod owned_tuple_annotation;
#[path = "source/owned_tuple_expression.rs"]
mod owned_tuple_expression;

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::expression::Expression;

use super::*;
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::relation::source::resources::RelationResourceAccess;

pub(in crate::types::infer::builder) async fn expression<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    context: TypeContext<'db>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    expression_with_mode(
        builder,
        expression,
        context,
        ExpressionMode::Cached,
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn maybe_standalone_expression<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    context: TypeContext<'db>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    expression_with_mode(
        builder,
        expression,
        context,
        ExpressionMode::MaybeStandalone,
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn standalone_expression<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    context: TypeContext<'db>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    let standalone = effects
        .local(1, 0, || builder.index.try_expression(expression))
        .await?
        .ok_or(RunError::Contract(
            "a mandatory standalone expression must have a canonical ingredient",
        ))?;
    canonical_expression(builder, expression, standalone, context, effects).await
}

async fn canonical_expression<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    standalone: Expression<'db>,
    context: TypeContext<'db>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    let inference = effects.canonical_expression(standalone, context).await?;
    builder.extend_expression_with(inference, effects).await?;
    let work = SourceEffects::<A>::checked(inference.expressions.iter().len().checked_add(1))?;
    // The child's fallback can differ from the parent's merged cycle value.
    effects
        .local(work, 0, || inference.expression_type(expression))
        .await
}

async fn expression_with_mode<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    context: TypeContext<'db>,
    mode: ExpressionMode,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    run(
        builder,
        Work::Expression(BuilderId::ROOT, expression, context, mode),
        effects,
    )
    .await
}

/// Starts tuple value inference when a caller has not already entered the local expression driver.
pub(in crate::types::infer::builder) async fn tuple_value<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    tuple: &ast::ExprTuple,
    context: TypeContext<'db>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    run(
        builder,
        Work::StartTupleExpression(BuilderId::ROOT, tuple, context),
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn assignment_call<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    target: &ast::Expr,
    call: &ast::ExprCall,
    definition: Definition<'db>,
    context: TypeContext<'db>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    run(
        builder,
        Work::Callee(
            BuilderId::ROOT,
            &call.func,
            CalleeContinuation::Assignment {
                target,
                call,
                definition,
                context,
            },
        ),
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn annotation<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    annotation: &ast::Expr,
    deferred_state: DeferredExpressionState,
    policy: PEP613Policy,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<TypeAndQualifiers<'db>> {
    run_annotation(
        builder,
        Work::AnnotationStart(BuilderId::ROOT, annotation, deferred_state, policy),
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn annotation_body<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    annotation: &ast::Expr,
    policy: PEP613Policy,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<TypeAndQualifiers<'db>> {
    run_annotation(
        builder,
        Work::AnnotationBody(
            AnnotationRoot {
                builder: BuilderId::ROOT,
                annotation,
                saved: None,
            },
            policy,
        ),
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn type_expression<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    mode: TypeExpressionMode,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    type_expression_request(
        builder,
        TypeExpressionRequest::Expression { expression, mode },
        effects,
    )
    .await
}

pub(in crate::types::infer::builder) async fn type_expression_request<
    'run,
    'db: 'run,
    A: SourceAccess<'run, 'db>,
>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    request: TypeExpressionRequest<'db, '_>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    run(
        builder,
        Work::TypeExpression(BuilderId::ROOT, request),
        effects,
    )
    .await
}

async fn run<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    work: Work<'db, '_>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Type<'db>> {
    match run_result(builder, work, effects).await? {
        Some(LocalResult::Type(ty)) => Ok(ty),
        Some(LocalResult::Annotation(_)) | None => Err(RunError::Contract(
            "local source inference returned without a type result",
        )),
    }
}

async fn run_annotation<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    work: Work<'db, '_>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<TypeAndQualifiers<'db>> {
    match run_result(builder, work, effects).await? {
        Some(LocalResult::Annotation(annotation)) => Ok(annotation),
        Some(LocalResult::Type(_)) | None => Err(RunError::Contract(
            "local source inference returned without an annotation result",
        )),
    }
}

async fn run_result<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    work: Work<'db, '_>,
    effects: &SourceEffects<'_, 'run, 'db, A>,
) -> RunResult<Option<LocalResult<'db>>> {
    effects
        .local_with_fixed_transfers(
            36,
            size_of::<
                LocalInvocation<
                    '_,
                    'db,
                    '_,
                    '_,
                    <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                >,
            >() * 2 + size_of::<crate::types::relation::stable_storage::StableStorage<string_annotation::ParsedAnnotation>>() * 2,
            || (),
        )
        .await?;
    // Local expressions also run inside scope and definition futures. Keep the driver future
    // out of their layouts so polling does not accumulate large temporary stack frames.
    // Pending work must retire before the invocation restores its builders.
    #[cfg(test)]
    let syntax_complete = crate::types::infer::source_runtime::tests::quoted_annotations::SyntaxDropComplete::new();
    let syntax = crate::types::relation::stable_storage::StableStorage::new();
    #[cfg(test)]
    let _syntax_begin = crate::types::infer::source_runtime::tests::quoted_annotations::SyntaxDropBegin::new(syntax_complete.id());
    let mut invocation = None;
    let mut work = Some(work);
    let continuation = effects
        .quoted_annotation_future({
            let syntax = &syntax;
            let invocation_slot = &mut invocation;
            let work_slot = &mut work;
            move || {
                let invocation_slot = invocation_slot;
                let work_slot = work_slot;
                let initialized = LocalInvocation::new(builder);
                #[cfg(test)]
                let initialized = tests::resume_allocation::observe_invocation(initialized);
                #[cfg(test)]
                let initialized = tests::tuple_annotations::observe_invocation(initialized);
                #[cfg(test)]
                let initialized = tests::callable_annotations::observe_invocation(initialized);
                #[cfg(test)]
                if matches!(
                    work_slot.as_ref(),
                    Some(Work::AnnotationStart(..) | Work::AnnotationBody(..))
                ) {
                    tests::annotation_qualifiers::invocation_started(
                        initialized.builders.root as *const _ as usize,
                        function_annotation_state(initialized.builders.root),
                    );
                }
                drive(
                    work_slot,
                    invocation_slot.insert(initialized),
                    LocalFacts,
                    effects,
                    syntax,
                )
            }
        })
        .await?;
    continuation.await
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> LocalEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Builder = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;
    type CustomSpecializationTarget = Infallible;
    type StringAnnotations = crate::types::relation::stable_storage::StableStorage<string_annotation::ParsedAnnotation>;

    async fn parse_string_annotation<'expr>(
        &self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        string: &ast::ExprStringLiteral, storage: &'expr Self::StringAnnotations,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local_parse_string_annotation(builders.builder(id), string, storage).await
    }

    async fn prepare_string_annotation<'expr>(
        &self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        string: &'expr ast::ExprStringLiteral, parsed: &'expr ast::Expr,
    ) -> RunResult<string_annotation::Scope<'expr>> {
        self.local_prepare_string_annotation(builders.builder(id), string, parsed).await
    }

    async fn enter_string_annotation(
        &self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        scope: string_annotation::Scope<'_>,
    ) -> RunResult<()> {
        self.local_enter_string_annotation(builders.get_mut(id), scope).await
    }

    async fn finish_string_annotation(
        &self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        scope: string_annotation::Scope<'_>,
    ) -> RunResult<()> {
        self.local_finish_string_annotation(builders.get_mut(id), scope).await
    }


    async fn next<'expr>(
        &self,
        work: &mut Option<Work<'db, 'expr>>,
    ) -> RunResult<Option<Work<'db, 'expr>>> {
        self.local(1, size_of::<Option<Work<'db, 'expr>>>(), || work.take()).await
    }
    async fn continue_with<'expr>(
        &self,
        work: &mut Option<Work<'db, 'expr>>,
        next: Work<'db, 'expr>,
    ) -> RunResult<()> {
        let mut next = Some(next);
        self.local(1, size_of::<Option<Work<'db, 'expr>>>(), || {
            *work = next.take()
        })
        .await
    }
    async fn push<'expr>(
        &self,
        invocation: &mut LocalInvocation<
            '_,
            'db,
            'ast,
            'expr,
            <Self as LocalEffects<'db, 'ast>>::Builder,
        >,
        frame: Frame<'db, 'expr>,
    ) -> RunResult<()> {
        if !matches!(
            &frame,
            Frame::Finish(..)
                | Frame::AnnotationResume(..)
                | Frame::TypeExpressionResume(..)
                | Frame::TypeExpressionFinish(..)
                | Frame::StringAnnotation(..)
                | Frame::Callee(..)
                | Frame::AssignmentFinish(..)
                | Frame::LegacyTypeVar(..)
                | Frame::SubscriptReceiver(..)
                | Frame::SubscriptSlice(..)
                | Frame::Argument(..)
                | Frame::Specialization(..)
                | Frame::CallableAnnotation(..)
                | Frame::TupleAnnotation(..)
                | Frame::TupleExpression(..)
                | Frame::ParamSpec(..)
        ) {
            return self.unavailable(SourceOperation::CallArguments).await;
        }
        #[cfg(test)]
        let quoted = matches!(&frame, Frame::StringAnnotation(..));
        #[cfg(test)]
        if quoted {
            crate::types::infer::source_runtime::tests::quoted_annotations::observe_before(
                crate::types::infer::source_runtime::tests::quoted_annotations::Stage::FrameInstalled,
            );
        }
        let frames = &mut invocation.frames;
        let growth = frames.len() == frames.capacity();
        // Buffer growth relocates the old frames; every push also initializes one frame.
        let moved = if growth { frames.len() } else { 0 };
        let allocation = if growth { Self::checked(frames.len().checked_add(1))? } else { 0 };
        std::alloc::Layout::array::<Frame<'db, 'expr>>(allocation)
            .map_err(|_| RunError::Contract("local frame allocation layout overflow"))?;
        let bytes = Self::checked(moved.checked_add(allocation).and_then(|count| count.checked_add(1)).and_then(|count| count.checked_mul(size_of::<Frame<'db, 'expr>>())))?;
        let work = Self::checked(if growth {
            frames.len().checked_add(3)
        } else {
            Some(3)
        })?;
        let mut frame = Some(frame);
        let action = || {
            if growth {
                frames.reserve_exact(1);
            }
            frames.extend(frame.take());
        };
        self.local_with_fixed_transfers(work, bytes, action)
        .await?;
        #[cfg(test)]
        if quoted {
            crate::types::infer::source_runtime::tests::quoted_annotations::observe_after(
                crate::types::infer::source_runtime::tests::quoted_annotations::Stage::FrameInstalled,
            );
        }
        Ok(())
    }
    async fn pop<'expr>(
        &self,
        frames: &mut Vec<Frame<'db, 'expr>>,
    ) -> RunResult<Option<Frame<'db, 'expr>>> {
        self.local(1, size_of::<Option<Frame<'db, 'expr>>>(), || frames.pop()).await
    }
    async fn push_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>, continuation: AnnotationContinuation<'expr>) -> RunResult<()> {
        let annotations = &mut invocation.annotations;
        let growth = annotations.len() == annotations.capacity();
        let moved = if growth { annotations.len() } else { 0 };
        let backing = if growth { Self::checked(annotations.len().checked_add(1))? } else { 0 };
        let bytes = Self::checked(moved.checked_add(backing).and_then(|count| count.checked_add(1)).and_then(|count| count.checked_mul(size_of::<AnnotationContinuation<'expr>>())))?;
        // Each entry pays its retirement once. Replacing a buffer also retires its old backing.
        let work = Self::checked(moved.checked_add(if growth { annotations.capacity() } else { 0 }).and_then(|count| count.checked_add(backing)).and_then(|count| count.checked_add(3)))?;
        let mut continuation = Some(continuation);
        self.local(work, bytes, || {
            if growth { annotations.reserve_exact(1); }
            annotations.extend(continuation.take());
        }).await?;
        #[cfg(test)]
        tests::annotation_qualifiers::parent_suspended(invocation.builders.root.db(), invocation.builders.root as *const _ as usize, annotations.len());
        Ok(())
    }

    async fn pop_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>) -> RunResult<Option<AnnotationContinuation<'expr>>> {
        #[cfg(test)]
        tests::annotation_qualifiers::child_completed(invocation.builders.root.db(), invocation.builders.root as *const _ as usize, invocation.annotations.len());
        self.local(1, size_of::<Option<AnnotationContinuation<'expr>>>(), || invocation.annotations.pop()).await
    }

    async fn canonical(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let builder = builders.get_mut(id);
        let standalone = self
            .local(1, 0, || builder.index.try_expression(expression))
            .await?;
        let Some(standalone) = standalone else {
            return Ok(None);
        };
        canonical_expression(builder, expression, standalone, tcx, self)
            .await
            .map(Some)
    }
    async fn existing(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
    ) -> RunResult<Option<Type<'db>>> {
        let builder = builders.builder(id);
        let units = Self::checked(builder.expressions.capacity().checked_add(1))?;
        self.local(units, 0, || builder.try_expression_type(expression))
            .await
    }
    async fn cache_enabled(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> RunResult<bool> {
        self.local(1, 0, || builders.builder(id).expression_cache.is_some())
            .await
    }
    async fn cache_lookup(
        &self,
        _builders: &BuilderStore<'_, 'db, 'ast>,
        _id: BuilderId,
        _expression: &ast::Expr,
        _tcx: TypeContext<'db>,
    ) -> RunResult<Option<ExpressionCacheEntry<'db>>> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn cache_hit(
        &self,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
        _id: BuilderId,
        _expression: &ast::Expr,
        _entry: ExpressionCacheEntry<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn speculate(
        &self,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
        _id: BuilderId,
    ) -> RunResult<BuilderId> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn cache_commit(
        &self,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
        _parent: BuilderId,
        _child: BuilderId,
        _expression: &ast::Expr,
        _tcx: TypeContext<'db>,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExpressionCache).await
    }
    async fn contextual_dispatch(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> RunResult<source_expression::ContextualExpressionResult<'db>> {
        source_expression::contextual_expression_with(builders.get_mut(id), expression, tcx, self)
            .await
    }
    async fn other_expression(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        builders
            .get_mut(id)
            .infer_value_expression_with(self, expression, tcx)
            .await
    }
    async fn finish_expression(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> RunResult<Type<'db>> {
        builders
            .get_mut(id)
            .finish_expression_type_with(self, expression, ty, tcx)
            .await
    }
    async fn enter_callee(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> RunResult<CalleeState<'db>> {
        self.local(3, 0, || {
            let builder = builders.get_mut(id);
            CalleeState {
                binding: builder.typevar_binding_context.take(),
                check_unbound: builder
                    .context
                    .inference_flags
                    .replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, true),
            }
        })
        .await
    }
    async fn restore_callee(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        state: CalleeState<'db>,
    ) -> RunResult<()> {
        self.local(2, 0, || {
            let builder = builders.get_mut(id);
            builder
                .context
                .inference_flags
                .set(InferenceFlags::CHECK_UNBOUND_TYPEVARS, state.check_unbound);
            builder.typevar_binding_context = state.binding;
        })
        .await
    }
    async fn prepare_annotation_scope(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        state: DeferredExpressionState,
    ) -> RunResult<AnnotationScope> {
        self.local_prepare_annotation_scope(builders.builder(id), state)
            .await
    }
    async fn enter_annotation_scope(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: AnnotationScope,
    ) -> RunResult<()> {
        self.local(2, 0, || scope.enter(builders.get_mut(id))).await
    }
    async fn restore_annotation_scope(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: AnnotationScope,
    ) -> RunResult<()> {
        self.local(2, 0, || scope.restore(builders.get_mut(id)))
            .await
    }
    async fn store_annotation_qualifiers(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        qualifiers: TypeQualifiers,
    ) -> RunResult<()> {
        self.store_annotation_qualifiers(builders.get_mut(id), expression, qualifiers)
            .await
    }
    async fn store_type_expression(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.local_store_type_expression(builders.get_mut(id), expression, ty)
            .await
    }
    async fn prepare_type_expression_scope(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        mode: TypeExpressionMode,
    ) -> RunResult<Option<TypeExpressionScope>> {
        self.local_prepare_type_expression_scope(builders.builder(id), mode)
            .await
    }
    async fn enter_type_expression_scope(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: TypeExpressionScope,
    ) -> RunResult<()> {
        self.local(4, 0, || scope.enter(builders.get_mut(id))).await
    }
    async fn restore_type_expression_before_store(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: TypeExpressionScope,
    ) -> RunResult<()> {
        self.local(3, 0, || scope.restore_before_store(builders.get_mut(id)))
            .await
    }
    async fn restore_type_expression_after_store(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: TypeExpressionScope,
    ) -> RunResult<()> {
        self.local(1, 0, || scope.restore_after_store(builders.get_mut(id)))
            .await
    }
    async fn start_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        annotation: &'expr ast::Expr,
        policy: PEP613Policy,
    ) -> RunResult<AnnotationStep<'db, 'expr>> {
        annotation_expression::start_annotation_with(builders.get_mut(id), annotation, policy, self)
            .await
    }
    async fn resume_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: AnnotationPending<'expr>,
        ty: Type<'db>,
    ) -> RunResult<AnnotationStep<'db, 'expr>> {
        annotation_expression::resume_annotation_with(builders.get_mut(id), pending, ty, self).await
    }
    async fn resume_qualifier<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, root: &AnnotationRoot<'expr>, pending: QualifierPending<'expr>, ty: TypeAndQualifiers<'db>) -> RunResult<AnnotationStep<'db, 'expr>> {
        annotation_expression::resume_qualifier_with(builders.get_mut(root.builder), root.annotation, pending, ty, self).await
    }

    async fn start_type_expression<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        request: TypeExpressionRequest<'db, 'expr>,
    ) -> RunResult<TypeExpressionStep<'db, 'expr>> {
        type_expression::start_type_expression_with(builders.get_mut(id), request, self).await
    }
    async fn resume_type_expression<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: TypeExpressionPending<'db, 'expr>,
        ty: Type<'db>,
    ) -> RunResult<TypeExpressionStep<'db, 'expr>> {
        type_expression::resume_type_expression_with(builders.get_mut(id), pending, ty, self).await
    }
    async fn start_call<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &'expr ast::ExprCall,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> RunResult<call::Start<'db, 'expr>> {
        call::start_with(
            builders.get_mut(id),
            expression,
            ty,
            tcx,
            call::CallFacts,
            self,
        )
        .await
    }
    async fn start_assignment<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        target: &'expr ast::Expr,
        call: &'expr ast::ExprCall,
        definition: Definition<'db>,
        callable_type: Type<'db>,
    ) -> RunResult<assignment::Start<'db, 'expr>> {
        assignment::start_with(
            builders.get_mut(id),
            target,
            call,
            definition,
            callable_type,
            assignment::AssignmentFacts,
            self,
        )
        .await
    }
    async fn finish_assignment(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        target: &ast::Expr,
        call: &ast::ExprCall,
        callable_type: Type<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        assignment::finish_with(
            builders.get_mut(id),
            target,
            call,
            callable_type,
            ty,
            assignment::AssignmentFacts,
            self,
        )
        .await
    }
    async fn legacy_typevar<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        state: legacy::State<'db, 'expr>,
    ) -> RunResult<legacy::Action<'db, 'expr>> {
        legacy::advance_with(state, builders.get_mut(id), self).await
    }
    async fn resume_legacy_typevar<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: legacy::Pending<'db, 'expr>,
        ty: Type<'db>,
    ) -> RunResult<legacy::State<'db, 'expr>> {
        legacy::resume_with(pending, ty, builders.get_mut(id), self).await
    }
    async fn subscript_receiver<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        subscript: &'expr ast::ExprSubscript,
        ty: Type<'db>,
    ) -> RunResult<subscript::SubscriptStart<'db, 'expr>> {
        subscript::subscript_after_receiver_with(builders.get_mut(id), subscript, ty, self).await
    }
    async fn subscript_slice<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: subscript::SubscriptPending<'db, 'expr>,
        ty: Type<'db>,
    ) -> RunResult<Result<Type<'db>, Type<'db>>> {
        subscript::subscript_after_slice_with(builders.get_mut(id), pending, ty, self).await
    }
    async fn prepare<'expr>(
        &self,
        id: BuilderId,
        source: &'expr ast::Arguments,
    ) -> RunResult<Preparation<'db, 'expr>> {
        let bytes = Self::checked(CallArguments::capacity_bytes(source.len()))?;
        let work = Self::checked(source.len().checked_mul(2).and_then(|n| n.checked_add(4)))?;
        self.local(work, bytes, || Preparation {
            builder: id,
            source,
            cursor: ArgumentsIter::from_ast(source),
            arguments: CallArguments::with_capacity(source.len()),
        })
        .await
    }
    async fn preparation_step<'expr>(
        &self,
        preparation: Preparation<'db, 'expr>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<PreparationStep<'db, 'expr>> {
        preparation::advance(preparation, builders, self).await
    }
    async fn resume_splat<'expr>(
        &self,
        _splat: Splat<'db, 'expr>,
        _ty: Type<'db>,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<Preparation<'db, 'expr>> {
        self.unavailable(SourceOperation::CallArguments).await
    }
    async fn prepared_call<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        id: BuilderId,
        data: call::CallData<'db, 'expr>,
        arguments: CallArguments<'expr, 'db>,
    ) -> RunResult<PreparedCall<'db>> {
        let mut input = Some((data, arguments));
        self.allocate_future(|| async {
            let (data, arguments) = input.take().ok_or(RunError::Contract(
                "AST call preparation input was already consumed",
            ))?;
            owned_prepared_call(builders, owners, id, data, arguments, self).await
        })
        .await?
        .await
    }
    async fn argument_step<'expr>(
        &self,
        owner: ActiveArgument,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<ArgumentStep<'db, 'expr>> {
        // Keep call argument state in a separate admitted allocation so literal operands
        // and other non-call expressions do not reserve it in each local driver.
        self.allocate_future(|| owned_argument_step(owner, owners, builders, self))
            .await?
            .await
    }
    async fn resume_argument<'expr>(
        &self,
        owner: PendingArgument,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<ActiveArgument> {
        #[cfg(test)]
        let db = builders.builder(BuilderId::ROOT).db();
        #[cfg(test)]
        tests::resume_allocation::before(owners, &owner);
        self.allocate_future(|| {
            #[cfg(test)]
            tests::resume_allocation::admitted(db);
            owned_resume_argument(owner, ty, owners, builders, self)
        })
        .await?
        .await
    }
    async fn finish_call<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        owner: CompletedArgument,
    ) -> RunResult<Type<'db>> {
        owned_finish_call(builders, owners, owner, self).await
    }
    async fn start_callable_annotation<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        request: callable_annotation::Request<'expr>,
    ) -> RunResult<callable_annotation::Active> {
        owned_callable_annotation::start(self, owners, builders, id, request).await
    }

    async fn callable_annotation_step<'expr>(
        &self,
        owner: callable_annotation::Active,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<callable_annotation::Step<'expr>> {
        owned_callable_annotation::step(self, owner, owners, builders).await
    }

    async fn resume_callable_annotation<'expr>(
        &self,
        owner: callable_annotation::Waiting,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<callable_annotation::Active> {
        owned_callable_annotation::resume(self, owner, ty, owners, builders).await
    }

    async fn finish_callable_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        owner: callable_annotation::Finished,
    ) -> RunResult<Type<'db>> {
        owned_callable_annotation::finish(self, builders, owners, owner).await
    }

    async fn start_tuple_annotation<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        request: tuple_annotation::Request<'expr>,
    ) -> RunResult<tuple_annotation::Active> {
        owned_tuple_annotation::start(self, owners, builders, id, request).await
    }

    async fn tuple_annotation_step<'expr>(
        &self,
        owner: tuple_annotation::Active,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<tuple_annotation::Step<'expr>> {
        owned_tuple_annotation::step(self, owner, owners, builders).await
    }

    async fn resume_tuple_annotation<'expr>(
        &self,
        owner: tuple_annotation::Waiting,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<tuple_annotation::Active> {
        owned_tuple_annotation::resume(self, owner, ty, owners, builders).await
    }

    async fn finish_tuple_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        owner: tuple_annotation::Finished,
    ) -> RunResult<Type<'db>> {
        owned_tuple_annotation::finish(self, builders, owners, owner).await
    }

    async fn start_tuple_value<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        id: BuilderId,
        tuple: &'expr ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> RunResult<tuple_expression::Active> {
        owned_tuple_expression::start(self, owners, id, tuple, context).await
    }
    async fn tuple_value_step<'expr>(
        &self,
        owner: tuple_expression::Active,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<tuple_expression::Step<'db, 'expr>> {
        owned_tuple_expression::step(self, owner, owners, builders).await
    }
    async fn resume_tuple_value<'expr>(
        &self,
        owner: tuple_expression::Waiting,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
    ) -> RunResult<tuple_expression::Active> {
        owned_tuple_expression::resume(self, owner, owners).await
    }
    async fn finish_tuple_value<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        owner: tuple_expression::Finished,
    ) -> RunResult<Type<'db>> {
        owned_tuple_expression::finish(self, owners, owner).await
    }

    async fn start_specialization<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        id: BuilderId,
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
        kind: ClassSpecializationKind,
    ) -> RunResult<ActiveSpecialization> {
        owned_specialization_effects::start_specialization(
            self,
            owners,
            id,
            subscript,
            value_ty,
            class,
            generic_context,
            kind,
        )
        .await
    }

    async fn specialization_step<'expr>(
        &self,
        owner: ActiveSpecialization,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<SpecializationStep<'expr>> {
        // The local driver only needs control back to infer an expression or finish. Keeping
        // internal transitions here avoids allocating a new future for every phase.
        self.allocate_future(|| async move {
            let mut owner = owner;
            loop {
                match owned_specialization_step(owner, owners, builders, self).await? {
                    SpecializationStep::Continue(next) => owner = next,
                    terminal => return Ok(terminal),
                }
            }
        })
        .await?
        .await
    }

    async fn resume_specialization<'expr>(
        &self,
        owner: PendingSpecialization,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<ActiveSpecialization> {
        #[cfg(test)]
        let db = builders.builder(BuilderId::ROOT).db();
        #[cfg(test)]
        tests::resume_allocation::before_specialization(owners, &owner);
        self.allocate_future(|| {
            #[cfg(test)]
            tests::resume_allocation::admitted_specialization(db);
            owned_resume_specialization(owner, ty, owners, builders, self)
        })
        .await?
        .await
    }

    async fn finish_specialization<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, <Self as LocalEffects<'db, 'ast>>::Builder>,
        owner: CompletedSpecialization,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| owned_finish_specialization(builders, owners, owner, self))
            .await?
            .await
    }
    async fn enter_paramspec(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> RunResult<bool> {
        self.local(1, 0, || {
            builders
                .get_mut(id)
                .context
                .inference_flags
                .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true)
        })
        .await
    }
    async fn restore_paramspec(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        previous: bool,
    ) -> RunResult<()> {
        self.local(1, 0, || {
            builders
                .get_mut(id)
                .context
                .inference_flags
                .set(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, previous)
        })
        .await
    }
    async fn permit_paramspec(
        &self,
        policy: arguments::ArgumentPolicy,
        expression: &ast::Expr,
    ) -> RunResult<bool> {
        if matches!(policy, arguments::ArgumentPolicy::PermitParamSpec) {
            return self.unavailable(SourceOperation::CallArguments).await;
        }
        self.local(1, 0, || {
            match SynchronousLocalEffects::permit_paramspec(
                &OrdinaryLocalEffects::default(),
                policy,
                expression,
            ) {
                Ok(value) => value,
                Err(never) => match never {},
            }
        })
        .await
    }
    async fn complete<'expr>(
        &self,
        invocation: &mut LocalInvocation<
            '_,
            'db,
            'ast,
            'expr,
            <Self as LocalEffects<'db, 'ast>>::Builder,
        >,
    ) -> RunResult<()> {
        self.local(2, 0, || invocation.complete()).await
    }
}
