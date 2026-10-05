//! Shared declaration and binding reducers await the original Definition query.

use ruff_index::IndexSlice;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::KnownModule;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::narrowing_constraints::{NarrowingConstraints, ScopedNarrowingConstraint};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{PredicateNarrowingTargets, ProgramFile, Truthiness};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::imported::{
    ImportedFallbackEffects, ImportedFallbackFacts, ReExportEffects, imported_fallback_with,
    is_reexported_with,
};
use crate::place::normalization::{
    PlaceNormalizationEffects, PlaceNormalizationFacts, place_cycle_normalized_with,
};
use crate::place::source_effects::{
    PublicLookupEffects, SourcePlaceEffects, SourcePlaceWork, sealed,
};
use crate::place::{
    ConsideredDefinitions, LookupError, LookupResult, LoopHeaderReachability, PlaceAndQualifiers,
    RequiresExplicitReExport, imported_symbol_with, place_by_id_with,
    preserve_raw_public_type_in_scope,
};
use crate::reachability::narrowing_entry::NarrowingEntryEffects;
use crate::reachability::source::{ReachabilityFacts, evaluate_cached_reachability_with};
use crate::reachability::{NarrowingProjector, ReachabilityEvaluationCache};
use crate::types::infer::{DefinitionInference, DefinitionTypes};
use crate::types::member_lookup::general::{
    GeneralMemberFacts, GeneralMemberName, member_lookup_entry_with,
};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::{
    FunctionType, KnownClass, MemberLookupPolicy, ResolvedMember, Type, TypeAndQualifiers,
    UnionBuilder,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_place_by_id(
        &self,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let db = self.db();
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        let use_def = index.use_def_map(file_scope);
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
        let work = Self::checked(
            declarations
                .checked_add(imports)
                .and_then(|count| count.checked_add(bindings))
                .and_then(|count| count.checked_mul(4))
                .and_then(|count| count.checked_add(6)),
        )?;
        self.work(work).await?;
        // Construct the child after admission so the admission closure does not retain
        // its state inline in every name and attribute lookup.
        self.allocate_future(|| {
            place_by_id_with(db, self, scope, place, reexport, considered, use_def)
        })
        .await?
        .await
    }

    pub(in crate::types::infer) async fn normalize_place_cycle(
        &self,
        env: &ProgramEnvironment<'db>,
        current: PlaceAndQualifiers<'db>,
        previous: PlaceAndQualifiers<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.allocate_future(|| {
            place_cycle_normalized_with(
                current,
                env,
                previous,
                cycle,
                PlaceNormalizationFacts,
                self,
            )
        })
        .await?
        .await
    }

    pub(super) async fn inferred_binding_type(
        &self,
        inference: &DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> RunResult<Type<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.bindings.len(),
            _ => 1,
        };
        let binding = self
            .local(Self::checked(entries.checked_add(3))?, 0, || {
                inference
                    .types
                    .binding_type(definition, definition)
                    .or_else(|| inference.fallback_type())
            })
            .await?;
        match binding {
            Some(ty) => Ok(ty),
            None => self.unavailable(SourceOperation::MissingBinding).await,
        }
    }

    pub(super) async fn imported_symbol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
        reexport: Option<RequiresExplicitReExport>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        if let Some(file) = file {
            self.check_file_program(file).await?;
        }
        imported_symbol_with(db, &env, self, file, name, reexport).await
    }

    async fn check_module_scope(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
    ) -> RunResult<(ProgramFile<'db>, FileScopeId)> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        if file_scope.is_global() {
            Ok((file, file_scope))
        } else {
            self.unavailable(SourceOperation::PlaceScope).await
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PublicLookupEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn promote_public_type(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.promote_public_type_source(ty, env).await
    }
    async fn union_two(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.access.union_from_two_elements(first, second).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourcePlaceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn check_imported_file(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        file: ProgramFile<'db>,
    ) -> RunResult<()> {
        self.check_file_program(file).await?;
        self.environment_program(env).await?;
        Ok(())
    }
    async fn file_is_stub(&self, _db: &'db dyn Db, file: ProgramFile<'db>) -> RunResult<bool> {
        self.file_is_stub(self.physical_file(file).await?).await
    }
    async fn reduction_checkpoint(&self, _work: SourcePlaceWork) -> RunResult<()> {
        self.work(1).await
    }
    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.field(definition.read_fields(self.db()).kind()).await
    }
    async fn definition_is_reexported(&self, definition: Definition<'db>) -> RunResult<bool> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        Ok(self
            .field(definition.read_fields(self.db()).place_info())
            .await?
            .is_reexported())
    }
    async fn function_is_overload(&self, _function: FunctionType<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::FunctionMetadata).await
    }
    async fn union_builder(&self, env: &ProgramEnvironment<'db>) -> RunResult<UnionBuilder<'db>> {
        PairUnionEffects::new_union(self, env).await
    }
    async fn narrowing_projector<'map>(
        &self,
        env: &'map ProgramEnvironment<'db>,
        constraints: &'map NarrowingConstraints,
        predicates: &'map IndexSlice<ScopedPredicateId, Predicate<'db>>,
        targets: &'map PredicateNarrowingTargets,
        binding: Definition<'db>,
        base_ty: Type<'db>,
    ) -> RunResult<NarrowingProjector<'map, 'db>>
    where
        'db: 'map,
    {
        let file = self.definition_file(binding).await?;
        self.check_file_program(file).await?;
        let place = self
            .field(binding.read_fields(self.db()).place_info())
            .await?
            .place();
        self.create_narrowing_projector(env, constraints, predicates, targets, place, base_ty)
            .await
    }

    async fn symbol_id(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        name: &str,
    ) -> RunResult<Option<ScopedSymbolId>> {
        let (file, file_scope) = self.check_module_scope(db, scope).await?;
        let index = self.access.semantic_index(file).await?;
        let table = index.place_table(file_scope);
        let units = Self::checked(table.symbol_lookup_work(name.len()))?;
        self.local(units, 0, || table.symbol_id(name)).await
    }
    async fn is_known_module(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        module: KnownModule,
    ) -> RunResult<bool> {
        let (file, _) = self.check_module_scope(db, scope).await?;
        let known = match self.access.file_module(file).await? {
            Some(resolved) => resolved.known_with(self.access.endpoint()).await?,
            None => None,
        };
        self.local(1, 0, || known == Some(module)).await
    }

    async fn place_by_id(
        &self,
        _db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.access
            .place_by_id(scope, place, reexport, considered)
            .await
    }

    async fn global_scope(
        &self,
        _db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> RunResult<ScopeId<'db>> {
        self.check_file_program(file).await?;
        self.access.global_scope(file).await
    }
    async fn resolve_known_module(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        module: KnownModule,
    ) -> RunResult<Option<ProgramFile<'db>>> {
        let program = self.environment_program(env).await?;
        let name = self
            .local(module.as_str().len() + 1, 0, || module.name())
            .await?;
        let module = self.access.resolve_module(program, &name, None).await?;
        let file = match module {
            Some(module) => module.file_with(self.access.endpoint()).await?,
            None => None,
        };
        let Some(file) = file else {
            return Ok(None);
        };
        Ok(Some(self.access.prepare_file(file, program).await?.file))
    }
    async fn imported_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: PlaceAndQualifiers<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        if let Some(file) = file {
            self.check_file_program(file).await?;
        }
        self.allocate_future(|| {
            imported_fallback_with(db, &env, prior, file, name, ImportedFallbackFacts, self)
        })
        .await?
        .await
    }
    async fn is_reexported(&self, definition: Definition<'db>) -> RunResult<bool> {
        is_reexported_with(definition, self).await
    }
    async fn inferred_declaration(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<TypeAndQualifiers<'db>>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let inference = self.access.definition(definition).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.declarations.len(),
            _ => 1,
        };
        self.local(Self::checked(entries.checked_add(1))?, 0, || {
            inference.types.declaration_type(definition, definition)
        })
        .await
    }
    async fn binding_type(&self, definition: Definition<'db>) -> RunResult<Type<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let inference = self.access.definition(definition).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.bindings.len(),
            _ => 1,
        };
        let binding = self
            .local(Self::checked(entries.checked_add(1))?, 0, || {
                inference.types.binding_type(definition, definition)
            })
            .await?;
        match binding {
            Some(ty) => Ok(ty),
            None => self.unavailable(SourceOperation::MissingBinding).await,
        }
    }
    async fn is_discarded_dict_key_assignment(
        &self,
        _definition: Definition<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::DiscardedBinding).await
    }
    async fn loop_header_reachability(
        &self,
        _definition: Definition<'db>,
    ) -> RunResult<LoopHeaderReachability<'db>> {
        self.unavailable(SourceOperation::LoopHeader).await
    }
    async fn reachability(
        &self,
        cache: Option<&ReachabilityEvaluationCache<'db>>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        constraint: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        if let Some(cache) = cache {
            evaluate_cached_reachability_with(
                cache,
                constraints,
                predicates,
                constraint,
                ReachabilityFacts,
                self,
            )
            .await
        } else {
            self.evaluate_reachability(constraints, predicates, constraint)
                .await
        }
    }
    async fn narrow(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        NarrowingEntryEffects::narrow_projector(self, projector, constraint, ty).await
    }
    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        PairUnionEffects::union_add(self, builder, ty).await
    }
    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::union_build(self, builder).await
    }
    async fn function_same_place(
        &self,
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::FunctionComparison).await
    }
    async fn function_contains(
        &self,
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::FunctionComparison).await
    }
    async fn equivalent(
        &self,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::Equivalence).await
    }
    async fn preserve_raw_public_type(
        &self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> RunResult<bool> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        let is_stub = self
            .file_is_stub(self.physical_file(file).await?)
            .await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        self.local(32, 0, || {
            let symbol_name = place.as_symbol().map(|id| {
                index
                    .place_table(file_scope)
                    .symbol(id)
                    .name()
                    .as_str()
            });
            preserve_raw_public_type_in_scope(index.scope(file_scope), symbol_name, is_stub)
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ReExportEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn redundant_alias(&self, definition: Definition<'db>) -> RunResult<bool> {
        SourcePlaceEffects::definition_is_reexported(self, definition).await
    }

    async fn export_names(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<&'db FxHashSet<Name>>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let names = self.access.dunder_all_names(file).await?;
        self.local(1, 0, || names.as_ref()).await
    }

    async fn definition_name(&self, definition: Definition<'db>) -> RunResult<&'db Name> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let scope = self.definition_scope(definition).await?;
        let table = self.access.place_table(scope).await?;
        let place = self
            .field(definition.read_fields(self.db()).place_info())
            .await?
            .place();
        self.local(2, 0, || {
            place
                .as_symbol()
                .map(|symbol| table.symbol(symbol).name())
                .ok_or(RunError::Contract("re-export definition is not a symbol"))
        })
        .await?
    }

    async fn contains_name(&self, names: &FxHashSet<Name>, name: &Name) -> RunResult<bool> {
        self.export_names_contains(names, name.as_str()).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImportedFallbackEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(16).await
    }

    async fn lookup_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        prior.into_lookup_result_with(db, &env, self).await
    }

    async fn known_class_instance(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        self.access.known_class_instance(program, class).await
    }

    async fn member_lookup(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.environment_program(env).await?;
        let member = member_lookup_entry_with(
            ty,
            GeneralMemberName::Text(name),
            policy,
            None,
            GeneralMemberFacts,
            self,
        )
        .await?;
        self.work(2).await?;
        let member = match member {
            Ok(member) => member,
            Err(error) => {
                self.local(2, 0, || {
                    *error
                        .read_fields(salsa::FieldReads::new(self.db()))
                        .fallback_member()
                })
                .await?
            }
        };
        match member {
            ResolvedMember::Plain(place) => Ok(place),
            ResolvedMember::WithMetadata(metadata) => {
                self.local(2, 0, || {
                    *metadata
                        .read_fields(salsa::FieldReads::new(self.db()))
                        .member()
                })
                .await
            }
        }
    }

    async fn combine_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        prior.or_fall_back_to_with(db, &env, self, fallback).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PlaceNormalizationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn cycle_normalize(
        &self,
        env: &ProgramEnvironment<'db>,
        current: Type<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        SourceEffects::cycle_normalize(self, env, current, previous, cycle).await
    }

    async fn normalize_heads(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        self.normalize_cycle_heads(env, ty, cycle).await
    }

    async fn next_head(
        &self,
        heads: &mut salsa::CycleHeadCandidates<'_>,
    ) -> RunResult<Option<salsa::CycleHeadCandidate>> {
        self.local(1, 0, || heads.next()).await
    }
}
