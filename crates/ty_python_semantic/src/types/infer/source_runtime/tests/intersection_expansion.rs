use std::panic::AssertUnwindSafe;

use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::FxOrderSet;
use crate::types::NegativeIntersectionElements;
use crate::types::relation::redundancy_ingredient;
use crate::types::relation::source::disjointness_observations;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::set_theoretic::builder::intersection_expansion::{Sign, expansion_observations};
use crate::types::set_theoretic::builder::intersection_insertion::insertion_observations;
use crate::types::set_theoretic::builder::{
    IntersectionBuilder, intersection_simplification_ingredient,
};

#[derive(Clone, Copy)]
enum Action<'a, 'db> {
    Expand(&'a [(Type<'db>, Sign)], bool),
    Canonical(Type<'db>, Type<'db>),
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Signed,
    PositiveUnion,
    NegativeUnion,
    PositiveIntersection,
    NegativeIntersection,
}

#[derive(Default)]
struct Progress {
    completed_additions: Cell<usize>,
    dropped_branches: Cell<Option<usize>>,
    retired: Cell<bool>,
}

struct OwnedBuilder<'a, 'db> {
    builder: Option<IntersectionBuilder<'db>>,
    progress: &'a Progress,
}

impl Drop for OwnedBuilder<'_, '_> {
    fn drop(&mut self) {
        if let Some(builder) = self.builder.take() {
            let branches = builder.branches_storage().0.len();
            drop(builder);
            self.progress.dropped_branches.set(Some(branches));
        }
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    action: Action<'_, 'db>,
    policy: &AnalysisPolicy,
    progress: &Progress,
    output: &mut Option<IntersectionBuilder<'db>>,
) -> Result<AnalysisOutcome<()>, AnalysisFailure> {
    expansion_observations::reset();
    insertion_observations::reset();
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
        let output = &mut *output;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            if let Action::Canonical(first, second) = action {
                access.intersection_from_two_elements(first, second).await?;
                return Ok(());
            }
            let Action::Expand(additions, retire) = action else {
                return Err(RunError::Contract("expansion fixture has no additions"));
            };
            let effects = SourceEffects::new(&access, session.program());
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let mut owner = OwnedBuilder {
                builder: Some(effects.new_intersection(&env).await?),
                progress,
            };
            for &(ty, sign) in additions {
                let Some(builder) = &mut owner.builder else {
                    return Err(RunError::Contract("expansion fixture lost its builder"));
                };
                match sign {
                    Sign::Positive => effects.intersection_add_positive(builder, ty).await?,
                    Sign::Negative => effects.intersection_add_negative(builder, ty).await?,
                }
                progress
                    .completed_additions
                    .set(progress.completed_additions.get() + 1);
            }
            let Some(builder) = owner.builder.take() else {
                return Err(RunError::Contract("expansion fixture lost its builder"));
            };
            if retire {
                effects.retire_intersection(builder).await?;
                progress.retired.set(true);
            } else {
                *output = Some(builder);
            }
            Ok(())
        })
    })
}

fn union<'db>(db: &'db dyn Db, elements: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    Type::Union(UnionType::new(
        db,
        elements.into_iter().collect::<Box<[_]>>(),
        RecursivelyDefined::No,
    ))
}

fn signed_intersection(db: &dyn Db) -> Type<'_> {
    Type::Intersection(IntersectionType::new(
        db,
        FxOrderSet::from_iter([Type::unknown()]),
        NegativeIntersectionElements::Single(Type::AlwaysTruthy),
    ))
}

fn nested_additions(db: &dyn Db) -> [(Type<'_>, Sign); 2] {
    let inner = union(db, [Type::AlwaysTruthy, Type::Never]);
    let outer = union(db, [inner, Type::Never]);
    [(Type::unknown(), Sign::Positive), (outer, Sign::Positive)]
}

fn ordinary<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    additions: &[(Type<'db>, Sign)],
) -> IntersectionBuilder<'db> {
    let mut builder = IntersectionBuilder::new(db, env);
    for &(ty, sign) in additions {
        match sign {
            Sign::Positive => builder.add_positive_in_place(ty),
            Sign::Negative => builder.add_negative_in_place(ty),
        }
    }
    builder
}

fn assert_cleanup() {
    let progress = expansion_observations::progress();
    assert_eq!(progress.live_expansions, 0);
    assert_eq!(progress.live_parents, 0);
    assert_eq!(insertion_observations::progress().0, 0);
    assert_no_active_attempt();
}

fn simplification_key(db: &TestDb, events: &[salsa::Event]) -> salsa::Id {
    let Some(key) = events.iter().find_map(|event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind
            && db.ingredient_debug_name(database_key.ingredient_index())
                == "simplify_intersection_pair_impl"
        {
            Some(database_key.key_index())
        } else {
            None
        }
    }) else {
        panic!("canonical intersection did not execute its simplification child");
    };
    key
}

#[test]
fn signed_union_and_intersection_expansion_matches_ordinary_branches() {
    for shape in [
        Shape::Signed,
        Shape::PositiveUnion,
        Shape::NegativeUnion,
        Shape::PositiveIntersection,
        Shape::NegativeIntersection,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let additions = match shape {
            Shape::Signed => vec![
                (Type::unknown(), Sign::Negative),
                (Type::AlwaysTruthy, Sign::Positive),
            ],
            Shape::PositiveUnion => vec![(
                union(&db, [Type::unknown(), Type::AlwaysTruthy]),
                Sign::Positive,
            )],
            Shape::NegativeUnion => vec![(
                union(&db, [Type::unknown(), Type::AlwaysTruthy]),
                Sign::Negative,
            )],
            Shape::PositiveIntersection => vec![(signed_intersection(&db), Sign::Positive)],
            Shape::NegativeIntersection => vec![(signed_intersection(&db), Sign::Negative)],
        };
        let mut output = None;
        assert_eq!(
            controlled(
                &prepared,
                Action::Expand(&additions, false),
                &funded(),
                &Progress::default(),
                &mut output
            ),
            Ok(AnalysisOutcome::Complete(())),
            "{shape:?}",
        );
        assert_cleanup();
        let Some(output) = output else {
            panic!("completed expansion did not return its builder");
        };
        // Ordinary evaluation follows completion so it cannot supply semantic child memos.
        let expected = ordinary(&db, &env, &additions);
        assert_eq!(output.branches_storage().0, expected.branches_storage().0);
        assert_eq!(output.has_disjunction(), expected.has_disjunction());
        assert_cleanup();
    }
}

#[test]
fn distribution_preserves_order_and_deduplicates_complete_branches() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = union(
        &db,
        [
            Type::unknown(),
            Type::AlwaysTruthy,
            Type::unknown(),
            Type::Never,
        ],
    );
    let second = union(&db, [Type::unknown(), Type::unknown()]);
    let additions = [(first, Sign::Positive), (second, Sign::Positive)];
    let mut output = None;
    assert_eq!(
        controlled(
            &prepared,
            Action::Expand(&additions, false),
            &funded(),
            &Progress::default(),
            &mut output
        ),
        Ok(AnalysisOutcome::Complete(())),
    );
    assert_cleanup();
    let Some(output) = output else {
        panic!("completed distribution did not return its builder");
    };
    let expected = ordinary(&db, &env, &additions);
    assert_eq!(output.branches_storage().0, expected.branches_storage().0);
    assert!(output.has_disjunction());
    let branches = output.branches_storage().0;
    assert_eq!(branches.len(), 2);
    let unknown = ordinary(&db, &env, &[(Type::unknown(), Sign::Positive)]);
    let truthy_unknown = ordinary(
        &db,
        &env,
        &[
            (Type::AlwaysTruthy, Sign::Positive),
            (Type::unknown(), Sign::Positive),
        ],
    );
    assert_eq!(&branches[0..1], unknown.branches_storage().0);
    assert_eq!(&branches[1..2], truthy_unknown.branches_storage().0);
    assert_cleanup();
}

#[test]
fn retained_nested_parents_retire_after_work_refusal_and_native_cancellation() {
    let measured = fixture();
    let prepared = prepare(&measured);
    disjointness_observations::reset(false);
    assert_eq!(
        controlled(
            &prepared,
            Action::Expand(&nested_additions(&measured), true),
            &funded(),
            &Progress::default(),
            &mut None
        ),
        Ok(AnalysisOutcome::Complete(())),
    );
    let Some(remaining) = disjointness_observations::progress().2 else {
        panic!("nested expansion did not enter disjointness");
    };
    let retained_work = funded().semantic_work_limit - remaining;
    let observed = expansion_observations::progress();
    assert!(observed.peak_parents >= 2 && observed.spilled_frames);
    assert_cleanup();

    for cancel in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let additions = nested_additions(&db);
        let progress = Progress::default();
        disjointness_observations::reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                Action::Expand(&additions, true),
                &policy,
                &progress,
                &mut None,
            )
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            ),
            other => panic!("{other:?}"),
        }
        let observed = expansion_observations::progress();
        assert!(observed.peak_parents >= 2 && observed.spilled_frames);
        assert_eq!(progress.completed_additions.get(), 1);
        assert_eq!(progress.dropped_branches.get(), Some(1));
        assert!(!progress.retired.get());
        assert_cleanup();

        disjointness_observations::reset(false);
        let retry = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                Action::Expand(&additions, true),
                &funded(),
                &retry,
                &mut None
            ),
            Ok(AnalysisOutcome::Complete(())),
        );
        assert!(retry.retired.get());
        assert_eq!(retry.completed_additions.get(), 2);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn allocation_refusal_retires_nested_parents_before_same_revision_retry() {
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let db = fixture();
        let prepared = prepare(&db);
        let result = controlled(
            &prepared,
            Action::Expand(&nested_additions(&db), true),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &Progress::default(),
            &mut None,
        );
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(()))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        ..
                    })
            ),
            "{result:?}"
        );
        if expansion_observations::progress().peak_parents >= 2 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_cleanup();
    }
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let additions = nested_additions(&db);
    let progress = Progress::default();
    // This quota admits both parent owners but refuses subsequent continuation or child growth.
    assert_eq!(
        controlled(
            &prepared,
            Action::Expand(&additions, true),
            &AnalysisPolicy {
                requested_bytes_limit: upper,
                ..funded()
            },
            &progress,
            &mut None
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        }),
    );
    assert!(expansion_observations::progress().peak_parents >= 2);
    assert_eq!(progress.completed_additions.get(), 1);
    assert_eq!(progress.dropped_branches.get(), Some(1));
    assert!(!progress.retired.get());
    assert_cleanup();

    let retry = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            Action::Expand(&additions, true),
            &funded(),
            &retry,
            &mut None
        ),
        Ok(AnalysisOutcome::Complete(())),
    );
    assert!(retry.retired.get());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn canonical_pair_completes_and_reuses_completed_children() {
    let db = fixture();
    let mut events = db.clone();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let first = Type::unknown();
    let second = Type::AlwaysTruthy;
    let mut child = None;
    for attempt in 0..2 {
        events.take_salsa_events();
        assert_eq!(
            controlled(
                &prepared,
                Action::Canonical(first, second),
                &funded(),
                &Progress::default(),
                &mut None
            ),
            Ok(AnalysisOutcome::Complete(())),
        );
        let observed = expansion_observations::progress();
        assert_eq!(observed.entered_expansions, if attempt == 0 { 2 } else { 0 });
        assert_cleanup();
        let events = events.take_salsa_events();
        if attempt == 0 {
            child = Some(simplification_key(&db, &events));
        } else {
            assert_function_query_was_not_run_by_name(
                &db,
                "simplify_intersection_pair_impl",
                None,
                &events,
            );
            assert_function_query_was_not_run_by_name(&db, "is_redundant_with_impl", None, &events);
        }
        let Some(child) = child else {
            panic!("canonical intersection has no simplification child");
        };
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_simplification_ingredient(&db),
                child
            )
            .is_ok()
        );
        let program = prepared.program_file().program(&db);
        let pair = TypePair::new(&db, program, first, second);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                intersection_from_two_elements_ingredient(&db),
                pair.as_id()
            )
            .is_ok()
        );
        for (first, second) in [(first, second), (second, first)] {
            let pair = TypePair::new(&db, program, first, second);
            assert!(
                FinalSourceMemo::certify(&db as &dyn Db, redundancy_ingredient(&db), pair.as_id())
                    .is_ok()
            );
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}
