use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;

use super::*;
use crate::types::normalization::RecursiveNormalizationRequest;
use crate::types::normalization::source::observations as normalization_observations;
use crate::types::normalization::source::observations::{Event, Stop, StopKind};
use crate::types::tuple::{TupleSpec, TupleType, VariableLengthTuple, VariableSegment};
use crate::types::typevar::{BindingContext, TypeVarNonce};

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    env: &ProgramEnvironment<'db>,
    request: RecursiveNormalizationRequest<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Option<Type<'db>>>, AnalysisFailure> {
    controlled_with_stop(prepared, env, request, policy, None)
}

fn controlled_with_stop<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    env: &ProgramEnvironment<'db>,
    request: RecursiveNormalizationRequest<'db>,
    policy: &AnalysisPolicy,
    stop: Option<Stop>,
) -> Result<AnalysisOutcome<Option<Type<'db>>>, AnalysisFailure> {
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
                SourceEffects::new(&access, session.program())
                    .recursive_normalize(env, request)
                    .await
            })
        }));
        assert_eq!(observations::counts().0, 0);
        let snapshot = normalization_observations::snapshot();
        assert_eq!(snapshot.live_buffers, 0, "{snapshot:?}");
        assert_eq!(snapshot.buffers, snapshot.dropped_buffers, "{snapshot:?}");
        assert_eq!(snapshot.children, snapshot.dropped, "{snapshot:?}");
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

fn marker<'db>(prepared: &PreparedAnalysisFile<'db>) -> Type<'db> {
    Type::divergent(
        prepared
            .semantic_index()
            .expression(expression_key(prepared))
            .as_id(),
    )
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

fn raw_tuple<'db>(db: &'db dyn Db, program: Program<'db>, spec: TupleSpec<'db>) -> Type<'db> {
    Type::tuple(TupleType::new_internal(db, program, spec))
}

#[derive(Clone, Copy, Debug)]
enum TupleCase {
    Empty,
    Fixed,
    Homogeneous,
    Prefix,
    Suffix,
    Mixed,
    Pack,
    PackPrefix,
    PackSuffix,
    PackMixed,
    NeverHomogeneous,
}

impl TupleCase {
    fn value<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        child: Type<'db>,
        pack: BoundTypeVarInstance<'db>,
    ) -> Type<'db> {
        let first = Type::int_literal(1);
        let last = Type::int_literal(2);
        let spec = match self {
            Self::Empty => TupleSpec::heterogeneous([]),
            Self::Fixed => TupleSpec::heterogeneous([first, child, last]),
            Self::Homogeneous => TupleSpec::homogeneous(child),
            Self::Prefix => {
                VariableLengthTuple::mixed([first, child], VariableSegment::Homogeneous(child), [])
                    .into()
            }
            Self::Suffix => {
                VariableLengthTuple::mixed([], VariableSegment::Homogeneous(child), [child, last])
                    .into()
            }
            Self::Mixed => VariableLengthTuple::mixed(
                [first, child],
                VariableSegment::Homogeneous(child),
                [child, last],
            )
            .into(),
            Self::Pack => {
                VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(pack), []).into()
            }
            Self::PackPrefix => {
                VariableLengthTuple::mixed([first, child], VariableSegment::TypeVarTuple(pack), [])
                    .into()
            }
            Self::PackSuffix => {
                VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(pack), [child, last])
                    .into()
            }
            Self::PackMixed => VariableLengthTuple::mixed(
                [first, child],
                VariableSegment::TypeVarTuple(pack),
                [child, last],
            )
            .into(),
            Self::NeverHomogeneous => TupleSpec::homogeneous(Type::Never),
        };
        raw_tuple(db, env.program(db), spec)
    }

    fn has_collapsing_child(self) -> bool {
        !matches!(self, Self::Empty | Self::Pack | Self::NeverHomogeneous)
    }
}

fn display_result<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    value: Option<Type<'db>>,
) -> Option<String> {
    value.map(|ty| ty.display(db, env).to_string())
}

#[test]
fn recursive_normalization_preserves_exact_tuple_shapes_and_collapses_nested_children() {
    for case in [
        TupleCase::Empty,
        TupleCase::Fixed,
        TupleCase::Homogeneous,
        TupleCase::Prefix,
        TupleCase::Suffix,
        TupleCase::Mixed,
        TupleCase::Pack,
        TupleCase::PackPrefix,
        TupleCase::PackSuffix,
        TupleCase::PackMixed,
        TupleCase::NeverHomogeneous,
    ] {
        for nested in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let divergent = marker(&prepared);
            let typevartuple = pack(&db, &env);
            let child = Type::heterogeneous_tuple(&db, &env, [divergent, Type::int_literal(3)]);
            let input = case.value(&db, &env, child, typevartuple);
            let revision = salsa::plumbing::current_revision(&db);
            let captured = capture(&db, || {
                controlled(
                    &prepared,
                    &env,
                    RecursiveNormalizationRequest {
                        ty: input,
                        divergent,
                        nested,
                    },
                    &funded(),
                )
            })
            .unwrap();
            let Ok(AnalysisOutcome::Complete(actual)) = captured.value else {
                panic!("{case:?}, nested={nested}: {:?}", captured.value);
            };
            assert!(captured.reads.is_empty());
            let expected = if nested && case.has_collapsing_child() {
                None
            } else {
                Some(case.value(&db, &env, divergent, typevartuple))
            };
            assert_eq!(actual, expected, "{case:?}, nested={nested}");
            if !nested && case.has_collapsing_child() {
                assert_ne!(actual, Some(input));
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();

            let ordinary_db = fixture();
            let ordinary_prepared = prepare(&ordinary_db);
            let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
            let ordinary_marker = marker(&ordinary_prepared);
            let ordinary_pack = pack(&ordinary_db, &ordinary_env);
            let ordinary_child = Type::heterogeneous_tuple(
                &ordinary_db,
                &ordinary_env,
                [ordinary_marker, Type::int_literal(3)],
            );
            let ordinary_input =
                case.value(&ordinary_db, &ordinary_env, ordinary_child, ordinary_pack);
            let ordinary = ordinary_input.recursive_type_normalized_impl(
                &ordinary_db,
                &ordinary_env,
                ordinary_marker,
                nested,
            );
            assert_eq!(
                display_result(&db, &env, actual),
                display_result(&ordinary_db, &ordinary_env, ordinary),
                "{case:?}, nested={nested}",
            );
        }
    }
}

#[test]
fn recursive_normalization_type_forms_propagate_collapse_at_the_root() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let divergent = marker(&prepared);
    let tuple = Type::heterogeneous_tuple(&db, &env, [divergent, Type::int_literal(1)]);
    let ordinary_db = fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ordinary_marker = marker(&ordinary_prepared);
    let ordinary_tuple = Type::heterogeneous_tuple(
        &ordinary_db,
        &ordinary_env,
        [ordinary_marker, Type::int_literal(1)],
    );
    for (ty, ordinary_ty, collapses) in [
        (divergent, ordinary_marker, true),
        (tuple, ordinary_tuple, true),
        (
            TypeFormType::from_type_expression(&db, divergent),
            TypeFormType::from_type_expression(&ordinary_db, ordinary_marker),
            true,
        ),
        (Type::int_literal(1), Type::int_literal(1), false),
    ] {
        let form = TypeFormType::from_type_expression(&db, ty);
        let ordinary_form = TypeFormType::from_type_expression(&ordinary_db, ordinary_ty);
        for nested in [false, true] {
            let expected = (!collapses).then_some(form);
            assert_eq!(
                controlled(
                    &prepared,
                    &env,
                    RecursiveNormalizationRequest {
                        ty: form,
                        divergent,
                        nested
                    },
                    &funded(),
                ),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            let ordinary = ordinary_form.recursive_type_normalized_impl(
                &ordinary_db,
                &ordinary_env,
                ordinary_marker,
                nested,
            );
            assert_eq!(
                display_result(&db, &env, expected),
                display_result(&ordinary_db, &ordinary_env, ordinary),
            );
            assert_no_active_attempt();
        }
    }
}

#[test]
fn recursive_normalization_stops_before_later_unsupported_children() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let divergent = marker(&prepared);
    let unsupported = Type::Union(UnionType::new(
        &db,
        vec![Type::int_literal(1), Type::int_literal(2)].into_boxed_slice(),
        RecursivelyDefined::No,
    ));
    for spec in [
        TupleSpec::heterogeneous([divergent, unsupported]),
        VariableLengthTuple::mixed(
            [unsupported],
            VariableSegment::Homogeneous(divergent),
            [unsupported],
        )
        .into(),
        VariableLengthTuple::mixed(
            [divergent, unsupported],
            VariableSegment::Homogeneous(Type::int_literal(3)),
            [unsupported],
        )
        .into(),
        VariableLengthTuple::mixed(
            [Type::int_literal(3)],
            VariableSegment::Homogeneous(Type::int_literal(4)),
            [divergent, unsupported],
        )
        .into(),
    ] {
        let input = raw_tuple(&db, env.program(&db), spec);
        for nested in [true, false] {
            let result = controlled(
                &prepared,
                &env,
                RecursiveNormalizationRequest {
                    ty: input,
                    divergent,
                    nested,
                },
                &funded(),
            );
            assert_eq!(
                result,
                if nested {
                    Ok(AnalysisOutcome::Complete(None))
                } else {
                    Ok(unavailable(OperationId::Union))
                },
            );
            assert_no_active_attempt();
        }
    }
}

#[test]
fn recursive_normalization_resolves_the_caller_program_only_for_tuple_work() {
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
    let divergent = marker(&prepared);
    let Type::Divergent(marker) = divergent else {
        panic!("fixture divergent marker");
    };
    let materialized = Type::Divergent(marker.materialized(MaterializationKind::Bottom));
    assert_ne!(divergent, materialized);
    for (ty, nested, expected) in [
        (materialized, true, None),
        (materialized, false, Some(materialized)),
        (Type::unknown(), true, Some(Type::unknown())),
        (Type::object(), true, Some(Type::object())),
        (Type::int_literal(1), true, Some(Type::int_literal(1))),
    ] {
        assert_eq!(
            controlled(
                &prepared,
                &foreign_env,
                RecursiveNormalizationRequest {
                    ty,
                    divergent,
                    nested
                },
                &funded(),
            ),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_no_active_attempt();
    }

    let input = raw_tuple(&db, foreign, TupleSpec::homogeneous(Type::Never));
    assert_eq!(
        controlled(
            &prepared,
            &foreign_env,
            RecursiveNormalizationRequest {
                ty: input,
                divergent,
                nested: false
            },
            &funded(),
        ),
        Err(AnalysisFailure::Execution(RunError::Contract(
            "source program is foreign"
        ))),
    );
    let Ok(AnalysisOutcome::Complete(Some(actual))) = controlled(
        &prepared,
        &env,
        RecursiveNormalizationRequest {
            ty: input,
            divergent,
            nested: false,
        },
        &funded(),
    ) else {
        panic!("tuple normalization with the caller's program must complete");
    };
    let Type::NominalInstance(instance) = actual else {
        panic!("normalization preserves the exact tuple instance");
    };
    let tuple = instance.exact_tuple().unwrap();
    assert_eq!(tuple.program(&db), program);
    assert_eq!(tuple.tuple(&db), &TupleSpec::homogeneous(Type::Never));
    assert_ne!(actual, input);
    assert_no_active_attempt();
}

fn nested_input<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    divergent: Type<'db>,
) -> Type<'db> {
    Type::heterogeneous_tuple(
        db,
        env,
        [
            Type::int_literal(1),
            Type::heterogeneous_tuple(
                db,
                env,
                [Type::int_literal(2), divergent, Type::int_literal(3)],
            ),
            Type::int_literal(4),
        ],
    )
}

#[test]
fn recursive_normalization_interruption_drains_children_before_buffers_and_retries() {
    for kind in [
        StopKind::Work,
        StopKind::Bytes,
        StopKind::Cancel,
        StopKind::Panic,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let divergent = marker(&prepared);
        let input = nested_input(&db, &env, divergent);
        let request = RecursiveNormalizationRequest {
            ty: input,
            divergent,
            nested: false,
        };
        let revision = salsa::plumbing::current_revision(&db);
        let result = catch_unwind(AssertUnwindSafe(|| {
            controlled_with_stop(
                &prepared,
                &env,
                request,
                &funded(),
                Some(Stop { child: 3, kind }),
            )
        }));
        match (kind, result) {
            (StopKind::Work | StopKind::Bytes, Ok(result)) => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: if kind == StopKind::Work {
                        AnalysisIncomplete::WorkLimit
                    } else {
                        AnalysisIncomplete::RequestedAllocationLimit
                    },
                    completed: (),
                }),
            ),
            (StopKind::Cancel, Err(payload)) => {
                assert!(matches!(
                    payload.downcast_ref::<salsa::Cancelled>(),
                    Some(salsa::Cancelled::Local)
                ));
            }
            (StopKind::Panic, Err(payload)) => {
                assert_eq!(
                    payload.downcast_ref::<&str>(),
                    Some(&"recursive normalization child panic")
                );
            }
            (kind, result) => panic!("{kind:?}: unexpected interruption outcome {result:?}"),
        }
        assert_no_active_attempt();
        let snapshot = normalization_observations::snapshot();
        assert_eq!(
            (snapshot.children, snapshot.started, snapshot.dropped),
            (3, 3, 3)
        );
        assert_eq!(
            (
                snapshot.buffers,
                snapshot.live_buffers,
                snapshot.dropped_buffers
            ),
            (2, 0, 2)
        );
        let events = snapshot.events[..snapshot.event_count]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ChildEntered {
                child: 3,
                live_buffers: 2,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::ChildDropped {
                child: 3,
                live_buffers: 2
            }
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Event::BufferFinished { .. }))
        );
        let position = |expected| events.iter().position(|event| *event == expected).unwrap();
        assert!(
            position(Event::ChildDropped {
                child: 3,
                live_buffers: 2
            }) < position(Event::BufferDropped { buffer: 2 })
        );
        assert!(
            position(Event::BufferDropped { buffer: 2 })
                < position(Event::ChildDropped {
                    child: 2,
                    live_buffers: 1
                })
        );
        assert!(
            position(Event::ChildDropped {
                child: 2,
                live_buffers: 1
            }) < position(Event::BufferDropped { buffer: 1 })
        );
        for child in 1..=3 {
            let queued = events
                .iter()
                .find_map(|event| match event {
                    Event::ChildQueued {
                        child: actual,
                        environment,
                    } if *actual == child => Some(*environment),
                    _ => None,
                })
                .unwrap();
            let entered = events
                .iter()
                .find_map(|event| match event {
                    Event::ChildEntered {
                        child: actual,
                        environment,
                        ..
                    } if *actual == child => Some(*environment),
                    _ => None,
                })
                .unwrap();
            assert_ne!(queued, 0);
            assert_eq!(queued, entered);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        let Ok(AnalysisOutcome::Complete(Some(retried))) =
            controlled(&prepared, &env, request, &funded())
        else {
            panic!("same-revision normalization retry must complete");
        };
        assert_eq!(
            retried,
            Type::heterogeneous_tuple(
                &db,
                &env,
                [Type::int_literal(1), divergent, Type::int_literal(4)],
            ),
        );
        assert_ne!(retried, input);
        let snapshot = normalization_observations::snapshot();
        assert_eq!(
            (snapshot.children, snapshot.started, snapshot.dropped),
            (5, 5, 5)
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn recursive_normalization_refuses_after_buffer_completion_and_retries() {
    let measured_db = fixture();
    let measured_prepared = prepare(&measured_db);
    let measured_env = ProgramEnvironment::from_file(measured_prepared.program_file());
    let measured_marker = marker(&measured_prepared);
    let measured_input = nested_input(&measured_db, &measured_env, measured_marker);
    assert!(matches!(
        controlled(
            &measured_prepared,
            &measured_env,
            RecursiveNormalizationRequest {
                ty: measured_input,
                divergent: measured_marker,
                nested: false,
            },
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete(Some(_))),
    ));
    let completed_remaining = normalization_observations::snapshot()
        .events
        .into_iter()
        .flatten()
        .find_map(|event| match event {
            Event::BufferFinished {
                buffer: 1,
                remaining,
            } => remaining,
            _ => None,
        })
        .unwrap();

    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let divergent = marker(&prepared);
    let input = nested_input(&db, &env, divergent);
    let request = RecursiveNormalizationRequest {
        ty: input,
        divergent,
        nested: false,
    };
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled(
            &prepared,
            &env,
            request,
            &AnalysisPolicy {
                semantic_work_limit: funded().semantic_work_limit - completed_remaining,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    let snapshot = normalization_observations::snapshot();
    assert!(snapshot.events.into_iter().flatten().any(|event| matches!(
        event,
        Event::BufferFinished {
            buffer: 1,
            remaining: Some(0)
        },
    )));
    assert_eq!(
        (snapshot.children, snapshot.started, snapshot.dropped),
        (5, 5, 5)
    );
    assert_eq!(
        (
            snapshot.buffers,
            snapshot.live_buffers,
            snapshot.dropped_buffers
        ),
        (2, 0, 2)
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    let retry = controlled(&prepared, &env, request, &funded());
    assert_eq!(
        retry,
        Ok(AnalysisOutcome::Complete(Some(Type::heterogeneous_tuple(
            &db,
            &env,
            [Type::int_literal(1), divergent, Type::int_literal(4)],
        )))),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
