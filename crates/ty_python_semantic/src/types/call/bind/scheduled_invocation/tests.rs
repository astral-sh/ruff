use super::*;
use crate::db::tests::setup_db;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};

fn callback<'db>(db: &'db dyn Db, result: Type<'db>) -> Type<'db> {
    Type::Callable(CallableType::single(
        db,
        Signature::new(Parameters::empty(), result),
    ))
}

fn overload<'db>(db: &'db dyn Db, expected: Type<'db>, result: i64) -> Signature<'db> {
    Signature::new(
        Parameters::standard([
            Parameter::positional_only(None).with_annotated_type(callback(db, expected))
        ]),
        Type::int_literal(result),
    )
}

fn overloaded<'db>(db: &'db dyn Db, first: Option<Type<'db>>) -> CallableType<'db> {
    let signatures = first
        .into_iter()
        .map(|expected| overload(db, expected, 10))
        .chain([overload(db, Type::any(), 20)]);
    CallableType::new(
        db,
        CallableSignature::from_overloads(signatures),
        CallableTypeKind::Regular,
    )
}

fn policy(allowance: usize, reverse_execution: bool, reverse_merge: bool) -> InvocationPolicy {
    InvocationPolicy {
        preparation_allowance: 128,
        scheduler_allowance: allowance,
        reverse_execution,
        reverse_merge,
    }
}

#[test]
fn scheduled_invocation_blocks_later_overload_on_unresolved_callback_relation() -> anyhow::Result<()>
{
    let db = setup_db();
    let env = db.program_environment();
    let argument = callback(&db, Type::int_literal(1));
    let callable = overloaded(&db, Some(Type::object()));
    let first_parameter = callback(&db, Type::object());
    let constraints = ConstraintSetBuilder::new();
    let arguments = CallArguments::positional([argument]);
    let mut synchronous = CallableBinding::from_overloads(
        Type::Callable(callable),
        callable.signatures(&db).overloads.iter().cloned(),
    );
    synchronous.match_parameters(&db, &env, &arguments);
    synchronous.check_types(&db, &env, &constraints, &arguments, TypeContext::default());
    assert_eq!(synchronous.return_type(), Type::int_literal(10));
    assert!(synchronous.as_result().is_ok());

    for reverse_execution in [false, true] {
        for reverse_merge in [false, true] {
            for allowance in [0, 1, 2, 4, 8, 16, 32, 64] {
                let constraints = ConstraintSetBuilder::new();
                let router = Router::with_constraints(&constraints);
                let result = run_invocation(
                    &db,
                    &env,
                    &constraints,
                    &router,
                    callable,
                    &[argument],
                    policy(allowance, reverse_execution, reverse_merge),
                )
                .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))?;
                let InvocationCompletion::Incomplete {
                    boundary,
                    argument: pending,
                } = result.completion
                else {
                    anyhow::bail!("an unresolved earlier overload published bindings");
                };
                assert!(result.completed_arguments.is_empty());
                assert!(pending.is_none_or(|pending| {
                    pending
                        == ArgumentDependency {
                            source: argument,
                            target: first_parameter,
                        }
                }));
                if allowance == 64 {
                    assert_eq!(
                        boundary,
                        Some(InvocationBoundary::Relation(Boundary::SemanticOperation)),
                    );
                    assert!(pending.is_some());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn scheduled_invocation_controls_use_the_same_overload_loop() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let argument = callback(&db, Type::int_literal(1));
    for first in [None, Some(Type::int_literal(2))] {
        let callable = overloaded(&db, first);
        let expected_index = usize::from(first.is_some());
        let constraints = ConstraintSetBuilder::new();
        let arguments = CallArguments::positional([argument]);
        let mut synchronous = CallableBinding::from_overloads(
            Type::Callable(callable),
            callable.signatures(&db).overloads.iter().cloned(),
        );
        synchronous.match_parameters(&db, &env, &arguments);
        synchronous.check_types(&db, &env, &constraints, &arguments, TypeContext::default());
        assert_eq!(synchronous.return_type(), Type::int_literal(20));
        assert!(synchronous.as_result().is_ok());

        for reverse_execution in [false, true] {
            for reverse_merge in [false, true] {
                let mut previous_evidence = Vec::new();
                let mut completed_at = None;
                let mut observed_cancelled_partial_check = false;
                for allowance in 0..=32 {
                    let constraints = ConstraintSetBuilder::new();
                    let router = Router::with_constraints(&constraints);
                    let result = run_invocation(
                        &db,
                        &env,
                        &constraints,
                        &router,
                        callable,
                        &[argument],
                        policy(allowance, reverse_execution, reverse_merge),
                    )
                    .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))?;
                    assert!(result.scheduler_work <= allowance);
                    assert!(result.completed_arguments.starts_with(&previous_evidence));
                    previous_evidence = result.completed_arguments.to_vec();
                    match result.completion {
                        InvocationCompletion::Complete(binding) => {
                            completed_at.get_or_insert(allowance);
                            assert!(binding.as_result().is_ok());
                            assert_eq!(binding.return_type(), synchronous.return_type());
                            assert_eq!(
                                binding
                                    .selected_overloads()
                                    .map(|(index, _)| index)
                                    .collect::<Vec<_>>(),
                                [expected_index],
                            );
                            assert_eq!(result.completed_arguments.len(), expected_index + 1);
                            assert!(
                                !result.completed_arguments[expected_index].is_never_assignable
                            );
                            if first.is_some() {
                                assert!(result.completed_arguments[0].is_never_assignable);
                                assert_eq!(binding.overloads()[0].errors().len(), 1);
                            }
                        }
                        InvocationCompletion::Incomplete {
                            boundary,
                            argument: pending,
                        } => {
                            if first.is_some()
                                && result.completed_arguments.len() == 1
                                && pending.is_some()
                            {
                                assert_eq!(
                                    pending,
                                    Some(ArgumentDependency {
                                        source: argument,
                                        target: callback(&db, Type::any()),
                                    })
                                );
                                assert_eq!(
                                    result.completed_arguments[0].dependency,
                                    ArgumentDependency {
                                        source: argument,
                                        target: callback(&db, Type::int_literal(2)),
                                    }
                                );
                                observed_cancelled_partial_check = true;
                            }
                            assert!(completed_at.is_none());
                            assert_eq!(boundary, None);
                        }
                    }
                }
                assert!(completed_at.is_some());
                assert_eq!(observed_cancelled_partial_check, first.is_some());
            }
        }
    }
    Ok(())
}

#[test]
fn scheduled_invocation_preparation_is_bounded_before_argument_relations() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let argument = callback(&db, Type::int_literal(1));
    let callable = overloaded(&db, None);
    for preparation_allowance in [0, 1, 2, 4] {
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        let result = run_invocation(
            &db,
            &env,
            &constraints,
            &router,
            callable,
            &[argument],
            InvocationPolicy {
                preparation_allowance,
                ..policy(64, false, false)
            },
        )
        .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))?;
        assert!(matches!(
            result.completion,
            InvocationCompletion::Incomplete {
                boundary: Some(InvocationBoundary::PreparationAllowance),
                argument: None,
            }
        ));
        assert!(result.completed_arguments.is_empty());
        assert_eq!(result.preparation_work, preparation_allowance);
    }
    Ok(())
}

#[test]
fn scheduled_invocation_rejects_unsupported_legacy_paths() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let argument = callback(&db, Type::int_literal(1));
    let regular = overloaded(&db, None);
    let function_like = regular.with_kind(&db, CallableTypeKind::FunctionLike);
    let variadic = CallableType::single(
        &db,
        Signature::new(Parameters::gradual_form(), Type::int_literal(20)),
    );
    for callable in [function_like, variadic] {
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        let result = run_invocation(
            &db,
            &env,
            &constraints,
            &router,
            callable,
            &[argument],
            policy(64, false, false),
        )
        .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))?;
        assert!(matches!(
            result.completion,
            InvocationCompletion::Incomplete {
                boundary: Some(InvocationBoundary::Preparation),
                argument: None,
            }
        ));
        assert!(result.completed_arguments.is_empty());
    }

    // Both candidates accept the argument, so the ordinary algorithm reaches step 5.
    let callable = overloaded(&db, Some(Type::any()));
    let constraints = ConstraintSetBuilder::new();
    let router = Router::with_constraints(&constraints);
    let result = run_invocation(
        &db,
        &env,
        &constraints,
        &router,
        callable,
        &[argument],
        policy(64, false, false),
    )
    .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))?;
    assert!(matches!(
        result.completion,
        InvocationCompletion::Incomplete {
            boundary: Some(InvocationBoundary::Legacy(
                BinderLegacyEffect::OverloadFiltering
            )),
            argument: None,
        }
    ));
    assert_eq!(result.completed_arguments.len(), 2);
    Ok(())
}

#[test]
fn scheduled_invocation_reserves_matching_before_many_overloads_and_arguments() -> anyhow::Result<()>
{
    let db = setup_db();
    let env = db.program_environment();
    let overload_count = 64;
    let parameter_count = 8;
    let argument_count = 128;
    let signature =
        Signature::new(
            Parameters::standard((0..parameter_count).map(|_| {
                Parameter::positional_only(None).with_annotated_type(Type::int_literal(2))
            })),
            Type::int_literal(10),
        );
    let callable = CallableType::new(
        &db,
        CallableSignature::from_overloads(std::iter::repeat_n(signature, overload_count)),
        CallableTypeKind::Regular,
    );
    let arguments = vec![Type::Never; argument_count];
    let run = |preparation_allowance| {
        let constraints = ConstraintSetBuilder::new();
        let router = Router::with_constraints(&constraints);
        run_invocation(
            &db,
            &env,
            &constraints,
            &router,
            callable,
            &arguments,
            InvocationPolicy {
                preparation_allowance,
                ..policy(64, false, false)
            },
        )
        .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))
    };

    // Inspecting the signatures fits; matching every excess argument against every overload does not.
    let limited = run(4_096)?;
    assert!(matches!(
        limited.completion,
        InvocationCompletion::Incomplete {
            boundary: Some(InvocationBoundary::PreparationAllowance),
            argument: None,
        }
    ));
    assert!(limited.preparation_work < 4_096);
    assert!(limited.completed_arguments.is_empty());

    let complete = run(100_000)?;
    let InvocationCompletion::Complete(binding) = complete.completion else {
        anyhow::bail!("the matching reservation should fit");
    };
    assert!(binding.as_result().is_err());
    assert_eq!(binding.overloads().len(), overload_count);
    assert_eq!(binding.matching_overloads().count(), 0);
    assert!(complete.completed_arguments.is_empty());
    assert_eq!(
        complete.preparation_work - limited.preparation_work,
        (argument_count + 1) * overload_count * (parameter_count + 1),
    );
    assert!(complete.preparation_work <= 100_000);
    Ok(())
}

#[test]
fn scheduled_invocation_matching_reservation_reports_overflow() {
    for (argument_count, parameter_counts) in [
        (usize::MAX, vec![0]),
        (0, vec![usize::MAX]),
        (0, vec![usize::MAX - 1, 0]),
        (1, vec![usize::MAX / 2]),
    ] {
        assert_eq!(
            matching_preparation_work(argument_count, parameter_counts),
            Err(InvocationBoundary::PreparationCostOverflow),
        );
    }
}

#[test]
fn scheduled_invocation_completes_when_every_callback_overload_is_rejected() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let callable = CallableType::new(
        &db,
        CallableSignature::from_overloads([
            overload(&db, Type::int_literal(2), 10),
            overload(&db, Type::int_literal(3), 20),
        ]),
        CallableTypeKind::Regular,
    );
    let argument = callback(&db, Type::int_literal(1));
    let constraints = ConstraintSetBuilder::new();
    let arguments = CallArguments::positional([argument]);
    let mut synchronous = CallableBinding::from_overloads(
        Type::Callable(callable),
        callable.signatures(&db).overloads.iter().cloned(),
    );
    synchronous.match_parameters(&db, &env, &arguments);
    synchronous.check_types(&db, &env, &constraints, &arguments, TypeContext::default());
    assert!(synchronous.as_result().is_err());

    for reverse_execution in [false, true] {
        for reverse_merge in [false, true] {
            let constraints = ConstraintSetBuilder::new();
            let router = Router::with_constraints(&constraints);
            let result = run_invocation(
                &db,
                &env,
                &constraints,
                &router,
                callable,
                &[argument],
                policy(64, reverse_execution, reverse_merge),
            )
            .map_err(|error| anyhow::anyhow!("session boundary: {error:?}"))?;
            let InvocationCompletion::Complete(binding) = result.completion else {
                anyhow::bail!("two completed rejections should produce a completed failed call");
            };
            assert!(binding.as_result().is_err());
            assert_eq!(binding.return_type(), synchronous.return_type());
            assert_eq!(binding.matching_overloads().count(), 0);
            assert!(
                binding
                    .overloads()
                    .iter()
                    .all(|overload| overload.errors().len() == 1)
            );
            assert_eq!(result.completed_arguments.len(), 2);
            assert!(
                result
                    .completed_arguments
                    .iter()
                    .all(|evidence| evidence.is_never_assignable)
            );
        }
    }
    Ok(())
}
