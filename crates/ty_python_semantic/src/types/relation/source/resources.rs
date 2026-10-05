//! Source relation owners outlive every task in their registered execution run.

use std::marker::PhantomData;
use std::sync::OnceLock;
use rustc_hash::FxHashMap;
use crate::FxIndexMap;
use crate::types::BoundTypeVarInstance;
use crate::types::callable::CallableType;
use crate::types::constraints::control::hash_slots;
use crate::types::mapping::return_callables::{RetainedReturnCallables, RetainedReturnTypevars, ReturnCallableMap, ReturnTypevarMap};

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::owned_constraints;
use super::retained::{CheckerStorage, UnavailablePairs};
use super::{
    BorrowedPairs, FreshRelation, RelationSourceEffects, RelationSourceOperation,
    RetainedRelationSource, materialization_guard_bytes,
};
use crate::types::ApplyTypeMappingVisitor;
use crate::types::class_base::specialization::ClassBaseMapping;
use crate::types::constraints::source::SourceStructuralResult;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintSet};
use crate::types::generics::prefix::{DefaultArgumentBuffer, DefaultArgumentSlots};
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::mapping::OwnedTypeMapping;
use crate::types::mapping::source::{
    MappingResourceAccess, MappingSourceEffects, RetainedMappingSource,
    apply_class_base_mapping_with_retained, apply_mapping_with_retained,
    apply_parameter_mapping_with_retained, apply_callable_signature_mapping_with_retained,
    apply_signature_mapping_with_retained, apply_retained_bounds_with_retained,
    compose_specialization_with_retained,
};
use crate::types::relation::stable_storage::StableStorage;
use crate::types::relation::{
    DirectionalEquivalenceEffects, EquivalenceChecker, RelationOwners, directional_equivalence_with,
};
use crate::types::typevar::TypeVarSet;
use crate::types::signatures::{CallableSignature, Parameters, Signature};
use crate::types::{ClassBase, ClassType, Specialization, Type};
use crate::{Db, Program, ProgramEnvironment};

#[cfg(test)]
pub(in crate::types) mod observations;

pub(in crate::types) struct SourceOwners<'env, 'c, 'db> {
    relation: RelationOwners<'env, 'c, 'db>,
    #[cfg(test)]
    _redundancy_lifetime: Option<super::redundancy_observations::OwnersLifetime>,
    #[cfg(test)]
    _disjointness_lifetime: Option<super::disjointness_observations::OwnersLifetime>,
}

/// Owns immutable return-callable maps until the execution run and all mapping visitors retire.
/// Declare this storage before the visitors and registered run that can borrow its maps.
pub(in crate::types) struct ReturnCallableMappingStorage<'db> {
    typevars: StableStorage<ReturnTypevarMap<'db>>,
    callables: StableStorage<ReturnCallableMap<'db>>,
}

impl<'db> ReturnCallableMappingStorage<'db> {
    pub(in crate::types) fn new() -> Self {
        Self { typevars: StableStorage::new(), callables: StableStorage::new() }
    }
}

/// The pools are declared separately in dependency order; this capability owns only references.
/// Builder and visitor lifetimes remain independent of the registered execution run.
#[derive(Clone, Copy)]
pub(in crate::types) struct SourceResources<'pool, 'owner, 'env, 'c, 'db> {
    environments: &'env StableStorage<ProgramEnvironment<'db>>,
    builders: &'c StableStorage<ConstraintSetBuilder<'db>>,
    owners: &'owner StableStorage<SourceOwners<'env, 'c, 'db>>,
    mapping: &'owner StableStorage<ApplyTypeMappingVisitor<'owner, 'db>>,
    checkers: &'pool CheckerStorage<'owner, 'c, 'db>,
    default_arguments: &'owner StableStorage<DefaultArgumentSlots<'db>>,
    return_callables: &'owner ReturnCallableMappingStorage<'db>,
}

impl<'pool, 'owner, 'env, 'c, 'db> SourceResources<'pool, 'owner, 'env, 'c, 'db> {
    pub(in crate::types) fn new(
        environments: &'env StableStorage<ProgramEnvironment<'db>>,
        builders: &'c StableStorage<ConstraintSetBuilder<'db>>,
        owners: &'owner StableStorage<SourceOwners<'env, 'c, 'db>>,
        mapping: &'owner StableStorage<ApplyTypeMappingVisitor<'owner, 'db>>,
        checkers: &'pool CheckerStorage<'owner, 'c, 'db>,
        default_arguments: &'owner StableStorage<DefaultArgumentSlots<'db>>,
        return_callables: &'owner ReturnCallableMappingStorage<'db>,
    ) -> Self {
        Self {
            environments,
            builders,
            owners,
            mapping,
            checkers,
            default_arguments,
            return_callables,
        }
    }

    fn retain_environment(
        self,
        endpoint: &TaskEndpoint<'_, 'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<&'env ProgramEnvironment<'db>> {
        // ProgramEnvironment contains only a source identity and its lazy resolution cache.
        // Retaining that identity does not copy comparison state or create query dependencies.
        self.environments.allocate_admitted(
            endpoint,
            2,
            || env.clone(),
        )
    }

    fn allocate_builder(
        self,
        endpoint: &TaskEndpoint<'_, 'db>,
    ) -> RunResult<&'c ConstraintSetBuilder<'db>> {
        // Descendant mutations admit their own work, including eventual owner disposal.
        self.builders.allocate_admitted(
            endpoint,
            2,
            ConstraintSetBuilder::new,
        )
    }

    fn allocate_mapping_visitor(
        self,
        endpoint: &TaskEndpoint<'_, 'db>,
        program: Program<'db>,
    ) -> RunResult<&'owner ApplyTypeMappingVisitor<'owner, 'db>>
    where
        'env: 'owner,
    {
        let env = self.environments.allocate_admitted(
            endpoint,
            2,
            || ProgramEnvironment::from_program(program),
        )?;
        self.mapping.allocate_admitted(
            endpoint,
            2,
            || ApplyTypeMappingVisitor::new(env),
        )
    }

    fn retain_owners(
        self,
        _db: &'db dyn Db,
        endpoint: &TaskEndpoint<'_, 'db>,
        env: &'env ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        _relation: Option<FreshRelation>,
    ) -> RunResult<&'owner RelationOwners<'env, 'c, 'db>> {
        let owners = self.owners.allocate_admitted(
            endpoint,
            2,
            || SourceOwners {
                relation: RelationOwners::new(env, constraints),
                #[cfg(test)]
                _redundancy_lifetime: matches!(_relation, Some(FreshRelation::Redundancy))
                    .then(|| super::redundancy_observations::owners_ready(_db)),
                #[cfg(test)]
                _disjointness_lifetime: matches!(
                    _relation,
                    Some(FreshRelation::Disjointness { .. })
                )
                .then(|| super::disjointness_observations::owners_ready(_db)),
            },
        )?;
        Ok(&owners.relation)
    }

    async fn fresh_owners<'run>(
        self,
        db: &'db dyn Db,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &ProgramEnvironment<'db>,
        relation: Option<FreshRelation>,
    ) -> RunResult<(
        &'c ConstraintSetBuilder<'db>,
        &'owner RelationOwners<'env, 'c, 'db>,
    )>
    where
        'db: 'run,
    {
        local_with_fixed_transfers_at(endpoint, 3, 0, || {
            let env = self.retain_environment(endpoint, env)?;
            let constraints = self.allocate_builder(endpoint)?;
            let owners = self.retain_owners(db, endpoint, env, constraints, relation)?;
            Ok((constraints, owners))
        })
        .await?
    }
}

pub(in crate::types) trait EnvironmentResourceAccess<'run, 'db: 'run>:
    Copy + 'run
{
    async fn retain_environment(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<&'run ProgramEnvironment<'db>>;
}

impl<'run, 'pool: 'run, 'owner: 'pool, 'env: 'owner, 'c: 'owner, 'db: 'run>
    EnvironmentResourceAccess<'run, 'db> for SourceResources<'pool, 'owner, 'env, 'c, 'db>
{
    async fn retain_environment(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<&'run ProgramEnvironment<'db>> {
        Ok(endpoint
            .local_call(|| self.retain_environment(endpoint, env))
            .await)
    }
}

/// Selects the fresh direct-class comparison used by metaclass reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ClassRelation {
    Subtyping,
    Assignability,
}

/// Keeps concrete pool and invariant builder lifetimes inside the resource implementation.
pub(in crate::types) trait RelationResourceAccess<'run, 'db: 'run>:
    EnvironmentResourceAccess<'run, 'db>
{
    type Builder: Copy + std::borrow::Borrow<ConstraintSetBuilder<'db>> + 'run;

    async fn invocation_builder(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
    ) -> RunResult<Self::Builder>;

    /// Produces the owned lazy-assignability condition, retaining the original comparison owners.
    /// Nonterminal results require compaction and remain unavailable.
    async fn owned_assignability<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> RunResult<OwnedConstraintSet<'db>>;

    /// Intersects terminal owned sets in a fresh retained builder; rejects stored conditions before loading.
    async fn intersect_owned_terminals<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        first: &OwnedConstraintSet<'db>,
        second: &OwnedConstraintSet<'db>,
        effects: &E,
    ) -> RunResult<SourceStructuralResult<OwnedConstraintSet<'db>>>;

    #[expect(clippy::too_many_arguments)]
    async fn assignability<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: Self::Builder,
        source: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
        always: bool,
        effects: &E,
    ) -> RunResult<bool>;

    async fn condition<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        relation: FreshRelation,
        effects: &E,
    ) -> RunResult<bool>;

    /// Compares classes with fresh constraints and visitors, returning whether the relation holds.
    /// Terminal constraints yield `true` or `false`; nonterminal satisfaction remains unavailable.
    async fn class_condition<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
        relation: ClassRelation,
        effects: &E,
    ) -> RunResult<bool>;

    async fn equivalence<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> RunResult<bool>;
}

impl<'run, 'pool: 'run, 'owner: 'pool, 'env: 'owner, 'c: 'owner, 'db: 'run>
    MappingResourceAccess<'run, 'db> for SourceResources<'pool, 'owner, 'env, 'c, 'db>
{
    async fn apply_signature_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        signature: &Signature<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<Signature<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            local_with_fixed_transfers_at(
                endpoint, 2, 0, || self.allocate_mapping_visitor(endpoint, program),
            ).await??
        };
        apply_signature_mapping_with_retained(db, signature, program, mapping, visitor, source).await
    }

    async fn retain_return_typevars(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        mut values: FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
    ) -> RunResult<RetainedReturnTypevars<'run, 'db>> {
        crate::types::local_transfer::local_with_fixed_transfers_at(
            endpoint, 8,
size_of::<(Self, &TaskEndpoint<'run, 'db>, &mut FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>)>() * 2
                + size_of::<FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>>() * 2
                + size_of::<RetainedReturnTypevars<'run, 'db>>() * 2
                + size_of::<RunResult<&ReturnTypevarMap<'db>>>() * 2,
            || (),
        ).await?;
        // The callback borrows this future's map. Refused arena admission leaves it here
        // until local_call has drained children; successful admission moves its backing once.
        let owner = endpoint.local_call(|| {
            endpoint.admit_work(12)?;
            let slots = hash_slots::<RunError>(values.capacity()).map_err(|_| RunError::Contract("return typevar map retention overflow"))?;
            let work = values.len().checked_mul(3)
                .and_then(|work| work.checked_add(slots))
                .and_then(|work| work.checked_add(4))
                .ok_or(RunError::Contract("return typevar map retirement overflow"))?;
            self.return_callables.typevars.allocate_admitted(endpoint, work, || ReturnTypevarMap {
                values: std::mem::take(&mut values),
            })
        }).await;
        Ok(RetainedReturnTypevars::new(owner))
    }

    async fn retain_return_callables(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        mut values: FxHashMap<CallableType<'db>, CallableType<'db>>,
    ) -> RunResult<RetainedReturnCallables<'run, 'db>> {
        crate::types::local_transfer::local_with_fixed_transfers_at(
            endpoint, 8,
size_of::<(Self, &TaskEndpoint<'run, 'db>, &mut FxHashMap<CallableType<'db>, CallableType<'db>>)>() * 2
                + size_of::<FxHashMap<CallableType<'db>, CallableType<'db>>>() * 2
                + size_of::<RetainedReturnCallables<'run, 'db>>() * 2
                + size_of::<RunResult<&ReturnCallableMap<'db>>>() * 2,
            || (),
        ).await?;
        let owner = endpoint.local_call(|| {
            endpoint.admit_work(12)?;
            let slots = hash_slots::<RunError>(values.capacity()).map_err(|_| RunError::Contract("return callable map retention overflow"))?;
            let work = values.len().checked_mul(2)
                .and_then(|work| work.checked_add(slots))
                .and_then(|work| work.checked_add(4))
                .ok_or(RunError::Contract("return callable map retirement overflow"))?;
            self.return_callables.callables.allocate_admitted(endpoint, work, || ReturnCallableMap {
                values: std::mem::take(&mut values),
            })
        }).await;
        Ok(RetainedReturnCallables::new(owner))
    }

    async fn apply_callable_signature_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        signatures: &CallableSignature<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<CallableSignature<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            crate::types::local_transfer::local_with_fixed_transfers_at(
                endpoint, 2, 0, || self.allocate_mapping_visitor(endpoint, program),
            ).await??
        };
        apply_callable_signature_mapping_with_retained(db, signatures, program, mapping, visitor, source).await
    }

    async fn default_arguments(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        len: usize,
    ) -> RunResult<DefaultArgumentBuffer<'run, 'db>> {
        Ok(endpoint
            .local_call(|| {
                let bytes =
                    len.checked_mul(size_of::<OnceLock<Type<'db>>>())
                        .ok_or(RunError::Contract(
                            "default-argument slot allocation overflow",
                        ))?;
                let work = bytes
                    .checked_mul(2)
                    .and_then(|work| work.checked_add(len.checked_mul(4)?))
                    .and_then(|work| {
                        work.checked_add(size_of::<DefaultArgumentSlots<'db>>() * 2 + 4)
                    })
                    .ok_or(RunError::Contract("default-argument slot work overflow"))?;
                // StableStorage admits the owner itself; the fixed slot array is its separately
                // admitted backing. All slots and their eventual disposal are covered here.
                endpoint.admit_work(work)?;
                if bytes != 0 {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: bytes,
                    })?;
                }
                endpoint.check_completion()?;
                let owner = self
                    .default_arguments
                    .allocate_admitted(endpoint, 1, || DefaultArgumentSlots::new(len))?;
                Ok(owner.buffer())
            })
            .await)
    }

    async fn apply_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<Type<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            crate::types::local_transfer::local_with_fixed_transfers_at(
                endpoint, 2, 0, || self.allocate_mapping_visitor(endpoint, program),
            ).await??
        };
        apply_mapping_with_retained(db, ty, program, mapping, visitor, source).await
    }

    async fn apply_retained_bounds<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        bounds: crate::types::TypeVarBoundOrConstraints<'db>,
        program: Program<'db>,
        specialization: Specialization<'db>,
        source: R,
    ) -> RunResult<crate::types::TypeVarBoundOrConstraints<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            local_with_fixed_transfers_at(endpoint, 2, 0, || self.allocate_mapping_visitor(endpoint, program)).await??
        };
        apply_retained_bounds_with_retained(db, bounds, program, specialization, visitor, source).await
    }

    async fn apply_parameter_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        parameters: &Parameters<'db>,
        program: Program<'db>,
        mapping: OwnedTypeMapping<'run, 'db>,
        source: R,
    ) -> RunResult<Parameters<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            crate::types::local_transfer::local_with_fixed_transfers_at(
                endpoint, 2, 0, || self.allocate_mapping_visitor(endpoint, program),
            ).await??
        };
        apply_parameter_mapping_with_retained(db, parameters, program, mapping, visitor, source).await
    }

    async fn apply_class_base_mapping<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        base: ClassBase<'db>,
        program: Program<'db>,
        mapping: ClassBaseMapping<'db>,
        source: R,
    ) -> RunResult<ClassBase<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            endpoint
                .local_call(|| self.allocate_mapping_visitor(endpoint, program))
                .await
        };
        apply_class_base_mapping_with_retained(db, base, program, mapping, visitor, source).await
    }
    async fn compose_specialization<R: RetainedMappingSource<'run, 'db>>(
        self,
        db: &'db dyn Db,
        base: Specialization<'db>,
        additional: Specialization<'db>,
        program: Program<'db>,
        source: R,
    ) -> RunResult<Specialization<'db>> {
        let visitor = {
            let effects = source.effects();
            let endpoint = effects.endpoint();
            endpoint
                .local_call(|| self.allocate_mapping_visitor(endpoint, program))
                .await
        };
        compose_specialization_with_retained(db, base, additional, program, visitor, source).await
    }
}

impl<'run, 'pool: 'run, 'owner: 'pool, 'env: 'owner, 'c: 'owner, 'db: 'run>
    RelationResourceAccess<'run, 'db> for SourceResources<'pool, 'owner, 'env, 'c, 'db>
{
    type Builder = &'c ConstraintSetBuilder<'db>;

    async fn owned_assignability<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> RunResult<OwnedConstraintSet<'db>> {
        let endpoint = effects.endpoint();
        let (constraints, owners) = self.fresh_owners(db, endpoint, env, None).await?;
        #[cfg(test)]
        super::receiver_constraint_observations::observe_before(
            super::receiver_constraint_observations::Stage::Owner,
        );
        let checker = self.checkers.allocate_with_fixed_transfers(endpoint, || {
            let checker = owners.constraint_set_assignability();
            #[cfg(test)]
            observations::observe_assignability_root(&checker, false);
            #[cfg(test)]
            super::receiver_constraint_observations::observe_after(
                super::receiver_constraint_observations::Stage::Owner,
            );
            checker
        }).await?;
        let access = retained_source(effects).await?;
        let result = owned_constraints::produce(db, endpoint, checker, access, source, target).await?;
        #[cfg(test)]
        observations::observe_assignability_result(constraints, result, false);
        owned_constraints::package_terminal(constraints, result, effects).await
    }

    async fn intersect_owned_terminals<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        first: &OwnedConstraintSet<'db>,
        second: &OwnedConstraintSet<'db>,
        effects: &E,
    ) -> RunResult<SourceStructuralResult<OwnedConstraintSet<'db>>> {
        let endpoint = effects.endpoint();
        let terminals = local_with_fixed_transfers_at(endpoint, 4, 0, || {
            first.terminal().zip(second.terminal())
        }).await?;
        let Some((first, second)) = terminals else {
            #[cfg(test)]
            super::receiver_constraint_observations::observe_rejected_merge();
            return local_with_fixed_transfers_at(endpoint, 1, 0, || {
                SourceStructuralResult::Unsupported
            }).await;
        };
        let builder = local_with_fixed_transfers_at(endpoint, 1, 0, || {
            self.allocate_builder(endpoint)
        }).await??;
        // Terminal load, builder checks, and conjunction take at most 32 fixed operations.
        // Both roots have no source history, so this path cannot allocate graph or source-order storage.
        #[cfg(test)]
        super::receiver_constraint_observations::observe_before(
            super::receiver_constraint_observations::Stage::Merge,
        );
        let result = local_with_fixed_transfers_at(endpoint, 32, 0, || {
            #[cfg(test)]
            super::receiver_constraint_observations::observe_merge_load();
            let result = builder.load_terminal(first).and(db, builder, || {
                #[cfg(test)]
                super::receiver_constraint_observations::observe_merge_load();
                builder.load_terminal(second)
            });
            #[cfg(test)]
            super::receiver_constraint_observations::observe_after(
                super::receiver_constraint_observations::Stage::Merge,
            );
            result
        }).await?;
        let owned = owned_constraints::package_terminal(builder, result, effects).await?;
        local_with_fixed_transfers_at(endpoint, 1, 0, || {
            SourceStructuralResult::Complete(owned)
        }).await
    }

    async fn invocation_builder(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
    ) -> RunResult<Self::Builder> {
        Ok(endpoint
            .local_call(|| {
                let builder = self.allocate_builder(endpoint)?;
                #[cfg(test)]
                observations::observe_invocation_allocation(builder);
                Ok(builder)
            })
            .await)
    }

    async fn assignability<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: Self::Builder,
        source: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
        always: bool,
        effects: &E,
    ) -> RunResult<bool> {
        let endpoint = effects.endpoint();
        let owners = endpoint
            .local_call(|| {
                let env = self.retain_environment(endpoint, env)?;
                self.retain_owners(db, endpoint, env, constraints, None)
            })
            .await;
        let checker = self
            .checkers
            .allocate(endpoint, || {
                let checker = owners.assignability(inferable);
                #[cfg(test)]
                observations::observe_assignability_root(&checker, always);
                checker
            })
            .await;
        let access = retained_source(effects).await?;
        let result = checker.pair(db, access, source, target).await?;
        #[cfg(test)]
        observations::observe_assignability_result(constraints, result, always);
        BorrowedPairs {
            children: &UnavailablePairs,
            db,
            endpoint,
            effects,
            constraints,
        }
        .satisfy(result, always)
        .await
    }

    async fn condition<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        relation: FreshRelation,
        effects: &E,
    ) -> RunResult<bool> {
        let endpoint = effects.endpoint();
        let (constraints, owners) = self.fresh_owners(db, endpoint, env, Some(relation)).await?;
        let pairs = BorrowedPairs {
            children: &UnavailablePairs,
            db,
            endpoint,
            effects,
            constraints,
        };
        let result = match relation {
            FreshRelation::Subtyping | FreshRelation::Assignability | FreshRelation::Redundancy => {
                let checker = self
                    .checkers
                    .allocate(endpoint, || match relation {
                        FreshRelation::Subtyping => owners.subtyping(TypeVarSet::None),
                        FreshRelation::Assignability => owners.assignability(TypeVarSet::None),
                        _ => owners.redundancy(),
                    })
                    .await;
                let access = retained_source(effects).await?;
                checker.pair(db, access, source, target).await?
            }
            FreshRelation::Disjointness {
                perform_expensive_checks,
            } => {
                let checker = self
                    .checkers
                    .allocate_disjointness(endpoint, || {
                        let mut checker = owners.disjointness(TypeVarSet::None);
                        checker.perform_expensive_checks = perform_expensive_checks;
                        checker
                    })
                    .await;
                let access = retained_source(effects).await?;
                checker.disjoint_pair(db, access, source, target).await?
            }
        };
        pairs.satisfy(result, true).await
    }

    async fn class_condition<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
        relation: ClassRelation,
        effects: &E,
    ) -> RunResult<bool> {
        let endpoint = effects.endpoint();
        endpoint
            .local_call(|| {
                let requested_bytes = size_of::<(
                    &'c ConstraintSetBuilder<'db>,
                    &'owner RelationOwners<'env, 'c, 'db>,
                )>()
                .checked_mul(2)
                .ok_or(RunError::Contract("class owner handles quotation overflow"))?;
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })
            })
            .await;
        let (constraints, owners) = self.fresh_owners(db, endpoint, env, None).await?;
        let checker = self
            .checkers
            .allocate(endpoint, || {
                let checker = match relation {
                    ClassRelation::Subtyping => owners.subtyping(TypeVarSet::None),
                    ClassRelation::Assignability => owners.assignability(TypeVarSet::None),
                };
                #[cfg(test)]
                observations::observe_class_condition_root(&checker);
                checker
            })
            .await;
        let access = retained_source(effects).await?;
        let result = checker.class_pair(db, endpoint, access, source, target).await?;
        #[cfg(test)]
        observations::observe_class_condition_result(constraints, result);
        let pairs = endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<BorrowedPairs<'_, 'run, 'db, 'c, E, UnavailablePairs>>()
                        .checked_mul(2)
                        .ok_or(RunError::Contract("class result adapter quotation overflow"))?,
                })?;
                Ok(BorrowedPairs {
                    children: &UnavailablePairs,
                    db,
                    endpoint,
                    effects,
                    constraints,
                })
            })
            .await;
        pairs.satisfy(result, true).await
    }

    async fn equivalence<E: RelationSourceEffects<'run, 'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> RunResult<bool> {
        let endpoint = effects.endpoint();
        let (constraints, owners) = self.fresh_owners(db, endpoint, env, None).await?;
        let checker = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<EquivalenceChecker<'owner, 'c, 'db>>() * 2)?;
                Ok(owners.equivalence())
            })
            .await;
        let access = retained_source(effects).await?;
        let provider = RetainedEquivalence {
            db,
            resources: self,
            access,
            lifetime: PhantomData,
        };
        let result = directional_equivalence_with(&checker, source, target, &provider).await?;
        BorrowedPairs {
            children: &UnavailablePairs,
            db,
            endpoint,
            effects,
            constraints,
        }
        .satisfy(result, true)
        .await
    }
}

async fn retained_source<'run, 'db: 'run, E: RelationSourceEffects<'run, 'db>>(
    effects: &E,
) -> RunResult<E::Retained> {
    local_with_fixed_transfers_at(effects.endpoint(), 2, 0, || effects.retained()).await
}

struct RetainedEquivalence<'run, 'pool, 'owner, 'env, 'c, 'db, R> {
    db: &'db dyn Db,
    resources: SourceResources<'pool, 'owner, 'env, 'c, 'db>,
    access: R,
    lifetime: PhantomData<&'run ()>,
}

impl<
    'run,
    'pool: 'run,
    'owner: 'pool,
    'env: 'owner,
    'c: 'owner,
    'db: 'run,
    R: RetainedRelationSource<'run, 'db>,
> DirectionalEquivalenceEffects<'owner, 'c, 'db>
    for RetainedEquivalence<'run, 'pool, 'owner, 'env, 'c, 'db, R>
{
    type Error = RunError;

    async fn direction(
        &self,
        checker: &EquivalenceChecker<'owner, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let effects = self.access.effects();
        let endpoint = effects.endpoint();
        let visitor = endpoint
            .local_call(|| {
                let guard_bytes = materialization_guard_bytes()?;
                let work = size_of::<ApplyTypeMappingVisitor<'owner, 'db>>()
                    .checked_add(guard_bytes)
                    .and_then(|size| size.checked_mul(2))
                    .ok_or(RunError::Contract(
                        "materialization owner quotation overflow",
                    ))?;
                if checker
                    .materialization_visitor
                    .materialization_equivalence
                    .get()
                    .is_none()
                {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: guard_bytes,
                    })?;
                }
                // Each direction starts with empty caches and shares the original recursion guard.
                let visitor = self
                    .resources
                    .mapping
                    .allocate_admitted(endpoint, work, || {
                        checker
                            .materialization_visitor
                            .for_new_materialization_root()
                    })?;
                #[cfg(test)]
                observations::observe(self.db, checker, visitor);
                Ok(visitor)
            })
            .await;
        let retained = self
            .resources
            .checkers
            .allocate(endpoint, || checker.as_relation_checker(visitor))
            .await;
        let access = retained_source(&effects).await?;
        retained.pair(self.db, access, source, target).await
    }

    async fn is_never(
        &self,
        checker: &EquivalenceChecker<'owner, 'c, 'db>,
        value: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        let effects = self.access.effects();
        let endpoint = effects.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                value.verify_builder(checker.constraints);
                Ok(value.is_trivially_never_satisfied())
            })
            .await)
    }

    async fn conjoin(
        &self,
        checker: &EquivalenceChecker<'owner, 'c, 'db>,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let effects = self.access.effects();
        let endpoint = effects.endpoint();
        let terminals = endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                left.verify_builder(checker.constraints);
                right.verify_builder(checker.constraints);
                Ok(left.to_owned_terminal().is_some() && right.to_owned_terminal().is_some())
            })
            .await;
        if !terminals {
            return effects
                .unavailable(RelationSourceOperation::ConstraintCombination)
                .await;
        }
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                Ok(left.and(self.db, checker.constraints, || right))
            })
            .await)
    }
}
