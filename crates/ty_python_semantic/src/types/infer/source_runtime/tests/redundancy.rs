use std::panic::AssertUnwindSafe;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use test_case::test_matrix;

use super::*;
use crate::FxOrderSet;
use crate::analysis::TruthinessOperation;
use crate::types::call::bind::source_check::condition_for_test;
use crate::types::class::KnownClassInstanceEffects;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::context::ProgramEnvironmentSource;
use crate::types::dedicated::pydantic::ConfigBoolean;
use crate::types::function::overloads_and_implementation_ingredient;
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedType, MethodWrapper, MethodWrapperKind,
    UnionTypeInstance,
};
use crate::types::mapping::source::MappingSourceEffects;
use crate::types::mapping::source::observations as canonical_materialization_observations;
use crate::types::relation::redundancy_ingredient;
use crate::types::relation::source::resources::observations as equivalence_observations;
use crate::types::relation::source::resources::{
    RelationResourceAccess,
    observations::{self as invocation_observations, InvocationStage},
};
use crate::types::relation::source::retained::{
    CheckerStorage, observations as retained_observations,
};
use crate::types::relation::source::signature_observations::{
    self, Event as SignatureEvent, Stage as SignatureStage,
};
use crate::types::relation::source::{
    RelationSourceEffects, equivalence_condition, guard_observations, materialization_observations,
    assignability_condition, redundancy_observations, subtyping_condition,
};
use crate::types::relation::{RelationOwners, TypeRelation, TypeVarEvaluation};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::set_theoretic::builder::intersection_insertion::Sign;
use crate::types::signatures::{ConcatenateTail, Parameter, Parameters, ParametersKind, Signature};
use crate::types::typevar::{TypeVarIdentity, TypeVarKind, TypeVarNonce, TypeVarSet};
use crate::types::visitor::{SearchOperation, TypeWalkFieldOperation};
use crate::types::{
    BindingContext, BoundTypeVarInstance, CallableType, KnownClass, KnownInstanceType, LiteralValueType,
    MappingOperation, MaterializationOperation, NegativeIntersectionElements, NominalInstanceType,
    RelationOperation, TypeGuardType, TypeVarInstance, TypeVarVariance, todo_type,
};

#[derive(Clone, Copy)]
enum Action<'db> {
    Redundancy(Type<'db>, Type<'db>),
    Membership(IntersectionType<'db>, Type<'db>, Sign),
    RetainedPair(Type<'db>, Type<'db>, bool),
    SignaturePair {
        source: Type<'db>,
        target: Type<'db>,
        expected: bool,
        relation: TypeRelation,
        typevars: TypeVarEvaluation,
    },
    FilePair {
        file: ProgramFile<'db>,
        source: Type<'db>,
        target: Type<'db>,
        expected: bool,
    },
    FreshSubtyping(Type<'db>, Type<'db>),
    FreshAssignability(Type<'db>, Type<'db>),
    NominalDefinitions {
        file: ProgramFile<'db>,
        source: Definition<'db>,
        target: Definition<'db>,
        expected: bool,
        cancel_at_guard: bool,
    },
    Equivalence(Type<'db>, Type<'db>),
    NominalGeneric(NominalInstanceType<'db>),
    NominalKnown(NominalInstanceType<'db>, Option<KnownClass>),
    AliasPreservingUnion {
        first: Type<'db>,
        second: Type<'db>,
        expected: Type<'db>,
    },
    Materialization {
        file: ProgramFile<'db>,
        ty: Type<'db>,
        kind: MaterializationKind,
        expected: Type<'db>,
    },
    Invocation {
        source: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
        expected: bool,
        disjoint_checks: usize,
    },
    MismatchedInvocation,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    first: Type<'db>,
    second: Type<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<bool>, AnalysisFailure> {
    controlled_action(
        prepared,
        Action::Redundancy(first, second),
        policy,
        &Cell::new(None),
    )
}

fn controlled_action<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    action: Action<'db>,
    policy: &AnalysisPolicy,
    remaining: &Cell<Option<usize>>,
) -> Result<AnalysisOutcome<bool>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        canonical_materialization_observations::reset(None);
        let env = match action {
            Action::Materialization { file, .. }
            | Action::FilePair { file, .. }
            | Action::NominalDefinitions { file, .. } => {
                ProgramEnvironment::from_file(file)
            }
            _ => ProgramEnvironment::from_program(session.program()),
        };
        let constraints = ConstraintSetBuilder::new();
        let retained_owners = RelationOwners::new(&env, &constraints);
        let retained_checkers = CheckerStorage::new();
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
        let retained_checkers = &retained_checkers;
        let retained_owners = &retained_owners;
        let constraints = &constraints;
        let env = &env;
        let result = run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let result = match action {
                Action::Redundancy(first, second) => {
                    access.is_redundant_with(first, second).await?
                }
                Action::RetainedPair(source, target, expected)
                | Action::FilePair { source, target, expected, .. }
                | Action::SignaturePair { source, target, expected, .. } => {
                    let effects = SourceEffects::new(&access, session.program());
                    let factory = access.endpoint.local_call(|| {
                        access.endpoint.admit_work(size_of_val(&access) * 2)?;
                        Ok(effects.retained_relation_source())
                    }).await;
                    let checker = retained_checkers
                        .allocate(&access.endpoint, || {
                            let mut checker = retained_owners.subtyping(TypeVarSet::None);
                            if let Action::SignaturePair { relation, typevars, .. } = action {
                                checker.relation = relation;
                                checker.typevar_evaluation = typevars;
                            }
                            checker
                        })
                        .await;
                    let result = checker.pair(session.db(), factory, source, target).await?;
                    assert!(result.ownership_probe_same_set(ConstraintSet::from_bool(constraints, expected)));
                    result.is_trivially_always_satisfied()
                }
                Action::FreshSubtyping(source, target) => {
                    let effects = SourceEffects::new(&access, session.program());
                    subtyping_condition(session.db(), env, source, target, &effects).await?
                }
                Action::FreshAssignability(source, target) => {
                    let effects = SourceEffects::new(&access, session.program());
                    assignability_condition(session.db(), env, source, target, &effects).await?
                }
                Action::NominalDefinitions { source, target, expected, cancel_at_guard, .. } => {
                    let source = controlled_definition_instance(&access, session.program(), source).await?;
                    let target = controlled_definition_instance(&access, session.program(), target).await?;
                    guard_observations::reset(cancel_at_guard);
                    retained_observations::reset(None);
                    let effects = SourceEffects::new(&access, session.program());
                    let factory = access.endpoint.local_call(|| {
                        access.endpoint.admit_work(size_of_val(&access) * 2)?;
                        Ok(effects.retained_relation_source())
                    }).await;
                    let checker = retained_checkers
                        .allocate(&access.endpoint, || {
                            retained_owners.subtyping(TypeVarSet::None)
                        })
                        .await;
                    let result = checker.pair(session.db(), factory, source, target).await?;
                    assert!(result.ownership_probe_same_set(ConstraintSet::from_bool(constraints, expected)));
                    result.is_trivially_always_satisfied()
                }
                Action::Equivalence(source, target) => {
                    let effects = SourceEffects::new(&access, session.program());
                    equivalence_condition(session.db(), env, source, target, &effects).await?
                }
                Action::NominalGeneric(instance) => {
                    SourceEffects::new(&access, session.program())
                        .nominal_is_definition_generic(instance)
                        .await?
                }
                Action::NominalKnown(instance, expected) => {
                    let effects = SourceEffects::new(&access, session.program());
                    assert_eq!(
                        RelationSourceEffects::nominal_known_class(&effects, instance).await?,
                        expected,
                    );
                    true
                }
                Action::AliasPreservingUnion { first, second, expected } => {
                    let effects = SourceEffects::new(&access, session.program());
                    let mut builder = MappingSourceEffects::new_union(&effects, env).await?;
                    MappingSourceEffects::union_add(&effects, &mut builder, first).await?;
                    MappingSourceEffects::union_add(&effects, &mut builder, second).await?;
                    let actual = MappingSourceEffects::finish_union(
                        &effects, builder, RecursivelyDefined::No,
                    ).await?;
                    assert_eq!(actual, expected);
                    true
                }
                Action::Materialization { file, ty, kind, expected } => {
                    assert!(matches!(env.source(), ProgramEnvironmentSource::File(source) if source == file));
                    let effects = SourceEffects::new(&access, session.program());
                    let actual = RelationSourceEffects::cached_materialization(
                        &effects, env, ty, kind,
                    ).await?;
                    assert_eq!(actual, expected);
                    true
                }
                Action::Invocation { source, target, inferable, expected, .. } => {
                    let effects = SourceEffects::new(&access, session.program());
                    let retained = resources.invocation_builder(&access.endpoint).await?;
                    for inferable in [TypeVarSet::None, inferable] {
                        for always in [true, false] {
                            let satisfied = condition_for_test(
                                session.db(),
                                env,
                                retained,
                                retained,
                                (source, target, inferable),
                                always,
                                &effects,
                            ).await?;
                            assert_eq!(satisfied, expected == always);
                        }
                    }
                    true
                }
                Action::MismatchedInvocation => {
                    let effects = SourceEffects::new(&access, session.program());
                    let retained = resources.invocation_builder(&access.endpoint).await?;
                    condition_for_test(
                        session.db(),
                        env,
                        retained,
                        constraints,
                        (Type::bool_literal(true), Type::AlwaysTruthy, TypeVarSet::None),
                        true,
                        &effects,
                    ).await?
                }
                Action::Membership(intersection, ty, sign) => {
                    let effects = SourceEffects::new(&access, session.program());
                    match sign {
                        Sign::Positive => {
                            effects
                                .intersection_positive_contains(intersection, ty)
                                .await?
                        }
                        Sign::Negative => {
                            effects
                                .intersection_negative_contains(intersection, ty)
                                .await?
                        }
                    }
                }
            };
            remaining.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                session.db(),
            ));
            Ok(result)
        });
        let expected_counts = match action {
            Action::FreshSubtyping(..) => Some([1, 1, 1, 0, 1]),
            Action::Equivalence(..) => {
                let directions = equivalence_observations::snapshot().count;
                Some([1, 1, 1, directions, directions])
            }
            Action::Invocation { disjoint_checks, .. } => Some([4, 1, 4, 0, 4 * (1 + disjoint_checks)]),
            Action::MismatchedInvocation => Some([0, 1, 0, 0, 0]),
            Action::Materialization { .. } => Some([0, 0, 0, 0, 0]),
            _ => None,
        };
        if let Some(mut expected_counts) = expected_counts {
            let materializations = canonical_materialization_observations::snapshot().root_count;
            expected_counts[0] += materializations;
            expected_counts[3] += materializations;
            let retained = [
                environments.retained_payload(),
                builders.retained_payload(),
                owners.retained_payload(),
                mapping.retained_payload(),
                checkers.retained_payload(),
            ]
            .map(Option::unwrap);
            let counts = retained.map(|(count, _)| count);
            let payloads = retained.map(|(_, bytes)| bytes);
            eprintln!("SOURCE_RESOURCES counts={counts:?} payload_capacity_bytes={payloads:?}");
            if result.is_ok() || matches!(action, Action::MismatchedInvocation) {
                assert_eq!(counts, expected_counts);
            }
        }
        result
    })
}

#[derive(Clone, Copy, Debug)]
enum FallbackLiteral {
    Bool,
    Int,
    LiteralString,
    Module,
    Function,
}

impl FallbackLiteral {
    fn class(self) -> KnownClass {
        match self {
            Self::Bool => KnownClass::Bool,
            Self::Int => KnownClass::Int,
            Self::LiteralString => KnownClass::Str,
            Self::Module => KnownClass::ModuleType,
            Self::Function => KnownClass::FunctionType,
        }
    }

    fn value<'db>(self, db: &'db TestDb, file: ProgramFile<'db>) -> Type<'db> {
        match self {
            Self::Bool => Type::bool_literal(true),
            Self::Int => Type::int_literal(37),
            Self::LiteralString => Type::literal_string(),
            Self::Module => {
                let ty = crate::place::global_symbol(db, file, "dependency")
                    .place
                    .expect_type();
                assert!(matches!(ty, Type::ModuleLiteral(_)));
                ty
            }
            Self::Function => {
                let ty = crate::place::global_symbol(db, file, "callback")
                    .place
                    .expect_type();
                assert!(matches!(ty, Type::FunctionLiteral(_)));
                ty
            }
        }
    }
}

fn literal_fallback_fixture() -> TestDb {
    let mut db = fixture();
    db.write_file("src/dependency.py", "").unwrap();
    db.write_file(
        "src/main.py",
        "import dependency\ndef callback(): ...\nleft = right = 1\n",
    )
    .unwrap();
    db
}

fn assert_literal_fallback_pairs(cases: &[FallbackLiteral]) {
    for &case in cases {
        let oracle = literal_fallback_fixture();
        let oracle_prepared = prepare(&oracle);
        let oracle_env = ProgramEnvironment::from_file(oracle_prepared.program_file());
        let oracle_source = case.value(&oracle, oracle_prepared.program_file());
        assert!(!oracle_source.is_subtype_of(
            &oracle,
            &oracle_env,
            TypeFormType::from_type_expression(&oracle, Type::Never),
        ));

        let db = literal_fallback_fixture();
        let prepared = prepare(&db);
        let source = case.value(&db, prepared.program_file());
        let target = TypeFormType::from_type_expression(&db, Type::Never);
        let action = Action::FilePair {
            file: prepared.program_file(),
            source,
            target,
            expected: false,
        };
        let revision = salsa::plumbing::current_revision(&db);
        let mut events = db.clone();
        assert_function_query_was_not_run_by_name(
            &db,
            "known_class_to_instance",
            None,
            &events.take_salsa_events(),
        );
        retained_observations::reset(None);
        let cold = capture(&db, || {
            controlled_action(&prepared, action, &funded(), &Cell::new(None))
        })
        .unwrap();
        assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(false)), "{case:?}");
        assert_eq!(cold.check_root_reads(), Ok(()));
        let (live, entered, polling) = retained_observations::progress();
        assert_eq!((live, polling), (0, 1));
        assert!(entered >= 2);
        let program = prepared.program_file().program(&db);
        let argument = KnownClassArgument::new(&db, case.class(), program);
        let ingredient = known_class_to_instance_ingredient(&db);
        let key = ingredient.database_key_index(argument.as_id());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
        let cold_address = cold
            .reads
            .iter()
            .find(|read| read.key == key)
            .map(|read| read.memo_address);
        assert!(cold_address.is_some(), "{case:?}");
        assert!(events.take_salsa_events().iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));

        let ordinary = capture(&db, || {
            case.class()
                .to_instance(&db, &ProgramEnvironment::from_program(program))
        })
        .unwrap();
        assert!(
            ordinary
                .reads
                .iter()
                .any(|read| { read.key == key && Some(read.memo_address) == cold_address })
        );
        retained_observations::reset(None);
        let warm = capture(&db, || {
            controlled_action(&prepared, action, &funded(), &Cell::new(None))
        })
        .unwrap();
        assert_eq!(warm.value, cold.value);
        assert!(
            warm.reads
                .iter()
                .any(|read| { read.key == key && Some(read.memo_address) == cold_address })
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "known_class_to_instance",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(retained_observations::progress(), (0, 2, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn literal_fallback_pairs_read_canonical_known_class_instances_from_the_file_environment() {
    assert_literal_fallback_pairs(&[FallbackLiteral::Bool, FallbackLiteral::Int]);
}

#[test]
fn module_literal_fallback_reads_its_canonical_known_class_instance() {
    assert_literal_fallback_pairs(&[FallbackLiteral::Module]);
}

#[test]
fn function_literal_fallback_reads_its_canonical_known_class_instance() {
    assert_literal_fallback_pairs(&[FallbackLiteral::Function]);
}

#[test]
fn literal_string_fallback_reads_its_canonical_known_class_instance() {
    assert_literal_fallback_pairs(&[FallbackLiteral::LiteralString]);
}

#[test]
fn enum_literal_fallback_keeps_its_named_refusal() {
    let mut db = fixture();
    db.write_dedented(
        "src/main.py",
        r#"
        from enum import Enum
        from typing import Literal

        class Choice(Enum):
            FIRST = 1
            SECOND = 2

        selected: Literal[Choice.FIRST] = Choice.FIRST
        left = right = 1
        "#,
    )
    .unwrap();
    let prepared = prepare(&db);
    let member = crate::place::global_symbol(&db, prepared.program_file(), "selected")
        .place
        .expect_type();
    assert!(member.as_enum_literal().is_some());
    assert_literal_fallback_refusal(
        &db,
        &prepared,
        member,
        OperationId::Relation(RelationOperation::LiteralFallbackEnumInstance),
    );
}

#[test]
fn overloaded_function_fallback_completes_and_reuses_canonical_metadata() {
    let mut db = fixture();
    db.write_dedented(
        "src/main.py",
        r#"
        from typing import overload

        @overload
        def callback(value: int) -> int: ...
        @overload
        def callback(value: str) -> str: ...
        def callback(value): return value

        left = right = 1
        "#,
    )
    .unwrap();
    let prepared = prepare(&db);
    let source = crate::place::global_symbol(&db, prepared.program_file(), "callback")
        .place
        .expect_type();
    let Type::FunctionLiteral(function) = source else {
        panic!("fixture callback is not a function literal");
    };
    let last_definition = function.literal(&db).last_definition;
    let ingredient = overloads_and_implementation_ingredient(&db);
    let key = ingredient.database_key_index(last_definition.as_id());
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, last_definition.as_id()).is_err());
    let target = TypeFormType::from_type_expression(&db, Type::Never);
    let action = Action::FilePair {
        file: prepared.program_file(),
        source,
        target,
        expected: false,
    };
    let revision = salsa::plumbing::current_revision(&db);
    let mut events = db.clone();
    events.take_salsa_events();
    retained_observations::reset(None);
    let cold = capture(&db, || {
        controlled_action(&prepared, action, &funded(), &Cell::new(None))
    })
    .unwrap();
    assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(false)));
    assert_eq!(cold.check_root_reads(), Ok(()));
    let (live, entered, polling) = retained_observations::progress();
    assert_eq!((live, polling), (0, 1));
    assert!(entered >= 2);
    assert_no_active_attempt();
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, last_definition.as_id()).is_ok());
    let cold_read = cold.reads.iter().find(|read| read.key == key).unwrap();
    assert!(events.take_salsa_events().iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
    }));

    let ordinary = capture(&db, || function.overloads_and_implementation(&db)).unwrap();
    assert_eq!(ordinary.value.0.len(), 2);
    assert_eq!(ordinary.value.1, Some(last_definition));
    assert!(ordinary.reads.iter().any(|read| {
        read.key == key
            && read.memo_address == cold_read.memo_address
            && read.stamp == cold_read.stamp
    }));
    assert!(!source.is_subtype_of(
        &db,
        &ProgramEnvironment::from_file(prepared.program_file()),
        target,
    ));
    retained_observations::reset(None);
    let warm = capture(&db, || {
        controlled_action(&prepared, action, &funded(), &Cell::new(None))
    })
    .unwrap();
    assert_eq!(warm.value, cold.value);
    assert_eq!(warm.check_root_reads(), Ok(()));
    assert!(warm.reads.iter().any(|read| {
        read.key == key
            && read.memo_address == cold_read.memo_address
            && read.stamp == cold_read.stamp
    }));
    let events = events.take_salsa_events();
    for query in [
        "overloads_and_implementation_inner",
        "known_class_to_instance",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(retained_observations::progress(), (0, entered, 1));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

fn assert_literal_fallback_refusal<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    source: Type<'db>,
    operation: OperationId,
) {
    let target = TypeFormType::from_type_expression(db, Type::Never);
    let revision = salsa::plumbing::current_revision(db);
    for _ in 0..2 {
        retained_observations::reset(None);
        let captured = capture(db, || {
            controlled_action(
                prepared,
                Action::FilePair {
                    file: prepared.program_file(),
                    source,
                    target,
                    expected: false,
                },
                &funded(),
                &Cell::new(None),
            )
        })
        .unwrap();
        assert_eq!(captured.value, Ok(unavailable(operation)));
        assert!(captured.reads.is_empty());
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(salsa::plumbing::current_revision(db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn completed_literal_fallback_child_survives_caller_refusal_and_cancellation() {
    assert_completed_literal_fallback_child_cleanup(FallbackLiteral::Bool);
}

#[test]
fn completed_function_fallback_child_survives_caller_refusal_and_cancellation() {
    assert_completed_literal_fallback_child_cleanup(FallbackLiteral::Function);
}

fn assert_completed_literal_fallback_child_cleanup(case: FallbackLiteral) {
    let measured = literal_fallback_fixture();
    let measured_prepared = prepare(&measured);
    let remaining = Cell::new(None);
    retained_observations::reset(None);
    assert_eq!(
        controlled_action(
            &measured_prepared,
            Action::FilePair {
                file: measured_prepared.program_file(),
                source: case.value(&measured, measured_prepared.program_file()),
                target: TypeFormType::from_type_expression(&measured, Type::Never),
                expected: false,
            },
            &funded(),
            &remaining,
        ),
        Ok(AnalysisOutcome::Complete(false)),
    );
    let completed_work = funded().semantic_work_limit - remaining.get().unwrap();
    let completed_entries = retained_observations::progress().1;
    assert!(completed_entries >= 2);
    for cancel in [false, true] {
        let db = literal_fallback_fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let action = Action::FilePair {
            file: prepared.program_file(),
            source: case.value(&db, prepared.program_file()),
            target: TypeFormType::from_type_expression(&db, Type::Never),
            expected: false,
        };
        retained_observations::reset(cancel.then_some(completed_entries));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: completed_work - 1,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                ..
            })) if !cancel => {}
            other => panic!("cancel={cancel}: {other:?}"),
        }
        let (live, entered, polling) = retained_observations::progress();
        assert_eq!((live, polling), (0, 1));
        assert!(entered >= 2);
        let argument =
            KnownClassArgument::new(&db, case.class(), prepared.program_file().program(&db));
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                known_class_to_instance_ingredient(&db),
                argument.as_id(),
            )
            .is_ok()
        );
        assert_no_active_attempt();

        let mut events = db.clone();
        events.take_salsa_events();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(false)),
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "known_class_to_instance",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(retained_observations::progress(), (0, 2, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn retained_recursive_pairs_keep_full_results_and_flat_polling() {
    recursive_pairs_keep_full_results_and_flat_polling(false);
}

#[test]
fn fresh_recursive_pairs_keep_flat_polling_and_one_root() {
    recursive_pairs_keep_full_results_and_flat_polling(true);
}

fn recursive_pairs_keep_full_results_and_flat_polling(fresh: bool) {
    for depth in [1, 8, 32] {
        for expected in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let revision = salsa::plumbing::current_revision(&db);
            let source = Type::bool_literal(expected);
            let mut target = Type::bool_literal(true);
            for _ in 0..depth {
                target =
                    Type::Intersection(signed_intersection(&db, Sign::Positive, &[target], &[]));
            }
            retained_observations::reset(None);
            let remaining = Cell::new(None);
            let action = if fresh {
                Action::FreshSubtyping(source, target)
            } else {
                Action::RetainedPair(source, target, expected)
            };
            assert_eq!(
                controlled_action(
                    &prepared,
                    action,
                    &funded(),
                    &remaining
                ),
                Ok(AnalysisOutcome::Complete(expected)),
                "depth={depth}"
            );
            assert_eq!(retained_observations::progress(), (0, depth + 1, 1));
            assert_eq!(source.is_subtype_of(&db, &env, target), expected);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
            eprintln!(
                "RETAINED_PAIR fresh={fresh} depth={depth} expected={expected} work={}",
                funded().semantic_work_limit - remaining.get().unwrap()
            );
        }
    }
}

#[test]
fn retained_pair_refusal_and_cancellation_drain_before_same_revision_retry() {
    pair_refusal_and_cancellation_drain_before_same_revision_retry(false);
}

#[test]
fn fresh_pair_refusal_and_cancellation_drain_before_same_revision_retry() {
    pair_refusal_and_cancellation_drain_before_same_revision_retry(true);
}

fn pair_refusal_and_cancellation_drain_before_same_revision_retry(fresh: bool) {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let source = Type::bool_literal(true);
    let mut target = source;
    for _ in 0..8 {
        target = Type::Intersection(signed_intersection(&db, Sign::Positive, &[target], &[]));
    }
    let action = if fresh {
        Action::FreshSubtyping(source, target)
    } else {
        Action::RetainedPair(source, target, true)
    };
    retained_observations::reset(None);
    let remaining = Cell::new(None);
    assert_eq!(
        controlled_action(&prepared, action, &funded(), &remaining),
        Ok(AnalysisOutcome::Complete(true))
    );
    let completed_work = funded().semantic_work_limit - remaining.get().unwrap();
    for cancel in [false, true] {
        retained_observations::reset(cancel.then_some(3));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: completed_work - completed_work / 4,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                ..
            })) if !cancel => {}
            other => panic!("cancel={cancel}: {other:?}"),
        }
        let (live, entered, poll_depth) = retained_observations::progress();
        assert_eq!(live, 0);
        assert!(entered > 1, "cancel={cancel}: entered={entered}");
        assert_eq!(poll_depth, 1);
        assert_no_active_attempt();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true))
        );
        assert_eq!(retained_observations::progress(), (0, 9, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn equivalence_preserves_direction_order_and_materialization_owners() {
    for (source, target, expected, directions) in [
        (Type::bool_literal(true), Type::Never, false, 1),
        (Type::Never, Type::bool_literal(true), false, 2),
        (Type::bool_literal(true), Type::bool_literal(true), true, 2),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        equivalence_observations::reset(None);
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Equivalence(source, target),
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_equivalence_resources(directions);
        assert_eq!(retained_observations::progress(), (0, directions, 1));
        assert_eq!(source.is_equivalent_to(&db, &env, target), expected);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

fn assert_equivalence_resources(directions: usize) {
    let snapshot = equivalence_observations::snapshot();
    assert_eq!(snapshot.count, directions);
    let forward = snapshot.directions[0].unwrap();
    assert!(forward.guard.is_some());
    if directions == 2 {
        let reverse = snapshot.directions[1].unwrap();
        assert_eq!(forward.builder, reverse.builder);
        assert_ne!(forward.visitor, reverse.visitor);
        assert_eq!(forward.guard, reverse.guard);
    } else {
        assert!(snapshot.directions[1].is_none());
    }
}

#[test]
fn equivalence_interruption_after_forward_direction_drains_before_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let value = Type::bool_literal(true);
    let action = Action::Equivalence(value, value);
    equivalence_observations::reset(None);
    retained_observations::reset(None);
    assert_eq!(
        controlled_action(&prepared, action, &funded(), &Cell::new(None)),
        Ok(AnalysisOutcome::Complete(true)),
    );
    assert_equivalence_resources(2);
    let reverse = equivalence_observations::snapshot().directions[1].unwrap();
    let reverse_work = funded().semantic_work_limit - reverse.remaining_work.unwrap();
    for cancel in [false, true] {
        equivalence_observations::reset(cancel.then_some(2));
        retained_observations::reset(None);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: reverse_work - 1,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                ..
            })) if !cancel => {}
            other => panic!("cancel={cancel}: {other:?}"),
        }
        assert_equivalence_resources(if cancel { 2 } else { 1 });
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_no_active_attempt();
        equivalence_observations::reset(None);
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_equivalence_resources(2);
        assert_eq!(retained_observations::progress(), (0, 2, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

fn signed_intersection<'db>(
    db: &'db dyn Db,
    sign: Sign,
    elements: &[Type<'db>],
    opposite: &[Type<'db>],
) -> IntersectionType<'db> {
    let (positive, negative) = match sign {
        Sign::Positive => (elements, opposite),
        Sign::Negative => (opposite, elements),
    };
    IntersectionType::new(
        db,
        FxOrderSet::from_iter(positive.iter().copied()),
        match negative {
            [] => NegativeIntersectionElements::Empty,
            [ty] => NegativeIntersectionElements::Single(*ty),
            _ => NegativeIntersectionElements::Multiple(FxOrderSet::from_iter(
                negative.iter().copied(),
            )),
        },
    )
}

#[test]
fn source_intersection_preserves_relation_and_original_builder() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let promotable = Type::bool_literal(true);
    let unpromotable = Type::LiteralValue(LiteralValueType::unpromotable(true));
    for (positive, negative, target, redundancy, subtyping, comparisons) in [
        (vec![promotable], vec![], Type::AlwaysTruthy, true, true, 2),
        (
            vec![Type::bool_literal(false)],
            vec![],
            Type::AlwaysTruthy,
            false,
            false,
            2,
        ),
        (
            vec![Type::unknown(), promotable],
            vec![],
            Type::AlwaysTruthy,
            true,
            true,
            3,
        ),
        (
            vec![promotable, Type::any()],
            vec![],
            Type::AlwaysTruthy,
            true,
            true,
            2,
        ),
        (
            vec![],
            vec![Type::bool_literal(false)],
            Type::Never,
            false,
            false,
            2,
        ),
        (vec![unpromotable], vec![], promotable, false, true, 2),
        (vec![promotable], vec![], unpromotable, true, true, 2),
    ] {
        let source = Type::Intersection(signed_intersection(
            &db,
            Sign::Positive,
            &positive,
            &negative,
        ));
        for canonical in [false, true] {
            let (action, expected) = if canonical {
                (Action::Redundancy(source, target), redundancy)
            } else {
                (Action::RetainedPair(source, target, subtyping), subtyping)
            };
            retained_observations::reset(None);
            redundancy_observations::reset(false);
            assert_eq!(
                controlled_action(&prepared, action, &funded(), &Cell::new(None)),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_eq!(retained_observations::progress(), (0, comparisons, 1));
            let ordinary = if canonical {
                let (live, entered, _) = redundancy_observations::progress();
                assert_eq!((live, entered), (0, 1));
                let pair = TypePair::new(&db, env.program(&db), source, target);
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        redundancy_ingredient(&db),
                        pair.as_id(),
                    )
                    .is_ok()
                );
                source.is_redundant_with(&db, &env, target)
            } else {
                source.is_subtype_of(&db, &env, target)
            };
            assert_eq!(ordinary, expected);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn recursive_source_intersections_keep_flat_polling_and_one_root() {
    for depth in [1, 8, 32] {
        for expected in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let revision = salsa::plumbing::current_revision(&db);
            let mut source = Type::bool_literal(expected);
            for _ in 0..depth {
                source =
                    Type::Intersection(signed_intersection(&db, Sign::Positive, &[source], &[]));
            }
            let target = Type::AlwaysTruthy;
            retained_observations::reset(None);
            assert_eq!(
                controlled_action(
                    &prepared,
                    Action::FreshSubtyping(source, target),
                    &funded(),
                    &Cell::new(None),
                ),
                Ok(AnalysisOutcome::Complete(expected)),
                "depth={depth}",
            );
            assert_eq!(retained_observations::progress(), (0, depth + 1, 1));
            assert_eq!(source.is_subtype_of(&db, &env, target), expected);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

/// Fresh assignability matches ordinary checks for gradual types and accepted or rejected literals.
#[test]
fn fresh_assignability_preserves_gradual_and_literal_results() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let one = Type::int_literal(1);
    assert!(!Type::any().is_subtype_of(&db, &env, one));
    for (source, target, expected) in [
        (Type::any(), one, true),
        (one, Type::any(), true),
        (Type::unknown(), one, true),
        (Type::Never, one, true),
        (one, one, true),
        (one, Type::int_literal(2), false),
    ] {
        assert_eq!(source.is_assignable_to(&db, &env, target), expected);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::FreshAssignability(source, target),
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(expected)),
            "{source:?} -> {target:?}",
        );
        assert_no_active_attempt();
    }
}

#[test]
fn source_intersection_refusal_and_cancellation_drain_before_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut source = Type::bool_literal(true);
    for _ in 0..8 {
        source = Type::Intersection(signed_intersection(&db, Sign::Positive, &[source], &[]));
    }
    for action in [
        Action::FreshSubtyping(source, Type::AlwaysTruthy),
        Action::FreshAssignability(source, Type::AlwaysTruthy),
    ] {
        retained_observations::reset(None);
        let remaining = Cell::new(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &remaining),
            Ok(AnalysisOutcome::Complete(true)),
        );
        let Some(remaining) = remaining.get() else {
            panic!("source intersection comparison did not complete");
        };
        let completed_work = funded().semantic_work_limit - remaining;
        assert_eq!(retained_observations::progress(), (0, 9, 1));
        assert_no_active_attempt();
        for cancel in [false, true] {
            retained_observations::reset(cancel.then_some(3));
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: completed_work - completed_work / 4,
                    ..funded()
                }
            };
            let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled_action(&prepared, action, &policy, &Cell::new(None))
            }));
            match outcome {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    ..
                })) if !cancel => {}
                other => panic!("cancel={cancel}: {other:?}"),
            }
            let (live, entered, poll_depth) = retained_observations::progress();
            assert_eq!(live, 0);
            assert!(entered > 1, "cancel={cancel}: entered={entered}");
            if cancel {
                assert_eq!(entered, 3);
            }
            assert_eq!(poll_depth, 1);
            assert_no_active_attempt();
            retained_observations::reset(None);
            assert_eq!(
                controlled_action(&prepared, action, &funded(), &Cell::new(None)),
                Ok(AnalysisOutcome::Complete(true)),
            );
            assert_eq!(retained_observations::progress(), (0, 9, 1));
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

fn invocation_builder_identity() -> usize {
    let snapshot = invocation_observations::invocation_snapshot();
    assert!(snapshot.count <= snapshot.events.len());
    let mut allocations = snapshot
        .events
        .iter()
        .flatten()
        .filter(|event| event.stage == InvocationStage::Allocated);
    let Some(allocation) = allocations.next() else {
        panic!("invocation did not allocate its builder");
    };
    assert!(allocations.next().is_none());
    for event in snapshot.events.iter().flatten() {
        assert_eq!(event.builder, allocation.builder);
    }
    allocation.builder
}

fn assert_invocation_assignability(inferable: TypeVarSet<'_>, expected: bool, depth: usize) {
    let builder = invocation_builder_identity();
    let snapshot = invocation_observations::assignability_snapshot();
    assert_eq!(snapshot.root_count, 4);
    assert_eq!(snapshot.pair_count, 4 * (depth + 1));
    assert_eq!(snapshot.result_count, 4);
    let inferable = match inferable {
        TypeVarSet::None => None,
        TypeVarSet::Some(inferable) => Some(inferable.as_id()),
    };
    let mut previous: Option<invocation_observations::AssignabilityPair> = None;
    for index in 0..4 {
        let Some(root) = snapshot.roots[index] else {
            panic!("assignability root {index} was not observed");
        };
        assert_eq!(root.identity.builder, builder);
        assert_eq!(root.identity.relation, TypeRelation::Assignability);
        assert_eq!(root.identity.typevars, TypeVarEvaluation::Eager);
        assert_eq!(
            root.identity.inferable,
            if index < 2 { None } else { inferable }
        );
        assert!(root.identity.given_is_original_never);
        assert!(root.identity.expensive);
        assert_eq!(root.identity.context, None);
        assert_eq!(root.always, index % 2 == 0);
        let pairs = &snapshot.pairs[index * (depth + 1)..(index + 1) * (depth + 1)];
        let Some(first) = pairs[0] else {
            panic!("assignability root {index} did not enter its retained checker");
        };
        for pair in pairs {
            let Some(pair) = pair else {
                panic!("assignability root {index} lost a queued child");
            };
            assert_eq!(pair.checker, first.checker);
            assert_eq!(pair.identity, root.identity);
        }
        if let Some(previous) = previous {
            assert_ne!(first.checker, previous.checker);
            assert_ne!(root.identity.environment, previous.identity.environment);
            for (visitor, previous) in root
                .identity
                .visitors
                .into_iter()
                .zip(previous.identity.visitors)
            {
                assert_ne!(visitor, previous);
            }
        }
        previous = Some(first);
        let Some(result) = snapshot.results[index] else {
            panic!("assignability root {index} did not return its full result");
        };
        assert_eq!(result.always, root.always);
        assert_eq!(result.terminal, Some(expected));
        assert!(result.original_terminal);
    }
}

#[test]
fn invocation_assignability_retains_one_builder_across_recursive_roots() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let typevar = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("T"),
        TypeVarVariance::Invariant,
    );
    let inferable = TypeVarSet::from_typevars(&db, [typevar]);
    let revision = salsa::plumbing::current_revision(&db);
    for expected in [false, true] {
        let mut source = Type::bool_literal(expected);
        for _ in 0..2 {
            source = Type::Intersection(signed_intersection(&db, Sign::Positive, &[source], &[]));
        }
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Invocation {
                    source,
                    target: Type::AlwaysTruthy,
                    inferable,
                    expected,
                    disjoint_checks: 0,
                },
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_invocation_assignability(inferable, expected, 2);
        assert_eq!(retained_observations::progress(), (0, 12, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// Fixed positional callable checks preserve ordinary default and parameter compatibility rules.
#[test]
fn invocation_fixed_callable_signatures_match_ordinary() {
    for (source_parameter, target_parameter, expected, children) in [
        (
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::AlwaysTruthy)
                .with_default_type(Type::bool_literal(true)),
            Parameter::positional_only(None).with_annotated_type(Type::bool_literal(true)),
            true,
            2,
        ),
        (
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::AlwaysTruthy),
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::bool_literal(true))
                .with_default_type(Type::bool_literal(true)),
            false,
            1,
        ),
        (
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::bool_literal(false)),
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::bool_literal(true)),
            false,
            2,
        ),
    ] {
        let ordinary = fixture();
        let controlled = fixture();
        for (db, run_controlled) in [(&ordinary, false), (&controlled, true)] {
            let prepared = prepare(db);
            let revision = salsa::plumbing::current_revision(db);
            let source = Type::Callable(CallableType::single(
                db,
                Signature::new(
                    Parameters::standard([source_parameter.clone()]),
                    Type::bool_literal(true),
                ),
            ));
            let target = Type::Callable(CallableType::single(
                db,
                Signature::new(
                    Parameters::standard([target_parameter.clone()]),
                    Type::AlwaysTruthy,
                ),
            ));
            assert_ne!(source, target);
            if run_controlled {
                invocation_observations::reset_invocations();
                invocation_observations::reset_assignability();
                retained_observations::reset(None);
                assert_eq!(
                    controlled_action(
                        &prepared,
                        Action::Invocation {
                            source,
                            target,
                            inferable: TypeVarSet::None,
                            expected,
                            disjoint_checks: 0,
                        },
                        &funded(),
                        &Cell::new(None),
                    ),
                    Ok(AnalysisOutcome::Complete(true)),
                );
                assert_invocation_assignability(TypeVarSet::None, expected, children);
                assert_eq!(
                    retained_observations::progress(),
                    (0, 4 * (children + 1), 1)
                );
            } else {
                let env = ProgramEnvironment::from_file(prepared.program_file());
                assert_eq!(source.is_assignable_to(db, &env, target), expected);
            }
            assert_eq!(salsa::plumbing::current_revision(db), revision);
            assert_no_active_attempt();
        }
    }
}

/// Supplies signature pairs for controlled normalization, prefix iteration, and relation-mode checks.
fn gradual_signature_cases(
    db: &TestDb,
    relation: TypeRelation,
    typevars: TypeVarEvaluation,
) -> Vec<(Signature<'_>, Signature<'_>)> {
    let positional = || {
        Parameter::positional_only(None).with_annotated_type(Type::bool_literal(true))
    };
    let fixed = Parameters::standard([positional()]);
    let optional = Parameters::standard([positional().with_default_type(Type::bool_literal(true))]);
    let named = Parameters::standard([
        Parameter::positional_or_keyword(Name::new_static("value"))
            .with_annotated_type(Type::AlwaysTruthy),
    ]);
    let concatenate = Parameters::concatenate(db, vec![positional()], ConcatenateTail::Gradual);
    let longer = Parameters::concatenate(
        db,
        vec![positional(), positional()],
        ConcatenateTail::Gradual,
    );
    let empty_concatenate = Parameters::concatenate(db, Vec::new(), ConcatenateTail::Gradual);
    // Type transformations preserve the stored gradual kind even when tail annotations become
    // concrete. These lists exercise that provenance independently of dynamic annotations.
    let static_gradual = Parameters::with_kind_for_test(
        [
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::AlwaysTruthy),
            Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object()),
            Parameter::keyword_variadic(Name::new_static("kwargs"))
                .with_annotated_type(Type::object()),
        ],
        ParametersKind::Gradual,
    );
    let other_static_gradual = Parameters::with_kind_for_test(
        [
            Parameter::positional_or_keyword(Name::new_static("other"))
                .with_annotated_type(Type::AlwaysTruthy),
            Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object()),
            Parameter::keyword_variadic(Name::new_static("kwargs"))
                .with_annotated_type(Type::object()),
        ],
        ParametersKind::Gradual,
    );
    let mut pairs = vec![
        (Parameters::top(), Parameters::top()),
        (Parameters::top(), Parameters::gradual_form()),
        (Parameters::top(), fixed.clone()),
        (Parameters::gradual_form(), fixed.clone()),
        (concatenate.clone(), longer),
        (concatenate.clone(), fixed),
        (static_gradual.clone(), concatenate.clone()),
    ];
    // The preceding pairs cover mode-dependent dispatch. These additional iterator exits and
    // metadata branches need one representative mode because their operations are shared.
    if relation == TypeRelation::Assignability && typevars == TypeVarEvaluation::Eager {
        pairs.extend([
            (Parameters::gradual_form(), concatenate.clone()),
            (concatenate.clone(), Parameters::standard([positional(), positional()])),
            (concatenate.clone(), optional),
            (concatenate.clone(), named),
            (concatenate, Parameters::empty()),
            (empty_concatenate, Parameters::empty()),
            (static_gradual.clone(), static_gradual.clone()),
            (static_gradual, other_static_gradual),
        ]);
    }
    let mut cases = Vec::new();
    for (source, target) in pairs {
        let reverse = (source != target).then(|| (target.clone(), source.clone()));
        for (source, target) in std::iter::once((source, target)).chain(reverse) {
            cases.push((
                Signature::new(source, Type::bool_literal(true)),
                Signature::new(target, Type::AlwaysTruthy),
            ));
        }
    }
    if relation == TypeRelation::Assignability && typevars == TypeVarEvaluation::Eager {
        cases.push((
            Signature::new(Parameters::gradual_form(), Type::bool_literal(false)),
            Signature::new(Parameters::top(), Type::AlwaysTruthy),
        ));
    }
    cases
}

/// Controlled Top and gradual comparisons preserve the ordinary checker's result, relation mode,
/// type-variable evaluation mode, and original constraint builder.
#[test_matrix(
    [
        TypeRelation::Assignability,
        TypeRelation::Subtyping,
        TypeRelation::SubtypingAssuming,
        TypeRelation::Redundancy { pure: false },
        TypeRelation::Redundancy { pure: true },
    ],
    [TypeVarEvaluation::Eager, TypeVarEvaluation::Lazy]
)]
fn gradual_signature_modes_preserve_ordinary_results(
    relation: TypeRelation,
    typevars: TypeVarEvaluation,
) {
            let ordinary = fixture();
            let ordinary_prepared = prepare(&ordinary);
            let ordinary_env = ProgramEnvironment::from_program(ordinary_prepared.program_file().program(&ordinary));
            let db = fixture();
            let prepared = prepare(&db);
            let revision = salsa::plumbing::current_revision(&db);
            for ((source, target), (controlled_source, controlled_target)) in
                gradual_signature_cases(&ordinary, relation, typevars)
                    .into_iter()
                    .zip(gradual_signature_cases(&db, relation, typevars))
            {
                let constraints = ConstraintSetBuilder::new();
                let owners = RelationOwners::new(&ordinary_env, &constraints);
                let mut checker = owners.subtyping(TypeVarSet::None);
                checker.relation = relation;
                checker.typevar_evaluation = typevars;
                let expected = checker.check_type_pair(
                    &ordinary,
                    Type::Callable(CallableType::single(&ordinary, source)),
                    Type::Callable(CallableType::single(&ordinary, target)),
                );
                assert!(expected.is_trivially_always_satisfied() || expected.is_trivially_never_satisfied());
                let expected = expected.is_trivially_always_satisfied();
                let source = Type::Callable(CallableType::single(&db, controlled_source));
                let target = Type::Callable(CallableType::single(&db, controlled_target));
                assert_ne!(source, target);
                retained_observations::reset(None);
                assert_eq!(
                    controlled_action(
                        &prepared,
                        Action::SignaturePair { source, target, expected, relation, typevars },
                        &funded(),
                        &Cell::new(None),
                    ),
                    Ok(AnalysisOutcome::Complete(expected)),
                    "{relation:?}, {typevars:?}, {source:?}, {target:?}",
                );
                let (live, entered, polling) = retained_observations::progress();
                assert_eq!((live, polling), (0, 1));
                assert!(entered > 1);
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
                assert_no_active_attempt();
            }
}

/// Unsupported signature capabilities refuse before parameter or return comparison, even when
/// their annotations or stored gradual kind would otherwise allow the relation.
#[test]
fn invocation_callable_signature_gates_precede_type_relations() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let declaration = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(&db, Name::new_static("P"), None, TypeVarKind::Pep695ParamSpec),
        None,
        Some(TypeVarVariance::Invariant),
        None,
    );
    let paramspec = BoundTypeVarInstance::new(
        &db,
        declaration,
        BindingContext::Synthetic(env.program(&db)),
        None,
        TypeVarNonce::NONE,
    );
    let target = Type::Callable(CallableType::single(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::bool_literal(true))
            ]),
            Type::AlwaysTruthy,
        ),
    ));
    let args = Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::any());
    let kwargs = Parameter::keyword_variadic(Name::new_static("kwargs"))
        .with_annotated_type(Type::any());
    for (parameters, operation) in [
        (
            Parameters::standard([
                Parameter::keyword_only(Name::new_static("value"))
                    .with_annotated_type(Type::AlwaysTruthy),
            ]),
            RelationOperation::SignatureKeywordParameters,
        ),
        (
            Parameters::standard([args.clone(), kwargs.clone()]),
            RelationOperation::SignatureVariadic,
        ),
        (
            Parameters::with_kind_for_test(
                [args.clone(), Parameter::positional_only(None), kwargs.clone()],
                ParametersKind::Gradual,
            ),
            RelationOperation::SignatureVariadic,
        ),
        (
            Parameters::with_kind_for_test(
                [args.clone().with_starred_annotation(), kwargs.clone()],
                ParametersKind::Gradual,
            ),
            RelationOperation::SignatureVariadic,
        ),
        (
            Parameters::with_kind_for_test(
                [
                    args,
                    Parameter::keyword_only(Name::new_static("value")),
                    kwargs,
                ],
                ParametersKind::Gradual,
            ),
            RelationOperation::SignatureKeywordParameters,
        ),
        (Parameters::paramspec(&db, paramspec), RelationOperation::SignatureVariadic),
        (
            Parameters::concatenate(
                &db,
                vec![Parameter::positional_only(None)],
                ConcatenateTail::ParamSpec(paramspec),
            ),
            RelationOperation::SignatureVariadic,
        ),
    ] {
        let unsupported = Type::Callable(CallableType::single(
            &db,
            Signature::new(parameters, Type::bool_literal(true)),
        ));
        for (source, target) in [(unsupported, target), (target, unsupported)] {
        assert_ne!(source, target);
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Invocation {
                    source,
                    target,
                    inferable: TypeVarSet::None,
                    expected: true,
                    disjoint_checks: 0,
                },
                &funded(),
                &Cell::new(None),
            ),
            Ok(unavailable(OperationId::Relation(operation))),
        );
        invocation_builder_identity();
        let snapshot = invocation_observations::assignability_snapshot();
        assert_eq!(
            (
                snapshot.root_count,
                snapshot.pair_count,
                snapshot.result_count
            ),
            (1, 1, 0),
        );
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
        }
    }
}

/// A controlled normalization checks both variadic annotations for aliases before its dynamic-tail exit.
/// An alias in either position therefore refuses without starting the return comparison.
#[test]
fn gradual_signature_alias_normalization_preserves_order() {
    let mut db = fixture();
    db.write_file("src/main.py", "type Alias = int\n").unwrap();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
        crate::place::global_symbol(&db, prepared.program_file(), "Alias")
            .place
            .expect_type()
    else {
        panic!("expected a type alias");
    };
    let alias = Type::TypeAlias(alias);
    let parameters = |annotation| {
        Parameters::with_kind_for_test(
            [
                Parameter::variadic(Name::new_static("args")).with_annotated_type(annotation),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::any()),
            ],
            ParametersKind::Gradual,
        )
    };
    for (source, target) in [(Type::any(), alias), (alias, Type::any())] {
        let source = Type::Callable(CallableType::single(
            &db,
            Signature::new(parameters(source), Type::bool_literal(true)),
        ));
        let target = Type::Callable(CallableType::single(
            &db,
            Signature::new(parameters(target), Type::AlwaysTruthy),
        ));
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Invocation {
                    source,
                    target,
                    inferable: TypeVarSet::None,
                    expected: true,
                    disjoint_checks: 0,
                },
                &funded(),
                &Cell::new(None),
            ),
            Ok(unavailable(OperationId::Relation(RelationOperation::SignatureAlias))),
        );
        invocation_builder_identity();
        let snapshot = invocation_observations::assignability_snapshot();
        assert_eq!((snapshot.root_count, snapshot.pair_count, snapshot.result_count), (1, 1, 0));
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// Cancelling a callable's return or gradual-prefix comparison after an actual `Pending` retires its
/// child futures before releasing the signature's parameter handles and permits a successful retry in
/// the same revision. Each attempt preserves its caller's checker and builder.
#[test]
fn invocation_callable_child_cancellation_preserves_checker_and_builder() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let parameter = Parameter::positional_only(Some(Name::new_static("value")))
        .with_annotated_type(Type::AlwaysTruthy);
    let accepted = Type::Union(UnionType::new(
        &db,
        vec![Type::AlwaysFalsy, Type::bool_literal(true)].into_boxed_slice(),
        RecursivelyDefined::No,
    ));
    // Child 1 is the outer callable comparison. The selected child checks `Literal[False]` against
    // `AlwaysFalsy | Literal[True]`.
    // It returns `Pending` while its first union-member comparison is queued. The first fixture
    // selects the return relation (child 2); the second selects the contravariant gradual-prefix relation
    // (child 3), after its scalar return relation completes.
    for (parameters, source_return, target_return, target_parameter, cancel_at) in [
        (
            Parameters::standard([parameter.clone()]),
            Type::bool_literal(false),
            accepted,
            Type::bool_literal(true),
            2,
        ),
        (
            Parameters::concatenate(
                &db,
                vec![parameter.with_annotated_type(accepted)],
                ConcatenateTail::Gradual,
            ),
            Type::bool_literal(true),
            Type::AlwaysTruthy,
            Type::bool_literal(false),
            3,
        ),
    ] {
    let source = Type::Callable(CallableType::single(
        &db,
        Signature::new(
            parameters,
            source_return,
        ),
    ));
    let target = Type::Callable(CallableType::single(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(target_parameter)
            ]),
            target_return,
        ),
    ));
    assert_ne!(source, target);
    let action = Action::Invocation {
        source,
        target,
        inferable: TypeVarSet::None,
        expected: true,
        disjoint_checks: 0,
    };
    invocation_observations::reset_invocations();
    invocation_observations::reset_assignability();
    retained_observations::reset(None);
    retained_observations::set_cancel_at_pending(Some(cancel_at));
    signature_observations::reset(None);
    let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_action(&prepared, action, &funded(), &Cell::new(None))
    }));
    signature_observations::stop();
    assert!(matches!(outcome, Err(salsa::Cancelled::Local)));
    let signature = signature_observations::snapshot();
    assert_gradual_signature_storage_retired(&signature);
    let events = &signature.events[..signature.count];
    let normalized = events
        .iter()
        .position(|event| matches!(event, Some(SignatureEvent::StorageNormalized { .. })))
        .expect("signature parameters were not normalized");
    let entered = events
        .iter()
        .position(|event| *event == Some(SignatureEvent::Child(
            retained_observations::Event::Entered(cancel_at),
        )))
        .expect("selected comparison did not enter");
    let pending = events
        .iter()
        .position(|event| *event == Some(SignatureEvent::Child(
            retained_observations::Event::Pending(cancel_at),
        )))
        .expect("selected comparison did not suspend");
    let retired = events
        .iter()
        .position(|event| *event == Some(SignatureEvent::Child(
            retained_observations::Event::Retired(cancel_at),
        )))
        .expect("selected comparison did not retire");
    assert!(normalized < entered && entered < pending && pending < retired);
    let prefix = events.iter().position(|event| matches!(
        event,
        Some(SignatureEvent::Boundary { stage: SignatureStage::BeforePrefix, .. }),
    ));
    if cancel_at == 2 {
        assert_eq!(prefix, None);
    } else {
        assert!(prefix.is_some_and(|prefix| prefix < entered));
    }
    let builder = invocation_builder_identity();
    let snapshot = invocation_observations::assignability_snapshot();
    assert_eq!(
        (
            snapshot.root_count,
            snapshot.pair_count,
            snapshot.result_count
        ),
        (1, cancel_at, 0),
    );
    let Some(root) = snapshot.roots[0] else {
        panic!("callable comparison did not enter its assignability root");
    };
    assert_eq!(root.identity.builder, builder);
    let Some(parent) = snapshot.pairs[0] else {
        panic!("callable comparison did not enter its retained checker");
    };
    assert_eq!(parent.identity, root.identity);
    for child in &snapshot.pairs[1..cancel_at] {
        let Some(child) = child else {
            panic!("callable comparison lost a relation child");
        };
        assert_eq!(parent.checker, child.checker);
        assert_eq!(child.identity, root.identity);
    }
    assert_eq!(retained_observations::progress(), (0, cancel_at, 1));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();

    invocation_observations::reset_invocations();
    invocation_observations::reset_assignability();
    retained_observations::reset(None);
    assert_eq!(
        controlled_action(&prepared, action, &funded(), &Cell::new(None)),
        Ok(AnalysisOutcome::Complete(true)),
    );
    assert_invocation_assignability(TypeVarSet::None, true, 3);
    assert_eq!(retained_observations::progress(), (0, 16, 1));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    }
}

/// Checks that relation children retire before the signature frame releases its parameter handles,
/// restoring the original storage strong counts.
fn assert_gradual_signature_storage_retired(snapshot: &signature_observations::Snapshot) {
    assert!(!snapshot.overflowed, "{snapshot:?}");
    assert_eq!(snapshot.active_scopes, 0);
    assert_eq!(snapshot.count, snapshot.events.iter().flatten().count());
    let mut original_counts = None;
    let mut children = Vec::new();
    for event in snapshot.events.iter().flatten() {
        match *event {
            SignatureEvent::StorageStarted { source, target } => {
                assert!(original_counts.replace((source, target)).is_none());
            }
            SignatureEvent::StorageNormalized { source, target } => {
                let Some((original_source, original_target)) = original_counts else {
                    panic!("normalization did not retain its parameter storage");
                };
                assert!(source > original_source && target > original_target);
            }
            SignatureEvent::StorageRetired { source, target } => {
                assert!(children.is_empty(), "{snapshot:?}");
                assert_eq!(original_counts.take(), Some((source, target)));
            }
            SignatureEvent::Child(retained_observations::Event::Entered(child)) => {
                if original_counts.is_some() {
                    children.push(child);
                }
            }
            SignatureEvent::Child(retained_observations::Event::Retired(child)) => {
                children.retain(|entered| *entered != child);
            }
            SignatureEvent::Boundary { .. }
            | SignatureEvent::Child(retained_observations::Event::Pending(_)) => {}
        }
    }
    assert!(original_counts.is_none());
    assert!(children.is_empty());
}

/// Returns the work remaining when an attempt first reaches a particular signature boundary.
/// These tests record each selected boundary in a running budgeted attempt before applying its
/// interruption, so a reached boundary has a work value.
fn gradual_signature_boundary_work(
    snapshot: &signature_observations::Snapshot,
    target: SignatureStage,
) -> Option<usize> {
    snapshot.events.iter().flatten().find_map(|event| match *event {
        SignatureEvent::Boundary { stage, remaining_work } if stage == target => remaining_work,
        SignatureEvent::Boundary { .. }
        | SignatureEvent::StorageStarted { .. }
        | SignatureEvent::StorageNormalized { .. }
        | SignatureEvent::StorageRetired { .. }
        | SignatureEvent::Child(_) => None,
    })
}

/// Real work and byte limits, completion refusal, and cancellation stop signature transfers
/// without leaking parameter handles, and a funded retry succeeds in the same revision. The
/// canonical redundancy case additionally verifies that completion refusal publishes no memo.
#[test]
fn gradual_signature_transfer_interruption_drains_and_retries() {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Stop {
        None,
        Cancel(SignatureStage),
        Incomplete(SignatureStage),
        Pending(usize),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Execution {
        Retained,
        Canonical,
    }

    /// Runs a controlled comparison and checks that children retire before its parameter handles
    /// are released, restoring the original strong counts. Then retries with a funded budget in
    /// the same revision.
    /// The contravariant prefix comparison checks `Literal[False]` against
    /// `AlwaysFalsy | Literal[True]`. It suspends while a queued child checks the first union member,
    /// keeping the signature's parameter handles alive until that child drains.
    fn run(
        policy: AnalysisPolicy,
        stop: Stop,
        execution: Execution,
    ) -> (signature_observations::Snapshot, Option<AnalysisIncomplete>) {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let parameters = |annotation| {
            Parameters::with_kind_for_test(
                [
                    Parameter::positional_only(None).with_annotated_type(annotation),
                    Parameter::variadic(Name::new_static("args"))
                        .with_annotated_type(Type::object()),
                    Parameter::keyword_variadic(Name::new_static("kwargs"))
                        .with_annotated_type(Type::object()),
                ],
                ParametersKind::Concatenate(ConcatenateTail::Gradual),
            )
        };
        let accepted_prefix = Type::Union(UnionType::new(
            &db,
            vec![Type::AlwaysFalsy, Type::bool_literal(true)].into_boxed_slice(),
            RecursivelyDefined::No,
        ));
        let source = Type::Callable(CallableType::single(
            &db,
            Signature::new(parameters(accepted_prefix), Type::bool_literal(true)),
        ));
        let target = Type::Callable(CallableType::single(
            &db,
            Signature::new(parameters(Type::bool_literal(false)), Type::AlwaysTruthy),
        ));
        let action = match execution {
            Execution::Retained => Action::SignaturePair {
                source,
                target,
                expected: true,
                relation: TypeRelation::Assignability,
                typevars: TypeVarEvaluation::Eager,
            },
            Execution::Canonical => Action::Redundancy(source, target),
        };
        retained_observations::reset(None);
        signature_observations::reset(match stop {
            Stop::Cancel(stage) => Some(stage),
            Stop::None | Stop::Incomplete(_) | Stop::Pending(_) => None,
        });
        match stop {
            Stop::Incomplete(stage) => signature_observations::set_incomplete_at(Some(stage)),
            Stop::Pending(child) => retained_observations::set_cancel_at_pending(Some(child)),
            Stop::None | Stop::Cancel(_) => {}
        }
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        signature_observations::stop();
        let snapshot = signature_observations::snapshot();
        assert_gradual_signature_storage_retired(&snapshot);
        let children = retained_observations::snapshot();
        assert!(!children.overflowed);
        assert_eq!(retained_observations::progress().0, 0);
        let reason = match result {
            Err(salsa::Cancelled::Local) if matches!(stop, Stop::Cancel(_) | Stop::Pending(_)) => None,
            Ok(Ok(AnalysisOutcome::Complete(true))) if stop == Stop::None => None,
            Ok(Ok(AnalysisOutcome::Incomplete { reason, completed: () }))
                if matches!(reason, AnalysisIncomplete::WorkLimit | AnalysisIncomplete::RequestedAllocationLimit) =>
            {
                Some(reason)
            }
            other => panic!("{stop:?}, {execution:?}: {other:?}"),
        };
        if let Stop::Pending(child) = stop {
            assert!(snapshot.events.contains(&Some(SignatureEvent::Child(
                retained_observations::Event::Pending(child),
            ))));
            assert!(snapshot.events.contains(&Some(SignatureEvent::Child(
                retained_observations::Event::Retired(child),
            ))));
        }
        if execution == Execution::Canonical && reason.is_some() {
            let pair = TypePair::new(&db, prepared.program_file().program(&db), source, target);
            assert!(FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id()).is_err());
        }
        assert_no_active_attempt();
        retained_observations::reset(None);
        signature_observations::reset(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true)),
        );
        signature_observations::stop();
        assert_gradual_signature_storage_retired(&signature_observations::snapshot());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
        (snapshot, reason)
    }

    let (measured, reason) = run(funded(), Stop::None, Execution::Retained);
    assert_eq!(reason, None);
    assert!(measured.events.contains(&Some(SignatureEvent::Child(
        retained_observations::Event::Pending(3),
    ))));
    for (stage, successor) in [
        (SignatureStage::BeforeClone, Some(SignatureStage::AfterClone)),
        (SignatureStage::BeforeNormalize, Some(SignatureStage::AfterNormalize)),
        (SignatureStage::BeforeTransfer, Some(SignatureStage::AfterTransfer)),
        (SignatureStage::BeforePrefix, None),
    ] {
        let remaining = gradual_signature_boundary_work(&measured, stage)
            .expect("funded comparison must reach each transfer boundary");
        let (stopped, reason) = run(
            AnalysisPolicy {
                semantic_work_limit: funded().semantic_work_limit - remaining,
                ..funded()
            },
            Stop::None,
            Execution::Retained,
        );
        assert_eq!(reason, Some(AnalysisIncomplete::WorkLimit));
        let (cancelled, reason) = run(funded(), Stop::Cancel(stage), Execution::Retained);
        assert_eq!(reason, None);
        for snapshot in [&stopped, &cancelled] {
            assert!(gradual_signature_boundary_work(snapshot, stage).is_some(), "{stage:?}: {snapshot:?}");
            if let Some(successor) = successor {
                assert!(gradual_signature_boundary_work(snapshot, successor).is_none());
            } else {
                assert!(!snapshot.events.contains(&Some(SignatureEvent::Child(
                    retained_observations::Event::Entered(3),
                ))));
            }
        }
    }

    // Each boundary immediately precedes a positive byte admission. The minimum allowance that
    // reaches it therefore leaves too little for the following clone or transfer operation.
    for (stage, successor) in [
        (SignatureStage::BeforeClone, SignatureStage::AfterClone),
        (SignatureStage::BeforeTransfer, SignatureStage::AfterTransfer),
    ] {
        let mut low = 0;
        let mut high = funded().requested_bytes_limit;
        while low < high {
            let middle = low + (high - low) / 2;
            let (snapshot, _) = run(
                AnalysisPolicy { requested_bytes_limit: middle, ..funded() },
                Stop::None,
                Execution::Retained,
            );
            if gradual_signature_boundary_work(&snapshot, stage).is_some() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let (stopped, reason) = run(
            AnalysisPolicy { requested_bytes_limit: high, ..funded() },
            Stop::None,
            Execution::Retained,
        );
        assert_eq!(reason, Some(AnalysisIncomplete::RequestedAllocationLimit));
        assert!(gradual_signature_boundary_work(&stopped, stage).is_some());
        assert!(gradual_signature_boundary_work(&stopped, successor).is_none());
    }
    for execution in [Execution::Retained, Execution::Canonical] {
        let (stopped, reason) = run(
            funded(),
            Stop::Incomplete(SignatureStage::BeforeTransferCompletion),
            execution,
        );
        assert_eq!(reason, Some(AnalysisIncomplete::WorkLimit));
        assert!(gradual_signature_boundary_work(&stopped, SignatureStage::BeforeTransferCompletion).is_some());
        assert!(gradual_signature_boundary_work(&stopped, SignatureStage::AfterTransfer).is_none());
    }
    // The callable root enters first, then its return relation, then this distinct prefix relation.
    run(funded(), Stop::Pending(3), Execution::Retained);
}

#[test]
fn dynamic_intersection_assignability_preserves_the_invocation_builder() {
    let db = fixture();
    let prepared = prepare(&db);
    for dynamic in [
        Type::any(),
        Type::unknown(),
        todo_type!("invocation dynamic membership"),
        Type::divergent(salsa::Id::from_bits(321)),
    ] {
        let source = Type::Intersection(signed_intersection(
            &db,
            Sign::Positive,
            &[Type::bool_literal(false), dynamic],
            &[],
        ));
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Invocation {
                    source,
                    target: Type::AlwaysTruthy,
                    inferable: TypeVarSet::None,
                    expected: true,
                    disjoint_checks: 0,
                },
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_invocation_assignability(TypeVarSet::None, true, 0);
        assert_eq!(retained_observations::progress(), (0, 4, 1));
    }
}

async fn controlled_definition_instance<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    program: Program<'db>,
    definition: Definition<'db>,
) -> RunResult<Type<'db>> {
    let effects = SourceEffects::new(access, program);
    let inference = access.definition(definition).await?;
    let ty = access
        .endpoint()
        .local_call(|| {
            access.endpoint().admit_work(2)?;
            access.endpoint().check_completion()?;
            inference
                .original_class_type(definition)
                .map(Type::ClassLiteral)
                .ok_or(RunError::Contract("fixture definition is not a class"))
        })
        .await;
    let class = KnownClassInstanceEffects::to_class_type(&effects, ty)
        .await?
        .ok_or(RunError::Contract("fixture class has no class type"))?;
    KnownClassInstanceEffects::instance(&effects, class).await
}

fn nominal_pair_fixture() -> TestDb {
    let mut db = fixture();
    db.write_file(
        "src/main.py",
        "class Base: ...\nclass Leaf(Base): ...\nclass Unrelated: ...\n",
    )
    .unwrap();
    db
}

fn nominal_pair_definition<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    index: usize,
) -> Definition<'db> {
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[index] else {
        panic!("fixture statement is not a class");
    };
    prepared.semantic_index().expect_single_definition(class)
}

fn completed_nominal_class<'db>(
    db: &'db TestDb,
    definition: Definition<'db>,
) -> StaticClassLiteral<'db> {
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id(),
        )
        .is_ok()
    );
    let Some(ClassLiteral::Static(class)) =
        infer_definition_types(db, definition).original_class_type(definition)
    else {
        panic!("completed fixture definition is not a static class");
    };
    class
}

/// Instances of a subclass are subtypes of their base, while the reverse direction and unrelated
/// classes are rejected. Controlled inference starts from class definitions and matches an ordinary
/// check in a separate database; the source class's canonical MRO is reused on the same revision.
#[test]
fn nominal_instance_pairs_match_ordinary_and_reuse_the_canonical_mro() {
    for (source_index, target_index, expected) in [(1, 0, true), (0, 1, false), (1, 2, false)] {
        let ordinary_db = nominal_pair_fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
        let ordinary_types = [source_index, target_index].map(|index| {
            let definition = nominal_pair_definition(&ordinary_prepared, index);
            let Some(class) =
                infer_definition_types(&ordinary_db, definition).original_class_type(definition)
            else {
                panic!("ordinary fixture definition is not a class");
            };
            Type::instance(&ordinary_db, &ordinary_env, ClassType::NonGeneric(class))
        });
        assert_eq!(
            ordinary_types[0].is_subtype_of(&ordinary_db, &ordinary_env, ordinary_types[1]),
            expected,
        );

        let db = nominal_pair_fixture();
        let prepared = prepare(&db);
        let source = nominal_pair_definition(&prepared, source_index);
        let target = nominal_pair_definition(&prepared, target_index);
        let action = Action::NominalDefinitions {
            file: prepared.program_file(),
            source,
            target,
            expected,
            cancel_at_guard: false,
        };
        let revision = salsa::plumbing::current_revision(&db);
        let mut events = db.clone();
        events.take_salsa_events();
        observations::reset(None);
        let cold = capture(&db, || {
            controlled_action(&prepared, action, &funded(), &Cell::new(None))
        })
        .unwrap();
        assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(expected)));
        assert_eq!(cold.check_root_reads(), Ok(()));
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        let class = completed_nominal_class(&db, source);
        let ingredient = try_mro_unspecialized_ingredient(&db);
        let key = ingredient.database_key_index(class.as_id());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, class.as_id()).is_ok());
        let cold_read = cold.reads.iter().find(|read| read.key == key).unwrap();
        assert!(events.take_salsa_events().iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));

        let warm = capture(&db, || {
            controlled_action(&prepared, action, &funded(), &Cell::new(None))
        })
        .unwrap();
        assert_eq!(warm.value, cold.value);
        assert_eq!(warm.check_root_reads(), Ok(()));
        assert!(warm.reads.iter().any(|read| {
            read.key == key
                && read.memo_address == cold_read.memo_address
                && read.stamp == cold_read.stamp
        }));
        assert!(!events.take_salsa_events().iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// Work-limit refusal or local cancellation while the relation guard has an active entry releases
/// the queued comparison. The source class's MRO query completes before this interruption, so retry
/// reuses its canonical result on the same revision.
#[test]
fn nominal_instance_guard_interruption_drains_and_reuses_the_completed_mro() {
    let measured = nominal_pair_fixture();
    let measured_prepared = prepare(&measured);
    assert_eq!(
        controlled_action(
            &measured_prepared,
            Action::NominalDefinitions {
                file: measured_prepared.program_file(),
                source: nominal_pair_definition(&measured_prepared, 1),
                target: nominal_pair_definition(&measured_prepared, 0),
                expected: true,
                cancel_at_guard: false,
            },
            &funded(),
            &Cell::new(None),
        ),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (entered, remaining, active) = guard_observations::progress();
    assert_eq!((entered, active), (1, 1));
    let guard_work = funded().semantic_work_limit - remaining.unwrap();
    assert_eq!(retained_observations::progress(), (0, 1, 1));
    assert_no_active_attempt();

    for cancel in [false, true] {
        let db = nominal_pair_fixture();
        let prepared = prepare(&db);
        let source = nominal_pair_definition(&prepared, 1);
        let target = nominal_pair_definition(&prepared, 0);
        let action = Action::NominalDefinitions {
            file: prepared.program_file(),
            source,
            target,
            expected: true,
            cancel_at_guard: cancel,
        };
        let revision = salsa::plumbing::current_revision(&db);
        let mut events = db.clone();
        events.take_salsa_events();
        observations::reset(None);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: guard_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            })) if !cancel => {}
            other => panic!("cancel={cancel}: {other:?}"),
        }
        let (entered, remaining, active) = guard_observations::progress();
        assert_eq!((entered, active), (1, 1));
        if !cancel {
            assert_eq!(remaining, Some(0));
        }
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        let class = completed_nominal_class(&db, source);
        let ingredient = try_mro_unspecialized_ingredient(&db);
        let key = ingredient.database_key_index(class.as_id());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, class.as_id()).is_ok());
        assert!(events.take_salsa_events().iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));

        let retry = Action::NominalDefinitions {
            file: prepared.program_file(),
            source,
            target,
            expected: true,
            cancel_at_guard: false,
        };
        assert_eq!(
            controlled_action(&prepared, retry, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert!(!events.take_salsa_events().iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, class.as_id()).is_ok());
        assert_eq!(retained_observations::progress(), (0, 1, 1));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn nominal_generic_classification_matches_ordinary() {
    let mut db = setup_db();
    db.write_dedented(
        "src/main.py",
        r#"
        from typing import Any

        class Plain: ...
        class Generic[T]: ...
        class FromAny(Any): ...
        class GenericFromAny[T](Any): ...

        Dynamic = type("Dynamic", (), {})
        number: int
        items: list[int]

        left = right = 1
        "#,
    )
    .unwrap();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let instance = |name| {
        let Some(class) = crate::place::global_symbol(&db, prepared.program_file(), name)
            .place
            .expect_type()
            .to_class_type(&db)
        else {
            panic!("fixture does not name a class: {name}");
        };
        Type::instance(&db, &env, class)
    };
    let cases = [
        ("tuple", Type::empty_tuple(&db, &env), true, false),
        ("object", Type::object(), false, false),
        ("version-info", Type::sys_version_info(), false, false),
        ("plain", instance("Plain"), false, false),
        ("generic", instance("Generic"), true, false),
        ("explicit-any", instance("FromAny"), false, true),
        (
            "generic-explicit-any",
            instance("GenericFromAny"),
            true,
            true,
        ),
    ];
    let revision = salsa::plumbing::current_revision(&db);
    for (name, ty, expected, inherits_any) in cases {
        let Some(instance) = ty.as_nominal_instance() else {
            panic!("fixture is not a nominal instance: {name}");
        };
        assert_eq!(
            instance.inherits_from_explicit_any(),
            inherits_any,
            "{name}"
        );
        assert_eq!(
            controlled_action(
                &prepared,
                Action::NominalGeneric(instance),
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(expected)),
            "{name}",
        );
        assert_eq!(instance.is_definition_generic(&db), expected, "{name}");
        assert_eq!(
            controlled_action(
                &prepared,
                Action::NominalKnown(instance, instance.known_class(&db)),
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(true)),
            "{name}",
        );
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        materialization_observations::reset(None);
        let (source, negative) = if expected && inherits_any {
            (Type::bool_literal(true), vec![ty])
        } else {
            (ty, vec![])
        };
        let outcome = controlled_action(
            &prepared,
            Action::Invocation {
                source,
                target: Type::Intersection(signed_intersection(
                    &db,
                    Sign::Negative,
                    &negative,
                    &[],
                )),
                inferable: TypeVarSet::None,
                expected: true,
                disjoint_checks: 0,
            },
            &funded(),
            &Cell::new(None),
        );
        assert_eq!(
            outcome,
            Ok(if expected && name != "tuple" {
                unavailable(OperationId::Materialization(
                    MaterializationOperation::LegacyContinuation,
                ))
            } else {
                AnalysisOutcome::Complete(true)
            }),
            "{name}",
        );
        invocation_builder_identity();
        assert_eq!(
            materialization_observations::snapshot().count,
            match (expected && name != "tuple", inherits_any) {
                (true, true) => 2,
                (true, false) => 1,
                (false, true) => 0,
                (false, false) => 4,
            },
            "{name}",
        );
        assert_eq!(retained_observations::progress().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    for (ty, expected) in [
        (instance("Dynamic"), None),
        (
            crate::place::global_symbol(&db, prepared.program_file(), "number")
                .place
                .expect_type(),
            Some(KnownClass::Int),
        ),
        (
            crate::place::global_symbol(&db, prepared.program_file(), "items")
                .place
                .expect_type(),
            Some(KnownClass::List),
        ),
    ] {
        let Some(instance) = ty.as_nominal_instance() else {
            panic!("fixture is not a nominal instance: {ty:?}");
        };
        assert_eq!(instance.known_class(&db), expected);
        let captured = capture(&db, || {
            controlled_action(
                &prepared,
                Action::NominalKnown(instance, expected),
                &funded(),
                &Cell::new(None),
            )
        })
        .unwrap();
        assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(true)));
        assert!(captured.reads.is_empty());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn invocation_target_intersection_materializes_before_disjoint_comparison() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    for (negative, expected) in [
        (vec![], true),
        (vec![Type::bool_literal(false)], true),
        (vec![Type::any()], true),
        (vec![Type::bool_literal(true)], false),
    ] {
        let source = Type::bool_literal(true);
        let target = Type::Intersection(signed_intersection(
            &db,
            Sign::Positive,
            &[Type::AlwaysTruthy],
            &negative,
        ));
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Invocation {
                    source,
                    target,
                    inferable: TypeVarSet::None,
                    expected,
                    disjoint_checks: negative.len(),
                },
                &funded(),
                &Cell::new(None),
            ),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_invocation_assignability(TypeVarSet::None, expected, 1);
        assert_eq!(source.is_assignable_to(&db, &env, target), expected);
        assert_eq!(retained_observations::progress().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn invocation_target_intersection_keeps_callable_materialization_unavailable() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let cached = Type::Callable(CallableType::bottom(&db));
    for (source, negative) in [(cached, vec![]), (Type::bool_literal(true), vec![cached])] {
        let target = Type::Intersection(signed_intersection(&db, Sign::Negative, &negative, &[]));
        for _ in 0..2 {
            invocation_observations::reset_invocations();
            invocation_observations::reset_assignability();
            retained_observations::reset(None);
            assert_eq!(
                controlled_action(
                    &prepared,
                    Action::Invocation {
                        source,
                        target,
                        inferable: TypeVarSet::None,
                        expected: true,
                        disjoint_checks: 0,
                    },
                    &funded(),
                    &Cell::new(None),
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(OperationId::Materialization(
                        MaterializationOperation::Leaf(MappingOperation::Callable),
                    )),
                    completed: (),
                }),
            );
            invocation_builder_identity();
            assert_eq!(
                invocation_observations::assignability_snapshot().result_count,
                0
            );
            assert_eq!(retained_observations::progress(), (0, 1, 1));
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn relation_materialization_resolves_an_unresolved_file_environment() {
    let db = fixture();
    let prepared = prepare(&db);
    let input = TypeFormType::from_type_expression(&db, Type::any());
    let expected = TypeFormType::from_type_expression(&db, Type::Never);
    let revision = salsa::plumbing::current_revision(&db);
    let outcome = capture(&db, || {
        controlled_action(
            &prepared,
            Action::Materialization {
                file: prepared.program_file(),
                ty: input,
                kind: MaterializationKind::Bottom,
                expected,
            },
            &funded(),
            &Cell::new(None),
        )
    })
    .unwrap();
    assert_eq!(outcome.value, Ok(AnalysisOutcome::Complete(true)));
    assert_eq!(outcome.check_root_reads(), Ok(()));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn source_union_construction_preserves_nested_aliases_without_reading_their_bodies() {
    let mut db = fixture();
    db.write_file("src/main.py", "type Alias = int\n").unwrap();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
        crate::place::global_symbol(&db, prepared.program_file(), "Alias")
            .place
            .expect_type()
    else {
        panic!("expected a type alias");
    };
    let alias = Type::TypeAlias(alias);
    let field = |default, converter| {
        Type::KnownInstance(KnownInstanceType::Field(FieldInstance::new(
            &db,
            default,
            true,
            None,
            None,
            converter,
            ConfigBoolean::Unspecified,
        )))
    };
    let nested = [
        TypeFormType::from_type_expression(&db, alias),
        Type::heterogeneous_tuple(&db, &env, [alias]),
        TypeFormType::from_type_expression(
            &db,
            Type::Intersection(signed_intersection(&db, Sign::Positive, &[alias], &[])),
        ),
        Type::TypeGuard(TypeGuardType::new(&db, alias, None)),
        field(Some(alias), None),
        field(None, Some((Type::Never, alias))),
        Type::KnownInstance(KnownInstanceType::Annotated(InternedType::new(&db, alias))),
        Type::KnownInstance(KnownInstanceType::FunctoolsPartial(
            FunctoolsPartialInstance::new(
                &db,
                InternedType::new(&db, Type::Never),
                CallableType::single(&db, Signature::new(Parameters::empty(), alias)),
            ),
        )),
        Type::KnownInstance(KnownInstanceType::MethodWrapper(MethodWrapper::new(
            &db,
            alias,
            MethodWrapperKind::Staticmethod,
        ))),
    ];
    let revision = salsa::plumbing::current_revision(&db);
    let mut reader = db.clone();
    assert_function_query_was_not_run_by_name(
        &db,
        "raw_value_type",
        None,
        &reader.take_salsa_events(),
    );
    for first in nested {
        let second = Type::literal_string();
        let expected = Type::Union(UnionType::new(
            &db,
            vec![first, second].into_boxed_slice(),
            RecursivelyDefined::No,
        ));
        let captured = capture(&db, || {
            controlled_action(
                &prepared,
                Action::AliasPreservingUnion {
                    first,
                    second,
                    expected,
                },
                &funded(),
                &Cell::new(None),
            )
        })
        .unwrap();
        assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(true)));
        assert!(captured.reads.is_empty());
        assert!(captured.reads.iter().all(|read| {
            db.ingredient_debug_name(read.key.ingredient_index()) != "raw_value_type"
        }));
        assert_function_query_was_not_run_by_name(
            &db,
            "raw_value_type",
            None,
            &reader.take_salsa_events(),
        );
        assert_eq!(
            canonical_materialization_observations::snapshot().root_count,
            0
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }

    let captured = capture(&db, || {
        controlled_action(
            &prepared,
            Action::Materialization {
                file: prepared.program_file(),
                ty: nested[0],
                kind: MaterializationKind::Bottom,
                expected: Type::Never,
            },
            &funded(),
            &Cell::new(None),
        )
    })
    .unwrap();
    assert_eq!(
        captured.value,
        Ok(unavailable(OperationId::Materialization(
            MaterializationOperation::Leaf(MappingOperation::TypeAlias,)
        ))),
    );
    assert!(
        captured.reads.iter().all(|read| {
            db.ingredient_debug_name(read.key.ingredient_index()) != "raw_value_type"
        })
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "raw_value_type",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn source_union_alias_search_refuses_an_unavailable_stored_field() {
    let db = fixture();
    let prepared = prepare(&db);
    let first = Type::KnownInstance(KnownInstanceType::UnionType(UnionTypeInstance::new(
        &db,
        None,
        Ok(Type::any()),
    )));
    let second = Type::literal_string();
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        let captured = capture(&db, || {
            controlled_action(
                &prepared,
                Action::AliasPreservingUnion {
                    first,
                    second,
                    expected: Type::Never,
                },
                &funded(),
                &Cell::new(None),
            )
        })
        .unwrap();
        assert_eq!(
            captured.value,
            Ok(unavailable(OperationId::TypeSearch(
                SearchOperation::StoredField(TypeWalkFieldOperation::UnionValue,)
            ))),
        );
        assert!(captured.reads.is_empty());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }

    let first = Type::bool_literal(true);
    let expected = Type::Union(UnionType::new(
        &db,
        vec![first, second].into_boxed_slice(),
        RecursivelyDefined::No,
    ));
    let captured = capture(&db, || {
        controlled_action(
            &prepared,
            Action::AliasPreservingUnion {
                first,
                second,
                expected,
            },
            &funded(),
            &Cell::new(None),
        )
    })
    .unwrap();
    assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(true)));
    assert_eq!(captured.check_root_reads(), Ok(()));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn relation_materialization_rejects_a_foreign_file_environment() {
    let db = fixture();
    let prepared = prepare(&db);
    let file = prepared.program_file();
    let program = file.program(&db);
    let platform = if *program.python_platform(&db) == PythonPlatform::All {
        PythonPlatform::Identifier("linux".into())
    } else {
        PythonPlatform::All
    };
    let foreign = Program::new(&db, &platform, program.resolver_environment(&db));
    let foreign_file = ProgramFile::new(&db, file.file(&db), foreign);
    let input = TypeFormType::from_type_expression(&db, Type::any());
    let expected = TypeFormType::from_type_expression(&db, Type::Never);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events = db.clone();
    events.take_salsa_events();
    assert_eq!(
        controlled_action(
            &prepared,
            Action::Materialization {
                file: foreign_file,
                ty: input,
                kind: MaterializationKind::Bottom,
                expected,
            },
            &funded(),
            &Cell::new(None),
        ),
        Err(AnalysisFailure::Execution(RunError::Contract(
            "source program is foreign"
        ))),
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "cached_materialization",
        None,
        &events.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn invocation_target_materialization_interruption_retries_in_the_same_revision() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let source = Type::bool_literal(true);
    let target = Type::Intersection(signed_intersection(
        &db,
        Sign::Positive,
        &[Type::AlwaysTruthy],
        &[Type::bool_literal(false)],
    ));
    let action = Action::Invocation {
        source,
        target,
        inferable: TypeVarSet::None,
        expected: true,
        disjoint_checks: 1,
    };
    materialization_observations::reset(None);
    retained_observations::reset(None);
    assert_eq!(
        controlled_action(&prepared, action, &funded(), &Cell::new(None)),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let completed = materialization_observations::snapshot();
    assert_eq!(completed.count, 8);
    for (before, after) in completed.before.into_iter().zip(completed.after) {
        let (Some(before), Some(after)) = (before, after) else {
            panic!("materialization admission was not observed");
        };
        assert_eq!(before - after, 2);
    }
    for boundary in [1, 2] {
        for cancel in [false, true] {
            invocation_observations::reset_invocations();
            invocation_observations::reset_assignability();
            retained_observations::reset(None);
            materialization_observations::reset(cancel.then_some(boundary));
            let Some(remaining) = completed.before[boundary - 1] else {
                panic!("materialization boundary was not reached");
            };
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: funded().semantic_work_limit - remaining + 1,
                    ..funded()
                }
            };
            let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled_action(&prepared, action, &policy, &Cell::new(None))
            }));
            match outcome {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    ..
                })) if !cancel => {}
                other => panic!("cancel={cancel}: {other:?}"),
            }
            invocation_builder_identity();
            let stopped = materialization_observations::snapshot();
            assert_eq!(stopped.count, boundary);
            assert_eq!(stopped.after[boundary - 1], None);
            assert_eq!(retained_observations::progress(), (0, 2, 1));
            assert_no_active_attempt();
            invocation_observations::reset_invocations();
            invocation_observations::reset_assignability();
            retained_observations::reset(None);
            materialization_observations::reset(None);
            assert_eq!(
                controlled_action(&prepared, action, &funded(), &Cell::new(None)),
                Ok(AnalysisOutcome::Complete(true)),
            );
            assert_invocation_assignability(TypeVarSet::None, true, 1);
            assert_eq!(retained_observations::progress().0, 0);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn invocation_condition_rejects_a_different_builder() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    invocation_observations::reset_invocations();
    invocation_observations::reset_assignability();
    retained_observations::reset(None);
    assert_eq!(
        controlled_action(
            &prepared,
            Action::MismatchedInvocation,
            &funded(),
            &Cell::new(None),
        ),
        Err(AnalysisFailure::Execution(RunError::Contract(
            "argument condition uses a different constraint builder",
        ))),
    );
    invocation_builder_identity();
    let snapshot = invocation_observations::assignability_snapshot();
    assert_eq!(
        (
            snapshot.root_count,
            snapshot.pair_count,
            snapshot.result_count
        ),
        (0, 0, 0)
    );
    assert_eq!(retained_observations::progress(), (0, 0, 0));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn invocation_assignability_interruption_drains_before_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut source = Type::bool_literal(true);
    for _ in 0..8 {
        source = Type::Intersection(signed_intersection(&db, Sign::Positive, &[source], &[]));
    }
    let action = Action::Invocation {
        source,
        target: Type::AlwaysTruthy,
        inferable: TypeVarSet::None,
        expected: true,
        disjoint_checks: 0,
    };
    let remaining = Cell::new(None);
    invocation_observations::reset_invocations();
    invocation_observations::reset_assignability();
    retained_observations::reset(None);
    assert_eq!(
        controlled_action(&prepared, action, &funded(), &remaining),
        Ok(AnalysisOutcome::Complete(true)),
    );
    assert_invocation_assignability(TypeVarSet::None, true, 8);
    let Some(remaining) = remaining.get() else {
        panic!("invocation assignability did not complete");
    };
    let completed_work = funded().semantic_work_limit - remaining;
    for cancel in [false, true] {
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(cancel.then_some(3));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: completed_work - completed_work / 4,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                ..
            })) if !cancel => {}
            other => panic!("cancel={cancel}: {other:?}"),
        }
        let builder = invocation_builder_identity();
        let snapshot = invocation_observations::assignability_snapshot();
        assert!(snapshot.root_count > 0);
        for root in snapshot.roots.iter().flatten() {
            assert_eq!(root.identity.builder, builder);
        }
        for pair in snapshot.pairs.iter().flatten() {
            assert_eq!(pair.identity.builder, builder);
        }
        let (live, entered, poll_depth) = retained_observations::progress();
        assert_eq!(live, 0);
        assert!(entered > 1, "cancel={cancel}: entered={entered}");
        if cancel {
            assert_eq!(entered, 3);
        }
        assert_eq!(poll_depth, 1);
        assert_no_active_attempt();
        invocation_observations::reset_invocations();
        invocation_observations::reset_assignability();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_invocation_assignability(TypeVarSet::None, true, 8);
        assert_eq!(retained_observations::progress(), (0, 36, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn signed_membership_preserves_exact_identity_and_ignores_opposite_elements() {
    let db = fixture();
    let prepared = prepare(&db);
    let promotable = Type::bool_literal(true);
    let unpromotable = Type::LiteralValue(LiteralValueType::unpromotable(true));
    for sign in [Sign::Positive, Sign::Negative] {
        for (elements, target, expected) in [
            (vec![], promotable, false),
            (vec![promotable, Type::literal_string()], promotable, true),
            (vec![Type::AlwaysFalsy, promotable], promotable, true),
            (
                vec![Type::literal_string(), Type::AlwaysFalsy],
                promotable,
                false,
            ),
            (vec![unpromotable], promotable, false),
            (vec![promotable], unpromotable, false),
            (vec![unpromotable], unpromotable, true),
            (
                vec![todo_type!("membership")],
                todo_type!("membership"),
                true,
            ),
            (
                vec![todo_type!("membership")],
                todo_type!("another membership payload"),
                !cfg!(debug_assertions),
            ),
        ] {
            let intersection = signed_intersection(&db, sign, &elements, &[target]);
            assert_eq!(
                controlled_action(
                    &prepared,
                    Action::Membership(intersection, target, sign),
                    &funded(),
                    &Cell::new(None),
                ),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            let ordinary = match sign {
                Sign::Positive => intersection.positive(&db).contains(&target),
                Sign::Negative => intersection.negative(&db).contains(&target),
            };
            assert_eq!(ordinary, expected);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn signed_membership_charges_visited_elements_and_both_inline_payloads() {
    let db = fixture();
    let prepared = prepare(&db);
    let short = todo_type!("short");
    let long = todo_type!(
        "intersection membership compares this retained inline diagnostic payload with each candidate and charges its bytes before deciding exact equality"
    );
    for sign in [Sign::Positive, Sign::Negative] {
        let mut work = Vec::new();
        let cases = [
            (
                vec![Type::AlwaysTruthy, Type::AlwaysFalsy],
                Type::AlwaysTruthy,
                true,
            ),
            (
                vec![Type::AlwaysTruthy, Type::AlwaysFalsy],
                Type::AlwaysFalsy,
                true,
            ),
            (vec![short], Type::unknown(), false),
            (vec![long], Type::unknown(), false),
            (vec![Type::unknown()], short, false),
            (vec![Type::unknown()], long, false),
        ]
        .map(|(elements, target, expected)| {
            (
                signed_intersection(&db, sign, &elements, &[]),
                target,
                expected,
            )
        });
        for (intersection, target, expected) in cases {
            let remaining = Cell::new(None);
            assert_eq!(
                controlled_action(
                    &prepared,
                    Action::Membership(intersection, target, sign),
                    &funded(),
                    &remaining,
                ),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            let Some(remaining) = remaining.get() else {
                panic!("intersection membership did not complete");
            };
            work.push(funded().semantic_work_limit - remaining);
            assert_no_active_attempt();
        }
        assert!(work[1] > work[0], "late matches must inspect more elements");
        if cfg!(debug_assertions) {
            assert!(work[3] > work[2], "retained inline payload was not quoted");
            assert!(work[5] > work[4], "incoming inline payload was not quoted");
        }
    }
}

#[test]
fn signed_membership_work_refusal_retries_in_the_same_revision() {
    for sign in [Sign::Positive, Sign::Negative] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let intersection = signed_intersection(
            &db,
            sign,
            &[
                Type::AlwaysFalsy,
                Type::literal_string(),
                Type::AlwaysTruthy,
            ],
            &[],
        );
        let action = Action::Membership(intersection, Type::AlwaysTruthy, sign);
        let remaining = Cell::new(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &remaining),
            Ok(AnalysisOutcome::Complete(true)),
        );
        let Some(remaining) = remaining.get() else {
            panic!("intersection membership did not complete");
        };
        let refused = Cell::new(None);
        assert_eq!(
            controlled_action(
                &prepared,
                action,
                &AnalysisPolicy {
                    semantic_work_limit: funded().semantic_work_limit - remaining - 1,
                    ..funded()
                },
                &refused,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
        );
        assert_eq!(refused.get(), None);
        assert_no_active_attempt();
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn canonical_positive_membership_publishes_and_survives_native_cancellation() {
    canonical_membership(Sign::Positive);
}

#[test]
fn canonical_negative_membership_publishes_and_survives_native_cancellation() {
    canonical_membership(Sign::Negative);
}

#[test]
fn target_intersection_negatives_publish_and_survive_native_cancellation() {
    for (negative, expected) in [
        (vec![], true),
        (vec![Type::bool_literal(false)], true),
        (
            vec![Type::bool_literal(false), Type::bool_literal(true)],
            false,
        ),
    ] {
        for cancel in [false, true] {
            let db = fixture();
            let mut events = db.clone();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let revision = salsa::plumbing::current_revision(&db);
            let source = Type::bool_literal(true);
            let target =
                Type::Intersection(signed_intersection(&db, Sign::Negative, &negative, &[]));
            redundancy_observations::reset(cancel);
            let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled(&prepared, source, target, &funded())
            }));
            match result {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(outcome) if !cancel => {
                    assert_eq!(outcome, Ok(AnalysisOutcome::Complete(expected)));
                }
                other => panic!("{other:?}"),
            }
            let (live, entered, _) = redundancy_observations::progress();
            assert_eq!((live, entered), (0, 1));
            let pair = TypePair::new(&db, env.program(&db), source, target);
            assert!(
                FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                    .is_ok()
            );
            assert_no_active_attempt();
            redundancy_observations::reset(false);
            events.take_salsa_events();
            assert_eq!(
                controlled(&prepared, source, target, &funded()),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_eq!(source.is_redundant_with(&db, &env, target), expected);
            assert_eq!(redundancy_observations::progress(), (0, 0, None));
            assert_function_query_was_not_run_by_name(
                &db,
                "is_redundant_with_impl",
                None,
                &events.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn target_intersection_recursive_child_publishes_and_reuses_parent() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let source = Type::bool_literal(true);
    let target = Type::Intersection(signed_intersection(
        &db,
        Sign::Positive,
        &[Type::AlwaysTruthy],
        &[Type::bool_literal(false)],
    ));
    for expected_entered in [1, 0] {
        redundancy_observations::reset(false);
        assert_eq!(
            controlled(&prepared, source, target, &funded()),
            Ok(AnalysisOutcome::Complete(true)),
        );
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, expected_entered));
        let pair = TypePair::new(&db, prepared.program_file().program(&db), source, target);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_ok()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn target_intersection_work_refusal_retires_owners_and_retries() {
    let source = Type::bool_literal(true);
    fn target(db: &TestDb) -> Type<'_> {
        Type::Intersection(signed_intersection(
            db,
            Sign::Negative,
            &[Type::bool_literal(false), Type::literal_string()],
            &[],
        ))
    }
    let measured = fixture();
    let prepared = prepare(&measured);
    let remaining = Cell::new(None);
    redundancy_observations::reset(false);
    assert_eq!(
        controlled_action(
            &prepared,
            Action::Redundancy(source, target(&measured)),
            &funded(),
            &remaining,
        ),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (live, entered, owner_remaining) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let owner_work = funded().semantic_work_limit - owner_remaining.unwrap();
    let total_work = funded().semantic_work_limit - remaining.get().unwrap();
    assert!(total_work > owner_work);

    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let target = target(&db);
    redundancy_observations::reset(false);
    assert_eq!(
        controlled(
            &prepared,
            source,
            target,
            &AnalysisPolicy {
                semantic_work_limit: owner_work + (total_work - owner_work) / 2,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let pair = TypePair::new(&db, prepared.program_file().program(&db), source, target);
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id()).is_err()
    );
    assert_no_active_attempt();
    redundancy_observations::reset(false);
    assert_eq!(
        controlled(&prepared, source, target, &funded()),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

fn canonical_membership(sign: Sign) {
    for cancel in [false, true] {
        let db = fixture();
        let mut events = db.clone();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        let target = Type::bool_literal(true);
        let source = Type::Intersection(signed_intersection(
            &db,
            sign,
            &[Type::literal_string(), target],
            &[],
        ));
        let expected = matches!(sign, Sign::Positive);
        redundancy_observations::reset(cancel);
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, source, target, &funded())
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => {
                assert_eq!(outcome, Ok(AnalysisOutcome::Complete(expected)));
            }
            other => panic!("{other:?}"),
        }
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        let pair = TypePair::new(&db, env.program(&db), source, target);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_ok()
        );
        assert_no_active_attempt();
        redundancy_observations::reset(false);
        events.take_salsa_events();
        assert_eq!(
            controlled(&prepared, source, target, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(source.is_redundant_with(&db, &env, target), expected);
        assert_eq!(redundancy_observations::progress(), (0, 0, None));
        assert_function_query_was_not_run_by_name(
            &db,
            "is_redundant_with_impl",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn nondivergent_dynamic_membership_publishes_and_survives_native_cancellation() {
    let divergent = Type::divergent(salsa::plumbing::Id::from_bits(1));
    let todo = todo_type!("dynamic intersection membership");
    for (positive, negative, target, expected) in [
        (vec![], vec![Type::any()], Type::unknown(), false),
        (
            vec![Type::AlwaysTruthy],
            vec![Type::unknown(), todo],
            Type::any(),
            false,
        ),
        (vec![Type::any()], vec![], Type::unknown(), true),
        (vec![Type::unknown()], vec![], Type::any(), true),
        (vec![todo], vec![], Type::unknown(), true),
        (vec![divergent], vec![Type::any()], Type::unknown(), false),
        (vec![divergent, Type::any()], vec![], Type::unknown(), true),
    ] {
        for cancel in [false, true] {
            let db = fixture();
            let mut events = db.clone();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let revision = salsa::plumbing::current_revision(&db);
            let source = Type::Intersection(signed_intersection(
                &db,
                Sign::Positive,
                &positive,
                &negative,
            ));
            redundancy_observations::reset(cancel);
            let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled(&prepared, source, target, &funded())
            }));
            match result {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(outcome) if !cancel => {
                    assert_eq!(outcome, Ok(AnalysisOutcome::Complete(expected)));
                }
                other => panic!("{other:?}"),
            }
            let (live, entered, _) = redundancy_observations::progress();
            assert_eq!((live, entered), (0, 1));
            let pair = TypePair::new(&db, env.program(&db), source, target);
            assert!(
                FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                    .is_ok()
            );
            assert_no_active_attempt();
            redundancy_observations::reset(false);
            events.take_salsa_events();
            assert_eq!(
                controlled(&prepared, source, target, &funded()),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_eq!(source.is_redundant_with(&db, &env, target), expected);
            assert_eq!(redundancy_observations::progress(), (0, 0, None));
            assert_function_query_was_not_run_by_name(
                &db,
                "is_redundant_with_impl",
                None,
                &events.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn nondivergent_dynamic_membership_stops_at_the_first_match() {
    let mut work = Vec::new();
    for positive in [
        [Type::any(), Type::AlwaysTruthy, Type::AlwaysFalsy],
        [Type::AlwaysTruthy, Type::AlwaysFalsy, Type::any()],
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let source = Type::Intersection(signed_intersection(&db, Sign::Positive, &positive, &[]));
        let remaining = Cell::new(None);
        redundancy_observations::reset(false);
        assert_eq!(
            controlled_action(
                &prepared,
                Action::Redundancy(source, Type::unknown()),
                &funded(),
                &remaining,
            ),
            Ok(AnalysisOutcome::Complete(true)),
        );
        let Some(remaining) = remaining.get() else {
            panic!("dynamic intersection membership did not complete");
        };
        work.push(funded().semantic_work_limit - remaining);
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        assert_no_active_attempt();
    }
    assert!(work[1] > work[0], "late matches must inspect more elements");
}

#[test]
fn nondivergent_dynamic_membership_work_refusal_retires_owners_and_retries() {
    fn source(db: &TestDb) -> Type<'_> {
        Type::Intersection(signed_intersection(
            db,
            Sign::Positive,
            &[Type::AlwaysTruthy, Type::AlwaysFalsy, Type::any()],
            &[],
        ))
    }
    let target = Type::unknown();
    let measured = fixture();
    let prepared = prepare(&measured);
    let remaining = Cell::new(None);
    redundancy_observations::reset(false);
    assert_eq!(
        controlled_action(
            &prepared,
            Action::Redundancy(source(&measured), target),
            &funded(),
            &remaining,
        ),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (live, entered, owner_remaining) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let owner_work = funded().semantic_work_limit - owner_remaining.unwrap();
    let total_work = funded().semantic_work_limit - remaining.get().unwrap();
    assert!(total_work > owner_work);
    assert_no_active_attempt();

    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let source = source(&db);
    redundancy_observations::reset(false);
    assert_eq!(
        controlled(
            &prepared,
            source,
            target,
            &AnalysisPolicy {
                semantic_work_limit: owner_work + (total_work - owner_work) / 2,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let pair = TypePair::new(&db, prepared.program_file().program(&db), source, target);
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id()).is_err()
    );
    assert_no_active_attempt();
    redundancy_observations::reset(false);
    assert_eq!(
        controlled(&prepared, source, target, &funded()),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (live, entered, _) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id()).is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn canonical_redundancy_preserves_ordered_keys_and_ordinary_reuse() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let promotable = Type::bool_literal(true);
    let unpromotable = Type::LiteralValue(LiteralValueType::unpromotable(true));
    for (first, second, expected) in [
        (promotable, unpromotable, true),
        (unpromotable, promotable, false),
        (Type::unknown(), Type::AlwaysTruthy, false),
        (Type::AlwaysTruthy, Type::unknown(), false),
        (Type::bool_literal(false), Type::AlwaysFalsy, true),
        (Type::bool_literal(false), Type::AlwaysTruthy, false),
        (Type::bool_literal(true), Type::AlwaysFalsy, false),
        (Type::bool_literal(true), Type::AlwaysTruthy, true),
        (Type::literal_string(), Type::AlwaysFalsy, false),
        (Type::literal_string(), Type::AlwaysTruthy, false),
    ] {
        redundancy_observations::reset(false);
        assert_eq!(
            controlled(&prepared, first, second, &funded()),
            Ok(AnalysisOutcome::Complete(expected))
        );
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        let pair = TypePair::new(&db, env.program(&db), first, second);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_ok()
        );
        events.take_salsa_events();
        assert_eq!(first.is_redundant_with(&db, &env, second), expected);
        assert_eq!(
            controlled(&prepared, first, second, &funded()),
            Ok(AnalysisOutcome::Complete(expected))
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "is_redundant_with_impl",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    assert_ne!(
        TypePair::new(&db, env.program(&db), promotable, unpromotable),
        TypePair::new(&db, env.program(&db), unpromotable, promotable)
    );
}

#[test]
fn identical_operands_do_not_intern_a_pair_or_enter_the_query() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    redundancy_observations::reset(false);
    events.take_salsa_events();
    assert_eq!(
        controlled(&prepared, Type::unknown(), Type::unknown(), &funded()),
        Ok(AnalysisOutcome::Complete(true))
    );
    assert_eq!(redundancy_observations::progress(), (0, 0, None));
    let events = events.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "is_redundant_with_impl", None, &events);
    for event in &events {
        if let salsa::EventKind::DidInternValue { key, .. } = event.kind {
            assert_ne!(db.ingredient_debug_name(key.ingredient_index()), "TypePair");
        }
    }
    assert_no_active_attempt();
}

#[test]
fn unavailable_redundancy_descendant_leaves_the_parent_unpublished() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let first = KnownClass::Int.to_class_literal(&db, &env);
    let second = Type::AlwaysTruthy;
    for _ in 0..2 {
        redundancy_observations::reset(false);
        assert_eq!(
            controlled(&prepared, first, second, &funded()),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::Truthiness(
                    TruthinessOperation::MetaclassInstance
                )),
                completed: ()
            })
        );
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        let pair = TypePair::new(&db, env.program(&db), first, second);
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_err()
        );
        assert_no_active_attempt();
    }
    assert!(!first.is_redundant_with(&db, &env, second));
    events.take_salsa_events();
    assert_eq!(
        controlled(&prepared, first, second, &funded()),
        Ok(AnalysisOutcome::Complete(false))
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "is_redundant_with_impl",
        None,
        &events.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn interrupted_redundancy_retires_owners_and_retries_in_the_same_revision() {
    let first = Type::literal_string();
    let second = Type::AlwaysTruthy;
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    redundancy_observations::reset(false);
    assert_eq!(
        controlled(&measured_prepared, first, second, &funded()),
        Ok(AnalysisOutcome::Complete(false))
    );
    let (live, entered, remaining) = redundancy_observations::progress();
    assert_eq!((live, entered), (0, 1));
    let retained_work = funded().semantic_work_limit - remaining.unwrap();

    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        redundancy_observations::reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, first, second, &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            ),
            other => panic!("{other:?}"),
        }
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, 1));
        let pair = TypePair::new(&db, prepared.program_file().program(&db), first, second);
        // Salsa masks local cancellation while a fixpoint query owns its claim. The completed
        // child remains reusable when cancellation reaches the caller; work refusal publishes nothing.
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                .is_ok(),
            cancel,
            "cancel={cancel}",
        );
        assert_no_active_attempt();
        redundancy_observations::reset(false);
        assert_eq!(
            controlled(&prepared, first, second, &funded()),
            Ok(AnalysisOutcome::Complete(false))
        );
        let (live, entered, _) = redundancy_observations::progress();
        assert_eq!((live, entered), (0, usize::from(!cancel)));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn target_union_preserves_candidate_order_and_original_builder() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let source = Type::bool_literal(true);
    for (targets, expected, comparisons) in [
        ([Type::AlwaysTruthy, Type::AlwaysFalsy], true, 2),
        ([Type::AlwaysFalsy, Type::AlwaysTruthy], true, 3),
        ([Type::AlwaysFalsy, Type::bool_literal(false)], false, 3),
    ] {
        let target = Type::Union(UnionType::new(
            &db,
            Vec::from(targets).into_boxed_slice(),
            RecursivelyDefined::No,
        ));
        for canonical in [false, true] {
            let action = if canonical {
                Action::Redundancy(source, target)
            } else {
                Action::RetainedPair(source, target, expected)
            };
            retained_observations::reset(None);
            redundancy_observations::reset(false);
            assert_eq!(
                controlled_action(&prepared, action, &funded(), &Cell::new(None)),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_eq!(retained_observations::progress(), (0, comparisons, 1));
            if canonical {
                let (live, entered, _) = redundancy_observations::progress();
                assert_eq!((live, entered), (0, 1));
                let pair = TypePair::new(&db, env.program(&db), source, target);
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        redundancy_ingredient(&db),
                        pair.as_id(),
                    )
                    .is_ok()
                );
                retained_observations::reset(None);
                redundancy_observations::reset(false);
                assert_eq!(source.is_redundant_with(&db, &env, target), expected);
                assert_eq!(
                    controlled_action(&prepared, action, &funded(), &Cell::new(None)),
                    Ok(AnalysisOutcome::Complete(expected)),
                );
                assert_eq!(retained_observations::progress(), (0, 0, 0));
                assert_eq!(redundancy_observations::progress(), (0, 0, None));
            } else {
                assert_eq!(source.is_subtype_of(&db, &env, target), expected);
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn target_union_refusal_and_child_cancellation_drain_before_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let source = Type::bool_literal(true);
    // Nested target intersections queue descendants while the union fold awaits its first child.
    // The complete run enters the union root, each intersection, and the truthiness leaf.
    let child_depth = 8;
    let mut recursive_child = Type::AlwaysTruthy;
    for _ in 0..child_depth {
        recursive_child = Type::Intersection(signed_intersection(
            &db,
            Sign::Positive,
            &[recursive_child],
            &[],
        ));
    }
    let target = Type::Union(UnionType::new(
        &db,
        vec![recursive_child, Type::AlwaysFalsy].into_boxed_slice(),
        RecursivelyDefined::No,
    ));
    let action = Action::FreshSubtyping(source, target);
    retained_observations::reset(None);
    let remaining = Cell::new(None);
    assert_eq!(
        controlled_action(&prepared, action, &funded(), &remaining),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let Some(remaining) = remaining.get() else {
        panic!("target union comparison did not complete");
    };
    let completed_work = funded().semantic_work_limit - remaining;
    assert_eq!(retained_observations::progress(), (0, child_depth + 2, 1));
    assert_no_active_attempt();
    for cancel in [false, true] {
        retained_observations::reset(cancel.then_some(3));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: completed_work - completed_work / 4,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_action(&prepared, action, &policy, &Cell::new(None))
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                ..
            })) if !cancel => {}
            other => panic!("cancel={cancel}: {other:?}"),
        }
        let (live, entered, poll_depth) = retained_observations::progress();
        assert_eq!(live, 0);
        assert!(entered > 2, "cancel={cancel}: entered={entered}");
        if cancel {
            assert_eq!(entered, 3);
        }
        assert_eq!(poll_depth, 1);
        assert_no_active_attempt();
        retained_observations::reset(None);
        assert_eq!(
            controlled_action(&prepared, action, &funded(), &Cell::new(None)),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert_eq!(retained_observations::progress(), (0, child_depth + 2, 1));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}
