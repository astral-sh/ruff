use ruff_python_ast::name::Name;

use super::*;
use crate::types::infer::builder::source_definition::controlled::tuple_widening_observations;
use crate::types::normalization::{NormalizationEffects, NormalizationSearch};
use crate::types::tuple::{TupleSpec, TupleType, VariableLengthTuple, VariableSegment};
use crate::types::typevar::{BindingContext, TypeVarNonce};
use crate::types::visitor::{SearchOperation, TypeWalkFieldOperation, any_over_type};
use crate::types::{DynamicType, PropertyInstanceType};

enum Operation<'db> {
    Widen {
        previous: Type<'db>,
        current: Type<'db>,
    },
    RecoveryUnion {
        previous: Type<'db>,
        current: Type<'db>,
    },
    Search {
        ty: Type<'db>,
        search: NormalizationSearch,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum Output<'db> {
    Widened(Option<Type<'db>>),
    Recovered(Type<'db>),
    Found(bool),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    env: &ProgramEnvironment<'db>,
    operation: Operation<'db>,
    policy: &AnalysisPolicy,
    refuse_bytes: bool,
) -> Result<AnalysisOutcome<Output<'db>>, AnalysisFailure> {
    tuple_widening_observations::reset(refuse_bytes);
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
        let result = run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            match operation {
                Operation::Widen { previous, current } => effects
                    .widen_growing_tuples(env, previous, current)
                    .await
                    .map(Output::Widened),
                Operation::Search { ty, search } => {
                    NormalizationEffects::contains(&effects, ty, env, search)
                        .await
                        .map(Output::Found)
                }
                Operation::RecoveryUnion { previous, current } => {
                    NormalizationEffects::recovery_union(&effects, env, previous, current)
                        .await
                        .map(Output::Recovered)
                }
            }
        });
        assert_eq!(observations::counts().0, 0);
        result
    })
}

fn tuple<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, spec: TupleSpec<'db>) -> Type<'db> {
    Type::tuple(TupleType::new_internal(db, env.program(db), spec))
}

fn union<'db>(db: &'db dyn Db, elements: &[Type<'db>], recursion: RecursivelyDefined) -> Type<'db> {
    Type::Union(UnionType::new(
        db,
        elements.to_vec().into_boxed_slice(),
        recursion,
    ))
}

fn pack<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> BoundTypeVarInstance<'db> {
    let variable = TypeVarInstance::new(
        db,
        TypeVarIdentity::new(
            db,
            Name::new_static("Ts"),
            None,
            TypeVarKind::Pep695TypeVarTuple,
        ),
        None,
        None,
        None,
    );
    BoundTypeVarInstance::new(
        db,
        variable,
        BindingContext::Synthetic(env.program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

#[derive(Clone, Copy, Debug)]
enum WideningCase {
    Identical,
    NoPreviousTuple,
    SameFixedLength,
    StableAlternatives,
    Longer,
    Shorter,
    Prefix,
    Suffix,
    StablePack,
    GrowingPack,
    Never,
    OrderedAlternatives,
}

impl WideningCase {
    fn inputs<'db>(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> (Type<'db>, Type<'db>) {
        let one = tuple(db, env, TupleSpec::heterogeneous([Type::object()]));
        let two = tuple(
            db,
            env,
            TupleSpec::heterogeneous([Type::object(), Type::object()]),
        );
        match self {
            Self::Identical => (one, one),
            Self::NoPreviousTuple => (Type::object(), two),
            Self::SameFixedLength => (
                one,
                tuple(db, env, TupleSpec::heterogeneous([Type::unknown()])),
            ),
            Self::StableAlternatives => (
                union(db, &[one, two], RecursivelyDefined::No),
                union(db, &[two, one], RecursivelyDefined::No),
            ),
            Self::Longer => (one, two),
            Self::Shorter => (two, one),
            Self::Prefix | Self::Suffix => (
                tuple(db, env, TupleSpec::homogeneous(Type::object())),
                tuple(
                    db,
                    env,
                    if matches!(self, Self::Prefix) {
                        VariableLengthTuple::mixed(
                            [Type::unknown()],
                            VariableSegment::Homogeneous(Type::object()),
                            [],
                        )
                        .into()
                    } else {
                        VariableLengthTuple::mixed(
                            [],
                            VariableSegment::Homogeneous(Type::object()),
                            [Type::unknown()],
                        )
                        .into()
                    },
                ),
            ),
            Self::StablePack | Self::GrowingPack => {
                let pack = pack(db, env);
                let previous = tuple(
                    db,
                    env,
                    VariableLengthTuple::mixed(
                        [Type::unknown()],
                        VariableSegment::TypeVarTuple(pack),
                        [],
                    )
                    .into(),
                );
                let current = tuple(
                    db,
                    env,
                    if matches!(self, Self::StablePack) {
                        VariableLengthTuple::mixed(
                            [Type::object()],
                            VariableSegment::TypeVarTuple(pack),
                            [],
                        )
                        .into()
                    } else {
                        VariableLengthTuple::mixed(
                            [Type::unknown()],
                            VariableSegment::TypeVarTuple(pack),
                            [Type::Never],
                        )
                        .into()
                    },
                );
                (previous, current)
            }
            Self::Never => (
                tuple(db, env, TupleSpec::heterogeneous([Type::Never])),
                tuple(
                    db,
                    env,
                    TupleSpec::heterogeneous([Type::Never, Type::Never]),
                ),
            ),
            Self::OrderedAlternatives => (
                union(
                    db,
                    &[
                        Type::AlwaysFalsy,
                        tuple(db, env, TupleSpec::heterogeneous([Type::unknown()])),
                    ],
                    RecursivelyDefined::No,
                ),
                union(db, &[Type::AlwaysTruthy, two], RecursivelyDefined::Yes),
            ),
        }
    }

    fn expected<'db>(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        match self {
            Self::Identical
            | Self::NoPreviousTuple
            | Self::SameFixedLength
            | Self::StableAlternatives
            | Self::StablePack => None,
            Self::Prefix | Self::Suffix => {
                let elements = union(
                    db,
                    &[Type::object(), Type::unknown()],
                    RecursivelyDefined::Yes,
                );
                Some(tuple(db, env, TupleSpec::homogeneous(elements)))
            }
            Self::GrowingPack => {
                let elements = union(
                    db,
                    &[Type::unknown(), Type::object()],
                    RecursivelyDefined::Yes,
                );
                Some(tuple(db, env, TupleSpec::homogeneous(elements)))
            }
            Self::OrderedAlternatives => {
                let elements = union(
                    db,
                    &[Type::unknown(), Type::object()],
                    RecursivelyDefined::Yes,
                );
                let widened = tuple(db, env, TupleSpec::homogeneous(elements));
                Some(union(
                    db,
                    &[Type::AlwaysTruthy, widened],
                    RecursivelyDefined::Yes,
                ))
            }
            _ => Some(tuple(db, env, TupleSpec::homogeneous(Type::object()))),
        }
    }
}

#[test]
fn widening_preserves_stable_lengths_and_reconstructs_growing_tuples() {
    for case in [
        WideningCase::Identical,
        WideningCase::NoPreviousTuple,
        WideningCase::SameFixedLength,
        WideningCase::StableAlternatives,
        WideningCase::Longer,
        WideningCase::Shorter,
        WideningCase::Prefix,
        WideningCase::Suffix,
        WideningCase::StablePack,
        WideningCase::GrowingPack,
        WideningCase::Never,
        WideningCase::OrderedAlternatives,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (previous, current) = case.inputs(&db, &env);
        let revision = salsa::plumbing::current_revision(&db);
        let captured = capture(&db, || {
            controlled(
                &prepared,
                &env,
                Operation::Widen { previous, current },
                &funded(),
                false,
            )
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete(Output::Widened(actual))) = captured.value else {
            panic!("{case:?}: {:?}", captured.value);
        };
        assert!(captured.reads.is_empty(), "{case:?}: {:?}", captured.reads);
        assert_eq!(actual, case.expected(&db, &env), "{case:?}");
        if let Some(actual) = actual {
            assert_ne!(actual, current, "{case:?}");
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();

        let ordinary_db = fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
        let (previous, current) = case.inputs(&ordinary_db, &ordinary_env);
        let ordinary =
            UnionType::widen_growing_tuples(&ordinary_db, &ordinary_env, previous, current);
        assert_eq!(
            ordinary,
            case.expected(&ordinary_db, &ordinary_env),
            "{case:?}"
        );
        assert_eq!(
            actual.map(|ty| ty.display(&db, &env).to_string()),
            ordinary.map(|ty| ty.display(&ordinary_db, &ordinary_env).to_string()),
            "{case:?}",
        );
    }
}

#[test]
fn recovery_union_preserves_previous_order_and_merges_nested_recursion() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let previous = union(
        &db,
        &[Type::unknown(), Type::AlwaysTruthy],
        RecursivelyDefined::No,
    );
    let current = union(
        &db,
        &[Type::AlwaysTruthy, Type::object()],
        RecursivelyDefined::Yes,
    );
    let revision = salsa::plumbing::current_revision(&db);
    let result = controlled(
        &prepared,
        &env,
        Operation::RecoveryUnion { previous, current },
        &funded(),
        false,
    );
    let Ok(AnalysisOutcome::Complete(Output::Recovered(Type::Union(actual)))) = result else {
        panic!("recovery union must complete: {result:?}");
    };
    assert_eq!(
        actual.elements(&db),
        &[Type::unknown(), Type::AlwaysTruthy, Type::object()]
    );
    assert_eq!(actual.recursively_defined(&db), RecursivelyDefined::Yes);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();

    let ordinary_db = fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let previous = union(
        &ordinary_db,
        &[Type::unknown(), Type::AlwaysTruthy],
        RecursivelyDefined::No,
    );
    let current = union(
        &ordinary_db,
        &[Type::AlwaysTruthy, Type::object()],
        RecursivelyDefined::Yes,
    );
    let ordinary =
        UnionType::from_elements_cycle_recovery(&ordinary_db, &ordinary_env, [previous, current])
            .expect_union();
    assert_eq!(ordinary.elements(&ordinary_db), actual.elements(&db));
    assert_eq!(
        ordinary.recursively_defined(&ordinary_db),
        actual.recursively_defined(&db)
    );
}

#[test]
fn widening_keeps_literal_grouping_unavailable() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = tuple(&db, &env, TupleSpec::heterogeneous([Type::int_literal(1)]));
    let second = tuple(
        &db,
        &env,
        TupleSpec::heterogeneous([Type::int_literal(1), Type::int_literal(2)]),
    );
    assert_eq!(
        controlled(
            &prepared,
            &env,
            Operation::Widen {
                previous: first,
                current: second
            },
            &funded(),
            false
        ),
        Ok(unavailable(OperationId::Union)),
    );
    assert_no_active_attempt();
    let ordinary_db = fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let first = tuple(
        &ordinary_db,
        &ordinary_env,
        TupleSpec::heterogeneous([Type::int_literal(1)]),
    );
    let second = tuple(
        &ordinary_db,
        &ordinary_env,
        TupleSpec::heterogeneous([Type::int_literal(1), Type::int_literal(2)]),
    );
    assert!(UnionType::widen_growing_tuples(&ordinary_db, &ordinary_env, first, second).is_some());
}

#[test]
fn normalization_search_uses_exact_predicates_and_skips_unneeded_fields() {
    for ambiguous in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let program = env.program(&db);
        let platform = if *program.python_platform(&db) == PythonPlatform::All {
            PythonPlatform::Identifier("linux".into())
        } else {
            PythonPlatform::All
        };
        let foreign = Program::new(&db, &platform, program.resolver_environment(&db));
        let foreign_env = ProgramEnvironment::from_program(foreign);
        let divergent = Type::divergent(
            prepared
                .semantic_index()
                .expression(expression_key(&prepared))
                .as_id(),
        );
        let needle = if ambiguous {
            Type::Dynamic(DynamicType::AmbiguousOverload)
        } else {
            divergent
        };
        let search = || {
            if ambiguous {
                NormalizationSearch::AmbiguousOverload
            } else {
                NormalizationSearch::Divergent
            }
        };
        let unsupported = Type::PropertyInstance(PropertyInstanceType::new(&db, None, None, None));
        let found = tuple(
            &db,
            &env,
            TupleSpec::heterogeneous([
                TypeFormType::from_type_expression(&db, needle),
                unsupported,
            ]),
        );
        assert_eq!(
            controlled(
                &prepared,
                &foreign_env,
                Operation::Search {
                    ty: found,
                    search: search()
                },
                &funded(),
                false
            ),
            Ok(AnalysisOutcome::Complete(Output::Found(true))),
        );
        assert_no_active_attempt();
        assert_eq!(
            controlled(
                &prepared,
                &foreign_env,
                Operation::Search {
                    ty: Type::unknown(),
                    search: search()
                },
                &funded(),
                false
            ),
            Ok(AnalysisOutcome::Complete(Output::Found(false))),
        );
        assert_no_active_attempt();
        let other = if ambiguous {
            divergent
        } else {
            Type::Dynamic(DynamicType::AmbiguousOverload)
        };
        assert_eq!(
            controlled(
                &prepared,
                &foreign_env,
                Operation::Search {
                    ty: other,
                    search: search()
                },
                &funded(),
                false
            ),
            Ok(AnalysisOutcome::Complete(Output::Found(false))),
        );
        assert_no_active_attempt();
        let refused = tuple(&db, &env, TupleSpec::heterogeneous([unsupported, needle]));
        assert_eq!(
            controlled(
                &prepared,
                &foreign_env,
                Operation::Search {
                    ty: refused,
                    search: search()
                },
                &funded(),
                false
            ),
            Ok(unavailable(OperationId::TypeSearch(
                SearchOperation::StoredField(TypeWalkFieldOperation::PropertyDeleter)
            ))),
        );
        assert_no_active_attempt();

        let ordinary_db = fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
        let ordinary_needle = if ambiguous {
            Type::Dynamic(DynamicType::AmbiguousOverload)
        } else {
            Type::divergent(
                ordinary_prepared
                    .semantic_index()
                    .expression(expression_key(&ordinary_prepared))
                    .as_id(),
            )
        };
        let ordinary = tuple(
            &ordinary_db,
            &ordinary_env,
            TupleSpec::heterogeneous([
                TypeFormType::from_type_expression(&ordinary_db, ordinary_needle),
                Type::PropertyInstance(PropertyInstanceType::new(&ordinary_db, None, None, None)),
            ]),
        );
        assert!(any_over_type(
            &ordinary_db,
            &ordinary_env,
            ordinary,
            false,
            |ty| {
                if ambiguous {
                    matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
                } else {
                    ty.is_divergent()
                }
            }
        ));
    }
}

#[test]
fn widening_refusal_after_length_storage_retries_at_the_same_revision() {
    let measured_db = fixture();
    let measured_prepared = prepare(&measured_db);
    let measured_env = ProgramEnvironment::from_file(measured_prepared.program_file());
    let (previous, current) = WideningCase::Never.inputs(&measured_db, &measured_env);
    assert!(matches!(
        controlled(
            &measured_prepared,
            &measured_env,
            Operation::Widen { previous, current },
            &funded(),
            false,
        ),
        Ok(AnalysisOutcome::Complete(Output::Widened(Some(_))))
    ));
    let progress = tuple_widening_observations::snapshot();
    assert_eq!(progress.lengths_pushed, 1);
    assert_eq!(progress.union_builds, 2);
    let stored_work = funded().semantic_work_limit - progress.first_length_remaining.unwrap();
    let final_build_work =
        funded().semantic_work_limit - progress.union_build_remaining[1].unwrap();
    for (refuse_bytes, spent, final_build) in [
        (false, stored_work, false),
        (true, 0, false),
        (false, final_build_work, true),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (previous, current) = WideningCase::Never.inputs(&db, &env);
        let revision = salsa::plumbing::current_revision(&db);
        let policy = if refuse_bytes {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: spent,
                ..funded()
            }
        };
        assert_eq!(
            controlled(
                &prepared,
                &env,
                Operation::Widen { previous, current },
                &policy,
                refuse_bytes
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: if refuse_bytes {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: (),
            }),
        );
        let progress = tuple_widening_observations::snapshot();
        assert_eq!(progress.lengths_pushed, 1);
        assert_eq!(progress.union_builds, if final_build { 2 } else { 0 });
        if final_build {
            assert_eq!(progress.union_build_remaining[1], Some(0));
        } else if !refuse_bytes {
            assert_eq!(progress.first_length_remaining, Some(0));
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
        let retry = controlled(
            &prepared,
            &env,
            Operation::Widen { previous, current },
            &funded(),
            false,
        );
        assert_eq!(
            retry,
            Ok(AnalysisOutcome::Complete(Output::Widened(
                WideningCase::Never.expected(&db, &env)
            )))
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn length_buffer_growth_obeys_its_finite_byte_budget() {
    let probe = |requested_bytes_limit| {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (previous, current) = WideningCase::Never.inputs(&db, &env);
        let result = controlled(
            &prepared,
            &env,
            Operation::Widen { previous, current },
            &AnalysisPolicy {
                requested_bytes_limit,
                ..funded()
            },
            false,
        );
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(Output::Widened(Some(_))))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        ..
                    })
            ),
            "{requested_bytes_limit}: {result:?}"
        );
        assert_no_active_attempt();
        tuple_widening_observations::snapshot()
    };
    let complete = probe(funded().requested_bytes_limit);
    assert_eq!(complete.growth_attempts, 1);
    assert_eq!(complete.growth_allocations, 1);
    let growth_bytes = complete.first_growth_requested_bytes.unwrap();
    assert!(growth_bytes > 0);

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if probe(middle).growth_allocations == 0 {
            lower = middle + 1;
        } else {
            upper = middle;
        }
    }
    let reserve_limit = lower;
    assert!(reserve_limit > growth_bytes);
    let before_growth_limit = reserve_limit - growth_bytes;
    let before = probe(before_growth_limit);
    assert_eq!(before.growth_attempts, 1);
    assert_eq!(before.growth_allocations, 0);
    assert_eq!(before.lengths_pushed, 0);
    assert_eq!(before.first_growth_requested_bytes, Some(growth_bytes));
    assert_eq!(probe(before_growth_limit - 1).growth_attempts, 0);
    assert_eq!(probe(reserve_limit).growth_allocations, 1);

    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (previous, current) = WideningCase::Never.inputs(&db, &env);
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled(
            &prepared,
            &env,
            Operation::Widen { previous, current },
            &AnalysisPolicy {
                requested_bytes_limit: reserve_limit - 1,
                ..funded()
            },
            false,
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: (),
        }),
    );
    let refused = tuple_widening_observations::snapshot();
    assert_eq!(refused.growth_attempts, 1);
    assert_eq!(refused.growth_allocations, 0);
    assert_eq!(refused.lengths_pushed, 0);
    assert_eq!(refused.union_builds, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();

    assert_eq!(
        controlled(
            &prepared,
            &env,
            Operation::Widen { previous, current },
            &funded(),
            false,
        ),
        Ok(AnalysisOutcome::Complete(Output::Widened(
            WideningCase::Never.expected(&db, &env)
        ))),
    );
    let retried = tuple_widening_observations::snapshot();
    assert_eq!(retried.growth_attempts, 1);
    assert_eq!(retried.growth_allocations, 1);
    assert_eq!(retried.lengths_pushed, 1);
    assert_eq!(retried.union_builds, 2);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
