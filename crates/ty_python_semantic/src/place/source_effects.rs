//! Dependencies of canonical source place reduction.
//!
//! Providers receive immutable prepared source inputs. Mutable reduction state belongs to the
//! calling continuation, so suspending a dependency never retains a scheduler storage borrow.

use std::convert::Infallible;
use std::future::{Future, ready};

use ruff_index::IndexSlice;
use ty_module_resolver::{KnownModule, file_to_module, resolve_module_confident};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::narrowing_constraints::NarrowingConstraints;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    PredicateNarrowingTargets, ProgramFile, Truthiness, global_scope, place_table,
};

use super::imported::{
    ImportedFallbackFacts, InlineImportedEffects, imported_fallback_sync, is_reexported_sync,
};
use super::{
    ConsideredDefinitions, LoopHeaderReachability, PlaceAndQualifiers, RequiresExplicitReExport,
};
use crate::reachability::{
    NarrowingProjector, ReachabilityEvaluationCache, evaluate_reachability_with_cache,
};
use crate::types::promotion::InlinePublicPromotionEffects;
use crate::types::{FunctionType, Type, TypeAndQualifiers, UnionBuilder, UnionType};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

pub(crate) mod sealed {
    pub(crate) trait Sealed {}
}

/// Public member conversion can promote inferred types and combine possible definitions.
pub(crate) trait PublicLookupEffects<'db>: sealed::Sealed {
    type Error;

    async fn promote_public_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourcePlaceWork {
    Start,
    BindingAdvance,
    DeclarationAdvance,
    ImportedFinalAdvance,
    Complete,
}

pub(crate) trait SourcePlaceEffects<'db>: PublicLookupEffects<'db> {
    async fn check_imported_file(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: ProgramFile<'db>,
    ) -> Result<(), Self::Error>;
    async fn file_is_stub(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<bool, Self::Error>;
    async fn reduction_checkpoint(&self, _work: SourcePlaceWork) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;
    async fn definition_is_reexported(
        &self,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;
    async fn function_is_overload(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;
    async fn union_builder(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error>;
    async fn narrowing_projector<'map>(
        &self,
        env: &'map ProgramEnvironment<'db>,
        constraints: &'map NarrowingConstraints,
        predicates: &'map IndexSlice<ScopedPredicateId, Predicate<'db>>,
        targets: &'map PredicateNarrowingTargets,
        binding: Definition<'db>,
        base_ty: Type<'db>,
    ) -> Result<NarrowingProjector<'map, 'db>, Self::Error>
    where
        'db: 'map;
    async fn symbol_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error>;
    async fn is_known_module(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        module: KnownModule,
    ) -> Result<bool, Self::Error>;
    async fn place_by_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn global_scope(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<ScopeId<'db>, Self::Error>;
    async fn resolve_known_module(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: KnownModule,
    ) -> Result<Option<ProgramFile<'db>>, Self::Error>;
    async fn imported_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: PlaceAndQualifiers<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn is_reexported(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
    async fn inferred_declaration(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<TypeAndQualifiers<'db>>, Self::Error>;
    async fn binding_type(&self, definition: Definition<'db>) -> Result<Type<'db>, Self::Error>;
    async fn is_discarded_dict_key_assignment(
        &self,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;
    async fn loop_header_reachability(
        &self,
        definition: Definition<'db>,
    ) -> Result<LoopHeaderReachability<'db>, Self::Error>;
    async fn reachability(
        &self,
        cache: Option<&ReachabilityEvaluationCache<'db>>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        constraint: ScopedReachabilityConstraintId,
    ) -> Result<Truthiness, Self::Error>;
    async fn narrow(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ty_python_core::narrowing_constraints::ScopedNarrowingConstraint,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn union_add(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error>;
    async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    async fn function_same_place(
        &self,
        function: FunctionType<'db>,
        other: FunctionType<'db>,
    ) -> Result<bool, Self::Error>;
    async fn function_contains(
        &self,
        function: FunctionType<'db>,
        other: FunctionType<'db>,
    ) -> Result<bool, Self::Error>;
    async fn equivalent(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error>;
    async fn preserve_raw_public_type(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Result<bool, Self::Error>;
}

/// Literal reachability constraints do not read predicates or infer source expressions.
pub(crate) async fn reachability_with<'db, E: SourcePlaceEffects<'db>>(
    effects: &E,
    cache: Option<&ReachabilityEvaluationCache<'db>>,
    constraints: &ReachabilityConstraints,
    predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
    constraint: ScopedReachabilityConstraintId,
) -> Result<Truthiness, E::Error> {
    match constraint {
        ScopedReachabilityConstraintId::ALWAYS_TRUE => Ok(Truthiness::AlwaysTrue),
        ScopedReachabilityConstraintId::ALWAYS_FALSE => Ok(Truthiness::AlwaysFalse),
        ScopedReachabilityConstraintId::AMBIGUOUS => Ok(Truthiness::Ambiguous),
        _ => {
            effects
                .reachability(cache, constraints, predicates, constraint)
                .await
        }
    }
}

pub(crate) struct LegacyInlineEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> LegacyInlineEffects<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for LegacyInlineEffects<'_> {}

impl<'db> PublicLookupEffects<'db> for LegacyInlineEffects<'db> {
    type Error = Infallible;

    fn promote_public_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(ty.promote_public_sync(db, env, &InlinePublicPromotionEffects))
    }

    fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(UnionType::from_two_elements(db, env, first, second)))
    }
}

impl<'db> SourcePlaceEffects<'db> for LegacyInlineEffects<'db> {
    fn check_imported_file(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: ProgramFile<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        debug_assert_eq!(file.program(db), env.program(db));
        ready(Ok(()))
    }
    fn file_is_stub(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(file.file(db).is_stub(db)))
    }
    fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<&'db DefinitionKind<'db>, Self::Error>> {
        ready(Ok(definition.kind(self.db)))
    }
    fn definition_is_reexported(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(definition.is_reexported(self.db)))
    }
    fn function_is_overload(
        &self,
        function: FunctionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(function
            .literal(self.db)
            .last_definition
            .is_overload(self.db)))
    }
    fn union_builder(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Self::Error>> {
        ready(Ok(UnionBuilder::new(self.db, env)))
    }
    async fn narrowing_projector<'map>(
        &self,
        env: &'map ProgramEnvironment<'db>,
        constraints: &'map NarrowingConstraints,
        predicates: &'map IndexSlice<ScopedPredicateId, Predicate<'db>>,
        targets: &'map PredicateNarrowingTargets,
        binding: Definition<'db>,
        base_ty: Type<'db>,
    ) -> Result<NarrowingProjector<'map, 'db>, Self::Error>
    where
        'db: 'map,
    {
        Ok(NarrowingProjector::new(
            self.db,
            env,
            constraints,
            predicates,
            targets,
            binding.place(self.db),
            base_ty,
        ))
    }

    fn symbol_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        name: &str,
    ) -> impl Future<Output = Result<Option<ScopedSymbolId>, Self::Error>> {
        ready(Ok(place_table(db, scope).symbol_id(name)))
    }
    fn is_known_module(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        module: KnownModule,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(file_to_module(
            db,
            scope.program_file(db).resolver_file(db),
        )
        .is_some_and(|resolved| resolved.is_known(db, module))))
    }
    fn place_by_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(Ok(super::place_by_id(
            db, scope, place, reexport, considered,
        )))
    }
    fn global_scope(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> impl Future<Output = Result<ScopeId<'db>, Self::Error>> {
        ready(Ok(global_scope(db, file)))
    }
    fn resolve_known_module(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: KnownModule,
    ) -> impl Future<Output = Result<Option<ProgramFile<'db>>, Self::Error>> {
        ready(Ok(resolve_module_confident(
            db,
            env.resolver_environment(db),
            &module.name(),
        )
        .and_then(|module| {
            Some(ProgramFile::new(db, module.file(db)?, env.program(db)))
        })))
    }
    fn imported_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: PlaceAndQualifiers<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(imported_fallback_sync(
            db,
            env,
            prior,
            file,
            name,
            ImportedFallbackFacts,
            &InlineImportedEffects { db },
        ))
    }
    fn is_reexported(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = self.db;
        ready(is_reexported_sync(
            definition,
            &InlineImportedEffects { db },
        ))
    }
    fn inferred_declaration(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<Option<TypeAndQualifiers<'db>>, Self::Error>> {
        let db = self.db;
        ready(Ok(
            crate::types::inferred_declaration(db, definition).declared()
        ))
    }
    fn binding_type(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        let db = self.db;
        ready(Ok(crate::types::binding_type(db, definition)))
    }
    fn is_discarded_dict_key_assignment(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = self.db;
        ready(Ok(crate::types::is_discarded_dict_key_assignment(
            db, definition,
        )))
    }
    fn loop_header_reachability(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<LoopHeaderReachability<'db>, Self::Error>> {
        let db = self.db;
        ready(Ok(super::loop_header_reachability(db, definition)))
    }
    fn reachability(
        &self,
        cache: Option<&ReachabilityEvaluationCache<'db>>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        constraint: ScopedReachabilityConstraintId,
    ) -> impl Future<Output = Result<Truthiness, Self::Error>> {
        let db = self.db;
        ready(Ok(evaluate_reachability_with_cache(
            db,
            cache,
            constraints,
            predicates,
            constraint,
        )))
    }
    fn narrow(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ty_python_core::narrowing_constraints::ScopedNarrowingConstraint,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(projector.narrow(constraint, ty)))
    }
    fn union_add(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        builder.add_in_place(ty);
        ready(Ok(()))
    }
    fn union_build(
        &self,
        builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(builder.build()))
    }
    fn function_same_place(
        &self,
        function: FunctionType<'db>,
        other: FunctionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = self.db;
        ready(Ok(function.has_same_place_as(db, other)))
    }
    fn function_contains(
        &self,
        function: FunctionType<'db>,
        other: FunctionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = self.db;
        ready(Ok(
            function.contains_definition(db, other.last_definition(db))
        ))
    }
    fn equivalent(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = self.db;
        ready(Ok(first.is_equivalent_to(db, env, second)))
    }
    fn preserve_raw_public_type(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(super::preserve_raw_public_type(db, scope, place)))
    }
}
