//! Admitted callable owners keep parameter storage alive across return-type inference.

use super::*;
use crate::types::call::preparation::known_class::KnownClassBindingEffects;
use crate::types::infer::builder::source_expression::SourceExpressionEffects;

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>>
    callable_annotation::CallableAnnotationEffects<'db, 'ast> for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn gradual_parameters(&self) -> RunResult<Parameters<'db>> {
        KnownClassBindingEffects::gradual_parameters(self).await
    }

    async fn next_parameter<'expr>(
        &self,
        state: &mut callable_annotation::ParameterState<'db, 'expr>,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local(3, 0, || callable_annotation::next_parameter(state))
            .await
    }

    async fn restore_unpack(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        saved: callable_annotation::SavedFlags,
    ) -> RunResult<()> {
        self.local(
            2,
            size_of::<callable_annotation::SavedFlags>() + size_of::<InferenceFlags>(),
            || saved.restore_unpack(builder),
        )
        .await
    }

    async fn normalize(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        state: &mut callable_annotation::ParameterState<'db, '_>,
    ) -> RunResult<Parameters<'db>> {
        let parameters = self
            .local(size_of::<Vec<Parameter<'db>>>() * 2 + 1, 0, || {
                std::mem::take(&mut state.parameters)
            })
            .await?;
        Parameters::from_annotation_with(builder.db(), parameters, self).await
    }

    async fn restore_concatenate(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        saved: callable_annotation::SavedFlags,
    ) -> RunResult<()> {
        self.local(
            2,
            size_of::<callable_annotation::SavedFlags>() + size_of::<InferenceFlags>(),
            || saved.restore_concatenate(builder),
        )
        .await
    }

    async fn parameter(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<Parameter<'db>> {
        let work = Self::checked(
            builder
                .type_expression_flags
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        let flags = self
            .local(work, 0, || builder.type_expression_flags(expression))
            .await?;
        if flags.contains(TypeExpressionFlags::UNPACK) {
            return self
                .unavailable(SourceOperation::TypeExpressionCallableUnpack)
                .await;
        }
        self.local(size_of::<Parameter<'db>>() * 2 + 1, 0, || {
            Parameter::positional_only(None).with_annotated_type(ty)
        })
        .await
    }

    async fn append(
        &self,
        state: &mut callable_annotation::ParameterState<'db, '_>,
        parameter: Parameter<'db>,
    ) -> RunResult<()> {
        Parameters::push_parameter_with(&mut state.parameters, parameter, self).await
    }

    async fn callable(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        completed: &callable_annotation::Completed<'db, '_>,
    ) -> RunResult<Type<'db>> {
        let signature = self
            .local(
                8,
                size_of::<Signature<'db>>() * 2
                    + size_of::<Parameters<'db>>()
                    + size_of::<Type<'db>>(),
                || Signature::new(completed.parameters.clone(), completed.return_type),
            )
            .await?;
        self.single_callable_type(signature).await
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.store_expression(builder, expression, ty).await
    }

    async fn restore_unbound(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        saved: callable_annotation::SavedFlags,
    ) -> RunResult<()> {
        self.local(
            2,
            size_of::<callable_annotation::SavedFlags>() + size_of::<InferenceFlags>(),
            || saved.restore_unbound(builder),
        )
        .await
    }
}

/// Quotes fixed phase construction before taking a payload out of its owner slot.
const fn transition_bytes() -> usize {
    size_of::<callable_annotation::Phase<'_, '_>>()
        + size_of::<callable_annotation::State<'_, '_>>()
        + size_of::<callable_annotation::Pending<'_, '_>>()
        + size_of::<callable_annotation::Action<'_, '_>>()
        + size_of::<callable_annotation::Completed<'_, '_>>()
        + size_of::<callable_annotation::Request<'_>>()
        + size_of::<callable_annotation::SavedFlags>()
        + size_of::<Option<callable_annotation::Taken<callable_annotation::Action<'_, '_>>>>()
        + size_of::<Option<callable_annotation::Taken<callable_annotation::State<'_, '_>>>>()
}

pub(super) async fn start<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
    builder: BuilderId,
    request: callable_annotation::Request<'expr>,
) -> RunResult<callable_annotation::Active> {
    let grows = owners.slots.len() == owners.slots.capacity();
    let slot_size = size_of::<
        OwnerSlot<'db, 'expr, <A::Resources as RelationResourceAccess<'run, 'db>>::Builder>,
    >();
    let allocation = if grows {
        SourceEffects::<'_, 'run, 'db, A>::checked(owners.slots.len().checked_add(1))?
    } else {
        0
    };
    let relocated = if grows { owners.slots.len() } else { 0 };
    let bytes = SourceEffects::<'_, 'run, 'db, A>::checked(
        allocation
            .checked_add(relocated)
            .and_then(|count| count.checked_add(1))
            .and_then(|count| count.checked_mul(slot_size))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<callable_annotation::State<'db, 'expr>>())
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    size_of::<callable_annotation::SavedFlags>() + size_of::<InferenceFlags>(),
                )
            })
            .and_then(|bytes| bytes.checked_add(size_of::<callable_annotation::Active>())),
    )?;
    let work = SourceEffects::<'_, 'run, 'db, A>::checked(if grows {
        owners.slots.len().checked_add(16)
    } else {
        Some(16)
    })?;
    effects
        .local(work, bytes, || {
            if grows {
                owners.slots.reserve_exact(1);
            }
            owners.push_callable_annotation(builder, request, builders.get_mut(builder))
        })
        .await
}

pub(super) async fn step<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owner: callable_annotation::Active,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
) -> RunResult<callable_annotation::Step<'expr>> {
    #[cfg(test)]
    tests::callable_annotations::before_step(builders, owners, &owner);
    effects
        .allocate_future(|| async move {
            let taken = effects
                .local(
                    8,
                    size_of::<callable_annotation::Taken<callable_annotation::State<'db, 'expr>>>()
                        + transition_bytes()
                        + size_of::<
                            OwnerSlot<
                                'db,
                                'expr,
                                <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                            >,
                        >(),
                    || owners.take_callable_active(owner),
                )
                .await?;
            let callable_annotation::Taken { index, payload } = taken;
            let callable_annotation::Payload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            } = payload;
            let phase = callable_annotation::advance_with(
                phase,
                builders.get_mut(builder),
                callable_annotation::Facts,
                effects,
            )
            .await?;
            let mut taken = Some(callable_annotation::Taken {
                index,
                payload: callable_annotation::Payload {
                    builder,
                    phase,
                    #[cfg(test)]
                    lifetime,
                },
            });
            effects
                .local(
                    4,
                    size_of::<
                        OwnerSlot<
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                        >,
                    >() + size_of::<callable_annotation::Step<'expr>>(),
                    || {
                        taken
                            .take()
                            .map(|taken| owners.install_callable_action(taken))
                            .ok_or(RunError::Contract(
                                "callable annotation action owner was consumed",
                            ))
                    },
                )
                .await?
        })
        .await?
        .await
}

pub(super) async fn resume<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owner: callable_annotation::Waiting,
    ty: Type<'db>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
) -> RunResult<callable_annotation::Active> {
    effects
        .allocate_future(|| async move {
            let taken = effects
                .local(
                    8,
                    size_of::<callable_annotation::Taken<callable_annotation::Pending<'db, 'expr>>>(
                    ) + transition_bytes()
                        + size_of::<
                            OwnerSlot<
                                'db,
                                'expr,
                                <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                            >,
                        >(),
                    || owners.take_callable_pending(owner),
                )
                .await?;
            let callable_annotation::Taken { index, payload } = taken;
            let callable_annotation::Payload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            } = payload;
            let phase =
                callable_annotation::resume_with(phase, ty, builders.builder(builder), effects)
                    .await?;
            let mut taken = Some(callable_annotation::Taken {
                index,
                payload: callable_annotation::Payload {
                    builder,
                    phase,
                    #[cfg(test)]
                    lifetime,
                },
            });
            effects
                .local(
                    4,
                    size_of::<
                        OwnerSlot<
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                        >,
                    >() + size_of::<callable_annotation::Active>(),
                    || {
                        taken
                            .take()
                            .map(|taken| owners.install_callable_active(taken))
                            .ok_or(RunError::Contract(
                                "resumed callable annotation owner was consumed",
                            ))
                    },
                )
                .await?
        })
        .await?
        .await
}

pub(super) async fn finish<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    owner: callable_annotation::Finished,
) -> RunResult<Type<'db>> {
    effects
        .allocate_future(|| async move {
            let mut lease = effects
                .local(
                    4,
                    size_of::<
                        callable_annotation::CompletionLease<
                            '_,
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                            Infallible,
                        >,
                    >() + size_of::<
                        OwnerSlot<
                            'db,
                            'expr,
                            <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                        >,
                    >(),
                    || owners.callable_completion(owner),
                )
                .await?;
            let index = lease.index;
            let (builder, completed) = lease.parts_mut();
            let ty = callable_annotation::finish_with(
                completed,
                builders.get_mut(builder),
                callable_annotation::Facts,
                effects,
            )
            .await?;
            drop(lease);
            effects
                .local(
                    4,
                    size_of::<FinishedOwner<'db>>()
                        + size_of::<Type<'db>>()
                        + size_of::<
                            OwnerSlot<
                                'db,
                                'expr,
                                <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                            >,
                        >(),
                    || owners.retire(FinishedOwner { index, ty }),
                )
                .await
        })
        .await?
        .await
}
