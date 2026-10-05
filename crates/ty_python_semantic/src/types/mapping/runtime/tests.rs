use std::cell::{OnceCell, RefCell};
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

use ruff_db::files::system_path_to_file;
use ruff_db::source::source_text;
use ruff_db::system::DbWithWritableSystem;
use salsa::Database;
use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
use salsa::execution_probe::{
    Demand, ExecutionAdmission, FinalSourceError, FinalSourceMemo, RegistryBuilder,
};
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::types::cached_materialization_ingredient;
use crate::types::cyclic::TypeTransformationStorage;
use crate::types::relation::runtime_resources::CallResourceCapacity;

#[derive(Clone, Copy, Debug)]
enum Refusal {
    None,
    Begin,
    ArgumentRead,
    Child,
    Intern,
    Publication,
    Wrapper,
    Finish,
    Cache,
}

struct Admission<'a> {
    observations: &'a MaterializationObservations,
    refusal: Refusal,
    refused: Cell<bool>,
    execution_error: Cell<Option<RunError>>,
}

impl ExecutionAdmission for Admission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !matches!(work, ExecutionWork::Work { .. }) {
            return Ok(());
        }
        let reject = match self.refusal {
            Refusal::None => false,
            Refusal::Begin => self.observations.stage.get() == Some(MaterializationStage::Begin),
            Refusal::ArgumentRead => {
                self.observations.stage.get() == Some(MaterializationStage::ArgumentRead)
            }
            Refusal::Child => self.observations.stage.get() == Some(MaterializationStage::Child),
            Refusal::Intern => self.observations.stage.get() == Some(MaterializationStage::Intern),
            Refusal::Publication => {
                self.observations.stage.get()
                    == Some(MaterializationStage::Mapping(
                        MappingWork::ResultPublication,
                    ))
                    && self.observations.wrapper_interns.get() == 4
            }
            Refusal::Wrapper => {
                self.observations.stage.get()
                    == Some(MaterializationStage::Mapping(MappingWork::WrapperIntern))
            }

            Refusal::Finish => self.observations.stage.get() == Some(MaterializationStage::Finish),
            Refusal::Cache => matches!(
                self.observations.stage.get(),
                Some(MaterializationStage::Transformation(
                    TypeTransformationWork::CacheStorage { .. }
                ))
            ),
        };
        if reject {
            self.refused.set(true);
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

fn run<'db>(
    db: &'db TestDb,
    ty: Type<'db>,
    kind: MaterializationKind,
    allowance: usize,
    admission: &Admission<'_>,
) -> AttemptOutcome<RunResult<Type<'db>>> {
    let program = db.program_environment().program(db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let forms = OnceCell::new();
    let keys = OnceCell::new();
    try_with_attempt(db, allowance, || {
        let mut registry = RegistryBuilder::new(db, admission)?;
        let route =
            registry.reserve_callable(db as &dyn Db, cached_materialization_ingredient(db))?;
        let registered_keys =
            registry.callable_query_keys::<_, MaterializationKeyProfile>(&route)?;
        let keys = keys.get_or_init(|| registered_keys);
        let registered_forms =
            registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
        let forms = forms.get_or_init(|| registered_forms);
        registry.bind_callable(
            &route,
            MaterializationProvider {
                environments: &environments,
                visitors: &visitors,
                forms: &forms,
                observations: admission.observations,
            },
        )?;
        let queries = MaterializationQueries { route, keys: &keys };
        let result = registry.seal()?.run(|endpoint| async move {
            queries
                .materialization(&endpoint, db, ty, program, kind)
                .await
        });
        admission
            .execution_error
            .set(result.as_ref().err().copied());
        result
    })
    .expect("materialization starts outside a Salsa query")
}

fn complete<'db>(outcome: AttemptOutcome<RunResult<Type<'db>>>) -> Type<'db> {
    match outcome {
        AttemptOutcome::Complete(Ok(ty)) => ty,
        other => panic!("expected completed materialization, got {other:?}"),
    }
}

fn wrapped<'db>(db: &'db dyn Db, mut ty: Type<'db>, count: usize) -> Type<'db> {
    for _ in 0..count {
        ty = TypeFormType::from_type_expression(db, ty);
    }
    ty
}

fn admission(observations: &MaterializationObservations, refusal: Refusal) -> Admission<'_> {
    Admission {
        observations,
        refusal,
        refused: Cell::new(false),
        execution_error: Cell::new(None),
    }
}

#[test]
fn primitive_root_shortcuts_do_not_create_cached_materializations() {
    let db = setup_db();
    let env = db.program_environment();
    for input in [
        Type::Never,
        Type::int_literal(7),
        Type::any(),
        Type::object(),
    ] {
        for kind in [MaterializationKind::Top, MaterializationKind::Bottom] {
            let observations = MaterializationObservations::default();
            let expected = input.materialization(&db, &env, kind);
            let captured = prepared_source_probe::capture(&db, || {
                run(
                    &db,
                    input,
                    kind,
                    100_000,
                    &admission(&observations, Refusal::None),
                )
            })
            .unwrap();
            assert_eq!(complete(captured.value), expected);
            assert_eq!(observations.roots.get(), 0);
            assert_eq!(observations.children.get(), 0);
            let ingredient = cached_materialization_ingredient(&db);
            assert!(
                captured
                    .reads
                    .iter()
                    .all(|read| read.key != ingredient.database_key_index(read.key.key_index()))
            );
        }
    }
}

#[test]
fn cold_typeform_materialization_uses_one_canonical_query_and_same_visitor_children() {
    let db = setup_db();
    let env = db.program_environment();
    let mut reader = db.clone();
    let input = wrapped(&db, Type::any(), 5);
    let mut materialization_keys = Vec::new();
    for (kind, argument) in [
        (MaterializationKind::Top, Type::object()),
        (MaterializationKind::Bottom, Type::Never),
    ] {
        let ordinary_db = setup_db();
        let ordinary_env = ordinary_db.program_environment();
        let ordinary_input = wrapped(&ordinary_db, Type::any(), 5);
        let ordinary_argument = match kind {
            MaterializationKind::Top => Type::object(),
            MaterializationKind::Bottom => Type::Never,
        };
        let ordinary_expected = wrapped(&ordinary_db, ordinary_argument, 5);
        let ordinary = prepared_source_probe::capture(&ordinary_db, || {
            ordinary_input.materialization(&ordinary_db, &ordinary_env, kind)
        })
        .unwrap();
        assert_eq!(ordinary.value, ordinary_expected);
        assert_eq!(ordinary.check_root_reads(), Ok(()));

        assert!(
            !TypeFormType::ingredient(db.zalsa())
                .entries(db.zalsa())
                .any(|entry| entry.value().fields().0 == argument)
        );
        reader.clear_salsa_events();
        let observations = MaterializationObservations::default();
        let cold = prepared_source_probe::capture(&db, || {
            run(
                &db,
                input,
                kind,
                100_000,
                &admission(&observations, Refusal::None),
            )
        })
        .unwrap();
        let actual = complete(cold.value);
        let expected = wrapped(&db, argument, 5);
        assert_eq!(actual, expected);
        assert_eq!(observations.roots.get(), 1);
        assert_eq!(observations.children.get(), 5);
        assert_eq!(observations.completed_children.get(), 5);
        assert_eq!(
            observations.last_child_visitor.get(),
            observations.root_visitor.get()
        );
        assert!(observations.root_visitor.get().is_some());
        assert_eq!(observations.deepest_active.get(), 5);
        assert_eq!(observations.wrapper_interns.get(), 5);
        assert_eq!(observations.unavailable.get(), None);
        let executed: Vec<_> = reader
            .take_salsa_events()
            .into_iter()
            .filter_map(|event| match event.kind {
                salsa::EventKind::WillExecute { database_key } => Some(database_key),
                _ => None,
            })
            .collect();
        assert_eq!(
            executed.len(),
            1,
            "children map within the same visitor instead of creating cached queries"
        );
        let root = cold
            .reads
            .iter()
            .find(|read| read.key == executed[0] && read.parent.is_none())
            .expect("the canonical query is read by the root");
        assert_eq!(root.status, prepared_source_probe::Status::Final);
        materialization_keys.push(root.key);
        let native =
            prepared_source_probe::capture(&db, || input.materialization(&db, &env, kind)).unwrap();
        assert_eq!(native.value, expected);
        assert!(
            native
                .reads
                .iter()
                .any(|read| read.key == root.key && read.memo_address == root.memo_address)
        );
        assert!(
            reader
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
        let warm_observations = MaterializationObservations::default();
        assert_eq!(
            complete(run(
                &db,
                input,
                kind,
                100_000,
                &admission(&warm_observations, Refusal::None)
            )),
            expected
        );
        assert_eq!(warm_observations.roots.get(), 0);
        assert_eq!(warm_observations.children.get(), 0);
        assert!(
            reader
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }
    assert_eq!(materialization_keys.len(), 2);
    assert_ne!(materialization_keys[0], materialization_keys[1]);
}

#[test]
fn unchanged_typeform_still_uses_the_canonical_wrapper_interner() {
    let db = setup_db();
    let input = wrapped(&db, Type::int_literal(1), 1);
    let observations = MaterializationObservations::default();
    assert_eq!(
        complete(run(
            &db,
            input,
            MaterializationKind::Top,
            100_000,
            &admission(&observations, Refusal::None)
        )),
        input
    );
    assert_eq!(observations.roots.get(), 1);
    assert_eq!(observations.wrapper_interns.get(), 1);
    assert_eq!(observations.children.get(), 1);
    let warm = MaterializationObservations::default();
    assert_eq!(
        complete(run(
            &db,
            input,
            MaterializationKind::Top,
            100_000,
            &admission(&warm, Refusal::None)
        )),
        input
    );
    assert_eq!(warm.roots.get(), 0);
    assert_eq!(warm.wrapper_interns.get(), 0);
}

#[test]
fn materialization_refusals_leave_the_real_query_cold_for_retry() {
    for refusal in [
        Refusal::Begin,
        Refusal::ArgumentRead,
        Refusal::Child,
        Refusal::Wrapper,
        Refusal::Intern,
        Refusal::Finish,
        Refusal::Cache,
        Refusal::Publication,
    ] {
        let db = setup_db();
        let input = wrapped(&db, Type::any(), 4);
        let observations = MaterializationObservations::default();
        let control = admission(&observations, refusal);
        let stamp = prepared_source_probe::Stamp::current(&db);
        let captured = prepared_source_probe::capture(&db, || {
            run(&db, input, MaterializationKind::Top, 100_000, &control)
        })
        .unwrap();
        assert!(control.refused.get(), "{refusal:?}");
        assert!(
            matches!(
                captured.value,
                AttemptOutcome::Incomplete(Incomplete::Allowance)
            ),
            "{refusal:?}: {:?}",
            captured.value
        );
        assert_eq!(
            control.execution_error.get(),
            Some(RunError::Refused(Incomplete::Allowance))
        );
        let ingredient = cached_materialization_ingredient(&db);
        let program = db.program_environment().program(&db);
        let id =
            existing_key(&db, ingredient, &(input, program, MaterializationKind::Top)).unwrap();
        assert!(matches!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
            Err(FinalSourceError::MissingMemo)
        ));
        let key = ingredient.database_key_index(id);
        assert!(!captured.reads.iter().any(|read| read.key == key));
        assert_materialization_idle(&db);
        if matches!(refusal, Refusal::Publication) {
            assert_eq!(observations.completed_children.get(), 4);
        }

        let retry = MaterializationObservations::default();
        let expected = wrapped(&db, Type::object(), 4);
        assert_eq!(
            complete(run(
                &db,
                input,
                MaterializationKind::Top,
                100_000,
                &admission(&retry, Refusal::None)
            )),
            expected
        );
        assert_eq!(retry.roots.get(), 1);
        assert_eq!(retry.children.get(), 4);
        assert_eq!(prepared_source_probe::Stamp::current(&db), stamp);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        assert_materialization_warm(&db, input, program, MaterializationKind::Top, id, expected);
    }
}

#[derive(Debug)]
struct MaterializationPanic(Arc<()>);

#[test]
fn materialization_allowance_allows_a_fresh_root() {
    let db = setup_db();
    let input = wrapped(&db, Type::any(), 4);
    let observations = MaterializationObservations::default();
    assert!(matches!(
        run(
            &db,
            input,
            MaterializationKind::Bottom,
            0,
            &admission(&observations, Refusal::None)
        ),
        AttemptOutcome::Incomplete(Incomplete::Allowance)
    ));
    let retry = MaterializationObservations::default();
    let actual = complete(run(
        &db,
        input,
        MaterializationKind::Bottom,
        100_000,
        &admission(&retry, Refusal::None),
    ));
    assert_eq!(actual, wrapped(&db, Type::Never, 4));
    assert_eq!(retry.roots.get(), 1);
}

#[test]
fn native_materialization_panic_preserves_payload_and_revision_poison() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_file("/src/materialization.py", "# initial revision\n")
        .build()?;
    let file = system_path_to_file(&db, "/src/materialization.py")?;
    assert_eq!(source_text(&db, file).as_str(), "# initial revision\n");
    let old_stamp = prepared_source_probe::Stamp::current(&db);
    let id = {
        let input = wrapped(&db, Type::any(), 4);
        let ingredient = cached_materialization_ingredient(&db);
        let program = db.program_environment().program(&db);
        let observations = MaterializationObservations::default();
        let journal = CleanupJournal::new(CleanupBoundary::Panic);
        let captured = prepared_source_probe::capture(&db, || {
            catch_unwind(AssertUnwindSafe(|| {
                run_cleanup(
                    &db,
                    input,
                    MaterializationKind::Bottom,
                    &observations,
                    &journal,
                )
            }))
        })
        .unwrap();
        let payload = captured.value.unwrap_err();
        let payload = payload
            .downcast::<MaterializationPanic>()
            .expect("the original panic payload survives");
        assert!(Arc::ptr_eq(&payload.0, &journal.panic_identity));
        journal.assert_finished();
        assert_cleanup_mapping_finished(&observations, 4);
        assert_eq!(journal.execution_error.get(), None);
        let id = existing_key(
            &db,
            ingredient,
            &(input, program, MaterializationKind::Bottom),
        )
        .unwrap();
        assert_eq!(journal.key.get(), Some(id));
        assert!(
            !captured
                .reads
                .iter()
                .any(|read| read.key == ingredient.database_key_index(id))
        );
        assert!(
            matches!(
                FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
                Err(FinalSourceError::ProvisionalMemo)
            ),
            "the native panic poisons the current revision"
        );
        assert_materialization_idle(&db);
        assert_eq!(prepared_source_probe::Stamp::current(&db), old_stamp);
        let mut reader = db.clone();
        reader.clear_salsa_events();
        let retry = MaterializationObservations::default();
        let poisoned = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            run(
                &db,
                input,
                MaterializationKind::Bottom,
                100_000,
                &admission(&retry, Refusal::None),
            )
        }));
        assert!(
            matches!(poisoned, Err(salsa::Cancelled::PropagatedPanic)),
            "same-revision retry retains native poison"
        );
        assert_eq!(retry.roots.get(), 0);
        assert_no_materialization_execution(&mut reader, ingredient.database_key_index(id));
        assert_materialization_idle(&db);
        assert_eq!(prepared_source_probe::Stamp::current(&db), old_stamp);
        assert!(matches!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
            Err(FinalSourceError::ProvisionalMemo)
        ));
        id
    };
    db.write_file("/src/materialization.py", "# next revision\n")?;
    assert_ne!(
        prepared_source_probe::Stamp::current(&db),
        old_stamp,
        "the file edit advances Salsa's revision"
    );
    assert_eq!(source_text(&db, file).as_str(), "# next revision\n");
    let input = wrapped(&db, Type::any(), 4);
    let ingredient = cached_materialization_ingredient(&db);
    assert!(
        matches!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
            Err(FinalSourceError::UnverifiedMemo)
        ),
        "the old poison is unverified in the new revision"
    );
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let retry = MaterializationObservations::default();
    let actual = complete(run(
        &db,
        input,
        MaterializationKind::Bottom,
        100_000,
        &admission(&retry, Refusal::None),
    ));
    assert_eq!(actual, wrapped(&db, Type::Never, 4));
    assert_eq!(retry.roots.get(), 1);
    assert!(reader.take_salsa_events().iter().any(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == ingredient.database_key_index(id))));
    assert_eq!(
        existing_key(
            &db,
            ingredient,
            &(
                input,
                db.program_environment().program(&db),
                MaterializationKind::Bottom
            )
        ),
        Some(id)
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_materialization_warm(
        &db,
        input,
        db.program_environment().program(&db),
        MaterializationKind::Bottom,
        id,
        actual,
    );
    Ok(())
}

#[test]
fn unsupported_typevar_materialization_is_a_protected_child_refusal() {
    let db = setup_db();
    let env = db.program_environment();
    let variable = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        ruff_python_ast::name::Name::new_static("T"),
        crate::types::TypeVarVariance::Invariant,
    );
    let input = wrapped(&db, Type::TypeVar(variable), 2);
    let observations = MaterializationObservations::default();
    let control = admission(&observations, Refusal::None);
    let result = run(&db, input, MaterializationKind::Top, 100_000, &control);
    assert!(matches!(
        result,
        AttemptOutcome::Incomplete(Incomplete::Interrupted)
    ));
    assert_eq!(
        control.execution_error.get(),
        Some(RunError::Refused(Incomplete::Interrupted))
    );
    assert_eq!(
        observations.unavailable.get(),
        Some(UnavailableMaterialization::TypeVar)
    );
    assert_eq!(observations.roots.get(), 1);
    assert_eq!(observations.children.get(), 2);
    assert_eq!(observations.wrapper_interns.get(), 0);
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(
        &db,
        ingredient,
        &(input, env.program(&db), MaterializationKind::Top),
    )
    .unwrap();
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
        Err(FinalSourceError::MissingMemo)
    ));
    assert_materialization_idle(&db);
}

#[test]
fn same_visitor_separates_top_and_bottom_and_reuses_completed_siblings() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let input = wrapped(&db, Type::any(), 3);
    let top = wrapped(&db, Type::object(), 3);
    let bottom = wrapped(&db, Type::Never, 3);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let observations = MaterializationObservations::default();
    let control = admission(&observations, Refusal::None);
    let forms = OnceCell::new();
    let outcome = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &control)?;
        let registered_forms =
            registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
        let forms = forms.get_or_init(|| registered_forms);
        let environments = &environments;
        let visitors = &visitors;
        let forms = &forms;
        let observations = &observations;
        let db = &db;
        registry.seal()?.run(move |endpoint| async move {
            let env = environments.allocate(&endpoint, program).await;
            let visitor = visitors.allocate(&endpoint, env).await;
            let effects = MaterializationMapping {
                endpoint: &endpoint,
                visitor,
                forms,
                observations,
            };
            for (kind, expected, child_count) in [
                (MaterializationKind::Top, top, 3),
                (MaterializationKind::Bottom, bottom, 6),
                (MaterializationKind::Top, top, 6),
            ] {
                let result = input
                    .apply_type_mapping_with(
                        db,
                        &TypeMapping::Materialize(kind),
                        TypeContext::default(),
                        visitor,
                        &effects,
                    )
                    .await?;
                assert_eq!(result, expected);
                assert_eq!(observations.children.get(), child_count);
            }
            Ok(top)
        })
    })
    .unwrap();
    assert_eq!(complete(outcome), top);
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

struct RecoveryProvider<'run, 'db: 'run, C: MaterializationConfiguration> {
    inner: MaterializationProvider<'run, 'db>,
    route: CallableRoute<'run, 'db, C>,
    keys: &'run QueryKeys<'db, C, MaterializationKeyProfile>,
    initial: &'run Cell<Option<Type<'db>>>,
    computed: &'run Cell<Option<Type<'db>>>,
    recovered: &'run Cell<bool>,
    cleanup: Option<CleanupContext<'run, 'db>>,
}

impl<'run, 'db: 'run, C: MaterializationConfiguration> CallableRouteProvider<'run, 'db, C>
    for RecoveryProvider<'run, 'db, C>
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
        <MaterializationProvider<'run, 'db> as CallableRouteProvider<'run, 'db, C>>::native_value(
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
        if let Some(cleanup) = self.cleanup {
            cleanup
                .journal
                .initial
                .set(cleanup.journal.initial.get() + 1);
        }
        let _owner = self
            .cleanup
            .filter(|cleanup| cleanup.journal.boundary == CleanupBoundary::Initial)
            .map(|cleanup| cleanup.owner(&endpoint, CleanupHeld::Initial { id, input: &input }));
        let value =
            <MaterializationProvider<'run, 'db> as CallableRouteProvider<'run, 'db, C>>::initial(
                &self.inner,
                endpoint,
                db,
                id,
                input,
            )
            .await?;
        self.initial.set(Some(value));
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
        if let Some(cleanup) = self.cleanup {
            cleanup.journal.body.set(cleanup.journal.body.get() + 1);
        }
        let id = endpoint.intern_query_key(self.keys, input).await;
        if let Some(cleanup) = self.cleanup {
            assert!(cleanup.journal.key.replace(Some(id)).is_none());
        }
        let value = if let Some(cleanup) = self.cleanup
            && cleanup.journal.boundary.observes_finish()
        {
            let (ty, program, kind) = input;
            let env = self.inner.environments.allocate(&endpoint, program).await;
            let visitor = self.inner.visitors.allocate(&endpoint, env).await;
            endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    self.inner
                        .observations
                        .roots
                        .set(self.inner.observations.roots.get() + 1);
                    self.inner
                        .observations
                        .root_visitor
                        .set(Some(std::ptr::from_ref(visitor).cast()));
                    Ok(())
                })
                .await;
            let effects = ObservedFinish {
                inner: MaterializationMapping {
                    endpoint: &endpoint,
                    visitor,
                    forms: self.inner.forms,
                    observations: self.inner.observations,
                },
                cleanup,
                db,
            };
            ty.apply_type_mapping_with(
                db,
                &TypeMapping::Materialize(kind),
                TypeContext::default(),
                visitor,
                &effects,
            )
            .await?
        } else {
            // The self-edge reaches native cycle callbacks for an otherwise acyclic input.
            let seed = endpoint
                .child_call(|| async { Ok(*endpoint.fetch_ref(&self.route, id)?.await?) })
                .await;
            assert_eq!(Some(seed), self.initial.get());
            <MaterializationProvider<'run, 'db> as CallableRouteProvider<'run, 'db, C>>::body(
                &self.inner,
                endpoint,
                db,
                input,
            )
            .await?
        };
        self.computed.set(Some(value));
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
        assert_eq!(Some(*last), self.initial.get());
        assert_eq!(Some(value), self.computed.get());
        assert_ne!(value, *last);
        assert!(cycle.head_ids().any(|id| id == cycle.id()));
        self.recovered.set(true);
        if let Some(cleanup) = self.cleanup {
            cleanup
                .journal
                .recovery
                .set(cleanup.journal.recovery.get() + 1);
        }
        let _owner = self
            .cleanup
            .filter(|cleanup| cleanup.journal.boundary == CleanupBoundary::Recovery)
            .map(|cleanup| {
                cleanup.owner(
                    &endpoint,
                    CleanupHeld::Recovery {
                        cycle,
                        last,
                        value,
                        input: &input,
                        expected_last: self.initial.get().unwrap(),
                        expected_value: self.computed.get().unwrap(),
                    },
                )
            });
        <MaterializationProvider<'run, 'db> as CallableRouteProvider<'run, 'db, C>>::recover(
            &self.inner,
            endpoint,
            db,
            cycle,
            last,
            value,
            input,
        )
        .await
    }
}

#[test]
fn registered_materialization_recovery_refuses_after_the_real_seed_and_body() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let input = wrapped(&db, Type::any(), 2);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let observations = MaterializationObservations::default();
    let control = admission(&observations, Refusal::None);
    let initial = Cell::new(None);
    let computed = Cell::new(None);
    let recovered = Cell::new(false);
    let forms = OnceCell::new();
    let keys = OnceCell::new();
    let outcome = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &control)?;
        let route =
            registry.reserve_callable(&db as &dyn Db, cached_materialization_ingredient(&db))?;
        let registered_keys =
            registry.callable_query_keys::<_, MaterializationKeyProfile>(&route)?;
        let keys = keys.get_or_init(|| registered_keys);
        let registered_forms =
            registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
        let forms = forms.get_or_init(|| registered_forms);
        registry.bind_callable(
            &route,
            RecoveryProvider {
                inner: MaterializationProvider {
                    environments: &environments,
                    visitors: &visitors,
                    forms: &forms,
                    observations: &observations,
                },
                route: route.clone(),
                keys: &keys,
                initial: &initial,
                computed: &computed,
                recovered: &recovered,
                cleanup: None,
            },
        )?;
        let queries = MaterializationQueries { route, keys: &keys };
        let db = &db;
        let result = registry.seal()?.run(move |endpoint| async move {
            queries
                .materialization(&endpoint, db, input, program, MaterializationKind::Bottom)
                .await
        });
        control.execution_error.set(result.as_ref().err().copied());
        result
    })
    .unwrap();
    assert!(matches!(
        outcome,
        AttemptOutcome::Incomplete(Incomplete::Interrupted)
    ));
    assert_eq!(
        control.execution_error.get(),
        Some(RunError::Refused(Incomplete::Interrupted))
    );
    assert!(recovered.get());
    assert_eq!(
        observations.unavailable.get(),
        Some(UnavailableMaterialization::Recovery)
    );
    assert_eq!(computed.get(), Some(wrapped(&db, Type::Never, 2)));
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(
        &db,
        ingredient,
        &(input, program, MaterializationKind::Bottom),
    )
    .unwrap();
    assert_eq!(
        initial.get(),
        Some(Type::Divergent(
            DivergentType::new(id).materialized(MaterializationKind::Bottom)
        ))
    );
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id),
        Err(FinalSourceError::ProvisionalMemo)
    ));
    let retry = MaterializationObservations::default();
    let actual = complete(run(
        &db,
        input,
        MaterializationKind::Bottom,
        100_000,
        &admission(&retry, Refusal::None),
    ));
    assert_eq!(actual, computed.get().unwrap());
    assert_eq!(retry.roots.get(), 1);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());

    let seed = initial.get().unwrap();
    let shortcut = MaterializationObservations::default();
    let env = db.program_environment();
    assert_eq!(
        complete(run(
            &db,
            seed,
            MaterializationKind::Top,
            100_000,
            &admission(&shortcut, Refusal::None)
        )),
        seed.materialization(&db, &env, MaterializationKind::Top)
    );
    assert_eq!(shortcut.roots.get(), 0);
}

fn assert_materialization_idle(db: &TestDb) {
    assert_eq!(
        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
        None
    );
    assert!(matches!(
        try_with_attempt(db, 0, || ()),
        Ok(AttemptOutcome::Complete(()))
    ));
}

fn assert_cleanup_mapping_finished(observations: &MaterializationObservations, depth: usize) {
    assert_eq!(observations.roots.get(), 1);
    assert_eq!(observations.children.get(), depth);
    assert_eq!(observations.completed_children.get(), depth);
    assert_eq!(observations.wrapper_interns.get(), depth);
    assert_eq!(observations.deepest_active.get(), depth);
    assert!(observations.root_visitor.get().is_some());
    assert_eq!(
        observations.last_child_visitor.get(),
        observations.root_visitor.get()
    );
}

fn assert_no_materialization_execution(reader: &mut TestDb, key: salsa::DatabaseKeyIndex) {
    assert!(!reader.take_salsa_events().iter().any(|event| matches!(
        event.kind,
        salsa::EventKind::WillExecute { database_key } if database_key == key
    )));
}

fn assert_materialization_warm<'db>(
    db: &'db TestDb,
    input: Type<'db>,
    program: Program<'db>,
    kind: MaterializationKind,
    id: salsa::Id,
    expected: Type<'db>,
) {
    let ingredient = cached_materialization_ingredient(db);
    assert_eq!(
        existing_key(db, ingredient, &(input, program, kind)),
        Some(id)
    );
    assert!(FinalSourceMemo::certify(db as &dyn Db, ingredient, id).is_ok());
    let key = ingredient.database_key_index(id);
    let stamp = prepared_source_probe::Stamp::current(db);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let ordinary = prepared_source_probe::capture(db, || {
        input.materialization(db, &db.program_environment(), kind)
    })
    .unwrap();
    assert_eq!(ordinary.value, expected);
    let native = ordinary
        .reads
        .iter()
        .find(|read| read.key == key && read.parent.is_none())
        .unwrap();
    assert_eq!(native.status, prepared_source_probe::Status::Final);
    assert_no_materialization_execution(&mut reader, key);
    let observations = MaterializationObservations::default();
    let warm = prepared_source_probe::capture(db, || {
        run(
            db,
            input,
            kind,
            100_000,
            &admission(&observations, Refusal::None),
        )
    })
    .unwrap();
    assert_eq!(complete(warm.value), expected);
    assert_eq!(observations.roots.get(), 0);
    assert_eq!(observations.children.get(), 0);
    let selected = warm
        .reads
        .iter()
        .find(|read| read.key == key && read.parent.is_none())
        .unwrap();
    assert_eq!(selected.status, prepared_source_probe::Status::Final);
    assert_eq!(selected.memo_address, native.memo_address);
    assert_eq!(selected.stamp, stamp);
    assert_no_materialization_execution(&mut reader, key);
    assert_materialization_idle(db);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupBoundary {
    Initial,
    Recovery,
    Finish,
    Cache,
    CacheAcceptance,
    Growth,
    RehashKey,
    Resource,
    Panic,
}

impl CleanupBoundary {
    fn observes_finish(self) -> bool {
        matches!(
            self,
            Self::Finish
                | Self::Cache
                | Self::CacheAcceptance
                | Self::Growth
                | Self::RehashKey
                | Self::Resource
                | Self::Panic
        )
    }

    fn selected(self, stage: Option<MaterializationStage>) -> bool {
        match self {
            Self::Initial => stage == Some(MaterializationStage::Initial),
            Self::Recovery => stage == Some(MaterializationStage::Recovery),
            Self::Finish | Self::Panic => stage == Some(MaterializationStage::Finish),
            Self::Cache | Self::CacheAcceptance => matches!(
                stage,
                Some(MaterializationStage::Transformation(
                    TypeTransformationWork::CacheStorage { .. }
                ))
            ),
            Self::Growth | Self::Resource => matches!(
                stage,
                Some(MaterializationStage::Transformation(
                    TypeTransformationWork::Grow {
                        storage: TypeTransformationStorage::Cache,
                        ..
                    }
                ))
            ),
            Self::RehashKey => matches!(
                stage,
                Some(MaterializationStage::Transformation(
                    TypeTransformationWork::RehashKey { .. }
                ))
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum FinishCancellationPhase {
    Idle,
    AdmissionResume,
    LocalFinalResume,
    Fired,
}

struct FinishCancellation {
    phase: AtomicU8,
    cancel_at: FinishCancellationPhase,
    admission_resume: AtomicBool,
    local_final_resume: AtomicBool,
    requested: AtomicBool,
    token: OnceLock<salsa::CancellationToken>,
}

impl FinishCancellation {
    fn new(cancel_at: FinishCancellationPhase) -> Self {
        Self {
            phase: AtomicU8::new(FinishCancellationPhase::Idle as u8),
            cancel_at,
            admission_resume: AtomicBool::new(false),
            local_final_resume: AtomicBool::new(false),
            requested: AtomicBool::new(false),
            token: OnceLock::new(),
        }
    }

    fn arm(&self) {
        self.phase.store(
            FinishCancellationPhase::AdmissionResume as u8,
            Ordering::SeqCst,
        );
    }

    fn observe(&self, event: &salsa::EventKind) {
        if !matches!(event, salsa::EventKind::WillCheckCancellation) {
            return;
        }
        let observed = match self.phase.load(Ordering::SeqCst) {
            phase if phase == FinishCancellationPhase::AdmissionResume as u8 => {
                self.admission_resume.store(true, Ordering::SeqCst);
                FinishCancellationPhase::AdmissionResume
            }
            phase if phase == FinishCancellationPhase::LocalFinalResume as u8 => {
                self.local_final_resume.store(true, Ordering::SeqCst);
                FinishCancellationPhase::LocalFinalResume
            }
            _ => return,
        };
        if observed == self.cancel_at {
            self.phase
                .store(FinishCancellationPhase::Fired as u8, Ordering::SeqCst);
            if let Some(token) = self.token.get() {
                token.cancel();
                self.requested.store(token.is_cancelled(), Ordering::SeqCst);
            }
        } else {
            self.phase.store(
                FinishCancellationPhase::LocalFinalResume as u8,
                Ordering::SeqCst,
            );
        }
    }
}

struct CleanupJournal {
    boundary: CleanupBoundary,
    armed: Cell<bool>,
    owner_live: Cell<bool>,
    queued: Cell<usize>,
    started: Cell<usize>,
    initial: Cell<usize>,
    body: Cell<usize>,
    recovery: Cell<usize>,
    admitted: Cell<usize>,
    key: Cell<Option<salsa::Id>>,
    cache_len: Cell<Option<usize>>,
    execution_error: Cell<Option<RunError>>,
    drops: RefCell<Vec<&'static str>>,
    panic_identity: Arc<()>,
    accepted_child: Cell<Option<FinishCleanupObservation>>,
    accepted_owner: Cell<Option<FinishCleanupObservation>>,
    after_finish: Cell<bool>,
    acceptance_work: Cell<Option<TypeTransformationWork>>,
    finish_cancellation: Option<Arc<FinishCancellation>>,
}

impl CleanupJournal {
    fn new(boundary: CleanupBoundary) -> Self {
        Self {
            boundary,
            armed: Cell::new(false),
            owner_live: Cell::new(false),
            queued: Cell::new(0),
            started: Cell::new(0),
            initial: Cell::new(0),
            body: Cell::new(0),
            recovery: Cell::new(0),
            admitted: Cell::new(0),
            key: Cell::new(None),
            cache_len: Cell::new(None),
            execution_error: Cell::new(None),
            drops: RefCell::new(Vec::new()),
            panic_identity: Arc::new(()),
            accepted_child: Cell::new(None),
            accepted_owner: Cell::new(None),
            after_finish: Cell::new(false),
            acceptance_work: Cell::new(None),
            finish_cancellation: None,
        }
    }

    fn assert_finished(&self) {
        assert!(!self.armed.get());
        assert!(!self.owner_live.get());
        assert_eq!(self.queued.get(), 1);
        assert_eq!(self.started.get(), 0);
        assert_eq!(self.admitted.get(), 1);
        assert_eq!(self.body.get(), 1);
        assert_eq!(
            self.initial.get(),
            usize::from(!self.boundary.observes_finish())
        );
        assert_eq!(
            self.recovery.get(),
            usize::from(self.boundary == CleanupBoundary::Recovery)
        );
        assert_eq!(&*self.drops.borrow(), &["child", "owner"]);
        assert_eq!(
            self.cache_len.get().is_some(),
            self.boundary.observes_finish()
        );
    }
}

#[derive(Clone, Copy)]
struct FinishWitness<'run, 'db> {
    db: &'db dyn Db,
    visitor: &'run ApplyTypeMappingVisitor<'run, 'db>,
    input: Type<'db>,
    result: Type<'db>,
    kind: MaterializationKind,
    prefetched_children: Option<(Type<'db>, Type<'db>)>,
}

#[derive(Debug, PartialEq, Eq)]
struct ProbeStopped {
    active: usize,
}

#[derive(Default)]
struct TransformationProbe {
    cache_len: Cell<usize>,
}

impl TypeTransformationControl for TransformationProbe {
    type Error = ProbeStopped;

    fn checkpoint(&self, work: TypeTransformationWork) -> Result<(), Self::Error> {
        match work {
            TypeTransformationWork::CacheLookup { len } => self.cache_len.set(len),
            TypeTransformationWork::AncestorComparison
            | TypeTransformationWork::InlinePayload { .. } => {}
            // Refuse before begin_visit_with can push an entry or create a scope.
            TypeTransformationWork::ActiveStorage { len, .. } => {
                return Err(ProbeStopped { active: len });
            }
            TypeTransformationWork::CacheStorage { .. }
            | TypeTransformationWork::RehashKey { .. }
            | TypeTransformationWork::Grow { .. } => {
                panic!("a read-only probe cannot finish a transformation")
            }
        }
        Ok(())
    }

    fn identity<'db>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<TypeIdentity<'db>, Self::Error> {
        assert!(matches!(ty, Type::TypeForm(_)));
        Ok(ty.to_type_identity(db))
    }

    fn prepare_growth(
        &self,
        _request: TypeTransformationGrowth,
    ) -> Result<Option<GrowthPlan>, ProbeStopped> {
        panic!("the read-only probe must refuse before a growth request")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinishLookupObservation {
    Original,
    Mapped,
    Absent { active: usize },
    Other,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FinishCleanupObservation {
    root: FinishLookupObservation,
    child: FinishLookupObservation,
    cache_len: usize,
    owner_live: bool,
}

#[derive(Default)]
struct FinishCleanupProbe {
    cache_len: Cell<usize>,
}

impl TypeTransformationControl for FinishCleanupProbe {
    type Error = Option<usize>;

    fn checkpoint(&self, work: TypeTransformationWork) -> Result<(), Self::Error> {
        match work {
            TypeTransformationWork::CacheLookup { len } => self.cache_len.set(len),
            TypeTransformationWork::AncestorComparison
            | TypeTransformationWork::InlinePayload { .. } => {}
            TypeTransformationWork::ActiveStorage { len, .. } => return Err(Some(len)),
            _ => return Err(None),
        }
        Ok(())
    }

    fn identity<'db>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<TypeIdentity<'db>, Self::Error> {
        match ty {
            // TypeForm's existing identity path is fixed and makes no database reads.
            Type::TypeForm(_) => Ok(ty.to_type_identity(db)),
            _ => Err(None),
        }
    }

    fn prepare_growth(
        &self,
        _request: TypeTransformationGrowth,
    ) -> Result<Option<GrowthPlan>, Self::Error> {
        Err(None)
    }
}

impl FinishWitness<'_, '_> {
    fn observe_scope(self, owner_live: bool) -> FinishCleanupObservation {
        let mut observation = FinishCleanupObservation {
            root: FinishLookupObservation::Unavailable,
            child: FinishLookupObservation::Unavailable,
            cache_len: 0,
            owner_live,
        };
        let mapping = TypeMapping::Materialize(self.kind);
        let Some(transformer) = self.visitor.transformer_cell(&mapping).get() else {
            return observation;
        };
        let probe = FinishCleanupProbe::default();
        let lookup = |input, mapped| match transformer.begin_visit_with(self.db, input, &probe) {
            Ok(TypeTransformerVisit::Ready(value)) if value == input => {
                FinishLookupObservation::Original
            }
            Ok(TypeTransformerVisit::Ready(value)) if value == mapped => {
                FinishLookupObservation::Mapped
            }
            Err(Some(active)) => FinishLookupObservation::Absent { active },
            _ => FinishLookupObservation::Other,
        };
        observation.root = lookup(self.input, self.result);
        if let Some((input, mapped)) = self.prefetched_children {
            observation.child = lookup(input, mapped);
        }
        observation.cache_len = probe.cache_len.get();
        observation
    }

    fn assert_scope(self, active: bool) -> usize {
        assert_ne!(self.result, self.input);
        let mapping = TypeMapping::Materialize(self.kind);
        let transformer = self.visitor.transformer_cell(&mapping).get().unwrap();
        let probe = TransformationProbe::default();
        match (
            active,
            transformer.begin_visit_with(self.db, self.input, &probe),
        ) {
            (true, Ok(TypeTransformerVisit::Ready(value))) => assert_eq!(value, self.input),
            (false, Err(stopped)) => assert_eq!(stopped, ProbeStopped { active: 0 }),
            _ => panic!("the root's actual active entry must follow the finish future's lifetime"),
        }
        let len = probe.cache_len.get();
        assert!(len > 0, "a nested TypeForm child has already completed");
        let Type::TypeForm(input) = self.input else {
            panic!("expected the nested TypeForm fixture")
        };
        let Type::TypeForm(result) = self.result else {
            panic!("expected the actual mapped TypeForm")
        };
        let child = *input
            .read_fields(salsa::FieldReads::new(self.db))
            .type_argument();
        let mapped_child = *result
            .read_fields(salsa::FieldReads::new(self.db))
            .type_argument();
        assert!(matches!(child, Type::TypeForm(_)));
        match transformer.begin_visit_with(self.db, child, &probe) {
            Ok(TypeTransformerVisit::Ready(value)) => assert_eq!(value, mapped_child),
            _ => panic!("the completed child's transformation cache must survive the root's abort"),
        }
        assert_eq!(probe.cache_len.get(), len);
        len
    }
}

#[derive(Clone, Copy)]
struct CleanupContext<'run, 'db: 'run> {
    journal: &'run CleanupJournal,
    input: (Type<'db>, Program<'db>, MaterializationKind),
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    witness: &'run Cell<Option<FinishWitness<'run, 'db>>>,
}

impl<'run, 'db: 'run> CleanupContext<'run, 'db> {
    fn owner<'a>(
        self,
        endpoint: &TaskEndpoint<'run, 'db>,
        held: CleanupHeld<'a, 'db>,
    ) -> CleanupOwner<'a, 'run, 'db> {
        assert!(
            self.endpoint
                .borrow_mut()
                .replace(endpoint.clone())
                .is_none()
        );
        assert!(!self.journal.owner_live.replace(true));
        assert!(!self.journal.armed.replace(true));
        CleanupOwner {
            context: self,
            held,
        }
    }
}

enum CleanupHeld<'a, 'db> {
    Initial {
        id: salsa::Id,
        input: &'a (Type<'db>, Program<'db>, MaterializationKind),
    },
    Recovery {
        cycle: &'a salsa::Cycle<'a>,
        last: &'a Type<'db>,
        value: Type<'db>,
        input: &'a (Type<'db>, Program<'db>, MaterializationKind),
        expected_last: Type<'db>,
        expected_value: Type<'db>,
    },
    Finish,
}

struct CleanupOwner<'a, 'run, 'db: 'run> {
    context: CleanupContext<'run, 'db>,
    held: CleanupHeld<'a, 'db>,
}

impl Drop for CleanupOwner<'_, '_, '_> {
    fn drop(&mut self) {
        let journal = self.context.journal;
        if journal.boundary == CleanupBoundary::CacheAcceptance {
            journal.accepted_owner.set(
                self.context
                    .witness
                    .get()
                    .map(|witness| witness.observe_scope(journal.owner_live.get())),
            );
            journal.owner_live.set(false);
            journal.drops.borrow_mut().push("owner");
            return;
        }
        match &self.held {
            CleanupHeld::Initial { id, input } => {
                assert_eq!(Some(*id), journal.key.get());
                assert_eq!(**input, self.context.input);
            }
            CleanupHeld::Recovery {
                cycle,
                last,
                value,
                input,
                expected_last,
                expected_value,
            } => {
                assert_eq!(Some(cycle.id()), journal.key.get());
                assert!(cycle.head_ids().any(|id| id == cycle.id()));
                assert_eq!(**last, *expected_last);
                assert_eq!(*value, *expected_value);
                assert_eq!(**input, self.context.input);
            }
            CleanupHeld::Finish => {
                let witness = self.context.witness.get().unwrap();
                assert_eq!(Some(witness.assert_scope(false)), journal.cache_len.get());
            }
        }
        assert_eq!(&*journal.drops.borrow(), &["child"]);
        assert!(journal.owner_live.replace(false));
        journal.drops.borrow_mut().push("owner");
    }
}

struct CleanupChild<'run, 'db> {
    journal: &'run CleanupJournal,
    witness: Option<FinishWitness<'run, 'db>>,
}

impl Drop for CleanupChild<'_, '_> {
    fn drop(&mut self) {
        if self.journal.boundary == CleanupBoundary::CacheAcceptance {
            self.journal.accepted_child.set(
                self.witness
                    .map(|witness| witness.observe_scope(self.journal.owner_live.get())),
            );
            self.journal.drops.borrow_mut().push("child");
            return;
        }
        assert!(self.journal.owner_live.get());
        if let Some(witness) = self.witness {
            assert!(
                self.journal
                    .cache_len
                    .replace(Some(witness.assert_scope(true)))
                    .is_none()
            );
        }
        assert!(self.journal.drops.borrow().is_empty());
        self.journal.drops.borrow_mut().push("child");
    }
}

struct CleanupAdmission<'run, 'db: 'run> {
    observations: &'run MaterializationObservations,
    context: CleanupContext<'run, 'db>,
}

impl ExecutionAdmission for CleanupAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let journal = self.context.journal;
        let selected_work = if journal.boundary == CleanupBoundary::Resource {
            matches!(work, ExecutionWork::Resource { .. })
        } else {
            matches!(work, ExecutionWork::Work { .. })
        };
        if !selected_work
            || !journal.armed.get()
            || !journal.boundary.selected(self.observations.stage.get())
        {
            return Ok(());
        }
        if journal.boundary == CleanupBoundary::CacheAcceptance
            && let Some(MaterializationStage::Transformation(work)) = self.observations.stage.get()
        {
            journal.acceptance_work.set(Some(work));
        }
        assert!(journal.armed.replace(false));
        journal.admitted.set(journal.admitted.get() + 1);
        let endpoint = self.context.endpoint.borrow().as_ref().cloned().unwrap();
        let child = CleanupChild {
            journal,
            witness: self.context.witness.get(),
        };
        let pending = endpoint.demand(move || {
            child.journal.started.set(child.journal.started.get() + 1);
            std::future::poll_fn(move |_| -> std::task::Poll<RunResult<()>> {
                let _child = &child;
                panic!("the refused callback must drain its queued child without polling it");
            })
        })?;
        journal.queued.set(journal.queued.get() + 1);
        assert!(self.context.pending.borrow_mut().replace(pending).is_none());
        // Demand creation has its own resume checks. Arm only once those checks have completed:
        // the next event is admission's final check, followed by the local callback's final check.
        if let Some(cancellation) = &journal.finish_cancellation {
            cancellation.arm();
        }
        match journal.boundary {
            CleanupBoundary::Recovery | CleanupBoundary::CacheAcceptance => Ok(()),
            CleanupBoundary::Panic => {
                std::panic::panic_any(MaterializationPanic(Arc::clone(&journal.panic_identity)))
            }
            _ => Err(RunError::Refused(Incomplete::Allowance)),
        }
    }
}

struct CleanupSlots<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
    witness: &'a Cell<Option<FinishWitness<'run, 'db>>>,
}

impl Drop for CleanupSlots<'_, '_, '_> {
    fn drop(&mut self) {
        drop(self.pending.borrow_mut().take());
        drop(self.endpoint.borrow_mut().take());
        self.witness.set(None);
    }
}

struct ObservedFinish<'call, 'run, 'db: 'run> {
    inner: MaterializationMapping<'call, 'run, 'db>,
    cleanup: CleanupContext<'run, 'db>,
    db: &'db dyn Db,
}

impl<'db> SharedMappingStartEffects<'db> for ObservedFinish<'_, '_, 'db> {
    type Failure = RunError;

    async fn checkpoint(&self, work: MappingWork) -> RunResult<()> {
        self.inner.checkpoint(work).await
    }
    async fn admit_mode(&self, mapping: &TypeMapping<'_, 'db>) -> RunResult<()> {
        self.inner.admit_mode(mapping).await
    }
    async fn expand_paramspecs(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.inner
            .expand_paramspecs(db, ty, mapping, tcx, visitor)
            .await
    }
    async fn nominal_known_class(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.inner.nominal_known_class(db, instance).await
    }
    async fn start_typevar(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>> {
        self.inner
            .start_typevar(db, variable, mapping, visitor)
            .await
    }
    async fn begin_transformation<'v>(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<TypeTransformerVisit<'db, MappingTransformationScope<'v, 'db>>> {
        self.inner
            .begin_transformation(db, ty, mapping, visitor)
            .await
    }
    async fn legacy_leaf(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        leaf: NativeMappingLeaf<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.inner
            .legacy_leaf(db, ty, leaf, mapping, tcx, visitor)
            .await
    }
}

impl<'db> SharedMappingEffects<'db> for ObservedFinish<'_, '_, 'db> {
    async fn map_union(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.inner.map_union(db, union, mapping, tcx, visitor).await
    }
    async fn map_intersection(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.inner
            .map_intersection(db, intersection, mapping, tcx, visitor)
            .await
    }
    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.inner.map_tuple(db, tuple, mapping, tcx, visitor).await
    }
    async fn typeform_argument(
        &self,
        db: &'db dyn Db,
        form: TypeFormType<'db>,
    ) -> RunResult<Type<'db>> {
        self.inner.typeform_argument(db, form).await
    }
    async fn intern_typeform(&self, db: &'db dyn Db, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.inner.intern_typeform(db, argument).await
    }
    async fn finish_transformation(
        &self,
        scope: MappingTransformationScope<'_, 'db>,
        result: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let prefetched_children = if self.cleanup.journal.boundary
            == CleanupBoundary::CacheAcceptance
        {
            let (Type::TypeForm(input), Type::TypeForm(mapped)) = (self.cleanup.input.0, result)
            else {
                return Err(RunError::Contract(
                    "finish observation requires TypeForm roots",
                ));
            };
            let child = *input
                .read_fields(salsa::FieldReads::new(self.db))
                .type_argument();
            let mapped_child = *mapped
                .read_fields(salsa::FieldReads::new(self.db))
                .type_argument();
            assert!(
                matches!(child, Type::TypeForm(_)) && matches!(mapped_child, Type::TypeForm(_))
            );
            assert_ne!(child, mapped_child);
            Some((child, mapped_child))
        } else {
            None
        };
        assert!(
            self.cleanup
                .witness
                .replace(Some(FinishWitness {
                    db: self.db,
                    visitor: self.inner.visitor,
                    input: self.cleanup.input.0,
                    result,
                    kind: self.cleanup.input.2,
                    prefetched_children,
                }))
                .is_none()
        );
        let _owner = self.cleanup.owner(self.inner.endpoint, CleanupHeld::Finish);
        // The production future owns the actual scope throughout callback rejection and cleanup.
        let result = self.inner.finish_transformation(scope, result).await;
        if self.cleanup.journal.boundary == CleanupBoundary::CacheAcceptance {
            self.cleanup.journal.after_finish.set(true);
        }
        result
    }
    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.inner.map_type(db, ty, mapping, tcx, visitor).await
    }
    async fn resume_legacy(
        &self,
        db: &'db dyn Db,
        continuation: LegacyTypeMappingContinuation<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.inner
            .resume_legacy(db, continuation, mapping, tcx, visitor)
            .await
    }
}

fn run_cleanup<'db>(
    db: &'db TestDb,
    input: Type<'db>,
    kind: MaterializationKind,
    observations: &MaterializationObservations,
    journal: &CleanupJournal,
) -> AttemptOutcome<RunResult<Type<'db>>> {
    let program = db.program_environment().program(db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let forms = OnceCell::new();
    let keys = OnceCell::new();
    let initial = Cell::new(None);
    let computed = Cell::new(None);
    let recovered = Cell::new(false);
    let endpoint = RefCell::new(ManuallyDrop::new(None));
    let pending = RefCell::new(None);
    let witness = Cell::new(None);
    let cleanup = CleanupContext {
        journal,
        input: (input, program, kind),
        endpoint: &endpoint,
        pending: &pending,
        witness: &witness,
    };
    let control = CleanupAdmission {
        observations,
        context: cleanup,
    };
    let _slots = CleanupSlots {
        endpoint: &endpoint,
        pending: &pending,
        witness: &witness,
    };
    let outcome = try_with_attempt(db, 100_000, || {
        let mut registry = RegistryBuilder::new(db, &control)?;
        let route =
            registry.reserve_callable(db as &dyn Db, cached_materialization_ingredient(db))?;
        let registered_keys =
            registry.callable_query_keys::<_, MaterializationKeyProfile>(&route)?;
        let keys = keys.get_or_init(|| registered_keys);
        let registered_forms =
            registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
        let forms = forms.get_or_init(|| registered_forms);
        registry.bind_callable(
            &route,
            RecoveryProvider {
                inner: MaterializationProvider {
                    environments: &environments,
                    visitors: &visitors,
                    forms,
                    observations,
                },
                route: route.clone(),
                keys,
                initial: &initial,
                computed: &computed,
                recovered: &recovered,
                cleanup: Some(cleanup),
            },
        )?;
        let queries = MaterializationQueries { route, keys };
        let result = registry.seal()?.run(|endpoint| async move {
            queries
                .materialization(&endpoint, db, input, program, kind)
                .await
        });
        journal.execution_error.set(result.as_ref().err().copied());
        result
    })
    .unwrap();
    if journal.boundary == CleanupBoundary::Initial {
        assert_eq!(initial.get(), None);
        assert_eq!(computed.get(), None);
        assert!(!recovered.get());
        assert_eq!(observations.roots.get(), 0);
    } else if journal.boundary == CleanupBoundary::Recovery {
        let id = journal.key.get().unwrap();
        assert_eq!(
            initial.get(),
            Some(Type::Divergent(DivergentType::new(id).materialized(kind)))
        );
        let value = computed.get().unwrap();
        assert_ne!(Some(value), initial.get());
        assert_eq!(
            value,
            wrapped(
                db,
                match kind {
                    MaterializationKind::Top => Type::object(),
                    MaterializationKind::Bottom => Type::Never,
                },
                4
            )
        );
        assert!(recovered.get());
        assert_eq!(observations.roots.get(), 1);
    } else {
        assert_eq!(initial.get(), None);
        assert_eq!(computed.get(), None);
        assert!(!recovered.get());
    }
    outcome
}

fn assert_cleanup_refusal(boundary: CleanupBoundary) {
    assert_cleanup_refusal_at_depth(boundary, 4);
}

fn assert_cleanup_refusal_at_depth(boundary: CleanupBoundary, depth: usize) {
    let db = setup_db();
    let input = wrapped(&db, Type::any(), depth);
    let kind = MaterializationKind::Bottom;
    let program = db.program_environment().program(&db);
    let stamp = prepared_source_probe::Stamp::current(&db);
    let observations = MaterializationObservations::default();
    let journal = CleanupJournal::new(boundary);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(&db, || {
        run_cleanup(&db, input, kind, &observations, &journal)
    })
    .unwrap();
    let reason = if boundary == CleanupBoundary::Recovery {
        Incomplete::Interrupted
    } else {
        Incomplete::Allowance
    };
    assert!(
        matches!(captured.value, AttemptOutcome::Incomplete(actual) if actual == reason),
        "{boundary:?}: {:?}",
        captured.value
    );
    assert_eq!(
        journal.execution_error.get(),
        Some(RunError::Refused(reason))
    );
    journal.assert_finished();
    if matches!(
        boundary,
        CleanupBoundary::Growth | CleanupBoundary::RehashKey | CleanupBoundary::Resource
    ) {
        assert_eq!(journal.cache_len.get(), Some(depth - 1));
    }
    if boundary != CleanupBoundary::Initial {
        assert_cleanup_mapping_finished(&observations, depth);
    }
    assert_materialization_idle(&db);
    assert_eq!(prepared_source_probe::Stamp::current(&db), stamp);
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, program, kind)).unwrap();
    assert_eq!(journal.key.get(), Some(id));
    let key = ingredient.database_key_index(id);
    assert!(reader.take_salsa_events().iter().any(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)));
    assert!(
        !captured
            .reads
            .iter()
            .any(|read| read.key == key && read.parent.is_none())
    );
    let expected_status = if boundary == CleanupBoundary::Recovery {
        FinalSourceError::ProvisionalMemo
    } else {
        FinalSourceError::MissingMemo
    };
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(expected_status)
    );
    if boundary == CleanupBoundary::Recovery {
        assert_eq!(
            observations.unavailable.get(),
            Some(UnavailableMaterialization::Recovery)
        );
    } else {
        assert_eq!(observations.unavailable.get(), None);
    }
    let retry = MaterializationObservations::default();
    reader.clear_salsa_events();
    let actual = complete(run(
        &db,
        input,
        kind,
        100_000,
        &admission(&retry, Refusal::None),
    ));
    assert_eq!(actual, wrapped(&db, Type::Never, depth));
    assert_eq!(retry.roots.get(), 1);
    assert_eq!(retry.children.get(), depth);
    assert_eq!(prepared_source_probe::Stamp::current(&db), stamp);
    assert!(reader.take_salsa_events().iter().any(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)));
    assert_materialization_warm(&db, input, program, kind, id, actual);
}

#[test]
fn registered_materialization_initial_drains_child_before_its_owner() {
    assert_cleanup_refusal(CleanupBoundary::Initial);
}

#[test]
fn registered_materialization_recovery_drains_child_before_its_borrowed_owner() {
    assert_cleanup_refusal(CleanupBoundary::Recovery);
}

#[test]
fn materialization_finish_refusal_drains_child_before_the_actual_scope() {
    assert_cleanup_refusal(CleanupBoundary::Finish);
}

#[test]
fn materialization_cache_refusal_drains_child_before_the_actual_scope() {
    assert_cleanup_refusal(CleanupBoundary::Cache);
}

#[test]
fn materialization_growth_key_and_resource_refusals_retain_the_actual_scope() {
    // Finishing the third wrapper causes the inline spill; the eighth causes the next full-table growth.
    for depth in [3, 8] {
        for boundary in [
            CleanupBoundary::Growth,
            CleanupBoundary::RehashKey,
            CleanupBoundary::Resource,
        ] {
            assert_cleanup_refusal_at_depth(boundary, depth);
        }
    }
}

#[derive(Default)]
struct MappingCacheAdmission {
    events: RefCell<Vec<ExecutionWork>>,
}

impl ExecutionAdmission for MappingCacheAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.events.borrow_mut().push(work);
        Ok(())
    }
}

fn run_direct_materializations<'db>(
    db: &'db TestDb,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    inputs: &[Type<'db>],
    allowance: usize,
    observations: &MaterializationObservations,
) -> (AttemptOutcome<RunResult<()>>, usize, Vec<ExecutionWork>) {
    let control = MappingCacheAdmission::default();
    let forms = OnceCell::new();
    let completed = Cell::new(0);
    let outcome = try_with_attempt(db, allowance, || {
        let mut registry = RegistryBuilder::new(db, &control)?;
        let registered =
            registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
        let forms = forms.get_or_init(|| registered);
        let completed = &completed;
        registry.seal()?.run(move |endpoint| async move {
            let effects = MaterializationMapping {
                endpoint: &endpoint,
                visitor,
                forms,
                observations,
            };
            for &input in inputs {
                let actual = input
                    .apply_type_mapping_with(
                        db,
                        &TypeMapping::Materialize(MaterializationKind::Top),
                        TypeContext::default(),
                        visitor,
                        &effects,
                    )
                    .await?;
                assert_eq!(actual, input);
                completed.set(completed.get() + 1);
            }
            Ok(())
        })
    })
    .unwrap();
    let events = control.events.borrow().clone();
    (outcome, completed.get(), events)
}

#[test]
fn endpoint_mapping_cache_hits_have_fixed_work_after_controlled_shallow_fills() {
    let db = setup_db();
    let env = db.program_environment();
    let mut fixed_hit = None;
    let units = |events: &[ExecutionWork]| {
        events
            .iter()
            .filter_map(|event| match event {
                ExecutionWork::Work { units } => Some(*units),
                _ => None,
            })
            .sum::<usize>()
    };
    for width in [2usize, 32, 128] {
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let inputs: Vec<_> = (0..width)
            .map(|index| wrapped(&db, Type::int_literal(index as i64), 1))
            .collect();
        let cold = MaterializationObservations::default();
        let (outcome, completed, events) =
            run_direct_materializations(&db, &visitor, &inputs, usize::MAX, &cold);
        assert!(matches!(outcome, AttemptOutcome::Complete(Ok(()))));
        assert_eq!(completed, width);
        assert_eq!(cold.children.get(), width);
        assert_eq!(cold.completed_children.get(), width);
        assert_eq!(cold.deepest_active.get(), 1);
        assert_eq!(
            cold.last_child_visitor.get(),
            Some(std::ptr::from_ref(&visitor).cast())
        );
        assert!(units(&events) <= 8192 * width);
        let idle = MaterializationObservations::default();
        let (outcome, completed, setup) =
            run_direct_materializations(&db, &visitor, &[], usize::MAX, &idle);
        assert!(matches!(outcome, AttemptOutcome::Complete(Ok(()))));
        assert_eq!(completed, 0);
        let hit = MaterializationObservations::default();
        let (outcome, completed, events) =
            run_direct_materializations(&db, &visitor, &inputs[..1], usize::MAX, &hit);
        assert!(matches!(outcome, AttemptOutcome::Complete(Ok(()))));
        assert_eq!(completed, 1);
        assert_eq!(hit.children.get(), 0);
        assert_eq!(hit.deepest_active.get(), 0);
        assert_eq!(hit.wrapper_interns.get(), 0);
        let root_poll = events
            .iter()
            .position(|event| *event == ExecutionWork::Poll)
            .unwrap();
        assert!(
            !events[root_poll + 1..]
                .iter()
                .any(|event| matches!(event, ExecutionWork::Resource { .. }))
        );
        let hit_units = units(&events) - units(&setup);
        assert!(hit_units > 0);
        if let Some(previous) = fixed_hit {
            assert_eq!(hit_units, previous);
        } else {
            fixed_hit = Some(hit_units);
        }
        let repeated = [inputs[0]; 8];
        let hit = MaterializationObservations::default();
        let (outcome, completed, _) = run_direct_materializations(
            &db,
            &visitor,
            &repeated,
            units(&setup) + 3 * hit_units,
            &hit,
        );
        assert!(matches!(
            outcome,
            AttemptOutcome::Incomplete(Incomplete::Allowance)
        ));
        assert_eq!(completed, 3);
        assert_eq!(hit.children.get(), 0);
        let (retry, completed, _) = run_direct_materializations(
            &db,
            &visitor,
            &inputs[..1],
            usize::MAX,
            &MaterializationObservations::default(),
        );
        assert!(matches!(retry, AttemptOutcome::Complete(Ok(()))));
        assert_eq!(completed, 1);
        assert_materialization_idle(&db);
    }
}

#[test]
fn materialization_cache_acceptance_keeps_the_root_active_until_local_completion() {
    let db = setup_db();
    let input = wrapped(&db, Type::any(), 2);
    let kind = MaterializationKind::Bottom;
    let program = db.program_environment().program(&db);
    let observations = MaterializationObservations::default();
    let journal = CleanupJournal::new(CleanupBoundary::CacheAcceptance);
    let outcome = run_cleanup(&db, input, kind, &observations, &journal);
    assert!(
        matches!(outcome, AttemptOutcome::Incomplete(Incomplete::Interrupted)),
        "{outcome:?}"
    );
    assert_eq!(
        journal.execution_error.get(),
        Some(RunError::Contract("completed task retained a child"))
    );
    assert_eq!(journal.queued.get(), 1);
    assert_eq!(journal.started.get(), 0);
    assert_eq!(journal.admitted.get(), 1);
    assert_eq!(
        journal.acceptance_work.get(),
        Some(TypeTransformationWork::CacheStorage {
            len: 1,
            capacity: 2
        })
    );
    assert_eq!(journal.body.get(), 1);
    assert_eq!(journal.initial.get(), 0);
    assert_eq!(journal.recovery.get(), 0);
    assert!(!journal.after_finish.get());
    assert!(!journal.armed.get());
    assert!(!journal.owner_live.get());
    assert_eq!(&*journal.drops.borrow(), &["child", "owner"]);
    assert_cleanup_mapping_finished(&observations, 2);
    assert_materialization_idle(&db);
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, program, kind)).unwrap();
    assert_eq!(journal.key.get(), Some(id));
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let child = journal
        .accepted_child
        .get()
        .expect("queued child recorded its cleanup");
    let owner = journal
        .accepted_owner
        .get()
        .expect("root owner recorded its cleanup");
    eprintln!(
        "FINISH_ACCEPTANCE child={child:?} owner={owner:?} error={:?}",
        journal.execution_error.get()
    );
    assert!(child.owner_live && owner.owner_live);
    assert_eq!(child.child, FinishLookupObservation::Mapped);
    assert_eq!(owner.child, FinishLookupObservation::Mapped);
    // A rejected local completion keeps the active fallback until descendants have drained.
    assert_eq!(
        child.root,
        FinishLookupObservation::Original,
        "root was published before local completion accepted its finish"
    );
    assert_eq!(owner.root, FinishLookupObservation::Absent { active: 0 });
    assert_eq!((child.cache_len, owner.cache_len), (1, 1));

    let retry = MaterializationObservations::default();
    let actual = complete(run(
        &db,
        input,
        kind,
        100_000,
        &admission(&retry, Refusal::None),
    ));
    assert_eq!(actual, wrapped(&db, Type::Never, 2));
    assert_eq!(retry.roots.get(), 1);
    assert_eq!(retry.children.get(), 2);
    assert_materialization_warm(&db, input, program, kind, id, actual);
}

fn run_direct_finish_cleanup<'db>(
    db: &'db TestDb,
    input: Type<'db>,
    kind: MaterializationKind,
    observations: &MaterializationObservations,
    journal: &CleanupJournal,
) -> AttemptOutcome<RunResult<Type<'db>>> {
    let program = db.program_environment().program(db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let visitors = CallMappingVisitors::with_capacity(capacity);
    let forms = OnceCell::new();
    let endpoint = RefCell::new(ManuallyDrop::new(None));
    let pending = RefCell::new(None);
    let witness = Cell::new(None);
    let cleanup = CleanupContext {
        journal,
        input: (input, program, kind),
        endpoint: &endpoint,
        pending: &pending,
        witness: &witness,
    };
    let control = CleanupAdmission {
        observations,
        context: cleanup,
    };
    let _slots = CleanupSlots {
        endpoint: &endpoint,
        pending: &pending,
        witness: &witness,
    };
    try_with_attempt(db, 100_000, || {
        let mut registry = RegistryBuilder::new(db, &control)?;
        let registered_forms =
            registry.finite_interned_values_with_memos(TypeFormType::ingredient(db.zalsa()), ())?;
        let forms = forms.get_or_init(|| registered_forms);
        let environments = &environments;
        let visitors = &visitors;
        // The direct driver reaches the actual mapping finish without the cached query's
        // fixpoint guard, which masks Local cancellation throughout its provider body.
        let result = registry.seal()?.run(move |endpoint| async move {
            let env = environments.allocate(&endpoint, program).await;
            let visitor = visitors.allocate(&endpoint, env).await;
            endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    observations.roots.set(observations.roots.get() + 1);
                    observations
                        .root_visitor
                        .set(Some(std::ptr::from_ref(visitor).cast()));
                    Ok(())
                })
                .await;
            let effects = ObservedFinish {
                inner: MaterializationMapping {
                    endpoint: &endpoint,
                    visitor,
                    forms,
                    observations,
                },
                cleanup,
                db,
            };
            input
                .apply_type_mapping_with(
                    db,
                    &TypeMapping::Materialize(kind),
                    TypeContext::default(),
                    visitor,
                    &effects,
                )
                .await
        });
        journal.execution_error.set(result.as_ref().err().copied());
        result
    })
    .unwrap()
}

#[derive(Clone, Copy, Debug)]
enum FinishCancellationRoute {
    Direct,
    CachedQuery,
}

fn materialization_cache_cancellation_retains_scope(
    cancel_at: FinishCancellationPhase,
    route: FinishCancellationRoute,
) -> anyhow::Result<()> {
    let cancellation = Arc::new(FinishCancellation::new(cancel_at));
    let callback = Arc::clone(&cancellation);
    let db = TestDbBuilder::new()
        .with_salsa_event_callback(move |event| callback.observe(event))
        .build()?;
    let input = wrapped(&db, Type::any(), 2);
    let kind = MaterializationKind::Bottom;
    assert!(cancellation.token.set(db.cancellation_token()).is_ok());
    let observations = MaterializationObservations::default();
    let mut journal = CleanupJournal::new(CleanupBoundary::CacheAcceptance);
    journal.finish_cancellation = Some(Arc::clone(&cancellation));
    let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| match route {
        FinishCancellationRoute::Direct => {
            run_direct_finish_cleanup(&db, input, kind, &observations, &journal)
        }
        FinishCancellationRoute::CachedQuery => {
            run_cleanup(&db, input, kind, &observations, &journal)
        }
    }));
    eprintln!(
        "FINISH_CANCELLATION route={route:?} cancel_at={cancel_at:?} outcome={outcome:?} phase={} admission_resume={} local_final_resume={} requested={} error={:?} queued={} started={} admitted={} after_finish={} armed={} owner_live={} drops={:?} child={:?} owner={:?} cache_work={:?} body={} initial={} recovery={} key={:?}",
        cancellation.phase.load(Ordering::SeqCst),
        cancellation.admission_resume.load(Ordering::SeqCst),
        cancellation.local_final_resume.load(Ordering::SeqCst),
        cancellation.requested.load(Ordering::SeqCst),
        journal.execution_error.get(),
        journal.queued.get(),
        journal.started.get(),
        journal.admitted.get(),
        journal.after_finish.get(),
        journal.armed.get(),
        journal.owner_live.get(),
        journal.drops.borrow(),
        journal.accepted_child.get(),
        journal.accepted_owner.get(),
        journal.acceptance_work.get(),
        journal.body.get(),
        journal.initial.get(),
        journal.recovery.get(),
        journal.key.get(),
    );
    match route {
        FinishCancellationRoute::Direct => {
            assert!(
                matches!(outcome, Err(salsa::Cancelled::Local)),
                "{outcome:?}"
            );
            assert_eq!(journal.execution_error.get(), None);
            assert_eq!(journal.body.get(), 0);
            assert_eq!(journal.key.get(), None);
        }
        FinishCancellationRoute::CachedQuery => {
            assert!(
                matches!(
                    outcome,
                    Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
                ),
                "{outcome:?}"
            );
            assert_eq!(
                journal.execution_error.get(),
                Some(RunError::Contract("completed task retained a child"))
            );
            assert_eq!(journal.body.get(), 1);
            assert!(journal.key.get().is_some());
        }
    }
    assert_eq!(
        cancellation.phase.load(Ordering::SeqCst),
        FinishCancellationPhase::Fired as u8
    );
    assert!(cancellation.admission_resume.load(Ordering::SeqCst));
    assert!(cancellation.requested.load(Ordering::SeqCst));
    assert_eq!(
        cancellation.local_final_resume.load(Ordering::SeqCst),
        cancel_at == FinishCancellationPhase::LocalFinalResume
    );
    assert_eq!(journal.initial.get(), 0);
    assert_eq!(journal.recovery.get(), 0);
    assert_eq!(journal.queued.get(), 1);
    assert_eq!(journal.started.get(), 0);
    assert_eq!(journal.admitted.get(), 1);
    // One cached TypeForm child leaves inline room, and the root has no inline payload. Thus
    // preparation has no admission between CacheStorage's resume check and callback completion.
    assert_eq!(
        journal.acceptance_work.get(),
        Some(TypeTransformationWork::CacheStorage {
            len: 1,
            capacity: 2
        })
    );
    assert!(!journal.after_finish.get());
    assert!(!journal.armed.get());
    assert!(!journal.owner_live.get());
    assert_eq!(&*journal.drops.borrow(), &["child", "owner"]);
    assert_cleanup_mapping_finished(&observations, 2);
    assert_eq!(
        salsa::attempt_probe::remaining_allowance_for_diagnostics(&db),
        None
    );
    let Some(child) = journal.accepted_child.get() else {
        anyhow::bail!("queued child did not record its cleanup");
    };
    let Some(owner) = journal.accepted_owner.get() else {
        anyhow::bail!("root owner did not record its cleanup");
    };
    assert!(child.owner_live && owner.owner_live);
    assert_eq!(child.root, FinishLookupObservation::Original);
    assert_eq!(owner.root, FinishLookupObservation::Absent { active: 0 });
    assert_eq!(child.child, FinishLookupObservation::Mapped);
    assert_eq!(owner.child, FinishLookupObservation::Mapped);
    assert_eq!((child.cache_len, owner.cache_len), (1, 1));
    Ok(())
}

#[test]
fn materialization_cache_final_resume_cancellation_retains_the_actual_scope() -> anyhow::Result<()>
{
    materialization_cache_cancellation_retains_scope(
        FinishCancellationPhase::LocalFinalResume,
        FinishCancellationRoute::Direct,
    )
}

#[test]
fn materialization_cache_admission_resume_cancellation_retains_the_actual_scope()
-> anyhow::Result<()> {
    materialization_cache_cancellation_retains_scope(
        FinishCancellationPhase::AdmissionResume,
        FinishCancellationRoute::Direct,
    )
}

#[test]
fn materialization_cached_query_masks_local_cancellation_until_queued_child_rejection()
-> anyhow::Result<()> {
    for phase in [
        FinishCancellationPhase::AdmissionResume,
        FinishCancellationPhase::LocalFinalResume,
    ] {
        materialization_cache_cancellation_retains_scope(
            phase,
            FinishCancellationRoute::CachedQuery,
        )?;
    }
    Ok(())
}
