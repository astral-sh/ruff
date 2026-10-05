use std::cell::RefCell;
use std::future::{Future, ready};
use std::task::Poll;

use anyhow::Context;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::DbWithWritableSystem as _;
use ty_python_core::definition::DefinitionNodeKey;
use ty_python_core::{semantic_index, use_def_map};

use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::place::{LookupError, Place, PublicTypePolicy, TypeOrigin, place_from_bindings_with};
use crate::types::TypeQualifiers;
use crate::types::try_poll_immediate;

#[derive(Debug, PartialEq, Eq)]
enum Event<'db> {
    Binding(Definition<'db>),
    UnionAdd(Type<'db>),
    UnionBuild,
}

struct RecordingEffects<'db> {
    db: &'db dyn Db,
    binding_types: Vec<(Definition<'db>, Type<'db>)>,
    events: RefCell<Vec<Event<'db>>>,
}

impl<'db> RecordingEffects<'db> {
    fn new(db: &'db dyn Db, binding_types: Vec<(Definition<'db>, Type<'db>)>) -> Self {
        Self {
            db,
            binding_types,
            events: RefCell::default(),
        }
    }
}

impl sealed::Sealed for RecordingEffects<'_> {}

macro_rules! reject_effect {
    ($name:ident($($argument:ident: $argument_type:ty),* $(,)?) -> $output:ty) => {
        fn $name(
            &self,
            $($argument: $argument_type),*
        ) -> impl Future<Output = Result<$output, Self::Error>> {
            ready(Err(stringify!($name)))
        }
    };
}

impl<'db> PublicLookupEffects<'db> for RecordingEffects<'db> {
    type Error = &'static str;

    reject_effect!(promote_public_type(
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> Type<'db>);
    reject_effect!(union_two(
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> Type<'db>);
}

impl<'db> SourcePlaceEffects<'db> for RecordingEffects<'db> {
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

    fn binding_type(
        &self,
        definition: Definition<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        self.events.borrow_mut().push(Event::Binding(definition));
        ready(
            self.binding_types
                .iter()
                .find_map(|(binding, ty)| (*binding == definition).then_some(*ty))
                .ok_or("binding_type"),
        )
    }

    fn union_add(
        &self,
        _builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        self.events.borrow_mut().push(Event::UnionAdd(ty));
        ready(Ok(()))
    }

    fn union_build(
        &self,
        _builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        self.events.borrow_mut().push(Event::UnionBuild);
        ready(Ok(Type::unknown()))
    }

    reject_effect!(symbol_id(
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _name: &str,
    ) -> Option<ScopedSymbolId>);
    reject_effect!(is_known_module(
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _module: KnownModule,
    ) -> bool);
    reject_effect!(place_by_id(
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _place: ScopedPlaceId,
        _reexport: RequiresExplicitReExport,
        _considered: ConsideredDefinitions,
    ) -> PlaceAndQualifiers<'db>);
    reject_effect!(global_scope(
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
    ) -> ScopeId<'db>);
    reject_effect!(resolve_known_module(
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _module: KnownModule,
    ) -> Option<ProgramFile<'db>>);
    reject_effect!(imported_fallback(
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _prior: PlaceAndQualifiers<'db>,
        _file: Option<ProgramFile<'db>>,
        _name: &str,
    ) -> PlaceAndQualifiers<'db>);
    reject_effect!(is_reexported(
        _definition: Definition<'db>,
    ) -> bool);
    reject_effect!(inferred_declaration(
        _definition: Definition<'db>,
    ) -> Option<TypeAndQualifiers<'db>>);
    reject_effect!(is_discarded_dict_key_assignment(
        _definition: Definition<'db>,
    ) -> bool);
    reject_effect!(loop_header_reachability(
        _definition: Definition<'db>,
    ) -> LoopHeaderReachability<'db>);
    reject_effect!(reachability(
        _cache: Option<&ReachabilityEvaluationCache<'db>>,
        _constraints: &ReachabilityConstraints,
        _predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        _constraint: ScopedReachabilityConstraintId,
    ) -> Truthiness);
    reject_effect!(narrow(
        _projector: &mut NarrowingProjector<'_, 'db>,
        _constraint: ty_python_core::narrowing_constraints::ScopedNarrowingConstraint,
        _ty: Type<'db>,
    ) -> Type<'db>);
    reject_effect!(function_same_place(
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> bool);
    reject_effect!(function_contains(
        _function: FunctionType<'db>,
        _other: FunctionType<'db>,
    ) -> bool);
    reject_effect!(equivalent(
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> bool);
    reject_effect!(preserve_raw_public_type(
        _db: &'db dyn Db,
        _scope: ScopeId<'db>,
        _place: ScopedPlaceId,
    ) -> bool);
}

fn fixture(db: &TestDb) -> anyhow::Result<(ProgramFile<'_>, ScopeId<'_>, ScopedSymbolId)> {
    let file = system_path_to_file(db, "/src/test.py")?;
    let file = ProgramFile::new(db, file, db.program_environment().program(db));
    let scope = global_scope(db, file);
    let symbol = place_table(db, scope)
        .symbol_id("x")
        .context("fixture must define x")?;
    Ok((file, scope, symbol))
}

fn assignments<'db>(db: &'db dyn Db, file: ProgramFile<'db>) -> Vec<Definition<'db>> {
    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);
    module
        .suite()
        .iter()
        .filter_map(|statement| statement.as_assign_stmt())
        .flat_map(DefinitionNodeKey::from_assignment)
        .map(|key| index.expect_single_definition(key))
        .collect()
}

fn ready_result<T>(future: impl Future<Output = Result<T, &'static str>>) -> anyhow::Result<T> {
    match try_poll_immediate(future) {
        Poll::Ready(result) => result.map_err(anyhow::Error::msg),
        Poll::Pending => anyhow::bail!("test effects must complete immediately"),
    }
}

#[test]
fn empty_preceding_bindings_need_no_semantic_effects() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented("/src/test.py", "x = 1")?;
    let (file, scope, _) = fixture(&db)?;
    let assignments = assignments(&db, file);
    assert_eq!(assignments.len(), 1);
    let use_def = use_def_map(&db, scope);
    let effects = RecordingEffects::new(&db, vec![]);
    let result = ready_result(place_from_bindings_with(
        &db.program_environment(),
        &effects,
        use_def.bindings_at_definition(assignments[0]),
        RequiresExplicitReExport::No,
        None,
    ))?;

    assert_eq!(result.place, Place::Undefined);
    assert_eq!(result.first_definition, None);
    assert!(effects.events.borrow().is_empty());
    Ok(())
}

#[test]
fn single_binding_bypasses_union_effects() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented("/src/test.py", "x = 1")?;
    let (file, scope, symbol) = fixture(&db)?;
    let assignments = assignments(&db, file);
    assert_eq!(assignments.len(), 1);
    let binding = assignments[0];
    let ty = Type::int_literal(1);
    let use_def = use_def_map(&db, scope);
    let effects = RecordingEffects::new(&db, vec![(binding, ty)]);
    let result = ready_result(place_from_bindings_with(
        &db.program_environment(),
        &effects,
        use_def.reachable_bindings(symbol.into()),
        RequiresExplicitReExport::No,
        None,
    ))?;

    assert!(result.place.is_definitely_bound());
    assert_eq!(result.place.raw_type(), Some(ty));
    assert_eq!(result.first_definition, Some(binding));
    assert_eq!(*effects.events.borrow(), [Event::Binding(binding)]);
    Ok(())
}

#[test]
fn binding_inference_and_union_effects_follow_source_order() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/test.py",
        r#"
        x = 1
        x = 2
        x = 3
        "#,
    )?;
    let (file, scope, symbol) = fixture(&db)?;
    let assignments = assignments(&db, file);
    assert_eq!(assignments.len(), 3);
    let types = [
        Type::int_literal(1),
        Type::int_literal(2),
        Type::int_literal(3),
    ];
    let effects = RecordingEffects::new(&db, assignments.iter().copied().zip(types).collect());
    let use_def = use_def_map(&db, scope);
    let result = ready_result(place_from_bindings_with(
        &db.program_environment(),
        &effects,
        use_def.reachable_bindings(symbol.into()),
        RequiresExplicitReExport::No,
        None,
    ))?;

    assert!(result.place.is_definitely_bound());
    assert_eq!(result.place.raw_type(), Some(Type::unknown()));
    assert_eq!(result.first_definition, Some(assignments[0]));
    assert_eq!(
        *effects.events.borrow(),
        [
            Event::Binding(assignments[0]),
            Event::Binding(assignments[1]),
            Event::UnionAdd(types[0]),
            Event::UnionAdd(types[1]),
            Event::Binding(assignments[2]),
            Event::UnionAdd(types[2]),
            Event::UnionBuild,
        ]
    );
    Ok(())
}

#[test]
fn unsupported_reachability_stops_before_binding_inference() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/test.py",
        r#"
        if flag:
            x = 1
        "#,
    )?;
    let (_, scope, symbol) = fixture(&db)?;
    let use_def = use_def_map(&db, scope);
    let effects = RecordingEffects::new(&db, vec![]);
    let result = try_poll_immediate(place_from_bindings_with(
        &db.program_environment(),
        &effects,
        use_def.reachable_bindings(symbol.into()),
        RequiresExplicitReExport::No,
        None,
    ));

    assert!(matches!(result, Poll::Ready(Err("reachability"))));
    assert!(effects.events.borrow().is_empty());
    Ok(())
}

#[test]
fn public_lookup_only_requests_promotion_when_required() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let effects = RecordingEffects::new(&db, vec![]);
    let ty = Type::int_literal(1);
    let raw = PlaceAndQualifiers::from(Place::bound(ty));

    let result = ready_result(raw.into_lookup_result_with(&db, &env, &effects))?;
    assert_eq!(result.map(|result| result.inner_type()), Ok(ty));

    let promoted = PlaceAndQualifiers::from(
        Place::bound(ty).with_public_type_policy(PublicTypePolicy::Promote),
    );
    assert!(matches!(
        try_poll_immediate(promoted.into_lookup_result_with(&db, &env, &effects)),
        Poll::Ready(Err("promote_public_type"))
    ));
    assert!(effects.events.borrow().is_empty());
    Ok(())
}

#[test]
fn lookup_fallback_only_requests_union_for_two_present_types() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let effects = RecordingEffects::new(&db, vec![]);
    let fallback_ty = Type::int_literal(2);
    let fallback = PlaceAndQualifiers::from(Place::bound(fallback_ty));
    let undefined = LookupError::Undefined(TypeQualifiers::empty());
    let result = ready_result(undefined.or_fall_back_to_with(&db, &env, &effects, fallback))?;
    assert_eq!(result.map(|result| result.inner_type()), Ok(fallback_ty));

    let possibly_undefined = LookupError::PossiblyUndefined(TypeAndQualifiers::new(
        Type::int_literal(1),
        TypeOrigin::Inferred,
        TypeQualifiers::empty(),
    ));
    assert!(matches!(
        try_poll_immediate(possibly_undefined.or_fall_back_to_with(&db, &env, &effects, fallback)),
        Poll::Ready(Err("union_two"))
    ));
    assert!(effects.events.borrow().is_empty());
    Ok(())
}
