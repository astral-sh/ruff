//! A discarded canonical self-fetch exercises recovery without replacing the default body.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::ZalsaDatabase;
use salsa::prepared_source_probe;

use super::*;
use crate::types::DivergentType;
use crate::types::normalization::source::observations as normalization_observations;
use crate::types::normalization::source::observations::{Stop, StopKind};
use crate::types::typevar::{
    BindingContext, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarKind,
    TypeVarNonce, bound_typevar_default_ingredient, lazy_typevar_default_ingredient,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultQuery<'db> {
    Bound(BoundTypeVarInstance<'db>),
    Lazy(TypeVarInstance<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultKey {
    Bound(salsa::Id),
    Lazy(salsa::Id),
}

impl DefaultKey {
    fn id(self) -> salsa::Id {
        match self {
            Self::Bound(id) | Self::Lazy(id) => id,
        }
    }
}

impl DefaultQuery<'_> {
    fn key(self) -> DefaultKey {
        match self {
            Self::Bound(variable) => DefaultKey::Bound(variable.as_id()),
            Self::Lazy(variable) => DefaultKey::Lazy(variable.as_id()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Bound(_) => "bound_typevar_default_type",
            Self::Lazy(_) => "lazy_default_unchecked",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultValue {
    Absent,
    Any,
    TypeForm(salsa::Id),
}

impl DefaultValue {
    fn from_type(value: Option<Type<'_>>) -> Self {
        match value {
            None => Self::Absent,
            Some(Type::TypeForm(form)) => Self::TypeForm(form.as_id()),
            Some(ty) if ty == Type::any() => Self::Any,
            other => panic!("unexpected fixture default: {other:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RecoverySnapshot {
    initial: Option<DivergentType>,
    key: Option<salsa::Id>,
    body_result: Option<DefaultValue>,
    bodies: usize,
    self_edges: usize,
    recoveries: usize,
    completed: usize,
    owner_drops: usize,
    dropped_children: usize,
    first_previous_is_seed: bool,
    recovery_remaining: Option<usize>,
    iterations: [Option<u32>; 8],
}

thread_local! {
    static FORCE_CYCLE: Cell<Option<DefaultKey>> = const { Cell::new(None) };
    static RECOVERY: Cell<RecoverySnapshot> = Cell::new(RecoverySnapshot::default());
}

struct CycleMode;

impl CycleMode {
    fn enter(query: DefaultQuery<'_>) -> Self {
        assert!(FORCE_CYCLE.replace(Some(query.key())).is_none());
        RECOVERY.set(RecoverySnapshot::default());
        Self
    }
}

impl Drop for CycleMode {
    fn drop(&mut self) {
        FORCE_CYCLE.set(None);
    }
}

fn selected(query: DefaultQuery<'_>) -> bool {
    FORCE_CYCLE.get() == Some(query.key())
}

fn observe_initial(id: salsa::Id, value: Option<Type<'_>>) {
    let Some(Type::Divergent(seed)) = value else {
        panic!("default cycle seed")
    };
    assert_eq!(seed, DivergentType::new(id));
    let mut snapshot = RECOVERY.get();
    snapshot.initial = Some(seed);
    snapshot.key = Some(id);
    RECOVERY.set(snapshot);
}

fn observe_body(value: Option<Type<'_>>) {
    let mut snapshot = RECOVERY.get();
    snapshot.bodies += 1;
    snapshot.body_result = Some(DefaultValue::from_type(value));
    RECOVERY.set(snapshot);
}

struct RecoveryOwner<'call, 'db> {
    cycle: &'call salsa::Cycle<'call>,
    last: &'call Option<Type<'db>>,
    expected_last: Option<Type<'db>>,
    input: &'call DefaultQuery<'db>,
    expected_input: DefaultQuery<'db>,
}

impl<'call, 'db> RecoveryOwner<'call, 'db> {
    fn new(
        db: &dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<Type<'db>>,
        value: Option<Type<'db>>,
        input: &'call DefaultQuery<'db>,
    ) -> Self {
        let mut snapshot = RECOVERY.get();
        assert_eq!(snapshot.body_result, Some(DefaultValue::from_type(value)));
        assert_eq!(snapshot.key, Some(cycle.id()));
        assert_eq!(input.key().id(), cycle.id());
        if snapshot.recoveries == 0 {
            snapshot.first_previous_is_seed =
                matches!(last, Some(Type::Divergent(seed)) if Some(*seed) == snapshot.initial);
            snapshot.recovery_remaining =
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        snapshot.iterations[snapshot.recoveries] = Some(cycle.iteration());
        snapshot.recoveries += 1;
        RECOVERY.set(snapshot);
        Self {
            cycle,
            last,
            expected_last: *last,
            input,
            expected_input: *input,
        }
    }
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

pub(in crate::types::infer::source_runtime) struct BoundRecoveryProvider<'db, MakeAccess> {
    inner: BoundTypeVarDefaultProvider<'db, MakeAccess>,
}

impl<'db, MakeAccess> BoundRecoveryProvider<'db, MakeAccess> {
    pub(in crate::types::infer::source_runtime) fn new(
        inner: BoundTypeVarDefaultProvider<'db, MakeAccess>,
    ) -> Self {
        Self { inner }
    }
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for BoundRecoveryProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = BoundTypeVarInstance<'a>,
            Output<'a> = Option<Type<'a>>,
        >,
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
        <BoundTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<'run, 'db, C>>::native_value(
            &self.inner, endpoint, db, operation,
        )
        .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let value = <BoundTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::initial(&self.inner, endpoint, db, id, variable)
        .await?;
        if selected(DefaultQuery::Bound(variable)) {
            observe_initial(id, value);
        }
        Ok(value)
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let enabled = selected(DefaultQuery::Bound(variable));
        if enabled {
            let access = create_source_access(&endpoint, &self.inner.access).await?;
            // Keep the self-edge on every attempt; only the actual body supplies the result.
            access.bound_typevar_default(variable).await?;
            let mut snapshot = RECOVERY.get();
            snapshot.self_edges += 1;
            RECOVERY.set(snapshot);
        }
        let value = <BoundTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::body(&self.inner, endpoint, db, variable)
        .await?;
        if enabled {
            observe_body(value);
        }
        Ok(value)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let input = DefaultQuery::Bound(variable);
        let owner = selected(input).then(|| RecoveryOwner::new(db, cycle, last, value, &input));
        let result = <BoundTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::recover(&self.inner, endpoint, db, cycle, last, value, variable)
        .await?;
        if owner.is_some() {
            assert_eq!(result, value);
            let mut snapshot = RECOVERY.get();
            snapshot.completed += 1;
            RECOVERY.set(snapshot);
        }
        drop(owner);
        Ok(result)
    }
}

pub(in crate::types::infer::source_runtime) struct LazyRecoveryProvider<'db, MakeAccess> {
    inner: LazyTypeVarDefaultProvider<'db, MakeAccess>,
}

impl<'db, MakeAccess> LazyRecoveryProvider<'db, MakeAccess> {
    pub(in crate::types::infer::source_runtime) fn new(
        inner: LazyTypeVarDefaultProvider<'db, MakeAccess>,
    ) -> Self {
        Self { inner }
    }
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for LazyRecoveryProvider<'db, MakeAccess>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypeVarInstance<'a>,
            Output<'a> = Option<Type<'a>>,
        >,
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
        <LazyTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<'run, 'db, C>>::native_value(
            &self.inner, endpoint, db, operation,
        )
        .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let value = <LazyTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::initial(&self.inner, endpoint, db, id, variable)
        .await?;
        if selected(DefaultQuery::Lazy(variable)) {
            observe_initial(id, value);
        }
        Ok(value)
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let enabled = selected(DefaultQuery::Lazy(variable));
        if enabled {
            let access = create_source_access(&endpoint, &self.inner.access).await?;
            access.lazy_typevar_default(variable).await?;
            let mut snapshot = RECOVERY.get();
            snapshot.self_edges += 1;
            RECOVERY.set(snapshot);
        }
        let value = <LazyTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::body(&self.inner, endpoint, db, variable)
        .await?;
        if enabled {
            observe_body(value);
        }
        Ok(value)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call Option<Type<'db>>,
        value: Option<Type<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>
    where
        'run: 'call,
    {
        let input = DefaultQuery::Lazy(variable);
        let owner = selected(input).then(|| RecoveryOwner::new(db, cycle, last, value, &input));
        let result = <LazyTypeVarDefaultProvider<MakeAccess> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::recover(&self.inner, endpoint, db, cycle, last, value, variable)
        .await?;
        if owner.is_some() {
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
    query: DefaultQuery<'db>,
    policy: &AnalysisPolicy,
    stop: Option<Stop>,
) -> Result<AnalysisOutcome<Option<Type<'db>>>, AnalysisFailure> {
    let _mode = CycleMode::enter(query);
    observations::reset(None);
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
                match query {
                    DefaultQuery::Bound(variable) => access.bound_typevar_default(variable).await,
                    DefaultQuery::Lazy(variable) => access.lazy_typevar_default(variable).await,
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

#[derive(Clone, Copy, Debug)]
enum DefaultCase {
    BoundPresent,
    BoundAbsent,
    LazyPresent,
    LazyAbsent,
}

fn fixture() -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file("src/main.py", "class Caller: ...\n").unwrap();
    db.write_file(
        "src/defaults.py",
        "from typing import Any, TypeVar\nT = TypeVar(\"T\", default=Any)\n",
    )
    .unwrap();
    db
}

fn wrapped<'db>(db: &'db dyn Db) -> Type<'db> {
    TypeFormType::from_type_expression(db, TypeFormType::from_type_expression(db, Type::any()))
}

impl DefaultCase {
    fn query<'db>(
        self,
        db: &'db TestDb,
        prepared: &PreparedAnalysisFile<'db>,
    ) -> DefaultQuery<'db> {
        let present = matches!(self, Self::BoundPresent | Self::LazyPresent);
        let definition = if present {
            let file = system_path_to_file(db, "src/defaults.py").unwrap();
            let source = prepare_file(db, file).unwrap();
            assert_ne!(source.program_file(), prepared.program_file());
            let Some(Stmt::Assign(assignment)) = source.parsed_module().syntax().body.last() else {
                panic!("fixture TypeVar assignment")
            };
            Some(assignment_definition(&source, assignment))
        } else {
            None
        };
        let stored = match self {
            Self::BoundPresent => Some(TypeVarDefaultEvaluation::Eager(wrapped(db))),
            Self::BoundAbsent => None,
            Self::LazyPresent | Self::LazyAbsent => Some(TypeVarDefaultEvaluation::Lazy),
        };
        let variable = TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new_static("T"),
                definition,
                TypeVarKind::LegacyTypeVar,
            ),
            None,
            None,
            stored,
        );
        match self {
            Self::BoundPresent | Self::BoundAbsent => {
                DefaultQuery::Bound(BoundTypeVarInstance::new(
                    db,
                    variable,
                    BindingContext::Synthetic(prepared.program_file().program(db)),
                    None,
                    TypeVarNonce::NONE,
                ))
            }
            Self::LazyPresent | Self::LazyAbsent => DefaultQuery::Lazy(variable),
        }
    }

    fn expected<'db>(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::BoundPresent => Some(wrapped(db)),
            Self::LazyPresent => Some(Type::any()),
            Self::BoundAbsent | Self::LazyAbsent => None,
        }
    }
}

fn ordinary<'db>(db: &'db TestDb, query: DefaultQuery<'db>) -> Option<Type<'db>> {
    match query {
        DefaultQuery::Bound(variable) => variable.default_type(db),
        DefaultQuery::Lazy(variable) => salsa::attach(db, || {
            *lazy_typevar_default_ingredient(db).fetch(
                db as &dyn Db,
                (db as &dyn Db).zalsa(),
                (db as &dyn Db).zalsa_local(),
                variable.as_id(),
            )
        }),
    }
}

fn memo_status(db: &TestDb, query: DefaultQuery<'_>) -> Result<(), FinalSourceError> {
    match query {
        DefaultQuery::Bound(variable) => FinalSourceMemo::certify(
            db as &dyn Db,
            bound_typevar_default_ingredient(db),
            variable.as_id(),
        )
        .map(|_| ()),
        DefaultQuery::Lazy(variable) => FinalSourceMemo::certify(
            db as &dyn Db,
            lazy_typevar_default_ingredient(db),
            variable.as_id(),
        )
        .map(|_| ()),
    }
}

fn assert_recovery_completed(snapshot: RecoverySnapshot, query: DefaultQuery<'_>) {
    assert!(snapshot.recoveries > 0, "{snapshot:?}");
    assert_eq!(snapshot.completed, snapshot.recoveries);
    assert_eq!(snapshot.owner_drops, snapshot.recoveries);
    assert_eq!(snapshot.self_edges, snapshot.bodies);
    assert_eq!(snapshot.key, Some(query.key().id()));
    assert_eq!(snapshot.initial, Some(DivergentType::new(query.key().id())));
    assert!(snapshot.first_previous_is_seed);
    assert!(
        snapshot.iterations[..snapshot.recoveries]
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
}

#[test]
fn default_recovery_publishes_actual_present_and_absent_values_and_reuses_final_memos() {
    for case in [
        DefaultCase::BoundPresent,
        DefaultCase::BoundAbsent,
        DefaultCase::LazyPresent,
        DefaultCase::LazyAbsent,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let query = case.query(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        assert_eq!(memo_status(&db, query), Err(FinalSourceError::MissingMemo));
        let cold = capture(&db, || controlled(&prepared, query, &funded(), None)).unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(case.expected(&db))),
            "{case:?}"
        );
        cold.check_root_reads().unwrap();
        let snapshot = RECOVERY.get();
        assert_recovery_completed(snapshot, query);
        assert_eq!(
            snapshot.body_result,
            Some(DefaultValue::from_type(case.expected(&db)))
        );
        if matches!(case, DefaultCase::BoundPresent) {
            assert!(snapshot.dropped_children >= 2);
        } else {
            // The source-inferred Any default and absent defaults need no normalization children.
            assert_eq!(snapshot.dropped_children, 0);
        }
        assert_eq!(memo_status(&db, query), Ok(()));
        assert_no_active_attempt();

        let ordinary_db = fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_query = case.query(&ordinary_db, &ordinary_prepared);
        assert_eq!(
            ordinary(&ordinary_db, ordinary_query),
            case.expected(&ordinary_db)
        );

        let key = match query {
            DefaultQuery::Bound(variable) => {
                bound_typevar_default_ingredient(&db).database_key_index(variable.as_id())
            }
            DefaultQuery::Lazy(variable) => {
                lazy_typevar_default_ingredient(&db).database_key_index(variable.as_id())
            }
        };
        let cold_root = cold
            .reads
            .iter()
            .find(|read| read.key == key && read.parent.is_none())
            .unwrap();
        let mut reader = db.clone();
        reader.take_salsa_events();
        let native = capture(&db, || ordinary(&db, query)).unwrap();
        assert_eq!(native.value, case.expected(&db));
        let native_root = native
            .reads
            .iter()
            .find(|read| read.key == key && read.parent.is_none())
            .unwrap();
        assert_eq!(native_root.status, prepared_source_probe::Status::Final);
        assert_eq!(native_root.memo_address, cold_root.memo_address);
        let warm = capture(&db, || controlled(&prepared, query, &funded(), None)).unwrap();
        assert_eq!(warm.value, cold.value);
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
            query.name(),
            Some(query.key().id()),
            &reader.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn bound_default_recovery_child_refusal_drops_before_cycle_owner_and_retries() {
    let db = fixture();
    let prepared = prepare(&db);
    let query = DefaultCase::BoundPresent.query(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled(
            &prepared,
            query,
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
    assert_eq!(
        memo_status(&db, query),
        Err(FinalSourceError::ProvisionalMemo)
    );
    assert_no_active_attempt();
    assert_eq!(
        controlled(&prepared, query, &funded(), None),
        Ok(AnalysisOutcome::Complete(
            DefaultCase::BoundPresent.expected(&db)
        )),
    );
    assert_recovery_completed(RECOVERY.get(), query);
    assert_eq!(memo_status(&db, query), Ok(()));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn bound_default_recovery_child_cancellation_retains_completion_for_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let query = DefaultCase::BoundPresent.query(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(
            &prepared,
            query,
            &funded(),
            Some(Stop {
                child: 2,
                kind: StopKind::Cancel,
            }),
        )
    }));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert_recovery_completed(RECOVERY.get(), query);
    assert!(RECOVERY.get().dropped_children >= 2);
    assert_eq!(memo_status(&db, query), Ok(()));
    assert_no_active_attempt();
    let mut reader = db.clone();
    reader.take_salsa_events();
    assert_eq!(
        controlled(&prepared, query, &funded(), None),
        Ok(AnalysisOutcome::Complete(
            DefaultCase::BoundPresent.expected(&db)
        )),
    );
    assert_eq!((RECOVERY.get().bodies, RECOVERY.get().recoveries), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        query.name(),
        Some(query.key().id()),
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn lazy_default_recovery_work_refusal_retries_with_the_same_source_and_self_edge() {
    let measured_db = fixture();
    let measured_prepared = prepare(&measured_db);
    let measured_query = DefaultCase::LazyPresent.query(&measured_db, &measured_prepared);
    assert_eq!(
        controlled(&measured_prepared, measured_query, &funded(), None),
        Ok(AnalysisOutcome::Complete(Some(Type::any()))),
    );
    // The scalar lazy default has no normalization child at which to interrupt recovery.
    let work = funded().semantic_work_limit - RECOVERY.get().recovery_remaining.unwrap();
    let db = fixture();
    let prepared = prepare(&db);
    let query = DefaultCase::LazyPresent.query(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled(
            &prepared,
            query,
            &AnalysisPolicy {
                semantic_work_limit: work,
                ..funded()
            },
            None
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
    assert_eq!(snapshot.dropped_children, 0);
    assert_eq!(
        memo_status(&db, query),
        Err(FinalSourceError::ProvisionalMemo)
    );
    assert_no_active_attempt();
    assert_eq!(
        controlled(&prepared, query, &funded(), None),
        Ok(AnalysisOutcome::Complete(Some(Type::any()))),
    );
    assert_recovery_completed(RECOVERY.get(), query);
    assert_eq!(memo_status(&db, query), Ok(()));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
