use std::panic::AssertUnwindSafe;

use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::types::narrow::admission::clone_constraint;
use crate::types::narrow::application::tests::{Shape, constraint};
use crate::types::narrow::application::{
    ApplicationEffects, ConjunctionApplication, Conjuncts, ConstraintApplication, Disjuncts,
};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::{NarrowingConstraint, UnionBuilder};

#[derive(Clone, Copy)]
enum Action {
    Union(Type<'static>, Type<'static>),
    CycleRecoveryUnion([Type<'static>; 2], RecursivelyDefined),
    Intersection(Type<'static>, Type<'static>),
    Apply(Shape),
    RetainApplication,
}

#[derive(Default)]
struct Progress {
    cancel_after_clone: bool,
    cancel_after_completion: bool,
    cancel_after_retention: bool,
    cloned: Cell<bool>,
    retired: Cell<bool>,
    completed: Cell<bool>,
    after_clone: Cell<Option<usize>>,
    retained: Cell<bool>,
    after_retention: Cell<Option<usize>>,
    application_retired: Cell<usize>,
}

struct ApplicationOwners<'owner, 'db> {
    disjuncts: Option<Disjuncts<'db>>,
    conjuncts: Option<Conjuncts<'db>>,
    union: Option<UnionBuilder<'db>>,
    retired: &'owner Cell<usize>,
}

impl Drop for ApplicationOwners<'_, '_> {
    fn drop(&mut self) {
        drop(self.disjuncts.take());
        drop(self.conjuncts.take());
        drop(self.union.take());
        self.retired.set(self.retired.get() + 1);
    }
}

struct OwnedConstraint<'owner, 'db> {
    value: NarrowingConstraint<'db>,
    retired: &'owner Cell<bool>,
}

impl Drop for OwnedConstraint<'_, '_> {
    fn drop(&mut self) {
        drop(std::mem::take(&mut self.value));
        self.retired.set(true);
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    action: Action,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    let input = match action {
        Action::Apply(shape) => constraint(shape),
        Action::RetainApplication => constraint(Shape::Retained),
        _ => NarrowingConstraint::default(),
    };
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
        let (values, input) = (&values, &input);
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            let result = match action {
                Action::Union(first, second) => {
                    access.union_from_two_elements(first, second).await?
                }
                Action::CycleRecoveryUnion(elements, recursively_defined) => {
                    let env = access
                        .endpoint
                        .local_call(|| {
                            access
                                .endpoint
                                .admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                            Ok(ProgramEnvironment::from_file(prepared.program_file()))
                        })
                        .await;
                    let union = effects.new_union(&env).await?;
                    let mut union = access
                        .endpoint
                        .local_call(|| {
                            access
                                .endpoint
                                .admit_work(size_of::<UnionBuilder<'db>>() * 2 + 2)?;
                            Ok(union
                                .cycle_recovery(true)
                                .or_recursively_defined(recursively_defined))
                        })
                        .await;
                    for ty in elements {
                        effects.union_add(&mut union, ty).await?;
                    }
                    effects.union_build(union).await?
                }
                Action::Intersection(first, second) => {
                    access.intersection_from_two_elements(first, second).await?
                }
                Action::Apply(_) => {
                    let mut owned = OwnedConstraint {
                        value: clone_constraint(&access.endpoint, input).await?,
                        retired: &progress.retired,
                    };
                    progress.cloned.set(true);
                    progress.after_clone.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db()),
                    );
                    if progress.cancel_after_clone {
                        access
                            .endpoint
                            .local_call(|| {
                                session.db().cancellation_token().cancel();
                                access.endpoint.check_completion()
                            })
                            .await;
                    }
                    let env = access
                        .endpoint
                        .local_call(|| {
                            access
                                .endpoint
                                .admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                            Ok(ProgramEnvironment::from_program(session.program()))
                        })
                        .await;
                    effects
                        .apply_narrowing_constraint(&env, std::mem::take(&mut owned.value))
                        .await?
                }
                Action::RetainApplication => {
                    let constraint = clone_constraint(&access.endpoint, input).await?;
                    let env = access
                        .endpoint
                        .local_call(|| {
                            access
                                .endpoint
                                .admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                            Ok(ProgramEnvironment::from_program(session.program()))
                        })
                        .await;
                    let ConstraintApplication::Combined(mut disjuncts) =
                        effects.start(constraint).await?
                    else {
                        panic!("fixture must retain disjunct storage");
                    };
                    let mut union = effects.new_union(&env).await?;
                    let first = effects.next_disjunct(&mut disjuncts).await?.unwrap();
                    let ty = effects.conjunction(&env, first).await?;
                    effects.union_add(&mut union, ty).await?;
                    assert_eq!(union.elements_storage().0, 1);
                    let next = effects.next_disjunct(&mut disjuncts).await?.unwrap();
                    let ConjunctionApplication::Fold(conjuncts) =
                        effects.start_conjunction(next).await?
                    else {
                        panic!("fixture must retain conjunction storage");
                    };
                    let owners = ApplicationOwners {
                        disjuncts: Some(disjuncts),
                        conjuncts: Some(conjuncts),
                        union: Some(union),
                        retired: &progress.application_retired,
                    };
                    progress.retained.set(true);
                    progress.after_retention.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db()),
                    );
                    access
                        .endpoint
                        .local_call(|| {
                            if progress.cancel_after_retention {
                                session.db().cancellation_token().cancel();
                            }
                            access.endpoint.admit_work(256)?;
                            access.endpoint.check_completion()
                        })
                        .await;
                    drop(owners);
                    Type::Never
                }
            };
            progress.completed.set(true);
            if progress.cancel_after_completion {
                access
                    .endpoint
                    .local_call(|| {
                        session.db().cancellation_token().cancel();
                        access.endpoint.check_completion()
                    })
                    .await;
            }
            Ok(result)
        })
    })
}

#[test]
fn canonical_pair_unions_keep_ordered_keys_and_ordinary_reuse() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    for (first, second, expected) in [
        (Type::Never, Type::AlwaysTruthy, Type::AlwaysTruthy),
        (Type::AlwaysTruthy, Type::Never, Type::AlwaysTruthy),
        (Type::Never, Type::Never, Type::Never),
        (Type::unknown(), Type::Never, Type::unknown()),
        (Type::AlwaysTruthy, Type::AlwaysTruthy, Type::AlwaysTruthy),
        (Type::object(), Type::AlwaysTruthy, Type::object()),
        (Type::AlwaysTruthy, Type::object(), Type::object()),
        (
            Type::bool_literal(true),
            Type::bool_literal(true),
            Type::bool_literal(true),
        ),
        (
            Type::bool_literal(false),
            Type::bool_literal(false),
            Type::bool_literal(false),
        ),
    ] {
        assert_eq!(
            controlled(
                &prepared,
                Action::Union(first, second),
                &funded(),
                &Progress::default()
            ),
            Ok(AnalysisOutcome::Complete(expected))
        );
        let key = TypePair::new(&db, env.program(&db), first, second);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                union_from_two_elements_ingredient(&db),
                key.as_id()
            )
            .is_ok()
        );
        events.take_salsa_events();
        assert_eq!(
            UnionType::from_two_elements(&db, &env, first, second),
            expected
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "union_from_two_elements",
            None,
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    assert_ne!(
        TypePair::new(&db, env.program(&db), Type::Never, Type::AlwaysTruthy),
        TypePair::new(&db, env.program(&db), Type::AlwaysTruthy, Type::Never)
    );
}

#[test]
fn cycle_recovery_unions_preserve_order_recursion_and_interned_identity() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    let mut unions = Vec::new();
    for recursively_defined in [RecursivelyDefined::No, RecursivelyDefined::Yes] {
        for elements in [
            [Type::unknown(), Type::AlwaysTruthy],
            [Type::AlwaysTruthy, Type::unknown()],
        ] {
            let action = Action::CycleRecoveryUnion(elements, recursively_defined);
            let result = controlled(&prepared, action, &funded(), &Progress::default());
            let Ok(AnalysisOutcome::Complete(Type::Union(union))) = result else {
                panic!("cycle-recovery union did not complete: {result:?}");
            };
            assert_eq!(union.elements(&db), &elements);
            assert_eq!(union.recursively_defined(&db), recursively_defined);
            assert!(!unions.contains(&union));
            unions.push(union);
            assert_no_active_attempt();

            // The ordinary construction follows controlled interning so it cannot supply
            // the union's canonical identity before the controlled result is produced.
            let mut ordinary = UnionBuilder::new(&db, &env)
                .cycle_recovery(true)
                .or_recursively_defined(recursively_defined);
            for ty in elements {
                ordinary.add_in_place(ty);
            }
            assert_eq!(ordinary.build(), Type::Union(union));
            assert_eq!(
                controlled(&prepared, action, &funded(), &Progress::default()),
                Ok(AnalysisOutcome::Complete(Type::Union(union)))
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn application_preserves_supported_shapes_and_normalization_boundaries() {
    for shape in [
        Shape::Empty,
        Shape::Intersection,
        Shape::Replacement,
        Shape::SingletonFiltering,
        Shape::SpilledNever,
        Shape::Pair,
        Shape::AtomicObject,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let progress = Progress::default();
        let result = controlled(&prepared, Action::Apply(shape), &funded(), &progress);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let expected = constraint(shape).evaluate_constraint_type(&db, &env);
        assert_eq!(result, Ok(AnalysisOutcome::Complete(expected)), "{shape:?}");
        assert!(progress.cloned.get() && progress.retired.get());
        assert_no_active_attempt();
    }
    let db = fixture();
    let prepared = prepare(&db);
    let result = controlled(
        &prepared,
        Action::Apply(Shape::ReplacementFirst),
        &funded(),
        &Progress::default(),
    );
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::UnavailableOperation(OperationId::Narrowing),
            completed: ()
        }),
    );
    assert_no_active_attempt();
}

#[test]
fn pair_descendants_publish_only_completed_results() {
    for (action, operation) in [
        (
            Action::Intersection(Type::Never, Type::AlwaysTruthy),
            None,
        ),
        (
            Action::Union(Type::Never, Type::int_literal(1)),
            Some(OperationId::Union),
        ),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        for _ in 0..2 {
            let expected = if let Some(operation) = operation {
                AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(operation),
                    completed: (),
                }
            } else {
                AnalysisOutcome::Complete(Type::Never)
            };
            assert_eq!(
                controlled(&prepared, action, &funded(), &Progress::default()),
                Ok(expected)
            );
            let program = prepared.program_file().program(&db);
            match action {
                Action::Union(first, second) => {
                    let key = TypePair::new(&db, program, first, second);
                    assert!(
                        FinalSourceMemo::certify(
                            &db as &dyn Db,
                            union_from_two_elements_ingredient(&db),
                            key.as_id()
                        )
                        .is_err()
                    );
                }
                Action::Intersection(first, second) => {
                    let key = TypePair::new(&db, program, first, second);
                    assert!(
                        FinalSourceMemo::certify(
                            &db as &dyn Db,
                            intersection_from_two_elements_ingredient(&db),
                            key.as_id()
                        )
                        .is_ok()
                    );
                }
                Action::CycleRecoveryUnion(..) | Action::Apply(_) | Action::RetainApplication => {
                    unreachable!()
                }
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn application_interruptions_retire_spilled_input_and_allow_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let measured = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            Action::Apply(Shape::SpilledNever),
            &funded(),
            &measured
        ),
        Ok(AnalysisOutcome::Complete(Type::Never))
    );
    let after_clone = measured.after_clone.get().unwrap();
    let policy = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - after_clone + 128,
        ..funded()
    };
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            Action::Apply(Shape::SpilledNever),
            &policy,
            &progress
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert!(progress.cloned.get() && progress.retired.get() && !progress.completed.get());
    assert_no_active_attempt();
    let progress = Progress {
        cancel_after_clone: true,
        ..Progress::default()
    };
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(
            &prepared,
            Action::Apply(Shape::SpilledNever),
            &funded(),
            &progress,
        )
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert!(progress.cloned.get() && progress.retired.get() && !progress.completed.get());
    assert_no_active_attempt();
    assert_eq!(
        controlled(
            &prepared,
            Action::Apply(Shape::SpilledNever),
            &funded(),
            &Progress::default()
        ),
        Ok(AnalysisOutcome::Complete(Type::Never))
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn retained_application_states_are_destroyed_before_same_revision_retry() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let measured = Progress::default();
    assert_eq!(
        controlled(&prepared, Action::RetainApplication, &funded(), &measured),
        Ok(AnalysisOutcome::Complete(Type::Never))
    );
    assert!(measured.retained.get());
    assert_eq!(measured.application_retired.get(), 1);
    let after_retention = measured.after_retention.get().unwrap();
    let policy = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - after_retention + 128,
        ..funded()
    };
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, Action::RetainApplication, &policy, &progress),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert!(progress.retained.get() && !progress.completed.get());
    assert_eq!(progress.application_retired.get(), 1);
    assert_no_active_attempt();
    let progress = Progress {
        cancel_after_retention: true,
        ..Progress::default()
    };
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Action::RetainApplication, &funded(), &progress)
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert!(progress.retained.get() && !progress.completed.get());
    assert_eq!(progress.application_retired.get(), 1);
    assert_no_active_attempt();
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, Action::RetainApplication, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(Type::Never))
    );
    assert!(progress.retained.get() && progress.completed.get());
    assert_eq!(progress.application_retired.get(), 1);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn cancelled_pair_caller_retains_the_completed_canonical_child() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress {
        cancel_after_completion: true,
        ..Progress::default()
    };
    let action = Action::Union(Type::AlwaysTruthy, Type::AlwaysTruthy);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, action, &funded(), &progress)
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert!(progress.completed.get());
    assert_no_active_attempt();
    events.take_salsa_events();
    assert_eq!(
        controlled(&prepared, action, &funded(), &Progress::default()),
        Ok(AnalysisOutcome::Complete(Type::AlwaysTruthy))
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "union_from_two_elements",
        None,
        &events.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}
