use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;
use salsa::prepared_source_probe;

use super::*;
use crate::FxOrderSet;
use crate::types::NegativeIntersectionElements;
use crate::types::cyclic::{TypeTransformationStorage, TypeTransformationWork};
use crate::types::mapping::materialization::MaterializationConfiguration;
use crate::types::mapping::source::observations as materialization_observations;
use crate::types::mapping::source::observations::{
    CleanupBoundary, CleanupDrop, Lookup, OwnedMappingSnapshot, SetCleanupEvent, SetKind,
};
use crate::types::set_theoretic::RecursivelyDefined;

#[derive(Default)]
struct Progress {
    pools: Cell<Option<[usize; 5]>>,
    remaining: Cell<Option<usize>>,
    retired: Cell<bool>,
    attempt_count: Cell<usize>,
    attempts: Cell<[Option<AttemptProgress>; 8]>,
}

#[derive(Clone, Copy, Debug)]
struct AttemptProgress {
    first_root: usize,
    roots: usize,
    first_child: usize,
    children: usize,
    pools: [usize; 5],
    partial_builders: usize,
}

struct Retirement<'a>(&'a Cell<bool>);

impl Drop for Retirement<'_> {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    kind: MaterializationKind,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let before = materialization_observations::snapshot();
        let builders_before = materialization_observations::set_snapshot();
        progress.retired.set(false);
        let _retirement = Retirement(&progress.retired);
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
                    .cached_materialization(session.program(), ty, kind)
                    .await
            })
        }));
        let pools = [
                environments.retained_payload(),
                builders.retained_payload(),
                owners.retained_payload(),
                mapping.retained_payload(),
                checkers.retained_payload(),
            ]
            .map(|payload| payload.unwrap().0);
        progress.pools.set(Some(pools));
        let after = materialization_observations::snapshot();
        let builders_after = materialization_observations::set_snapshot();
        assert_eq!(builders_after.live, 0);
        assert_eq!(
            builders_after.dropped - builders_before.dropped,
            builders_after.created - builders_before.created,
        );
        let mut attempts = progress.attempts.get();
        attempts[progress.attempt_count.get()] = Some(AttemptProgress {
            first_root: before.root_count,
            roots: after.root_count - before.root_count,
            first_child: before.child_count,
            children: after.child_count - before.child_count,
            pools,
            partial_builders: builders_after.partial_drops - builders_before.partial_drops,
        });
        progress.attempts.set(attempts);
        progress.attempt_count.set(progress.attempt_count.get() + 1);
        progress
            .remaining
            .set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                session.db(),
            ));
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

fn wrapped<'db>(db: &'db dyn Db, mut ty: Type<'db>, depth: usize) -> Type<'db> {
    for _ in 0..depth {
        ty = TypeFormType::from_type_expression(db, ty);
    }
    ty
}

#[derive(Clone, Copy, Debug)]
enum TupleCase {
    Empty,
    Fixed,
    Homogeneous,
    Prefix,
    Suffix,
    Mixed,
    NestedTuple,
    NestedTypeForm,
    UnchangedFixed,
    UnchangedVariable,
}

impl TupleCase {
    fn value<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        gradual: Type<'db>,
    ) -> Type<'db> {
        let first = Type::bool_literal(true);
        let last = Type::bool_literal(false);
        match self {
            Self::Empty => Type::empty_tuple(db, env),
            Self::Fixed => Type::heterogeneous_tuple(db, env, [first, gradual, last]),
            Self::Homogeneous => Type::homogeneous_tuple(db, env, gradual),
            Self::Prefix => Type::tuple(TupleType::mixed(db, env, [first, last], gradual, [])),
            Self::Suffix => Type::tuple(TupleType::mixed(db, env, [], gradual, [last, first])),
            Self::Mixed => Type::tuple(TupleType::mixed(
                db,
                env,
                [first, last],
                gradual,
                [last, first],
            )),
            Self::NestedTuple => Type::heterogeneous_tuple(
                db,
                env,
                [
                    wrapped(db, gradual, 1),
                    Type::homogeneous_tuple(db, env, gradual),
                    Type::heterogeneous_tuple(db, env, [last, gradual]),
                ],
            ),
            Self::NestedTypeForm => wrapped(
                db,
                Type::heterogeneous_tuple(
                    db,
                    env,
                    [
                        gradual,
                        wrapped(db, Type::homogeneous_tuple(db, env, gradual), 1),
                    ],
                ),
                1,
            ),
            Self::UnchangedFixed => Type::heterogeneous_tuple(db, env, [first, last]),
            Self::UnchangedVariable => {
                Type::tuple(TupleType::mixed(db, env, [first], last, [first]))
            }
        }
    }

    fn children(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Fixed | Self::Prefix | Self::Suffix | Self::UnchangedVariable => 3,
            Self::Homogeneous => 1,
            Self::Mixed | Self::NestedTypeForm => 5,
            Self::NestedTuple => 7,
            Self::UnchangedFixed => 2,
        }
    }
}

fn set_union<'db>(
    db: &'db dyn Db,
    elements: impl IntoIterator<Item = Type<'db>>,
    recursively_defined: RecursivelyDefined,
) -> Type<'db> {
    Type::Union(UnionType::new(
        db,
        elements.into_iter().collect::<Box<[_]>>(),
        recursively_defined,
    ))
}

fn set_intersection<'db, const P: usize, const N: usize>(
    db: &'db dyn Db,
    positive: [Type<'db>; P],
    negative: [Type<'db>; N],
) -> Type<'db> {
    Type::Intersection(IntersectionType::new(
        db,
        FxOrderSet::from_iter(positive),
        match negative.as_slice() {
            [] => NegativeIntersectionElements::Empty,
            [ty] => NegativeIntersectionElements::Single(*ty),
            _ => NegativeIntersectionElements::Multiple(FxOrderSet::from_iter(negative)),
        },
    ))
}

#[derive(Clone, Copy, Debug)]
enum SetCase {
    UnchangedUnion,
    ChangedUnion,
    UnionParentMetadata,
    UnionChildMetadata,
    NestedTypeForm,
    NestedTuple,
    UnionTypeFormMember,
    UnionTupleMember,
    UnionIntersectionMember,
    PositiveIntersection,
    NegativeIntersection,
    MixedIntersection,
    MixedNegativeAny,
    NestedIntersection,
    NegativeTypeForm,
}

impl SetCase {
    fn value<'db>(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let first = Type::bool_literal(true);
        let last = Type::literal_string();
        match self {
            Self::UnchangedUnion => set_union(db, [first, last], RecursivelyDefined::Yes),
            Self::ChangedUnion => {
                set_union(db, [first, Type::any(), last], RecursivelyDefined::Yes)
            }
            Self::UnionParentMetadata | Self::UnionChildMetadata => set_union(
                db,
                [
                    Type::any(),
                    set_union(
                        db,
                        [first, Type::any(), last],
                        if matches!(self, Self::UnionChildMetadata) {
                            RecursivelyDefined::Yes
                        } else {
                            RecursivelyDefined::No
                        },
                    ),
                ],
                if matches!(self, Self::UnionParentMetadata) {
                    RecursivelyDefined::Yes
                } else {
                    RecursivelyDefined::No
                },
            ),
            Self::NestedTypeForm => wrapped(db, Self::ChangedUnion.value(db, env), 1),
            Self::NestedTuple => Type::heterogeneous_tuple(
                db,
                env,
                [
                    Self::ChangedUnion.value(db, env),
                    wrapped(db, Type::any(), 1),
                ],
            ),
            Self::UnionTypeFormMember | Self::UnionTupleMember | Self::UnionIntersectionMember => {
                let argument = match self {
                    Self::UnionTupleMember => Type::heterogeneous_tuple(db, env, [Type::any()]),
                    Self::UnionIntersectionMember => {
                        set_intersection(db, [Type::any()], [Type::bool_literal(false)])
                    }
                    _ => Type::any(),
                };
                set_union(db, [wrapped(db, argument, 1), last], RecursivelyDefined::No)
            }
            Self::PositiveIntersection => set_intersection(db, [Type::any()], []),
            Self::NegativeIntersection => set_intersection(db, [], [Type::any()]),
            Self::MixedIntersection => {
                set_intersection(db, [Type::any()], [Type::bool_literal(false)])
            }
            Self::MixedNegativeAny => set_intersection(db, [first], [Type::any()]),
            Self::NestedIntersection => set_intersection(
                db,
                [Type::any()],
                [set_union(
                    db,
                    [Type::any(), Type::bool_literal(false)],
                    RecursivelyDefined::No,
                )],
            ),
            Self::NegativeTypeForm => {
                set_intersection(db, [Type::any()], [wrapped(db, Type::any(), 1)])
            }
        }
    }

    fn expected<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        kind: MaterializationKind,
    ) -> Type<'db> {
        let argument = match kind {
            MaterializationKind::Top => Type::object(),
            MaterializationKind::Bottom => Type::Never,
        };
        match self {
            Self::UnchangedUnion => self.value(db, env),
            Self::ChangedUnion | Self::UnionParentMetadata | Self::UnionChildMetadata => match kind
            {
                MaterializationKind::Top => Type::object(),
                MaterializationKind::Bottom => Self::UnchangedUnion.value(db, env),
            },
            Self::NestedTypeForm => wrapped(db, Self::ChangedUnion.expected(db, env, kind), 1),
            Self::NestedTuple => Type::heterogeneous_tuple(
                db,
                env,
                [
                    Self::ChangedUnion.expected(db, env, kind),
                    wrapped(db, argument, 1),
                ],
            ),
            Self::UnionTypeFormMember | Self::UnionTupleMember | Self::UnionIntersectionMember => {
                let argument = match self {
                    Self::UnionTupleMember => Type::heterogeneous_tuple(db, env, [argument]),
                    Self::UnionIntersectionMember => {
                        Self::MixedIntersection.expected(db, env, kind)
                    }
                    _ => argument,
                };
                set_union(
                    db,
                    [wrapped(db, argument, 1), Type::literal_string()],
                    RecursivelyDefined::No,
                )
            }
            Self::PositiveIntersection | Self::NegativeIntersection => argument,
            Self::MixedIntersection | Self::NestedIntersection => match kind {
                MaterializationKind::Top => set_intersection(db, [], [Type::bool_literal(false)]),
                MaterializationKind::Bottom => Type::Never,
            },
            Self::MixedNegativeAny => match kind {
                MaterializationKind::Top => Type::bool_literal(true),
                MaterializationKind::Bottom => Type::Never,
            },
            Self::NegativeTypeForm => match kind {
                MaterializationKind::Top => set_intersection(db, [], [wrapped(db, Type::Never, 1)]),
                MaterializationKind::Bottom => Type::Never,
            },
        }
    }

    fn children(self) -> usize {
        match self {
            Self::UnchangedUnion | Self::MixedIntersection | Self::MixedNegativeAny => 2,
            Self::ChangedUnion | Self::NegativeTypeForm | Self::UnionTypeFormMember => 3,
            Self::UnionParentMetadata
            | Self::UnionChildMetadata
            | Self::UnionIntersectionMember => 5,
            Self::NestedTypeForm | Self::NestedIntersection | Self::UnionTupleMember => 4,
            Self::NestedTuple => 6,
            Self::PositiveIntersection | Self::NegativeIntersection => 1,
        }
    }
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

fn assert_retired(progress: &Progress, roots: usize) {
    assert!(progress.retired.get());
    assert_eq!(progress.pools.get(), Some([roots, 0, 0, roots, 0]));
    assert_no_active_attempt();
}

fn assert_shared_visitor(depth: usize) {
    let snapshot = materialization_observations::snapshot();
    assert_eq!(snapshot.root_count, 1);
    let root = snapshot.roots[0].expect("the canonical body retains one root");
    assert_ne!(root.environment, 0);
    assert_ne!(root.visitor, 0);
    assert_eq!(snapshot.child_count, depth);
    assert!(
        snapshot.child_visitors[..depth]
            .iter()
            .all(|visitor| *visitor == Some(root.visitor))
    );
}

fn assert_shared_visitor_per_attempt(progress: &Progress, children: usize) {
    assert_shared_visitor_attempts(progress, children, 0);
}

fn assert_shared_visitor_attempts(progress: &Progress, children: usize, partial_builders: usize) {
    let snapshot = materialization_observations::snapshot();
    let attempts = progress.attempts.get();
    let attempts = &attempts[..progress.attempt_count.get()];
    assert_eq!(snapshot.root_count, attempts.len());
    let mut observed_children = 0;
    for attempt in attempts {
        let attempt = attempt.expect("the completed source attempt was observed");
        assert_eq!(attempt.roots, 1, "{attempt:?}");
        assert_eq!(attempt.pools[3], 1, "{attempt:?}");
        let root = snapshot.roots[attempt.first_root].expect("the attempt owns its visitor");
        assert_ne!(root.environment, 0);
        assert_ne!(root.visitor, 0);
        assert!(attempt.children <= children, "{attempt:?}");
        assert!(
            snapshot.child_visitors[attempt.first_child..attempt.first_child + attempt.children]
                .iter()
                .all(|visitor| *visitor == Some(root.visitor))
        );
        observed_children += attempt.children;
    }
    assert_eq!(snapshot.child_count, observed_children);
    let last = attempts.last().unwrap().unwrap();
    assert_eq!(last.children, children);
    assert_eq!(last.partial_builders, partial_builders);
}

fn assert_finish_cleanup(boundary: CleanupBoundary, depth: usize) {
    let db = fixture();
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let input = wrapped(&db, Type::any(), depth);
    let kind = MaterializationKind::Bottom;
    let revision = salsa::plumbing::current_revision(&db);
    materialization_observations::reset(None);
    materialization_observations::reset_cleanup(boundary, depth);
    let progress = Progress::default();
    let captured = capture(&db, || {
        controlled(&prepared, input, kind, &funded(), &progress)
    })
    .unwrap();
    let expected = match boundary {
        CleanupBoundary::Acceptance => Err(AnalysisFailure::Execution(RunError::Contract(
            "completed task retained a child",
        ))),
        _ => Ok(AnalysisOutcome::Incomplete {
            reason: if boundary == CleanupBoundary::Resource {
                AnalysisIncomplete::RequestedAllocationLimit
            } else {
                AnalysisIncomplete::WorkLimit
            },
            completed: (),
        }),
    };
    assert_eq!(captured.value, expected, "{boundary:?} at depth {depth}");
    assert_retired(&progress, 1);
    assert_shared_visitor(depth);
    let snapshot = materialization_observations::cleanup_snapshot();
    assert_eq!(snapshot.finishes, depth);
    assert_eq!((snapshot.queued, snapshot.started), (1, 0));
    assert_eq!(snapshot.prepared, boundary == CleanupBoundary::Acceptance);
    assert!(!snapshot.committed);
    assert_eq!(snapshot.drop_count, 2);
    assert_eq!(
        snapshot.drops,
        [Some(CleanupDrop::Child), Some(CleanupDrop::Owner)],
    );
    let child = snapshot
        .child
        .expect("the queued child observed the real scope");
    let owner = snapshot.owner.expect("the finish future retired its scope");
    assert_eq!(child.root, Lookup::Original);
    assert_eq!(owner.root, Lookup::Absent { active: 0 });
    assert_eq!((child.active, owner.active), (Some(1), Some(0)));
    assert_eq!((child.cache_len, owner.cache_len), (depth - 1, depth - 1));
    match boundary {
        CleanupBoundary::Finish | CleanupBoundary::Acceptance => assert_eq!(snapshot.work, None),
        CleanupBoundary::Cache => assert!(matches!(
            snapshot.work,
            Some(TypeTransformationWork::CacheStorage { len, .. }) if len == depth - 1
        )),
        CleanupBoundary::RehashKey => assert!(matches!(
            snapshot.work,
            Some(TypeTransformationWork::RehashKey { .. })
        )),
        CleanupBoundary::Growth | CleanupBoundary::Resource => {
            let Some(TypeTransformationWork::Grow {
                storage: TypeTransformationStorage::Cache,
                requested_capacity,
                requested_payload_bytes,
                ..
            }) = snapshot.work
            else {
                panic!("the selected root must reach cache growth: {snapshot:?}");
            };
            assert!(requested_capacity >= depth);
            assert!(requested_payload_bytes > 0);
            assert_eq!(
                snapshot.resource,
                (boundary == CleanupBoundary::Resource).then_some(requested_payload_bytes),
            );
        }
    }
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, program, kind))
        .expect("the refused finish belongs to the canonical root");
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let key = ingredient.database_key_index(id);
    assert!(
        !captured
            .reads
            .iter()
            .any(|read| { read.key == key && read.status == prepared_source_probe::Status::Final })
    );

    materialization_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, input, kind, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(wrapped(&db, Type::Never, depth))),
    );
    assert_retired(&progress, 1);
    assert_shared_visitor(depth);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn source_materialization_finish_admission_drains_children_before_the_scope() {
    for boundary in [CleanupBoundary::Finish, CleanupBoundary::Cache] {
        assert_finish_cleanup(boundary, 2);
    }
}

#[test]
fn source_materialization_rejected_finish_remains_uncommitted_during_cleanup() {
    for depth in [2, 3, 8] {
        assert_finish_cleanup(CleanupBoundary::Acceptance, depth);
    }
}

#[test]
fn source_materialization_cache_growth_refusals_preserve_the_scope_and_retry() {
    // The third wrapper spills inline storage; the eighth grows the full hash table.
    for depth in [3, 8] {
        for boundary in [
            CleanupBoundary::Growth,
            CleanupBoundary::RehashKey,
            CleanupBoundary::Resource,
        ] {
            assert_finish_cleanup(boundary, depth);
        }
    }
}

#[test]
fn cold_typeform_materialization_publishes_one_root_and_reuses_its_memo() {
    for depth in [1, 8, 32] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let program = env.program(&db);
        let input = wrapped(&db, Type::any(), depth);
        let revision = salsa::plumbing::current_revision(&db);
        let mut reader = db.clone();
        let mut keys = Vec::new();

        for (kind, argument) in [
            (MaterializationKind::Top, Type::object()),
            (MaterializationKind::Bottom, Type::Never),
        ] {
            let ordinary_db = fixture();
            let ordinary_env = ordinary_db.program_environment();
            let ordinary_input = wrapped(&ordinary_db, Type::any(), depth);
            let ordinary_expected = wrapped(&ordinary_db, argument, depth);
            assert_eq!(
                ordinary_input.materialization(&ordinary_db, &ordinary_env, kind),
                ordinary_expected,
            );
            assert!(
                existing_key(
                    &db,
                    cached_materialization_ingredient(&db),
                    &(input, program, kind),
                )
                .is_none()
            );
            assert!(
                !TypeFormType::ingredient(db.zalsa())
                    .entries(db.zalsa())
                    .any(|entry| entry.value().fields().0 == argument)
            );

            materialization_observations::reset(None);
            reader.take_salsa_events();
            let progress = Progress::default();
            let cold = capture(&db, || {
                controlled(&prepared, input, kind, &funded(), &progress)
            })
            .unwrap();
            let expected = wrapped(&db, argument, depth);
            assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(expected)));
            assert_eq!(cold.check_root_reads(), Ok(()));
            assert_retired(&progress, 1);
            assert_shared_visitor(depth);

            let ingredient = cached_materialization_ingredient(&db);
            let id = existing_key(&db, ingredient, &(input, program, kind))
                .expect("the source call retains the canonical key");
            assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
            let key = ingredient.database_key_index(id);
            let executed = reader
                .take_salsa_events()
                .into_iter()
                .filter_map(|event| match event.kind {
                    salsa::EventKind::WillExecute { database_key }
                        if db.ingredient_debug_name(database_key.ingredient_index())
                            == "cached_materialization" =>
                    {
                        Some(database_key)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(executed, [key]);
            let root = cold
                .reads
                .iter()
                .find(|read| read.key == key && read.parent.is_none())
                .expect("the source entry reads the canonical root");
            assert_eq!(root.status, prepared_source_probe::Status::Final);
            keys.push(key);

            let ordinary = capture(&db, || input.materialization(&db, &env, kind)).unwrap();
            assert_eq!(ordinary.value, expected);
            assert!(
                ordinary
                    .reads
                    .iter()
                    .any(|read| read.key == key && read.memo_address == root.memo_address)
            );
            materialization_observations::reset(None);
            let progress = Progress::default();
            assert_eq!(
                controlled(&prepared, input, kind, &funded(), &progress),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_retired(&progress, 0);
            let snapshot = materialization_observations::snapshot();
            assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
            assert_function_query_was_not_run_by_name(
                &db,
                "cached_materialization",
                None,
                &reader.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        assert_ne!(keys[0], keys[1]);
    }
}

#[test]
fn source_tuple_materialization_matches_ordinary_and_reuses_canonical_memos() {
    for case in [
        TupleCase::Empty,
        TupleCase::Fixed,
        TupleCase::Homogeneous,
        TupleCase::Prefix,
        TupleCase::Suffix,
        TupleCase::Mixed,
        TupleCase::NestedTuple,
        TupleCase::NestedTypeForm,
        TupleCase::UnchangedFixed,
        TupleCase::UnchangedVariable,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let program = env.program(&db);
        let input = case.value(&db, &env, Type::any());
        let revision = salsa::plumbing::current_revision(&db);
        let mut reader = db.clone();
        let mut keys = Vec::new();

        for (kind, argument) in [
            (MaterializationKind::Top, Type::object()),
            (MaterializationKind::Bottom, Type::Never),
        ] {
            let ordinary_db = fixture();
            let ordinary_env = ordinary_db.program_environment();
            let ordinary_input = case.value(&ordinary_db, &ordinary_env, Type::any());
            assert_eq!(
                ordinary_input.materialization(&ordinary_db, &ordinary_env, kind),
                case.value(&ordinary_db, &ordinary_env, argument),
                "{case:?}, {kind:?}",
            );
            let ingredient = cached_materialization_ingredient(&db);
            assert!(existing_key(&db, ingredient, &(input, program, kind)).is_none());
            materialization_observations::reset(None);
            reader.take_salsa_events();
            let progress = Progress::default();
            let cold = capture(&db, || {
                controlled(&prepared, input, kind, &funded(), &progress)
            })
            .unwrap();
            let expected = case.value(&db, &env, argument);
            assert_eq!(
                cold.value,
                Ok(AnalysisOutcome::Complete(expected)),
                "{case:?}, {kind:?}"
            );
            assert_eq!(cold.check_root_reads(), Ok(()));
            assert_retired(&progress, 1);
            assert_shared_visitor(case.children());
            if matches!(
                case,
                TupleCase::Empty | TupleCase::UnchangedFixed | TupleCase::UnchangedVariable
            ) {
                assert_eq!(expected, input);
            }

            let id = existing_key(&db, ingredient, &(input, program, kind))
                .expect("the tuple call retains the canonical key");
            assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
            let key = ingredient.database_key_index(id);
            let root = cold
                .reads
                .iter()
                .find(|read| read.key == key && read.parent.is_none())
                .expect("tuple materialization reads the canonical root");
            assert_eq!(root.status, prepared_source_probe::Status::Final);
            keys.push(key);

            reader.take_salsa_events();
            let ordinary = capture(&db, || input.materialization(&db, &env, kind)).unwrap();
            assert_eq!(ordinary.value, expected);
            assert!(
                ordinary
                    .reads
                    .iter()
                    .any(|read| { read.key == key && read.memo_address == root.memo_address })
            );
            materialization_observations::reset(None);
            let progress = Progress::default();
            assert_eq!(
                controlled(&prepared, input, kind, &funded(), &progress),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_retired(&progress, 0);
            let snapshot = materialization_observations::snapshot();
            assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
            assert_function_query_was_not_run_by_name(
                &db,
                "cached_materialization",
                None,
                &reader.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        assert_ne!(keys[0], keys[1]);
    }
}

#[test]
fn source_set_materialization_matches_ordinary_and_reuses_canonical_memos() {
    for case in [
        SetCase::UnchangedUnion,
        SetCase::ChangedUnion,
        SetCase::UnionParentMetadata,
        SetCase::UnionChildMetadata,
        SetCase::NestedTypeForm,
        SetCase::NestedTuple,
        SetCase::PositiveIntersection,
        SetCase::NegativeIntersection,
        SetCase::MixedIntersection,
        SetCase::MixedNegativeAny,
        SetCase::NestedIntersection,
        SetCase::NegativeTypeForm,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let program = env.program(&db);
        let input = case.value(&db, &env);
        let revision = salsa::plumbing::current_revision(&db);
        let mut reader = db.clone();
        let mut keys = Vec::new();

        for kind in [MaterializationKind::Top, MaterializationKind::Bottom] {
            let ordinary_db = fixture();
            let ordinary_env = ordinary_db.program_environment();
            let ordinary_input = case.value(&ordinary_db, &ordinary_env);
            assert_eq!(
                ordinary_input.materialization(&ordinary_db, &ordinary_env, kind),
                case.expected(&ordinary_db, &ordinary_env, kind),
                "{case:?}, {kind:?}",
            );
            let ingredient = cached_materialization_ingredient(&db);
            assert!(existing_key(&db, ingredient, &(input, program, kind)).is_none());
            materialization_observations::reset(None);
            reader.take_salsa_events();
            let progress = Progress::default();
            let cold = capture(&db, || {
                controlled(&prepared, input, kind, &funded(), &progress)
            })
            .unwrap();
            let expected = case.expected(&db, &env, kind);
            assert_eq!(
                cold.value,
                Ok(AnalysisOutcome::Complete(expected)),
                "{case:?}, {kind:?}",
            );
            assert_eq!(cold.check_root_reads(), Ok(()));
            assert!(progress.retired.get());
            assert_eq!(progress.pools.get().map(|pools| pools[3]), Some(1));
            assert_no_active_attempt();
            assert_shared_visitor_per_attempt(&progress, case.children());
            let builders = materialization_observations::set_snapshot();
            assert_eq!(builders.live, 0);
            assert_eq!(builders.dropped, builders.created);
            if matches!(case, SetCase::UnchangedUnion) {
                assert_eq!(expected, input);
                assert_eq!(builders.created, 0);
            }
            if let Type::Union(union) = expected
                && matches!(
                    case,
                    SetCase::UnchangedUnion
                        | SetCase::ChangedUnion
                        | SetCase::UnionParentMetadata
                        | SetCase::UnionChildMetadata
                )
            {
                assert_eq!(union.recursively_defined(&db), RecursivelyDefined::Yes);
                assert_eq!(
                    union.elements(&db),
                    &[Type::bool_literal(true), Type::literal_string()]
                );
            }

            let id = existing_key(&db, ingredient, &(input, program, kind))
                .expect("set materialization retains the canonical key");
            assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
            let key = ingredient.database_key_index(id);
            let root = cold
                .reads
                .iter()
                .find(|read| read.key == key && read.parent.is_none())
                .expect("set materialization reads the canonical root");
            assert_eq!(root.status, prepared_source_probe::Status::Final);
            keys.push(key);

            reader.take_salsa_events();
            let ordinary = capture(&db, || input.materialization(&db, &env, kind)).unwrap();
            assert_eq!(ordinary.value, expected);
            assert!(
                ordinary
                    .reads
                    .iter()
                    .any(|read| { read.key == key && read.memo_address == root.memo_address })
            );
            materialization_observations::reset(None);
            let progress = Progress::default();
            assert_eq!(
                controlled(&prepared, input, kind, &funded(), &progress),
                Ok(AnalysisOutcome::Complete(expected)),
            );
            assert_retired(&progress, 0);
            let snapshot = materialization_observations::snapshot();
            assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
            assert_function_query_was_not_run_by_name(
                &db,
                "cached_materialization",
                None,
                &reader.take_salsa_events(),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        assert_ne!(keys[0], keys[1]);
    }
}

fn assert_compound_member_completes_and_reuses_canonical_memo(
    case: SetCase,
    kind: MaterializationKind,
) {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let program = env.program(&db);
    let input = case.value(&db, &env);
    let revision = salsa::plumbing::current_revision(&db);
    let ordinary = fixture();
    let ordinary_env = ordinary.program_environment();
    assert_eq!(
        case.value(&ordinary, &ordinary_env)
            .materialization(&ordinary, &ordinary_env, kind),
        case.expected(&ordinary, &ordinary_env, kind),
        "{case:?}, {kind:?}",
    );
    let ingredient = cached_materialization_ingredient(&db);
    assert!(existing_key(&db, ingredient, &(input, program, kind)).is_none());
    let mut reader = db.clone();
    reader.take_salsa_events();
    materialization_observations::reset(None);
    let progress = Progress::default();
    let captured = capture(&db, || {
        controlled(&prepared, input, kind, &funded(), &progress)
    })
    .unwrap();
    let expected = case.expected(&db, &env, kind);
    assert_eq!(
        captured.value,
        Ok(AnalysisOutcome::Complete(expected)),
        "{case:?}, {kind:?}",
    );
    assert_eq!(captured.check_root_reads(), Ok(()));
    assert!(progress.retired.get());
    assert_eq!(progress.attempt_count.get(), 1);
    let mappings = materialization_observations::mapping_snapshot();
    // Union simplification can invoke other mappings. The requested materialization creates its
    // visitor before any of those dependencies.
    let root =
        mappings.roots[0].expect("the principal mapping owns its visitor before its children");
    assert!(mappings.root_count <= mappings.roots.len());
    assert_eq!(
        mappings.roots[..mappings.root_count]
            .iter()
            .flatten()
            .filter(|candidate| candidate.visitor == root.visitor)
            .count(),
        1,
    );
    assert_eq!(root.mapping, OwnedMappingSnapshot::Materialize(kind));
    assert_eq!(root.program, Some(program.as_id()));
    assert!(root.default_context);
    assert_ne!(root.visitor, 0);
    assert!(mappings.child_count <= mappings.children.len());
    let mut children = 0;
    let mut opposite_children = 0;
    for child in mappings.children[..mappings.child_count].iter().flatten() {
        if child.visitor == root.visitor {
            let OwnedMappingSnapshot::Materialize(child_kind) = child.mapping else {
                panic!("materialization child uses another mapping: {child:?}");
            };
            opposite_children += usize::from(child_kind != kind);
            assert_eq!(child.program, None);
            assert!(child.default_context);
            children += 1;
        }
    }
    assert_eq!(children, case.children());
    // The negative intersection element reverses materialization while sharing the visitor.
    assert_eq!(
        opposite_children,
        usize::from(matches!(case, SetCase::UnionIntersectionMember)),
    );
    let id = existing_key(&db, ingredient, &(input, program, kind))
        .expect("the completed reconstruction retains its canonical key");
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    let key = ingredient.database_key_index(id);
    assert_eq!(
        reader
            .take_salsa_events()
            .iter()
            .filter(|event| {
                matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
            })
            .count(),
        1,
    );
    let root = captured
        .reads
        .iter()
        .find(|read| read.key == key && read.parent.is_none())
        .expect("materialization reads the canonical root");
    assert_eq!(root.status, prepared_source_probe::Status::Final);
    let ordinary = capture(&db, || input.materialization(&db, &env, kind)).unwrap();
    assert_eq!(ordinary.value, expected);
    assert!(
        ordinary
            .reads
            .iter()
            .any(|read| { read.key == key && read.memo_address == root.memo_address })
    );
    materialization_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, input, kind, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_retired(&progress, 0);
    assert_eq!(materialization_observations::snapshot().root_count, 0);
    assert_function_query_was_not_run_by_name(
        &db,
        "cached_materialization",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn source_union_type_form_member_top_materialization_completes() {
    assert_compound_member_completes_and_reuses_canonical_memo(
        SetCase::UnionTypeFormMember,
        MaterializationKind::Top,
    );
}

#[test]
fn source_union_type_form_member_bottom_materialization_completes() {
    assert_compound_member_completes_and_reuses_canonical_memo(
        SetCase::UnionTypeFormMember,
        MaterializationKind::Bottom,
    );
}

#[test]
fn source_union_tuple_member_top_materialization_completes() {
    assert_compound_member_completes_and_reuses_canonical_memo(
        SetCase::UnionTupleMember,
        MaterializationKind::Top,
    );
}

#[test]
fn source_union_tuple_member_bottom_materialization_completes() {
    assert_compound_member_completes_and_reuses_canonical_memo(
        SetCase::UnionTupleMember,
        MaterializationKind::Bottom,
    );
}

#[test]
fn source_union_intersection_member_top_materialization_completes() {
    assert_compound_member_completes_and_reuses_canonical_memo(
        SetCase::UnionIntersectionMember,
        MaterializationKind::Top,
    );
}

#[test]
fn source_union_intersection_member_bottom_materialization_completes() {
    assert_compound_member_completes_and_reuses_canonical_memo(
        SetCase::UnionIntersectionMember,
        MaterializationKind::Bottom,
    );
}

#[test]
fn source_materialization_reuses_an_ordinary_memo_without_root_owners() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = wrapped(&db, Type::any(), 5);
    let kind = MaterializationKind::Bottom;
    let expected = input.materialization(&db, &env, kind);
    let revision = salsa::plumbing::current_revision(&db);
    let mut reader = db.clone();
    reader.take_salsa_events();
    materialization_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, input, kind, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_retired(&progress, 0);
    let snapshot = materialization_observations::snapshot();
    assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "cached_materialization",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn materialization_work_refusal_retires_owners_and_retries_the_unpublished_root() {
    let depth = 32;
    let kind = MaterializationKind::Bottom;
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    let input = wrapped(&measured, Type::any(), depth);
    materialization_observations::reset(None);
    let cold = Progress::default();
    assert!(matches!(
        controlled(&measured_prepared, input, kind, &funded(), &cold),
        Ok(AnalysisOutcome::Complete(_)),
    ));
    assert_retired(&cold, 1);
    assert_shared_visitor(depth);
    let cold_work = funded().semantic_work_limit - cold.remaining.get().unwrap();
    let warm = Progress::default();
    assert!(matches!(
        controlled(&measured_prepared, input, kind, &funded(), &warm),
        Ok(AnalysisOutcome::Complete(_)),
    ));
    assert_retired(&warm, 0);
    let warm_work = funded().semantic_work_limit - warm.remaining.get().unwrap();
    assert!(cold_work > warm_work);

    let db = fixture();
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let input = wrapped(&db, Type::any(), depth);
    let revision = salsa::plumbing::current_revision(&db);
    materialization_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            input,
            kind,
            &AnalysisPolicy {
                semantic_work_limit: warm_work + (cold_work - warm_work) / 2,
                ..funded()
            },
            &progress,
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert_retired(&progress, 1);
    let snapshot = materialization_observations::snapshot();
    assert_eq!(snapshot.root_count, 1);
    assert!(snapshot.child_count > 0 && snapshot.child_count < depth);
    let ingredient = cached_materialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, program, kind))
        .expect("refusal occurs inside the canonical body");
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_err());

    materialization_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, input, kind, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(wrapped(&db, Type::Never, depth))),
    );
    assert_retired(&progress, 1);
    assert_shared_visitor(depth);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn tuple_materialization_work_refusal_retires_partial_buffers_before_retry() {
    fn input<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, mixed: bool) -> Type<'db> {
        if mixed {
            Type::tuple(TupleType::mixed(
                db,
                env,
                std::iter::repeat_n(Type::any(), 16),
                Type::any(),
                std::iter::repeat_n(Type::any(), 16),
            ))
        } else {
            Type::heterogeneous_tuple(db, env, std::iter::repeat_n(Type::any(), 32))
        }
    }

    let kind = MaterializationKind::Bottom;
    for mixed in [false, true] {
        let measured = fixture();
        let measured_prepared = prepare(&measured);
        let measured_env = ProgramEnvironment::from_file(measured_prepared.program_file());
        let measured_input = input(&measured, &measured_env, mixed);
        materialization_observations::reset(None);
        let progress = Progress::default();
        assert!(matches!(
            controlled(
                &measured_prepared,
                measured_input,
                kind,
                &funded(),
                &progress
            ),
            Ok(AnalysisOutcome::Complete(_)),
        ));
        assert_retired(&progress, 1);
        let measured_buffers = materialization_observations::tuple_snapshot();
        assert_eq!(
            (
                measured_buffers.created,
                measured_buffers.live,
                measured_buffers.dropped
            ),
            (1, 0, 1)
        );
        assert_eq!(measured_buffers.partial_drops, 0);
        assert_eq!(measured_buffers.push_count, 32);
        let push = measured_buffers
            .pushes
            .iter()
            .flatten()
            .find(|push| push.buffer == 1 && push.len == 8)
            .expect("eight mapped elements remain in the tuple buffer");
        let work = funded().semantic_work_limit - push.remaining.unwrap();

        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let program = env.program(&db);
        let input = input(&db, &env, mixed);
        let revision = salsa::plumbing::current_revision(&db);
        materialization_observations::reset(None);
        let progress = Progress::default();
        let refused = capture(&db, || {
            controlled(
                &prepared,
                input,
                kind,
                &AnalysisPolicy {
                    semantic_work_limit: work,
                    ..funded()
                },
                &progress,
            )
        })
        .unwrap();
        assert_eq!(
            refused.value,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
            "mixed={mixed}"
        );
        assert_retired(&progress, 1);
        let buffers = materialization_observations::tuple_snapshot();
        assert_eq!(
            (
                buffers.created,
                buffers.live,
                buffers.dropped,
                buffers.partial_drops
            ),
            (1, 0, 1, 1)
        );
        assert_eq!(buffers.push_count, 8);
        assert_eq!(buffers.last_dropped_len, Some(8));
        let ingredient = cached_materialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, program, kind))
            .expect("the partial tuple belongs to the canonical root");
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        let key = ingredient.database_key_index(id);
        assert!(!refused.reads.iter().any(|read| {
            read.key == key && read.status == prepared_source_probe::Status::Final
        }));

        materialization_observations::reset(None);
        let progress = Progress::default();
        let expected = Type::heterogeneous_tuple(&db, &env, std::iter::repeat_n(Type::Never, 32));
        assert_eq!(
            controlled(&prepared, input, kind, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_retired(&progress, 1);
        assert_shared_visitor(32 + usize::from(mixed));
        let buffers = materialization_observations::tuple_snapshot();
        assert_eq!(
            (
                buffers.created,
                buffers.live,
                buffers.dropped,
                buffers.partial_drops
            ),
            (1, 0, 1, 0)
        );
        assert_eq!(buffers.push_count, 32);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn set_materialization_work_refusal_retires_partial_builders_before_retry() {
    fn input<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, kind: SetKind) -> Type<'db> {
        match kind {
            SetKind::Union => SetCase::ChangedUnion.value(db, env),
            SetKind::Intersection => SetCase::MixedNegativeAny.value(db, env),
        }
    }

    for (builder_kind, kind, additions, children) in [
        (SetKind::Union, MaterializationKind::Bottom, 2, 3),
        (SetKind::Intersection, MaterializationKind::Top, 1, 2),
    ] {
        let measured = fixture();
        let measured_prepared = prepare(&measured);
        let measured_env = ProgramEnvironment::from_file(measured_prepared.program_file());
        let measured_input = input(&measured, &measured_env, builder_kind);
        materialization_observations::reset(None);
        let progress = Progress::default();
        let complete = controlled(
            &measured_prepared,
            measured_input,
            kind,
            &funded(),
            &progress,
        );
        assert!(
            matches!(complete, Ok(AnalysisOutcome::Complete(_))),
            "{builder_kind:?}: {complete:?}"
        );
        let measured_builders = materialization_observations::set_snapshot();
        assert_eq!(
            (
                measured_builders.created,
                measured_builders.live,
                measured_builders.dropped
            ),
            (1, 0, 1),
        );
        assert_eq!(measured_builders.partial_drops, 0);
        let mutation = measured_builders
            .mutations
            .iter()
            .flatten()
            .find(|mutation| {
                mutation.builder == 1
                    && mutation.kind == builder_kind
                    && mutation.count == additions
            })
            .expect("the mapping builder retains its earlier insertions");
        let retained_len = (builder_kind == SetKind::Union).then_some(1);
        assert_eq!(mutation.len, retained_len);
        let work = funded().semantic_work_limit - mutation.remaining.unwrap();

        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let program = env.program(&db);
        let input = input(&db, &env, builder_kind);
        let revision = salsa::plumbing::current_revision(&db);
        materialization_observations::reset(None);
        let progress = Progress::default();
        let refused = capture(&db, || {
            controlled(
                &prepared,
                input,
                kind,
                &AnalysisPolicy {
                    semantic_work_limit: work,
                    ..funded()
                },
                &progress,
            )
        })
        .unwrap();
        assert_eq!(
            refused.value,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
            "{builder_kind:?}",
        );
        assert!(progress.retired.get());
        assert_eq!(progress.pools.get().map(|pools| pools[3]), Some(1));
        assert_no_active_attempt();
        let builders = materialization_observations::set_snapshot();
        assert_eq!(
            (
                builders.created,
                builders.live,
                builders.dropped,
                builders.partial_drops
            ),
            (1, 0, 1, 1),
        );
        assert_eq!(builders.mutation_count, additions);
        assert_eq!(builders.last_dropped_len, retained_len);
        let ingredient = cached_materialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, program, kind))
            .expect("the partial set builder belongs to the canonical root");
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        let key = ingredient.database_key_index(id);
        assert!(!refused.reads.iter().any(|read| {
            read.key == key && read.status == prepared_source_probe::Status::Final
        }));

        materialization_observations::reset(None);
        let progress = Progress::default();
        let expected = match builder_kind {
            SetKind::Union => SetCase::ChangedUnion.expected(&db, &env, kind),
            SetKind::Intersection => Type::bool_literal(true),
        };
        assert_eq!(
            controlled(&prepared, input, kind, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert!(progress.retired.get());
        assert_eq!(progress.pools.get().map(|pools| pools[3]), Some(1));
        assert_no_active_attempt();
        assert_shared_visitor(children);
        let builders = materialization_observations::set_snapshot();
        assert_eq!(
            (
                builders.created,
                builders.live,
                builders.dropped,
                builders.partial_drops
            ),
            (1, 0, 1, 0),
        );
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn nested_materialization_child_refusal_drains_before_retained_set_builders() {
    fn input(db: &dyn Db, kind: SetKind) -> Type<'_> {
        match kind {
            SetKind::Union => set_union(
                db,
                [
                    Type::bool_literal(true),
                    Type::any(),
                    wrapped(db, Type::any(), 2),
                ],
                RecursivelyDefined::No,
            ),
            SetKind::Intersection => set_intersection(
                db,
                [
                    Type::bool_literal(true),
                    set_intersection(db, [Type::any()], []),
                ],
                [],
            ),
        }
    }

    for (builder_kind, kind, selected_child, live_builders, children, expected) in [
        (
            SetKind::Union,
            MaterializationKind::Top,
            4,
            1,
            5,
            Type::object(),
        ),
        (
            SetKind::Intersection,
            MaterializationKind::Bottom,
            3,
            2,
            3,
            Type::Never,
        ),
    ] {
        let ordinary = fixture();
        assert_eq!(
            input(&ordinary, builder_kind).materialization(
                &ordinary,
                &ordinary.program_environment(),
                kind
            ),
            expected,
        );
        let measured = fixture();
        let measured_prepared = prepare(&measured);
        materialization_observations::reset(None);
        assert_eq!(
            controlled(
                &measured_prepared,
                input(&measured, builder_kind),
                kind,
                &funded(),
                &Progress::default()
            ),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        let journal = materialization_observations::set_cleanup_snapshot();
        assert!(journal.event_count <= journal.events.len());
        let remaining = journal
            .events
            .iter()
            .flatten()
            .find_map(|event| {
                if let SetCleanupEvent::ChildEntered {
                    child,
                    live_builders: live,
                    remaining,
                } = *event
                    && child == selected_child
                    && live == live_builders
                {
                    remaining
                } else {
                    None
                }
            })
            .expect("the nested mapping task entered while its parent retained a builder");
        let work = funded().semantic_work_limit - remaining;

        let db = fixture();
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let input = input(&db, builder_kind);
        let revision = salsa::plumbing::current_revision(&db);
        materialization_observations::reset(None);
        let progress = Progress::default();
        let refused = capture(&db, || {
            controlled(
                &prepared,
                input,
                kind,
                &AnalysisPolicy {
                    semantic_work_limit: work,
                    ..funded()
                },
                &progress,
            )
        })
        .unwrap();
        assert_eq!(
            refused.value,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
        );
        assert!(progress.retired.get());
        assert_shared_visitor(selected_child);
        assert_no_active_attempt();
        let builders = materialization_observations::set_snapshot();
        assert_eq!(
            (builders.created, builders.live, builders.dropped),
            (live_builders, 0, live_builders)
        );
        assert_eq!(builders.partial_drops, 1);
        let journal = materialization_observations::set_cleanup_snapshot();
        assert!(journal.event_count <= journal.events.len());
        let drops = journal
            .events
            .iter()
            .flatten()
            .copied()
            .filter(|event| !matches!(event, SetCleanupEvent::ChildEntered { .. }))
            .collect::<Vec<_>>();
        let expected_drops = match builder_kind {
            SetKind::Union => vec![
                SetCleanupEvent::ChildDropped {
                    child: 4,
                    live_builders: 1,
                },
                SetCleanupEvent::ChildDropped {
                    child: 3,
                    live_builders: 1,
                },
                SetCleanupEvent::BuilderDropped { builder: 1 },
            ],
            SetKind::Intersection => vec![
                SetCleanupEvent::ChildDropped {
                    child: 3,
                    live_builders: 2,
                },
                SetCleanupEvent::BuilderDropped { builder: 2 },
                SetCleanupEvent::ChildDropped {
                    child: 2,
                    live_builders: 1,
                },
                SetCleanupEvent::BuilderDropped { builder: 1 },
            ],
        };
        assert!(
            drops.ends_with(&expected_drops),
            "{builder_kind:?}: {drops:?}"
        );
        let ingredient = cached_materialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, program, kind)).unwrap();
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        let key = ingredient.database_key_index(id);
        assert!(!refused.reads.iter().any(|read| {
            read.key == key && read.status == prepared_source_probe::Status::Final
        }));

        materialization_observations::reset(None);
        let progress = Progress::default();
        assert_eq!(
            controlled(&prepared, input, kind, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert!(progress.retired.get());
        assert_shared_visitor_per_attempt(&progress, children);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn materialization_child_cancellation_preserves_the_completed_memo_for_retry() {
    let depth = 8;
    let kind = MaterializationKind::Top;
    for cancel_at in [1, 4, depth] {
        let db = fixture();
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let input = wrapped(&db, Type::any(), depth);
        let revision = salsa::plumbing::current_revision(&db);
        materialization_observations::reset(Some(cancel_at));
        let progress = Progress::default();
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, input, kind, &funded(), &progress)
        }));
        assert!(matches!(result, Err(salsa::Cancelled::Local)));
        assert_retired(&progress, 1);
        assert_shared_visitor(depth);
        let ingredient = cached_materialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, program, kind))
            .expect("cancellation occurs inside the canonical body");
        // Salsa masks local cancellation while the canonical query owns its claim, so its
        // completed memo remains reusable when cancellation reaches the source caller.
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());

        materialization_observations::reset(None);
        let mut reader = db.clone();
        reader.take_salsa_events();
        let progress = Progress::default();
        assert_eq!(
            controlled(&prepared, input, kind, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(wrapped(
                &db,
                Type::object(),
                depth
            ))),
        );
        assert_retired(&progress, 0);
        let snapshot = materialization_observations::snapshot();
        assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
        assert_function_query_was_not_run_by_name(
            &db,
            "cached_materialization",
            None,
            &reader.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
