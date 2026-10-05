use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;
use salsa::attempt_probe::remaining_allowance_for_diagnostics;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::plumbing::function::IngredientImpl;
use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::prepared_source_probe;
use ty_python_core::ProgramFile;

use super::*;
use crate::place::global_symbol;
use crate::types::dedicated::pydantic::ConfigBoolean;
use crate::types::known_instance::{FieldInstance, InternedType};
use crate::types::mapping::source::composition_observations;
use crate::types::mapping::source::observations as mapping_observations;
use crate::types::mapping::source::observations::{
    CleanupBoundary, CleanupDrop, Lookup, OwnedMappingSnapshot, SetCleanupEvent,
};
use crate::types::mapping::specialization::SpecializationConfiguration;
use crate::types::mro::root::MroRootEffects;
use crate::types::tuple::{TupleType, VariableSegment};
use crate::types::typevar::{
    ParamSpecAttrKind, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarNonce,
};
use crate::types::{
    ApplySpecialization, BindingContext, BoundTypeVarInstance, CallableType, GenericContext,
    KnownInstanceType, MappingOperation, MaterializationOperation, Specialization, TypeMapping,
    apply_specialization_ingredient,
};

#[derive(Default)]
struct Progress {
    pools: Cell<Option<[usize; 5]>>,
    retired: Cell<bool>,
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
    specialization: Specialization<'db>,
    specialize_self_domain: bool,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    controlled_action(
        prepared,
        (ty, specialization, specialize_self_domain),
        Action::Query,
        policy,
        progress,
    )
}

#[derive(Clone, Copy)]
enum Action {
    Query,
    Initial(salsa::Id),
}

fn configured_initial<'db, C: SpecializationConfiguration>(
    db: &'db dyn Db,
    _ingredient: &IngredientImpl<C>,
    id: salsa::Id,
    input: (Type<'db>, Specialization<'db>, bool),
) -> Type<'db> {
    C::cycle_initial(db, id, input)
}

async fn provider_initial<'run, 'db: 'run, C, P>(
    db: &'db dyn Db,
    _ingredient: &IngredientImpl<C>,
    provider: &P,
    endpoint: TaskEndpoint<'run, 'db>,
    id: salsa::Id,
    input: (Type<'db>, Specialization<'db>, bool),
) -> RunResult<Type<'db>>
where
    C: SpecializationConfiguration,
    P: CallableRouteProvider<'run, 'db, C>,
{
    provider.initial(endpoint, db, id, input).await
}

fn controlled_action<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    (ty, specialization, specialize_self_domain): (Type<'db>, Specialization<'db>, bool),
    action: Action,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
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
                if let Action::Initial(id) = action {
                    let provider = SpecializationProvider {
                        program: session.program(),
                        access: move |endpoint| SourceQueryAccess {
                            session,
                            endpoint,
                            routes: routes.clone(),
                            values,
                        },
                    };
                    return provider_initial(
                        session.db(),
                        apply_specialization_ingredient(session.db()),
                        &provider,
                        endpoint,
                        id,
                        (ty, specialization, specialize_self_domain),
                    )
                    .await;
                }
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes,
                    values,
                };
                access
                    .apply_specialization(ty, specialization, specialize_self_domain)
                    .await
            })
        }));
        progress.pools.set(Some(
            [
                environments.retained_payload(),
                builders.retained_payload(),
                owners.retained_payload(),
                mapping.retained_payload(),
                checkers.retained_payload(),
            ]
            .map(|payload| payload.unwrap().0),
        ));
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

fn specialization<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    replacement: Type<'db>,
    kind: Option<MaterializationKind>,
) -> (BoundTypeVarInstance<'db>, Specialization<'db>) {
    let variable =
        BoundTypeVarInstance::synthetic(db, env, Name::new_static("T"), TypeVarVariance::Invariant);
    let context = GenericContext::from_typevar_instances(db, env, [variable]);
    (
        variable,
        Specialization::new(
            db,
            context,
            vec![replacement].into_boxed_slice(),
            kind,
            None,
        ),
    )
}

fn wrapped<'db>(db: &'db dyn Db, depth: usize) -> Type<'db> {
    let mut ty = Type::any();
    for _ in 0..depth {
        ty = TypeFormType::from_type_expression(db, ty);
    }
    ty
}

fn existing_key<'db, C: SpecializationConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    fields: &(Type<'db>, Specialization<'db>, bool),
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

fn assert_mapping<'db>(
    db: &'db TestDb,
    specialization: Specialization<'db>,
    specialize_self_domain: bool,
    children: usize,
) {
    let snapshot = mapping_observations::snapshot();
    assert_eq!(snapshot.root_count, 1);
    let root = snapshot.roots[0].expect("the canonical body retains a visitor");
    assert_ne!(root.environment, 0);
    assert_ne!(root.visitor, 0);
    assert_eq!(snapshot.child_count, children);
    assert!(
        snapshot.child_visitors[..children]
            .iter()
            .all(|visitor| *visitor == Some(root.visitor))
    );

    let invocations = mapping_observations::mapping_snapshot();
    assert_eq!(
        (invocations.root_count, invocations.child_count),
        (1, children)
    );
    let invocation = invocations.roots[0].expect("the root records its mapping request");
    let expected = OwnedMappingSnapshot::Specialization {
        specialization: specialization.as_id(),
        specialize_self_domain,
        materialization_kind: specialization.materialization_kind(db),
    };
    assert_eq!(invocation.visitor, root.visitor);
    assert_eq!(invocation.mapping, expected);
    assert!(invocation.default_context);
    assert_eq!(
        invocation.program,
        Some(specialization.generic_context(db).program(db).as_id()),
    );
    for child in invocations.children[..children].iter().flatten() {
        assert_eq!(child.visitor, root.visitor);
        assert_eq!(child.mapping, expected);
        assert!(child.default_context);
        assert_eq!(child.program, None);
    }
    assert!(invocations.children[..children].iter().all(Option::is_some));
}

#[test]
fn cold_raw_typevar_specialization_preserves_all_canonical_key_fields() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (variable, first) = specialization(&db, &env, Type::any(), None);
    let (_, second) = specialization(&db, &env, Type::Never, None);
    let raw = Type::KnownInstance(KnownInstanceType::TypeVar(variable.typevar(&db)));
    let wrapped = TypeFormType::from_type_expression(&db, raw);
    let ingredient = apply_specialization_ingredient(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut reader = db.clone();
    let mut keys = Vec::new();

    for (input, specialization, self_domain, children) in [
        (raw, first, false, 0),
        (wrapped, first, false, 1),
        (raw, second, false, 0),
        (raw, first, true, 0),
    ] {
        assert!(existing_key(&db, ingredient, &(input, specialization, self_domain)).is_none());
        mapping_observations::reset(None);
        reader.take_salsa_events();
        let progress = Progress::default();
        let cold = capture(&db, || {
            controlled(
                &prepared,
                input,
                specialization,
                self_domain,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(input)));
        assert_eq!(cold.check_root_reads(), Ok(()));
        assert_retired(&progress, 1);
        assert_mapping(&db, specialization, self_domain, children);

        let id = existing_key(&db, ingredient, &(input, specialization, self_domain))
            .expect("the source call uses the original specialization key");
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        let key = ingredient.database_key_index(id);
        assert!(keys.iter().all(|previous| *previous != key));
        keys.push(key);
        let executions: Vec<_> = reader
            .take_salsa_events()
            .into_iter()
            .filter_map(|event| match event.kind {
                salsa::EventKind::WillExecute { database_key }
                    if database_key.ingredient_index() == key.ingredient_index() =>
                {
                    Some(database_key)
                }
                _ => None,
            })
            .collect();
        assert_eq!(executions, [key]);
        let root = cold
            .reads
            .iter()
            .find(|read| read.key == key && read.parent.is_none())
            .expect("the source caller reads the canonical specialization memo");
        assert_eq!(root.status, prepared_source_probe::Status::Final);

        let ordinary = capture(&db, || {
            input.apply_specialization_impl(&db, specialization, self_domain)
        })
        .unwrap();
        assert_eq!(ordinary.value, input);
        assert!(
            ordinary
                .reads
                .iter()
                .any(|read| { read.key == key && read.memo_address == root.memo_address })
        );
        mapping_observations::reset(None);
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                input,
                specialization,
                self_domain,
                &funded(),
                &progress
            ),
            Ok(AnalysisOutcome::Complete(input)),
        );
        assert_retired(&progress, 0);
        let snapshot = mapping_observations::snapshot();
        assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
        assert_function_query_was_not_run_by_name(
            &db,
            "apply_specialization_inner",
            None,
            &reader.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn nested_specialization_preserves_owned_modes_and_retained_visitors() {
    for kind in [
        None,
        Some(MaterializationKind::Top),
        Some(MaterializationKind::Bottom),
    ] {
        for self_domain in [false, true] {
            let db = fixture();
            let prepared = prepare(&db);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let (_, specialization) = specialization(&db, &env, Type::any(), kind);
            let input = wrapped(&db, 8);
            let revision = salsa::plumbing::current_revision(&db);
            mapping_observations::reset(None);
            let progress = Progress::default();
            assert_eq!(
                controlled(
                    &prepared,
                    input,
                    specialization,
                    self_domain,
                    &funded(),
                    &progress
                ),
                Ok(AnalysisOutcome::Complete(input)),
            );
            assert_retired(&progress, 1);
            assert_mapping(&db, specialization, self_domain, 8);
            let ingredient = apply_specialization_ingredient(&db);
            let id = existing_key(&db, ingredient, &(input, specialization, self_domain)).unwrap();
            assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
            assert_eq!(salsa::plumbing::current_revision(&db), revision);

            let ordinary_db = fixture();
            let ordinary_env = ordinary_db.program_environment();
            let (_, ordinary_specialization) =
                self::specialization(&ordinary_db, &ordinary_env, Type::any(), kind);
            let ordinary_input = wrapped(&ordinary_db, 8);
            // The Any is already present in the input, so substitution does not materialize it.
            assert_eq!(
                ordinary_input.apply_specialization_impl(
                    &ordinary_db,
                    ordinary_specialization,
                    self_domain,
                ),
                ordinary_input,
            );
        }
    }
}

#[test]
fn specialization_reuses_an_ordinary_memo_without_retained_owners() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (_, specialization) =
        specialization(&db, &env, Type::any(), Some(MaterializationKind::Top));
    let input = wrapped(&db, 3);
    let ordinary = capture(&db, || {
        input.apply_specialization_impl(&db, specialization, true)
    })
    .unwrap();
    assert_eq!(ordinary.value, input);
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, specialization, true)).unwrap();
    let key = ingredient.database_key_index(id);
    let memo_address = ordinary
        .reads
        .iter()
        .find(|read| read.key == key)
        .unwrap()
        .memo_address;
    let revision = salsa::plumbing::current_revision(&db);
    let mut reader = db.clone();
    reader.take_salsa_events();
    mapping_observations::reset(None);
    let progress = Progress::default();
    let controlled = capture(&db, || {
        controlled(&prepared, input, specialization, true, &funded(), &progress)
    })
    .unwrap();
    assert_eq!(controlled.value, Ok(AnalysisOutcome::Complete(input)));
    assert!(
        controlled
            .reads
            .iter()
            .any(|read| read.key == key && read.memo_address == memo_address)
    );
    assert_retired(&progress, 0);
    let snapshot = mapping_observations::snapshot();
    assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "apply_specialization_inner",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn specialization_finish_refusals_drain_children_and_retry_the_original_query() {
    for (depth, boundary) in [
        (2, CleanupBoundary::Finish),
        (2, CleanupBoundary::Cache),
        (2, CleanupBoundary::Acceptance),
        (3, CleanupBoundary::Growth),
        (3, CleanupBoundary::Resource),
        (8, CleanupBoundary::RehashKey),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (_, specialization) =
            specialization(&db, &env, Type::any(), Some(MaterializationKind::Bottom));
        let input = wrapped(&db, depth);
        let revision = salsa::plumbing::current_revision(&db);
        mapping_observations::reset(None);
        mapping_observations::reset_cleanup(boundary, depth);
        let progress = Progress::default();
        let refused = capture(&db, || {
            controlled(&prepared, input, specialization, true, &funded(), &progress)
        })
        .unwrap();
        let expected = if boundary == CleanupBoundary::Acceptance {
            Err(AnalysisFailure::Execution(RunError::Contract(
                "completed task retained a child",
            )))
        } else {
            Ok(AnalysisOutcome::Incomplete {
                reason: if boundary == CleanupBoundary::Resource {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: (),
            })
        };
        assert_eq!(refused.value, expected, "{boundary:?} at depth {depth}");
        assert_retired(&progress, 1);
        assert_mapping(&db, specialization, true, depth);
        let cleanup = mapping_observations::cleanup_snapshot();
        assert_eq!(cleanup.finishes, depth);
        assert_eq!((cleanup.queued, cleanup.started), (1, 0));
        assert_eq!(cleanup.prepared, boundary == CleanupBoundary::Acceptance);
        assert!(!cleanup.committed);
        assert_eq!(cleanup.drop_count, 2);
        assert_eq!(
            cleanup.drops,
            [Some(CleanupDrop::Child), Some(CleanupDrop::Owner)]
        );
        let child = cleanup
            .child
            .expect("the queued child observes the active scope");
        let owner = cleanup.owner.expect("the finish future retires the scope");
        assert_eq!(child.root, Lookup::Original);
        assert_eq!(owner.root, Lookup::Absent { active: 0 });
        assert_eq!((child.active, owner.active), (Some(1), Some(0)));
        assert_eq!((child.cache_len, owner.cache_len), (depth - 1, depth - 1));
        if boundary == CleanupBoundary::Resource {
            assert!(cleanup.resource.is_some_and(|bytes| bytes > 0));
        }
        let ingredient = apply_specialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, specialization, true)).unwrap();
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        let key = ingredient.database_key_index(id);
        assert!(!refused.reads.iter().any(|read| {
            read.key == key && read.status == prepared_source_probe::Status::Final
        }));

        mapping_observations::reset(None);
        let progress = Progress::default();
        assert_eq!(
            controlled(&prepared, input, specialization, true, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(input)),
        );
        assert_retired(&progress, 1);
        assert_mapping(&db, specialization, true, depth);
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn specialization_work_refusal_retires_the_partial_query_and_retries_same_revision() {
    let depth = 8;
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    let measured_env = ProgramEnvironment::from_file(measured_prepared.program_file());
    let (_, measured_specialization) = specialization(&measured, &measured_env, Type::any(), None);
    let measured_input = wrapped(&measured, depth);
    mapping_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &measured_prepared,
            measured_input,
            measured_specialization,
            false,
            &funded(),
            &progress
        ),
        Ok(AnalysisOutcome::Complete(measured_input)),
    );
    assert_retired(&progress, 1);
    let remaining = mapping_observations::set_cleanup_snapshot()
        .events
        .into_iter()
        .flatten()
        .find_map(|event| match event {
            SetCleanupEvent::ChildEntered {
                child: 3,
                remaining,
                ..
            } => remaining,
            _ => None,
        })
        .expect("the third child records its remaining allowance");
    let policy = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - remaining,
        ..funded()
    };

    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (_, specialization) = specialization(&db, &env, Type::any(), None);
    let input = wrapped(&db, depth);
    let revision = salsa::plumbing::current_revision(&db);
    mapping_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, input, specialization, false, &policy, &progress),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        }),
    );
    assert_retired(&progress, 1);
    let snapshot = mapping_observations::snapshot();
    assert_eq!((snapshot.root_count, snapshot.child_count), (1, 3));
    assert_mapping(&db, specialization, false, 3);
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, specialization, false)).unwrap();
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    mapping_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            input,
            specialization,
            false,
            &funded(),
            &progress
        ),
        Ok(AnalysisOutcome::Complete(input)),
    );
    assert_retired(&progress, 1);
    assert_mapping(&db, specialization, false, depth);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn specialization_child_cancellation_preserves_the_completed_memo_for_retry() {
    let depth = 8;
    for cancel_at in [1, 4, depth] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (_, specialization) =
            specialization(&db, &env, Type::any(), Some(MaterializationKind::Top));
        let input = wrapped(&db, depth);
        let revision = salsa::plumbing::current_revision(&db);
        mapping_observations::reset(Some(cancel_at));
        let progress = Progress::default();
        let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, input, specialization, true, &funded(), &progress)
        }));
        assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
        assert_retired(&progress, 1);
        assert_mapping(&db, specialization, true, depth);
        let ingredient = apply_specialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, specialization, true)).unwrap();
        // Salsa completes the claimed query before delivering local cancellation to its caller.
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());

        mapping_observations::reset(None);
        let mut reader = db.clone();
        reader.take_salsa_events();
        let progress = Progress::default();
        assert_eq!(
            controlled(&prepared, input, specialization, true, &funded(), &progress),
            Ok(AnalysisOutcome::Complete(input)),
        );
        assert_retired(&progress, 0);
        let snapshot = mapping_observations::snapshot();
        assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
        assert_function_query_was_not_run_by_name(
            &db,
            "apply_specialization_inner",
            None,
            &reader.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn known_instance_specialization_preserves_identity_or_refuses_before_publication() {
    for kind in [
        None,
        Some(MaterializationKind::Top),
        Some(MaterializationKind::Bottom),
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (variable, specialization) = specialization(&db, &env, Type::any(), kind);
        let field = FieldInstance::new(
            &db,
            Some(Type::TypeVar(variable)),
            true,
            None,
            None,
            None,
            ConfigBoolean::Unspecified,
        );
        let cases = [
            (
                KnownInstanceType::Range {
                    is_non_empty: false,
                },
                None,
            ),
            (KnownInstanceType::Range { is_non_empty: true }, None),
            (KnownInstanceType::Field(field), None),
            (
                KnownInstanceType::Annotated(InternedType::new(&db, Type::TypeVar(variable))),
                Some(MappingOperation::KnownInstance),
            ),
            (
                KnownInstanceType::Callable(CallableType::unknown(&db)),
                Some(MappingOperation::Callable),
            ),
        ];
        let revision = salsa::plumbing::current_revision(&db);
        for (instance, refusal) in cases {
            let input = Type::KnownInstance(instance);
            let ingredient = apply_specialization_ingredient(&db);
            assert!(existing_key(&db, ingredient, &(input, specialization, false)).is_none());
            let mut previous_id = None;
            for attempt in 0..2 {
                mapping_observations::reset(None);
                let progress = Progress::default();
                let captured = capture(&db, || {
                    controlled(
                        &prepared,
                        input,
                        specialization,
                        false,
                        &funded(),
                        &progress,
                    )
                })
                .unwrap();
                let expected = match refusal {
                    Some(operation) => unavailable(OperationId::Specialization(
                        MaterializationOperation::Leaf(operation),
                    )),
                    None => AnalysisOutcome::Complete(input),
                };
                assert_eq!(captured.value, Ok(expected), "{instance:?}, {kind:?}");
                let roots = usize::from(attempt == 0 || refusal.is_some());
                assert_retired(&progress, roots);
                if roots == 1 {
                    assert_mapping(&db, specialization, false, 0);
                } else {
                    let snapshot = mapping_observations::snapshot();
                    assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
                }
                let id = existing_key(&db, ingredient, &(input, specialization, false)).unwrap();
                if let Some(previous) = previous_id.replace(id) {
                    assert_eq!(id, previous);
                }
                let key = ingredient.database_key_index(id);
                let memo = FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ());
                if refusal.is_some() {
                    assert_eq!(memo, Err(FinalSourceError::MissingMemo));
                    assert!(!captured.reads.iter().any(|read| {
                        read.key == key && read.status == prepared_source_probe::Status::Final
                    }));
                } else {
                    assert_eq!(memo, Ok(()));
                    assert_eq!(captured.check_root_reads(), Ok(()));
                }
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
            }
        }
    }
}

#[test]
fn specialization_rejects_a_foreign_program_before_retaining_owners() {
    let db = fixture();
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let platform = if *program.python_platform(&db) == PythonPlatform::All {
        PythonPlatform::Identifier("linux".into())
    } else {
        PythonPlatform::All
    };
    let foreign = Program::new(&db, &platform, program.resolver_environment(&db));
    assert_ne!(foreign, program);
    let foreign_env = ProgramEnvironment::from_program(foreign);
    let (_, foreign_specialization) = specialization(&db, &foreign_env, Type::any(), None);
    let input = wrapped(&db, 2);
    let revision = salsa::plumbing::current_revision(&db);
    mapping_observations::reset(None);
    let progress = Progress::default();
    let captured = capture(&db, || {
        controlled(
            &prepared,
            input,
            foreign_specialization,
            false,
            &funded(),
            &progress,
        )
    })
    .unwrap();
    assert_eq!(
        captured.value,
        Err(AnalysisFailure::Execution(RunError::Contract(
            "source program is foreign"
        ))),
    );
    assert_retired(&progress, 0);
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &(input, foreign_specialization, false)).unwrap();
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

    let env = ProgramEnvironment::from_program(program);
    let (_, local_specialization) = specialization(&db, &env, Type::any(), None);
    mapping_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(
            &prepared,
            input,
            local_specialization,
            false,
            &funded(),
            &progress
        ),
        Ok(AnalysisOutcome::Complete(input)),
    );
    assert_retired(&progress, 1);
    assert_mapping(&db, local_specialization, false, 2);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn specialization_provider_initial_uses_the_original_configuration_and_query_id() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (_, specialization) = specialization(&db, &env, Type::any(), None);
    let input = wrapped(&db, 1);
    let fields = (input, specialization, true);
    mapping_observations::reset(None);
    let progress = Progress::default();
    assert_eq!(
        controlled(&prepared, input, specialization, true, &funded(), &progress),
        Ok(AnalysisOutcome::Complete(input)),
    );
    assert_retired(&progress, 1);
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &fields).unwrap();
    let expected = configured_initial(&db, ingredient, id, fields);
    assert_eq!(expected, Type::divergent(id));
    let revision = salsa::plumbing::current_revision(&db);
    mapping_observations::reset(None);
    let mut reader = db.clone();
    reader.take_salsa_events();
    let progress = Progress::default();
    // Invoke the actual provider callback directly; this does not create a query cycle.
    assert_eq!(
        controlled_action(&prepared, fields, Action::Initial(id), &funded(), &progress),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    assert_retired(&progress, 0);
    let snapshot = mapping_observations::snapshot();
    assert_eq!((snapshot.root_count, snapshot.child_count), (0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "apply_specialization_inner",
        None,
        &reader.take_salsa_events(),
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[derive(Clone, Copy)]
struct ComposeSpecializations<'db> {
    base: Specialization<'db>,
    additional: Specialization<'db>,
}

impl<'db> super::nominal_members::MemberOperation<'db> for ComposeSpecializations<'db> {
    type Output = Specialization<'db>;
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        SourceEffects::new(access, program)
            .compose_source_specialization(self.base, self.additional)
            .await
    }
}

#[test]
fn source_composition_retains_one_visitor_for_actual_arguments_and_retries_cancellation() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (_, base) = specialization(&db, &env, Type::bool_literal(false), None);
    let (_, additional) = specialization(&db, &env, Type::bool_literal(true), None);
    let request = ComposeSpecializations { base, additional };
    let revision = salsa::plumbing::current_revision(&db);
    composition_observations::reset();
    mapping_observations::reset(None);
    let result = capture(&db, || {
        super::nominal_members::controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    assert_eq!(result.value, Ok(AnalysisOutcome::Complete(base)));
    assert!(result.reads.is_empty());
    let root =
        composition_observations::snapshot().expect("composition records its retained visitor");
    assert_eq!(
        (root.base, root.additional, root.program),
        (base.as_id(), additional.as_id(), env.program(&db).as_id())
    );
    assert_ne!(root.environment, 0);
    let mapping = mapping_observations::mapping_snapshot();
    assert_eq!((mapping.root_count, mapping.child_count), (0, 1));
    let child = mapping.children[0].expect("the stored argument is a Type child");
    assert_eq!(child.visitor, root.visitor);
    assert!(child.default_context);
    assert_eq!(
        child.mapping,
        OwnedMappingSnapshot::Specialization {
            specialization: additional.as_id(),
            specialize_self_domain: false,
            materialization_kind: None
        }
    );

    let ordinary_db = fixture();
    let ordinary_env = ordinary_db.program_environment();
    let (_, ordinary_base) =
        specialization(&ordinary_db, &ordinary_env, Type::bool_literal(false), None);
    let (_, ordinary_additional) =
        specialization(&ordinary_db, &ordinary_env, Type::bool_literal(true), None);
    let ordinary = ordinary_base.apply_specialization_impl(
        &ordinary_db,
        ordinary_additional,
        &crate::types::ApplyTypeMappingVisitor::new(&ordinary_env),
    );
    assert_eq!(ordinary, ordinary_base);
    assert_eq!(base.types(&db), ordinary.types(&ordinary_db));

    mapping_observations::reset(Some(1));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        super::nominal_members::controlled_member_operation(&prepared, request, &funded())
    }));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert_no_active_attempt();
    mapping_observations::reset(None);
    assert_eq!(
        super::nominal_members::controlled_member_operation(&prepared, request, &funded()),
        Ok(AnalysisOutcome::Complete(base))
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn source_composition_substitutes_stored_typevars_and_preserves_materialization_refusal() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (variable, additional) = specialization(&db, &env, Type::bool_literal(true), None);
    let base = Specialization::new(
        &db,
        additional.generic_context(&db),
        vec![Type::TypeVar(variable)].into_boxed_slice(),
        None,
        None,
    );
    assert_eq!(
        super::nominal_members::controlled_member_operation(
            &prepared,
            ComposeSpecializations { base, additional },
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(additional)),
    );

    let ordinary_db = fixture();
    let ordinary_env = ordinary_db.program_environment();
    let (ordinary_variable, ordinary_additional) =
        specialization(&ordinary_db, &ordinary_env, Type::bool_literal(true), None);
    let ordinary_base = Specialization::new(
        &ordinary_db,
        ordinary_additional.generic_context(&ordinary_db),
        Box::from([Type::TypeVar(ordinary_variable)]),
        None,
        None,
    );
    let ordinary = ordinary_base.apply_specialization_impl(
        &ordinary_db,
        ordinary_additional,
        &crate::types::ApplyTypeMappingVisitor::new(&ordinary_env),
    );
    assert_eq!(ordinary, ordinary_additional);
    assert_eq!(additional.types(&db), ordinary.types(&ordinary_db));

    let (_, base) = specialization(&db, &env, Type::bool_literal(false), None);
    let additional = additional.with_materialization_kind(&db, Some(MaterializationKind::Top));
    assert_eq!(
        super::nominal_members::controlled_member_operation(
            &prepared,
            ComposeSpecializations { base, additional },
            &funded()
        ),
        Ok(unavailable(OperationId::Materialization(
            MaterializationOperation::Leaf(MappingOperation::MaterializationOrPolarity)
        ))),
    );
    assert_no_active_attempt();
}

#[test]
fn full_specialization_lookup_preserves_identity_and_unfilled_arguments() {
    let mut db = fixture();
    db.write_file("src/main.py", "class Owner: ...\nleft = right = 1\n")
        .unwrap();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let owner = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(Stmt::as_class_def_stmt)
        .unwrap();
    let definition = prepared.semantic_index().expect_single_definition(owner);
    let (first, _) = specialization(&db, &env, Type::Never, None);
    let second = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("U"),
        TypeVarVariance::Invariant,
    );
    let typevar = first.typevar(&db);
    let declared = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(
                &db,
                Name::new_static("T"),
                Some(definition),
                TypeVarKind::Pep695TypeVar,
            ),
            None,
            typevar.explicit_variance(&db),
            None,
        ),
        first.binding_context(&db),
        None,
        TypeVarNonce::NONE,
    );
    let rebound = BoundTypeVarInstance::new(
        &db,
        typevar,
        BindingContext::Definition(definition),
        None,
        TypeVarNonce::NONE,
    );
    let fresh = BoundTypeVarInstance::new(
        &db,
        typevar,
        first.binding_context(&db),
        None,
        TypeVarNonce::NONE.increment(),
    );
    let altered = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            typevar.identity(&db),
            None,
            typevar.explicit_variance(&db),
            Some(TypeVarDefaultEvaluation::Eager(Type::bool_literal(false))),
        ),
        first.binding_context(&db),
        None,
        TypeVarNonce::NONE,
    );
    assert_ne!(altered, first);
    assert_eq!(altered.identity(&db), first.identity(&db));
    let context = GenericContext::from_typevar_instances(&db, &env, [first, second]);
    let replacement = Type::bool_literal(true);
    let revision = salsa::plumbing::current_revision(&db);
    for (variable, arguments, lookup) in [
        (first, vec![replacement], Some(replacement)),
        (altered, vec![replacement], Some(replacement)),
        (declared, vec![replacement], None),
        (rebound, vec![replacement], None),
        (fresh, vec![replacement], None),
        (second, vec![replacement], None),
        (first, vec![], None),
    ] {
        let specialization =
            Specialization::new(&db, context, arguments.into_boxed_slice(), None, None);
        let input = Type::TypeVar(variable);
        let expected = lookup.unwrap_or(input);
        mapping_observations::reset(None);
        let progress = Progress::default();
        let captured = capture(&db, || {
            controlled(
                &prepared,
                input,
                specialization,
                false,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(expected)));
        assert_eq!(captured.check_root_reads(), Ok(()));
        assert_retired(&progress, 1);
        assert_mapping(&db, specialization, false, 0);
        assert_eq!(specialization.get(&db, variable), lookup);
        assert_eq!(
            input.apply_type_mapping(
                &db,
                &env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::specialization(
                    specialization,
                )),
                TypeContext::default(),
            ),
            expected,
        );
        let ingredient = apply_specialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, specialization, false)).unwrap();
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn nested_full_specialization_transfers_replacements_without_substituting_them_again() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (first, _) = specialization(&db, &env, Type::Never, None);
    let second = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("U"),
        TypeVarVariance::Invariant,
    );
    let context = GenericContext::from_typevar_instances(&db, &env, [first, second]);
    let specialization = Specialization::new(
        &db,
        context,
        Box::from([Type::TypeVar(second), Type::bool_literal(true)]),
        None,
        None,
    );
    let input = Type::heterogeneous_tuple(
        &db,
        &env,
        [
            Type::TypeVar(first),
            TypeFormType::from_type_expression(&db, Type::TypeVar(first)),
            Type::TypeVar(second),
        ],
    );
    let expected = Type::heterogeneous_tuple(
        &db,
        &env,
        [
            Type::TypeVar(second),
            TypeFormType::from_type_expression(&db, Type::TypeVar(second)),
            Type::bool_literal(true),
        ],
    );
    mapping_observations::reset(None);
    let progress = Progress::default();
    let captured = capture(&db, || {
        controlled(
            &prepared,
            input,
            specialization,
            false,
            &funded(),
            &progress,
        )
    })
    .unwrap();
    assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(expected)));
    assert_eq!(captured.check_root_reads(), Ok(()));
    assert_retired(&progress, 1);
    assert_mapping(&db, specialization, false, 4);
    assert_eq!(mapping_observations::tuple_snapshot().live, 0);
    assert_eq!(
        input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::ApplySpecialization(ApplySpecialization::specialization(specialization)),
            TypeContext::default(),
        ),
        expected,
    );
}

/// P, P.args and P.kwargs refuse with `MappingOperation::ParamSpec`. Retained Self keeps its original
/// domain when `specialize_self_domain` is disabled, and specializes only that domain when enabled,
/// preserving occurrence metadata. Successful canonical results are published and match ordinary mapping.
#[test]
fn full_specialization_preserves_paramspec_and_retained_self_boundaries() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (first, specialization) = specialization(&db, &env, Type::bool_literal(true), None);
    let binding = BindingContext::Synthetic(env.program(&db));
    let paramspec = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(
                &db,
                Name::new_static("P"),
                None,
                TypeVarKind::LegacyParamSpec,
            ),
            None,
            None,
            None,
        ),
        binding,
        None,
        TypeVarNonce::NONE,
    );
    let retained_self = BoundTypeVarInstance::synthetic_self(&db, Type::TypeVar(first), binding);
    let revision = salsa::plumbing::current_revision(&db);
    for (variable, self_domain, refusal) in [
        (paramspec, false, Some(MappingOperation::ParamSpec)),
        (
            paramspec.with_paramspec_attr(&db, ParamSpecAttrKind::Args),
            false,
            Some(MappingOperation::ParamSpec),
        ),
        (
            paramspec.with_paramspec_attr(&db, ParamSpecAttrKind::Kwargs),
            false,
            Some(MappingOperation::ParamSpec),
        ),
        (retained_self, false, None),
        (retained_self, true, None),
    ] {
        let input = Type::TypeVar(variable);
        mapping_observations::reset(None);
        let progress = Progress::default();
        let captured = capture(&db, || {
            controlled(
                &prepared,
                input,
                specialization,
                self_domain,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        let mapped = if self_domain {
            Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                &db, Type::bool_literal(true), binding,
            ))
        } else {
            input
        };
        let expected = refusal.map(|operation| {
            unavailable(OperationId::Specialization(MaterializationOperation::Leaf(operation)))
        }).unwrap_or(AnalysisOutcome::Complete(mapped));
        assert_eq!(captured.value, Ok(expected));
        assert_retired(&progress, if self_domain { 2 } else { 1 });
        if self_domain {
            let snapshot = mapping_observations::mapping_snapshot();
            assert_eq!((snapshot.root_count, snapshot.child_count), (2, 1));
            let outer = snapshot.roots[0].expect("canonical specialization visitor");
            let bounds = snapshot.roots[1].expect("retained bounds visitor");
            assert_ne!(outer.visitor, bounds.visitor);
            assert_eq!(outer.program, Some(env.program(&db).as_id()));
            assert_eq!(outer.program, bounds.program);
            assert_eq!(outer.mapping, OwnedMappingSnapshot::Specialization {
                specialization: specialization.as_id(), specialize_self_domain: true,
                materialization_kind: None,
            });
            assert_eq!(bounds.mapping, OwnedMappingSnapshot::Specialization {
                specialization: specialization.as_id(), specialize_self_domain: false,
                materialization_kind: None,
            });
            let child = snapshot.children[0].expect("mapped upper bound");
            assert_eq!(child.visitor, bounds.visitor);
            assert_eq!(child.mapping, bounds.mapping);
            assert!(outer.default_context && bounds.default_context && child.default_context);
            let Type::TypeVar(mapped) = mapped else { panic!("retained Self result"); };
            assert_eq!(mapped.identity(&db), variable.identity(&db));
            assert_eq!(mapped.binding_context(&db), variable.binding_context(&db));
            assert_eq!(mapped.freshness(&db), variable.freshness(&db));
            assert_eq!(mapped.typevar(&db).explicit_variance(&db), variable.typevar(&db).explicit_variance(&db));
        } else {
            assert_mapping(&db, specialization, self_domain, 0);
        }
        let ingredient = apply_specialization_ingredient(&db);
        let id = existing_key(&db, ingredient, &(input, specialization, self_domain)).unwrap();
        let memo = FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ());
        if refusal.is_some() {
            assert_eq!(memo, Err(FinalSourceError::MissingMemo));
            let key = ingredient.database_key_index(id);
            assert!(!captured.reads.iter().any(|read| {
                read.key == key && read.status == prepared_source_probe::Status::Final
            }));
        } else {
            assert_eq!(memo, Ok(()));
            assert_eq!(captured.check_root_reads(), Ok(()));
            let key = ingredient.database_key_index(id);
            assert!(captured.reads.iter().any(|read| {
                read.key == key && read.status == prepared_source_probe::Status::Final
            }));
            assert_eq!(mapped, input.apply_type_mapping(
                &db, &env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                    specialization, specialize_self_domain: self_domain,
                }),
                TypeContext::default(),
            ));
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn source_composition_cancellation_after_a_changed_argument_drains_children_and_retries() {
    let db = fixture();
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (first, _) = specialization(&db, &env, Type::Never, None);
    let second = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("U"),
        TypeVarVariance::Invariant,
    );
    let context = GenericContext::from_typevar_instances(&db, &env, [first, second]);
    let base = Specialization::new(
        &db,
        context,
        Box::from([
            Type::TypeVar(first),
            TypeFormType::from_type_expression(&db, Type::TypeVar(second)),
        ]),
        None,
        None,
    );
    let additional = Specialization::new(
        &db,
        context,
        Box::from([Type::bool_literal(true), Type::bool_literal(false)]),
        None,
        None,
    );
    let request = ComposeSpecializations { base, additional };
    let revision = salsa::plumbing::current_revision(&db);
    let specializations_before = Specialization::ingredient(db.zalsa())
        .entries(db.zalsa())
        .count();
    composition_observations::reset();
    mapping_observations::reset(Some(2));
    // The first argument changes before composition starts the second stored argument's Type child.
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        super::nominal_members::controlled_member_operation(&prepared, request, &funded())
    }));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert_no_active_attempt();
    let composition = composition_observations::snapshot().unwrap();
    let mappings = mapping_observations::mapping_snapshot();
    assert_eq!(mappings.root_count, 0);
    assert!(mappings.child_count >= 2);
    for child in mappings.children[..mappings.child_count].iter().flatten() {
        assert_eq!(child.visitor, composition.visitor);
        assert_eq!(
            child.mapping,
            OwnedMappingSnapshot::Specialization {
                specialization: additional.as_id(),
                specialize_self_domain: false,
                materialization_kind: None,
            },
        );
    }
    let cleanup = mapping_observations::set_cleanup_snapshot();
    let events = &cleanup.events[..cleanup.event_count];
    let first_dropped = events
        .iter()
        .position(|event| matches!(event, Some(SetCleanupEvent::ChildDropped { child: 1, .. })))
        .unwrap();
    let second_entered = events
        .iter()
        .position(|event| matches!(event, Some(SetCleanupEvent::ChildEntered { child: 2, .. })))
        .unwrap();
    assert!(first_dropped < second_entered);
    for child in 1..=mappings.child_count {
        assert_eq!(
            events
                .iter()
                .filter(|event| {
                    matches!(event, Some(SetCleanupEvent::ChildDropped { child: dropped, .. }) if *dropped == child)
                })
                .count(),
            1,
        );
    }
    assert_eq!(mapping_observations::tuple_snapshot().live, 0);
    assert_eq!(mapping_observations::set_snapshot().live, 0);
    assert_eq!(
        Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count(),
        specializations_before,
    );

    composition_observations::reset();
    mapping_observations::reset(None);
    let retried =
        super::nominal_members::controlled_member_operation(&prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(result)) = retried else {
        panic!("composition retry: {retried:?}");
    };
    assert_ne!(result, base);
    assert_ne!(result, additional);
    assert_eq!(result.generic_context(&db), context);
    assert_eq!(
        result.types(&db),
        &[
            Type::bool_literal(true),
            TypeFormType::from_type_expression(&db, Type::bool_literal(false)),
        ],
    );
    assert_eq!(result.materialization_kind(&db), None);
    assert_eq!(result.tuple(&db), None);
    assert_eq!(
        base.apply_specialization_impl(
            &db,
            additional,
            &crate::types::ApplyTypeMappingVisitor::new(&env),
        ),
        result,
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

fn generic_alias_mapping_database() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("src/main.py", "class Container[T]: ...\n")?;
    Ok(db)
}

fn generic_alias_mapping_input<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    changed: bool,
) -> anyhow::Result<(GenericAlias<'db>, Specialization<'db>)> {
    let file = ProgramFile::new(db, system_path_to_file(db, "src/main.py")?, env.program(db));
    let origin = global_symbol(db, file, "Container")
        .place
        .ignore_possibly_undefined()
        .and_then(Type::as_class_literal)
        .and_then(ClassLiteral::as_static)
        .ok_or_else(|| anyhow::anyhow!("Container is not a static class"))?;
    let context = origin
        .generic_context(db)
        .ok_or_else(|| anyhow::anyhow!("Container has no generic context"))?;
    let (variable, mapping) = specialization(db, env, Type::bool_literal(true), None);
    let argument = if changed {
        Type::TypeVar(variable)
    } else {
        Type::bool_literal(false)
    };
    let stored = Specialization::new(db, context, Box::from([argument]), None, None);
    Ok((GenericAlias::new(db, origin, stored), mapping))
}

/// Controlled full specialization preserves an unchanged alias and canonically rebuilds an alias
/// whose stored TypeVar argument changes to a literal, matching ordinary mapping in a separate database.
#[test]
fn source_generic_alias_mapping_preserves_canonical_results() -> anyhow::Result<()> {
    for changed in [false, true] {
        let db = generic_alias_mapping_database()?;
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (input, mapping) = generic_alias_mapping_input(&db, &env, changed)?;
        let origin = input.origin(&db);
        let stored = input.specialization(&db);
        let aliases_before = GenericAlias::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count();
        let specializations_before = Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count();
        let ingredient = apply_specialization_ingredient(&db);
        let query = (Type::GenericAlias(input), mapping, false);
        assert!(existing_key(&db, ingredient, &query).is_none());
        let revision = salsa::plumbing::current_revision(&db);
        mapping_observations::reset(None);
        let progress = Progress::default();
        let result = controlled(&prepared, query.0, mapping, false, &funded(), &progress);
        let Ok(AnalysisOutcome::Complete(Type::GenericAlias(actual))) = result else {
            anyhow::bail!("controlled generic alias mapping: {result:?}");
        };
        assert_retired(&progress, 1);
        assert_mapping(&db, mapping, false, 1);
        assert_eq!(actual == input, !changed);
        assert_eq!(actual.origin(&db), origin);
        assert_eq!(actual.specialization(&db) == stored, !changed);
        assert_eq!(
            GenericAlias::ingredient(db.zalsa())
                .entries(db.zalsa())
                .count(),
            aliases_before + usize::from(changed),
        );
        assert_eq!(
            Specialization::ingredient(db.zalsa())
                .entries(db.zalsa())
                .count(),
            specializations_before + usize::from(changed),
        );
        let expected = Specialization::new(
            &db,
            stored.generic_context(&db),
            Box::from([Type::bool_literal(changed)]),
            None,
            None,
        );
        assert_eq!(actual.specialization(&db), expected);
        assert_eq!(actual, GenericAlias::new(&db, origin, expected));
        let id = existing_key(&db, ingredient, &query)
            .ok_or_else(|| anyhow::anyhow!("generic alias mapping did not retain its key"))?;
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());

        let ordinary_db = generic_alias_mapping_database()?;
        let ordinary_env = ordinary_db.program_environment();
        let (ordinary_input, ordinary_mapping) =
            generic_alias_mapping_input(&ordinary_db, &ordinary_env, changed)?;
        let ordinary = Type::GenericAlias(ordinary_input).apply_specialization_impl(
            &ordinary_db,
            ordinary_mapping,
            false,
        );
        let Type::GenericAlias(ordinary) = ordinary else {
            anyhow::bail!("ordinary generic alias mapping: {ordinary:?}");
        };
        assert_eq!(ordinary == ordinary_input, !changed);
        assert_eq!(
            ordinary.origin(&ordinary_db),
            ordinary_input.origin(&ordinary_db)
        );
        assert_eq!(
            origin.name(&db),
            ordinary.origin(&ordinary_db).name(&ordinary_db)
        );
        assert_eq!(
            actual.specialization(&db).types(&db),
            ordinary.specialization(&ordinary_db).types(&ordinary_db)
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    Ok(())
}

/// A work refusal after specialization interning leaves alias reconstruction and the enclosing
/// specialization query incomplete. A funded retry in the same revision reuses the specialization
/// and completes.
#[test]
fn source_generic_alias_mapping_refusal_preserves_specialization_for_retry() -> anyhow::Result<()> {
    let measured = generic_alias_mapping_database()?;
    let prepared = prepare(&measured);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (input, mapping) = generic_alias_mapping_input(&measured, &env, true)?;
    mapping_observations::reset(None);
    let progress = Progress::default();
    let result = controlled(
        &prepared,
        Type::GenericAlias(input),
        mapping,
        false,
        &funded(),
        &progress,
    );
    assert!(matches!(
        result,
        Ok(AnalysisOutcome::Complete(Type::GenericAlias(_)))
    ));
    assert_retired(&progress, 1);
    let remaining = mapping_observations::set_cleanup_snapshot()
        .events
        .into_iter()
        .flatten()
        .find_map(|event| match event {
            SetCleanupEvent::ChildEntered {
                child: 1,
                remaining,
                ..
            } => remaining,
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("alias argument did not record its allowance"))?;
    let mut lower = funded().semantic_work_limit - remaining;
    let mut upper = funded().semantic_work_limit;

    // Find the first allowance that interns the changed specialization, using a fresh database
    // for each attempt so no earlier attempt can reduce the work needed for this mapping.
    while upper - lower > 1 {
        let limit = lower + (upper - lower) / 2;
        let db = generic_alias_mapping_database()?;
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let (input, mapping) = generic_alias_mapping_input(&db, &env, true)?;
        let count = Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count();
        mapping_observations::reset(None);
        let progress = Progress::default();
        let result = controlled(
            &prepared,
            Type::GenericAlias(input),
            mapping,
            false,
            &AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            &progress,
        );
        assert!(matches!(
            result,
            Ok(AnalysisOutcome::Complete(_))
                | Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
        ));
        assert_retired(&progress, 1);
        let after = Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count();
        assert!(after == count || after == count + 1);
        if after == count + 1 {
            upper = limit;
        } else {
            lower = limit;
        }
    }

    let db = generic_alias_mapping_database()?;
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let (input, mapping) = generic_alias_mapping_input(&db, &env, true)?;
    let query = (Type::GenericAlias(input), mapping, false);
    let revision = salsa::plumbing::current_revision(&db);
    let aliases_before = GenericAlias::ingredient(db.zalsa())
        .entries(db.zalsa())
        .count();
    let specializations_before = Specialization::ingredient(db.zalsa())
        .entries(db.zalsa())
        .count();
    mapping_observations::reset(None);
    let progress = Progress::default();
    let refused = capture(&db, || {
        controlled(
            &prepared,
            query.0,
            mapping,
            false,
            &AnalysisPolicy {
                semantic_work_limit: upper,
                ..funded()
            },
            &progress,
        )
    })
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(
        refused.value,
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_retired(&progress, 1);
    assert_mapping(&db, mapping, false, 1);
    assert_eq!(
        GenericAlias::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count(),
        aliases_before
    );
    assert_eq!(
        Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count(),
        specializations_before + 1
    );
    let cleanup = mapping_observations::set_cleanup_snapshot();
    assert_eq!(
        cleanup.events[..cleanup.event_count]
            .iter()
            .filter(|event| matches!(event, Some(SetCleanupEvent::ChildDropped { child: 1, .. })))
            .count(),
        1
    );
    assert_eq!(mapping_observations::tuple_snapshot().live, 0);
    assert_eq!(mapping_observations::set_snapshot().live, 0);
    let ingredient = apply_specialization_ingredient(&db);
    let id = existing_key(&db, ingredient, &query)
        .ok_or_else(|| anyhow::anyhow!("refused generic alias mapping did not retain its key"))?;
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let key = ingredient.database_key_index(id);
    assert!(
        !refused
            .reads
            .iter()
            .any(|read| read.key == key && read.status == prepared_source_probe::Status::Final)
    );

    let retained = Specialization::new(
        &db,
        input.specialization(&db).generic_context(&db),
        Box::from([Type::bool_literal(true)]),
        None,
        None,
    );
    assert_eq!(
        Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count(),
        specializations_before + 1
    );
    mapping_observations::reset(None);
    let progress = Progress::default();
    let retry = controlled(&prepared, query.0, mapping, false, &funded(), &progress);
    let Ok(AnalysisOutcome::Complete(Type::GenericAlias(actual))) = retry else {
        anyhow::bail!("generic alias mapping retry: {retry:?}");
    };
    assert_retired(&progress, 1);
    assert_mapping(&db, mapping, false, 1);
    assert_eq!(actual.origin(&db), input.origin(&db));
    assert_eq!(actual.specialization(&db), retained);
    assert_eq!(actual, GenericAlias::new(&db, input.origin(&db), retained));
    assert_eq!(
        GenericAlias::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count(),
        aliases_before + 1
    );
    assert_eq!(
        Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count(),
        specializations_before + 1
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

#[derive(Clone, Copy)]
struct RuntimeTupleSpecialization<'a, 'db> {
    input: Specialization<'db>,
    before: &'a Cell<Option<usize>>,
}

impl<'db> super::nominal_members::MemberOperation<'db> for RuntimeTupleSpecialization<'_, 'db> {
    type Output = (Specialization<'db>, usize);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        self.before
            .set(remaining_allowance_for_diagnostics(access.db()));
        let result = MroRootEffects::tuple_runtime_specialization(
            &SourceEffects::new(access, program),
            self.input,
        )
        .await?;
        let remaining = remaining_allowance_for_diagnostics(access.db()).ok_or(
            RunError::Contract("tuple runtime test requires an active allowance"),
        )?;
        Ok((result, remaining))
    }
}

fn runtime_tuple_input<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    symbolic: bool,
) -> Specialization<'db> {
    let (_, plain) = specialization(db, env, Type::any(), Some(MaterializationKind::Bottom));
    if !symbolic {
        return plain;
    }
    let pack = BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
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
        ),
        BindingContext::Synthetic(env.program(db)),
        None,
        TypeVarNonce::NONE,
    );
    let tuple = TupleType::mixed_with_segment(
        db,
        env,
        [Type::int_literal(1)],
        VariableSegment::TypeVarTuple(pack),
        [Type::bool_literal(false)],
    );
    Specialization::new(
        db,
        plain.generic_context(db),
        Box::from([Type::TypeVar(pack)]),
        plain.materialization_kind(db),
        Some(tuple),
    )
}

/// Controlled MRO normalization preserves ordinary results and leaves the input's symbolic tuple
/// shape unchanged.
#[test]
fn source_tuple_runtime_specialization_preserves_canonical_results() -> anyhow::Result<()> {
    for symbolic in [false, true] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let input = runtime_tuple_input(&db, &env, symbolic);
        let original_tuple = input.tuple(&db);
        let count = Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count();
        let revision = salsa::plumbing::current_revision(&db);
        let before = Cell::new(None);
        let captured = capture(&db, || {
            super::nominal_members::controlled_member_operation(
                &prepared,
                RuntimeTupleSpecialization {
                    input,
                    before: &before,
                },
                &funded(),
            )
        })
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let Ok(AnalysisOutcome::Complete((actual, remaining))) = captured.value else {
            anyhow::bail!("controlled tuple runtime result: {:?}", captured.value);
        };
        assert!(before.get().is_some_and(|before| before > remaining));
        assert!(captured.reads.is_empty());
        assert_eq!(actual.generic_context(&db), input.generic_context(&db));
        assert_eq!(
            actual.materialization_kind(&db),
            input.materialization_kind(&db)
        );
        assert_eq!(actual.tuple(&db), None);
        assert_eq!(input.tuple(&db), original_tuple);
        assert_eq!(
            Specialization::ingredient(db.zalsa())
                .entries(db.zalsa())
                .count(),
            count + usize::from(symbolic),
        );
        if symbolic {
            assert_ne!(actual, input);
            assert_eq!(actual.types(&db), [Type::object()]);
        } else {
            assert_eq!(actual, input);
        }

        let ordinary_db = fixture();
        let ordinary_env = ordinary_db.program_environment();
        let ordinary_input = runtime_tuple_input(&ordinary_db, &ordinary_env, symbolic);
        let ordinary = ordinary_input.tuple_runtime_element_specialization(&ordinary_db);
        assert_eq!(actual.types(&db), ordinary.types(&ordinary_db));
        assert_eq!(
            actual.materialization_kind(&db),
            ordinary.materialization_kind(&ordinary_db)
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    Ok(())
}

/// Work refusal withholds the caller result before construction and after interning; a new
/// invocation in the same revision completes and reuses any specialization already interned.
#[test]
fn source_tuple_runtime_refusals_withhold_results_and_retry() -> anyhow::Result<()> {
    let measured_db = fixture();
    let measured_prepared = prepare(&measured_db);
    let measured_env = ProgramEnvironment::from_file(measured_prepared.program_file());
    let measured_input = runtime_tuple_input(&measured_db, &measured_env, true);
    let before = Cell::new(None);
    let measured = super::nominal_members::controlled_member_operation(
        &measured_prepared,
        RuntimeTupleSpecialization {
            input: measured_input,
            before: &before,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete((_, after))) = measured else {
        anyhow::bail!("tuple runtime calibration: {measured:?}");
    };
    let before = before
        .get()
        .ok_or_else(|| anyhow::anyhow!("missing entry allowance"))?;
    let before_work = funded().semantic_work_limit - before;
    // The successful operation's last explicit work admission is Publish. One fewer unit
    // withholds its result after interning has completed.
    let before_publication = funded().semantic_work_limit - after - 1;
    assert!(before_work < before_publication);

    for (limit, interned) in [(before_work, false), (before_publication, true)] {
        let db = fixture();
        let prepared = prepare(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let input = runtime_tuple_input(&db, &env, true);
        let before = Cell::new(None);
        let request = RuntimeTupleSpecialization {
            input,
            before: &before,
        };
        let revision = salsa::plumbing::current_revision(&db);
        let count = Specialization::ingredient(db.zalsa())
            .entries(db.zalsa())
            .count();
        assert_eq!(
            super::nominal_members::controlled_member_operation(
                &prepared,
                request,
                &AnalysisPolicy {
                    semantic_work_limit: limit,
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
        );
        assert!(before.get().is_some());
        assert_eq!(
            Specialization::ingredient(db.zalsa())
                .entries(db.zalsa())
                .count(),
            count + usize::from(interned),
        );
        assert_no_active_attempt();
        let retried =
            super::nominal_members::controlled_member_operation(&prepared, request, &funded());
        let Ok(AnalysisOutcome::Complete((actual, _))) = retried else {
            anyhow::bail!("tuple runtime retry: {retried:?}");
        };
        assert_eq!(actual.types(&db), [Type::object()]);
        assert_eq!(
            actual.materialization_kind(&db),
            Some(MaterializationKind::Bottom)
        );
        assert_eq!(actual.tuple(&db), None);
        assert_eq!(
            Specialization::ingredient(db.zalsa())
                .entries(db.zalsa())
                .count(),
            count + 1,
        );
        assert_eq!(actual, input.tuple_runtime_element_specialization(&db));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
    Ok(())
}
