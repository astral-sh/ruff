use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe;

use super::*;
use crate::types::mapping::specialization::SpecializationConfiguration;
use crate::types::normalization::source::observations as normalization_observations;
use crate::types::normalization::source::observations::{Stop, StopKind};
use crate::types::{
    BoundTypeVarInstance, DivergentType, GenericContext, Specialization,
    apply_specialization_ingredient,
};

type Input<'db> = (Type<'db>, Specialization<'db>, bool);

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
    fn enter() -> Self {
        assert!(!FORCE_CYCLE.replace(true));
        RECOVERY.set(RecoverySnapshot::default());
        Self
    }
}

impl Drop for CycleMode {
    fn drop(&mut self) {
        FORCE_CYCLE.set(false);
    }
}

pub(in crate::types::infer::source_runtime) struct RecoveryProvider<'db, MakeAccess> {
    inner: SpecializationProvider<'db, MakeAccess>,
}

impl<'db, MakeAccess> RecoveryProvider<'db, MakeAccess> {
    pub(in crate::types::infer::source_runtime) fn new(
        inner: SpecializationProvider<'db, MakeAccess>,
    ) -> Self {
        Self { inner }
    }
}

struct RecoveryOwner<'call, 'db> {
    cycle: &'call salsa::Cycle<'call>,
    last: &'call Type<'db>,
    expected_last: Type<'db>,
    input: &'call Input<'db>,
    expected_input: Input<'db>,
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
    for RecoveryProvider<'db, MakeAccess>
where
    C: SpecializationConfiguration,
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
        <SpecializationProvider<'db, MakeAccess> as CallableRouteProvider<'run, 'db, C>>::native_value(
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
        let value = <SpecializationProvider<'db, MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::initial(&self.inner, endpoint, db, id, input)
        .await?;
        if FORCE_CYCLE.get() {
            assert_eq!(value, Type::divergent(id));
            let mut snapshot = RECOVERY.get();
            snapshot.initial = Some(DivergentType::new(id));
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
            let (ty, specialization, specialize_self_domain) = input;
            // The self-edge reaches the real callback; its provisional result is not the body result.
            access
                .apply_specialization(ty, specialization, specialize_self_domain)
                .await?;
            let mut snapshot = RECOVERY.get();
            snapshot.self_edges += 1;
            RECOVERY.set(snapshot);
        }
        let value = <SpecializationProvider<'db, MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::body(&self.inner, endpoint, db, input)
        .await?;
        if FORCE_CYCLE.get() {
            let Type::TypeForm(form) = value else {
                panic!("fixture specialization remains a TypeForm")
            };
            assert_eq!(value, input.0);
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
        let result = <SpecializationProvider<'db, MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::recover(&self.inner, endpoint, db, cycle, last, value, input)
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

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    input: Input<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    controlled_with_stop(prepared, input, policy, None)
}

fn controlled_with_stop<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    (ty, specialization, specialize_self_domain): Input<'db>,
    policy: &AnalysisPolicy,
    stop: Option<Stop>,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    let _mode = CycleMode::enter();
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
                access
                    .apply_specialization(ty, specialization, specialize_self_domain)
                    .await
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

fn input<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>) -> Input<'db> {
    let variable =
        BoundTypeVarInstance::synthetic(db, env, Name::new_static("T"), TypeVarVariance::Invariant);
    let context = GenericContext::from_typevar_instances(db, env, [variable]);
    let specialization = Specialization::new(
        db,
        context,
        vec![Type::unknown()].into_boxed_slice(),
        None,
        None,
    );
    // No bound variable occurs in this type, so the real specialization body preserves it.
    let ty =
        TypeFormType::from_type_expression(db, TypeFormType::from_type_expression(db, Type::any()));
    (ty, specialization, false)
}

fn existing_key<'db, C: SpecializationConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    fields: &Input<'db>,
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
fn specialization_cycle_recovery_publishes_and_reuses_the_actual_body_result() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = input(&db, &env);
    let expected = input.0;
    let revision = salsa::plumbing::current_revision(&db);
    let cold = capture(&db, || controlled(&prepared, input, &funded())).unwrap();
    assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(expected)));
    cold.check_root_reads().unwrap();
    let snapshot = RECOVERY.get();
    assert_recovery_completed(snapshot);
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &input).unwrap();
    assert_eq!(snapshot.key, Some(id));
    assert_eq!(snapshot.initial, Some(DivergentType::new(id)));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_no_active_attempt();

    let ordinary_db = fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ordinary_input = self::input(&ordinary_db, &ordinary_env);
    let ordinary = ordinary_input.0.apply_specialization_impl(
        &ordinary_db,
        ordinary_input.1,
        ordinary_input.2,
    );
    assert_eq!(ordinary, ordinary_input.0);
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
        input.0.apply_specialization_impl(&db, input.1, input.2)
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
    let warm = capture(&db, || controlled(&prepared, input, &funded())).unwrap();
    assert_eq!(warm.value, Ok(AnalysisOutcome::Complete(expected)));
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
        "apply_specialization_inner",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn specialization_recovery_child_refusal_drops_before_cycle_owner_and_retries() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = input(&db, &env);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled_with_stop(
            &prepared,
            input,
            &funded(),
            Some(Stop {
                child: 2,
                kind: StopKind::Work,
            }),
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    let snapshot = RECOVERY.get();
    assert_eq!(
        (
            snapshot.recoveries,
            snapshot.completed,
            snapshot.owner_drops
        ),
        (1, 0, 1),
    );
    assert_eq!(snapshot.dropped_children, 2);
    let children = normalization_observations::snapshot();
    assert_eq!(
        (children.children, children.started, children.dropped),
        (2, 2, 2),
    );
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &input).unwrap();
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
        Err(FinalSourceError::ProvisionalMemo),
    ));
    assert_no_active_attempt();
    assert_eq!(
        controlled(&prepared, input, &funded()),
        Ok(AnalysisOutcome::Complete(input.0)),
    );
    assert_recovery_completed(RECOVERY.get());
    assert_eq!(RECOVERY.get().key, Some(id));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn specialization_recovery_child_cancellation_retains_completion_for_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = input(&db, &env);
    let revision = salsa::plumbing::current_revision(&db);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_with_stop(
            &prepared,
            input,
            &funded(),
            Some(Stop {
                child: 2,
                kind: StopKind::Cancel,
            }),
        )
    }));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert_recovery_completed(RECOVERY.get());
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &input).unwrap();
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_no_active_attempt();
    let mut reader = db.clone();
    reader.take_salsa_events();
    assert_eq!(
        controlled(&prepared, input, &funded()),
        Ok(AnalysisOutcome::Complete(input.0)),
    );
    assert_eq!((RECOVERY.get().bodies, RECOVERY.get().recoveries), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "apply_specialization_inner",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
