//! Owned receiver conditions use the canonical lazy relation query and its original solver.

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::local_transfer::local_quoted_with_fixed_transfers_at;
use crate::types::relation::source::RelationSourceEffects;
use crate::types::relation::source::resources::RelationResourceAccess;
use crate::types::relation::{
    ConstraintSetRelationFacts, OwnedConstraintSetEffects, constraint_set_assignable_owned_with,
    trivially_constraint_set_assignable_with,
};
use crate::types::set_theoretic::builder::controlled_union::UnionEffects;
use crate::types::{IntersectionType, Type, TypePair, UnionType};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Returns owned conditions for assigning the receiver to its annotation.
    /// Preserves the ordinary shortcut and query order.
    pub(in crate::types::infer) async fn owned_receiver_constraints(
        &self,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        annotation: Type<'db>,
    ) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>> {
        self.receiver_constraint_child(|| self.environment_program(env))
            .await?;
        let result = self
            .receiver_constraint_child(|| {
                constraint_set_assignable_owned_with(
                    receiver,
                    annotation,
                    self,
                    ConstraintSetRelationFacts,
                )
            })
            .await?;
        self.local_with_fixed_transfers(3, 0, || result).await
    }

    /// Computes owned assignability conditions for the canonical query using a retained lazy checker.
    pub(in crate::types::infer) async fn type_pair_owned_assignability(
        &self,
        pair: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>> {
        let context = self
            .local_with_fixed_transfers(2, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(2, 0, || pair.field_requests(context))
            .await?;
        let program_request = self
            .local_with_fixed_transfers(2, 0, || fields.program())
            .await?;
        let program = self.field(program_request).await?;
        self.local_with_fixed_transfers(2, 0, || self.check_program(program))
            .await??;
        let first_request = self
            .local_with_fixed_transfers(2, 0, || fields.first())
            .await?;
        let first = self.field(first_request).await?;
        let second_request = self
            .local_with_fixed_transfers(2, 0, || fields.second())
            .await?;
        let second = self.field(second_request).await?;
        let env = self
            .local_with_fixed_transfers(4, 0, || ProgramEnvironment::from_program(program))
            .await?;
        let value = self
            .receiver_constraint_child(|| {
                self.access
                    .resources()
                    .owned_assignability(self.db(), &env, first, second, self)
            })
            .await?;
        #[cfg(test)]
        crate::types::relation::source::receiver_constraint_observations::observe_before(
            crate::types::relation::source::receiver_constraint_observations::Stage::Transfer,
        );
        self.local_with_fixed_transfers(3, 0, || {
            #[cfg(test)]
            crate::types::relation::source::receiver_constraint_observations::observe_after(
                crate::types::relation::source::receiver_constraint_observations::Stage::Transfer,
            );
            value
        })
        .await
    }

    /// Admits a semantic child and its return carriers before constructing the child future.
    pub(in crate::types::infer) async fn receiver_constraint_child<
        T,
        F: Future<Output = RunResult<T>>,
    >(
        &self,
        make: impl FnOnce() -> F,
    ) -> RunResult<T> {
        receiver_constraint_child_at(self.access.endpoint(), make).await
    }

    /// Quotes finite type comparisons without resolving either type's semantic children.
    async fn receiver_comparison_work(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<usize> {
        let work = self
            .local_with_fixed_transfers(2, 0, || {
                source
                    .inline_payload_bytes()
                    .checked_add(target.inline_payload_bytes())
                    .and_then(|work| work.checked_add(12))
            })
            .await?;
        Self::checked(work)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OwnedConstraintSetEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn trivially_assignable(&self, source: Type<'db>, target: Type<'db>) -> RunResult<bool> {
        let work = self.receiver_comparison_work(source, target).await?;
        self.local_with_fixed_transfers(work, 0, || ()).await?;
        self.receiver_constraint_child(|| {
            trivially_constraint_set_assignable_with(
                source,
                target,
                self,
                ConstraintSetRelationFacts,
            )
        })
        .await
    }

    async fn union_contains(&self, union: UnionType<'db>, source: Type<'db>) -> RunResult<bool> {
        let elements = self
            .receiver_constraint_child(|| RelationSourceEffects::union_elements(self, union))
            .await?;
        let mut elements = self
            .local_with_fixed_transfers(1, 0, || elements.iter())
            .await?;
        loop {
            let next = self
                .local_with_fixed_transfers(2, 0, || elements.next().copied())
                .await?;
            let Some(target) = next else {
                return self.local_with_fixed_transfers(1, 0, || false).await;
            };
            if self
                .receiver_constraint_child(|| UnionEffects::same_type(self, source, target))
                .await?
            {
                return self.local_with_fixed_transfers(1, 0, || true).await;
            }
        }
    }

    async fn intersection_contains(
        &self,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> RunResult<bool> {
        self.receiver_constraint_child(|| {
            RelationSourceEffects::intersection_positive_contains(self, intersection, target)
        })
        .await
    }

    async fn cached_owned_assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db OwnedConstraintSet<'db>> {
        self.receiver_constraint_child(|| {
            self.access
                .canonical_owned_assignability(self.program, source, target)
        })
        .await
    }
}

/// Constructs and awaits a receiver-constraint future after admitting its storage and transfers.
pub(in crate::types::infer) async fn receiver_constraint_child_at<
    T,
    F: Future<Output = RunResult<T>>,
>(
    endpoint: &TaskEndpoint<'_, '_>,
    make: impl FnOnce() -> F,
) -> RunResult<T> {
    let quote = size_of::<RunResult<T>>()
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(2)?))
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .map(|bytes| (6, bytes))
        .ok_or(RunError::Contract(
            "receiver constraint child quotation overflow",
        ));
    let future: Pin<Box<F>> =
        local_quoted_with_fixed_transfers_at(endpoint, quote, || Box::pin(make())).await?;
    future.await
}
