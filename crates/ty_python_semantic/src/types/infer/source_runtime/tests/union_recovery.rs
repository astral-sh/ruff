use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe;

use super::*;
use crate::types::normalization::source::observations as normalization_observations;
use crate::types::normalization::source::observations::{Stop, StopKind};
use crate::types::normalization::{RecursiveNormalizationOperation, RecursiveNormalizationRequest};
use crate::types::{DivergentType, KnownInstanceType};

#[derive(Clone, Copy, Debug, Default)]
struct RecoverySnapshot {
    initial: Option<DivergentType>,
    key: Option<salsa::Id>,
    body_result: Option<salsa::Id>,
    bodies: usize,
    self_edges: usize,
    recoveries: usize,
    completed: usize,
    owner_drops: usize,
    dropped_children: usize,
    first_previous_is_seed: bool,
    iterations: [Option<u32>; 8],
}

thread_local! {
    static FORCE_CYCLE: Cell<bool> = const { Cell::new(false) };
    static RECOVERY: Cell<RecoverySnapshot> = Cell::new(RecoverySnapshot::default());
}

struct CycleMode;

impl CycleMode {
    fn enter(enabled: bool) -> Self {
        assert!(!FORCE_CYCLE.replace(enabled));
        RECOVERY.set(RecoverySnapshot::default());
        Self
    }
}

impl Drop for CycleMode {
    fn drop(&mut self) {
        FORCE_CYCLE.set(false);
    }
}

pub(in crate::types::infer::source_runtime) struct RecoveryProvider<MakeAccess> {
    inner: MaterializationProvider<MakeAccess>,
}

impl<MakeAccess> RecoveryProvider<MakeAccess> {
    pub(in crate::types::infer::source_runtime) fn new(
        inner: MaterializationProvider<MakeAccess>,
    ) -> Self {
        Self { inner }
    }
}

struct RecoveryOwner<'call, 'db> {
    cycle: &'call salsa::Cycle<'call>,
    last: &'call Type<'db>,
    expected_last: Type<'db>,
    input: &'call (Type<'db>, Program<'db>, MaterializationKind),
    expected_input: (Type<'db>, Program<'db>, MaterializationKind),
}

impl Drop for RecoveryOwner<'_, '_> {
    fn drop(&mut self) {
        let children = normalization_observations::snapshot();
        assert_eq!(children.children, children.dropped);
        assert_eq!(children.live_buffers, 0);
        assert_eq!(*self.last, self.expected_last);
        assert_eq!(*self.input, self.expected_input);
        assert!(self.cycle.head_ids().any(|id| id == self.cycle.id()));
        let mut snapshot = RECOVERY.get();
        assert_eq!(snapshot.key, Some(self.cycle.id()));
        snapshot.owner_drops += 1;
        snapshot.dropped_children = children.dropped;
        RECOVERY.set(snapshot);
    }
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for RecoveryProvider<MakeAccess>
where
    C: MaterializationConfiguration,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        <MaterializationProvider<MakeAccess> as CallableRouteProvider<'run, 'db, C>>::native_value(
            &self.inner,
            endpoint,
            db,
            operation,
        )
        .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let value =
            <MaterializationProvider<MakeAccess> as CallableRouteProvider<'run, 'db, C>>::initial(
                &self.inner,
                endpoint,
                db,
                id,
                input,
            )
            .await?;
        if FORCE_CYCLE.get() {
            let Type::Divergent(seed) = value else {
                panic!("materialization cycle seed")
            };
            assert_eq!(seed, DivergentType::new(id).materialized(input.2));
            let mut snapshot = RECOVERY.get();
            snapshot.initial = Some(seed);
            snapshot.key = Some(id);
            RECOVERY.set(snapshot);
        }
        Ok(value)
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        if FORCE_CYCLE.get() {
            let access = create_source_access(&endpoint, &self.inner.access).await?;
            let (ty, program, kind) = input;
            // The self-edge reaches the real callback; its provisional result is not the body result.
            access.cached_materialization(program, ty, kind).await?;
            let mut snapshot = RECOVERY.get();
            snapshot.self_edges += 1;
            RECOVERY.set(snapshot);
        }
        let value =
            <MaterializationProvider<MakeAccess> as CallableRouteProvider<'run, 'db, C>>::body(
                &self.inner,
                endpoint,
                db,
                input,
            )
            .await?;
        if FORCE_CYCLE.get() {
            let Type::TypeForm(form) = value else {
                panic!("fixture materialization remains a TypeForm")
            };
            let mut snapshot = RECOVERY.get();
            snapshot.bodies += 1;
            snapshot.body_result = Some(form.as_id());
            RECOVERY.set(snapshot);
        }
        Ok(value)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Type<'db>,
        value: Type<'db>,
        input: C::Input<'db>,
    ) -> RunResult<Type<'db>>
    where
        'run: 'call,
    {
        let owner = if FORCE_CYCLE.get() {
            let Type::TypeForm(form) = value else {
                panic!("fixture recovery receives the actual TypeForm")
            };
            let mut snapshot = RECOVERY.get();
            assert_eq!(snapshot.body_result, Some(form.as_id()));
            if let Some(key) = snapshot.key {
                assert_eq!(cycle.id(), key);
            }
            snapshot.key = Some(cycle.id());
            if snapshot.recoveries == 0 {
                snapshot.first_previous_is_seed =
                    matches!(last, Type::Divergent(seed) if Some(*seed) == snapshot.initial);
            }
            snapshot.iterations[snapshot.recoveries] = Some(cycle.iteration());
            snapshot.recoveries += 1;
            RECOVERY.set(snapshot);
            Some(RecoveryOwner {
                cycle,
                last,
                expected_last: *last,
                input: &input,
                expected_input: input,
            })
        } else {
            None
        };
        let result =
            <MaterializationProvider<MakeAccess> as CallableRouteProvider<'run, 'db, C>>::recover(
                &self.inner,
                endpoint,
                db,
                cycle,
                last,
                value,
                input,
            )
            .await?;
        if FORCE_CYCLE.get() {
            assert_eq!(result, value);
            let mut snapshot = RECOVERY.get();
            snapshot.completed += 1;
            RECOVERY.set(snapshot);
        }
        drop(owner);
        Ok(result)
    }
}

#[derive(Clone, Copy)]
enum Action<'db> {
    Normalize(RecursiveNormalizationRequest<'db>),
    Materialize(Type<'db>),
}

#[derive(Debug, PartialEq, Eq)]
enum TypeResult<'db> {
    Normalized(Option<Type<'db>>),
    Materialized(Type<'db>),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    env: &ProgramEnvironment<'db>,
    action: Action<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<TypeResult<'db>>, AnalysisFailure> {
    controlled_with_stop(prepared, env, action, policy, None)
}

fn controlled_with_stop<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    env: &ProgramEnvironment<'db>,
    action: Action<'db>,
    policy: &AnalysisPolicy,
    stop: Option<Stop>,
) -> Result<AnalysisOutcome<TypeResult<'db>>, AnalysisFailure> {
    let _mode = CycleMode::enter(matches!(action, Action::Materialize(_)));
    normalization_observations::reset(stop);
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        let result = catch_unwind(AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes,
                    values,
                };
                match action {
                    Action::Normalize(request) => SourceEffects::new(&access, session.program())
                        .recursive_normalize(env, request)
                        .await
                        .map(TypeResult::Normalized),
                    Action::Materialize(ty) => access
                        .cached_materialization(session.program(), ty, MaterializationKind::Bottom)
                        .await
                        .map(TypeResult::Materialized),
                }
            })
        }));
        let normalization = normalization_observations::snapshot();
        assert_eq!(normalization.children, normalization.dropped);
        assert_eq!(normalization.buffers, normalization.dropped_buffers);
        assert_eq!(normalization.live_buffers, 0);
        assert_eq!(observations::counts().0, 0);
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

fn wrapped<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    TypeFormType::from_type_expression(db, TypeFormType::from_type_expression(db, ty))
}

fn existing_key<'db, C: MaterializationConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    fields: &(Type<'db>, Program<'db>, MaterializationKind),
) -> Option<salsa::Id> {
    let mut entries = C::argument_ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| entry.value().fields() == fields);
    let result = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    result
}

fn assert_recovery_completed(snapshot: RecoverySnapshot) {
    assert!(snapshot.recoveries > 0, "{snapshot:?}");
    assert_eq!(snapshot.completed, snapshot.recoveries);
    assert_eq!(snapshot.owner_drops, snapshot.recoveries);
    assert_eq!(snapshot.self_edges, snapshot.bodies);
    assert!(snapshot.first_previous_is_seed);
    assert!(
        snapshot.iterations[..snapshot.recoveries]
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
    assert!(snapshot.dropped_children >= 2);
}

#[test]
fn materialization_cycle_recovery_publishes_and_reuses_the_actual_body_result() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = wrapped(&db, Type::any());
    let revision = salsa::plumbing::current_revision(&db);
    let cold = capture(&db, || {
        controlled(&prepared, &env, Action::Materialize(input), &funded())
    })
    .unwrap();
    let expected = wrapped(&db, Type::Never);
    assert_eq!(
        cold.value,
        Ok(AnalysisOutcome::Complete(TypeResult::Materialized(
            expected
        )))
    );
    cold.check_root_reads().unwrap();
    let snapshot = RECOVERY.get();
    assert_recovery_completed(snapshot);
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(
        &db,
        ingredient,
        &(input, env.program(&db), MaterializationKind::Bottom),
    )
    .unwrap();
    assert_eq!(snapshot.key, Some(id));
    assert_eq!(
        snapshot.initial,
        Some(DivergentType::new(id).materialized(MaterializationKind::Bottom))
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_no_active_attempt();

    let ordinary_db = fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ordinary_input = wrapped(&ordinary_db, Type::any());
    let ordinary =
        ordinary_input.materialization(&ordinary_db, &ordinary_env, MaterializationKind::Bottom);
    assert_eq!(ordinary, wrapped(&ordinary_db, Type::Never));
    assert_eq!(
        ordinary.display(&ordinary_db, &ordinary_env).to_string(),
        expected.display(&db, &env).to_string()
    );

    let key = ingredient.database_key_index(id);
    let cold_root = cold
        .reads
        .iter()
        .find(|read| read.key == key && read.parent.is_none())
        .unwrap();
    let mut reader = db.clone();
    reader.take_salsa_events();
    let native = capture(&db, || {
        input.materialization(&db, &env, MaterializationKind::Bottom)
    })
    .unwrap();
    assert_eq!(native.value, expected);
    let native_root = native
        .reads
        .iter()
        .find(|read| read.key == key && read.parent.is_none())
        .unwrap();
    assert_eq!(native_root.status, prepared_source_probe::Status::Final);
    assert_eq!(native_root.memo_address, cold_root.memo_address);
    let warm = capture(&db, || {
        controlled(&prepared, &env, Action::Materialize(input), &funded())
    })
    .unwrap();
    assert_eq!(
        warm.value,
        Ok(AnalysisOutcome::Complete(TypeResult::Materialized(
            expected
        )))
    );
    warm.check_root_reads().unwrap();
    let warm_root = warm
        .reads
        .iter()
        .find(|read| read.key == key && read.parent.is_none())
        .unwrap();
    assert_eq!(warm_root.memo_address, native_root.memo_address);
    assert_eq!(warm_root.stamp, native_root.stamp);
    assert_eq!((RECOVERY.get().bodies, RECOVERY.get().recoveries), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "cached_materialization",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn materialization_recovery_child_refusal_drops_before_cycle_owner_and_retries() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = wrapped(&db, Type::any());
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled_with_stop(
            &prepared,
            &env,
            Action::Materialize(input),
            &funded(),
            Some(Stop {
                child: 2,
                kind: StopKind::Work
            })
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        }),
    );
    let snapshot = RECOVERY.get();
    assert_eq!(
        (
            snapshot.recoveries,
            snapshot.completed,
            snapshot.owner_drops
        ),
        (1, 0, 1)
    );
    assert_eq!(snapshot.dropped_children, 2);
    let children = normalization_observations::snapshot();
    assert_eq!(
        (children.children, children.started, children.dropped),
        (2, 2, 2)
    );
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(
        &db,
        ingredient,
        &(input, env.program(&db), MaterializationKind::Bottom),
    )
    .unwrap();
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
        Err(FinalSourceError::ProvisionalMemo)
    ));
    assert_no_active_attempt();
    let retry = controlled(&prepared, &env, Action::Materialize(input), &funded());
    assert_eq!(
        retry,
        Ok(AnalysisOutcome::Complete(TypeResult::Materialized(
            wrapped(&db, Type::Never)
        )))
    );
    assert_recovery_completed(RECOVERY.get());
    assert_eq!(RECOVERY.get().key, Some(id));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn materialization_recovery_child_cancellation_retains_completion_for_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = wrapped(&db, Type::any());
    let revision = salsa::plumbing::current_revision(&db);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_with_stop(
            &prepared,
            &env,
            Action::Materialize(input),
            &funded(),
            Some(Stop {
                child: 2,
                kind: StopKind::Cancel,
            }),
        )
    }));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert_recovery_completed(RECOVERY.get());
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(
        &db,
        ingredient,
        &(input, env.program(&db), MaterializationKind::Bottom),
    )
    .unwrap();
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_no_active_attempt();
    let mut reader = db.clone();
    reader.take_salsa_events();
    let retry = controlled(&prepared, &env, Action::Materialize(input), &funded());
    assert_eq!(
        retry,
        Ok(AnalysisOutcome::Complete(TypeResult::Materialized(
            wrapped(&db, Type::Never)
        )))
    );
    assert_eq!((RECOVERY.get().bodies, RECOVERY.get().recoveries), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "cached_materialization",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
#[derive(Clone, Copy, Debug)]
enum RecursiveUnionCase {
    Empty,
    AllMarkers,
    ProcessedNever,
    SkippedMarkerThenNever,
    DuplicateMembers(RecursivelyDefined),
    SkippedMarkers,
    CollapsedTypeForm,
    NestedUnion,
    TupleChild,
    TypeFormChild,
}

fn recursive_union_marker(prepared: &PreparedAnalysisFile<'_>) -> DivergentType {
    DivergentType::new(
        prepared
            .semantic_index()
            .expression(expression_key(prepared))
            .as_id(),
    )
}

fn recursive_union<'db, const N: usize>(
    db: &'db dyn Db,
    elements: [Type<'db>; N],
    recursion: RecursivelyDefined,
) -> Type<'db> {
    Type::Union(UnionType::new(
        db,
        Vec::from(elements).into_boxed_slice(),
        recursion,
    ))
}

impl RecursiveUnionCase {
    fn input<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        marker: DivergentType,
    ) -> Type<'db> {
        let divergent = Type::Divergent(marker);
        let top = Type::Divergent(marker.materialized(MaterializationKind::Top));
        let bottom = Type::Divergent(marker.materialized(MaterializationKind::Bottom));
        match self {
            Self::Empty => recursive_union(db, [], RecursivelyDefined::No),
            Self::AllMarkers => {
                recursive_union(db, [divergent, top, bottom], RecursivelyDefined::No)
            }
            Self::ProcessedNever => recursive_union(db, [Type::Never], RecursivelyDefined::No),
            Self::SkippedMarkerThenNever => {
                recursive_union(db, [bottom, Type::Never], RecursivelyDefined::No)
            }
            Self::DuplicateMembers(recursion) => recursive_union(
                db,
                [Type::unknown(), Type::any(), Type::unknown()],
                recursion,
            ),
            Self::SkippedMarkers => recursive_union(
                db,
                [top, Type::unknown(), bottom, Type::any()],
                RecursivelyDefined::No,
            ),
            Self::CollapsedTypeForm => recursive_union(
                db,
                [
                    TypeFormType::from_type_expression(db, divergent),
                    Type::unknown(),
                ],
                RecursivelyDefined::No,
            ),
            Self::NestedUnion => recursive_union(
                db,
                [
                    recursive_union(db, [Type::unknown(), Type::any()], RecursivelyDefined::Yes),
                    Type::AlwaysTruthy,
                ],
                RecursivelyDefined::No,
            ),
            Self::TupleChild => recursive_union(
                db,
                [
                    Type::heterogeneous_tuple(
                        db,
                        env,
                        [
                            Type::heterogeneous_tuple(db, env, [divergent]),
                            Type::unknown(),
                        ],
                    ),
                    Type::Never,
                ],
                RecursivelyDefined::No,
            ),
            Self::TypeFormChild => recursive_union(
                db,
                [
                    TypeFormType::from_type_expression(
                        db,
                        recursive_union(
                            db,
                            [Type::unknown(), Type::any(), Type::unknown()],
                            RecursivelyDefined::No,
                        ),
                    ),
                    Type::Never,
                ],
                RecursivelyDefined::No,
            ),
        }
    }

    fn expected<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> Option<Type<'db>> {
        match self {
            Self::AllMarkers
            | Self::SkippedMarkerThenNever
            | Self::SkippedMarkers
            | Self::CollapsedTypeForm
            | Self::TupleChild
                if nested =>
            {
                None
            }
            Self::Empty | Self::AllMarkers => Some(divergent),
            Self::ProcessedNever | Self::SkippedMarkerThenNever => Some(Type::Never),
            Self::DuplicateMembers(recursion) => Some(recursive_union(
                db,
                [Type::unknown(), Type::any()],
                recursion,
            )),
            Self::SkippedMarkers => Some(recursive_union(
                db,
                [Type::unknown(), Type::any()],
                RecursivelyDefined::Yes,
            )),
            Self::CollapsedTypeForm => Some(recursive_union(
                db,
                [divergent, Type::unknown()],
                RecursivelyDefined::No,
            )),
            Self::NestedUnion => Some(recursive_union(
                db,
                [Type::unknown(), Type::any(), Type::AlwaysTruthy],
                RecursivelyDefined::Yes,
            )),
            Self::TupleChild => Some(Type::heterogeneous_tuple(
                db,
                env,
                [divergent, Type::unknown()],
            )),
            Self::TypeFormChild => Some(TypeFormType::from_type_expression(
                db,
                recursive_union(db, [Type::unknown(), Type::any()], RecursivelyDefined::No),
            )),
        }
    }
}

#[test]
fn recursive_union_normalization_reconstructs_members_and_recursion_flags() {
    for case in [
        RecursiveUnionCase::DuplicateMembers(RecursivelyDefined::No),
        RecursiveUnionCase::DuplicateMembers(RecursivelyDefined::Yes),
        RecursiveUnionCase::SkippedMarkers,
        RecursiveUnionCase::CollapsedTypeForm,
        RecursiveUnionCase::NestedUnion,
        RecursiveUnionCase::TupleChild,
        RecursiveUnionCase::TypeFormChild,
        RecursiveUnionCase::Empty,
        RecursiveUnionCase::AllMarkers,
        RecursiveUnionCase::ProcessedNever,
        RecursiveUnionCase::SkippedMarkerThenNever,
    ] {
        for nested in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let marker = recursive_union_marker(&prepared);
            let divergent = Type::Divergent(marker);
            let input = case.input(&db, &env, marker);
            let revision = salsa::plumbing::current_revision(&db);
            let captured = capture(&db, || {
                controlled(
                    &prepared,
                    &env,
                    Action::Normalize(RecursiveNormalizationRequest {
                        ty: input,
                        divergent,
                        nested,
                    }),
                    &funded(),
                )
            })
            .unwrap();
            let Ok(AnalysisOutcome::Complete(TypeResult::Normalized(actual))) = captured.value
            else {
                panic!("{case:?}, nested={nested}: {:?}", captured.value);
            };
            assert!(captured.reads.is_empty());
            let expected = case.expected(&db, &env, divergent, nested);
            assert_eq!(actual, expected, "{case:?}, nested={nested}");
            assert_ne!(actual, Some(input), "{case:?}, nested={nested}");
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();

            let ordinary_db = fixture();
            let ordinary_prepared = prepare(&ordinary_db);
            let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
            let ordinary_marker = recursive_union_marker(&ordinary_prepared);
            let ordinary_divergent = Type::Divergent(ordinary_marker);
            let ordinary_input = case.input(&ordinary_db, &ordinary_env, ordinary_marker);
            let ordinary = ordinary_input.recursive_type_normalized_impl(
                &ordinary_db,
                &ordinary_env,
                ordinary_divergent,
                nested,
            );
            assert_eq!(
                ordinary,
                case.expected(&ordinary_db, &ordinary_env, ordinary_divergent, nested),
                "ordinary {case:?}, nested={nested}",
            );
            assert_eq!(
                actual.map(|ty| ty.display(&db, &env).to_string()),
                ordinary.map(|ty| ty.display(&ordinary_db, &ordinary_env).to_string()),
                "{case:?}, nested={nested}",
            );
        }
    }
}

#[test]
fn recursive_union_normalization_short_circuits_before_unsupported_tails() {
    for normalized_marker in [false, true] {
        for literal_tail in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let divergent = Type::Divergent(recursive_union_marker(&prepared));
            let first = if normalized_marker {
                recursive_union(&db, [], RecursivelyDefined::No)
            } else {
                divergent
            };
            let (tail, operation) = if literal_tail {
                (Type::int_literal(1), OperationId::Union)
            } else {
                (
                    Type::KnownInstance(KnownInstanceType::Range {
                        is_non_empty: false,
                    }),
                    OperationId::RecursiveNormalization(
                        RecursiveNormalizationOperation::KnownInstance,
                    ),
                )
            };
            let input = recursive_union(&db, [first, tail], RecursivelyDefined::No);
            for nested in [false, true] {
                let result = controlled(
                    &prepared,
                    &env,
                    Action::Normalize(RecursiveNormalizationRequest {
                        ty: input,
                        divergent,
                        nested,
                    }),
                    &funded(),
                );
                if nested {
                    let expected = normalized_marker.then_some(divergent);
                    assert_eq!(
                        result,
                        Ok(AnalysisOutcome::Complete(TypeResult::Normalized(expected))),
                    );
                    assert_ne!(expected, Some(input));

                    let ordinary_db = fixture();
                    let ordinary_prepared = prepare(&ordinary_db);
                    let ordinary_env =
                        ProgramEnvironment::from_file(ordinary_prepared.program_file());
                    let ordinary_divergent =
                        Type::Divergent(recursive_union_marker(&ordinary_prepared));
                    let ordinary_first = if normalized_marker {
                        recursive_union(&ordinary_db, [], RecursivelyDefined::No)
                    } else {
                        ordinary_divergent
                    };
                    let ordinary_tail = if literal_tail {
                        Type::int_literal(1)
                    } else {
                        Type::KnownInstance(KnownInstanceType::Range {
                            is_non_empty: false,
                        })
                    };
                    let ordinary_input = recursive_union(
                        &ordinary_db,
                        [ordinary_first, ordinary_tail],
                        RecursivelyDefined::No,
                    );
                    let ordinary = ordinary_input.recursive_type_normalized_impl(
                        &ordinary_db,
                        &ordinary_env,
                        ordinary_divergent,
                        true,
                    );
                    assert_eq!(ordinary, normalized_marker.then_some(ordinary_divergent));
                    assert_eq!(
                        expected.map(|ty| ty.display(&db, &env).to_string()),
                        ordinary.map(|ty| ty.display(&ordinary_db, &ordinary_env).to_string()),
                    );
                } else {
                    assert_eq!(result, Ok(unavailable(operation)));
                }
                assert_no_active_attempt();
            }

            // Visiting either tail before the marker retains its normal refusal.
            let reversed = recursive_union(&db, [tail, first], RecursivelyDefined::No);
            assert_eq!(
                controlled(
                    &prepared,
                    &env,
                    Action::Normalize(RecursiveNormalizationRequest {
                        ty: reversed,
                        divergent,
                        nested: true,
                    }),
                    &funded(),
                ),
                Ok(unavailable(operation)),
            );
            assert_no_active_attempt();
        }
    }
}
