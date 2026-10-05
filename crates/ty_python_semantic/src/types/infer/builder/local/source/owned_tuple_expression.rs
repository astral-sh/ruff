//! Admitted tuple phases borrow their existing owner slot while semantic children suspend.

use super::*;
use crate::types::infer::builder::tuple_expression::{
    TupleExpressionFacts, advance_tuple_expression_with,
};

/// Installs a tuple cursor after funding its slot, any relocation, and eventual retirement.
pub(super) async fn start<'run, 'db: 'run, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builder: BuilderId,
    tuple: &'expr ast::ExprTuple,
    context: TypeContext<'db>,
) -> RunResult<tuple_expression::Active> {
    let grows = owners.slots.len() == owners.slots.capacity();
    let moved = if grows { owners.slots.len() } else { 0 };
    let allocation = if grows {
        SourceEffects::<A>::checked(owners.slots.len().checked_add(1))?
    } else {
        0
    };
    let bytes = SourceEffects::<A>::checked(
        moved
            .checked_add(allocation)
            .and_then(|count| count.checked_add(1))
            .and_then(|count| {
                count.checked_mul(size_of::<
                    OwnerSlot<
                        'db,
                        'expr,
                        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
                    >,
                >())
            })
            .and_then(|bytes| {
                bytes.checked_add(size_of::<tuple_expression::State<'db, 'expr>>() * 2)
            })
            .and_then(|bytes| bytes.checked_add(size_of::<tuple_expression::Active>() * 2)),
    )?;
    let work = SourceEffects::<A>::checked(
        moved
            .checked_mul(2)
            .and_then(|work| work.checked_add(allocation))
            .and_then(|work| work.checked_add(18)),
    )?;
    effects
        .local_with_fixed_transfers(work, bytes, || {
            if grows {
                owners.slots.reserve_exact(1);
            }
            let owner = owners.push_tuple_value(builder, tuple, context);
            #[cfg(all(test, feature = "experimental-analysis"))]
            {
                owners.tuple_value_lease(owner.0).state.preparation_lifetime = Some(
                    crate::types::infer::source_runtime::tests::contextual_tuple::OwnerLifetime::new(),
                );
            }
            owner
        })
        .await
}

/// Advances a tuple while its annotations remain in the owner slot, including on child failure.
pub(super) async fn step<'run, 'db: 'run, 'ast, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owner: tuple_expression::Active,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    builders: &mut BuilderStore<'_, 'db, 'ast>,
) -> RunResult<tuple_expression::Step<'db, 'expr>> {
    // These fixed carriers borrow the payload. Buffer construction and cleanup are admitted
    // by the operations that create the targets, resized specification, and annotation iterator.
    effects
        .local_with_fixed_transfers(
            12,
            size_of::<tuple_expression::Lease<'_, 'db, 'expr>>() * 2
                + size_of::<tuple_expression::Action<'db, 'expr>>() * 2
                + size_of::<tuple_expression::Step<'db, 'expr>>() * 2,
            || (),
        )
        .await?;
    effects
        .allocate_future(|| async move {
            let lease = owners.tuple_value_lease(owner.0);
            let action =
                advance_tuple_expression_with(lease.state, builders, TupleExpressionFacts, effects)
                    .await?;
            #[cfg(all(test, feature = "experimental-analysis"))]
            if let tuple_expression::Action::Infer {
                builder,
                expression,
                context,
            } = &action
            {
                effects
                    .local_with_fixed_transfers(1, 0, || {
                        crate::types::infer::source_runtime::tests::contextual_tuple::observe_element(
                            builders.builder(*builder).db(),
                            expression,
                            *context,
                        );
                    })
                    .await?;
            }
            Ok(LocalOwners::<
                'db,
                'expr,
                <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
            >::tuple_value_step(owner.0, action))
        })
        .await?
        .await
}

/// Restores the active token after the driver has stored the tuple element's result.
pub(super) async fn resume<'run, 'db: 'run, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owner: tuple_expression::Waiting,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
) -> RunResult<tuple_expression::Active> {
    effects
        .local_with_fixed_transfers(
            4,
            size_of::<tuple_expression::Active>() * 2
                + size_of::<tuple_expression::Lease<'_, 'db, 'expr>>(),
            || {
                if !matches!(
                    owners.tuple_value_lease(owner.0).state.phase,
                    tuple_expression::Phase::Waiting
                ) {
                    return Err(RunError::Contract(
                        "tuple expression resumed outside its child phase",
                    ));
                }
                Ok(tuple_expression::Active(owner.0))
            },
        )
        .await?
}

/// Returns the completed value and retires its prepaid tuple preparation buffers.
pub(super) async fn finish<'run, 'db: 'run, 'expr, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    owners: &mut LocalOwners<
        'db,
        'expr,
        <A::Resources as RelationResourceAccess<'run, 'db>>::Builder,
    >,
    owner: tuple_expression::Finished,
) -> RunResult<Type<'db>> {
    effects
        .local_with_fixed_transfers(
            5,
            size_of::<tuple_expression::Lease<'_, 'db, 'expr>>()
                + size_of::<FinishedOwner<'db>>()
                + size_of::<Type<'db>>() * 2,
            || owners.retire_tuple_value(owner),
        )
        .await
}
