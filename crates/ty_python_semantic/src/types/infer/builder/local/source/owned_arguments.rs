//! Protected movement of the local invocation's existing argument owners.

use super::*;

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> OwnedArgumentEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Builder = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;
    type CustomSpecializationTarget = Infallible;

    async fn prepared<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        data: call::CallData<'db, 'expr>,
        arguments: CallArguments<'expr, 'db>,
    ) -> RunResult<call::Prepared<'db, 'expr>> {
        call::prepared_with(builders.get_mut(id), data, arguments, call::CallFacts, self).await
    }

    async fn install_prepared<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        id: BuilderId,
        prepared: call::Prepared<'db, 'expr>,
    ) -> RunResult<PreparedCall<'db>> {
        let grows = matches!(&prepared, call::Prepared::Arguments(..))
            && owners.slots.len() == owners.slots.capacity();
        let bytes = if grows {
            Self::checked(
                owners
                    .slots
                    .len()
                    .checked_add(1)
                    .and_then(|count| count.checked_mul(size_of::<OwnerSlot<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>>())),
            )?
        } else {
            0
        };
        let work = Self::checked(if grows {
            owners.slots.len().checked_add(8)
        } else {
            Some(8)
        })?;
        let mut prepared = Some(prepared);
        self.local(work, bytes, || {
            if grows {
                owners.slots.reserve_exact(1);
            }
            let prepared = prepared
                .take()
                .ok_or(RunError::Contract("prepared argument owner was consumed"))?;
            Ok(install_prepared(owners, id, prepared))
        })
        .await?
    }

    async fn take_active<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        owner: ActiveArgument,
    ) -> RunResult<
        TakenArgument<
            'db,
            'expr,
            OwnedState<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
    > {
        self.local(4, 0, || owners.take_active(owner)).await
    }

    async fn advance<'expr>(
        &self,
        taken: TakenArgument<
            'db,
            'expr,
            OwnedState<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<
        TakenArgument<
            'db,
            'expr,
            OwnedAction<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
    > {
        let TakenArgument { index, payload } = taken;
        let ArgumentPayload {
            builder,
            data,
            recursion_guard,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let phase = arguments::advance_with(
            phase,
            builders,
            recursion_guard.as_ref(),
            arguments::ArgumentFacts,
            self,
        )
        .await?;
        if let arguments::Action::Infer {
            builder,
            argument: (_, expression, _),
            ..
        } = &phase
        {
            debug_assert!(
                !builders
                    .builder(*builder)
                    .index
                    .is_standalone_expression(*expression)
            );
        }
        Ok(TakenArgument {
            index,
            payload: ArgumentPayload {
                builder,
                data,
                phase,
                recursion_guard,
                #[cfg(test)]
                lifetime,
            },
        })
    }

    async fn install_action<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        taken: TakenArgument<
            'db,
            'expr,
            OwnedAction<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
    ) -> RunResult<ArgumentStep<'db, 'expr>> {
        let mut taken = Some(taken);
        self.local(4, 0, || {
            taken
                .take()
                .map(|taken| owners.install_action(taken))
                .ok_or(RunError::Contract("argument action owner was consumed"))
        })
        .await?
    }

    async fn take_pending<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        owner: PendingArgument,
    ) -> RunResult<
        TakenArgument<
            'db,
            'expr,
            OwnedPending<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
    > {
        self.local(4, 0, || owners.take_pending(owner)).await
    }

    async fn resume<'expr>(
        &self,
        taken: TakenArgument<
            'db,
            'expr,
            OwnedPending<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
        ty: Type<'db>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<
        TakenArgument<
            'db,
            'expr,
            OwnedState<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
    > {
        let TakenArgument { index, payload } = taken;
        let ArgumentPayload {
            builder,
            data,
            recursion_guard,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let phase = arguments::resume_with(phase, ty, builders, self).await?;
        Ok(TakenArgument {
            index,
            payload: ArgumentPayload {
                builder,
                data,
                phase,
                recursion_guard,
                #[cfg(test)]
                lifetime,
            },
        })
    }

    async fn install_active<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        taken: TakenArgument<
            'db,
            'expr,
            OwnedState<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        >,
    ) -> RunResult<ActiveArgument> {
        let mut taken = Some(taken);
        self.local(4, 0, || {
            taken
                .take()
                .map(|taken| owners.install_active(taken))
                .ok_or(RunError::Contract("resumed argument owner was consumed"))
        })
        .await?
    }

    async fn take_completed<'owner, 'expr>(
        &self,
        owners: &'owner mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedArgumentEffects<'db, 'ast>>::Builder,
        >,
        owner: CompletedArgument,
    ) -> RunResult<
        CompletedArgumentLease<
            'owner,
            'db,
            'expr,
            <Self as OwnedArgumentEffects<'db, 'ast>>::Builder,
        >,
    > {
        self.local(4, 0, || owners.take_completed(owner)).await
    }

    async fn finish<'expr>(
        &self,
        mut taken: CompletedArgumentLease<
            '_,
            'db,
            'expr,
            <Self as OwnedArgumentEffects<'db, 'ast>>::Builder,
        >,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<FinishedOwner<'db>> {
        let index = taken.index;
        let (builder, data, storage, result) = taken.parts_mut();
        let ty = call::finish_with(
            builders.get_mut(builder),
            data,
            storage,
            result,
            self,
        )
        .await?;
        drop(taken);
        Ok(FinishedOwner { index, ty })
    }

    async fn retire<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, <Self as OwnedArgumentEffects<'db, 'ast>>::Builder>,
        finished: FinishedOwner<'db>,
    ) -> RunResult<Type<'db>> {
        self.local(4, 0, || owners.retire(finished)).await
    }
}
