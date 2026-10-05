use super::*;

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> OwnedSpecializationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Builder = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;
    type CustomSpecializationTarget = Infallible;

    async fn take_active<'expr>(
        &self,
        owners: &mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
        >,
        owner: ActiveSpecialization,
    ) -> RunResult<
        TakenSpecialization<
            SpecializationState<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    > {
        self.local(4, 0, || owners.take_specialization_active(owner))
            .await
    }

    async fn advance<'expr>(
        &self,
        taken: TakenSpecialization<
            SpecializationState<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<
        TakenSpecialization<
            SpecializationAction<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    > {
        let TakenSpecialization { index, payload } = taken;
        let SpecializationPayload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let phase = specialization::advance_with(
            phase,
            builders.get_mut(builder),
            specialization::ExplicitSpecializationFacts,
            self,
        )
        .await?;
        Ok(TakenSpecialization {
            index,
            payload: SpecializationPayload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            },
        })
    }

    async fn install_action<'expr>(
        &self,
        owners: &mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
        >,
        taken: TakenSpecialization<
            SpecializationAction<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    ) -> RunResult<SpecializationStep<'expr>> {
        let mut taken = Some(taken);
        self.local(4, 0, || {
            taken
                .take()
                .map(|taken| owners.install_specialization_action(taken))
                .ok_or(RunError::Contract(
                    "specialization action owner was consumed",
                ))
        })
        .await?
    }

    async fn take_pending<'expr>(
        &self,
        owners: &mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
        >,
        owner: PendingSpecialization,
    ) -> RunResult<
        TakenSpecialization<
            SpecializationPending<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    > {
        self.local(4, 0, || owners.take_specialization_pending(owner))
            .await
    }

    async fn resume<'expr>(
        &self,
        taken: TakenSpecialization<
            SpecializationPending<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
        ty: Type<'db>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<
        TakenSpecialization<
            SpecializationState<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    > {
        let TakenSpecialization { index, payload } = taken;
        let SpecializationPayload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        #[cfg(test)]
        let db = builders.builder(builder).db();
        let phase = specialization::resume_with(
            phase,
            ty,
            builders.get_mut(builder),
            specialization::ExplicitSpecializationFacts,
            self,
        )
        .await?;
        #[cfg(test)]
        if let Some(constraints) = phase.constraint_builder() {
            tests::resume_allocation::specialization_resumed(std::borrow::Borrow::borrow(
                constraints,
            ));
        }
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::explicit_specialization::observe_argument(db);
        Ok(TakenSpecialization {
            index,
            payload: SpecializationPayload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            },
        })
    }

    async fn install_active<'expr>(
        &self,
        owners: &mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
        >,
        taken: TakenSpecialization<
            SpecializationState<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    ) -> RunResult<ActiveSpecialization> {
        let mut taken = Some(taken);
        self.local(4, 0, || {
            taken
                .take()
                .map(|taken| owners.install_specialization_active(taken))
                .ok_or(RunError::Contract(
                    "resumed specialization owner was consumed",
                ))
        })
        .await?
    }

    async fn take_completed<'expr>(
        &self,
        owners: &mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
        >,
        owner: CompletedSpecialization,
    ) -> RunResult<
        TakenSpecialization<
            SpecializationCompleted<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
    > {
        self.local(4, 0, || owners.take_specialization_completed(owner))
            .await
    }

    async fn finish<'expr>(
        &self,
        taken: TakenSpecialization<
            SpecializationCompleted<
                'db,
                'expr,
                <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
                Infallible,
            >,
        >,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> RunResult<FinishedOwner<'db>> {
        let TakenSpecialization { index, payload } = taken;
        let SpecializationPayload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        #[cfg(test)]
        let constraints = *phase.constraint_builder();
        #[cfg(test)]
        tests::resume_allocation::specialization_finishing(std::borrow::Borrow::borrow(
            &constraints,
        ));
        let ty = specialization::finish_with(phase, builders.get_mut(builder), self).await?;
        #[cfg(test)]
        tests::resume_allocation::specialization_finished(std::borrow::Borrow::borrow(
            &constraints,
        ));
        #[cfg(test)]
        drop(lifetime);
        Ok(FinishedOwner { index, ty })
    }

    async fn retire<'expr>(
        &self,
        owners: &mut LocalOwners<
            'db,
            'expr,
            <Self as OwnedSpecializationEffects<'db, 'ast>>::Builder,
        >,
        finished: FinishedOwner<'db>,
    ) -> RunResult<Type<'db>> {
        self.local(4, 0, || owners.retire(finished)).await
    }
}

pub(super) async fn start_specialization<'run, 'db: 'run, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builder: BuilderId,
    subscript: &'expr ast::ExprSubscript,
    value_ty: Type<'db>,
    class: StaticClassLiteral<'db>,
    generic_context: GenericContext<'db>,
    kind: ClassSpecializationKind,
) -> RunResult<ActiveSpecialization> {
    type Request<'db, 'expr> =
        specialization::Request<'db, 'expr, SpecializationTarget<'db, Infallible>>;
    type InsertionCarriers<'db, 'expr, B> = (
        [ClassSpecializationKind; 6],
        [SpecializationTarget<'db, Infallible>; 2],
        [Request<'db, 'expr>; 4],
        [SpecializationState<'db, 'expr, B, Infallible>; 2],
        [SpecializationPhase<'db, 'expr, B, Infallible>; 2],
        [SpecializationPayload<SpecializationPhase<'db, 'expr, B, Infallible>>; 2],
        [OwnerSlot<'db, 'expr, B>; 3],
    );
    type QuotationCarriers<'expr> = (
        [usize; 60],
        [Option<usize>; 60],
        [Option<(usize, usize)>; 4],
        [RunError; 2],
        [bool; 6],
        [&'expr (); 8],
    );

    // Admit the quotation before reading storage or performing checked arithmetic, including
    // when the insertion quote overflows. The extra unit constructs the growth/quote result.
    let (grows, quote) = effects
        .local_with_fixed_transfers(31, size_of::<QuotationCarriers<'expr>>(), || {
            let len = owners.slots.len();
            let grows = len == owners.slots.capacity();
            // These carriers include the kind through the driver and adapters, the target and
            // request, and each state wrapper through slot installation, even without growth.
            let fixed_bytes = size_of::<InsertionCarriers<'db, 'expr, <A::Resources as RelationResourceAccess<'run, 'db>>::Builder>>();
            let quote = if grows {
                len.checked_mul(2).and_then(|count| count.checked_add(1))
            } else {
                Some(0)
            }
            .and_then(|count| count.checked_mul(size_of::<OwnerSlot<'db, 'expr, <A::Resources as RelationResourceAccess<'run, 'db>>::Builder>>()))
            .and_then(|bytes| bytes.checked_add(fixed_bytes))
            .and_then(|bytes| {
                // Preserve the eight insertion units, add twenty-one fixed transfers and one
                // target selection. Growth also relocates each old slot.
                let work = if grows { len.checked_add(30) } else { Some(30) };
                work.map(|work| (work, bytes))
            })
            .ok_or(RunError::Contract("specialization owner quotation overflow"));
            (grows, quote)
        })
        .await?;
    effects
        .local_quoted_with_fixed_transfers(quote, || {
            if grows {
                owners.slots.reserve_exact(1);
            }
            owners.push_specialization(
                builder,
                specialization::Request {
                    subscript,
                    value_ty,
                    generic_context,
                    target: match kind {
                        ClassSpecializationKind::ClassObject => SpecializationTarget::ClassObject(class),
                        ClassSpecializationKind::Subclass => SpecializationTarget::ClassSubclass(class),
                    },
                    protocol_guard: Some(class),
                },
            )
        })
        .await
}
