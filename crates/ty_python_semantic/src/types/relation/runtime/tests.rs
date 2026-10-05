use std::cell::{Cell, OnceCell, RefCell};
use std::fmt::Debug;
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::sync::Arc;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, Demand, ExecutionAdmission, FinalSourceMemo,
    NativeValueOperation, NativeValueQuote, RegistryBuilder,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::Configuration;
use salsa::prepared_source_probe::{self, Stamp};
use ty_python_core::ProgramFile;

use super::constraint_set::{
    ConstraintSetObservation, OwnedRelationKind, OwnedRelationObservation, OwnedRelationProvider,
    OwnedRelationQueries,
};
use super::*;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::constraints::control::{hash_slots, map_growth};
use crate::types::constraints::{OwnedConstraintSet, possible_assignability_ingredient};
use crate::types::cyclic::CycleDetectorStorageProbe;
use crate::types::dedicated::pydantic::ConfigBoolean;
use crate::types::relation::pair_effects::{AsyncConstraintSet, AsyncIteratorConstraints};
use crate::types::relation::resources::RelationOwners;
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallMappingVisitors, CallRelationOwners, CallResourceCapacity,
};
use crate::types::relation::{
    HasRelationToVisitor, RelationObservationSite, RelationObservations,
    owned_assignability_ingredient, owned_equivalence_ingredient, redundancy_ingredient,
};
use crate::types::set_theoretic::{
    intersection_from_two_elements_ingredient, union_from_two_elements_ingredient,
};
use crate::types::typevar::TypeVarSet;
use crate::types::{
    BoundTypeVarInstance, ErrorContextTree, KnownClass, LiteralValueType, SubclassOfInner,
    TypeFormType, TypePair, TypeVarVariance, register_type_pair_values, todo_type,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CheckerSnapshot {
    resources: [*const (); 6],
    relation: TypeRelation,
    typevars: TypeVarEvaluation,
    inferable_none: bool,
    given_never: bool,
    given_always: bool,
    context_enabled: Option<bool>,
    observations: bool,
    expensive: bool,
}

impl CheckerSnapshot {
    fn capture(checker: &TypeRelationChecker<'_, '_, '_>) -> Self {
        Self {
            resources: [
                std::ptr::from_ref(checker.env).cast(),
                std::ptr::from_ref(checker.constraints).cast(),
                std::ptr::from_ref(checker.relation_visitor).cast(),
                std::ptr::from_ref(checker.disjointness_visitor).cast(),
                std::ptr::from_ref(checker.signature_relation_visitor).cast(),
                std::ptr::from_ref(checker.materialization_visitor).cast(),
            ],
            relation: checker.relation,
            typevars: checker.typevar_evaluation,
            inferable_none: checker.inferable == TypeVarSet::None,
            given_never: checker.given.is_trivially_never_satisfied(),
            given_always: checker.given.is_trivially_always_satisfied(),
            context_enabled: checker
                .context_tree
                .as_ref()
                .map(|context| context.is_enabled()),
            observations: checker.observations.is_some(),
            expensive: checker.perform_expensive_checks,
        }
    }
}

#[derive(Default)]
pub(super) struct Observations {
    entries: Cell<usize>,
    last: Cell<Option<CheckerSnapshot>>,
    operation: Cell<Option<UnsupportedPairOperation>>,
}

impl Observations {
    pub(super) fn record(&self, checker: &TypeRelationChecker<'_, '_, '_>) {
        self.entries.set(self.entries.get() + 1);
        self.last.set(Some(CheckerSnapshot::capture(checker)));
    }

    pub(super) fn mark_operation(&self, operation: UnsupportedPairOperation) -> impl Drop + '_ {
        OperationMarker {
            slot: &self.operation,
            previous: self.operation.replace(Some(operation)),
        }
    }
}

struct OperationMarker<'a> {
    slot: &'a Cell<Option<UnsupportedPairOperation>>,
    previous: Option<UnsupportedPairOperation>,
}

impl Drop for OperationMarker<'_> {
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}

#[derive(Default)]
struct Admission {
    events: RefCell<Vec<ExecutionWork>>,
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.events.borrow_mut().push(work);
        Ok(())
    }
}

async fn admitted_pair_field<T: Debug + PartialEq>(
    admission: &Admission,
    read: impl Future<Output = RunResult<T>>,
    expected: T,
) -> RunResult<()> {
    let start = admission.events.borrow().len();
    assert_eq!(read.await?, expected);
    let events = admission.events.borrow();
    let events = &events[start..];
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ExecutionWork::Work { units } if *units > 0))
    );
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, ExecutionWork::Task { .. }))
    );
    Ok(())
}

#[test]
fn pair_stored_fields_use_admitted_reads_and_preserve_copied_values() {
    let db = setup_db();
    let argument = Type::int_literal(37);
    let converted = Type::bool_literal(true);
    let type_form = TypeFormType::new(&db, argument);
    let type_is = TypeIsType::new(&db, argument, None);
    let type_guard = TypeGuardType::new(&db, converted, None);
    let field = FieldInstance::new(
        &db,
        Some(argument),
        true,
        None,
        None,
        Some((argument, converted)),
        ConfigBoolean::Unspecified,
    );
    let wrapper = MethodWrapper::new(&db, argument, MethodWrapperKind::Classmethod);
    let interned = InternedType::new(&db, argument);
    let callable = CallableType::bottom(&db);
    let partial = FunctoolsPartialInstance::new(&db, interned, callable);
    let builders = CallBuilders::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let admission = Admission::default();
    let mut reader = db.clone();
    reader.clear_salsa_events();

    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builders = &builders;
        let admission = &admission;
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                let builder = builders.allocate(&endpoint).await;
                let effects = RuntimePairs::new(db, endpoint, builder);
                admitted_pair_field(admission, effects.type_form_argument(type_form), argument)
                    .await?;
                admitted_pair_field(admission, effects.field_default(field), Some(argument))
                    .await?;
                admitted_pair_field(
                    admission,
                    effects.field_converter(field),
                    Some((argument, converted)),
                )
                .await?;
                admitted_pair_field(
                    admission,
                    effects.method_wrapper_kind(wrapper),
                    MethodWrapperKind::Classmethod,
                )
                .await?;
                admitted_pair_field(admission, effects.method_wrapper_type(wrapper), argument)
                    .await?;
                admitted_pair_field(admission, effects.partial_wrapped(partial), interned).await?;
                admitted_pair_field(admission, effects.partial_callable(partial), callable).await?;
                admitted_pair_field(admission, effects.interned_type(interned), argument).await?;
                admitted_pair_field(admission, effects.type_is_argument(type_is), argument).await?;
                admitted_pair_field(admission, effects.type_guard_return(type_guard), converted)
                    .await?;
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert!(!expansion_probe::active());
    assert!(
        reader
            .take_salsa_events()
            .iter()
            .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
}

#[test]
fn nominal_known_class_uses_admitted_stored_identity() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/nominal_identity.py",
            r#"from typing import Any

class Plain: ...
class Generic[T]: ...
class FromAny(Any): ...
class GenericFromAny[T](Any): ...
Dynamic = type("Dynamic", (), {})

plain: Plain
generic: Generic[int]
from_any: FromAny
generic_from_any: GenericFromAny[int]
known: int
known_generic: list[int]
dynamic: Dynamic
"#,
        )
        .build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/nominal_identity.py")?,
        env.program(&db),
    );
    let symbol = |name| global_symbol(&db, file, name).place.expect_type();
    assert!(matches!(
        symbol("Dynamic"),
        Type::ClassLiteral(ClassLiteral::Dynamic(_))
    ));
    let requested = [
        KnownClass::Tuple,
        KnownClass::Object,
        KnownClass::VersionInfo,
        KnownClass::Int,
        KnownClass::List,
        KnownClass::Str,
    ];
    let cases = [
        (
            "exact tuple",
            Type::empty_tuple(&db, &env),
            Some(KnownClass::Tuple),
            true,
            false,
        ),
        (
            "object",
            Type::object(),
            Some(KnownClass::Object),
            false,
            false,
        ),
        (
            "version info",
            Type::sys_version_info(),
            Some(KnownClass::VersionInfo),
            false,
            false,
        ),
        (
            "known",
            symbol("known"),
            Some(KnownClass::Int),
            false,
            false,
        ),
        ("plain", symbol("plain"), None, false, false),
        (
            "generic known origin",
            symbol("known_generic"),
            Some(KnownClass::List),
            true,
            false,
        ),
        (
            "generic unknown origin",
            symbol("generic"),
            None,
            true,
            false,
        ),
        ("dynamic", symbol("dynamic"), None, false, false),
        ("explicit Any", symbol("from_any"), None, false, true),
        (
            "generic explicit Any",
            symbol("generic_from_any"),
            None,
            true,
            true,
        ),
    ]
    .into_iter()
    .map(|(name, ty, expected, generic, explicit_any)| {
        let instance = ty
            .as_nominal_instance()
            .ok_or_else(|| anyhow::anyhow!("{name} must be a nominal instance"))?;
        assert_eq!(instance.known_class(&db), expected, "{name}");
        assert_eq!(instance.is_definition_generic(&db), generic, "{name}");
        assert_eq!(
            instance.inherits_from_explicit_any(),
            explicit_any,
            "{name}"
        );
        for class in requested {
            assert_eq!(
                instance.has_known_class(&db, class),
                expected == Some(class),
                "{name}"
            );
        }
        Ok((name, instance, expected))
    })
    .collect::<anyhow::Result<Vec<_>>>()?;
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let original = CheckerSnapshot::capture(&checker);
    let revision = salsa::plumbing::current_revision(&db);
    let admission = Admission::default();
    let mut reader = db.clone();
    reader.clear_salsa_events();

    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let checker = &checker;
        let cases = &cases;
        let admission = &admission;
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                for &(name, instance, expected) in cases {
                    for class in requested {
                        admitted_pair_field(
                            admission,
                            effects.nominal_has_known_class(checker, instance, class),
                            expected == Some(class),
                        )
                        .await?;
                        assert_eq!(CheckerSnapshot::capture(checker), original, "{name}");
                    }
                }
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert!(!expansion_probe::active());
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    report_accounting("nominal_known_class", &admission, &mut reader, 1);
    Ok(())
}

#[test]
fn stored_nominal_identity_keeps_standalone_semantic_refusals() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let original = CheckerSnapshot::capture(&checker);
    let revision = salsa::plumbing::current_revision(&db);
    for operation in [
        UnsupportedPairOperation::LiteralFallbackInstance,
        UnsupportedPairOperation::KnownClassInstance,
    ] {
        let admission = Admission::default();
        let mut reader = db.clone();
        reader.clear_salsa_events();
        let outcome = expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let checker = &checker;
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                    if operation == UnsupportedPairOperation::LiteralFallbackInstance {
                        effects
                            .literal_fallback_instance(checker, Type::bool_literal(true))
                            .await?;
                    } else {
                        effects
                            .known_class_instance(checker, KnownClass::Bool)
                            .await?;
                    }
                    Ok(())
                })
        })
        .0;
        assert!(
            matches!(outcome, Err(Incomplete::UnsupportedPairOperation(reason)) if reason == operation),
            "{outcome:?}",
        );
        assert!(!expansion_probe::active());
        assert_eq!(CheckerSnapshot::capture(&checker), original);
        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        report_accounting("standalone_semantic_refusal", &admission, &mut reader, 1);
    }
}

#[test]
fn real_terminal_pairs_preserve_the_original_checker_and_constraint_handles() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let observations = Observations::default();
    let admission = Admission::default();
    let mut reader = db.clone();
    reader.clear_salsa_events();

    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        let observations = &observations;
        let admission = &admission;
        let db = &db;
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                let env = environments.allocate(&endpoint, program).await;
                let builder = builders.allocate(&endpoint).await;
                let relation = owners.allocate(&endpoint, env, builder).await;
                let mut checker = relation.assignability(TypeVarSet::None);
                checker.given = ConstraintSet::from_bool(builder, true);
                checker.perform_expensive_checks = false;
                let original = CheckerSnapshot::capture(&checker);
                let mut effects = RuntimePairs::new(db, endpoint, builder);
                effects.observations = Some(observations);

                for (source, target, expected_bool) in [
                    (Type::Never, Type::int_literal(1), true),
                    (Type::int_literal(1), Type::int_literal(2), false),
                ] {
                    let expected = checker.check_type_pair(db, source, target);
                    let actual = effects.check_type_pair(&checker, source, target).await?;
                    assert!(actual.ownership_probe_same_set(expected));
                    assert_eq!(actual.is_trivially_always_satisfied(), expected_bool);
                    assert_eq!(actual.is_trivially_never_satisfied(), !expected_bool);
                    assert_eq!(observations.last.get(), Some(original));
                    assert_eq!(CheckerSnapshot::capture(&checker), original);
                }
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert_eq!(observations.entries.get(), 2);
    assert!(!expansion_probe::active());

    let events = admission.events.borrow();
    let task_bytes = events
        .iter()
        .filter_map(|event| match event {
            ExecutionWork::Task { requested_bytes } => Some(*requested_bytes),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(task_bytes.len(), 3);
    let work_units: usize = events
        .iter()
        .filter_map(|event| match event {
            ExecutionWork::Work { units } => Some(units),
            _ => None,
        })
        .sum();
    let resource_bytes = events
        .iter()
        .filter_map(|event| match event {
            ExecutionWork::Resource { requested_bytes } => Some(*requested_bytes),
            _ => None,
        })
        .collect::<Vec<_>>();
    let salsa_events = reader.take_salsa_events();
    assert!(
        salsa_events
            .iter()
            .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
    let cancellations = salsa_events
        .iter()
        .filter(|event| matches!(event.kind, salsa::EventKind::WillCheckCancellation))
        .count();
    assert!(cancellations > 0);
    eprintln!(
        "RUNTIME_PAIRS task_bytes={task_bytes:?} work_units={work_units} resource_bytes={resource_bytes:?} cancellation_checks={cancellations}"
    );
}

fn report_accounting(
    name: &str,
    admission: &Admission,
    reader: &mut TestDb,
    expected_tasks: usize,
) {
    let events = admission.events.borrow();
    let tasks = events
        .iter()
        .filter_map(|event| match event {
            ExecutionWork::Task { requested_bytes } => Some(*requested_bytes),
            _ => None,
        })
        .collect::<Vec<_>>();
    let resources = events
        .iter()
        .filter_map(|event| match event {
            ExecutionWork::Resource { requested_bytes } => Some(*requested_bytes),
            _ => None,
        })
        .collect::<Vec<_>>();
    let work: usize = events
        .iter()
        .filter_map(|event| match event {
            ExecutionWork::Work { units } => Some(units),
            _ => None,
        })
        .sum();
    let polls = events
        .iter()
        .filter(|event| matches!(event, ExecutionWork::Poll))
        .count();
    let salsa_events = reader.take_salsa_events();
    assert!(
        salsa_events
            .iter()
            .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
    let cancellations = salsa_events
        .iter()
        .filter(|event| matches!(event.kind, salsa::EventKind::WillCheckCancellation))
        .count();
    assert_eq!(tasks.len(), expected_tasks);
    eprintln!(
        "RUNTIME_PAIRS {name}: tasks={tasks:?} resources={resources:?} work={work} polls={polls} cancellations={cancellations}"
    );
}

fn fresh_lazy_helpers_succeed(db: &TestDb) {
    let program = db.program_environment().program(db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let observations = Observations::default();
    let admission = Admission::default();
    let requested = Cell::new(0);
    let skipped = Cell::new(0);
    let context = ErrorContextTree::new(TypeRelation::Assignability);
    context.set_enabled(false);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let outcome = expansion_probe::run(db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        let observations = &observations;
        let requested = &requested;
        let skipped = &skipped;
        let context = &context;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let env = environments.allocate(&endpoint, program).await;
                let builder = builders.allocate(&endpoint).await;
                let relation = owners.allocate(&endpoint, env, builder).await;
                let mut checker = relation.assignability(TypeVarSet::None);
                checker.context_tree = Some(context.clone());
                let original = CheckerSnapshot::capture(&checker);
                let mut effects = RuntimePairs::new(db, endpoint, builder);
                effects.observations = Some(observations);
                let always = ConstraintSet::from_bool(builder, true);
                let never = ConstraintSet::from_bool(builder, false);

                let result = [
                    (Type::Never, Type::int_literal(1)),
                    (Type::unknown(), Type::int_literal(2)),
                ]
                .into_iter()
                .when_all_with(
                    builder,
                    |(source, target)| {
                        requested.set(requested.get() + 1);
                        effects.check_type_pair(&checker, source, target)
                    },
                    &effects,
                )
                .await?;
                assert!(result.ownership_probe_same_set(always));
                assert_eq!(requested.get(), 2);
                let result = [
                    (Type::int_literal(1), Type::object()),
                    (Type::int_literal(1), Type::int_literal(2)),
                    (Type::Never, Type::object()),
                ]
                .into_iter()
                .when_all_with(
                    builder,
                    |(source, target)| {
                        requested.set(requested.get() + 1);
                        effects.check_type_pair(&checker, source, target)
                    },
                    &effects,
                )
                .await?;
                assert!(result.ownership_probe_same_set(never));
                assert_eq!(requested.get(), 4);

                let result = always
                    .and_with(
                        builder,
                        || {
                            requested.set(requested.get() + 1);
                            effects.check_type_pair(
                                &checker,
                                Type::int_literal(1),
                                Type::int_literal(2),
                            )
                        },
                        &effects,
                    )
                    .await?;
                assert!(result.ownership_probe_same_set(never));
                let result = never
                    .or_with(
                        builder,
                        || {
                            requested.set(requested.get() + 1);
                            effects.check_type_pair(&checker, Type::Never, Type::int_literal(2))
                        },
                        &effects,
                    )
                    .await?;
                assert!(result.ownership_probe_same_set(always));
                assert_eq!(requested.get(), 6);
                let result = never
                    .and_with(
                        builder,
                        || {
                            skipped.set(skipped.get() + 1);
                            effects.check_type_pair(&checker, Type::Never, Type::object())
                        },
                        &effects,
                    )
                    .await?;
                assert!(result.ownership_probe_same_set(never));
                let result = always
                    .or_with(
                        builder,
                        || {
                            skipped.set(skipped.get() + 1);
                            effects.check_type_pair(&checker, Type::Never, Type::object())
                        },
                        &effects,
                    )
                    .await?;
                assert!(result.ownership_probe_same_set(always));
                assert_eq!(observations.last.get(), Some(original));
                assert_eq!(CheckerSnapshot::capture(&checker), original);
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert_eq!(requested.get(), 6);
    assert_eq!(observations.entries.get(), 6);
    assert_eq!(skipped.get(), 0);
    assert!(!context.is_enabled());
    assert!(!expansion_probe::active());
    report_accounting("lazy_helpers", &admission, &mut reader, 7);
}

#[test]
fn structural_helpers_finish_and_stop_before_lazy_children() {
    fresh_lazy_helpers_succeed(&setup_db());
}

fn unsupported_operands<'db>(db: &'db TestDb) -> anyhow::Result<(Type<'db>, Type<'db>)> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/unsupported_pairs.py")?,
        env.program(db),
    );
    Ok((
        global_symbol(db, file, "c").place.expect_type(),
        global_symbol(db, file, "p").place.expect_type(),
    ))
}

fn unsupported_database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new().with_python_version(PythonVersion::PY313).with_file(
        "/src/unsupported_pairs.py",
        "from typing import Protocol\nclass C[T]:\n    value: T\nclass P[T](Protocol):\n    value: T\nc: C[int]\np: P[int]\n",
    ).build()
}

#[derive(Clone, Copy, Debug)]
enum Rejection {
    Operands,
    LazySubtyping,
    Observations,
    Context,
}

#[test]
fn unsupported_entries_return_an_operational_reason_without_a_pair_result() -> anyhow::Result<()> {
    for rejection in [
        Rejection::Operands,
        Rejection::LazySubtyping,
        Rejection::Observations,
        Rejection::Context,
    ] {
        let db = unsupported_database()?;
        let operands = match rejection {
            Rejection::Operands => unsupported_operands(&db)?,
            Rejection::LazySubtyping | Rejection::Observations | Rejection::Context => {
                (Type::Never, Type::int_literal(1))
            }
        };
        let mut reader = db.clone();
        let preparation = reader
            .take_salsa_events()
            .into_iter()
            .filter_map(|event| match event.kind {
                salsa::EventKind::WillExecute { database_key } => Some(database_key),
                _ => None,
            })
            .collect::<Vec<_>>();
        if matches!(rejection, Rejection::Operands) {
            assert!(matches!(operands.0, Type::NominalInstance(_)));
            assert!(matches!(operands.1, Type::ProtocolInstance(_)));
            assert!(!preparation.is_empty());
            eprintln!("RUNTIME_PAIRS unsupported_preparation={preparation:?}");
        }
        let stamp = Stamp::current(&db);
        for _ in 0..2 {
            refused_entry(&db, rejection, operands);
            assert!(stamp.belongs_to(&db));
        }
    }
    Ok(())
}

fn refused_entry<'db>(db: &'db TestDb, rejection: Rejection, operands: (Type<'db>, Type<'db>)) {
    let program = db.program_environment().program(db);
    let stamp = Stamp::current(db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let patterns = FxHashSet::from_iter([operands.0, operands.1]);
    let semantic_observations = RelationObservations {
        patterns: &patterns,
        results: RefCell::default(),
        site: Cell::new(RelationObservationSite::Argument(0)),
        inferred: RefCell::default(),
    };
    let observations = Observations::default();
    let admission = Admission::default();
    let returned = Cell::new(None);
    let after_pair = Cell::new(false);
    let original = Cell::new(None);
    let selected = RefCell::new(None::<TypeRelationChecker<'_, '_, '_>>);
    let context = ErrorContextTree::new(TypeRelation::Assignability);
    context.set_enabled(true);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let outcome = expansion_probe::run(db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        let observations = &observations;
        let semantic_observations = &semantic_observations;
        let after_pair = &after_pair;
        let original = &original;
        let selected = &selected;
        let context = &context;
        let result =
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    let env = environments.allocate(&endpoint, program).await;
                    let builder = builders.allocate(&endpoint).await;
                    let relation = owners.allocate(&endpoint, env, builder).await;
                    let mut checker = relation.assignability(TypeVarSet::None);
                    match rejection {
                        Rejection::Operands => {}
                        Rejection::LazySubtyping => {
                            checker.relation = TypeRelation::Subtyping;
                            checker.typevar_evaluation = TypeVarEvaluation::Lazy;
                        }
                        Rejection::Observations => {
                            checker.observations = Some(semantic_observations)
                        }
                        Rejection::Context => checker.context_tree = Some(context.clone()),
                    }
                    original.set(Some(CheckerSnapshot::capture(&checker)));
                    *selected.borrow_mut() = Some(checker.clone());
                    let mut effects = RuntimePairs::new(db, endpoint, builder);
                    effects.observations = Some(observations);
                    let _result = effects
                        .check_type_pair(&checker, operands.0, operands.1)
                        .await?;
                    after_pair.set(true);
                    Ok(())
                });
        returned.set(result.as_ref().err().copied());
        result
    })
    .0;
    let expected = match rejection {
        Rejection::Operands => UnsupportedPairOperation::ProtocolObjectEquivalence,
        Rejection::LazySubtyping | Rejection::Observations | Rejection::Context => {
            UnsupportedPairOperation::CheckerMode
        }
    };
    assert!(
        matches!(outcome, Err(Incomplete::UnsupportedPairOperation(actual)) if actual == expected),
        "{rejection:?}: {outcome:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(
            salsa::attempt_probe::Incomplete::Interrupted
        ))
    );
    assert!(!after_pair.get());
    let entered = usize::from(matches!(rejection, Rejection::Operands));
    assert_eq!(observations.entries.get(), entered);
    if entered != 0 {
        assert_eq!(observations.last.get(), original.get());
    }
    let selected = selected
        .borrow_mut()
        .take()
        .expect("root retained original checker resources");
    assert_eq!(Some(CheckerSnapshot::capture(&selected)), original.get());
    assert_eq!(selected.relation_visitor.ownership_probe_counts(), (0, 0));
    assert!(context.is_enabled());
    assert!(observations.operation.get().is_none());
    assert!(semantic_observations.results.borrow().is_empty());
    assert!(semantic_observations.inferred.borrow().is_empty());
    assert_eq!(
        semantic_observations.site.get(),
        RelationObservationSite::Argument(0)
    );
    assert!(stamp.belongs_to(db));
    assert!(!expansion_probe::active());
    report_accounting("unsupported", &admission, &mut reader, 2);
}

struct QueuedCleanup<'a>(&'a dyn Fn());

impl Drop for QueuedCleanup<'_> {
    fn drop(&mut self) {
        (self.0)();
    }
}

struct FaultAdmission<'run, 'db: 'run> {
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    cleanup: &'run dyn Fn(),
    child_factory_ran: &'run Cell<bool>,
    observations: &'run Observations,
    operation: Option<UnsupportedPairOperation>,
    armed: Cell<bool>,
    fired: Cell<bool>,
    audit: Admission,
}

impl ExecutionAdmission for FaultAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.audit.admit(work)?;
        let selected = match self.operation {
            None => matches!(work, ExecutionWork::Task { .. }),
            Some(operation) => {
                matches!(work, ExecutionWork::Work { .. })
                    && self.observations.operation.get() == Some(operation)
            }
        };
        if !self.armed.get() || self.fired.get() || !selected {
            return Ok(());
        }
        self.fired.set(true);
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract("pair fault has no endpoint"))?;
        let cleanup = QueuedCleanup(self.cleanup);
        let child_factory_ran = self.child_factory_ran;
        let pending = endpoint.demand(move || {
            child_factory_ran.set(true);
            async move {
                let _held = cleanup;
                Ok(())
            }
        })?;
        *self.pending.borrow_mut() = Some(pending);
        match self.operation {
            Some(_) => Ok(()),
            None => Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            )),
        }
    }
}

struct ResetSlots<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
}

impl Drop for ResetSlots<'_, '_, '_> {
    fn drop(&mut self) {
        let pending = self.pending.borrow_mut().take();
        let endpoint = self.endpoint.borrow_mut().take();
        drop(pending);
        drop(endpoint);
    }
}

struct ObservedPairs<'a, 'db> {
    pairs: std::array::IntoIter<(Type<'db>, Type<'db>), 3>,
    live: &'a Cell<bool>,
    yielded: &'a Cell<usize>,
    armed: &'a Cell<bool>,
    journal: &'a RefCell<Vec<&'static str>>,
}

impl<'db> Iterator for ObservedPairs<'_, 'db> {
    type Item = (Type<'db>, Type<'db>);

    fn next(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next()?;
        let count = self.yielded.get() + 1;
        self.yielded.set(count);
        if count == 2 {
            self.armed.set(true);
        }
        Some(pair)
    }
}

impl Drop for ObservedPairs<'_, '_> {
    fn drop(&mut self) {
        self.live.set(false);
        self.journal.borrow_mut().push("iterator");
    }
}

struct RootCleanup<'a>(&'a RefCell<Vec<&'static str>>);

impl Drop for RootCleanup<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().push("root");
    }
}

#[test]
fn refused_second_pair_retains_the_actual_iterator_and_original_resources() {
    refused_second_pair(&setup_db(), None);
}

#[test]
fn refused_protocol_preparation_retains_the_actual_iterator_and_original_resources()
-> anyhow::Result<()> {
    let db = unsupported_database()?;
    let operands = unsupported_operands(&db)?;
    assert!(matches!(operands.0, Type::NominalInstance(_)));
    assert!(matches!(operands.1, Type::ProtocolInstance(_)));
    let mut reader = db.clone();
    let preparation = reader
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| match event.kind {
            salsa::EventKind::WillExecute { database_key } => Some(database_key),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!preparation.is_empty());
    eprintln!("RUNTIME_PAIRS refusal_preparation={preparation:?}");
    let stamp = Stamp::current(&db);
    refused_second_pair(&db, Some(operands));
    for _ in 0..2 {
        refused_entry(&db, Rejection::Operands, operands);
        assert!(stamp.belongs_to(&db));
    }
    Ok(())
}

fn refused_second_pair<'db>(db: &'db TestDb, protocol_pair: Option<(Type<'db>, Type<'db>)>) {
    let program = db.program_environment().program(db);
    let stamp = Stamp::current(db);
    let operation = protocol_pair.map(|_| UnsupportedPairOperation::ProtocolObjectEquivalence);
    let expected_entries = if protocol_pair.is_some() { 2 } else { 1 };
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let observations = Observations::default();
    let original = Cell::new(None);
    let selected = RefCell::new(None::<TypeRelationChecker<'_, '_, '_>>);
    let live = Cell::new(false);
    let yielded = Cell::new(0);
    let cleanup_count = Cell::new(0);
    let journal = RefCell::new(Vec::new());
    let cleanup = || {
        assert!(live.get());
        assert_eq!(yielded.get(), 2);
        assert_eq!(observations.entries.get(), expected_entries);
        assert_eq!(observations.last.get(), original.get());
        let selected = selected.borrow();
        let checker = selected
            .as_ref()
            .expect("root published its original checker");
        assert_eq!(Some(CheckerSnapshot::capture(checker)), original.get());
        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
        assert_eq!(
            checker.disjointness_visitor.ownership_probe_counts(),
            (0, 0)
        );
        assert!(checker.signature_relation_visitor.is_empty());
        // The terminal path takes the real storage's mutable borrow and performs no solver work.
        assert!(
            !ConstraintSet::from_bool(checker.constraints, true)
                .is_never_satisfied(db, checker.env)
        );
        cleanup_count.set(cleanup_count.get() + 1);
        journal.borrow_mut().push("child");
    };
    let child_factory_ran = Cell::new(false);
    let after_fold = Cell::new(false);
    let returned = Cell::new(None);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let admission;
    let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
    let pending = RefCell::new(None);
    admission = FaultAdmission {
        endpoint: &endpoint_slot,
        pending: &pending,
        cleanup: &cleanup,
        child_factory_ran: &child_factory_ran,
        observations: &observations,
        operation,
        armed: Cell::new(false),
        fired: Cell::new(false),
        audit: Admission::default(),
    };
    // Install the reset guard before publishing any endpoint into the destructor-free slot.
    let reset = ResetSlots {
        endpoint: &endpoint_slot,
        pending: &pending,
    };
    let outcome = expansion_probe::run(db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        let observations = &observations;
        let selected = &selected;
        let original = &original;
        let live = &live;
        let yielded = &yielded;
        let journal = &journal;
        let after_fold = &after_fold;
        let admission = &admission;
        let result = RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| {
                **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                async move {
                    let _root = RootCleanup(journal);
                    let env = environments.allocate(&endpoint, program).await;
                    let builder = builders.allocate(&endpoint).await;
                    let relation = owners.allocate(&endpoint, env, builder).await;
                    let checker = relation.assignability(TypeVarSet::None);
                    original.set(Some(CheckerSnapshot::capture(&checker)));
                    *selected.borrow_mut() = Some(checker.clone());
                    let mut effects = RuntimePairs::new(db, endpoint, builder);
                    effects.observations = Some(observations);
                    live.set(true);
                    let pairs = ObservedPairs {
                        pairs: [
                            (Type::Never, Type::int_literal(1)),
                            protocol_pair.unwrap_or((Type::Never, Type::int_literal(2))),
                            (Type::Never, Type::object()),
                        ]
                        .into_iter(),
                        live,
                        yielded,
                        armed: &admission.armed,
                        journal,
                    };
                    let _result = pairs
                        .when_all_with(
                            builder,
                            |(source, target)| effects.check_type_pair(&checker, source, target),
                            &effects,
                        )
                        .await?;
                    after_fold.set(true);
                    Ok(())
                }
            });
        returned.set(result.as_ref().err().copied());
        result
    })
    .0;
    match operation {
        Some(expected) => assert!(
            matches!(outcome, Err(Incomplete::UnsupportedPairOperation(actual)) if actual == expected),
            "{outcome:?}"
        ),
        None => assert!(matches!(outcome, Err(Incomplete::Allowance)), "{outcome:?}"),
    }
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(if operation.is_some() {
            salsa::attempt_probe::Incomplete::Interrupted
        } else {
            salsa::attempt_probe::Incomplete::Allowance
        }))
    );
    assert!(admission.fired.get());
    assert!(!child_factory_ran.get());
    assert!(!after_fold.get());
    assert!(!live.get());
    assert_eq!(yielded.get(), 2);
    assert_eq!(observations.entries.get(), expected_entries);
    assert!(observations.operation.get().is_none());
    assert_eq!(cleanup_count.get(), 1);
    assert_eq!(&*journal.borrow(), &["child", "iterator", "root"]);
    assert!(stamp.belongs_to(db));
    assert!(!expansion_probe::active());
    drop(reset);
    assert!(endpoint_slot.borrow().is_none() && pending.borrow().is_none());
    selected.borrow_mut().take();
    report_accounting("second_pair_refusal", &admission.audit, &mut reader, 4);
    if operation.is_none() {
        for _ in 0..2 {
            fresh_lazy_helpers_succeed(db);
            assert!(stamp.belongs_to(db));
        }
    }
}

#[test]
fn real_guard_preserves_exact_cycle_fallback_and_completed_cache_reuse() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let observations = Observations::default();
    let admission = Admission::default();
    let bodies = Cell::new(0);
    let skipped = Cell::new(0);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let environments = &environments;
        let builders = &builders;
        let owners = &owners;
        let observations = &observations;
        let bodies = &bodies;
        let skipped = &skipped;
        let db = &db;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let env = environments.allocate(&endpoint, program).await;
                let builder = builders.allocate(&endpoint).await;
                let relation = owners.allocate(&endpoint, env, builder).await;
                let checker = relation.assignability(TypeVarSet::None);
                let mut effects = RuntimePairs::new(db, endpoint, builder);
                effects.observations = Some(observations);
                let source = Type::int_literal(1);
                let target = Type::int_literal(2);
                let always = ConstraintSet::from_bool(builder, true);
                let ordinary = HasRelationToVisitor::default(builder);
                let key = (source, target, checker.relation, checker.typevar_evaluation);
                let expected = ordinary.visit(db, key, || {
                    let exact = ordinary.visit(db, key, || {
                        skipped.set(skipped.get() + 1);
                        checker.check_type_pair(db, source, target)
                    });
                    assert!(exact.ownership_probe_same_set(always));
                    checker.check_type_pair(db, source, target)
                });
                assert!(expected.is_trivially_never_satisfied());
                assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
                let actual = effects
                    .guard(&checker, source, target, || async {
                        bodies.set(bodies.get() + 1);
                        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                        let exact = effects
                            .guard(&checker, source, target, || async {
                                skipped.set(skipped.get() + 1);
                                effects.check_type_pair(&checker, source, target).await
                            })
                            .await?;
                        assert!(exact.ownership_probe_same_set(always));
                        assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 0));
                        effects.check_type_pair(&checker, source, target).await
                    })
                    .await?;
                assert!(actual.ownership_probe_same_set(expected));
                assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
                let cached = effects
                    .guard(&checker, source, target, || async {
                        skipped.set(skipped.get() + 1);
                        effects.check_type_pair(&checker, source, target).await
                    })
                    .await?;
                assert!(cached.ownership_probe_same_set(expected));
                assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert_eq!(bodies.get(), 1);
    assert_eq!(skipped.get(), 0);
    assert_eq!(observations.entries.get(), 1);
    assert!(!expansion_probe::active());
    report_accounting("guard_exact_cache", &admission, &mut reader, 2);
}

fn finite_guard_pairs<'db>() -> [(Type<'db>, Type<'db>); 3] {
    [
        (Type::Never, Type::int_literal(11)),
        (Type::int_literal(12), Type::int_literal(13)),
        (Type::Never, Type::int_literal(14)),
    ]
}

fn guard_key<'db>(
    checker: &TypeRelationChecker<'_, '_, 'db>,
    pair: (Type<'db>, Type<'db>),
) -> RelationKey<'db> {
    (pair.0, pair.1, checker.relation, checker.typevar_evaluation)
}

async fn guarded_terminal<'run, 'a: 'run, 'db: 'run, 'c: 'run>(
    effects: &RuntimePairs<'run, 'db, 'c>,
    checker: &TypeRelationChecker<'a, 'c, 'db>,
    pair: (Type<'db>, Type<'db>),
) -> RunResult<ConstraintSet<'db, 'c>> {
    effects
        .guard(checker, pair.0, pair.1, || {
            effects.check_type_pair(checker, pair.0, pair.1)
        })
        .await
}

fn assert_cached<'db, 'c>(
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    pair: (Type<'db>, Type<'db>),
    expected: ConstraintSet<'db, 'c>,
) {
    assert!(
        checker
            .relation_visitor
            .ownership_probe_cached(guard_key(checker, pair))
            .is_some_and(|actual| actual.ownership_probe_same_set(expected))
    );
}

#[test]
fn cached_guard_hits_exhaust_shared_work_independently_of_retained_capacity() {
    let db = setup_db();
    let db = &db;
    let mut inline_hit_work = None;
    for cached_entries in [1, 32] {
        let env = db.program_environment();
        let builder = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &builder);
        let checker = owners.assignability(TypeVarSet::None);
        let checker = &checker;
        let pair = finite_guard_pairs()[1];
        let mut pairs = vec![pair];
        pairs.extend((1..cached_entries).map(|index| {
            (
                Type::int_literal(100 + index),
                Type::int_literal(200 + index),
            )
        }));
        let expected = controlled_guard_fill(db, checker, &pairs)[0];
        let original = CheckerSnapshot::capture(checker);
        let storage = checker.relation_visitor.ownership_probe_storage();
        let stamp = Stamp::current(db);
        assert_eq!(storage.cache_len, cached_entries as usize);
        assert_eq!(storage.cache_capacity.is_some(), cached_entries > 1);
        let bodies = Cell::new(0);
        let run_hits = |allowance, requested_hits| {
            let admission = Admission::default();
            let completed = Cell::new(0);
            let runtime_error = Cell::new(None);
            let outcome = expansion_probe::run(db, allowance, || {
                let result = RegistryBuilder::new(db, &admission)
                    .and_then(RegistryBuilder::seal)
                    .and_then(|registry| {
                        let completed = &completed;
                        let bodies = &bodies;
                        registry.run(move |endpoint| async move {
                            let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                            for _ in 0..requested_hits {
                                let actual = effects
                                    .guard(checker, pair.0, pair.1, || async {
                                        bodies.set(bodies.get() + 1);
                                        Err(RunError::Contract("cached guard invoked its body"))
                                    })
                                    .await?;
                                assert!(actual.ownership_probe_same_set(expected));
                                completed.set(completed.get() + 1);
                            }
                            Ok(())
                        })
                    });
                runtime_error.set(result.as_ref().err().copied());
                result
            })
            .0;
            let events = admission.events.into_inner();
            let root_poll = events
                .iter()
                .position(|work| *work == ExecutionWork::Poll)
                .expect("root poll");
            let events = &events[root_poll + 1..];
            assert!(
                events
                    .iter()
                    .all(|work| !matches!(work, ExecutionWork::Resource { .. }))
            );
            let work = events
                .iter()
                .copied()
                .filter_map(|event| match event {
                    ExecutionWork::Work { units } => Some(units),
                    _ => None,
                })
                .collect::<Vec<_>>();
            (outcome, runtime_error.get(), completed.get(), work)
        };

        let (outcome, error, completed, hit_work) = run_hits(usize::MAX, 1);
        assert_eq!(outcome, Ok(Ok(())));
        assert_eq!(error, None);
        assert_eq!(completed, 1);
        assert!(!hit_work.is_empty());
        assert!(hit_work.iter().all(|units| *units > 0));
        if let Some(inline) = &inline_hit_work {
            assert_eq!(
                &hit_work, inline,
                "retained hash capacity does not weight a hit"
            );
        } else {
            inline_hit_work = Some(hit_work.clone());
        }
        let allowance = 3 * hit_work.iter().sum::<usize>();
        let (outcome, error, completed, work) = run_hits(allowance, 8);
        assert_eq!(outcome, Err(expansion_probe::Incomplete::Allowance));
        assert_eq!(
            error,
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance
            ))
        );
        assert_eq!(completed, 3);
        assert_eq!(work, hit_work.repeat(3));
        assert_eq!(checker.relation_visitor.ownership_probe_storage(), storage);
        assert_eq!(CheckerSnapshot::capture(checker), original);
        assert_cached(checker, pair, expected);
        assert!(stamp.belongs_to(db));

        let (retry, error, completed, work) = run_hits(usize::MAX, 1);
        assert_eq!(retry, Ok(Ok(())));
        assert_eq!(error, None);
        assert_eq!(completed, 1);
        assert_eq!(work, hit_work);
        assert_eq!(bodies.get(), 0);
        assert_eq!(checker.relation_visitor.ownership_probe_storage(), storage);
        assert_eq!(CheckerSnapshot::capture(checker), original);
        assert_cached(checker, pair, expected);
        assert!(stamp.belongs_to(db));
    }
}

#[derive(Clone, Copy, Debug)]
struct GuardAllocation {
    before: CycleDetectorStorageProbe,
    previous_work: Option<usize>,
    requested_bytes: usize,
}

struct GrowthAdmission<'a, 'db, 'c> {
    visitor: &'a HasRelationToVisitor<'db, 'c>,
    previous_work: Cell<Option<usize>>,
    allocations: RefCell<Vec<GuardAllocation>>,
    audit: Admission,
}

impl ExecutionAdmission for GrowthAdmission<'_, '_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.audit.admit(work)?;
        if let ExecutionWork::Work { units } = work {
            self.previous_work.set(Some(units));
        } else if let ExecutionWork::Resource { requested_bytes } = work {
            let before = self.visitor.ownership_probe_storage();
            let active = before.active_len == 1
                && before.active_capacity == 1
                && requested_bytes == 4 * before.active_entry_bytes;
            let cache = (before.cache_len == 2 && before.cache_capacity.is_none()
                || before.cache_capacity == Some(before.cache_len))
                && requested_bytes
                    == map_growth::<RelationKey<'_>, ConstraintSet<'_, '_>, ()>(
                        before.cache_len,
                        before.cache_capacity.unwrap_or(0),
                        before.cache_len + 1,
                    )
                    .unwrap()
                    .requested_payload_bytes;
            if active || cache {
                self.allocations.borrow_mut().push(GuardAllocation {
                    before,
                    previous_work: self.previous_work.get(),
                    requested_bytes,
                });
            }
        }
        Ok(())
    }
}

fn fresh_guard_growth_succeeds(db: &TestDb) {
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let original = CheckerSnapshot::capture(&checker);
    let pairs = finite_guard_pairs();
    let expected = pairs.map(|pair| checker.check_type_pair(db, pair.0, pair.1));
    let admission = GrowthAdmission {
        visitor: checker.relation_visitor,
        previous_work: Cell::new(None),
        allocations: RefCell::new(Vec::new()),
        audit: Admission::default(),
    };
    let observations = Observations::default();
    let before = checker.relation_visitor.ownership_probe_storage();
    assert_eq!(
        (
            before.active_len,
            before.active_capacity,
            before.cache_len,
            before.cache_capacity
        ),
        (0, 1, 0, None)
    );
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let checker = &checker;
    let observations = &observations;
    let outcome = expansion_probe::run(db, usize::MAX, || {
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let mut effects = RuntimePairs::new(db, endpoint, checker.constraints);
                effects.observations = Some(observations);
                let outer = effects
                    .guard(checker, pairs[0].0, pairs[0].1, || async {
                        let middle = effects
                            .guard(checker, pairs[1].0, pairs[1].1, || async {
                                let leaf = effects
                                    .guard(checker, pairs[2].0, pairs[2].1, || async {
                                        assert_eq!(
                                            checker.relation_visitor.ownership_probe_counts(),
                                            (3, 0)
                                        );
                                        effects
                                            .check_type_pair(checker, pairs[2].0, pairs[2].1)
                                            .await
                                    })
                                    .await?;
                                assert!(leaf.ownership_probe_same_set(expected[2]));
                                effects
                                    .check_type_pair(checker, pairs[1].0, pairs[1].1)
                                    .await
                            })
                            .await?;
                        assert!(middle.ownership_probe_same_set(expected[1]));
                        effects
                            .check_type_pair(checker, pairs[0].0, pairs[0].1)
                            .await
                    })
                    .await?;
                assert!(outer.ownership_probe_same_set(expected[0]));
                for (pair, expected) in pairs.into_iter().zip(expected) {
                    assert_cached(checker, pair, expected);
                    let reused = effects
                        .guard(checker, pair.0, pair.1, || async {
                            Err(RunError::Contract("completed guard invoked its body"))
                        })
                        .await?;
                    assert!(reused.ownership_probe_same_set(expected));
                }
                assert_eq!(CheckerSnapshot::capture(checker), original);
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    let after = checker.relation_visitor.ownership_probe_storage();
    assert_eq!((after.active_len, after.cache_len), (0, 3));
    assert!(after.active_capacity >= 4);
    assert!(
        after
            .cache_capacity
            .is_some_and(|capacity| (4..=8).contains(&capacity))
    );
    let allocations = admission.allocations.borrow();
    assert_eq!(allocations.len(), 2, "{allocations:?}");
    assert_eq!(allocations[0].before.active_capacity, 1);
    assert_eq!(allocations[0].previous_work, Some(1));
    assert_eq!(
        allocations[0].requested_bytes,
        4 * before.active_entry_bytes
    );
    assert_eq!(allocations[1].before.cache_capacity, None);
    assert_eq!(allocations[1].before.cache_len, 2);
    // No old table, two retained entries, and the replacement-table extent.
    assert_eq!(allocations[1].previous_work, Some(2 + 68));
    assert_eq!(
        allocations[1].requested_bytes,
        4 * size_of::<(RelationKey<'_>, ConstraintSet<'_, '_>)>()
    );
    assert_eq!(observations.entries.get(), 3);
    assert_eq!(observations.last.get(), Some(original));
    assert!(!expansion_probe::active());
    eprintln!("GUARD_STORAGE before={before:?} after={after:?} allocations={allocations:?}");
    report_accounting("guard_growth", &admission.audit, &mut reader, 4);
}

#[test]
fn guard_growth_admits_capacity_before_mutation_and_reuses_exact_results() {
    fresh_guard_growth_succeeds(&setup_db());
}

#[test]
fn ordinary_guard_keeps_relation_and_typevar_mode_in_the_full_key() {
    let db = setup_db();
    let builder = ConstraintSetBuilder::new();
    let visitor = HasRelationToVisitor::default(&builder);
    let pairs = finite_guard_pairs();
    let mut calls = 0;
    for (relation, typevars) in [
        (TypeRelation::Assignability, TypeVarEvaluation::Eager),
        (TypeRelation::Subtyping, TypeVarEvaluation::Eager),
        (TypeRelation::Assignability, TypeVarEvaluation::Lazy),
    ] {
        let key = (pairs[0].0, pairs[0].1, relation, typevars);
        let expected = ConstraintSet::from_bool(&builder, typevars == TypeVarEvaluation::Eager);
        let actual = visitor.visit(&db, key, || {
            calls += 1;
            expected
        });
        assert!(actual.ownership_probe_same_set(expected));
        assert!(
            visitor
                .ownership_probe_cached(key)
                .is_some_and(|cached| cached.ownership_probe_same_set(expected))
        );
    }
    assert_eq!(calls, 3);
    assert_eq!(visitor.ownership_probe_counts(), (0, 3));
}

#[test]
fn runtime_guard_keeps_relation_and_typevar_mode_in_the_full_key() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let original = CheckerSnapshot::capture(&checker);
    let admission = Admission::default();
    let observations = Observations::default();
    let bodies = Cell::new(0);
    let pair = finite_guard_pairs()[0];
    let expected = ConstraintSet::from_bool(&builder, true);
    let revision = salsa::plumbing::current_revision(&db);
    let mut reader = db.clone();
    reader.clear_salsa_events();

    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let checker = &checker;
        let observations = &observations;
        let bodies = &bodies;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let mut effects = RuntimePairs::new(db, endpoint, checker.constraints);
                effects.observations = Some(observations);
                for (index, (relation, typevars)) in [
                    (TypeRelation::Assignability, TypeVarEvaluation::Eager),
                    (TypeRelation::Subtyping, TypeVarEvaluation::Eager),
                    (TypeRelation::Assignability, TypeVarEvaluation::Lazy),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut checker = checker.clone();
                    checker.relation = relation;
                    checker.typevar_evaluation = typevars;
                    let snapshot = CheckerSnapshot::capture(&checker);
                    assert_eq!(snapshot.resources, original.resources);
                    let actual = effects
                        .guard(&checker, pair.0, pair.1, || async {
                            bodies.set(bodies.get() + 1);
                            effects.check_type_pair(&checker, pair.0, pair.1).await
                        })
                        .await?;
                    assert!(actual.ownership_probe_same_set(expected));
                    assert_eq!(observations.last.get(), Some(snapshot));
                    assert_eq!(CheckerSnapshot::capture(&checker), snapshot);
                    assert_cached(&checker, pair, expected);
                    assert_eq!(
                        checker.relation_visitor.ownership_probe_counts(),
                        (0, index + 1)
                    );
                    let cached = effects
                        .guard(&checker, pair.0, pair.1, || async {
                            Err(RunError::Contract(
                                "mode-specific cache hit invoked its body",
                            ))
                        })
                        .await?;
                    assert!(cached.ownership_probe_same_set(expected));
                }
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert_eq!(bodies.get(), 3);
    assert_eq!(observations.entries.get(), 3);
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 3));
    assert_eq!(CheckerSnapshot::capture(&checker), original);
    assert!(!expansion_probe::active());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    report_accounting("guard_mode_keys", &admission, &mut reader, 4);
}

#[test]
fn context_recomputation_preserves_the_original_cached_handle() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let mut checker = owners.assignability(TypeVarSet::None);
    let context = ErrorContextTree::new(TypeRelation::Assignability);
    context.set_enabled(false);
    checker.context_tree = Some(context);
    let checker = &checker;
    let context = checker
        .context_tree
        .as_ref()
        .expect("prepared context tree");
    let admission = Admission::default();
    let observations = Observations::default();
    let observations = &observations;
    let pair = finite_guard_pairs()[1];
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let db = &db;
    let outcome = expansion_probe::run(db, usize::MAX, || {
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let mut effects = RuntimePairs::new(db, endpoint, checker.constraints);
                effects.observations = Some(observations);
                let original = guarded_terminal(&effects, checker, pair).await?;
                assert!(original.is_trivially_never_satisfied());
                context.set_enabled(true);
                // Contrasting child answers expose cache replacement. This tests guard storage
                // policy, not a change in the answer to an unchanged Python comparison.
                for _ in 0..2 {
                    let recomputed = effects
                        .guard(checker, pair.0, pair.1, || async {
                            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (1, 1));
                            assert_cached(checker, pair, original);
                            // The enclosing guard retains enabled context collection; this child
                            // only supplies its terminal result using the same original resources.
                            let quiet_child = checker.with_context_collection_disabled();
                            effects
                                .check_type_pair(&quiet_child, Type::Never, pair.1)
                                .await
                        })
                        .await?;
                    assert!(recomputed.is_trivially_always_satisfied());
                    assert_cached(checker, pair, original);
                    let quiet = checker.with_context_collection_disabled();
                    let reused = effects
                        .guard(&quiet, pair.0, pair.1, || async {
                            Err(RunError::Contract("quiet guard invoked its body"))
                        })
                        .await?;
                    assert!(reused.ownership_probe_same_set(original));
                    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
                }
                Ok(())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
    assert!(context.is_enabled());
    assert_eq!(observations.entries.get(), 3);
    report_accounting("guard_context", &admission, &mut reader, 4);
}

#[test]
fn conditional_cached_constraints_refuse_before_running_the_guard_body() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let mut checker = owners.assignability(TypeVarSet::None);
    let variable = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let source = KnownClass::Int.to_instance(&db, &env);
    let target = Type::TypeVar(variable);
    let mut preparation = checker.clone();
    preparation.typevar_evaluation = TypeVarEvaluation::Lazy;
    preparation.inferable = TypeVarSet::from_typevars(&db, [variable]);
    let conditional = preparation.check_type_pair(&db, source, target);
    assert!(
        !conditional.is_trivially_always_satisfied() && !conditional.is_trivially_never_satisfied()
    );
    checker
        .relation_visitor
        .visit(&db, guard_key(&checker, (source, target)), || conditional);
    checker.context_tree = Some(ErrorContextTree::new(TypeRelation::Assignability));
    let stamp = Stamp::current(&db);
    let admission = Admission::default();
    let body_ran = Cell::new(false);
    let after_guard = Cell::new(false);
    let returned = Cell::new(None);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let checker = &checker;
    let body_ran = &body_ran;
    let after_guard = &after_guard;
    let db = &db;
    let outcome = expansion_probe::run(db, usize::MAX, || {
        let result =
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                    let _ = effects
                        .guard(checker, source, target, || async {
                            body_ran.set(true);
                            Err(RunError::Contract("conditional cache ran its body"))
                        })
                        .await?;
                    after_guard.set(true);
                    Ok(())
                });
        returned.set(result.as_ref().err().copied());
        result
    })
    .0;
    assert!(
        matches!(
            outcome,
            Err(Incomplete::UnsupportedPairOperation(
                UnsupportedPairOperation::ConstraintSatisfaction
            ))
        ),
        "{outcome:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(
            salsa::attempt_probe::Incomplete::Interrupted
        ))
    );
    assert!(!body_ran.get() && !after_guard.get());
    assert_cached(checker, (source, target), conditional);
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 1));
    assert!(stamp.belongs_to(db));
    assert!(!expansion_probe::active());
    report_accounting("guard_conditional", &admission, &mut reader, 1);
}

#[test]
fn guard_identity_refuses_protocol_fields_but_short_circuits_distinct_nominal_sources()
-> anyhow::Result<()> {
    let db = TestDbBuilder::new().with_python_version(PythonVersion::PY313).with_file(
        "/src/guard_identity.py",
        "from typing import Protocol\nclass C[T]:\n    value: T\nclass P[T](Protocol):\n    value: T\nc1: C[int]\nc2: C[str]\np1: P[int]\np2: P[str]\n",
    ).build()?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/guard_identity.py")?,
        env.program(&db),
    );
    let symbol = |name| global_symbol(&db, file, name).place.expect_type();
    let (c1, c2, p1, p2) = (symbol("c1"), symbol("c2"), symbol("p1"), symbol("p2"));
    assert_ne!(c1, c2);
    assert_ne!(p1, p2);
    assert!(p1.as_protocol_instance().is_some() && p2.as_protocol_instance().is_some());
    assert!(p1.may_share_type_identity(&db, p2));
    assert!(!c1.may_share_type_identity(&db, c2));
    let stamp = Stamp::current(&db);
    for distinct_source in [false, true] {
        let builder = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &builder);
        let checker = owners.assignability(TypeVarSet::None);
        let original = CheckerSnapshot::capture(&checker);
        let admission = Admission::default();
        let observations = Observations::default();
        let inner_body = Cell::new(false);
        let after_guard = Cell::new(false);
        let returned = Cell::new(None);
        let inner = (if distinct_source { c2 } else { c1 }, p2);
        let mut reader = db.clone();
        reader.clear_salsa_events();
        let checker = &checker;
        let observations = &observations;
        let inner_body = &inner_body;
        let after_guard = &after_guard;
        let db = &db;
        let outcome = expansion_probe::run(db, usize::MAX, || {
            let result =
                RegistryBuilder::new(db, &admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let mut effects = RuntimePairs::new(db, endpoint, checker.constraints);
                        effects.observations = Some(observations);
                        // Guard keys exercise candidate classification; the child remains an audited
                        // terminal comparison, so this does not claim general protocol dispatch.
                        let result = effects
                            .guard(checker, c1, p1, || async {
                                effects
                                    .guard(checker, inner.0, inner.1, || async {
                                        inner_body.set(true);
                                        effects
                                            .check_type_pair(
                                                checker,
                                                Type::Never,
                                                Type::int_literal(1),
                                            )
                                            .await
                                    })
                                    .await
                            })
                            .await?;
                        assert!(result.is_trivially_always_satisfied());
                        after_guard.set(true);
                        Ok(())
                    });
            returned.set(result.as_ref().err().copied());
            result
        })
        .0;
        if distinct_source {
            assert!(matches!(outcome, Ok(Ok(()))), "{outcome:?}");
            assert!(inner_body.get() && after_guard.get());
            assert_eq!(observations.entries.get(), 1);
            assert_eq!(observations.last.get(), Some(original));
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 2));
            assert_cached(checker, (c1, p1), ConstraintSet::from_bool(&builder, true));
            assert_cached(checker, inner, ConstraintSet::from_bool(&builder, true));
        } else {
            assert!(
                matches!(
                    outcome,
                    Err(Incomplete::UnsupportedPairOperation(
                        UnsupportedPairOperation::GuardIdentity
                    ))
                ),
                "{outcome:?}"
            );
            assert_eq!(
                returned.get(),
                Some(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted
                ))
            );
            assert!(!inner_body.get() && !after_guard.get());
            assert_eq!(observations.entries.get(), 0);
            assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
            assert!(
                checker
                    .relation_visitor
                    .ownership_probe_cached(guard_key(checker, inner))
                    .is_none()
            );
        }
        assert_eq!(CheckerSnapshot::capture(checker), original);
        assert!(stamp.belongs_to(db));
        assert!(!expansion_probe::active());
        report_accounting(
            "guard_identity",
            &admission,
            &mut reader,
            if distinct_source { 2 } else { 1 },
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum TodoGuardPosition {
    Incoming,
    WrappedIncoming,
    ActiveTop,
    InlineCache,
    SpilledCache,
}

#[test]
fn todo_guard_keys_refuse_before_hashing_or_migrating_label_payloads() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let todo = todo_type!("guard key label");
    let wrapped = SubclassOfType::from(&db, &env, SubclassOfInner::Dynamic(todo.expect_dynamic()));
    let stamp = Stamp::current(&db);
    for position in [
        TodoGuardPosition::Incoming,
        TodoGuardPosition::WrappedIncoming,
        TodoGuardPosition::ActiveTop,
        TodoGuardPosition::InlineCache,
        TodoGuardPosition::SpilledCache,
    ] {
        let builder = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &builder);
        let checker = owners.assignability(TypeVarSet::None);
        let pair = match position {
            TodoGuardPosition::Incoming => (todo, Type::int_literal(1)),
            TodoGuardPosition::WrappedIncoming => (wrapped, Type::int_literal(1)),
            _ => finite_guard_pairs()[0],
        };
        let always = ConstraintSet::from_bool(&builder, true);
        let mut cached = Vec::new();
        let cache_case = matches!(
            position,
            TodoGuardPosition::InlineCache | TodoGuardPosition::SpilledCache
        );
        if cache_case {
            let key = guard_key(&checker, (todo, Type::int_literal(0)));
            checker.relation_visitor.visit(&db, key, || always);
            cached.push(key);
            loop {
                let state = checker.relation_visitor.ownership_probe_storage();
                let full = match position {
                    TodoGuardPosition::InlineCache => state.cache_len == 2,
                    TodoGuardPosition::SpilledCache => {
                        state.cache_capacity == Some(state.cache_len)
                    }
                    _ => false,
                };
                if full {
                    break;
                }
                let key = guard_key(
                    &checker,
                    (
                        Type::int_literal(1000 + cached.len() as i64),
                        Type::int_literal(2),
                    ),
                );
                checker.relation_visitor.visit(&db, key, || always);
                cached.push(key);
            }
        }
        let active = if matches!(position, TodoGuardPosition::ActiveTop) {
            match RelationGuardStep::start(
                &db,
                &checker,
                todo,
                Type::int_literal(0),
                &OrdinaryDependencies,
            )? {
                RelationGuardStep::Evaluate(scope) => Some(scope),
                _ => anyhow::bail!("ordinary Todo fixture did not create its active scope"),
            }
        } else {
            None
        };
        let before = checker.relation_visitor.ownership_probe_storage();
        let admission = Admission::default();
        let body_ran = Cell::new(false);
        let after_guard = Cell::new(false);
        let returned = Cell::new(None);
        let mut reader = db.clone();
        reader.clear_salsa_events();
        let checker = &checker;
        let body_ran = &body_ran;
        let after_guard = &after_guard;
        let db = &db;
        let outcome = expansion_probe::run(db, usize::MAX, || {
            let result =
                RegistryBuilder::new(db, &admission)?
                    .seal()?
                    .run(move |endpoint| async move {
                        let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                        let _ = effects
                            .guard(checker, pair.0, pair.1, || async {
                                body_ran.set(true);
                                effects.check_type_pair(checker, pair.0, pair.1).await
                            })
                            .await?;
                        after_guard.set(true);
                        Ok(())
                    });
            returned.set(result.as_ref().err().copied());
            result
        })
        .0;
        assert!(
            matches!(
                outcome,
                Err(Incomplete::UnsupportedPairOperation(
                    UnsupportedPairOperation::GuardKey
                ))
            ),
            "{position:?}: {outcome:?}"
        );
        assert_eq!(
            returned.get(),
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Interrupted
            ))
        );
        assert_eq!(body_ran.get(), cache_case);
        assert!(!after_guard.get());
        assert_eq!(checker.relation_visitor.ownership_probe_storage(), before);
        assert!(
            checker
                .relation_visitor
                .ownership_probe_cached(guard_key(checker, pair))
                .is_none()
        );
        for key in cached {
            assert!(
                checker
                    .relation_visitor
                    .ownership_probe_cached(key)
                    .is_some_and(|value| value.ownership_probe_same_set(always))
            );
        }
        drop(active);
        assert_eq!(checker.relation_visitor.ownership_probe_counts().0, 0);
        assert!(stamp.belongs_to(db));
        assert!(!expansion_probe::active());
        report_accounting(
            "guard_todo",
            &admission,
            &mut reader,
            if cache_case { 2 } else { 1 },
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuardFault {
    ActiveResource,
    CacheResource,
    ProbeRefusal,
    FinishRefusal,
    FinishAcceptance,
}

struct GuardFaultAdmission<'run, 'db: 'run, 'c: 'run> {
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    visitor: &'run HasRelationToVisitor<'db, 'c>,
    cleanup: &'run dyn Fn(),
    child_factory_ran: &'run Cell<bool>,
    fault: GuardFault,
    completed: usize,
    armed: Cell<bool>,
    fired: Cell<bool>,
    previous_work: Cell<Option<usize>>,
    allocation: Cell<Option<GuardAllocation>>,
    commit_probe_seen: Cell<bool>,
    injected_state: Cell<Option<CycleDetectorStorageProbe>>,
    audit: Admission,
}

impl ExecutionAdmission for GuardFaultAdmission<'_, '_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.audit.admit(work)?;
        if !self.armed.get() || self.fired.get() {
            return Ok(());
        }
        let state = self.visitor.ownership_probe_storage();
        let inject = match work {
            ExecutionWork::Resource { requested_bytes } => {
                let active = self.fault == GuardFault::ActiveResource
                    && state.active_len == 1
                    && state.active_capacity == 1
                    && state.cache_len == 0
                    && requested_bytes == 4 * state.active_entry_bytes;
                let cache = self.fault != GuardFault::ActiveResource
                    && state.active_len == 1
                    && state.cache_len == self.completed
                    && state
                        .cache_capacity
                        .is_none_or(|capacity| capacity == state.cache_len)
                    && requested_bytes
                        == map_growth::<RelationKey<'_>, ConstraintSet<'_, '_>, ()>(
                            state.cache_len,
                            state.cache_capacity.unwrap_or(0),
                            state.cache_len + 1,
                        )
                        .unwrap()
                        .requested_payload_bytes;
                if active || cache {
                    assert!(self.allocation.get().is_none());
                    self.allocation.set(Some(GuardAllocation {
                        before: state,
                        previous_work: self.previous_work.get(),
                        requested_bytes,
                    }));
                }
                active || (cache && self.fault == GuardFault::CacheResource)
            }
            ExecutionWork::Work { units } => {
                self.previous_work.set(Some(units));
                if matches!(
                    self.fault,
                    GuardFault::ProbeRefusal
                        | GuardFault::FinishRefusal
                        | GuardFault::FinishAcceptance
                ) && self.allocation.get().is_some()
                    && state.active_len == 1
                    && state.cache_len == self.completed
                    && state.cache_capacity.is_some()
                {
                    // This is the commit-probe admission after migration, then the final Finish.
                    if units == 3 {
                        assert!(!self.commit_probe_seen.replace(true));
                        self.fault == GuardFault::ProbeRefusal
                    } else {
                        self.commit_probe_seen.get() && units == 1
                    }
                } else {
                    false
                }
            }
            _ => false,
        };
        if !inject {
            return Ok(());
        }
        self.fired.set(true);
        self.injected_state.set(Some(state));
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract("guard fault has no endpoint"))?;
        let cleanup = QueuedCleanup(self.cleanup);
        let child_factory_ran = self.child_factory_ran;
        let pending = endpoint.demand(move || {
            child_factory_ran.set(true);
            async move {
                let _held = cleanup;
                Ok(())
            }
        })?;
        *self.pending.borrow_mut() = Some(pending);
        if self.fault == GuardFault::FinishAcceptance {
            Ok(())
        } else {
            Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            ))
        }
    }
}

fn guard_fault_retains_original_scope(fault: GuardFault, completed: usize) {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let checker = owners.assignability(TypeVarSet::None);
    let original = CheckerSnapshot::capture(&checker);
    let base = finite_guard_pairs();
    let mut pairs = base[..2].to_vec();
    pairs.extend((2..completed).map(|index| (Type::Never, Type::int_literal(1000 + index as i64))));
    pairs.push(base[2]);
    assert_eq!(pairs.len(), completed + 1);
    let expected: Vec<_> = pairs
        .iter()
        .map(|pair| checker.check_type_pair(&db, pair.0, pair.1))
        .collect();
    let stamp = Stamp::current(&db);
    let observations = Observations::default();
    let accepted_result = Cell::new(None::<ConstraintSet<'_, '_>>);
    let inner_body = Cell::new(false);
    let after_guard = Cell::new(false);
    let cleanup_count = Cell::new(0);
    let journal = RefCell::new(Vec::new());
    let cleanup = || {
        assert_eq!(CheckerSnapshot::capture(&checker), original);
        assert_eq!(observations.last.get(), Some(original));
        assert!(!after_guard.get());
        let state = checker.relation_visitor.ownership_probe_storage();
        assert_eq!(state.active_len, 1);
        let accepted = accepted_result
            .get()
            .expect("terminal child completed before the fault");
        if fault == GuardFault::ActiveResource {
            assert_eq!(
                (state.active_capacity, state.cache_len, state.cache_capacity),
                (1, 0, None)
            );
            assert_eq!(observations.entries.get(), 1);
            assert!(!inner_body.get());
            assert!(accepted.ownership_probe_same_set(expected[0]));
            for &pair in &pairs {
                assert!(
                    checker
                        .relation_visitor
                        .ownership_probe_cached(guard_key(&checker, pair))
                        .is_none()
                );
            }
        } else {
            assert_eq!(state.cache_len, completed);
            assert_eq!(observations.entries.get(), completed + 1);
            assert!(inner_body.get());
            assert!(accepted.ownership_probe_same_set(expected[completed]));
            for (&pair, &value) in pairs[..completed].iter().zip(&expected[..completed]) {
                assert_cached(&checker, pair, value);
            }
            assert!(
                checker
                    .relation_visitor
                    .ownership_probe_cached(guard_key(&checker, pairs[completed]))
                    .is_none()
            );
            if fault == GuardFault::CacheResource {
                assert_eq!(state.cache_capacity, (completed > 2).then_some(completed));
            } else {
                assert!(
                    state
                        .cache_capacity
                        .is_some_and(|capacity| capacity >= (completed * 2).max(4))
                );
            }
        }
        assert_eq!(
            checker.disjointness_visitor.ownership_probe_counts(),
            (0, 0)
        );
        assert!(checker.signature_relation_visitor.is_empty());
        // The original builder permits a mutable storage borrow while the guard owns its scope.
        assert!(
            !ConstraintSet::from_bool(checker.constraints, true)
                .is_never_satisfied(&db, checker.env)
        );
        cleanup_count.set(cleanup_count.get() + 1);
        journal.borrow_mut().push("child");
    };
    let child_factory_ran = Cell::new(false);
    let returned = Cell::new(None);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let admission;
    let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
    let pending = RefCell::new(None);
    admission = GuardFaultAdmission {
        endpoint: &endpoint_slot,
        pending: &pending,
        visitor: checker.relation_visitor,
        cleanup: &cleanup,
        child_factory_ran: &child_factory_ran,
        fault,
        completed,
        armed: Cell::new(false),
        fired: Cell::new(false),
        previous_work: Cell::new(None),
        allocation: Cell::new(None),
        commit_probe_seen: Cell::new(false),
        injected_state: Cell::new(None),
        audit: Admission::default(),
    };
    let reset = ResetSlots {
        endpoint: &endpoint_slot,
        pending: &pending,
    };
    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let pairs = &pairs;
        let expected = &expected;
        let checker = &checker;
        let observations = &observations;
        let accepted_result = &accepted_result;
        let inner_body = &inner_body;
        let after_guard = &after_guard;
        let journal = &journal;
        let admission = &admission;
        let db = &db;
        let result = RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| {
                **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                async move {
                    let _root = RootCleanup(journal);
                    let mut effects = RuntimePairs::new(db, endpoint, checker.constraints);
                    effects.observations = Some(observations);
                    if fault == GuardFault::ActiveResource {
                        effects
                            .guard(checker, pairs[0].0, pairs[0].1, || async {
                                let first = effects
                                    .check_type_pair(checker, pairs[0].0, pairs[0].1)
                                    .await?;
                                accepted_result.set(Some(first));
                                admission.armed.set(true);
                                effects
                                    .guard(checker, pairs[1].0, pairs[1].1, || async {
                                        inner_body.set(true);
                                        effects
                                            .check_type_pair(checker, pairs[1].0, pairs[1].1)
                                            .await
                                    })
                                    .await
                            })
                            .await?;
                    } else {
                        for index in 0..completed {
                            let result = guarded_terminal(&effects, checker, pairs[index]).await?;
                            assert!(result.ownership_probe_same_set(expected[index]));
                        }
                        effects
                            .guard(checker, pairs[completed].0, pairs[completed].1, || async {
                                inner_body.set(true);
                                let result = effects
                                    .check_type_pair(
                                        checker,
                                        pairs[completed].0,
                                        pairs[completed].1,
                                    )
                                    .await?;
                                accepted_result.set(Some(result));
                                admission.armed.set(true);
                                Ok(result)
                            })
                            .await?;
                    }
                    after_guard.set(true);
                    Ok(())
                }
            });
        returned.set(result.as_ref().err().copied());
        result
    })
    .0;
    if fault == GuardFault::FinishAcceptance {
        assert!(
            matches!(outcome, Err(Incomplete::Interrupted)),
            "{outcome:?}"
        );
        assert_eq!(
            returned.get(),
            Some(RunError::Contract("completed task retained a child"))
        );
        assert!(admission.commit_probe_seen.get());
    } else {
        assert!(matches!(outcome, Err(Incomplete::Allowance)), "{outcome:?}");
        assert_eq!(
            returned.get(),
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance
            ))
        );
        assert_eq!(
            admission.commit_probe_seen.get(),
            matches!(fault, GuardFault::ProbeRefusal | GuardFault::FinishRefusal)
        );
    }
    assert!(admission.fired.get());
    assert!(!child_factory_ran.get());
    assert!(!after_guard.get());
    assert_eq!(cleanup_count.get(), 1);
    assert_eq!(&*journal.borrow(), &["child", "root"]);
    assert_eq!(CheckerSnapshot::capture(&checker), original);
    let after = checker.relation_visitor.ownership_probe_storage();
    let injected = admission
        .injected_state
        .get()
        .expect("fault recorded its storage");
    assert_eq!(after.active_len, 0);
    assert_eq!(after.cache_len, injected.cache_len);
    assert_eq!(after.cache_capacity, injected.cache_capacity);
    let allocation = admission
        .allocation
        .get()
        .expect("guard quoted its allocation");
    if fault == GuardFault::ActiveResource {
        assert_eq!(allocation.previous_work, Some(1));
        assert_eq!(
            allocation.requested_bytes,
            4 * allocation.before.active_entry_bytes
        );
        assert_eq!(after.active_capacity, 1);
    } else {
        let plan = map_growth::<RelationKey<'_>, ConstraintSet<'_, '_>, ()>(
            completed,
            allocation.before.cache_capacity.unwrap_or(0),
            completed + 1,
        )
        .unwrap();
        assert_eq!(allocation.previous_work, Some(plan.relocation_units));
        assert_eq!(allocation.requested_bytes, plan.requested_payload_bytes);
        for (&pair, &value) in pairs[..completed].iter().zip(&expected[..completed]) {
            assert_cached(&checker, pair, value);
        }
    }
    assert!(
        checker
            .relation_visitor
            .ownership_probe_cached(guard_key(&checker, pairs[completed]))
            .is_none()
    );
    assert!(stamp.belongs_to(&db));
    assert!(!expansion_probe::active());
    drop(reset);
    assert!(endpoint_slot.borrow().is_none() && pending.borrow().is_none());
    let (name, tasks) = match fault {
        GuardFault::ActiveResource => ("guard_active_resource_refusal", 3),
        GuardFault::CacheResource => ("guard_cache_resource_refusal", completed + 3),
        GuardFault::ProbeRefusal => ("guard_probe_refusal", completed + 3),
        GuardFault::FinishRefusal => ("guard_finish_refusal", completed + 3),
        GuardFault::FinishAcceptance => ("guard_finish_acceptance_refusal", completed + 3),
    };
    eprintln!(
        "GUARD_FAULT {fault:?} allocation={allocation:?} injected={injected:?} after={after:?}"
    );
    report_accounting(name, &admission.audit, &mut reader, tasks);
    let retry_admission = Admission::default();
    let retried = expansion_probe::run(&db, usize::MAX, || {
        let checker = &checker;
        let pairs = &pairs;
        let expected = &expected;
        let db = &db;
        RegistryBuilder::new(db, &retry_admission)?
            .seal()?
            .run(move |endpoint| async move {
                let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                for (&pair, &expected) in pairs.iter().zip(expected) {
                    let actual = guarded_terminal(&effects, checker, pair).await?;
                    assert!(actual.ownership_probe_same_set(expected));
                    assert_cached(checker, pair, expected);
                    let cached = effects
                        .guard(checker, pair.0, pair.1, || async {
                            Err(RunError::Contract(
                                "same-visitor retry did not cache its result",
                            ))
                        })
                        .await?;
                    assert!(cached.ownership_probe_same_set(expected));
                }
                Ok(())
            })
    })
    .0;
    assert_eq!(retried, Ok(Ok(())));
    assert_eq!(
        checker.relation_visitor.ownership_probe_counts(),
        (0, completed + 1)
    );
    assert_eq!(CheckerSnapshot::capture(&checker), original);
    assert!(stamp.belongs_to(&db));
}

#[test]
fn active_growth_refusal_retains_the_enclosing_guard_and_original_resources() {
    guard_fault_retains_original_scope(GuardFault::ActiveResource, 2);
}

#[test]
fn cache_growth_refusal_retains_the_finishing_guard_and_completed_cache() {
    guard_fault_retains_original_scope(GuardFault::CacheResource, 2);
}

#[test]
fn finish_acceptance_rejection_keeps_the_scope_and_new_result_unpublished() {
    guard_fault_retains_original_scope(GuardFault::FinishAcceptance, 2);
}

#[test]
fn later_guard_growth_and_post_reservation_refusals_retry_on_the_same_visitor() {
    for completed in [2, 7, 14] {
        for fault in [
            GuardFault::CacheResource,
            GuardFault::ProbeRefusal,
            GuardFault::FinishRefusal,
            GuardFault::FinishAcceptance,
        ] {
            guard_fault_retains_original_scope(fault, completed);
        }
    }
}

fn controlled_guard_fill<'db, 'c>(
    db: &'db TestDb,
    checker: &TypeRelationChecker<'_, 'c, 'db>,
    pairs: &[(Type<'db>, Type<'db>)],
) -> Vec<ConstraintSet<'db, 'c>> {
    assert_eq!(checker.relation_visitor.ownership_probe_counts(), (0, 0));
    let admission = GrowthAdmission {
        visitor: checker.relation_visitor,
        previous_work: Cell::new(None),
        allocations: RefCell::new(Vec::new()),
        audit: Admission::default(),
    };
    let observations = Observations::default();
    let values = expansion_probe::run(db, usize::MAX, || {
        let admission = &admission;
        let observations = &observations;
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                let mut effects = RuntimePairs::new(db, endpoint, checker.constraints);
                effects.observations = Some(observations);
                let mut values = Vec::new();
                for &pair in pairs {
                    let before = checker.relation_visitor.ownership_probe_storage();
                    let allocations = admission.allocations.borrow().len();
                    let result = guarded_terminal(&effects, checker, pair).await?;
                    assert_cached(checker, pair, result);
                    values.push(result);
                    let after = checker.relation_visitor.ownership_probe_storage();
                    assert_eq!(after.active_len, 0);
                    assert_eq!(after.cache_len, before.cache_len + 1);
                    let grows = before.cache_len == 2 && before.cache_capacity.is_none()
                        || before.cache_capacity == Some(before.cache_len);
                    assert_eq!(
                        admission.allocations.borrow().len() - allocations,
                        usize::from(grows)
                    );
                    if !grows {
                        assert_eq!(before.cache_capacity, after.cache_capacity);
                    }
                }
                Ok(values)
            })
    })
    .0
    .unwrap()
    .unwrap();
    assert_eq!(observations.entries.get(), pairs.len());
    let after = checker.relation_visitor.ownership_probe_storage();
    let mut bulk = 0;
    for allocation in admission.allocations.borrow().iter() {
        let before = allocation.before;
        assert_eq!(before.active_len, 1);
        let capacity = before.cache_capacity.unwrap_or(0);
        let requested = (2 * capacity).max(before.cache_len + 1).max(4);
        let old_extent = if capacity == 0 {
            0
        } else {
            hash_slots::<()>(capacity).unwrap()
        };
        let relocation = old_extent + before.cache_len + hash_slots::<()>(2 * requested).unwrap();
        assert_eq!(allocation.previous_work, Some(relocation));
        assert_eq!(
            allocation.requested_bytes,
            requested * size_of::<(RelationKey<'_>, ConstraintSet<'_, '_>)>()
        );
        // Retained-key validation is a separate real scan from table relocation.
        bulk += relocation
            + before
                .cache_capacity
                .map_or(2, |capacity| hash_slots::<()>(capacity).unwrap());
    }
    assert!(bulk <= 128 * (after.cache_capacity.unwrap_or(2) + 1));
    assert!(after.cache_capacity.unwrap_or(2) <= 4 * pairs.len() + 8);
    assert_eq!(after.active_capacity, 1);
    values
}

#[test]
fn controlled_shallow_guard_fills_have_geometric_growth_and_real_results() {
    let db = setup_db();
    let env = db.program_environment();
    for width in [3, 32, 128] {
        let builder = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(&env, &builder);
        let checker = owners.assignability(TypeVarSet::None);
        let oracle_owners = RelationOwners::new(&env, &builder);
        let oracle = oracle_owners.assignability(TypeVarSet::None);
        let pairs: Vec<_> = (0..width)
            .map(|index| {
                if index % 2 == 0 {
                    (Type::Never, Type::int_literal(1000 + index as i64))
                } else {
                    (
                        Type::int_literal(1000 + index as i64),
                        Type::int_literal(2000 + index as i64),
                    )
                }
            })
            .collect();
        let expected: Vec<_> = pairs
            .iter()
            .map(|&(left, right)| oracle.check_type_pair(&db, left, right))
            .collect();
        let actual = controlled_guard_fill(&db, &checker, &pairs);
        for ((&pair, actual), expected) in pairs.iter().zip(actual).zip(expected) {
            assert!(actual.ownership_probe_same_set(expected));
            assert_cached(&checker, pair, expected);
        }
    }
}

#[test]
fn scalar_constraint_set_relations_keep_lazy_mode_and_their_fresh_builder() {
    let db = setup_db();
    let env = db.program_environment();
    let program = env.program(&db);
    let form = Type::TypeForm(crate::types::TypeFormType::new(&db, Type::int_literal(1)));
    for (left, right) in [(form, form), (Type::int_literal(1), Type::int_literal(2))] {
        let expected = left.is_constraint_set_assignable_to(&db, &env, right);
        let outer = ConstraintSetBuilder::new();
        let capacity = CallResourceCapacity {
            calls: NonZeroUsize::MIN,
        };
        let environments = CallEnvironments::with_capacity(capacity);
        let builders = CallBuilders::with_capacity(capacity);
        let owners = CallRelationOwners::with_capacity(capacity);
        let scalar_resources = Cell::new(None);
        let pair_seen = Cell::new(false);
        let always_seen = Cell::new(false);
        let observer = |observation: ConstraintSetObservation<'_, '_>| match observation {
            ConstraintSetObservation::Checker {
                resources,
                relation,
                evaluation,
                inferable_none,
                given_never,
            } => {
                assert_eq!(relation, TypeRelation::Assignability);
                assert_eq!(evaluation, TypeVarEvaluation::Lazy);
                assert!(inferable_none && given_never);
                assert_ne!(resources[1], std::ptr::from_ref(&outer).cast());
                assert!(resources.iter().all(|pointer| !pointer.is_null()));
                scalar_resources.set(Some(resources));
            }
            ConstraintSetObservation::PairResult(constraints) => {
                assert!(scalar_resources.get().is_some());
                assert_eq!(constraints.is_trivially_always_satisfied(), expected);
                assert_eq!(constraints.is_trivially_never_satisfied(), !expected);
                pair_seen.set(true);
            }
            ConstraintSetObservation::AlwaysResult {
                constraints,
                result,
            } => {
                assert!(pair_seen.get());
                assert_eq!(result, expected);
                assert_eq!(constraints.is_trivially_always_satisfied(), expected);
                always_seen.set(true);
            }
            ConstraintSetObservation::OwnedResult(_) => {
                panic!("scalar relation entered the owned wrapper")
            }
        };
        let admission = Admission::default();
        let outcome = expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let environments = &environments;
            let builders = &builders;
            let owners = &owners;
            let observer = &observer;
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    constraint_set::assignable_observed(
                        db,
                        &endpoint,
                        program,
                        left,
                        right,
                        environments,
                        builders,
                        owners,
                        NoProtocolQueries,
                        Some(observer),
                    )
                    .await
                })
        })
        .0;
        assert!(
            matches!(outcome, Ok(Ok(actual)) if actual == expected),
            "{outcome:?}"
        );
        assert!(pair_seen.get() && always_seen.get());
    }
}

#[test]
fn lazy_constraint_set_owner_preserves_each_original_resource() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let eager = CheckerSnapshot::capture(&owners.assignability(TypeVarSet::None));
    let lazy = CheckerSnapshot::capture(&owners.constraint_set_assignability());
    assert_eq!(lazy.resources, eager.resources);
    assert_eq!(lazy.relation, TypeRelation::Assignability);
    assert_eq!(lazy.typevars, TypeVarEvaluation::Lazy);
    assert!(lazy.inferable_none && lazy.given_never && !lazy.given_always);
    assert_eq!(lazy.context_enabled, None);
    assert!(!lazy.observations && lazy.expensive);
}

#[test]
fn owned_identical_bound_shortcut_uses_the_shared_wrapper_without_a_query() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let form = Type::TypeForm(crate::types::TypeFormType::new(&db, Type::int_literal(1)));
    let observed = Cell::new(false);
    let observer = |observation: ConstraintSetObservation<'_, '_>| match observation {
        ConstraintSetObservation::OwnedResult(result) => {
            assert!(matches!(result, std::borrow::Cow::Owned(_)));
            assert!(result.is_trivially_always_satisfied());
            observed.set(true);
        }
        _ => panic!("owned shortcut constructed a scalar checker"),
    };
    let admission = Admission::default();
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let observer = &observer;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let result = constraint_set::owned_assignable_observed(
                    db,
                    &endpoint,
                    program,
                    form,
                    form,
                    Some(observer),
                )
                .await?;
                Ok(matches!(result, std::borrow::Cow::Owned(_))
                    && result.is_trivially_always_satisfied())
            })
    })
    .0;
    assert!(matches!(outcome, Ok(Ok(true))), "{outcome:?}");
    assert!(observed.get());
    assert!(
        !reader
            .take_salsa_events()
            .iter()
            .any(|event| matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
}

#[test]
fn nontrivial_owned_relation_refuses_before_publishing_a_result() {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let observed = Cell::new(false);
    let observer = |_: ConstraintSetObservation<'_, '_>| observed.set(true);
    let admission = Admission::default();
    let returned = Cell::new(None);
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let observer = &observer;
        let result =
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    constraint_set::owned_assignable_observed(
                        db,
                        &endpoint,
                        program,
                        Type::int_literal(1),
                        Type::int_literal(2),
                        Some(observer),
                    )
                    .await
                    .map(|_| ())
                });
        returned.set(result.as_ref().err().copied());
        result
    })
    .0;
    assert!(
        matches!(
            outcome,
            Err(Incomplete::UnsupportedSequentOperation(
                crate::types::constraints::UnsupportedSequentOperation::OwnedAssignable
            ))
        ),
        "{outcome:?}"
    );
    assert_eq!(
        returned.get(),
        Some(RunError::Refused(
            salsa::attempt_probe::Incomplete::Interrupted
        ))
    );
    assert!(!observed.get());
    assert!(
        !reader
            .take_salsa_events()
            .iter()
            .any(|event| matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
}

struct ScalarNativePanic(Arc<()>);

struct ScalarFaultAdmission<'run, 'db: 'run> {
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    cleanup: &'run dyn Fn(),
    armed: Cell<bool>,
    fired: Cell<bool>,
    refused_work: Cell<Option<usize>>,
    panic: bool,
    panic_marker: Arc<()>,
}

impl ExecutionAdmission for ScalarFaultAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !matches!(work, ExecutionWork::Work { .. })
            || !self.armed.get()
            || self.fired.replace(true)
        {
            return Ok(());
        }
        if let ExecutionWork::Work { units } = work {
            self.refused_work.set(Some(units));
        }
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract("scalar fault has no endpoint"))?;
        let cleanup = QueuedCleanup(self.cleanup);
        *self.pending.borrow_mut() = Some(endpoint.demand(move || async move {
            let _held = cleanup;
            Ok(())
        })?);
        if self.panic {
            std::panic::panic_any(ScalarNativePanic(Arc::clone(&self.panic_marker)));
        }
        Err(RunError::Refused(
            salsa::attempt_probe::Incomplete::Allowance,
        ))
    }
}

fn scalar_satisfaction_fault_drains_queued_child(panic: bool) {
    let db = setup_db();
    let program = db.program_environment().program(&db);
    let capacity = CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    };
    let environments = CallEnvironments::with_capacity(capacity);
    let builders = CallBuilders::with_capacity(capacity);
    let owners = CallRelationOwners::with_capacity(capacity);
    let journal = RefCell::new(Vec::new());
    let scalar_resources = Cell::new(None);
    let pair_seen = Cell::new(false);
    let always_seen = Cell::new(false);
    let cleanup_observation = Cell::new(None);
    let cleanup = || {
        cleanup_observation.set(Some((
            scalar_resources.get(),
            pair_seen.get(),
            always_seen.get(),
        )));
        journal.borrow_mut().push("child");
    };
    let admission;
    let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
    let pending = RefCell::new(None);
    let panic_marker = Arc::new(());
    admission = ScalarFaultAdmission {
        endpoint: &endpoint_slot,
        pending: &pending,
        cleanup: &cleanup,
        armed: Cell::new(false),
        fired: Cell::new(false),
        refused_work: Cell::new(None),
        panic,
        panic_marker: Arc::clone(&panic_marker),
    };
    let reset = ResetSlots {
        endpoint: &endpoint_slot,
        pending: &pending,
    };
    let observer = |observation: ConstraintSetObservation<'_, '_>| match observation {
        ConstraintSetObservation::Checker { resources, .. } => {
            scalar_resources.set(Some(resources))
        }
        ConstraintSetObservation::PairResult(constraints) => {
            pair_seen.set(constraints.is_trivially_always_satisfied());
            admission.armed.set(true);
        }
        ConstraintSetObservation::AlwaysResult { .. } => always_seen.set(true),
        ConstraintSetObservation::OwnedResult(_) => {}
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let environments = &environments;
            let builders = &builders;
            let owners = &owners;
            let observer = &observer;
            let admission = &admission;
            let journal = &journal;
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(move |endpoint| {
                    **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                    async move {
                        let _root = RootCleanup(journal);
                        constraint_set::assignable_observed(
                            db,
                            &endpoint,
                            program,
                            Type::int_literal(1),
                            Type::int_literal(1),
                            environments,
                            builders,
                            owners,
                            NoProtocolQueries,
                            Some(observer),
                        )
                        .await
                    }
                })
        })
        .0
    }));
    if panic {
        assert!(outcome.is_err_and(|payload| {
            payload
                .downcast_ref::<ScalarNativePanic>()
                .is_some_and(|payload| Arc::ptr_eq(&payload.0, &panic_marker))
        }));
    } else {
        assert!(matches!(outcome, Ok(Err(Incomplete::Allowance))));
    }
    assert!(admission.fired.get());
    assert!(scalar_resources.get().is_some());
    assert_eq!(
        cleanup_observation.get(),
        Some((scalar_resources.get(), true, false))
    );
    assert_eq!(&*journal.borrow(), &["child", "root"]);
    assert!(!always_seen.get());
    assert!(!expansion_probe::active());
    drop(reset);
    assert!(endpoint_slot.borrow().is_none() && pending.borrow().is_none());
}

#[test]
fn scalar_always_refusal_drains_queued_children_before_root_cleanup() {
    scalar_satisfaction_fault_drains_queued_child(false);
}

#[test]
fn scalar_always_native_panic_drains_queued_children_before_root_cleanup() {
    scalar_satisfaction_fault_drains_queued_child(true);
}

#[test]
fn pair_field_refusal_drains_pending_child_before_caller_and_retries() {
    let db = setup_db();
    let argument = Type::bool_literal(true);
    let type_guard = TypeGuardType::new(&db, argument, None);
    let revision = salsa::plumbing::current_revision(&db);
    let builders = CallBuilders::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let measurement = Admission::default();
    let first_field_work = Cell::new(None);
    let measured = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builders = &builders;
        let measurement = &measurement;
        let first_field_work = &first_field_work;
        RegistryBuilder::new(db, measurement)?
            .seal()?
            .run(move |endpoint| async move {
                let builder = builders.allocate(&endpoint).await;
                let effects = RuntimePairs::new(db, endpoint, builder);
                let start = measurement.events.borrow().len();
                assert_eq!(effects.type_guard_return(type_guard).await?, argument);
                first_field_work.set(measurement.events.borrow()[start..].iter().find_map(
                    |event| match event {
                        ExecutionWork::Work { units } => Some(*units),
                        _ => None,
                    },
                ));
                Ok(())
            })
    })
    .0;
    assert!(matches!(measured, Ok(Ok(()))), "{measured:?}");
    assert!(first_field_work.get().is_some_and(|units| units > 0));

    let builders = CallBuilders::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let journal = RefCell::new(Vec::new());
    let observed = Cell::new(false);
    let cleanup = || journal.borrow_mut().push("child");
    let admission;
    let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
    let pending = RefCell::new(None);
    admission = ScalarFaultAdmission {
        endpoint: &endpoint_slot,
        pending: &pending,
        cleanup: &cleanup,
        armed: Cell::new(false),
        fired: Cell::new(false),
        refused_work: Cell::new(None),
        panic: false,
        panic_marker: Arc::new(()),
    };
    let reset = ResetSlots {
        endpoint: &endpoint_slot,
        pending: &pending,
    };
    let refused = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builders = &builders;
        let admission = &admission;
        let journal = &journal;
        let observed = &observed;
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| {
                **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                async move {
                    let _caller = RootCleanup(journal);
                    let builder = builders.allocate(&endpoint).await;
                    let effects = RuntimePairs::new(db, endpoint, builder);
                    admission.armed.set(true);
                    let result = effects.type_guard_return(type_guard).await?;
                    observed.set(true);
                    Ok(result)
                }
            })
    })
    .0;
    assert!(matches!(refused, Err(Incomplete::Allowance)), "{refused:?}");
    assert!(admission.fired.get());
    assert_eq!(admission.refused_work.get(), first_field_work.get());
    assert!(!observed.get());
    assert_eq!(&*journal.borrow(), &["child", "root"]);
    assert!(!expansion_probe::active());
    drop(reset);
    assert!(endpoint_slot.borrow().is_none() && pending.borrow().is_none());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let builders = CallBuilders::with_capacity(CallResourceCapacity {
        calls: NonZeroUsize::MIN,
    });
    let admission = Admission::default();
    let retried = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builders = &builders;
        let admission = &admission;
        RegistryBuilder::new(db, admission)?
            .seal()?
            .run(move |endpoint| async move {
                let builder = builders.allocate(&endpoint).await;
                let effects = RuntimePairs::new(db, endpoint, builder);
                admitted_pair_field(admission, effects.type_guard_return(type_guard), argument)
                    .await
            })
    })
    .0;
    assert!(matches!(retried, Ok(Ok(()))), "{retried:?}");
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnedRelationFixture {
    UnequalLiterals,
    Promotability,
    ReverseFailure,
}

fn owned_relation_fixture(db: &TestDb, fixture: OwnedRelationFixture) -> (Type<'_>, Type<'_>) {
    let (first, second) = match fixture {
        OwnedRelationFixture::UnequalLiterals => (
            Type::LiteralValue(LiteralValueType::promotable(1_i64)),
            Type::int_literal(2),
        ),
        OwnedRelationFixture::Promotability => (
            Type::LiteralValue(LiteralValueType::promotable(1_i64)),
            Type::LiteralValue(LiteralValueType::unpromotable(1_i64)),
        ),
        OwnedRelationFixture::ReverseFailure => (Type::Never, Type::int_literal(1)),
    };
    (
        TypeFormType::from_type_expression(db, first),
        TypeFormType::from_type_expression(db, second),
    )
}

fn ordinary_owned_direction_oracle<'db>(
    db: &'db TestDb,
    kind: OwnedRelationKind,
    source: Type<'db>,
    target: Type<'db>,
) -> (bool, Vec<(bool, bool)>) {
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let mut passes = Vec::new();
    let result = match kind {
        OwnedRelationKind::Assignability => {
            let value = owners
                .constraint_set_assignability()
                .check_type_pair(db, source, target);
            passes.push((false, value.is_trivially_always_satisfied()));
            value
        }
        OwnedRelationKind::Equivalence => {
            let checker = owners.constraint_set_equivalence();
            let forward_visitor = checker
                .materialization_visitor
                .for_new_materialization_root();
            let forward = checker
                .as_relation_checker(&forward_visitor)
                .check_type_pair(db, source, target);
            passes.push((false, forward.is_trivially_always_satisfied()));
            forward.and(db, &builder, || {
                let reverse_visitor = checker
                    .materialization_visitor
                    .for_new_materialization_root();
                let reverse = checker
                    .as_relation_checker(&reverse_visitor)
                    .check_type_pair(db, target, source);
                passes.push((true, reverse.is_trivially_always_satisfied()));
                reverse
            })
        }
    };
    (result.is_trivially_always_satisfied(), passes)
}

fn ordinary_owned_wrapper<'db>(
    db: &'db TestDb,
    env: &crate::ProgramEnvironment<'db>,
    kind: OwnedRelationKind,
    source: Type<'db>,
    target: Type<'db>,
) -> std::borrow::Cow<'db, OwnedConstraintSet<'db>> {
    match kind {
        OwnedRelationKind::Assignability => {
            source.when_constraint_set_assignable_to_owned(db, env, target)
        }
        OwnedRelationKind::Equivalence => {
            source.when_constraint_set_equivalent_to_owned(db, env, target)
        }
    }
}

#[test]
fn owned_query_producers_match_ordinary_directions_and_reuse_native_memos() {
    for kind in [
        OwnedRelationKind::Assignability,
        OwnedRelationKind::Equivalence,
    ] {
        for fixture in [
            OwnedRelationFixture::UnequalLiterals,
            OwnedRelationFixture::Promotability,
            OwnedRelationFixture::ReverseFailure,
        ] {
            let directional_db = setup_db();
            let (directional_source, directional_target) =
                owned_relation_fixture(&directional_db, fixture);
            let (expected, ordinary_passes) = ordinary_owned_direction_oracle(
                &directional_db,
                kind,
                directional_source,
                directional_target,
            );
            if fixture == OwnedRelationFixture::ReverseFailure {
                match kind {
                    OwnedRelationKind::Assignability => {
                        assert!(expected);
                        assert_eq!(ordinary_passes, [(false, true)]);
                    }
                    OwnedRelationKind::Equivalence => {
                        assert!(!expected);
                        assert_eq!(ordinary_passes, [(false, true), (true, false)]);
                    }
                }
            }

            let oracle_db = setup_db();
            let oracle_env = oracle_db.program_environment();
            let (oracle_source, oracle_target) = owned_relation_fixture(&oracle_db, fixture);
            assert_ne!(oracle_source, oracle_target);
            let mut oracle_reader = oracle_db.clone();
            oracle_reader.clear_salsa_events();
            let ordinary_capture = prepared_source_probe::capture(&oracle_db, || {
                let first = ordinary_owned_wrapper(
                    &oracle_db,
                    &oracle_env,
                    kind,
                    oracle_source,
                    oracle_target,
                );
                let second = ordinary_owned_wrapper(
                    &oracle_db,
                    &oracle_env,
                    kind,
                    oracle_source,
                    oracle_target,
                );
                assert!(matches!(first, std::borrow::Cow::Borrowed(_)));
                assert!(std::ptr::eq(first.as_ref(), second.as_ref()));
                assert_eq!(first.is_trivially_always_satisfied(), expected);
            })
            .expect("ordinary query starts outside an active query");
            let ordinary_events = oracle_reader.take_salsa_events();

            let db = setup_db();
            let env = db.program_environment();
            let program = env.program(&db);
            let (source, target) = owned_relation_fixture(&db, fixture);
            assert_ne!(source, target);
            let constructed = RefCell::new(Vec::new());
            let directions = RefCell::new(Vec::new());
            let packaged = Cell::new(0);
            let observer = |observation: OwnedRelationObservation<'_, '_, '_>| match observation {
                OwnedRelationObservation::Constructed {
                    kind: actual,
                    resources,
                    ..
                } => {
                    assert_eq!(actual, kind);
                    constructed.borrow_mut().push(resources);
                }
                OwnedRelationObservation::Direction {
                    source: actual_source,
                    target: actual_target,
                    resources,
                    relation,
                    evaluation,
                    materialization_guard,
                } => {
                    directions.borrow_mut().push((
                        (actual_source, actual_target) == (source, target),
                        (actual_source, actual_target) == (target, source),
                        resources,
                        relation,
                        evaluation,
                        materialization_guard,
                    ));
                }
                OwnedRelationObservation::PairResult(value) => {
                    assert_eq!(value.is_trivially_always_satisfied(), expected);
                    assert_eq!(value.is_trivially_never_satisfied(), !expected);
                }
                OwnedRelationObservation::Packaged(value) => {
                    assert_eq!(value.is_trivially_always_satisfied(), expected);
                    packaged.set(packaged.get() + 1);
                }
                OwnedRelationObservation::Initial | OwnedRelationObservation::Recovery => {
                    panic!("acyclic literal relation entered cycle callbacks")
                }
            };
            let admission = Admission::default();
            let native = Cell::new(None);
            let mut reader = db.clone();
            reader.clear_salsa_events();
            let runtime_capture = prepared_source_probe::capture(&db, || {
                expansion_probe::run(&db, usize::MAX, || {
                    let capacity = CallResourceCapacity {
                        calls: NonZeroUsize::new(8).unwrap(),
                    };
                    let environments = CallEnvironments::with_capacity(capacity);
                    let builders = CallBuilders::with_capacity(capacity);
                    let owners = CallRelationOwners::with_capacity(capacity);
                    let mappings = CallMappingVisitors::with_capacity(capacity);
                    let keys;
                    let queries;
                    let mut registry = RegistryBuilder::new(&db, &admission)?;
                    let assignability = owned_assignability_ingredient(&db);
                    let equivalence = owned_equivalence_ingredient(&db);
                    let assignability_route =
                        registry.reserve_callable(&db as &dyn Db, assignability)?;
                    let equivalence_route =
                        registry.reserve_callable(&db as &dyn Db, equivalence)?;
                    keys = register_type_pair_values(
                        &db,
                        &mut registry,
                        assignability,
                        equivalence,
                        redundancy_ingredient(&db),
                        possible_assignability_ingredient(&db),
                        union_from_two_elements_ingredient(&db),
                        intersection_from_two_elements_ingredient(&db),
                    )?;
                    queries = OwnedRelationQueries {
                        assignability: assignability_route,
                        equivalence: equivalence_route,
                        keys: &keys,
                    };
                    for (route_kind, route) in [
                        (OwnedRelationKind::Assignability, 0),
                        (OwnedRelationKind::Equivalence, 1),
                    ] {
                        let provider = OwnedRelationProvider {
                            kind: route_kind,
                            queries: NoProtocolQueries,
                            environments: &environments,
                            builders: &builders,
                            owners: &owners,
                            mappings: &mappings,
                            observer: Some(&observer),
                        };
                        if route == 0 {
                            registry.bind_callable(&queries.assignability, provider)?;
                        } else {
                            registry.bind_callable(&queries.equivalence, provider)?;
                        }
                    }
                    let db = &db;
                    let queries = &queries;
                    let native = &native;
                    registry.seal()?.run(move |endpoint| async move {
                        let shortcut = match kind {
                            OwnedRelationKind::Assignability => {
                                constraint_set::owned_assignable_with_queries_observed(
                                    db,
                                    &endpoint,
                                    program,
                                    source,
                                    source,
                                    queries.clone(),
                                    None,
                                )
                                .await?
                            }
                            OwnedRelationKind::Equivalence => {
                                constraint_set::owned_equivalent_observed(
                                    db,
                                    &endpoint,
                                    program,
                                    source,
                                    source,
                                    queries.clone(),
                                    None,
                                )
                                .await?
                            }
                        };
                        assert!(
                            matches!(shortcut, std::borrow::Cow::Owned(_))
                                && shortcut.is_trivially_always_satisfied()
                        );
                        for _ in 0..2 {
                            let value = match kind {
                                OwnedRelationKind::Assignability => {
                                    constraint_set::owned_assignable_with_queries_observed(
                                        db,
                                        &endpoint,
                                        program,
                                        source,
                                        target,
                                        queries.clone(),
                                        None,
                                    )
                                    .await?
                                }
                                OwnedRelationKind::Equivalence => {
                                    constraint_set::owned_equivalent_observed(
                                        db,
                                        &endpoint,
                                        program,
                                        source,
                                        target,
                                        queries.clone(),
                                        None,
                                    )
                                    .await?
                                }
                            };
                            assert!(matches!(value, std::borrow::Cow::Borrowed(_)));
                            assert_eq!(value.is_trivially_always_satisfied(), expected);
                            let address = std::ptr::from_ref(value.as_ref());
                            if let Some(first) = native.replace(Some(address)) {
                                assert_eq!(first, address);
                            }
                        }
                        Ok(())
                    })
                })
                .0
            })
            .expect("registered owned query starts outside an active query");
            assert!(
                matches!(runtime_capture.value, Ok(Ok(()))),
                "{:?}",
                runtime_capture.value
            );
            let runtime_events = reader.take_salsa_events();
            assert_eq!(constructed.borrow().len(), 1);
            assert_eq!(packaged.get(), 1);
            let directions = directions.borrow();
            assert_eq!(directions.len(), ordinary_passes.len());
            for (
                (actual_source, actual_target, resources, relation, evaluation, guard),
                (reverse, _),
            ) in directions.iter().zip(&ordinary_passes)
            {
                assert_eq!((*actual_source, *actual_target), (!*reverse, *reverse));
                assert_eq!(
                    *relation,
                    match kind {
                        OwnedRelationKind::Assignability => TypeRelation::Assignability,
                        OwnedRelationKind::Equivalence => TypeRelation::Redundancy { pure: true },
                    }
                );
                assert_eq!(*evaluation, TypeVarEvaluation::Lazy);
                assert_eq!(resources[..5], constructed.borrow()[0][..5]);
                if kind == OwnedRelationKind::Equivalence {
                    assert_ne!(resources[5], constructed.borrow()[0][5]);
                    assert!(guard.is_some());
                }
            }
            if directions.len() == 2 {
                assert_ne!(directions[0].2[5], directions[1].2[5]);
                assert_eq!(directions[0].5, directions[1].5);
            }
            let executions = |events: &[salsa::Event]| {
                events
                    .iter()
                    .filter(|event| matches!(event.kind, salsa::EventKind::WillExecute { .. }))
                    .count()
            };
            assert_eq!(runtime_capture.reads.len(), ordinary_capture.reads.len());
            assert_eq!(executions(&runtime_events), executions(&ordinary_events));
            eprintln!(
                "OWNED_DIRECTION_ORACLE kind={kind:?} fixture={fixture:?} ordinary_reads={} ordinary_executions={} passes={ordinary_passes:?}",
                ordinary_capture.reads.len(),
                executions(&ordinary_events)
            );
            let warm = ordinary_owned_wrapper(&db, &env, kind, source, target);
            assert_eq!(Some(std::ptr::from_ref(warm.as_ref())), native.get());
            assert!(!expansion_probe::active());
        }
    }
}

#[test]
fn pure_lazy_equivalence_preserves_promotability_and_rejects_other_redundancy_modes() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let owners = RelationOwners::new(&env, &builder);
    let (source, target) = owned_relation_fixture(&db, OwnedRelationFixture::Promotability);
    let mut checker = owners.constraint_set_assignability();
    checker.relation = TypeRelation::Redundancy { pure: false };
    let forward = checker.check_type_pair(&db, source, target);
    let reverse = checker.check_type_pair(&db, target, source);
    assert!(forward.is_trivially_always_satisfied());
    assert!(reverse.is_trivially_never_satisfied());
    for (relation, evaluation) in [
        (
            TypeRelation::Redundancy { pure: true },
            TypeVarEvaluation::Lazy,
        ),
        (
            TypeRelation::Redundancy { pure: true },
            TypeVarEvaluation::Eager,
        ),
        (
            TypeRelation::Redundancy { pure: false },
            TypeVarEvaluation::Lazy,
        ),
    ] {
        checker.relation = relation;
        checker.typevar_evaluation = evaluation;
        let admission = Admission::default();
        let outcome = expansion_probe::run(&db, usize::MAX, || {
            let db = &db;
            let checker = &checker;
            RegistryBuilder::new(db, &admission)?
                .seal()?
                .run(move |endpoint| async move {
                    let effects = RuntimePairs::new(db, endpoint, checker.constraints);
                    let actual = effects.check_type_pair(checker, target, source).await?;
                    Ok(actual.is_trivially_always_satisfied())
                })
        })
        .0;
        if relation == (TypeRelation::Redundancy { pure: true })
            && evaluation == TypeVarEvaluation::Lazy
        {
            assert!(matches!(outcome, Ok(Ok(true))), "{outcome:?}");
        } else {
            assert!(
                matches!(
                    outcome,
                    Err(Incomplete::UnsupportedPairOperation(
                        UnsupportedPairOperation::CheckerMode
                    ))
                ),
                "{outcome:?}"
            );
        }
    }
}

struct ObservedOwnedProvider<'run, 'db: 'run, C: Configuration> {
    inner: OwnedRelationProvider<'run, 'db, NoProtocolQueries>,
    route: CallableRoute<'run, 'db, C>,
    cycle: bool,
    previous: Cell<Option<bool>>,
    live: &'run Cell<usize>,
}

impl<C: Configuration> Drop for ObservedOwnedProvider<'_, '_, C> {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for ObservedOwnedProvider<'run, 'db, C>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
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
        <OwnedRelationProvider<'run, 'db, NoProtocolQueries> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::native_value(&self.inner, endpoint, db, operation)
        .await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        input: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        if self.cycle {
            // A self-edge exercises the registered query's callbacks; the actual relation
            // producer still computes and packages the result for the original operands.
            let seed = endpoint
                .child_call(|| async { endpoint.fetch_ref(&self.route, input.as_id())?.await })
                .await;
            assert_eq!(
                Some(seed.is_trivially_always_satisfied()),
                self.previous.get()
            );
        }
        <OwnedRelationProvider<'run, 'db, NoProtocolQueries> as CallableRouteProvider<
            'run,
            'db,
            C,
        >>::body(&self.inner, endpoint, db, input)
        .await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        let result =
            <OwnedRelationProvider<'run, 'db, NoProtocolQueries> as CallableRouteProvider<
                'run,
                'db,
                C,
            >>::initial(&self.inner, endpoint, db, id, input)
            .await?;
        assert!(result.is_trivially_always_satisfied());
        self.previous.set(Some(true));
        Ok(result)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call OwnedConstraintSet<'db>,
        value: OwnedConstraintSet<'db>,
        input: TypePair<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>>
    where
        'run: 'call,
    {
        assert!(cycle.head_ids().any(|id| id == cycle.id()));
        assert_eq!(cycle.id(), input.as_id());
        assert_eq!(
            Some(last.is_trivially_always_satisfied()),
            self.previous.get()
        );
        assert!(!value.is_trivially_always_satisfied());
        let result =
            <OwnedRelationProvider<'run, 'db, NoProtocolQueries> as CallableRouteProvider<
                'run,
                'db,
                C,
            >>::recover(&self.inner, endpoint, db, cycle, last, value, input)
            .await?;
        self.previous
            .set(Some(result.is_trivially_always_satisfied()));
        Ok(result)
    }
}

#[test]
fn both_owned_query_routes_use_the_original_seed_and_recovery_callbacks() {
    for kind in [
        OwnedRelationKind::Assignability,
        OwnedRelationKind::Equivalence,
    ] {
        let db = setup_db();
        let program = db.program_environment().program(&db);
        let (source, target) = owned_relation_fixture(&db, OwnedRelationFixture::UnequalLiterals);
        let initial = Cell::new(0);
        let recovery = Cell::new(0);
        let packed = Cell::new(0);
        let live = Cell::new(0);
        let observe = |observation: OwnedRelationObservation<'_, '_, '_>| match observation {
            OwnedRelationObservation::Initial => initial.set(initial.get() + 1),
            OwnedRelationObservation::Recovery => recovery.set(recovery.get() + 1),
            OwnedRelationObservation::Packaged(value) => {
                assert!(!value.is_trivially_always_satisfied());
                packed.set(packed.get() + 1);
            }
            _ => {}
        };
        let admission = Admission::default();
        let outcome = expansion_probe::run(&db, usize::MAX, || {
            let capacity = CallResourceCapacity {
                calls: NonZeroUsize::new(8).unwrap(),
            };
            let environments = CallEnvironments::with_capacity(capacity);
            let builders = CallBuilders::with_capacity(capacity);
            let owners = CallRelationOwners::with_capacity(capacity);
            let mappings = CallMappingVisitors::with_capacity(capacity);
            let keys;
            let queries;
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            let assignability = owned_assignability_ingredient(&db);
            let equivalence = owned_equivalence_ingredient(&db);
            let assignability_route = registry.reserve_callable(&db as &dyn Db, assignability)?;
            let equivalence_route = registry.reserve_callable(&db as &dyn Db, equivalence)?;
            keys = register_type_pair_values(
                &db,
                &mut registry,
                assignability,
                equivalence,
                redundancy_ingredient(&db),
                possible_assignability_ingredient(&db),
                union_from_two_elements_ingredient(&db),
                intersection_from_two_elements_ingredient(&db),
            )?;
            queries = OwnedRelationQueries {
                assignability: assignability_route,
                equivalence: equivalence_route,
                keys: &keys,
            };
            let provider = |route_kind| OwnedRelationProvider {
                kind: route_kind,
                queries: NoProtocolQueries,
                environments: &environments,
                builders: &builders,
                owners: &owners,
                mappings: &mappings,
                observer: Some(&observe),
            };
            live.set(2);
            registry.bind_callable(
                &queries.assignability,
                ObservedOwnedProvider {
                    inner: provider(OwnedRelationKind::Assignability),
                    route: queries.assignability.clone(),
                    cycle: kind == OwnedRelationKind::Assignability,
                    previous: Cell::new(None),
                    live: &live,
                },
            )?;
            registry.bind_callable(
                &queries.equivalence,
                ObservedOwnedProvider {
                    inner: provider(OwnedRelationKind::Equivalence),
                    route: queries.equivalence.clone(),
                    cycle: kind == OwnedRelationKind::Equivalence,
                    previous: Cell::new(None),
                    live: &live,
                },
            )?;
            let db = &db;
            let queries = &queries;
            registry.seal()?.run(move |endpoint| async move {
                let value = match kind {
                    OwnedRelationKind::Assignability => {
                        constraint_set::owned_assignable_with_queries_observed(
                            db,
                            &endpoint,
                            program,
                            source,
                            target,
                            queries.clone(),
                            None,
                        )
                        .await?
                    }
                    OwnedRelationKind::Equivalence => {
                        constraint_set::owned_equivalent_observed(
                            db,
                            &endpoint,
                            program,
                            source,
                            target,
                            queries.clone(),
                            None,
                        )
                        .await?
                    }
                };
                assert!(matches!(value, std::borrow::Cow::Borrowed(_)));
                Ok(value.is_trivially_always_satisfied())
            })
        })
        .0;
        assert!(matches!(outcome, Ok(Ok(false))), "{outcome:?}");
        assert!(initial.get() > 0 && recovery.get() > 0 && packed.get() > 0);
        assert_eq!(live.get(), 0);
    }
}

struct OwnedAcceptedChildAdmission<'run, 'db: 'run> {
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
    cleanup: &'run dyn Fn(),
    armed: &'run Cell<bool>,
    fired: Cell<bool>,
}

fn owned_relation_observer<'run, 'db: 'run, F>(observer: F) -> F
where
    F: for<'event> Fn(OwnedRelationObservation<'event, 'run, 'db>),
{
    observer
}

impl ExecutionAdmission for OwnedAcceptedChildAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !matches!(work, ExecutionWork::Work { .. })
            || !self.armed.get()
            || self.fired.replace(true)
        {
            return Ok(());
        }
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .ok_or(RunError::Contract(
                "owned completion has no active endpoint",
            ))?;
        let cleanup = QueuedCleanup(self.cleanup);
        *self.pending.borrow_mut() = Some(endpoint.demand(move || async move {
            let _held = cleanup;
            panic!("the rejected completion child must be drained without running")
        })?);
        Ok(())
    }
}

#[test]
fn owned_packaging_and_publication_reject_queued_children_before_retiring_resources() {
    for kind in [
        OwnedRelationKind::Assignability,
        OwnedRelationKind::Equivalence,
    ] {
        for publication in [false, true] {
            let db = setup_db();
            let env = db.program_environment();
            let program = env.program(&db);
            let (source, target) =
                owned_relation_fixture(&db, OwnedRelationFixture::UnequalLiterals);
            let retained_typevar = BoundTypeVarInstance::synthetic(
                &db,
                &env,
                Name::new_static("Retained"),
                TypeVarVariance::Invariant,
            );
            let retained_bound = Type::int_literal(17);
            let _ = (retained_typevar.identity(&db), retained_typevar.domain(&db));
            let live = Cell::new(0);
            let armed = Cell::new(false);
            let resources = Cell::new(None);
            let pair_seen = Cell::new(false);
            let packaged = Cell::new(false);
            let execution_error = Cell::new(None);
            let journal = RefCell::new(Vec::new());
            let cleanup_snapshot = Cell::new(None);
            let retained_before = Cell::new(None);
            let retained_cleanup = Cell::new(None);
            let capacity = CallResourceCapacity {
                calls: NonZeroUsize::new(8).unwrap(),
            };
            let environments = CallEnvironments::with_capacity(capacity);
            let builders = CallBuilders::with_capacity(capacity);
            let owners = CallRelationOwners::with_capacity(capacity);
            let mappings = CallMappingVisitors::with_capacity(capacity);
            let retained = Cell::new(None::<ConstraintSet<'_, '_>>);
            let cleanup = || {
                retained_cleanup.set(retained.get().and_then(|set| {
                    set.ownership_probe_single_equivalence(retained_typevar, retained_bound)
                }));
                cleanup_snapshot.set(Some((
                    live.get(),
                    resources.get(),
                    pair_seen.get(),
                    packaged.get(),
                )));
                journal.borrow_mut().push("child");
            };
            let keys = OnceCell::new();
            let queries = OnceCell::new();
            let admission;
            let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
            let pending = RefCell::new(None);
            admission = OwnedAcceptedChildAdmission {
                endpoint: &endpoint_slot,
                pending: &pending,
                cleanup: &cleanup,
                armed: &armed,
                fired: Cell::new(false),
            };
            let reset = ResetSlots {
                endpoint: &endpoint_slot,
                pending: &pending,
            };
            let observe = owned_relation_observer(|observation| match observation {
                OwnedRelationObservation::Constructed {
                    builder,
                    resources: actual,
                    ..
                } => {
                    resources.set(Some(actual));
                    assert_eq!(std::ptr::from_ref(builder).cast::<()>(), actual[1]);
                    // This unused constraint populates the producer's real private arenas. The
                    // producer still computes its result from the original TypeForm operands.
                    let set = ConstraintSet::constrain_typevar_equivalence_bound(
                        &db,
                        &env,
                        builder,
                        retained_typevar,
                        retained_bound,
                    );
                    let before =
                        set.ownership_probe_single_equivalence(retained_typevar, retained_bound);
                    assert!(
                        before.is_some_and(|(counts, _)| counts.into_iter().all(|count| count > 0)),
                        "retained storage before completion: {before:?}"
                    );
                    retained_before.set(before);
                    retained.set(Some(set));
                }
                OwnedRelationObservation::PairResult(value) => {
                    assert!(value.is_trivially_never_satisfied());
                    pair_seen.set(true);
                    if !publication {
                        armed.set(true);
                    }
                }
                OwnedRelationObservation::Packaged(value) => {
                    assert!(!value.is_trivially_always_satisfied());
                    packaged.set(true);
                    if publication {
                        armed.set(true);
                    }
                }
                _ => {}
            });
            let outcome = expansion_probe::run(&db, usize::MAX, || {
                let mut registry = RegistryBuilder::new(&db, &admission)?;
                let assignability = owned_assignability_ingredient(&db);
                let equivalence = owned_equivalence_ingredient(&db);
                let assignability_route =
                    registry.reserve_callable(&db as &dyn Db, assignability)?;
                let equivalence_route = registry.reserve_callable(&db as &dyn Db, equivalence)?;
                let keys = keys
                    .get_or_init(|| {
                        register_type_pair_values(
                            &db,
                            &mut registry,
                            assignability,
                            equivalence,
                            redundancy_ingredient(&db),
                            possible_assignability_ingredient(&db),
                            union_from_two_elements_ingredient(&db),
                            intersection_from_two_elements_ingredient(&db),
                        )
                    })
                    .as_ref()
                    .map_err(|error| *error)?;
                let queries = queries.get_or_init(|| OwnedRelationQueries {
                    assignability: assignability_route,
                    equivalence: equivalence_route,
                    keys,
                });
                let provider = |kind| OwnedRelationProvider {
                    kind,
                    queries: NoProtocolQueries,
                    environments: &environments,
                    builders: &builders,
                    owners: &owners,
                    mappings: &mappings,
                    observer: Some(&observe),
                };
                live.set(2);
                registry.bind_callable(
                    &queries.assignability,
                    ObservedOwnedProvider {
                        inner: provider(OwnedRelationKind::Assignability),
                        route: queries.assignability.clone(),
                        cycle: false,
                        previous: Cell::new(None),
                        live: &live,
                    },
                )?;
                registry.bind_callable(
                    &queries.equivalence,
                    ObservedOwnedProvider {
                        inner: provider(OwnedRelationKind::Equivalence),
                        route: queries.equivalence.clone(),
                        cycle: false,
                        previous: Cell::new(None),
                        live: &live,
                    },
                )?;
                let db = &db;
                let journal = &journal;
                let admission = &admission;
                let result = registry.seal()?.run(move |endpoint| {
                    **admission.endpoint.borrow_mut() = Some(endpoint.clone());
                    async move {
                        let _root = RootCleanup(journal);
                        let value = match kind {
                            OwnedRelationKind::Assignability => {
                                constraint_set::owned_assignable_with_queries_observed(
                                    db,
                                    &endpoint,
                                    program,
                                    source,
                                    target,
                                    queries.clone(),
                                    None,
                                )
                                .await?
                            }
                            OwnedRelationKind::Equivalence => {
                                constraint_set::owned_equivalent_observed(
                                    db,
                                    &endpoint,
                                    program,
                                    source,
                                    target,
                                    queries.clone(),
                                    None,
                                )
                                .await?
                            }
                        };
                        Ok(value.is_trivially_always_satisfied())
                    }
                });
                execution_error.set(result.as_ref().err().copied());
                result
            })
            .0;
            assert!(
                matches!(
                    outcome,
                    Err(Incomplete::Interrupted)
                        | Ok(Err(RunError::Contract("completed task retained a child")))
                ),
                "{outcome:?}"
            );
            assert_eq!(
                execution_error.get(),
                Some(RunError::Contract("completed task retained a child"))
            );
            let key = TypePair::new(&db, program, source, target);
            match kind {
                OwnedRelationKind::Assignability => assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        owned_assignability_ingredient(&db),
                        key.as_id()
                    )
                    .is_err()
                ),
                OwnedRelationKind::Equivalence => assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        owned_equivalence_ingredient(&db),
                        key.as_id()
                    )
                    .is_err()
                ),
            }
            assert!(admission.fired.get());
            assert!(resources.get().is_some());
            assert_eq!(
                cleanup_snapshot.get(),
                Some((2, resources.get(), true, publication))
            );
            assert!(retained_before.get().is_some());
            assert_eq!(retained_cleanup.get(), retained_before.get());
            assert_eq!(&*journal.borrow(), &["child", "root"]);
            // The admission fixture retains an endpoint, which owns the registered providers.
            assert_eq!(live.get(), 2);
            assert!(!expansion_probe::active());
            drop(reset);
            assert_eq!(live.get(), 0);
            assert!(endpoint_slot.borrow().is_none() && pending.borrow().is_none());
        }
    }
}

#[test]
fn owned_producer_packaging_refuses_actual_nonterminal_storage() {
    let db = setup_db();
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let variable = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let bound = TypeFormType::from_type_expression(&db, Type::int_literal(1));
    let set =
        ConstraintSet::constrain_typevar_equivalence_bound(&db, &env, &builder, variable, bound);
    assert!(set.to_owned_terminal().is_none());
    let before = set.display(&db, &env).to_string();
    let admission = Admission::default();
    let outcome = expansion_probe::run(&db, usize::MAX, || {
        let db = &db;
        let builder = &builder;
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                constraint_set::package_owned_result(db, &endpoint, builder, set).await
            })
    })
    .0;
    assert!(
        matches!(
            outcome,
            Err(Incomplete::UnsupportedPairOperation(
                UnsupportedPairOperation::OwnedCompaction
            ))
        ),
        "{outcome:?}"
    );
    assert_eq!(set.display(&db, &env).to_string(), before);
    assert!(set.to_owned_terminal().is_none());
}
