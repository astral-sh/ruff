//! Source-place reads within an owned definition transaction.
//!
//! The shared reductions decide declaration precedence and retain imported qualifiers. Every
//! definition read waits for that definition's committed transaction, including a read of the
//! originating definition; an unfinished declaration is never treated as an absent declaration.

use std::future::{Future, ready};

use ruff_index::IndexSlice;
use ty_module_resolver::{KnownModule, file_to_module, resolve_module_confident};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::narrowing_constraints::{NarrowingConstraints, ScopedNarrowingConstraint};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    PredicateNarrowingTargets, ProgramFile, Truthiness, global_scope, semantic_index,
};

use super::{QueuedDefinitionEffects, SourceDefinitionEffect};
use crate::place::source_effects::{PublicLookupEffects, SourcePlaceEffects, sealed};
use crate::place::{
    ConsideredDefinitions, LoopHeaderReachability, PlaceAndQualifiers, RequiresExplicitReExport,
    imported_symbol_with, place_by_id_with,
};
use crate::reachability::{NarrowingProjector, ReachabilityEvaluationCache};
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::{FunctionType, Type, TypeAndQualifiers, UnionBuilder};
use crate::{Db, ProgramEnvironment};

impl<'db> QueuedDefinitionEffects<'_, 'db, '_> {
    pub(super) async fn imported_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
        reexport: Option<RequiresExplicitReExport>,
    ) -> Result<PlaceAndQualifiers<'db>, Boundary> {
        if env.program(db) != self.owner.program(db)
            || file.is_some_and(|file| file.program(db) != self.owner.program(db))
        {
            return Err(Boundary::ProgramDomain);
        }
        imported_symbol_with(db, env, self, file, name, reexport).await
    }

    fn check_module_scope(&self, db: &'db dyn Db, scope: ScopeId<'db>) -> Result<(), Boundary> {
        if scope.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        if !scope.file_scope_id(db).is_global() {
            return Err(Boundary::SourceDefinition(
                SourceDefinitionEffect::PlaceScope,
            ));
        }
        Ok(())
    }
}

impl sealed::Sealed for QueuedDefinitionEffects<'_, '_, '_> {}

impl<'db> PublicLookupEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
    type Error = Boundary;

    fn promote_public_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::PlacePromotion,
        )))
    }

    fn union_two(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::Union,
        )))
    }
}

impl<'db> SourcePlaceEffects<'db> for QueuedDefinitionEffects<'_, 'db, '_> {
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

    async fn symbol_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        self.check_module_scope(db, scope)?;
        self.work(name.len().checked_add(1).ok_or(Boundary::CostOverflow)?)
            .await?;
        Ok(semantic_index(db, scope.program_file(db))
            .place_table(scope.file_scope_id(db))
            .symbol_id(name))
    }

    async fn is_known_module(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        module: KnownModule,
    ) -> Result<bool, Self::Error> {
        self.check_module_scope(db, scope)?;
        self.work(1).await?;
        Ok(file_to_module(db, scope.program_file(db).resolver_file(db))
            .is_some_and(|resolved| resolved.is_known(db, module)))
    }

    async fn place_by_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.check_module_scope(db, scope)?;
        if !place.is_symbol() {
            return Err(Boundary::SourceDefinition(
                SourceDefinitionEffect::PlaceScope,
            ));
        }
        let index = semantic_index(db, scope.program_file(db));
        let use_def = index.use_def_map(scope.file_scope_id(db));
        let (declarations, imports, bindings) = match considered {
            ConsideredDefinitions::EndOfScope => (
                use_def.end_of_scope_declarations(place).traversal_len(),
                use_def
                    .end_of_scope_imported_final_candidates(place)
                    .traversal_len(),
                use_def.end_of_scope_bindings(place).traversal_len(),
            ),
            ConsideredDefinitions::AllReachable => (
                use_def.reachable_declarations(place).traversal_len(),
                use_def
                    .reachable_imported_final_candidates(place)
                    .traversal_len(),
                use_def.reachable_bindings(place).traversal_len(),
            ),
        };
        // Filtered declaration iterators can inspect entries they do not yield. Reserve the
        // retained lengths before advancing any iterator, including its initial sentinel.
        let units = declarations
            .checked_add(imports)
            .and_then(|count| count.checked_add(bindings))
            .and_then(|count| count.checked_mul(4))
            .and_then(|count| count.checked_add(4))
            .ok_or(Boundary::CostOverflow)?;
        self.work(units).await?;
        Box::pin(place_by_id_with(
            db, self, scope, place, reexport, considered, use_def,
        ))
        .await
    }

    async fn global_scope(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        if file.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        Ok(global_scope(db, file))
    }

    async fn resolve_known_module(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: KnownModule,
    ) -> Result<Option<ProgramFile<'db>>, Self::Error> {
        if env.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        self.work(1).await?;
        Ok(
            resolve_module_confident(db, env.resolver_environment(db), &module.name())
                .and_then(|module| module.file(db))
                .map(|file| ProgramFile::new(db, file, env.program(db))),
        )
    }

    fn imported_fallback(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _prior: PlaceAndQualifiers<'db>,
        _file: Option<ProgramFile<'db>>,
        _name: &str,
    ) -> impl Future<Output = Result<PlaceAndQualifiers<'db>, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::ModuleFallback,
        )))
    }

    fn is_reexported(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        let db = self.db;
        ready(if definition.program(db) != self.owner.program(db) {
            Err(Boundary::ProgramDomain)
        } else if definition.is_reexported(db) {
            Ok(true)
        } else {
            // Checking `__all__` requires a separate source-inference dependency.
            Err(Boundary::SourceDefinition(SourceDefinitionEffect::ReExport))
        })
    }

    async fn inferred_declaration(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<TypeAndQualifiers<'db>>, Self::Error> {
        let db = self.db;
        if definition.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        let inference = self
            .router
            .definition_demand(self.owner, definition)
            .await?;
        Ok(inference.completed_declaration(definition))
    }

    async fn binding_type(&self, definition: Definition<'db>) -> Result<Type<'db>, Self::Error> {
        let db = self.db;
        if definition.program(db) != self.owner.program(db) {
            return Err(Boundary::ProgramDomain);
        }
        let inference = self
            .router
            .definition_demand(self.owner, definition)
            .await?;
        inference
            .completed_binding(definition)
            .ok_or(Boundary::SourceDefinition(
                SourceDefinitionEffect::MissingBinding,
            ))
    }

    fn is_discarded_dict_key_assignment(
        &self,
        _definition: Definition<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::DiscardedBinding,
        )))
    }

    fn loop_header_reachability(
        &self,
        _definition: Definition<'db>,
    ) -> impl Future<Output = Result<LoopHeaderReachability<'db>, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::LoopHeader,
        )))
    }

    fn reachability(
        &self,
        _cache: Option<&ReachabilityEvaluationCache<'db>>,
        _constraints: &ReachabilityConstraints,
        _predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        _constraint: ScopedReachabilityConstraintId,
    ) -> impl Future<Output = Result<Truthiness, Self::Error>> {
        // The shared reachability dispatcher handles literal constraints before this effect.
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::Reachability,
        )))
    }

    fn narrow(
        &self,
        _projector: &mut NarrowingProjector<'_, 'db>,
        _constraint: ScopedNarrowingConstraint,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::Narrowing,
        )))
    }

    fn union_add(
        &self,
        _builder: &mut UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::Union,
        )))
    }

    fn union_build(
        &self,
        _builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::Union,
        )))
    }

    fn function_same_place(
        &self,
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::FunctionSamePlace,
        )))
    }

    fn function_contains(
        &self,
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::FunctionContains,
        )))
    }

    fn equivalent(
        &self,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Err(Boundary::SourceDefinition(
            SourceDefinitionEffect::Equivalence,
        )))
    }

    fn preserve_raw_public_type(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        _place: ScopedPlaceId,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(self.check_module_scope(db, scope).map(|()| true))
    }
}
