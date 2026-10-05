mod annotated_declarations;
pub(in crate::types::infer) mod bound_defaults;
pub(in crate::types::infer) mod callable_guard;
pub(super) mod canonical_place;
pub(in crate::types::infer) mod comparison_guard;
pub(in crate::types::infer) mod class_decorators;
pub(in crate::types::infer) mod class_object_member;
mod constraint_application;
pub(in crate::types::infer) mod constructor_preparation;
pub(in crate::types::infer) mod constructor_matching;
mod constructor_self_mapping;
mod receiver_constraints;
pub(super) mod contextual_expression;
pub(in crate::types::infer) mod contextual_tuple;
mod continuation_allocation;
mod cycle_policy;
mod dataclass_transform;
pub(in crate::types::infer) mod default_binding;
pub(super) mod default_recovery;
pub(in crate::types::infer) mod default_self_reference;
pub(in crate::types::infer) mod deferred_assignments;
mod disjointness;
pub(in crate::types::infer) mod dunder_all;
mod dunder_callable;
pub(in crate::types::infer) mod explicit_specialization;
mod expression_search;
pub(in crate::types::infer) mod function_decorator_ingestion;
pub(in crate::types::infer) mod function_decorators;
pub(in crate::types::infer) mod type_parameter_shadow;
mod function_descriptor_update;
mod generic_intersection;
pub(in crate::types::infer) mod inheritance_cycle;
pub(in crate::types::infer) mod inner_metaclass;
pub(in crate::types::infer) mod instance_layout;
mod intersection_expansion;
mod intersection_finalization;
mod intersection_insertion;
mod intersection_simplification;
mod known_instance_conversion;
pub(in crate::types::infer) mod lazy_defaults;
pub(in crate::types::infer) mod legacy_context;
mod literal_meta_type;
mod materialization;
mod narrowing_evaluation;
mod narrowing_storage;
mod native_values;
pub(in crate::types::infer) mod nominal_members;
pub(in crate::types::infer) mod overload_collection;
mod override_entry;
mod override_variable_kind;
mod partial_specialization;
pub(in crate::types::infer) mod pep695_parameter_validation;
pub(in crate::types::infer) mod class_generic_validation;
mod predicate_constraints;
pub(in crate::types::infer) mod quoted_annotations;
mod recursive_normalization;
mod redundancy;
pub(super) mod shared_routes;
pub(in crate::types::infer) mod signature_annotations;
mod special_form_conversion;
mod specialization;
pub(super) mod specialization_recovery;
mod star_imports;
pub(in crate::types::infer) mod statement_traversal;
mod tuple_class;
mod typevar_set;
pub(super) mod union_recovery;

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::{
    assert_function_query_was_not_run_by_name, find_will_execute_event_by_name,
};
use ruff_python_ast::{self as ast, PythonVersion, Stmt};
use salsa::Database;
use salsa::attempt_probe::{Incomplete, report_incomplete};
use salsa::execution_probe::{
    ExecutionLimits, ExecutionWork, FinalSourceError, FinalSourceMemo, FixedQueryKeyProfile,
    RunError,
};
use salsa::prepared_source_probe::{Stamp, assert_no_active_attempt, capture};
use ty_module_resolver::{ImportingFile, KnownModule, resolve_module, resolve_module_confident};
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::{BindingsOwner, DefinitionKind, DefinitionState};
use ty_python_core::narrowing_constraints::{ConstraintKey, ScopedNarrowingConstraint};
use ty_python_core::place::{PlaceExpr, PlaceExprRef};
use ty_python_core::platform::PythonPlatform;
use ty_python_core::predicate::{Predicate, PredicateNode};
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::FileScopeId;
use ty_python_core::{
    ApplicableConstraints, EnclosingSnapshotResult, ExpressionNodeKey, UseDefMap, semantic_index,
};

use super::super::{
    DefinitionInferenceExtra, InferenceRegion, TypeInferenceBuilder, infer_deferred_types,
    infer_definition_types, infer_expression_types,
};
use crate::ProgramEnvironment;
use crate::analysis::{
    AnalysisFailure, AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, ClassCheckOperation,
    DeferredInferenceOperation, PreparedAnalysisFile, check_file_with_policy,
    expression_type_with_policy, prepare_file, with_analysis_session,
};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::PlaceAndQualifiers;
use crate::reachability::{ReachabilityCacheKey, ReachabilityEvaluationCache};
use crate::types::call::{Argument, Bindings, CallArguments};
use crate::types::class::known_class_to_instance_ingredient;
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::relation::source::resources::observations as invocation_observations;
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DescriptorOperation, MemberLookupKey, MemberLookupPolicy,
    MemberLookupResult, may_exist_at_runtime, member_lookup_ingredient,
    member_lookup_with_policy_impl,
};

use super::super::builder::source_definition::controlled::observations;
use super::*;

fn fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = 1\n").unwrap();
    db
}

fn boolean_fixture(value: bool) -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        if value {
            "left = right = True\n"
        } else {
            "left = right = False\n"
        },
    )
    .unwrap();
    db
}

fn controlled_expression_predicate<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    expression: Expression<'db>,
    positive: bool,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Truthiness>, AnalysisFailure> {
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let program = access.routes.program;
            let effects = SourceEffects::new(&access, program);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            crate::reachability::source::analyze_single_with(
                &env,
                &Predicate {
                    node: PredicateNode::Expression(expression),
                    is_positive: positive,
                },
                crate::reachability::source::ReachabilityFacts,
                &effects,
            )
            .await
        })
    })
}

fn controlled_cached_reachability<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    cache: &ReachabilityEvaluationCache<'db>,
    use_def: &UseDefMap<'db>,
    id: ScopedReachabilityConstraintId,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Truthiness>, AnalysisFailure> {
    controlled_reachability(prepared, Some(cache), use_def, id, policy)
}

fn controlled_reachability<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    cache: Option<&ReachabilityEvaluationCache<'db>>,
    use_def: &UseDefMap<'db>,
    id: ScopedReachabilityConstraintId,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Truthiness>, AnalysisFailure> {
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let effects = SourceEffects::new(&access, session.program());
            match cache {
                Some(cache) => {
                    crate::reachability::source::evaluate_cached_reachability_with(
                        cache,
                        use_def.reachability_constraints(),
                        use_def.predicates(),
                        id,
                        crate::reachability::source::ReachabilityFacts,
                        &effects,
                    )
                    .await
                }
                None => {
                    crate::place::source_effects::reachability_with(
                        &effects,
                        None,
                        use_def.reachability_constraints(),
                        use_def.predicates(),
                        id,
                    )
                    .await
                }
            }
        })
    })
}

fn uncached_reachability_fixture() -> TestDb {
    assignment_fixture("condition = True\nif condition:\n    pass\nelse:\n    raise RuntimeError\n")
}

fn cold_reachability_admission(policy: AnalysisPolicy) -> anyhow::Result<Option<usize>> {
    let db = uncached_reachability_fixture();
    let prepared = prepare(&db);
    let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let result = controlled_reachability(
        &prepared,
        None,
        use_def,
        use_def.end_of_scope_reachability(),
        &policy,
    )
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let (before, admitted, remaining) = observations::reachability_allocation();
    if policy == funded() {
        assert_eq!(result, AnalysisOutcome::Complete(Truthiness::AlwaysTrue));
        assert!(before > 0 && admitted > 0 && remaining.is_some());
        assert!(observations::counts().1 > 0);
    } else {
        let expected_reason = if policy.semantic_work_limit < funded().semantic_work_limit {
            AnalysisIncomplete::WorkLimit
        } else {
            AnalysisIncomplete::RequestedAllocationLimit
        };
        match result {
            AnalysisOutcome::Complete(truthiness) => {
                assert_eq!(truthiness, Truthiness::AlwaysTrue);
            }
            AnalysisOutcome::Incomplete { reason, .. } => assert_eq!(reason, expected_reason),
        }
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(remaining.map(|remaining| policy.semantic_work_limit - remaining))
}

#[test]
fn reachability_allocation_refusal_precedes_factory_and_allows_retry() -> anyhow::Result<()> {
    let Some(work) = cold_reachability_admission(funded())? else {
        anyhow::bail!("funded reachability did not admit its continuation");
    };
    // Cold probes measure this factory's admission, independently of later allocations.
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    for _ in 0..usize::BITS {
        if lower == upper {
            break;
        }
        let middle = lower + (upper - lower) / 2;
        if cold_reachability_admission(AnalysisPolicy {
            requested_bytes_limit: middle,
            ..funded()
        })?
        .is_some()
        {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    assert_eq!(lower, upper);
    assert!(work > 0 && upper > 0);
    for policy in [
        AnalysisPolicy {
            semantic_work_limit: work,
            ..funded()
        },
        AnalysisPolicy {
            requested_bytes_limit: upper,
            ..funded()
        },
    ] {
        assert!(cold_reachability_admission(policy)?.is_some());
    }
    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: work - 1,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let db = uncached_reachability_fixture();
        let prepared = prepare(&db);
        let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
        let id = use_def.end_of_scope_reachability();
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        observations::reset(None);
        assert_eq!(
            controlled_reachability(&prepared, None, use_def, id, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: (),
            }),
        );
        assert_eq!(observations::reachability_allocation(), (1, 0, None));
        assert_eq!(observations::counts(), (0, 0, 0));
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            controlled_reachability(&prepared, None, use_def, id, &funded()),
            Ok(AnalysisOutcome::Complete(Truthiness::AlwaysTrue)),
        );
        assert!(observations::reachability_allocation().1 > 0);
        assert!(observations::counts().1 > 0);
        let events = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
        }
        assert_eq!(
            crate::reachability::evaluate_reachability(&db, use_def, id),
            Truthiness::AlwaysTrue,
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

#[test]
fn reachability_child_cancellation_drains_owners_and_allows_retry() {
    let db = uncached_reachability_fixture();
    let prepared = prepare(&db);
    let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
    let id = use_def.end_of_scope_reachability();
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    observations::reset(Some(observations::Event::Created));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        controlled_reachability(&prepared, None, use_def, id, &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)));
    assert!(observations::reachability_allocation().1 > 0);
    assert!(observations::counts().1 > 0);
    assert_eq!(observations::counts().0, 0);
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events_db.take_salsa_events(),
        )
        .is_some()
    );
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    // Canonical queries can finish while cancellation is masked, so retry may reuse a child.
    observations::reset(None);
    assert_eq!(
        controlled_reachability(&prepared, None, use_def, id, &funded()),
        Ok(AnalysisOutcome::Complete(Truthiness::AlwaysTrue)),
    );
    assert!(observations::reachability_allocation().1 > 0);
    events_db.take_salsa_events();
    assert_eq!(
        crate::reachability::evaluate_reachability(&db, use_def, id),
        Truthiness::AlwaysTrue,
    );
    let events = events_db.take_salsa_events();
    for query in ["infer_expression_types_impl", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn cached_reachability_reuses_primary_and_secondary_results_and_canonical_children() {
    for secondary in [false, true] {
        let mut db = setup_db();
        db.write_files([
            (
                "src/main.py",
                "condition = True\nif condition:\n    pass\nelse:\n    raise RuntimeError\n",
            ),
            (
                "src/other.py",
                "condition = False\nif condition:\n    pass\nelse:\n    raise RuntimeError\n",
            ),
        ])
        .unwrap();
        let prepared = prepare(&db);
        let primary = prepared.semantic_index().use_def_map(FileScopeId::global());
        let primary_scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
        let cache =
            ReachabilityEvaluationCache::new(primary_scope, primary.reachability_constraints());
        let file = if secondary {
            let file = system_path_to_file(&db, "src/other.py").unwrap();
            ProgramFile::new(&db, file, prepared.program_file().program(&db))
        } else {
            prepared.program_file()
        };
        let index = semantic_index(&db, file);
        let use_def = index.use_def_map(FileScopeId::global());
        let id = use_def.end_of_scope_reachability();
        let key = cache.key(
            FileScopeId::global().to_scope_id(&db, file),
            use_def.reachability_constraints(),
            id,
        );
        assert_eq!(cache.lookup(key), None);
        let expected = Truthiness::from(!secondary);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        observations::reset(None);
        assert_eq!(
            controlled_cached_reachability(&prepared, &cache, use_def, id, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(cache.lookup(key), Some(expected));
        let (insertions, stored) = observations::cache_stored();
        assert_eq!(insertions, 1);
        let stored = stored.unwrap();
        assert_eq!(stored.key, key);
        match key {
            ReachabilityCacheKey::Primary(index) => {
                assert_eq!(stored.storage.0, index + 1);
                assert_eq!(stored.storage.2, 0);
            }
            ReachabilityCacheKey::Other { .. } => {
                assert_eq!(stored.storage.0, 0);
                assert_eq!(stored.storage.2, 1);
            }
        }
        let initial = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &initial)
                .is_some()
        );
        assert!(
            find_will_execute_event_by_name(&db, "infer_definition_types", None, &initial)
                .is_some()
        );

        observations::reset(None);
        assert_eq!(
            controlled_cached_reachability(&prepared, &cache, use_def, id, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(observations::cache_stored().0, 0);
        assert!(observations::cache_ready().is_none());
        assert_eq!(
            crate::reachability::evaluate_reachability(&db, use_def, id),
            expected
        );
        let reused = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &reused);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn cached_reachability_terminals_do_not_populate_storage() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let cache = ReachabilityEvaluationCache::new(scope, use_def.reachability_constraints());
    for (id, expected) in [
        (
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            Truthiness::AlwaysTrue,
        ),
        (
            ScopedReachabilityConstraintId::ALWAYS_FALSE,
            Truthiness::AlwaysFalse,
        ),
        (
            ScopedReachabilityConstraintId::AMBIGUOUS,
            Truthiness::Ambiguous,
        ),
    ] {
        observations::reset(None);
        assert_eq!(
            controlled_cached_reachability(&prepared, &cache, use_def, id, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(cache.retained_storage(), (0, 0, 0, 0));
        assert!(observations::cache_ready().is_none());
        assert_eq!(observations::cache_stored().0, 0);
        assert_eq!(observations::counts(), (0, 0, 0));
        assert_no_active_attempt();
    }
}

#[test]
fn cached_reachability_failed_predicates_leave_no_entry() {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "if [True]:\n    pass\nelse:\n    raise RuntimeError\n",
    )
    .unwrap();
    let prepared = prepare(&db);
    let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let id = use_def.end_of_scope_reachability();
    let cache = ReachabilityEvaluationCache::new(scope, use_def.reachability_constraints());
    let key = cache.key(scope, use_def.reachability_constraints(), id);
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        observations::reset(None);
        assert_eq!(
            controlled_cached_reachability(&prepared, &cache, use_def, id, &funded()),
            Ok(unavailable(OperationId::ExpressionKind)),
        );
        assert_eq!(cache.lookup(key), None);
        assert_eq!(cache.retained_storage(), (0, 0, 0, 0));
        assert!(observations::cache_ready().is_none());
        assert_eq!(observations::cache_stored().0, 0);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn cached_reachability_insertion_refusal_keeps_the_completed_predicate_without_an_entry() {
    let source = "condition = True\nif condition:\n    pass\nelse:\n    raise RuntimeError\n";
    let measured = assignment_fixture(source);
    let prepared = prepare(&measured);
    let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
    let scope = FileScopeId::global().to_scope_id(&measured, prepared.program_file());
    let cache = ReachabilityEvaluationCache::new(scope, use_def.reachability_constraints());
    observations::reset(None);
    assert_eq!(
        controlled_cached_reachability(
            &prepared,
            &cache,
            use_def,
            use_def.end_of_scope_reachability(),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Truthiness::AlwaysTrue)),
    );
    let ready_work =
        funded().semantic_work_limit - observations::cache_ready().unwrap().remaining.unwrap();

    let db = assignment_fixture(source);
    let prepared = prepare(&db);
    let use_def = prepared.semantic_index().use_def_map(FileScopeId::global());
    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let id = use_def.end_of_scope_reachability();
    let cache = ReachabilityEvaluationCache::new(scope, use_def.reachability_constraints());
    let key = cache.key(scope, use_def.reachability_constraints(), id);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    observations::reset(None);
    assert_eq!(
        controlled_cached_reachability(
            &prepared,
            &cache,
            use_def,
            id,
            &AnalysisPolicy {
                semantic_work_limit: ready_work,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert!(observations::cache_ready().is_some());
    assert_eq!(observations::cache_stored().0, 0);
    assert_eq!(cache.lookup(key), None);
    assert_eq!(cache.retained_storage(), (0, 0, 0, 0));
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();

    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        controlled_cached_reachability(&prepared, &cache, use_def, id, &funded()),
        Ok(AnalysisOutcome::Complete(Truthiness::AlwaysTrue)),
    );
    assert_eq!(cache.lookup(key), Some(Truthiness::AlwaysTrue));
    assert_eq!(observations::cache_stored().0, 1);
    let events = events_db.take_salsa_events();
    for query in ["infer_expression_types_impl", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn retained_reachability_cache_interruption_releases_owners_and_reuses_completed_predicates() {
    let source = "condition = True\nif condition:\n    alias = True\nleft = right = alias\n";
    let measured = assignment_fixture(source);
    let prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true))),
    );
    let stored_work =
        funded().semantic_work_limit - observations::cache_stored().1.unwrap().remaining.unwrap();

    for cancel in [false, true] {
        let db = assignment_fixture(source);
        let prepared = prepare(&db);
        let Stmt::If(conditional) = &prepared.parsed_module().syntax().body[1] else {
            panic!("fixture condition");
        };
        let predicate = prepared
            .semantic_index()
            .expression(conditional.test.as_ref());
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        observations::reset(cancel.then_some(observations::Event::ReachabilityCacheStored));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("{other:?}"),
        }
        let (insertions, retained) = observations::cache_stored();
        assert!(insertions > 0);
        let retained = retained.unwrap();
        assert!(retained.storage.0 > 0 || retained.storage.2 > 0);
        assert!(retained.storage.1 > 0 || retained.storage.3 > 0);
        assert_eq!(observations::counts().0, 0);
        assert!(observations::counts().1 > 0);
        assert_no_active_attempt();

        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::bool_literal(true))),
        );
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            Some(predicate.as_id()),
            &events,
        );
        if !cancel {
            assert!(observations::cache_stored().0 > 0);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn expression_predicate_retries_and_reuses_the_canonical_type_for_both_polarities() {
    for value in [false, true] {
        let db = boolean_fixture(value);
        let prepared = prepare(&db);
        let expression = expression(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        observations::reset(None);
        assert_eq!(
            controlled_expression_predicate(
                &prepared,
                expression,
                true,
                &AnalysisPolicy {
                    semantic_work_limit: 0,
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
        );
        for positive in [true, false] {
            events_db.take_salsa_events();
            assert_eq!(
                controlled_expression_predicate(&prepared, expression, positive, &funded()),
                Ok(AnalysisOutcome::Complete(
                    Truthiness::from(value).negate_if(!positive)
                )),
            );
            let events = events_db.take_salsa_events();
            if positive {
                assert!(
                    find_will_execute_event_by_name(
                        &db,
                        "infer_expression_types_impl",
                        None,
                        &events
                    )
                    .is_some()
                );
            } else {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_expression_types_impl",
                    None,
                    &events,
                );
            }
            assert_eq!(observations::counts().0, 0);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn expression_predicate_propagates_unavailable_inference_without_a_truthiness_result() {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = [1]\n")
        .unwrap();
    let prepared = prepare(&db);
    observations::reset(None);
    assert_eq!(
        controlled_expression_predicate(&prepared, expression(&db), false, &funded()),
        Ok(unavailable(OperationId::ExpressionKind)),
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn attribute_receiver_cancellation_cleans_up_and_reuses_completed_queries() {
    for cancel in [false, true] {
        let mut db = setup_db();
        db.write_file("src/main.py", "left = right = True.real\n")
            .unwrap();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        observations::reset(cancel.then_some(observations::Event::Stored));
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => {
                assert_eq!(outcome, Ok(AnalysisOutcome::Complete(Type::int_literal(1))),)
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        for retry in 0..2 {
            events_db.take_salsa_events();
            observations::reset(None);
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::int_literal(1))),
            );
            let events = events_db.take_salsa_events();
            if !cancel || retry == 1 {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_expression_types_impl",
                    None,
                    &events,
                );
            }
            assert_eq!(observations::counts().0, 0);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn cold_boolean_publishes_the_full_original_expression_result() {
    for value in [true, false] {
        let mut db = boolean_fixture(value);
        let prepared = prepare(&db);
        observations::reset(None);
        {
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::bool_literal(value)))
            );
            assert_eq!(observations::counts(), (0, 1, 1));
        }
        drop(prepared);
        db.take_salsa_events();
        let prepared = prepare(&db);
        {
            let expr = expression(&db);
            let ordinary = super::super::infer_expression_types(&db, expr, TypeContext::default());
            assert_eq!(ordinary.expressions.iter().len(), 1);
            assert_eq!(
                ordinary.expression_type(expr.node_ref(&db)),
                Type::bool_literal(value)
            );
            assert!(ordinary.extra.is_none());
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::bool_literal(value)))
            );
            assert!(std::ptr::eq(
                ordinary,
                super::super::infer_expression_types(&db, expr, TypeContext::default())
            ));
            assert_eq!(observations::counts(), (0, 1, 1));
        }
        drop(prepared);
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events,
        );
    }
}

#[test]
fn cold_integer_literals_publish_canonical_expression_results() {
    for value in [0, 3, i64::MAX] {
        let mut db = setup_db();
        db.write_file("src/main.py", format!("left = right = {value}\n"))
            .unwrap();
        let prepared = prepare(&db);
        observations::reset(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::int_literal(value))),
        );
        let expr = expression(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let canonical = infer_expression_types(&db, expr, TypeContext::default());
        assert_eq!(
            canonical.expression_type(expr.node_ref(&db)),
            Type::int_literal(value)
        );
        assert_eq!(canonical.expressions.iter().len(), 1);
        assert!(canonical.extra.is_none());
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events_db.take_salsa_events(),
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

#[test]
fn cold_tuple_literals_preserve_element_types_and_canonical_results() {
    for elements in [vec![], vec![3], vec![3, 11], vec![0, i64::MAX], vec![3; 64]] {
        let mut db = setup_db();
        let values = elements
            .iter()
            .map(|value| format!("{value},"))
            .collect::<String>();
        db.write_file("src/main.py", format!("left = right = ({values})\n"))
            .unwrap();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let expected = Type::tuple(crate::types::tuple::TupleType::new(
            &db,
            &env,
            &crate::types::tuple::TupleSpec::heterogeneous(
                elements.iter().copied().map(Type::int_literal),
            ),
        ));
        assert_eq!(
            result,
            Ok(AnalysisOutcome::Complete(expected)),
            "tuple length {}",
            elements.len()
        );
        assert_eq!(observations::tuple_shape_progress().0, 1);
        let expr = expression(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let canonical = infer_expression_types(&db, expr, TypeContext::default());
        assert_eq!(canonical.expression_type(expr.node_ref(&db)), expected);
        assert_eq!(canonical.expressions.iter().len(), elements.len() + 1);
        assert!(canonical.extra.is_none());
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert!(std::ptr::eq(
            canonical,
            infer_expression_types(&db, expr, TypeContext::default())
        ));
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events_db.take_salsa_events(),
        );
        assert_eq!(observations::tuple_shape_progress().0, 1);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn cold_rich_comparisons_publish_canonical_expression_results() {
    for (source, expected) in [
        ("14 >= 11", true),
        ("10 >= 11", false),
        ("3 < 11", true),
        ("3 > 11", false),
        ("3 <= 3", true),
        ("(3, 14) >= (3, 11)", true),
        ("(3, 10) >= (3, 11)", false),
        ("(3, 11) > (3, 11)", false),
        ("(3,) < (3, 11)", true),
        ("(3, 11) >= (3,)", true),
        ("() <= ()", true),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", format!("left = right = {source}\n"))
            .unwrap();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let expected = Type::bool_literal(expected);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
            "{source}"
        );
        let expr = expression(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let canonical = infer_expression_types(&db, expr, TypeContext::default());
        assert_eq!(canonical.expression_type(expr.node_ref(&db)), expected);
        assert!(canonical.extra.is_none());
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert!(std::ptr::eq(
            canonical,
            infer_expression_types(&db, expr, TypeContext::default())
        ));
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events_db.take_salsa_events(),
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn retained_comparison_interruption_cleans_up_and_retries_in_the_same_revision() {
    let make_db = || {
        let mut db = setup_db();
        db.write_file("src/main.py", "left = right = (3, 14) >= (3, 11)\n")
            .unwrap();
        db
    };
    let measured = make_db();
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let measured_result = expression_type_with_policy(
        &measured_prepared,
        expression_key(&measured_prepared),
        &funded(),
    );
    assert_eq!(
        measured_result,
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
    );
    let (comparisons, remaining) = observations::comparison_progress();
    assert!(comparisons > 0);
    let retained_work = funded().semantic_work_limit - remaining.unwrap();

    for cancel in [false, true] {
        let db = make_db();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::ComparisonRetained));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                }),
            ),
            other => panic!("{other:?}"),
        }
        assert!(observations::comparison_progress().0 > 0);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        observations::reset(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::bool_literal(true))),
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn tuple_precision_promotion_refuses_without_publishing_a_partial_type() {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        format!("left = right = ({})\n", "3,".repeat(65)),
    )
    .unwrap();
    let prepared = prepare(&db);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(unavailable(OperationId::ExpressionKind)),
    );
    assert_eq!(observations::tuple_shape_progress().0, 0);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    infer_expression_types(&db, expression(&db), TypeContext::default());
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events_db.take_salsa_events(),
        )
        .is_some()
    );
}

#[test]
fn tuple_shape_interruption_cleans_up_and_retries_in_the_same_revision() {
    let make_db = || {
        let mut db = setup_db();
        db.write_file("src/main.py", "left = right = (3, 11)\n")
            .unwrap();
        db
    };
    let measured = make_db();
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let measured_result = expression_type_with_policy(
        &measured_prepared,
        expression_key(&measured_prepared),
        &funded(),
    );
    assert!(matches!(measured_result, Ok(AnalysisOutcome::Complete(_))));
    let (shapes, remaining) = observations::tuple_shape_progress();
    assert_eq!(shapes, 1);
    let retained_work = funded().semantic_work_limit - remaining.unwrap();

    for cancel in [false, true] {
        let db = make_db();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::TupleShapeReady));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                }),
            ),
            other => panic!("{other:?}"),
        }
        assert_eq!(observations::tuple_shape_progress().0, 1);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        observations::reset(None);
        let retry = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let expected = Type::tuple(crate::types::tuple::TupleType::new(
            &db,
            &env,
            &crate::types::tuple::TupleSpec::heterogeneous([
                Type::int_literal(3),
                Type::int_literal(11),
            ]),
        ));
        assert_eq!(retry, Ok(AnalysisOutcome::Complete(expected)));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn refusal_after_expression_storage_discards_owner_then_retries_same_revision() {
    let measured = boolean_fixture(true);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert!(matches!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let stored_work = funded().semantic_work_limit - observations::stored_remaining().unwrap();

    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let limited = AnalysisPolicy {
        semantic_work_limit: stored_work,
        ..funded()
    };
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &limited),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_eq!(observations::counts(), (0, 1, 1));
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
    );
    assert_eq!(observations::counts(), (0, 2, 2));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn cancellation_after_builder_or_store_drains_owner_and_allows_retry() {
    for event in [observations::Event::Created, observations::Event::Stored] {
        let db = boolean_fixture(false);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let expr = expression(&db);
        observations::reset(Some(event));
        let outcome = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
        }));
        assert!(matches!(outcome, Err(salsa::Cancelled::Local)));
        // Canonical evaluation masks local cancellation until it leaves the query context.
        // The completed child may publish before the root rethrows the native cancellation.
        assert_eq!(observations::counts(), (0, 1, 1));
        let retry_expr = expression(&db);
        assert_eq!(retry_expr.as_id(), expr.as_id());
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::bool_literal(false)))
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(observations::counts().1, 1);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn plain_import_fixture(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db.write_file("src/dependency.py", "").unwrap();
    db.write_file("src/package/__init__.py", "").unwrap();
    db.write_file("src/package/child.py", "").unwrap();
    db
}

fn assigned_place_node<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> &'ast ast::Expr {
    let mut body = prepared.parsed_module().syntax().body.as_slice();
    loop {
        match body.last() {
            Some(Stmt::FunctionDef(function)) => body = function.body.as_slice(),
            Some(Stmt::ClassDef(class)) => body = class.body.as_slice(),
            Some(Stmt::If(statement)) => body = statement.body.as_slice(),
            Some(Stmt::Assign(assignment)) => return &assignment.value,
            _ => panic!("assigned-place fixture assignment"),
        }
    }
}

fn controlled_assigned_place<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<
    AnalysisOutcome<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>)>,
    AnalysisFailure,
> {
    let node = assigned_place_node(prepared);
    let expression = prepared.semantic_index().expression(node);
    with_analysis_session(prepared, policy, |session| {
        let place = PlaceExpr::try_from_expr(node).unwrap();
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let file = prepared.program_file();
            let source = access.prepare_existing(file).await?;
            let effects = SourceEffects::new(&access, access.routes.program);
            let env = ProgramEnvironment::from_file(file);
            effects
                .assigned_place_for_test(&source, &env, expression, node.into(), place)
                .await
        })
    })
}

fn controlled_applicable_constraints<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    constraints: &[(FileScopeId, ConstraintKey)],
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    let node = assigned_place_node(prepared);
    let expression = prepared.semantic_index().expression(node);
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let file = prepared.program_file();
            let source = access.prepare_existing(file).await?;
            let effects = SourceEffects::new(&access, access.routes.program);
            let env = ProgramEnvironment::from_file(file);
            effects
                .applicable_constraints_for_test(
                    &source,
                    &env,
                    expression,
                    node.into(),
                    ty,
                    constraints,
                )
                .await
        })
    })
}

fn ordinary_applicable_constraints<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    constraints: &[(FileScopeId, ConstraintKey)],
) -> Type<'db> {
    let file = prepared.program_file();
    let node = assigned_place_node(prepared);
    let expression = prepared.semantic_index().expression(node);
    let env = ProgramEnvironment::from_file(file);
    TypeInferenceBuilder::new(
        db,
        &env,
        InferenceRegion::Expression(expression, TypeContext::default()),
        file.file(db),
        file,
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .ordinary_applicable_constraints_for_test(node.into(), ty, constraints)
}

fn eager_member_constraint<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> ScopedNarrowingConstraint {
    let node = assigned_place_node(prepared);
    let index = prepared.semantic_index();
    let expression = index.expression(node);
    let place = PlaceExpr::try_from_expr(node).unwrap();
    let EnclosingSnapshotResult::FoundConstraint(constraint) = index.enclosing_snapshot(
        FileScopeId::global(),
        PlaceExprRef::from(&place),
        expression.scope(db).file_scope_id(db),
    ) else {
        panic!("the eager member read must retain an unbound enclosing constraint");
    };
    let ApplicableConstraints::UnboundBinding(evaluator) = index
        .use_def_map(FileScopeId::global())
        .applicable_constraints(
            ConstraintKey::NarrowingConstraint(constraint),
            FileScopeId::global(),
            PlaceExprRef::from(&place),
            index,
        )
    else {
        panic!("the retained constraint must select the unbound-binding reducer");
    };
    assert_eq!(evaluator.constraint(), constraint);
    constraint
}

#[test]
fn empty_applicable_constraints_preserve_the_type_after_interruption_and_retry() {
    let source = "left = right = target.member\n";
    let mut measured = setup_db();
    measured.write_file("src/main.py", source).unwrap();
    let measured_prepared = prepare(&measured);
    let ty = Type::bool_literal(true);
    observations::reset(None);
    assert_eq!(
        controlled_applicable_constraints(&measured_prepared, ty, &[], &funded()),
        Ok(AnalysisOutcome::Complete(ty)),
    );
    assert_eq!(
        ordinary_applicable_constraints(&measured, &measured_prepared, ty, &[]),
        ty,
    );
    let (steps, remaining) = observations::place_progress();
    assert!(steps > 1);
    let first_step_work = funded().semantic_work_limit - remaining.unwrap();

    for cancel in [false, true] {
        let mut db = setup_db();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::PlaceStep));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: first_step_work,
                ..funded()
            }
        };
        let outcome = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            controlled_applicable_constraints(&prepared, ty, &[], &policy)
        }));
        match outcome {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            ),
            other => panic!("{other:?}"),
        }
        assert!(observations::place_progress().0 > 0);
        assert!(observations::counts().1 > 0);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        observations::reset(None);
        assert_eq!(
            controlled_applicable_constraints(&prepared, ty, &[], &funded()),
            Ok(AnalysisOutcome::Complete(ty)),
        );
        assert_eq!(ordinary_applicable_constraints(&db, &prepared, ty, &[]), ty);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn eager_unbound_terminal_constraints_preserve_or_exclude_the_base_type() {
    let ty = Type::bool_literal(true);
    for (source, expected_constraint, expected) in [
        (
            "target.member\nclass Container:\n    left = right = target.member\n",
            ScopedNarrowingConstraint::ALWAYS_TRUE,
            ty,
        ),
        (
            "target.member\nraise RuntimeError\nclass Container:\n    left = right = target.member\n",
            ScopedNarrowingConstraint::ALWAYS_FALSE,
            Type::Never,
        ),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let constraint = eager_member_constraint(&db, &prepared);
        assert_eq!(constraint, expected_constraint);
        let constraints = [(
            FileScopeId::global(),
            ConstraintKey::NarrowingConstraint(constraint),
        )];
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            controlled_applicable_constraints(&prepared, ty, &constraints, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(
            ordinary_applicable_constraints(&db, &prepared, ty, &constraints),
            expected,
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

#[test]
fn constrained_unbound_bindings_keep_only_reachable_fallback_types() {
    let ty = Type::sys_version_info();
    for (source, reachability, expected) in [
        (
            "left = right = target.member\n",
            ScopedReachabilityConstraintId::ALWAYS_TRUE,
            ty,
        ),
        (
            "raise RuntimeError\nleft = right = target.member\n",
            ScopedReachabilityConstraintId::ALWAYS_FALSE,
            Type::Never,
        ),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let node = assigned_place_node(&prepared);
        let place = PlaceExpr::try_from_expr(node).unwrap();
        let key = ConstraintKey::UseId(
            ast::ExprRef::from(node).scoped_use_id(&db, prepared.program_file()),
        );
        let index = prepared.semantic_index();
        let ApplicableConstraints::ConstrainedBindings(mut bindings) = index
            .use_def_map(FileScopeId::global())
            .applicable_constraints(
                key,
                FileScopeId::global(),
                PlaceExprRef::from(&place),
                index,
            )
        else {
            panic!("a member use must retain its visible binding iterator");
        };
        let binding = bindings.next().unwrap();
        assert!(matches!(binding.binding, DefinitionState::Undefined));
        assert_eq!(binding.reachability_constraint, reachability);
        assert_eq!(
            binding.narrowing_constraint.constraint(),
            ScopedNarrowingConstraint::ALWAYS_TRUE,
        );
        assert!(bindings.next().is_none());
        let constraints = [(FileScopeId::global(), key)];
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            controlled_applicable_constraints(&prepared, ty, &constraints, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(
            ordinary_applicable_constraints(&db, &prepared, ty, &constraints),
            expected,
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

#[test]
fn eager_unbound_nonterminal_constraints_skip_only_unrelated_places() {
    for (source, targets_member) in [
        (
            "target.member\nif condition:\n    class Container:\n        left = right = target.member\n",
            false,
        ),
        (
            "target.member\nif target.member:\n    class Container:\n        left = right = target.member\n",
            true,
        ),
    ] {
        let mut db = setup_db();
        db.write_file("src/main.py", source).unwrap();
        let prepared = prepare(&db);
        let constraint = eager_member_constraint(&db, &prepared);
        assert!(!constraint.is_terminal());
        let node = assigned_place_node(&prepared);
        let place = PlaceExpr::try_from_expr(node).unwrap();
        let index = prepared.semantic_index();
        let place_id = index
            .place_table(FileScopeId::global())
            .place_id(PlaceExprRef::from(&place))
            .unwrap();
        let evaluator = index
            .use_def_map(FileScopeId::global())
            .narrowing_evaluator(constraint);
        assert_eq!(
            evaluator
                .predicate_narrowing_targets()
                .contains_place(place_id),
            targets_member,
        );
        let constraints = [(
            FileScopeId::global(),
            ConstraintKey::NarrowingConstraint(constraint),
        )];
        let ty = Type::bool_literal(true);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let outcome = controlled_applicable_constraints(&prepared, ty, &constraints, &funded());
        assert_eq!(
            outcome,
            Ok(if targets_member {
                unavailable(OperationId::Narrowing)
            } else {
                AnalysisOutcome::Complete(ty)
            }),
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        if !targets_member {
            assert_eq!(
                ordinary_applicable_constraints(&db, &prepared, ty, &constraints),
                ty,
            );
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

#[test]
fn cold_assigned_places_use_canonical_bindings_and_preserve_ordinary_constraints() {
    for (source, undefined, minimum_steps, infers_import) in [
        (
            "import dependency\nleft = right = dependency\n",
            false,
            1,
            true,
        ),
        (
            "def outer():\n    import dependency\n    def inner():\n        left = right = dependency\n",
            false,
            2,
            true,
        ),
        (
            "import dependency\nleft = right = dependency.value.inner\n",
            true,
            2,
            true,
        ),
        (
            "try:\n    import dependency\nexcept:\n    pass\nleft = right = dependency.value.inner\n",
            true,
            2,
            true,
        ),
        (
            "def outer():\n    import dependency\n    def inner():\n        left = right = dependency.value.inner\n",
            true,
            3,
            false,
        ),
    ] {
        let db = plain_import_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let result = controlled_assigned_place(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete((place, constraints))) = result else {
            panic!("{source}\n{result:?}");
        };
        assert_eq!(place.place.is_undefined(), undefined, "{source}");
        assert!(!constraints.is_empty(), "{source}");
        // Cold dependency preparation can repeat the resolver before its binding is available.
        assert!(
            observations::place_resolution_progress().0 >= minimum_steps,
            "{source}"
        );
        assert_eq!(observations::counts().0, 0);
        let events = events_db.take_salsa_events();
        assert_eq!(
            find_will_execute_event_by_name(&db, "infer_definition_types", None, &events).is_some(),
            infers_import,
            "{source}",
        );
        assert_no_active_attempt();

        let file = prepared.program_file();
        let node = assigned_place_node(&prepared);
        let expression = prepared.semantic_index().expression(node);
        let env = ProgramEnvironment::from_file(file);
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Expression(expression, TypeContext::default()),
            file.file(&db),
            file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .ordinary_assigned_place_for_test(node.into(), PlaceExpr::try_from_expr(node).unwrap());
        assert_eq!((place, constraints), ordinary, "{source}");
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cold_name_load_applies_incoming_constraints_and_reuses_canonical_results() {
    for (source, implicit_global) in [
        ("left = right = __builtins__\n", true),
        (
            "def outer():\n    import dependency\n    def inner():\n        left = right = dependency\n",
            false,
        ),
    ] {
        let db = plain_import_fixture(source);
        let prepared = prepare(&db);
        let node = assigned_place_node(&prepared);
        let expression = prepared.semantic_index().expression(node);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);

        let result = expression_type_with_policy(&prepared, node.into(), &funded());
        let Ok(AnalysisOutcome::Complete(ty)) = result else {
            panic!("{source}\n{result:?}");
        };
        if implicit_global {
            assert_eq!(ty, Type::any());
        } else {
            let Type::ModuleLiteral(module) = ty else {
                panic!("{ty:?}");
            };
            assert_eq!(module.module(&db).name(&db).as_str(), "dependency");
            assert_eq!(module.importing_file(&db), None);
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events)
                .is_some()
        );
        let created = observations::counts().1;

        let ordinary = infer_expression_types(&db, expression, TypeContext::default());
        assert_eq!(ordinary.expression_type(node), ty);
        assert_eq!(
            expression_type_with_policy(&prepared, node.into(), &funded()),
            Ok(AnalysisOutcome::Complete(ty)),
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_expression_types_impl", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(observations::counts().1, created);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn assigned_place_prefix_interruption_discards_spilled_storage_before_retry() {
    let source = "import dependency\nleft = right = dependency.a.b.c.d\n";
    let measured = plain_import_fixture(source);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let measured_result = controlled_assigned_place(&measured_prepared, &funded());
    assert!(matches!(measured_result, Ok(AnalysisOutcome::Complete(_))));
    let node = assigned_place_node(&measured_prepared);
    let expression = measured_prepared.semantic_index().expression(node);
    let prefixes = crate::place_load::resolve_place_load(
        &measured,
        measured_prepared.semantic_index(),
        expression.scope(&measured),
        PlaceExpr::try_from_expr(node).unwrap(),
        crate::place_load::PlaceLoadMode::AtExpression(node.into()),
    )
    .find_map(|step| match step {
        crate::place_load::PlaceLoadResolutionStep::MemberResolutionCondition(prefixes) => {
            Some(prefixes.iter().count())
        }
        _ => None,
    });
    assert!(prefixes.is_some_and(|count| count > 2));
    let (prefixes_ready, remaining) = observations::place_prefix_progress();
    assert!(prefixes_ready > 0);
    let retained_work = funded().semantic_work_limit - remaining.unwrap();

    for cancel in [false, true] {
        let db = plain_import_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::PlacePrefixesReady));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            controlled_assigned_place(&prepared, &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            ),
            other => panic!("{other:?}"),
        }
        assert!(observations::place_prefix_progress().0 > 0);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        observations::reset(None);
        let retry = controlled_assigned_place(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete((place, constraints))) = retry else {
            panic!("{retry:?}");
        };
        assert!(place.place.is_undefined());
        assert!(!constraints.is_empty());
        assert!(observations::place_prefix_progress().0 > 0);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn assigned_place_prefix_propagates_an_unavailable_binding_producer() {
    let db = plain_import_fixture("value = [True]\nleft = right = value.attribute\n");
    let prepared = prepare(&db);
    observations::reset(None);
    assert_eq!(
        controlled_assigned_place(&prepared, &funded()),
        Ok(unavailable(OperationId::ExpressionKind)),
    );
    assert!(observations::place_prefix_progress().0 > 0);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn attribute_place_interruption_discards_retained_steps_before_same_revision_retry() {
    let source = "import dependency\nleft = right = dependency.value\n";
    let measured = plain_import_fixture(source);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded(),
        ),
        Ok(unavailable(OperationId::MemberLookup(GeneralMemberOperation::NominalEnumMember))),
    );
    let (steps, remaining) = observations::place_progress();
    assert!(steps > 1);
    let first_step_work = funded().semantic_work_limit - remaining.unwrap();

    for cancel in [false, true] {
        let db = plain_import_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::PlaceStep));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: first_step_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            ),
            other => panic!("{other:?}"),
        }
        assert!(observations::place_progress().0 > 0);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(unavailable(OperationId::MemberLookup(GeneralMemberOperation::NominalEnumMember))),
        );
        assert!(observations::place_progress().0 > 1);
        let events = events_db.take_salsa_events();
        let expression = expression(&db);
        assert!(events.iter().any(|event| {
                matches!(event.kind, salsa::EventKind::WillExecute { database_key }
                    if db.ingredient_debug_name(database_key.ingredient_index()) == "infer_expression_types_impl"
                        && database_key.key_index() == expression.as_id())
            }));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn cold_plain_imports_preserve_module_identity_and_canonical_definition_payloads() {
    for (source, name, package) in [
        (
            "import dependency\nleft = right = dependency\n",
            "dependency",
            false,
        ),
        (
            "import package.child as imported\nleft = right = imported\n",
            "package.child",
            false,
        ),
        (
            "import package.child\nleft = right = package\n",
            "package",
            true,
        ),
        (
            "import package as imported\nleft = right = imported\n",
            "package",
            true,
        ),
    ] {
        let db = plain_import_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let Ok(AnalysisOutcome::Complete(Type::ModuleLiteral(module))) = result else {
            panic!("{result:?}");
        };
        assert_eq!(module.module(&db).name(&db).as_str(), name);
        assert_eq!(
            module.importing_file(&db),
            package.then_some(prepared.program_file())
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        events_db.take_salsa_events();
        let Stmt::Import(import) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture import");
        };
        let definition = prepared
            .semantic_index()
            .expect_single_definition(&import.names[0]);
        let canonical = infer_definition_types(&db, definition);
        assert_eq!(canonical.bindings(definition).len(), 1);
        assert_eq!(canonical.declarations(definition).len(), 0);
        assert!(canonical.expressions.iter().next().is_none());
        assert!(canonical.extra.is_none());
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            prepared.program_file().file(&db),
            prepared.program_file(),
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(canonical, &ordinary);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            result
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_definition_types", "infer_expression_types_impl"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn plain_import_binding_interruption_drains_owners_and_retries_in_the_same_revision() {
    let source = "import package.child\nleft = right = package\n";
    let measured = plain_import_fixture(source);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let completed = expression_type_with_policy(
        &measured_prepared,
        expression_key(&measured_prepared),
        &funded(),
    );
    assert!(
        matches!(completed, Ok(AnalysisOutcome::Complete(_))),
        "{completed:?}"
    );
    let stored_work =
        funded().semantic_work_limit - observations::binding_stored_remaining().unwrap();

    for cancel in [false, true] {
        let db = plain_import_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(cancel.then_some(observations::Event::BindingStored));
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert_eq!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            ),
            other => panic!("{other:?}"),
        }
        assert!(observations::binding_stored_remaining().is_some());
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        observations::reset(None);
        let retry = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let Ok(AnalysisOutcome::Complete(Type::ModuleLiteral(module))) = retry else {
            panic!("{retry:?}");
        };
        assert_eq!(module.module(&db).name(&db).as_str(), "package");
        assert_eq!(module.importing_file(&db), Some(prepared.program_file()));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

fn assignment_fixture(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db
}

fn assignment_before_lookup<'ast>(
    prepared: &'ast PreparedAnalysisFile<'_>,
) -> &'ast ast::StmtAssign {
    let body = &prepared.parsed_module().syntax().body;
    let Stmt::Assign(assignment) = &body[body.len() - 2] else {
        panic!("fixture assignment before lookup");
    };
    assignment
}

fn assignment_definition<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    assignment: &ast::StmtAssign,
) -> Definition<'db> {
    let ast::Expr::Name(target) = &assignment.targets[0] else {
        panic!("fixture assignment target");
    };
    prepared.semantic_index().expect_single_definition(target)
}

#[test]
fn cold_assignments_preserve_definition_and_expression_payloads_through_name_lookup() {
    for source in [
        "class Product:\n    pass\nAlias = Product\nleft = right = Alias\n",
        "alias = True\nleft = right = alias\n",
        "alias = 42\nleft = right = alias\n",
        "alias = other = True\nleft = right = alias\n",
        "TYPE_CHECKING = False\nleft = right = TYPE_CHECKING\n",
    ] {
        let db = assignment_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let assignment = assignment_before_lookup(&prepared);
        let definition = assignment_definition(&prepared, assignment);
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let Ok(AnalysisOutcome::Complete(ty)) = result else {
            panic!("{source}: {result:?}");
        };
        if source.starts_with("class") {
            assert!(matches!(ty, Type::ClassLiteral(ClassLiteral::Static(_))));
        } else {
            assert_eq!(
                ty,
                if source.contains("42") {
                    Type::int_literal(42)
                } else {
                    Type::bool_literal(true)
                }
            );
        }
        let canonical = infer_definition_types(&db, definition);
        assert_eq!(canonical.binding_type(definition), ty);
        assert_eq!(canonical.expression_type(&assignment.targets[0]), ty);
        assert_eq!(
            canonical.expression_type(assignment.value.as_ref()),
            if source.starts_with("TYPE_CHECKING") {
                Type::bool_literal(false)
            } else {
                ty
            }
        );
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            prepared.program_file().file(&db),
            prepared.program_file(),
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(canonical, &ordinary);
        for expression in std::iter::once(expression(&db)).chain(
            prepared
                .semantic_index()
                .try_expression(assignment.value.as_ref()),
        ) {
            let canonical = infer_expression_types(&db, expression, TypeContext::default());
            let ordinary = TypeInferenceBuilder::new(
                &db,
                &env,
                InferenceRegion::Expression(expression, TypeContext::default()),
                prepared.program_file().file(&db),
                prepared.program_file(),
                prepared.semantic_index(),
                prepared.parsed_module(),
            )
            .finish_expression();
            assert_eq!(canonical, &ordinary);
        }
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            result
        );
        let events = events_db.take_salsa_events();
        for query in ["infer_definition_types", "infer_expression_types_impl"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn assignment_driver_preserves_standalone_bindings_ownership() {
    for (source, owner, binding_count) in [
        (
            "alias = [(bound := True)]\nleft = right = alias\n",
            BindingsOwner::Definition,
            2,
        ),
        (
            "alias = other = (bound := True)\nleft = right = alias\n",
            BindingsOwner::Statement,
            1,
        ),
    ] {
        let db = assignment_fixture(source);
        let prepared = prepare(&db);
        let assignment = assignment_before_lookup(&prepared);
        let definition = assignment_definition(&prepared, assignment);
        let DefinitionKind::Assignment(kind) = definition.kind(&db) else {
            panic!("fixture assignment definition");
        };
        assert_eq!(kind.owner(), owner);
        let expression = prepared
            .semantic_index()
            .expression(assignment.value.as_ref());
        let rhs = infer_expression_types(&db, expression, TypeContext::default());
        let extra = rhs.extra.as_deref().expect("RHS walrus binding");
        assert_eq!(extra.bindings.len(), 1);
        let (bound, _) = extra.bindings[0];
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            prepared.program_file().file(&db),
            prepared.program_file(),
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(infer_definition_types(&db, definition), &ordinary);
        assert_eq!(ordinary.bindings(definition).len(), binding_count);
        assert_eq!(
            ordinary
                .bindings(definition)
                .any(|(binding, _)| binding == bound),
            owner == BindingsOwner::Definition
        );
        let value_ty = rhs.expression_type(assignment.value.as_ref());
        assert_eq!(
            ordinary.expression_type(assignment.value.as_ref()),
            value_ty
        );
        assert_eq!(ordinary.expression_type(&assignment.targets[0]), value_ty);
        assert_eq!(ordinary.binding_type(definition), value_ty);
    }
}

#[test]
fn assignment_target_retains_inferred_type_when_binding_validation_uses_declared_type() {
    let db = assignment_fixture("alias: bool\nalias = 42\nleft = right = alias\n");
    let prepared = prepare(&db);
    let assignment = assignment_before_lookup(&prepared);
    let definition = assignment_definition(&prepared, assignment);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Definition(definition),
        prepared.program_file().file(&db),
        prepared.program_file(),
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_definition(definition);
    assert_eq!(infer_definition_types(&db, definition), &ordinary);
    assert_eq!(
        ordinary.expression_type(assignment.value.as_ref()),
        Type::int_literal(42)
    );
    assert_eq!(
        ordinary.expression_type(&assignment.targets[0]),
        Type::int_literal(42)
    );
    assert_eq!(
        ordinary.binding_type(definition),
        KnownClass::Bool.to_instance(&db, &env)
    );
}

#[test]
fn cold_definition_owned_standalone_assignment_refuses_at_its_selected_rhs() {
    let db = assignment_fixture("alias = [True]\nleft = right = alias\n");
    let prepared = prepare(&db);
    let assignment = assignment_before_lookup(&prepared);
    let definition = assignment_definition(&prepared, assignment);
    let DefinitionKind::Assignment(kind) = definition.kind(&db) else {
        panic!("fixture assignment definition");
    };
    assert_eq!(kind.owner(), BindingsOwner::Definition);
    let rhs = prepared
        .semantic_index()
        .expression(assignment.value.as_ref());
    let mut events_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        observations::reset(None);
        events_db.take_salsa_events();
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(unavailable(OperationId::ExpressionKind))
        );
        let events = events_db.take_salsa_events();
        for (query, key) in [
            ("infer_definition_types", definition.as_id()),
            ("infer_expression_types_impl", rhs.as_id()),
        ] {
            assert!(find_will_execute_event_by_name(&db, query, Some(key), &events).is_some());
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn assignment_interruption_releases_owners_and_reuses_completed_rhs_on_retry() {
    let source = "alias = other = True\nleft = right = alias\n";
    let measured = assignment_fixture(source);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
    );
    let checkpoints = [
        (
            observations::Event::ExpressionMerged,
            observations::merged_remaining().unwrap(),
        ),
        (
            observations::Event::BindingStored,
            observations::binding_stored_remaining().unwrap(),
        ),
    ];
    for (event, remaining) in checkpoints {
        for cancel in [false, true] {
            let db = assignment_fixture(source);
            let prepared = prepare(&db);
            let assignment = assignment_before_lookup(&prepared);
            let definition = assignment_definition(&prepared, assignment);
            let rhs = prepared
                .semantic_index()
                .expression(assignment.value.as_ref());
            let revision = salsa::plumbing::current_revision(&db);
            let mut events_db = db.clone();
            observations::reset(cancel.then_some(event));
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: funded().semantic_work_limit - remaining,
                    ..funded()
                }
            };
            let outcome = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
            }));
            match outcome {
                Err(salsa::Cancelled::Local) if cancel => {}
                Ok(result) if !cancel => assert_eq!(
                    result,
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    })
                ),
                other => panic!("{event:?}: {other:?}"),
            }
            assert!(observations::merged_remaining().is_some());
            if event == observations::Event::BindingStored {
                assert!(observations::binding_stored_remaining().is_some());
            }
            assert_eq!(observations::counts().0, 0);
            assert_no_active_attempt();
            events_db.take_salsa_events();
            observations::reset(None);
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
            );
            let events = events_db.take_salsa_events();
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_expression_types_impl",
                Some(rhs.as_id()),
                &events,
            );
            if !cancel {
                for (query, key) in [
                    ("infer_definition_types", definition.as_id()),
                    ("infer_expression_types_impl", expression(&db).as_id()),
                ] {
                    assert!(
                        find_will_execute_event_by_name(&db, query, Some(key), &events).is_some()
                    );
                }
            }
            assert_eq!(observations::counts().0, 0);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

fn function_fixture(deferred: bool) -> TestDb {
    let mut db = setup_db();
    let source = if deferred {
        "def choose(value: bool = True) -> bool:\n    return value\nleft = right = choose\n"
    } else {
        "def choose(value):\n    return value\nleft = right = choose\n"
    };
    db.write_file("src/main.py", source).unwrap();
    db
}

#[test]
fn cold_function_publishes_original_definition_and_expression_payloads() {
    for deferred in [false, true] {
        let mut db = function_fixture(deferred);
        let prepared = prepare(&db);
        let expr = expression(&db);
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(Type::FunctionLiteral(_)))
            ),
            "{result:?}"
        );
        assert_eq!(observations::counts(), (0, 2, 2));
        let program_file = expr.program_file(&db);
        let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
        let Stmt::FunctionDef(function) = &module.syntax().body[0] else {
            panic!("fixture function")
        };
        let index = semantic_index(&db, program_file);
        let definition = index.expect_single_definition(function);
        let canonical_definition = infer_definition_types(&db, definition);
        assert_eq!(canonical_definition.bindings(definition).len(), 1);
        assert_eq!(canonical_definition.declarations(definition).len(), 1);
        assert_eq!(canonical_definition.expressions.iter().len(), 0);
        match canonical_definition.extra.as_deref() {
            Some(DefinitionInferenceExtra::Deferred(definitions)) if deferred => {
                assert_eq!(definitions.as_ref(), &[definition])
            }
            None if !deferred => {}
            extra => panic!("unexpected definition payload: {extra:?}"),
        }
        let env = ProgramEnvironment::from_file(program_file);
        let ordinary_definition = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            program_file.file(&db),
            program_file,
            index,
            &module,
        )
        .finish_definition(definition);
        assert_eq!(canonical_definition, &ordinary_definition);
        let canonical_expression = infer_expression_types(&db, expr, TypeContext::default());
        let ordinary_expression = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Expression(expr, TypeContext::default()),
            program_file.file(&db),
            program_file,
            index,
            &module,
        )
        .finish_expression();
        assert_eq!(canonical_expression, &ordinary_expression);
        let Ok(AnalysisOutcome::Complete(Type::FunctionLiteral(function))) = result else {
            unreachable!()
        };
        let function_id = function.as_id();
        drop(prepared);
        db.take_salsa_events();
        let prepared = prepare(&db);
        let repeated = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let Ok(AnalysisOutcome::Complete(Type::FunctionLiteral(function))) = repeated else {
            panic!("{repeated:?}")
        };
        assert_eq!(function.as_id(), function_id);
        drop(prepared);
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events,
        );
        assert_eq!(observations::counts(), (0, 2, 2));
    }
}

#[test]
fn refusal_after_function_binding_discards_definition_and_retries_same_revision() {
    let measured = function_fixture(true);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let result = expression_type_with_policy(
        &measured_prepared,
        expression_key(&measured_prepared),
        &funded(),
    );
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(_))),
        "{result:?}"
    );
    let stored_work =
        funded().semantic_work_limit - observations::definition_stored_remaining().unwrap();
    let db = function_fixture(true);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &prepared,
            expression_key(&prepared),
            &AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_eq!(observations::counts(), (0, 2, 1));
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(Type::FunctionLiteral(_)))
        ),
        "{result:?}"
    );
    assert_eq!(observations::counts(), (0, 4, 3));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn cancellation_after_function_binding_drains_owners_and_reuses_completed_definition() {
    let db = function_fixture(true);
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(Some(observations::Event::DefinitionStored));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(observations::counts().0, 0);
    events_db.take_salsa_events();
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(Type::FunctionLiteral(_)))
        ),
        "{result:?}"
    );
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn class_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "class Product:\n    pass\nleft = right = Product\n",
    )
    .unwrap();
    db
}

#[test]
fn class_file_refusal_drains_owners_and_retries_to_completion() {
    let fixture = || {
        let mut db = setup_db();
        db.write_file("src/main.py", "class Product:\n    pass\nProduct\n")
            .unwrap();
        db
    };
    let measured = fixture();
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let result = check_file_with_policy(&measured_prepared, &funded());
    assert!(
        matches!(&result, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
        "{result:?}"
    );
    let limited = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit
            - observations::file_scope_remaining().unwrap(),
        ..funded()
    };

    let db = fixture();
    let prepared = prepare(&db);
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let mut events_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    for attempt in 0..3 {
        events_db.take_salsa_events();
        let policy = if attempt == 0 { limited } else { funded() };
        let result = check_file_with_policy(&prepared, &policy);
        if attempt == 0 {
            assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            );
            assert_eq!(observations::file_scope_remaining(), Some(0));
        } else {
            assert!(
                matches!(result, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty())
            );
        }
        let events = events_db.take_salsa_events();
        if attempt == 0 {
            assert!(
                find_will_execute_event_by_name(&db, "infer_scope_types_impl", None, &events)
                    .is_some()
            );
        } else {
            assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
        }
        if attempt != 0 {
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events,
            );
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    events_db.take_salsa_events();
    let inference = infer_definition_types(&db, definition);
    let Some(ClassLiteral::Static(class)) = inference.original_class_type(definition) else {
        panic!("fixture class");
    };
    assert!(crate::types::enums::enum_metadata(&db, class.into()).is_none());
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_function_query_was_not_run_by_name(&db, "enum_metadata", None, &events);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn override_member_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "class Product:\n    def first(self):\n        pass\n    def second(self):\n        pass\nProduct\n",
    )
    .unwrap();
    db
}

#[test]
fn override_collection_retains_declared_and_bound_members_across_interruption() {
    let db = override_member_fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let result = controlled_module(&db, &prepared, &funded());
    let Ok(AnalysisOutcome::Complete(canonical)) = result else {
        panic!("{result:?}");
    };
    assert_eq!(
        observations::override_members().0,
        vec![
            ("first".into(), 1),
            ("second".into(), 2),
            ("first".into(), 2),
            ("second".into(), 2)
        ]
    );
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let retained_work = funded().semantic_work_limit - observations::override_members().1.unwrap();
    observations::reset(None);
    let warm = controlled_module(&db, &prepared, &funded());
    assert!(matches!(warm, Ok(AnalysisOutcome::Complete(value)) if std::ptr::eq(canonical, value)));
    assert!(observations::override_members().0.is_empty());
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    for cancel in [false, true] {
        let db = override_member_fixture();
        let prepared = prepare(&db);
        let module = FileScopeId::global().to_scope_id(&db, prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let Stmt::FunctionDef(first) = &class.body[0] else {
            panic!("fixture method");
        };
        let first_definition = prepared.semantic_index().expect_single_definition(first);
        observations::reset(cancel.then_some(observations::Event::OverrideMemberRetained));
        if cancel {
            let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                controlled_module(&db, &prepared, &funded())
            }));
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
            assert_eq!(
                observations::override_members().0,
                vec![
                    ("first".into(), 1),
                    ("second".into(), 2),
                    ("first".into(), 2),
                    ("second".into(), 2)
                ]
            );
        } else {
            let limited = AnalysisPolicy {
                semantic_work_limit: retained_work,
                ..funded()
            };
            assert_eq!(
                controlled_module(&db, &prepared, &limited),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            );
            assert_eq!(
                observations::override_members(),
                (
                    vec![
                        ("first".into(), 1),
                        ("second".into(), 2),
                        ("first".into(), 2),
                        ("second".into(), 2)
                    ],
                    Some(0)
                )
            );
        }
        assert!(!observations::override_members().0.is_empty());
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                scope_inference_ingredient(&db),
                module.as_id(),
            )
            .map(|_| ()),
            if cancel {
                Ok(())
            } else {
                Err(FinalSourceError::MissingMemo)
            }
        );

        events_db.take_salsa_events();
        let completed_parent = cancel.then(|| {
            super::super::infer_scope_types(&db, module, TypeContext::default())
        });
        observations::reset(None);
        let result = controlled_module(&db, &prepared, &funded());
        let Ok(AnalysisOutcome::Complete(retry)) = result else {
            panic!("{result:?}");
        };
        if let Some(completed_parent) = completed_parent {
            assert!(std::ptr::eq(completed_parent, retry));
            assert!(observations::override_members().0.is_empty());
        } else {
            assert_eq!(
                observations::override_members().0,
                vec![
                    ("first".into(), 1),
                    ("second".into(), 2),
                    ("first".into(), 2),
                    ("second".into(), 2)
                ]
            );
        }
        let events = events_db.take_salsa_events();
        assert_eq!(
            find_will_execute_event_by_name(
                &db,
                "infer_scope_types_impl",
                Some(module.as_id()),
                &events,
            )
            .is_some(),
            !cancel
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(first_definition.as_id()),
            &events,
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cold_class_publishes_original_definition_and_expression_payloads() {
    let mut db = class_fixture();
    let prepared = prepare(&db);
    let expr = expression(&db);
    observations::reset(None);
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    let Ok(AnalysisOutcome::Complete(Type::ClassLiteral(ClassLiteral::Static(class)))) = result
    else {
        panic!("{result:?}");
    };
    assert_eq!(observations::counts(), (0, 2, 2));
    let program_file = expr.program_file(&db);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let Stmt::ClassDef(class_node) = &module.syntax().body[0] else {
        panic!("fixture class");
    };
    let index = semantic_index(&db, program_file);
    let definition = index.expect_single_definition(class_node);
    let canonical_definition = infer_definition_types(&db, definition);
    assert_eq!(canonical_definition.bindings(definition).len(), 1);
    assert_eq!(canonical_definition.declarations(definition).len(), 1);
    assert_eq!(canonical_definition.expressions.iter().len(), 0);
    assert!(canonical_definition.extra.is_none());
    let env = ProgramEnvironment::from_file(program_file);
    let ordinary_definition = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Definition(definition),
        program_file.file(&db),
        program_file,
        index,
        &module,
    )
    .finish_definition(definition);
    assert_eq!(canonical_definition, &ordinary_definition);
    let canonical_expression = infer_expression_types(&db, expr, TypeContext::default());
    let ordinary_expression = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Expression(expr, TypeContext::default()),
        program_file.file(&db),
        program_file,
        index,
        &module,
    )
    .finish_expression();
    assert_eq!(canonical_expression, &ordinary_expression);
    let class_id = class.as_id();
    drop(prepared);
    db.take_salsa_events();
    let prepared = prepare(&db);
    let repeated = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    let Ok(AnalysisOutcome::Complete(Type::ClassLiteral(ClassLiteral::Static(class)))) = repeated
    else {
        panic!("{repeated:?}");
    };
    assert_eq!(class.as_id(), class_id);
    drop(prepared);
    let events = db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
    assert_eq!(observations::counts(), (0, 2, 2));
}

fn controlled_class_context<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    refuse_before_context: Option<Incomplete>,
) -> Result<AnalysisOutcome<(StaticClassLiteral<'db>, Option<GenericContext<'db>>)>, AnalysisFailure>
{
    controlled_class_context_for(prepared, None, refuse_before_context, &funded())
}

fn controlled_class_context_for<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    name: Option<&str>,
    refuse_before_context: Option<Incomplete>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<(StaticClassLiteral<'db>, Option<GenericContext<'db>>)>, AnalysisFailure>
{
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .rev()
        .find_map(|statement| {
            if let Stmt::ClassDef(class) = statement
                && name.is_none_or(|name| class.name.as_str() == name)
            {
                Some(class)
            } else {
                None
            }
        })
        .unwrap();
    let definition = prepared.semantic_index().expect_single_definition(class);
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let inference = access.definition(definition).await?;
            let class = access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(2)?;
                    access.endpoint.check_completion()?;
                    let Some(ClassLiteral::Static(class)) =
                        inference.original_class_type(definition)
                    else {
                        return Err(RunError::Contract(
                            "fixture definition is not a static class",
                        ));
                    };
                    if let Some(reason) = refuse_before_context {
                        report_incomplete(session.db(), reason);
                        access.endpoint.check_completion()?;
                    }
                    Ok(class)
                })
                .await;
            let context = access.class_generic_context(class).await?;
            Ok((class, context))
        })
    })
}

#[test]
fn cold_class_context_publishes_the_canonical_absent_context() {
    let db = class_fixture();
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let result = controlled_class_context(&prepared, None);
    let Ok(AnalysisOutcome::Complete((class, context))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(context, None);
    assert_eq!(class.name(&db), "Product");
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let events = events_db.take_salsa_events();
    for query in ["infer_definition_types", "static_class_generic_context"] {
        assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
    }
    assert_eq!(class.generic_context(&db), context);
    assert_eq!(controlled_class_context(&prepared, None), result);
    let events = events_db.take_salsa_events();
    for query in ["infer_definition_types", "static_class_generic_context"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
}

#[test]
fn class_context_refusal_releases_owners_and_reuses_completed_definition() {
    for reason in [Incomplete::Allowance, Incomplete::RequestedAllocation] {
        let db = class_fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            controlled_class_context(&prepared, Some(reason)),
            Ok(AnalysisOutcome::Incomplete {
                reason: if reason == Incomplete::Allowance {
                    AnalysisIncomplete::WorkLimit
                } else {
                    AnalysisIncomplete::RequestedAllocationLimit
                },
                completed: (),
            }),
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "infer_definition_types", None, &events).is_some()
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "static_class_generic_context",
            None,
            &events,
        );
        assert!(matches!(
            controlled_class_context(&prepared, None),
            Ok(AnalysisOutcome::Complete((_, None)))
        ));
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
        assert!(
            find_will_execute_event_by_name(&db, "static_class_generic_context", None, &events)
                .is_some()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

/// A cold class context publishes complete outer static-class and inner PEP 695 query results.
/// Repeated reads reuse both keys in the same revision and never certify the provisional `None` initializer.
#[test]
fn pep695_context_publishes_complete_context_for_retry() {
    let mut db = setup_db();
    db.write_file("src/main.pyi", "class Product[T]:\n    pass\n")
        .unwrap();
    let file = system_path_to_file(&db, "src/main.pyi").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    let mut completed = None;
    observations::reset(None);
    for attempt in 0..2 {
        events_db.take_salsa_events();
        let result = controlled_class_context(&prepared, None);
        let Ok(AnalysisOutcome::Complete((class, Some(context)))) = result else {
            panic!("{result:?}");
        };
        assert_eq!(context.variables(&db).len(), 1);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                static_class_generic_context_ingredient(&db),
                class.as_id(),
            )
            .is_ok()
        );
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                pep695_generic_context_ingredient(&db),
                class.as_id(),
            )
            .is_ok()
        );
        let events = events_db.take_salsa_events();
        if let Some(previous) = completed {
            assert_eq!((class, context), previous);
            assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
            assert_function_query_was_not_run_by_name(
                &db,
                "static_class_generic_context",
                Some(class.as_id()),
                &events,
            );
            assert_function_query_was_not_run_by_name(
                &db,
                "pep695_generic_context_inner",
                Some(class.as_id()),
                &events,
            );
        } else {
            assert_eq!(attempt, 0);
            completed = Some((class, context));
            assert!(
                find_will_execute_event_by_name(
                    &db,
                    "static_class_generic_context",
                    Some(class.as_id()),
                    &events
                )
                .is_some()
            );
            assert_eq!(class.pep695_generic_context(&db), Some(context));
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

fn forward_class_base_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        "class Derived(Base):\n    pass\nclass Base:\n    pass\n",
    )
    .unwrap();
    db
}

fn controlled_class_bases<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
    refuse_before_context: bool,
) -> Result<
    AnalysisOutcome<(
        StaticClassLiteral<'db>,
        &'db [Type<'db>],
        &'db DefinitionInference<'db>,
        Option<GenericContext<'db>>,
        Option<GenericContext<'db>>,
    )>,
    AnalysisFailure,
> {
    with_analysis_session(prepared, &funded(), |session| {
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let inference = access.definition(definition).await?;
            let class = access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(2)?;
                    access.endpoint.check_completion()?;
                    let Some(ClassLiteral::Static(class)) =
                        inference.original_class_type(definition)
                    else {
                        return Err(RunError::Contract(
                            "fixture definition is not a static class",
                        ));
                    };
                    Ok(class)
                })
                .await;
            let bases = access.explicit_bases(class).await?;
            if refuse_before_context {
                access
                    .endpoint
                    .local_call(|| {
                        report_incomplete(session.db(), Incomplete::Allowance);
                        access.endpoint.check_completion()
                    })
                    .await;
            }
            let deferred = access.deferred_definition(definition).await?;
            let inherited = access.inherited_class_context(class).await?;
            let context = access.class_generic_context(class).await?;
            Ok((class, bases, deferred, context, inherited))
        })
    })
}

#[test]
fn cold_forward_class_bases_publish_deferred_and_inherited_canonical_results() {
    let db = forward_class_base_fixture();
    let file = system_path_to_file(&db, "src/main.pyi").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let [Stmt::ClassDef(derived_node), Stmt::ClassDef(base_node)] =
        prepared.parsed_module().syntax().body.as_slice()
    else {
        panic!("fixture classes");
    };
    let definition = prepared
        .semantic_index()
        .expect_single_definition(derived_node);
    let base_definition = prepared
        .semantic_index()
        .expect_single_definition(base_node);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let traced = capture(&db, || controlled_class_bases(&prepared, definition, false)).unwrap();
    let Ok(AnalysisOutcome::Complete((derived, bases, deferred, context, inherited))) =
        traced.value
    else {
        panic!("{:?}", traced.value);
    };
    let [Type::ClassLiteral(ClassLiteral::Static(base))] = bases else {
        panic!("{bases:?}");
    };
    assert_eq!(derived.name(&db), "Derived");
    assert_eq!(base.name(&db), "Base");
    assert_eq!(context, None);
    assert_eq!(context, inherited);
    assert_eq!(deferred.expression_type(&derived_node.bases()[0]), bases[0]);
    assert!(traced.reads.iter().any(|read| {
        db.ingredient_debug_name(read.key.ingredient_index()) == "infer_deferred_types"
            && read.parent.is_some_and(|parent| {
                db.ingredient_debug_name(parent.ingredient_index()) == "explicit_bases_inner"
            })
    }));
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "infer_deferred_types",
        "explicit_bases_inner",
        "inherited_legacy_generic_context_inner",
        "static_class_generic_context",
    ] {
        assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        infer_definition_types(&db, base_definition).original_class_type(base_definition),
        Some(ClassLiteral::Static(*base)),
    );
    let db_view: &dyn Db = &db;
    assert_eq!(
        deferred_definition_inference_ingredient(&db).fetch(
            &db,
            db_view.zalsa(),
            db_view.zalsa_local(),
            definition.as_id(),
        ),
        deferred,
    );
    assert_eq!(
        explicit_bases_ingredient(&db)
            .fetch(&db, db_view.zalsa(), db_view.zalsa_local(), derived.as_id(),)
            .as_ref(),
        bases,
    );
    assert_eq!(
        *inherited_class_context_ingredient(&db).fetch(
            &db,
            db_view.zalsa(),
            db_view.zalsa_local(),
            derived.as_id(),
        ),
        inherited,
    );
    assert_eq!(
        *static_class_generic_context_ingredient(&db).fetch(
            &db,
            db_view.zalsa(),
            db_view.zalsa_local(),
            derived.as_id(),
        ),
        context,
    );
    assert_eq!(
        controlled_class_bases(&prepared, definition, false),
        traced.value
    );
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "infer_deferred_types",
        "explicit_bases_inner",
        "inherited_legacy_generic_context_inner",
        "static_class_generic_context",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn inherited_context_prefetch_refusal_reuses_completed_deferred_and_base_queries() {
    let db = forward_class_base_fixture();
    let file = system_path_to_file(&db, "src/main.pyi").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        controlled_class_bases(&prepared, definition, true),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let events = events_db.take_salsa_events();
    for query in ["infer_deferred_types", "explicit_bases_inner"] {
        assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
    }
    assert_function_query_was_not_run_by_name(
        &db,
        "inherited_legacy_generic_context_inner",
        None,
        &events,
    );
    let result = controlled_class_bases(&prepared, definition, false);
    let Ok(AnalysisOutcome::Complete((_, bases, deferred, None, None))) = result else {
        panic!("{result:?}");
    };
    let [Type::ClassLiteral(ClassLiteral::Static(base))] = bases else {
        panic!("{bases:?}");
    };
    assert_eq!(base.name(&db), "Base");
    assert_eq!(deferred.expression_type(&class.bases()[0]), bases[0]);
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "infer_deferred_types",
        "explicit_bases_inner",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert!(
        find_will_execute_event_by_name(
            &db,
            "inherited_legacy_generic_context_inner",
            None,
            &events,
        )
        .is_some()
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn controlled_static_mro<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
    refuse_before_mro: Option<Incomplete>,
) -> Result<AnalysisOutcome<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>>, AnalysisFailure> {
    with_analysis_session(prepared, &funded(), |session| {
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let inference = access.definition(definition).await?;
            let class = access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(2)?;
                    access.endpoint.check_completion()?;
                    let Some(ClassLiteral::Static(class)) =
                        inference.original_class_type(definition)
                    else {
                        return Err(RunError::Contract(
                            "fixture definition is not a static class",
                        ));
                    };
                    if let Some(reason) = refuse_before_mro {
                        report_incomplete(session.db(), reason);
                        access.endpoint.check_completion()?;
                    }
                    Ok(class)
                })
                .await;
            access.static_mro(class).await
        })
    })
}

#[test]
fn cold_static_mro_publishes_and_reuses_the_canonical_result() {
    let db = class_fixture();
    let prepared = prepare(&db);
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    observations::reset(None);
    events_db.take_salsa_events();
    let result = controlled_static_mro(&prepared, definition, None);
    let Ok(AnalysisOutcome::Complete(canonical)) = result else {
        panic!("{result:?}");
    };
    let Ok(mro) = canonical else {
        panic!("{canonical:?}");
    };
    let [
        ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Static(product))),
        ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Static(object))),
    ] = &**mro
    else {
        panic!("{mro:?}");
    };
    assert_eq!(product.name(&db), "Product");
    assert_eq!(object.known(&db), Some(KnownClass::Object));
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "try_mro_unspecialized",
        "known_class_to_class_literal",
    ] {
        assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let inference = infer_definition_types(&db, definition);
    assert_eq!(
        inference.original_class_type(definition),
        Some(ClassLiteral::Static(*product)),
    );
    let db_view: &dyn Db = &db;
    assert_eq!(
        try_mro_unspecialized_ingredient(&db).fetch(
            &db,
            db_view.zalsa(),
            db_view.zalsa_local(),
            product.as_id(),
        ),
        canonical,
    );
    assert_eq!(controlled_static_mro(&prepared, definition, None), result);
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "try_mro_unspecialized",
        "known_class_to_class_literal",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn static_mro_prefetch_refusal_preserves_cause_and_completes_same_revision_retry() {
    for reason in [Incomplete::Allowance, Incomplete::RequestedAllocation] {
        let db = class_fixture();
        let prepared = prepare(&db);
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let definition = prepared.semantic_index().expect_single_definition(class);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            controlled_static_mro(&prepared, definition, Some(reason)),
            Ok(AnalysisOutcome::Incomplete {
                reason: if reason == Incomplete::Allowance {
                    AnalysisIncomplete::WorkLimit
                } else {
                    AnalysisIncomplete::RequestedAllocationLimit
                },
                completed: (),
            })
        );
        assert_no_active_attempt();
        assert_eq!(observations::counts().0, 0);
        assert_function_query_was_not_run_by_name(
            &db,
            "try_mro_unspecialized",
            None,
            &events_db.take_salsa_events(),
        );
        let result = controlled_static_mro(&prepared, definition, None);
        let Ok(AnalysisOutcome::Complete(Ok(mro))) = result else {
            panic!("{result:?}");
        };
        let [
            ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Static(product))),
            ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Static(object))),
        ] = &**mro
        else {
            panic!("{mro:?}");
        };
        assert_eq!(product.name(&db), "Product");
        assert_eq!(object.known(&db), Some(KnownClass::Object));
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &events,
        );
        assert!(
            find_will_execute_event_by_name(&db, "try_mro_unspecialized", None, &events).is_some()
        );
        assert_no_active_attempt();
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn refusal_after_class_binding_discards_definition_and_retries_same_revision() {
    let measured = class_fixture();
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let result = expression_type_with_policy(
        &measured_prepared,
        expression_key(&measured_prepared),
        &funded(),
    );
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(Type::ClassLiteral(_)))),
        "{result:?}"
    );
    let stored_work =
        funded().semantic_work_limit - observations::definition_stored_remaining().unwrap();
    let db = class_fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &prepared,
            expression_key(&prepared),
            &AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_eq!(observations::counts(), (0, 2, 1));
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(Type::ClassLiteral(_)))),
        "{result:?}"
    );
    assert_eq!(observations::counts(), (0, 4, 3));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn cancellation_after_class_binding_drains_owners_and_reuses_completed_definition() {
    let db = class_fixture();
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(Some(observations::Event::DefinitionStored));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(observations::counts().0, 0);
    events_db.take_salsa_events();
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(Type::ClassLiteral(_)))),
        "{result:?}"
    );
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn expression(db: &TestDb) -> Expression<'_> {
    let file = system_path_to_file(db, "src/main.py").unwrap();
    let program_file = db.program_file(file);
    let module = parsed_module(db, program_file.python_file(db)).load(db);
    let Some(Stmt::Assign(assignment)) = module.syntax().body.last() else {
        panic!("fixture assignment")
    };
    semantic_index(db, program_file).expression(assignment.value.as_ref())
}

fn prepare(db: &TestDb) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, "src/main.py").unwrap();
    prepare_file(db, file).unwrap()
}

fn expression_key(prepared: &PreparedAnalysisFile<'_>) -> ExpressionNodeKey {
    match prepared.parsed_module().syntax().body.last() {
        Some(Stmt::Assign(assignment)) => assignment.value.as_ref().into(),
        Some(Stmt::Expr(statement)) => statement.value.as_ref().into(),
        _ => panic!("fixture expression"),
    }
}

fn expression_type_with_counted_root<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    root_entries: &Cell<usize>,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    let key = expression_key(prepared);
    let expression = prepared
        .semantic_index()
        .try_expression(key)
        .ok_or(AnalysisFailure::InvalidExpressionKey)?;
    with_analysis_session(prepared, &funded(), |session| {
        root_entries.set(root_entries.get() + 1);
        crate::types::run_source_expression(session, prepared, expression, key)
    })
}

fn check_file_with_counted_root(
    prepared: &PreparedAnalysisFile<'_>,
    root_entries: &Cell<usize>,
) -> Result<AnalysisOutcome<Result<Box<[Diagnostic]>, Diagnostic>>, AnalysisFailure> {
    with_analysis_session(prepared, &funded(), |session| {
        root_entries.set(root_entries.get() + 1);
        crate::types::run_source_file(session, prepared)
    })
}

#[test]
fn invalid_expression_key_refuses_before_semantic_inference() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let Some(Stmt::Assign(assignment)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture assignment")
    };
    let target_key = (&assignment.targets[0]).into();
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(&prepared, target_key, &funded()),
        Err(AnalysisFailure::InvalidExpressionKey),
    );
    assert_eq!(observations::counts(), (0, 0, 0));
}

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

fn unavailable<T>(operation: OperationId) -> AnalysisOutcome<T> {
    AnalysisOutcome::Incomplete {
        reason: AnalysisIncomplete::UnavailableOperation(operation),
        completed: (),
    }
}

#[test]
fn source_preparation_accepts_cold_files_only_in_the_root_program() {
    let mut db = boolean_fixture(true);
    db.write_file("src/other.py", "left = right = False\n")
        .unwrap();
    let prepared = prepare(&db);
    let original = prepared.program_file();
    let program = original.program(&db);
    let other_file = system_path_to_file(&db, "src/other.py").unwrap();
    let other_file = ProgramFile::new(&db, other_file, program);
    let other_platform = if *program.python_platform(&db) == PythonPlatform::All {
        PythonPlatform::Identifier("linux".into())
    } else {
        PythonPlatform::All
    };
    let other_program = Program::new(&db, &other_platform, program.resolver_environment(&db));
    let other_program_file = ProgramFile::new(&db, original.file(&db), other_program);
    assert_ne!(original, other_file);
    assert_ne!(original, other_program_file);
    let mut events_db = db.clone();
    events_db.take_salsa_events();

    for target in [original, other_file, other_program_file] {
        let attempts = Cell::new(0);
        let result = with_analysis_session(&prepared, &funded(), |session| {
            attempts.set(attempts.get() + 1);
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
            let (run, routes) = register(session, &prepared, registry, &values, resources)?;
            let values = &values;
            let prepared = &prepared;
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes,
                    values,
                };
                let source = access.prepare_existing(target).await?;
                assert_eq!(source.file, target);
                if target == original {
                    assert!(std::ptr::eq(source.index, prepared.semantic_index()));
                    assert!(std::ptr::eq(
                        source.module.syntax(),
                        prepared.parsed_module().syntax(),
                    ));
                }
                let python_file = access
                    .endpoint
                    .read_field(
                        target
                            .read_fields(access.endpoint.field_request_context())
                            .python_file(),
                        &BorrowOrCopy,
                    )
                    .await;
                let file = access
                    .endpoint
                    .read_field(
                        python_file
                            .read_fields(access.endpoint.field_request_context())
                            .file(),
                        &BorrowOrCopy,
                    )
                    .await;
                let repeated = access.prepare_file(file, program).await?;
                assert_eq!(repeated.file, target);
                assert!(std::ptr::eq(source.index, repeated.index));
                assert!(std::ptr::eq(
                    source.module.syntax(),
                    repeated.module.syntax()
                ));
                Ok(())
            })
        });
        if target == other_program_file {
            assert_eq!(
                result,
                Err(AnalysisFailure::Execution(RunError::Contract(
                    "source request belongs to another program"
                )))
            );
        } else {
            assert_eq!(result, Ok(AnalysisOutcome::Complete(())));
        }
        assert_eq!(attempts.get(), 1);
        assert_no_active_attempt();
        let events = events_db.take_salsa_events();
        for query in ["parsed_module", "semantic_index"] {
            if target == other_file {
                assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
            } else {
                assert_function_query_was_not_run_by_name(&db, query, None, &events);
            }
        }
    }
}

#[derive(Default)]
struct DependencyPreparationEvents {
    ingredients: Mutex<Vec<salsa::IngredientIndex>>,
    executions: AtomicUsize,
}

impl DependencyPreparationEvents {
    fn arm(&self, db: &TestDb, events: &[salsa::Event]) {
        let mut ingredients = self.ingredients.lock().unwrap();
        for event in events {
            if let salsa::EventKind::WillExecute { database_key } = event.kind
                && matches!(
                    db.ingredient_debug_name(database_key.ingredient_index())
                        .as_ref(),
                    "parsed_module" | "semantic_index"
                )
                && !ingredients.contains(&database_key.ingredient_index())
            {
                ingredients.push(database_key.ingredient_index());
            }
        }
        assert_eq!(ingredients.len(), 2);
    }

    fn observe(&self, event: &salsa::EventKind) {
        if let salsa::EventKind::WillExecute { database_key } = event
            && self
                .ingredients
                .lock()
                .unwrap()
                .contains(&database_key.ingredient_index())
        {
            assert_no_active_attempt();
            self.executions.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn dependency_fixture() -> (TestDb, Arc<DependencyPreparationEvents>) {
    let events = Arc::new(DependencyPreparationEvents::default());
    let observed = events.clone();
    let db = TestDbBuilder::new()
        .with_salsa_event_callback(move |event| observed.observe(event))
        .with_file("src/main.py", "left = right = True\n")
        .with_file("src/dependency.py", "left = right = False\n")
        .build()
        .unwrap();
    (db, events)
}

#[derive(Clone, Copy)]
enum BeforeDependency {
    Continue,
    Charge(ExecutionLimits),
    Refuse(Incomplete),
    CancelOnDrain,
}

struct CancelDependencyOnDrain<'db> {
    db: &'db dyn Db,
    enabled: bool,
}

impl Drop for CancelDependencyOnDrain<'_> {
    fn drop(&mut self) {
        if self.enabled {
            self.db.cancellation_token().cancel();
        }
    }
}

fn infer_cold_dependency<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    dependency: File,
    policy: &AnalysisPolicy,
    before: BeforeDependency,
    attempts: &Cell<usize>,
) -> Result<AnalysisOutcome<(Expression<'db>, &'db ExpressionInference<'db>)>, AnalysisFailure> {
    let root_expression = prepared
        .semantic_index()
        .try_expression(expression_key(prepared))
        .unwrap();
    with_analysis_session(prepared, policy, |session| {
        attempts.set(attempts.get() + 1);
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            access
                .expression(root_expression, TypeContext::default())
                .await?;
            let _cancel_on_drain = CancelDependencyOnDrain {
                db: session.db(),
                enabled: matches!(before, BeforeDependency::CancelOnDrain),
            };
            let program = session.program();
            access
                .endpoint
                .local_call(|| match before {
                    BeforeDependency::Continue | BeforeDependency::CancelOnDrain => Ok(()),
                    BeforeDependency::Charge(quote) => {
                        access.endpoint.admit_work(quote.semantic_work)?;
                        access.endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: quote.requested_bytes,
                        })
                    }
                    BeforeDependency::Refuse(reason) => {
                        Err(RunError::Refused(report_incomplete(session.db(), reason)))
                    }
                })
                .await;
            let source = access.prepare_file(dependency, program).await?;
            access
                .endpoint
                .local_call(|| match before {
                    BeforeDependency::Charge(quote) => {
                        access.endpoint.admit_work(quote.semantic_work)?;
                        access.endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: quote.requested_bytes,
                        })
                    }
                    BeforeDependency::CancelOnDrain => {
                        Err(RunError::Refused(report_incomplete(
                            session.db(),
                            Incomplete::Interrupted,
                        )))
                    }
                    BeforeDependency::Continue | BeforeDependency::Refuse(_) => Ok(()),
                })
                .await;
            let expression = access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(1)?;
                    let Some(Stmt::Assign(assignment)) = source.module.syntax().body.last() else {
                        return Err(RunError::Contract("dependency fixture has no assignment"));
                    };
                    Ok(source.index.expression(assignment.value.as_ref()))
                })
                .await;
            let inference = access
                .expression(expression, TypeContext::default())
                .await?;
            Ok((expression, inference))
        })
    })
}

#[test]
fn cold_dependency_preparation_publishes_the_canonical_result() {
    let (db, preparation) = dependency_fixture();
    let prepared = prepare(&db);
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let mut events_db = db.clone();
    preparation.arm(&db, &events_db.take_salsa_events());
    observations::reset(None);
    let attempts = Cell::new(0);
    let result = infer_cold_dependency(
        &prepared,
        dependency,
        &funded(),
        BeforeDependency::Continue,
        &attempts,
    );
    let Ok(AnalysisOutcome::Complete((expression, canonical))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(attempts.get(), 1);
    assert_eq!(preparation.executions.load(Ordering::SeqCst), 2);
    assert_eq!(observations::counts(), (0, 2, 2));
    assert_no_active_attempt();
    let file = expression.program_file(&db);
    assert_eq!(file.file(&db), dependency);
    assert_eq!(file.program(&db), prepared.program_file().program(&db));
    assert_eq!(
        canonical.expression_type(expression.node_ref(&db)),
        Type::bool_literal(false)
    );
    let events = events_db.take_salsa_events();
    let root_expression = prepared
        .semantic_index()
        .try_expression(expression_key(&prepared))
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key }
                if db.ingredient_debug_name(database_key.ingredient_index()) == "infer_expression_types_impl"
                    && database_key.key_index() == root_expression.as_id()))
            .count(),
        1
    );
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let env = ProgramEnvironment::from_file(file);
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Expression(expression, TypeContext::default()),
        dependency,
        file,
        semantic_index(&db, file),
        &module,
    )
    .finish_expression();
    assert_eq!(canonical, &ordinary);
    assert!(std::ptr::eq(
        canonical,
        infer_expression_types(&db, expression, TypeContext::default())
    ));
    events_db.take_salsa_events();
    attempts.set(0);
    let retry = infer_cold_dependency(
        &prepared,
        dependency,
        &funded(),
        BeforeDependency::Continue,
        &attempts,
    );
    assert!(
        matches!(retry, Ok(AnalysisOutcome::Complete((_, reused))) if std::ptr::eq(canonical, reused))
    );
    assert_eq!(attempts.get(), 1);
    let events = events_db.take_salsa_events();
    for query in [
        "parsed_module",
        "semantic_index",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts(), (0, 2, 2));
    assert_no_active_attempt();
}

#[test]
fn source_preparation_shares_cumulative_work_and_requested_bytes() {
    for (quote, reason) in [
        (
            ExecutionLimits {
                semantic_work: 600_000,
                requested_bytes: 0,
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            ExecutionLimits {
                semantic_work: 0,
                requested_bytes: 10 * 1024 * 1024,
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let (db, preparation) = dependency_fixture();
        let prepared = prepare(&db);
        let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
        let mut events_db = db.clone();
        preparation.arm(&db, &events_db.take_salsa_events());
        observations::reset(None);
        let attempts = Cell::new(0);
        assert_eq!(
            infer_cold_dependency(
                &prepared,
                dependency,
                &funded(),
                BeforeDependency::Charge(quote),
                &attempts,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: (),
            })
        );
        assert_eq!(attempts.get(), 1);
        assert_eq!(preparation.executions.load(Ordering::SeqCst), 2);
        assert_eq!(observations::counts(), (0, 1, 1));
        assert_no_active_attempt();
        attempts.set(0);
        let larger = AnalysisPolicy {
            semantic_work_limit: 3 * funded().semantic_work_limit,
            requested_bytes_limit: 3 * funded().requested_bytes_limit,
        };
        assert!(matches!(
            infer_cold_dependency(
                &prepared,
                dependency,
                &larger,
                BeforeDependency::Charge(quote),
                &attempts,
            ),
            Ok(AnalysisOutcome::Complete(_))
        ));
        assert_eq!(attempts.get(), 1);
        assert_eq!(observations::counts(), (0, 2, 2));
        assert_no_active_attempt();
    }
}

#[test]
fn existing_refusal_prevents_cold_dependency_preparation() {
    for cause in [Incomplete::Allowance, Incomplete::RequestedAllocation] {
        let (db, preparation) = dependency_fixture();
        let prepared = prepare(&db);
        let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
        let mut events_db = db.clone();
        preparation.arm(&db, &events_db.take_salsa_events());
        observations::reset(None);
        let attempts = Cell::new(0);
        assert_eq!(
            infer_cold_dependency(
                &prepared,
                dependency,
                &funded(),
                BeforeDependency::Refuse(cause),
                &attempts,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: if cause == Incomplete::Allowance {
                    AnalysisIncomplete::WorkLimit
                } else {
                    AnalysisIncomplete::RequestedAllocationLimit
                },
                completed: (),
            })
        );
        assert_eq!(attempts.get(), 1);
        assert_eq!(preparation.executions.load(Ordering::SeqCst), 0);
        assert_eq!(observations::counts(), (0, 1, 1));
        assert_no_active_attempt();
    }
}

#[test]
fn cancellation_during_drain_retains_prepared_sources_for_retry() {
    let (db, preparation) = dependency_fixture();
    let prepared = prepare(&db);
    let dependency = system_path_to_file(&db, "src/dependency.py").unwrap();
    let mut events_db = db.clone();
    preparation.arm(&db, &events_db.take_salsa_events());
    observations::reset(None);
    let attempts = Cell::new(0);
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        infer_cold_dependency(
            &prepared,
            dependency,
            &funded(),
            BeforeDependency::CancelOnDrain,
            &attempts,
        )
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(attempts.get(), 1);
    assert_eq!(preparation.executions.load(Ordering::SeqCst), 2);
    assert_eq!(observations::counts(), (0, 1, 1));
    assert_no_active_attempt();
    attempts.set(0);
    assert!(matches!(
        infer_cold_dependency(
            &prepared,
            dependency,
            &funded(),
            BeforeDependency::Continue,
            &attempts,
        ),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(attempts.get(), 1);
    assert_eq!(preparation.executions.load(Ordering::SeqCst), 2);
    assert_eq!(observations::counts(), (0, 2, 2));
    assert_no_active_attempt();
}

fn resolve_cold_modules<'db, const N: usize>(
    prepared: &PreparedAnalysisFile<'db>,
    requests: [(&ModuleName, Option<File>); N],
    policy: &AnalysisPolicy,
    quote: ExecutionLimits,
    attempts: &Cell<usize>,
) -> Result<AnalysisOutcome<[Option<Module<'db>>; N]>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        attempts.set(attempts.get() + 1);
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
        let descriptor_dispatches =
            register_descriptor_dispatches_values(session.db(), &mut registry)?;
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(quote.semantic_work)?;
                    access.endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: quote.requested_bytes,
                    })
                })
                .await;
            let program = session.program();
            let mut modules = [None; N];
            for ((name, importer), result) in requests.into_iter().zip(&mut modules) {
                *result = access.resolve_module(program, name, importer).await?;
            }
            access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(quote.semantic_work)?;
                    access.endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: quote.requested_bytes,
                    })
                })
                .await;
            Ok(modules)
        })
    })
}

const NO_EXTRA_MODULE_CHARGE: ExecutionLimits = ExecutionLimits {
    semantic_work: 0,
    requested_bytes: 0,
};

#[test]
fn cold_module_resolution_prepares_source_and_caches_absence() {
    let (db, preparation) = dependency_fixture();
    let prepared = prepare(&db);
    let present = ModuleName::new_static("dependency").unwrap();
    let missing =
        ModuleName::new("a_missing_module_name_long_enough_to_require_owned_heap_storage").unwrap();
    let mut events_db = db.clone();
    preparation.arm(&db, &events_db.take_salsa_events());
    observations::reset(None);
    let attempts = Cell::new(0);
    let requests = [(&present, None), (&missing, None), (&missing, None)];
    let result = resolve_cold_modules(
        &prepared,
        requests,
        &funded(),
        NO_EXTRA_MODULE_CHARGE,
        &attempts,
    );
    let Ok(AnalysisOutcome::Complete([Some(module), None, None])) = result else {
        panic!("{result:?}");
    };
    assert_eq!(attempts.get(), 1);
    assert_eq!(preparation.executions.load(Ordering::SeqCst), 2);
    assert_eq!(
        module.file(&db),
        Some(system_path_to_file(&db, "src/dependency.py").unwrap())
    );
    assert_eq!(observations::counts(), (0, 0, 0));
    assert_no_active_attempt();
    let events = events_db.take_salsa_events();
    assert!(find_will_execute_event_by_name(&db, "resolve_module_query", None, &events).is_some());
    for query in [
        "infer_scope_types_impl",
        "infer_expression_types_impl",
        "infer_definition_types",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    attempts.set(0);
    assert_eq!(
        resolve_cold_modules(
            &prepared,
            requests,
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
            &attempts
        ),
        Ok(AnalysisOutcome::Complete([Some(module), None, None]))
    );
    assert_eq!(attempts.get(), 1);
    let events = events_db.take_salsa_events();
    for query in [
        "resolve_module_query",
        "parsed_module",
        "semantic_index",
        "global_scope",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_no_active_attempt();
}

#[test]
fn module_resolution_keeps_confident_and_importer_relative_requests_distinct() {
    let mut db = boolean_fixture(true);
    for directory in ["left", "right"] {
        db.write_file(format!("/src/{directory}/main.py"), "")
            .unwrap();
        db.write_file(
            format!("/src/{directory}/sibling.py"),
            "left = right = False\n",
        )
        .unwrap();
    }
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let name = ModuleName::new_static("sibling").unwrap();
    let left = system_path_to_file(&db, "/src/left/main.py").unwrap();
    let right = system_path_to_file(&db, "/src/right/main.py").unwrap();
    let attempts = Cell::new(0);
    let result = resolve_cold_modules(
        &prepared,
        [
            (&name, None),
            (&name, Some(left)),
            (&name, Some(right)),
            (&name, None),
        ],
        &funded(),
        NO_EXTRA_MODULE_CHARGE,
        &attempts,
    );
    let Ok(AnalysisOutcome::Complete([None, Some(left_module), Some(right_module), None])) = result
    else {
        panic!("{result:?}");
    };
    assert_eq!(attempts.get(), 1);
    assert_eq!(
        left_module.file(&db),
        Some(system_path_to_file(&db, "/src/left/sibling.py").unwrap())
    );
    assert_eq!(
        right_module.file(&db),
        Some(system_path_to_file(&db, "/src/right/sibling.py").unwrap())
    );
    assert_eq!(
        resolve_module_confident(&db, program.resolver_environment(&db), &name),
        None
    );
    for (importer, module) in [(left, left_module), (right, right_module)] {
        assert_eq!(
            resolve_module(
                &db,
                ImportingFile::File(importer, program.resolver_environment(&db)),
                &name
            ),
            Some(module)
        );
    }
    assert_no_active_attempt();
}

#[test]
fn module_preparation_shares_cumulative_limits() {
    for (quote, reason) in [
        (
            ExecutionLimits {
                semantic_work: 600_000,
                requested_bytes: 0,
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            ExecutionLimits {
                semantic_work: 0,
                requested_bytes: 10 * 1024 * 1024,
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let name = ModuleName::new_static("missing_dependency").unwrap();
        let attempts = Cell::new(0);
        assert_eq!(
            resolve_cold_modules(&prepared, [(&name, None)], &funded(), quote, &attempts),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert_eq!(attempts.get(), 1);
        assert_no_active_attempt();
        attempts.set(0);
        let larger = AnalysisPolicy {
            semantic_work_limit: 3 * funded().semantic_work_limit,
            requested_bytes_limit: 3 * funded().requested_bytes_limit,
        };
        assert_eq!(
            resolve_cold_modules(&prepared, [(&name, None)], &larger, quote, &attempts),
            Ok(AnalysisOutcome::Complete([None]))
        );
        assert_eq!(attempts.get(), 1);
        assert_no_active_attempt();
    }
}

#[test]
fn import_resolution_records_canonical_dependencies_and_observes_new_files() {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "from dependency import left\nresult = alias = left\n",
    )
    .unwrap();
    for present in [false, true] {
        if present {
            db.write_file("src/dependency.py", "left = right = False\n")
                .unwrap();
        }
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let root_entries = Cell::new(0);
        observations::reset(None);
        let traced = capture(&db, || {
            expression_type_with_counted_root(&prepared, &root_entries)
        })
        .unwrap();
        assert_eq!(root_entries.get(), 1);
        traced.check_root_reads().unwrap();
        assert_eq!(
            traced.value,
            Ok(AnalysisOutcome::Complete(if present {
                Type::bool_literal(false)
            } else {
                Type::unknown()
            }))
        );
        assert!(traced.reads.iter().any(|read| {
            db.ingredient_debug_name(read.key.ingredient_index()) == "resolve_module_query"
                && read.parent.is_some_and(|parent| {
                    db.ingredient_debug_name(parent.ingredient_index()) == "infer_definition_types"
                })
        }));
        if !present {
            assert!(traced.reads.iter().any(|read| {
                db.ingredient_debug_name(read.key.ingredient_index())
                    == "desperately_resolve_module"
                    && read.parent.is_some_and(|parent| {
                        db.ingredient_debug_name(parent.ingredient_index())
                            == "infer_definition_types"
                    })
            }));
        }
        root_entries.set(0);
        let warm = capture(&db, || {
            expression_type_with_counted_root(&prepared, &root_entries)
        })
        .unwrap();
        assert_eq!(root_entries.get(), 1);
        assert_eq!(warm.value, traced.value);
        warm.check_root_reads().unwrap();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
    }
}

#[test]
fn import_edits_validate_in_one_root_and_preserve_warm_results() {
    for (dependency, value) in [
        ("left = right = False\n# unchanged value\n", false),
        ("left = right = True\n", true),
    ] {
        let source = "from dependency import left\nresult = alias = left\n";
        let mut db = setup_db();
        db.write_file("src/main.py", source).unwrap();
        db.write_file("src/dependency.py", "left = right = False\n")
            .unwrap();
        let previous_revision = salsa::plumbing::current_revision(&db);
        {
            let prepared = prepare(&db);
            let root_entries = Cell::new(0);
            let initial = capture(&db, || {
                expression_type_with_counted_root(&prepared, &root_entries)
            })
            .unwrap();
            assert_eq!(root_entries.get(), 1);
            assert_eq!(
                initial.value,
                Ok(AnalysisOutcome::Complete(Type::bool_literal(false)))
            );
            initial.check_root_reads().unwrap();
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), previous_revision);
        }

        db.write_file("src/dependency.py", dependency).unwrap();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        assert_ne!(revision, previous_revision);
        let expected = Type::bool_literal(value);
        let mut events_db = db.clone();
        for warm in [false, true] {
            events_db.take_salsa_events();
            let root_entries = Cell::new(0);
            let captured = capture(&db, || {
                expression_type_with_counted_root(&prepared, &root_entries)
            })
            .unwrap();
            assert_eq!(root_entries.get(), 1);
            assert_eq!(captured.value, Ok(AnalysisOutcome::Complete(expected)));
            captured.check_root_reads().unwrap();
            assert_no_active_attempt();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            if warm {
                let events = events_db.take_salsa_events();
                for query in ["infer_expression_types_impl", "infer_definition_types"] {
                    assert_function_query_was_not_run_by_name(&db, query, None, &events);
                }
            }
        }

        let mut ordinary_db = setup_db();
        ordinary_db.write_file("src/main.py", source).unwrap();
        ordinary_db
            .write_file("src/dependency.py", dependency)
            .unwrap();
        let expr = expression(&ordinary_db);
        let ordinary = infer_expression_types(&ordinary_db, expr, TypeContext::default());
        assert_eq!(
            ordinary.expression_type(expr.node_ref(&ordinary_db)),
            expected
        );
    }
}

fn imported_boolean_file_fixture(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db.write_file("src/dependency.py", "left = right = False\n")
        .unwrap();
    db
}

fn assert_boolean_file_completion(db: &TestDb, expected: bool) {
    let prepared = prepare(db);
    let revision = salsa::plumbing::current_revision(db);
    let root_entries = Cell::new(0);
    observations::reset(None);
    let captured = capture(db, || {
        check_file_with_counted_root(&prepared, &root_entries)
    })
    .unwrap();
    assert_eq!(root_entries.get(), 1);
    assert!(
        matches!(&captured.value, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
        "{:?}",
        captured.value,
    );
    captured.check_root_reads().unwrap();
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();

    let scope = ty_python_core::global_scope(db, prepared.program_file());
    let key = scope_inference_ingredient(db).database_key_index(scope.as_id());
    let address = captured
        .reads
        .iter()
        .find(|read| read.key == key)
        .map(|read| read.memo_address);
    assert!(address.is_some());
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let ordinary = capture(db, || {
        super::super::infer_scope_types(db, scope, TypeContext::default())
    })
    .unwrap();
    assert!(
        ordinary
            .reads
            .iter()
            .any(|read| read.key == key && Some(read.memo_address) == address)
    );
    let canonical = ordinary.value;
    for statement in &prepared.parsed_module().syntax().body {
        match statement {
            Stmt::Assign(assignment) => {
                for target in &assignment.targets {
                    assert_eq!(
                        canonical.expression_type(target),
                        Type::bool_literal(expected),
                    );
                }
                assert_eq!(
                    canonical.expression_type(assignment.value.as_ref()),
                    Type::bool_literal(expected),
                );
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    let definition = prepared.semantic_index().expect_single_definition(alias);
                    assert_eq!(
                        infer_definition_types(db, definition).binding_type(definition),
                        Type::bool_literal(expected),
                    );
                }
            }
            _ => {}
        }
    }
    root_entries.set(0);
    let warm = capture(db, || {
        check_file_with_counted_root(&prepared, &root_entries)
    })
    .unwrap();
    assert_eq!(root_entries.get(), 1);
    assert!(
        matches!(&warm.value, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
        "{:?}",
        warm.value,
    );
    warm.check_root_reads().unwrap();
    assert!(
        warm.reads
            .iter()
            .any(|read| read.key == key && Some(read.memo_address) == address)
    );
    let events = events_db.take_salsa_events();
    for query in [
        "infer_scope_types_impl",
        "infer_definition_types",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(db, query, None, &events);
    }

    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = TypeInferenceBuilder::new(
        db,
        &env,
        InferenceRegion::Scope(scope, TypeContext::default()),
        prepared.program_file().file(db),
        prepared.program_file(),
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_scope();
    assert_eq!(canonical, &ordinary);
    assert_eq!(salsa::plumbing::current_revision(db), revision);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn cold_import_and_assignment_statements_publish_canonical_scopes() {
    for source in [
        "from dependency import left\nresult = alias = left\n",
        "from dependency import left, right as other\nresult = left\nalias = other\n",
    ] {
        let db = imported_boolean_file_fixture(source);
        assert_boolean_file_completion(&db, false);
    }
}

#[test]
fn imported_boolean_file_edits_preserve_one_root_and_canonical_results() {
    for (dependency, expected) in [
        ("left = right = False\n# unchanged value\n", false),
        ("left = right = True\n", true),
    ] {
        let mut db =
            imported_boolean_file_fixture("from dependency import left\nresult = alias = left\n");
        assert_boolean_file_completion(&db, false);
        let previous_revision = salsa::plumbing::current_revision(&db);
        db.write_file("src/dependency.py", dependency).unwrap();
        assert_ne!(salsa::plumbing::current_revision(&db), previous_revision);
        assert_boolean_file_completion(&db, expected);
    }
}

#[test]
fn imported_boolean_file_interruption_reuses_completed_scopes_in_the_same_revision() {
    let source = "from dependency import left\nresult = alias = left\n";
    let measured = imported_boolean_file_fixture(source);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    let result = check_file_with_policy(&measured_prepared, &funded());
    assert!(matches!(result, Ok(AnalysisOutcome::Complete(Ok(_)))));
    let mut limited = funded();
    limited.semantic_work_limit -= observations::file_scope_remaining().unwrap() - 1;

    for cancel in [false, true] {
        let db = imported_boolean_file_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(if cancel {
            Some(observations::Event::FileScopeMerged)
        } else {
            None
        });
        let policy = if cancel { funded() } else { limited };
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            check_file_with_policy(&prepared, &policy)
        }));
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(outcome) if !cancel => assert!(matches!(
                outcome,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            )),
            other => panic!("{other:?}"),
        }
        assert!(observations::file_scope_remaining().is_some());
        assert!(observations::counts().1 > 0);
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(()),
        );

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let retry = check_file_with_policy(&prepared, &funded());
        assert!(
            matches!(&retry, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
            "{retry:?}",
        );
        let events = events_db.take_salsa_events();
        for query in [
            "infer_scope_types_impl",
            "infer_definition_types",
            "infer_expression_types_impl",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn import_and_assignment_statement_tails_keep_explicit_refusals() {
    for (source, operation) in [
        ("from missing import left\n", OperationId::ImportDiagnostic),
        (
            "from .dependency import left\n",
            OperationId::ImportDiagnostic,
        ),
        (
            "left, right = (False, False)\n",
            OperationId::AssignmentUnpack,
        ),
        ("value.attr = False\n", OperationId::StatementAssignment),
    ] {
        let db = imported_boolean_file_fixture(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        for _ in 0..2 {
            observations::reset(None);
            assert!(matches!(
                check_file_with_policy(&prepared, &funded()),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(actual),
                    completed: (),
                }) if actual == operation
            ));
            assert_eq!(observations::counts().0, 0);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn known_module_resolution_prepares_the_root_program_file_and_global_scope() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let attempts = Cell::new(0);
    let result = with_analysis_session(&prepared, &funded(), |session| {
        attempts.set(attempts.get() + 1);
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
        let (run, routes) = register(session, &prepared, registry, &values, resources)?;
        let values = &values;
        let prepared = &prepared;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let program = session.program();
            let effects = SourceEffects::new(&access, program);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let file = crate::place::source_effects::SourcePlaceEffects::resolve_known_module(
                &effects,
                session.db(),
                &env,
                KnownModule::Enum,
            )
            .await?;
            let Some(file) = file else {
                return Err(RunError::Contract("fixture has no enum module"));
            };
            let scope = crate::place::source_effects::SourcePlaceEffects::global_scope(
                &effects,
                session.db(),
                file,
            )
            .await?;
            Ok((file, scope))
        })
    });
    let Ok(AnalysisOutcome::Complete((file, scope))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(attempts.get(), 1);
    assert_eq!(file.program(&db), prepared.program_file().program(&db));
    assert_eq!(scope.program_file(&db), file);
    let events = events_db.take_salsa_events();
    for query in [
        "resolve_module_query",
        "parsed_module",
        "semantic_index",
        "global_scope",
    ] {
        assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
    }
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_no_active_attempt();
}

fn known_class_fixture(
    source: Option<&str>,
    stub: bool,
) -> (TestDb, Arc<DependencyPreparationEvents>) {
    let preparation = Arc::new(DependencyPreparationEvents::default());
    let observed = preparation.clone();
    let mut builder = TestDbBuilder::new()
        .with_salsa_event_callback(move |event| observed.observe(event))
        .with_file("src/main.py", "left = right = True\n");
    if let Some(source) = source {
        builder = builder.with_file("src/pydantic/__init__.py", "").with_file(
            if stub {
                "src/pydantic/main.pyi"
            } else {
                "src/pydantic/main.py"
            },
            source,
        );
    }
    (builder.build().unwrap(), preparation)
}

#[derive(Clone, Copy)]
enum MemberReceiver<'a, 'db> {
    Module(&'a ModuleName),
    Type(Type<'db>),
}

fn member_fixture() -> TestDb {
    TestDbBuilder::new()
        .with_file("src/main.py", "left = right = True\n")
        .with_file(
            "src/dependency.py",
            "def callback():\n    pass\n\ndef version_info():\n    pass\n",
        )
        .build()
        .unwrap()
}

fn controlled_member<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    receiver: MemberReceiver<'_, 'db>,
    name: &Name,
    member_policy: MemberLookupPolicy,
    policy: &AnalysisPolicy,
    quote: ExecutionLimits,
) -> Result<AnalysisOutcome<(Type<'db>, MemberLookupResult<'db>)>, AnalysisFailure> {
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
        let weak = Rc::downgrade(&routes);
        let query_routes = Rc::clone(&routes);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes: query_routes,
                    values,
                };
                let ty = match receiver {
                    MemberReceiver::Module(name) => {
                        let Some(module) = access
                            .resolve_module(access.routes.program, name, None)
                            .await?
                        else {
                            return Err(RunError::Contract("fixture module is missing"));
                        };
                        let kind = module.kind_with(&access.endpoint).await?;
                        let importing_file = access
                            .endpoint
                            .local_call(|| {
                                access.endpoint.admit_work(2)?;
                                access.endpoint.check_completion()?;
                                Ok(if kind.is_package() {
                                    Some(prepared.program_file())
                                } else {
                                    None
                                })
                            })
                            .await;
                        Type::ModuleLiteral(access.module_literal(module, importing_file).await?)
                    }
                    MemberReceiver::Type(ty) => ty,
                };
                access
                    .endpoint
                    .local_call(|| {
                        access.endpoint.admit_work(quote.semantic_work)?;
                        access.endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: quote.requested_bytes,
                        })
                    })
                    .await;
                Ok((ty, access.member_lookup(ty, name, member_policy).await?))
            })
        }));
        assert_eq!(observations::counts().0, 0);
        let normalization = crate::types::normalization::source::observations::snapshot();
        assert_eq!(normalization.children, normalization.dropped);
        assert_eq!(normalization.buffers, normalization.dropped_buffers);
        assert_eq!(normalization.live_buffers, 0);
        assert_eq!(Rc::strong_count(&routes), 1);
        drop(routes);
        assert!(weak.upgrade().is_none());
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

#[test]
fn cold_module_members_publish_full_canonical_results_and_reuse_them() {
    for name in ["callback", "version_info"] {
        let db = member_fixture();
        let prepared = prepare(&db);
        let module = ModuleName::new_static("dependency").unwrap();
        let name = Name::new_static(name);
        let policy = MemberLookupPolicy::NO_INSTANCE_FALLBACK;
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let result = controlled_member(
            &prepared,
            MemberReceiver::Module(&module),
            &name,
            policy,
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
        );
        let Ok(AnalysisOutcome::Complete((receiver, canonical))) = result else {
            panic!("{result:?}");
        };
        let Type::FunctionLiteral(function) = canonical.unwrap().member(&db).place.expect_type()
        else {
            panic!("{canonical:?}");
        };
        assert_eq!(function.name(&db), name.as_str());
        assert_eq!(observations::counts().0, 0);
        assert!(observations::counts().1 > 0);
        assert_no_active_attempt();
        let key = MemberLookupKey::new(
            &db,
            prepared.program_file().program(&db),
            receiver,
            name.as_str(),
            policy,
        );
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(
                &db,
                "member_lookup_with_policy_inner",
                Some(key.as_id()),
                &events,
            )
            .is_some()
        );
        assert!(
            find_will_execute_event_by_name(&db, "infer_definition_types", None, &events).is_some()
        );

        assert_eq!(
            member_lookup_with_policy_impl(&db, key, None, None),
            canonical
        );
        events_db.take_salsa_events();
        let db_view: &dyn Db = &db;
        assert_eq!(
            *member_lookup_ingredient(&db).fetch(
                &db,
                db_view.zalsa(),
                db_view.zalsa_local(),
                key.as_id(),
            ),
            canonical,
        );
        assert_eq!(
            controlled_member(
                &prepared,
                MemberReceiver::Module(&module),
                &name,
                policy,
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
            ),
            result,
        );
        let events = events_db.take_salsa_events();
        for query in ["member_lookup_with_policy_inner", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn imported_submodule_takes_precedence_over_a_package_global() {
    let db = TestDbBuilder::new()
        .with_file("src/main.py", "import package.child\nleft = right = True\n")
        .with_file("src/package/__init__.py", "child = True\n")
        .with_file("src/package/child.py", "")
        .build()
        .unwrap();
    let prepared = prepare(&db);
    let module = ModuleName::new_static("package").unwrap();
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        controlled_member(
            &prepared,
            MemberReceiver::Module(&module),
            &Name::new_static("child"),
            MemberLookupPolicy::default(),
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
        ),
        Ok(unavailable(OperationId::Submodule)),
    );
    let events = events_db.take_salsa_events();
    assert!(
        find_will_execute_event_by_name(&db, "member_lookup_with_policy_inner", None, &events)
            .is_some()
    );
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn configured_version_fields_resolve_before_nominal_instance_lookup() {
    for version in [PythonVersion::PY310, PythonVersion::PY313] {
        let db = TestDbBuilder::new()
            .with_python_version(version)
            .with_file("src/main.py", "left = right = True\n")
            .build()
            .unwrap();
        let prepared = prepare(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let module = ModuleName::new_static("sys").unwrap();
        let version_info = controlled_member(
            &prepared,
            MemberReceiver::Module(&module),
            &Name::new_static("version_info"),
            MemberLookupPolicy::default(),
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
        );
        let Ok(AnalysisOutcome::Complete((_, version_info))) = version_info else {
            panic!("{version_info:?}");
        };
        let version_info = version_info.unwrap().member(&db).place.expect_type();
        assert_eq!(version_info, Type::sys_version_info());
        for (name, segment) in [("major", version.major), ("minor", version.minor)] {
            let name = Name::new_static(name);
            let result = controlled_member(
                &prepared,
                MemberReceiver::Type(version_info),
                &name,
                MemberLookupPolicy::default(),
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
            );
            let Ok(AnalysisOutcome::Complete((receiver, canonical))) = result else {
                panic!("{result:?}");
            };
            assert_eq!(
                canonical.unwrap().member(&db).place.expect_type(),
                Type::int_literal(segment.into()),
            );
            let key = MemberLookupKey::new(
                &db,
                prepared.program_file().program(&db),
                receiver,
                name.as_str(),
                MemberLookupPolicy::default(),
            );
            assert_eq!(
                member_lookup_with_policy_impl(&db, key, None, None),
                canonical
            );
        }
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "member_lookup_with_policy_inner", None, &events)
                .is_some()
        );
        for query in [
            "infer_definition_types",
            "known_class_to_class_literal",
            "class_member_with_policy_inner",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts(), (0, 0, 0));
        assert_no_active_attempt();
    }
}

#[test]
fn member_prefetch_limits_release_owners_and_retry_in_the_same_revision() {
    for bytes in [false, true] {
        let db = member_fixture();
        let prepared = prepare(&db);
        let module = ModuleName::new_static("dependency").unwrap();
        let name = Name::new_static("callback");
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let quote = if bytes {
            ExecutionLimits {
                semantic_work: 0,
                requested_bytes: funded().requested_bytes_limit,
            }
        } else {
            ExecutionLimits {
                semantic_work: funded().semantic_work_limit,
                requested_bytes: 0,
            }
        };
        assert_eq!(
            controlled_member(
                &prepared,
                MemberReceiver::Module(&module),
                &name,
                MemberLookupPolicy::default(),
                &funded(),
                quote,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: if bytes {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: (),
            }),
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "member_lookup_with_policy_inner",
            None,
            &events_db.take_salsa_events(),
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        let retry = controlled_member(
            &prepared,
            MemberReceiver::Module(&module),
            &name,
            MemberLookupPolicy::default(),
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
        );
        let Ok(AnalysisOutcome::Complete((_, canonical))) = retry else {
            panic!("{retry:?}");
        };
        let Type::FunctionLiteral(function) = canonical.unwrap().member(&db).place.expect_type()
        else {
            panic!("{canonical:?}");
        };
        assert_eq!(function.name(&db), "callback");
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn member_cancellation_after_definition_storage_reuses_published_memos() {
    let db = member_fixture();
    let prepared = prepare(&db);
    let module = ModuleName::new_static("dependency").unwrap();
    let name = Name::new_static("callback");
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(Some(observations::Event::DefinitionStored));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        controlled_member(
            &prepared,
            MemberReceiver::Module(&module),
            &name,
            MemberLookupPolicy::default(),
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
        )
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert!(observations::counts().1 > 0);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let events = events_db.take_salsa_events();
    assert!(
        find_will_execute_event_by_name(&db, "member_lookup_with_policy_inner", None, &events)
            .is_some()
    );
    assert!(
        find_will_execute_event_by_name(&db, "infer_definition_types", None, &events).is_some()
    );
    observations::reset(None);
    let retry = controlled_member(
        &prepared,
        MemberReceiver::Module(&module),
        &name,
        MemberLookupPolicy::default(),
        &funded(),
        NO_EXTRA_MODULE_CHARGE,
    );
    let Ok(AnalysisOutcome::Complete((receiver, canonical))) = retry else {
        panic!("{retry:?}");
    };
    let Type::FunctionLiteral(function) = canonical.unwrap().member(&db).place.expect_type() else {
        panic!("{canonical:?}");
    };
    assert_eq!(function.name(&db), "callback");
    let events = events_db.take_salsa_events();
    // Both queries can publish before the outer root observes cancellation.
    for query in ["member_lookup_with_policy_inner", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    let key = MemberLookupKey::new(
        &db,
        prepared.program_file().program(&db),
        receiver,
        name.as_str(),
        MemberLookupPolicy::default(),
    );
    assert_eq!(
        member_lookup_with_policy_impl(&db, key, None, None),
        canonical,
    );
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

fn controlled_known_class<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    program: Program<'db>,
    class: KnownClass,
    policy: &AnalysisPolicy,
    quote: ExecutionLimits,
    attempts: &Cell<usize>,
) -> Result<
    AnalysisOutcome<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>>,
    AnalysisFailure,
> {
    with_analysis_session(prepared, policy, |session| {
        attempts.set(attempts.get() + 1);
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
        let class_values = register_class_values(session.db(), &mut registry)?;
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
            class: class_values,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            access
                .endpoint
                .local_call(|| {
                    access.endpoint.admit_work(quote.semantic_work)?;
                    access.endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: quote.requested_bytes,
                    })
                })
                .await;
            access.known_class_lookup(program, class).await
        })
    })
}

#[test]
fn cold_known_class_lookup_preserves_complete_canonical_results() {
    for (source, stub) in [
        (None, false),
        (Some("def BaseModel():\n    pass\n"), false),
        (Some("class BaseModel:\n    pass\n"), false),
        (
            Some("class BaseModel(metaclass=Meta):\n    pass\nclass Meta(type):\n    pass\n"),
            true,
        ),
    ] {
        let (db, preparation) = known_class_fixture(source, stub);
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let attempts = Cell::new(0);
        observations::reset(None);
        let mut events_db = db.clone();
        let structural_events = events_db.take_salsa_events();
        preparation.arm(&db, &structural_events);
        for event in &structural_events {
            if let salsa::EventKind::WillExecute { database_key } = event.kind
                && db.ingredient_debug_name(database_key.ingredient_index()) == "file_to_module"
            {
                preparation
                    .ingredients
                    .lock()
                    .unwrap()
                    .push(database_key.ingredient_index());
            }
        }
        assert_eq!(preparation.ingredients.lock().unwrap().len(), 3);
        let traced = capture(&db, || {
            controlled_known_class(
                &prepared,
                program,
                KnownClass::PydanticBaseModel,
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
                &attempts,
            )
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete(result)) = traced.value else {
            panic!("{:?}", traced.value);
        };
        assert_eq!(attempts.get(), 2);
        match source {
            None => assert_eq!(
                result,
                Err(KnownClassLookupError::ClassNotFound { third_party: true })
            ),
            Some(source) if source.starts_with("def ") => {
                let Err(KnownClassLookupError::SymbolNotAClass {
                    found_type: Type::FunctionLiteral(function),
                    third_party: true,
                }) = result
                else {
                    panic!("{result:?}");
                };
                assert_eq!(function.name(&db), "BaseModel");
            }
            Some(_) => {
                let literal = result.unwrap().unwrap();
                assert_eq!(literal.name(&db), "BaseModel");
            }
        }
        assert!(traced.reads.iter().any(|read| {
            db.ingredient_debug_name(read.key.ingredient_index()) == "resolve_module_query"
                && read.parent.is_some_and(|parent| {
                    db.ingredient_debug_name(parent.ingredient_index())
                        == "known_class_to_class_literal"
                })
        }));
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        events_db.take_salsa_events();
        let argument = KnownClassArgument::new(&db, KnownClass::PydanticBaseModel, program);
        let db_view: &dyn Db = &db;
        assert_eq!(
            *known_class_to_class_literal_ingredient(&db).fetch(
                &db,
                db_view.zalsa(),
                db_view.zalsa_local(),
                argument.as_id(),
            ),
            result,
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "known_class_to_class_literal",
            None,
            &events_db.take_salsa_events(),
        );
    }
}

#[derive(Clone, Copy)]
enum EnumMetadataSource<'db> {
    Known(KnownClass),
    Definition(Definition<'db>),
}

fn controlled_enum_metadata<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    source: EnumMetadataSource<'db>,
) -> Result<
    AnalysisOutcome<(StaticClassLiteral<'db>, Option<&'db EnumMetadata<'db>>)>,
    AnalysisFailure,
> {
    with_analysis_session(prepared, &funded(), |session| {
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let class = match source {
                EnumMetadataSource::Known(known) => {
                    let result = access
                        .known_class_lookup(access.routes.program, known)
                        .await?;
                    let Ok(Some(class)) = result else {
                        return Err(RunError::Contract("fixture known class is missing"));
                    };
                    class
                }
                EnumMetadataSource::Definition(definition) => {
                    let inference = access.definition(definition).await?;
                    access
                        .endpoint
                        .local_call(|| {
                            access.endpoint.admit_work(2)?;
                            access.endpoint.check_completion()?;
                            let Some(ClassLiteral::Static(class)) =
                                inference.original_class_type(definition)
                            else {
                                return Err(RunError::Contract(
                                    "fixture definition is not a static class",
                                ));
                            };
                            Ok(class)
                        })
                        .await
                }
            };
            let metadata = access.enum_metadata(class).await?;
            Ok((class, metadata))
        })
    })
}

#[test]
fn cold_enum_metadata_publishes_canonical_known_class_absence() {
    let db = class_fixture();
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let source = EnumMetadataSource::Known(KnownClass::Enum);
    let result = controlled_enum_metadata(&prepared, source);
    let Ok(AnalysisOutcome::Complete((class, metadata))) = result else {
        panic!("{result:?}");
    };
    assert_eq!(class.known(&db), Some(KnownClass::Enum));
    assert_eq!(metadata, None);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "known_class_to_class_literal",
        "enum_metadata",
    ] {
        assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
    }
    assert_eq!(
        crate::types::enums::enum_metadata(&db, ClassLiteral::Static(class)),
        metadata
    );
    assert_eq!(controlled_enum_metadata(&prepared, source), result);
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "known_class_to_class_literal",
        "enum_metadata",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
}

#[test]
fn cold_non_enum_metadata_publishes_and_reuses_canonical_absence() {
    let db = class_fixture();
    let prepared = prepare(&db);
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    observations::reset(None);
    for attempt in 0..2 {
        events_db.take_salsa_events();
        let result =
            controlled_enum_metadata(&prepared, EnumMetadataSource::Definition(definition));
        let Ok(AnalysisOutcome::Complete((class, None))) = result else {
            panic!("{result:?}");
        };
        assert_eq!(class.definition(&db), definition);
        let events = events_db.take_salsa_events();
        if attempt == 1 {
            assert_function_query_was_not_run_by_name(&db, "enum_metadata", None, &events);
            assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
        } else {
            assert!(find_will_execute_event_by_name(&db, "enum_metadata", None, &events).is_some());
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn known_class_source_retries_share_limits_and_release_query_claims() {
    for bytes in [false, true] {
        let (db, _) = known_class_fixture(None, false);
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let attempts = Cell::new(0);
        let policy = funded();
        let quote = if bytes {
            ExecutionLimits {
                semantic_work: 0,
                requested_bytes: policy.requested_bytes_limit / 2 + 1,
            }
        } else {
            ExecutionLimits {
                semantic_work: policy.semantic_work_limit / 2 + 1,
                requested_bytes: 0,
            }
        };
        assert_eq!(
            controlled_known_class(
                &prepared,
                program,
                KnownClass::PydanticBaseModel,
                &policy,
                quote,
                &attempts
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: if bytes {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: (),
            }),
        );
        assert_eq!(attempts.get(), 2);
        assert_no_active_attempt();
        attempts.set(0);
        assert_eq!(
            controlled_known_class(
                &prepared,
                program,
                KnownClass::PydanticBaseModel,
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
                &attempts
            ),
            Ok(AnalysisOutcome::Complete(Err(
                KnownClassLookupError::ClassNotFound { third_party: true }
            ))),
        );
        assert_eq!(attempts.get(), 2);
        assert_no_active_attempt();
    }
}

#[test]
fn known_class_lookup_rejects_foreign_programs_before_interning() {
    let (db, _) = known_class_fixture(None, false);
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let platform = if *program.python_platform(&db) == PythonPlatform::All {
        PythonPlatform::Identifier("linux".into())
    } else {
        PythonPlatform::All
    };
    let foreign = Program::new(&db, &platform, program.resolver_environment(&db));
    let attempts = Cell::new(0);
    assert_eq!(
        controlled_known_class(
            &prepared,
            foreign,
            KnownClass::PydanticBaseModel,
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
            &attempts
        ),
        Err(AnalysisFailure::Execution(RunError::Contract(
            "known class program is foreign"
        ))),
    );
    assert_eq!(attempts.get(), 1);
    assert_no_active_attempt();
}

#[test]
fn known_class_key_schema_covers_both_attached_queries() {
    let (db, _) = known_class_fixture(None, false);
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    assert_eq!(
        with_analysis_session(&prepared, &funded(), |session| {
            let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
            let owner = KnownClassArgument::ingredient(session.db().zalsa());
            let lookup = known_class_to_class_literal_ingredient(session.db());
            let _instance = known_class_to_instance_ingredient(session.db());
            let partial = registry.passive_memo::<_, _, FixedQueryKeyProfile>(owner, lookup)?;
            assert!(matches!(
                registry.finite_interned_values_with_memos(owner, (partial,)),
                Err(RunError::Contract(_))
            ));
            let _values = register_known_class_values(session.db(), &mut registry)?;
            registry.seal()?.run(|_| async { Ok(()) })
        }),
        Ok(AnalysisOutcome::Complete(()))
    );
    assert!(
        events_db
            .take_salsa_events()
            .iter()
            .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
}

#[test]
fn completed_known_class_lookup_observes_dependency_edits() {
    let (mut db, _) = known_class_fixture(Some("class BaseModel:\n    pass\n"), false);
    for is_class in [true, false] {
        if !is_class {
            db.write_file("src/pydantic/main.py", "def BaseModel():\n    pass\n")
                .unwrap();
        }
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let program = prepared.program_file().program(&db);
        let result = controlled_known_class(
            &prepared,
            program,
            KnownClass::PydanticBaseModel,
            &funded(),
            NO_EXTRA_MODULE_CHARGE,
            &Cell::new(0),
        );
        let Ok(AnalysisOutcome::Complete(result)) = result else {
            panic!("{result:?}");
        };
        if is_class {
            assert_eq!(result.unwrap().unwrap().name(&db), "BaseModel");
        } else {
            let Err(KnownClassLookupError::SymbolNotAClass {
                found_type: Type::FunctionLiteral(function),
                third_party: true,
            }) = result
            else {
                panic!("{result:?}");
            };
            assert_eq!(function.name(&db), "BaseModel");
        }
        assert_eq!(
            controlled_known_class(
                &prepared,
                program,
                KnownClass::PydanticBaseModel,
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
                &Cell::new(0),
            ),
            Ok(AnalysisOutcome::Complete(result)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn unsupported_statement_refuses_without_native_scope_inference() {
    let mut db = assignment_fixture("value: bool\nleft = right = False\n");
    let prepared = prepare(&db);
    let expr = expression(&db);
    let scope = expr.scope(&db);
    assert_eq!(InferScope::Bare(scope).as_id(), scope.as_id());
    assert_eq!(InferExpression::Bare(expr).as_id(), expr.as_id());
    let result = with_analysis_session(&prepared, &funded(), |session| {
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
        let (run, routes) = register(session, &prepared, registry, &values, resources)?;
        let values = &values;
        assert_eq!(
            routes.scope.database_key(scope.as_id()),
            scope_inference_ingredient(&db).database_key_index(scope.as_id())
        );
        assert_eq!(
            routes.expression.database_key(expr.as_id()),
            expression_inference_ingredient(&db).database_key_index(expr.as_id())
        );
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            access.scope(scope, TypeContext::default()).await?;
            Ok(())
        })
    });
    assert_eq!(
        result,
        Ok(unavailable(OperationId::StatementAnnotatedAssignment))
    );
    drop(prepared);
    let events = db.take_salsa_events();
    assert!(
        find_will_execute_event_by_name(&db, "infer_scope_types_impl", None, &events).is_some()
    );
}

#[test]
fn contextual_scope_keys_refuse_before_interning() {
    let mut db = fixture();
    let prepared = prepare(&db);
    let expr = expression(&db);
    let scope = expr.scope(&db);
    let result = with_analysis_session(&prepared, &funded(), |session| {
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
        let (run, routes) = register(session, &prepared, registry, &values, resources)?;
        let values = &values;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            let context = TypeContext::new(Some(Type::unknown()));
            access.scope(scope, context).await?;
            Ok(())
        })
    });
    assert_eq!(result, Ok(unavailable(OperationId::ContextualScopeKey)));
    drop(prepared);
    let events = db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
    for event in &events {
        if let salsa::EventKind::DidInternValue { key, .. } = event.kind {
            let name = db.ingredient_debug_name(key.ingredient_index());
            assert!(!name.contains("ScopeWithContext"));
        }
    }
}

#[test]
fn refusal_cleanup_then_exact_ordinary_memo_reuse() {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = [1]\n")
        .unwrap();
    let prepared = prepare(&db);
    {
        let expr = expression(&db);
        for _ in 0..2 {
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(unavailable(OperationId::ExpressionKind))
            );
        }
        let ordinary = super::super::infer_expression_types(&db, expr, TypeContext::default());
        let expected = ordinary.expression_type(expr.node_ref(&db));
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(expected))
        );
    }
    drop(prepared);
    db.take_salsa_events();
    let prepared = prepare(&db);
    {
        assert!(matches!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(_))
        ));
    }
    drop(prepared);
    let events = db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
}

#[test]
fn protected_unavailable_operations_keep_their_exact_identity() {
    let db = fixture();
    let prepared = prepare(&db);
    for operation in [
        OperationId::ScopeBody,
        OperationId::ExpressionKind,
        OperationId::ScopeCycleInitial,
        OperationId::ExpressionCycleInitial,
        OperationId::ScopeCycleRecovery,
        OperationId::ExpressionCycleRecovery,
    ] {
        let result = with_analysis_session(&prepared, &funded(), |session| {
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
            let (run, _) = register(session, &prepared, registry, &values, resources)?;
            run.run(|endpoint| async move {
                Ok(endpoint
                    .local_call(|| session.unavailable::<()>(&endpoint, operation))
                    .await)
            })
        });
        assert_eq!(result, Ok(unavailable(operation)));
    }
}

#[test]
fn existing_resource_cause_is_not_relabelled() {
    let db = fixture();
    let prepared = prepare(&db);
    for cause in [Incomplete::Allowance, Incomplete::RequestedAllocation] {
        let result = with_analysis_session(&prepared, &funded(), |session| {
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
            let (run, _) = register(session, &prepared, registry, &values, resources)?;
            run.run(|endpoint| async move {
                Ok(endpoint
                    .local_call(|| {
                        report_incomplete(session.db(), cause);
                        session.unavailable::<()>(&endpoint, OperationId::ExpressionKind)
                    })
                    .await)
            })
        });
        assert_eq!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: if cause == Incomplete::Allowance {
                    AnalysisIncomplete::WorkLimit
                } else {
                    AnalysisIncomplete::RequestedAllocationLimit
                },
                completed: (),
            })
        );
    }
}

#[test]
fn hard_errors_and_bare_interruption_are_not_unavailable_operations() {
    let db = fixture();
    let prepared = prepare(&db);
    for error in [
        RunError::Contract("source contract test"),
        RunError::RequiresFetch,
        RunError::Refused(Incomplete::Interrupted),
    ] {
        let result = with_analysis_session(&prepared, &funded(), |session| {
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
            let (run, _) = register(session, &prepared, registry, &values, resources)?;
            run.run(|endpoint| async move { Ok(endpoint.local_call(|| Err::<(), _>(error)).await) })
        });
        assert_eq!(result, Err(AnalysisFailure::Execution(error)));
    }
}

#[test]
fn local_cancellation_retains_the_native_payload() {
    let db = fixture();
    let prepared = prepare(&db);
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        with_analysis_session(&prepared, &funded(), |session| {
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
            let (run, _) = register(session, &prepared, registry, &values, resources)?;
            run.run(|endpoint| async move {
                Ok(endpoint
                    .local_call(|| {
                        session.db().cancellation_token().cancel();
                        endpoint.check_completion()
                    })
                    .await)
            })
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)));
}

#[test]
fn nested_protected_boundaries_share_cumulative_requested_bytes() {
    let db = fixture();
    let prepared = prepare(&db);
    let result = with_analysis_session(&prepared, &funded(), |session| {
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
        let (run, _) = register(session, &prepared, registry, &values, resources)?;
        run.run(|endpoint| async move {
            endpoint
                .local_call(|| {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: 8 * 1024 * 1024,
                    })
                })
                .await;
            endpoint
                .child_call(|| async {
                    endpoint
                        .local_call(|| {
                            endpoint.admit(ExecutionWork::Resource {
                                requested_bytes: 8 * 1024 * 1024,
                            })
                        })
                        .await;
                    Ok(())
                })
                .await;
            Ok(())
        })
    });
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        })
    );
}

async fn scope_initial<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>(
    routes: &Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
    session: &'run AnalysisSession<'session, 'db>,
    endpoint: TaskEndpoint<'run, 'db>,
    scope: ScopeId<'db>,
) -> RunResult<ScopeInference<'db>> {
    let provider = ScopeProvider {
        session,
        routes: routes.clone(),
        values,
    };
    <ScopeProvider<'_, '_, '_, Resources> as CallableRouteProvider<'run, 'db, crate::types::infer::InferScopeTypesImplConfiguration>>::initial(
        &provider,
        endpoint,
        session.db(),
        scope.as_id(),
        InferScope::Bare(scope),
    )
    .await
}

async fn expression_initial<
    'run,
    'session,
    'db: 'run,
    Resources: RelationResourceAccess<'run, 'db> + MappingResourceAccess<'run, 'db>,
>(
    routes: &Rc<SourceRoutes<'run, 'db, Resources>>,
    values: &'run SourceValues<'db>,
    session: &'run AnalysisSession<'session, 'db>,
    endpoint: TaskEndpoint<'run, 'db>,
    expression: Expression<'db>,
) -> RunResult<ExpressionInference<'db>> {
    let provider = ExpressionProvider {
        session,
        routes: routes.clone(),
        values,
    };
    <ExpressionProvider<'_, '_, '_, Resources> as CallableRouteProvider<'run, 'db, crate::types::infer::InferExpressionTypesImplConfiguration>>::initial(
        &provider,
        endpoint,
        session.db(),
        expression.as_id(),
        InferExpression::Bare(expression),
    )
    .await
}

#[test]
fn initial_callbacks_refuse_without_creating_seed_values() {
    let db = fixture();
    let prepared = prepare(&db);
    let expr = expression(&db);
    let scope = expr.scope(&db);
    for is_scope in [false, true] {
        let result = with_analysis_session(&prepared, &funded(), |session| {
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
            let (run, routes) = register(session, &prepared, registry, &values, resources)?;
            let values = &values;
            run.run(|endpoint| async move {
                if is_scope {
                    scope_initial(&routes, values, session, endpoint, scope).await?;
                } else {
                    expression_initial(&routes, values, session, endpoint, expr).await?;
                }
                Ok(())
            })
        });
        assert_eq!(
            result,
            Ok(unavailable(if is_scope {
                OperationId::ScopeCycleInitial
            } else {
                OperationId::ExpressionCycleInitial
            }))
        );
    }
}

#[test]
fn pending_write_cancellation_retains_the_native_payload() {
    let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let event_armed = armed.clone();

    let mut db = TestDbBuilder::new()
        .with_salsa_event_callback(move |event| {
            if matches!(event, salsa::EventKind::DidSetCancellationFlag)
                && event_armed.load(std::sync::atomic::Ordering::SeqCst)
            {
                let _ = cancelled_tx.send(());
            }
        })
        .build()
        .unwrap();
    db.write_file("src/main.py", "left = right = True\n")
        .unwrap();
    let prepared = prepare(&db);
    let stamp = Stamp::current(&db);
    let mut writer = db.clone();
    let (start_tx, start_rx) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        start_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        writer.trigger_cancellation();
        writer
    });
    let armed = &armed;
    let start_tx = &start_tx;
    let cancelled_rx = &cancelled_rx;
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        with_analysis_session(&prepared, &funded(), |session| {
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
            let (run, _) = register(session, &prepared, registry, &values, resources)?;
            run.run(|endpoint| async move {
                Ok(endpoint
                    .local_call(|| {
                        armed.store(true, std::sync::atomic::Ordering::SeqCst);
                        start_tx.send(()).unwrap();
                        cancelled_rx
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .unwrap();
                        endpoint.check_completion()
                    })
                    .await)
            })
        })
    }));
    assert!(matches!(result, Err(salsa::Cancelled::PendingWrite)));
    drop(prepared);
    drop(db);
    let writer = writer.join().unwrap();
    assert!(!stamp.belongs_to(&writer));

    let prepared = prepare(&writer);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(true))),
    );
}

fn call_expressions(db: &TestDb) -> (Expression<'_>, Expression<'_>) {
    let file = system_path_to_file(db, "src/main.py").unwrap();
    let program_file = db.program_file(file);
    let module = parsed_module(db, program_file.python_file(db)).load(db);
    let Some(Stmt::Expr(statement)) = module.syntax().body.last() else {
        panic!("fixture call")
    };
    let call = statement.value.as_call_expr().unwrap();
    let index = semantic_index(db, program_file);
    (
        index.expression(call.func.as_ref()),
        index.expression(statement.value.as_ref()),
    )
}

fn function_call_fixture(parameters: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        format!("def choose({parameters}):\n    return value\nchoose(True)\n"),
    )
    .unwrap();
    db
}

#[test]
fn cold_call_preserves_structural_reads_in_signature_and_definition_queries() {
    let mut sequences = Vec::new();
    for controlled in [false, true] {
        let db = function_call_fixture("value");
        let prepared = prepare(&db);
        let captured = capture(&db, || {
            if controlled {
                assert_eq!(
                    expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                    Ok(AnalysisOutcome::Complete(Type::unknown())),
                );
            } else {
                let (_, call) = call_expressions(&db);
                assert_eq!(
                    infer_expression_types(&db, call, TypeContext::default())
                        .expression_type(call.node_ref(&db)),
                    Type::unknown(),
                );
            }
        })
        .unwrap();

        let Stmt::FunctionDef(function) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture function");
        };
        let definition = prepared.semantic_index().expect_single_definition(function);
        let Type::FunctionLiteral(function) =
            infer_definition_types(&db, definition).binding_type(definition)
        else {
            panic!("fixture function type");
        };
        let parents = [
            function_literal_signature_ingredient(&db).database_key_index(function.as_id()),
            definition_inference_ingredient(&db).database_key_index(definition.as_id()),
        ];
        let structural_keys = [
            parsed_module::prepare_memo(&db, prepared.program_file().python_file(&db))
                .unwrap()
                .database_key(),
            semantic_index::prepare_memo(&db, prepared.program_file())
                .unwrap()
                .database_key(),
        ];
        // Capture preserves repeated query reads. Match each canonical parent separately so
        // preparation outside these queries cannot stand in for their structural dependencies.
        sequences.push(parents.map(|parent| {
            let reads: Vec<_> = captured
                .reads
                .iter()
                .filter(|read| read.parent == Some(parent))
                .filter_map(|read| {
                    let name = db.ingredient_debug_name(read.key.ingredient_index());
                    let dependency = match name.as_ref() {
                        "parsed_module" => 0,
                        "semantic_index" => 1,
                        _ => return None,
                    };
                    assert_eq!(read.status, salsa::prepared_source_probe::Status::Final);
                    assert_eq!(read.stamp, captured.stamp);
                    assert_eq!(read.key, structural_keys[dependency]);
                    Some(dependency)
                })
                .collect();
            assert!(reads.contains(&0));
            assert!(reads.contains(&1));
            reads
        }));
        assert_no_active_attempt();
    }
    assert!(
        sequences[0][0]
            .iter()
            .filter(|dependency| **dependency == 1)
            .count()
            > 1
    );
    assert_eq!(sequences[1][0], sequences[0][0]);
    for parents in &mut sequences {
        for reads in parents {
            let mut seen = [false; 2];
            reads.retain(|dependency| !std::mem::replace(&mut seen[*dependency], true));
        }
    }
    assert_eq!(sequences[1], sequences[0]);
}

#[test]
fn cold_call_publishes_the_original_function_signature() {
    for parameters in [
        "value",
        "value=True",
        "first, /, value=True, *args, flag=False, **kwargs",
    ] {
        let mut db = function_call_fixture(parameters);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        {
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::unknown())),
            );
            assert_eq!(observations::signature_ready().0, 1);
            assert_eq!(observations::counts().0, 0);
        }
        drop(prepared);
        db.take_salsa_events();
        let prepared = prepare(&db);
        {
            let (callee, _) = call_expressions(&db);
            let Type::FunctionLiteral(function) =
                infer_expression_types(&db, callee, TypeContext::default())
                    .expression_type(callee.node_ref(&db))
            else {
                panic!("fixture function");
            };
            let signature = function.signature(&db);
            assert_eq!(
                signature,
                &CallableSignature::single(function.literal(&db).last_definition.signature(&db))
            );
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::unknown())),
            );
            assert!(std::ptr::eq(signature, function.signature(&db)));
        }
        drop(prepared);
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "function_literal_signature", None, &events);
        assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
        assert_eq!(observations::signature_ready().0, 1);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn refusal_around_signature_completion_retries_in_the_same_revision() {
    let measured = function_call_fixture("value=True");
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    let signature_work = funded().semantic_work_limit - observations::signature_ready().1.unwrap();
    // The completed output is retained before the final local admission. Canonical publication
    // needs separate work admission, so exhausting work here leaves the signature unpublished.
    for before_completion in [true, false] {
        let db = function_call_fixture("value=True");
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        assert_eq!(
            expression_type_with_policy(
                &prepared,
                expression_key(&prepared),
                &AnalysisPolicy {
                    semantic_work_limit: signature_work - usize::from(before_completion),
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
        );
        assert_eq!(
            observations::signature_ready().0,
            usize::from(!before_completion)
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::unknown())),
        );
        assert_eq!(
            observations::signature_ready().0,
            1 + usize::from(!before_completion)
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cancellation_after_signature_construction_cleans_up_and_retries() {
    let db = function_call_fixture("value");
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(Some(observations::Event::FunctionSignatureReady));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(observations::signature_ready().0, 1);
    assert_eq!(observations::counts().0, 0);
    events_db.take_salsa_events();
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "function_literal_signature", None, &events);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn assert_boolean_argument_context<'db>(
    _db: &'db dyn Db,
    arguments: &CallArguments<'_, 'db>,
    bindings: &Bindings<'db>,
) {
    assert_eq!(arguments.len(), 1);
    let mut expected = CallArguments::default();
    expected.push_argument(Argument::Positional, None);
    expected.insert_type(
        0,
        TypeContext::new(Some(Type::unknown())),
        Type::bool_literal(true),
    );
    // Comparing the entire value distinguishes a parameter-keyed inference from a fallback type.
    assert_eq!(arguments.argument_types(0), expected.argument_types(0));
    let mut overloads = bindings.iter_flat().flat_map(IntoIterator::into_iter);
    let binding = overloads.next().unwrap();
    assert!(overloads.next().is_none());
    assert!(binding.errors().is_empty());
    assert_eq!(binding.parameter_types(), &[None]);
    let matches = binding.argument_matches();
    assert_eq!(matches.len(), 1);
    assert!(matches[0].matched);
    assert_eq!(matches[0].parameters.len(), 1);
    assert_eq!(matches[0].parameters[0].index, 0);
}

fn boolean_call_fixture(keyword: bool) -> TestDb {
    let mut db = setup_db();
    let argument = if keyword { "value=True" } else { "True" };
    db.write_file(
        "src/main.py",
        format!("def choose(value):\n    return value\nchoose({argument})\n"),
    )
    .unwrap();
    db
}

fn assert_checked_boolean_argument<'db>(
    _db: &'db dyn Db,
    _arguments: &CallArguments<'_, 'db>,
    bindings: &Bindings<'db>,
) {
    let mut overloads = bindings.iter_flat().flat_map(IntoIterator::into_iter);
    let binding = overloads.next().unwrap();
    assert!(overloads.next().is_none());
    assert!(binding.errors().is_empty());
    assert_eq!(binding.parameter_types(), &[Some(Type::bool_literal(true))]);
}

#[test]
fn cold_call_infers_boolean_arguments_under_the_original_parameter_context() {
    for keyword in [false, true] {
        let mut db = boolean_call_fixture(keyword);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        invocation_observations::reset_invocations();
        observations::set_arguments_observer(assert_boolean_argument_context);
        observations::set_checked_observer(assert_checked_boolean_argument);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::unknown())),
        );
        assert_eq!(observations::argument_progress().0, 1);
        assert_eq!(observations::signature_ready().0, 1);
        assert_eq!(observations::counts().0, 0);
        let invocation = invocation_observations::invocation_snapshot();
        let events = &invocation.events[..invocation.count];
        assert_eq!(events.len(), 4);
        assert_eq!(
            events.iter().map(|event| event.unwrap().stage).collect::<Vec<_>>(),
            [
                invocation_observations::InvocationStage::Allocated,
                invocation_observations::InvocationStage::Preparing,
                invocation_observations::InvocationStage::ArgumentCheck,
                invocation_observations::InvocationStage::BinderCheck,
            ],
        );
        let original = events[0].unwrap().builder;
        assert!(events.iter().all(|event| event.unwrap().builder == original));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        drop(prepared);
        db.take_salsa_events();
        let prepared = prepare(&db);
        {
            let (_, call) = call_expressions(&db);
            let canonical = infer_expression_types(&db, call, TypeContext::default());
            let program_file = call.program_file(&db);
            let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
            let index = semantic_index(&db, program_file);
            let env = ProgramEnvironment::from_file(program_file);
            let ordinary = TypeInferenceBuilder::new(
                &db,
                &env,
                InferenceRegion::Expression(call, TypeContext::default()),
                program_file.file(&db),
                program_file,
                index,
                &module,
            )
            .finish_expression();
            assert_eq!(canonical, &ordinary);
            assert_eq!(
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::unknown())),
            );
            assert!(std::ptr::eq(
                canonical,
                infer_expression_types(&db, call, TypeContext::default()),
            ));
            assert_eq!(observations::argument_progress().0, 1);
        }
        drop(prepared);
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events,
        );
        assert_function_query_was_not_run_by_name(&db, "function_literal_signature", None, &events);
    }
}

#[test]
fn argument_preparation_interruption_cleans_up_and_retries() {
    let measured = boolean_call_fixture(false);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    invocation_observations::reset_invocations();
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    let event = invocation_observations::invocation_snapshot().events[1].unwrap();
    assert_eq!(
        event.stage,
        invocation_observations::InvocationStage::Preparing
    );
    let preparation_work = funded().semantic_work_limit - event.remaining_work.unwrap();

    for cancel in [false, true] {
        let db = boolean_call_fixture(false);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        invocation_observations::reset_invocations();
        if cancel {
            invocation_observations::set_invocation_cancel_at(Some(
                invocation_observations::InvocationStage::Preparing,
            ));
            let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
            }));
            assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        } else {
            assert_eq!(
                expression_type_with_policy(
                    &prepared,
                    expression_key(&prepared),
                    &AnalysisPolicy {
                        semantic_work_limit: preparation_work,
                        ..funded()
                    },
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                }),
            );
        }
        let interrupted = invocation_observations::invocation_snapshot();
        // Native cancellation waits for the active canonical query to complete. Budget refusal
        // stops preparation, so only that case must construct another builder on retry.
        assert_eq!(
            interrupted.count,
            if cancel { 4 } else { 2 },
            "cancel={cancel}: {interrupted:?}"
        );
        let original = interrupted.events[0].unwrap().builder;
        assert!(
            interrupted.events[..interrupted.count]
                .iter()
                .all(|event| event.unwrap().builder == original)
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        invocation_observations::reset_invocations();
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::unknown())),
        );
        assert_eq!(
            invocation_observations::invocation_snapshot().count,
            if cancel { 0 } else { 4 }
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn refusal_around_argument_completion_cleans_up_and_retries() {
    let measured = boolean_call_fixture(false);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    observations::set_arguments_observer(assert_boolean_argument_context);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    let argument_work = funded().semantic_work_limit - observations::argument_progress().1.unwrap();
    for before_completion in [true, false] {
        let db = boolean_call_fixture(false);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        observations::set_arguments_observer(assert_boolean_argument_context);
        assert_eq!(
            expression_type_with_policy(
                &prepared,
                expression_key(&prepared),
                &AnalysisPolicy {
                    semantic_work_limit: argument_work - usize::from(before_completion),
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
        );
        assert_eq!(
            observations::argument_progress().0,
            usize::from(!before_completion)
        );
        assert_eq!(observations::counts().0, 0);
        let created = observations::counts().1;
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::unknown())),
        );
        assert_eq!(
            observations::argument_progress().0,
            1 + usize::from(!before_completion)
        );
        assert_eq!(observations::signature_ready().0, 1);
        assert_eq!(observations::counts().0, 0);
        assert!(observations::counts().1 > created);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cancellation_after_argument_inference_cleans_up_and_retries() {
    let db = boolean_call_fixture(true);
    let prepared = prepare(&db);
    let mut events_db = db.clone();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(Some(observations::Event::ArgumentsReady));
    observations::set_arguments_observer(assert_boolean_argument_context);
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(observations::argument_progress().0, 1);
    assert_eq!(observations::counts().0, 0);
    let created = observations::counts().1;
    events_db.take_salsa_events();
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "function_literal_signature", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
    assert_eq!(observations::signature_ready().0, 1);
    assert_eq!(observations::argument_progress().0, 1);
    assert_eq!(observations::call_progress().0, 1);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(observations::counts().1, created);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn refusal_around_call_storage_cleans_up_and_retries() {
    let measured = boolean_call_fixture(false);
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Type::unknown())),
    );
    let stored_work = funded().semantic_work_limit - observations::call_progress().1.unwrap();
    for before_storage in [true, false] {
        let db = boolean_call_fixture(false);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        assert_eq!(
            expression_type_with_policy(
                &prepared,
                expression_key(&prepared),
                &AnalysisPolicy {
                    semantic_work_limit: stored_work - usize::from(before_storage),
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
        );
        assert_eq!(
            observations::call_progress().0,
            usize::from(!before_storage)
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::unknown())),
        );
        assert_eq!(
            observations::call_progress().0,
            1 + usize::from(!before_storage)
        );
        assert_eq!(observations::signature_ready().0, 1);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn cancellation_after_call_checking_or_storage_preserves_canonical_completion() {
    for event in [
        observations::Event::ArgumentsChecked,
        observations::Event::CallStored,
    ] {
        let db = boolean_call_fixture(false);
        let prepared = prepare(&db);
        let mut events_db = db.clone();
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(Some(event));
        observations::set_checked_observer(assert_checked_boolean_argument);
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
        }));
        assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
        assert_eq!(observations::counts().0, 0);
        let created = observations::counts().1;
        events_db.take_salsa_events();
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::unknown())),
        );
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events,
        );
        assert_eq!(observations::call_progress().0, 1);
        assert_eq!(observations::counts().1, created);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn diagnostic_callee_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "unknown_name()\n").unwrap();
    let (callee, _) = call_expressions(&db);
    let native = infer_expression_types(&db, callee, TypeContext::default());
    assert!(
        native
            .extra
            .as_ref()
            .is_some_and(|extra| !extra.diagnostics.is_empty())
    );
    db
}

#[test]
fn native_callee_diagnostics_merge_before_the_next_call_boundary() {
    let db = diagnostic_callee_fixture();
    let prepared = prepare(&db);
    let (callee, _) = call_expressions(&db);
    let native = infer_expression_types(&db, callee, TypeContext::default());
    let diagnostics = (&native.extra.as_ref().unwrap().diagnostics)
        .into_iter()
        .len();
    assert!(diagnostics > 0);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(unavailable(OperationId::CallBindings)),
    );
    assert_eq!(observations::merged_diagnostics(), diagnostics);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(unavailable(OperationId::CallBindings)),
    );
    assert_eq!(observations::merged_diagnostics(), diagnostics);
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn refusal_after_diagnostic_merge_discards_parent_and_retries_same_revision() {
    let measured = diagnostic_callee_fixture();
    let measured_prepared = prepare(&measured);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(unavailable(OperationId::CallBindings))
    );
    let merged_work = funded().semantic_work_limit - observations::merged_remaining().unwrap();
    let db = diagnostic_callee_fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &prepared,
            expression_key(&prepared),
            &AnalysisPolicy {
                semantic_work_limit: merged_work,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert!(observations::merged_diagnostics() > 0);
    assert_eq!(observations::counts(), (0, 1, 0));
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(unavailable(OperationId::CallBindings))
    );
    assert!(observations::merged_diagnostics() > 0);
    assert_eq!(observations::counts(), (0, 2, 0));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

fn controlled_module<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<&'db ScopeInference<'db>>, AnalysisFailure> {
    let scope = ty_python_core::global_scope(db, prepared.program_file());
    controlled_scope(prepared, scope, policy)
}

fn controlled_scope<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    scope: ScopeId<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<&'db ScopeInference<'db>>, AnalysisFailure> {
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
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            access.scope(scope, TypeContext::default()).await
        })
    })
}

#[test]
fn cold_module_publishes_the_full_original_scope_result() {
    for keyword in [false, true] {
        let db = boolean_call_fixture(keyword);
        let prepared = prepare(&db);
        observations::reset(None);
        let result = controlled_module(&db, &prepared, &funded()).unwrap();
        let AnalysisOutcome::Complete(canonical) = result else {
            panic!("{result:?}");
        };
        assert_eq!(observations::counts().0, 0);
        let scope = ty_python_core::global_scope(&db, prepared.program_file());
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Scope(scope, TypeContext::default()),
            prepared.program_file().file(&db),
            prepared.program_file(),
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_scope();
        assert_eq!(canonical, &ordinary);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let retry = controlled_module(&db, &prepared, &funded()).unwrap();
        assert!(matches!(retry, AnalysisOutcome::Complete(warm) if std::ptr::eq(canonical, warm)));
        assert!(std::ptr::eq(
            canonical,
            super::super::infer_scope_types(&db, scope, TypeContext::default())
        ));
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_expression_types_impl",
            None,
            &events,
        );
        assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    }
}

/// A cold scope merges its function's deferred annotations and publishes the ordinary result.
/// Its immediate retry reuses the scope memo without executing the scope or deferred-annotation query.
/// Ordinary inference runs afterward so it cannot prewarm that retry.
#[test]
fn cold_function_annotations_publish_the_full_scope_and_reuse_its_memo() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "from typing import Any\ndef choose(value: Any) -> Any:\n    pass\n",
    )?;
    let prepared = prepare(&db);
    let [Stmt::ImportFrom(_), Stmt::FunctionDef(function)] =
        prepared.parsed_module().syntax().body.as_slice()
    else {
        anyhow::bail!("annotation fixture must contain an import and a function");
    };
    let [parameter] = function.parameters.args.as_slice() else {
        anyhow::bail!("annotation fixture must have exactly one regular parameter");
    };
    let parameter_annotation = parameter
        .annotation()
        .ok_or_else(|| anyhow::anyhow!("annotation fixture parameter must have an annotation"))?;
    let return_annotation = function
        .returns
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("annotation fixture function must have a return annotation"))?;
    let definition = prepared.semantic_index().expect_single_definition(function);
    let scope = ty_python_core::global_scope(&db, prepared.program_file());
    let scope_key = InferScope::Bare(scope).as_id();
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
            .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    observations::reset(None);
    let cold = capture(&db, || controlled_module(&db, &prepared, &funded()))
        .map_err(|error| anyhow::anyhow!("failed to capture cold annotation scope: {error:?}"))?;
    let Ok(AnalysisOutcome::Complete(canonical)) = cold.value else {
        anyhow::bail!("cold annotation scope did not complete: {:?}", cold.value);
    };
    cold.check_root_reads()
        .map_err(|error| anyhow::anyhow!("cold annotation scope read audit failed: {error:?}"))?;
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(())
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok()
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
            .is_ok()
    );
    assert_eq!(canonical.expression_type(parameter_annotation), Type::any());
    assert_eq!(canonical.expression_type(return_annotation), Type::any());

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let retry = controlled_module(&db, &prepared, &funded())
        .map_err(|error| anyhow::anyhow!("annotation scope retry failed: {error:?}"))?;
    assert!(matches!(retry, AnalysisOutcome::Complete(warm) if std::ptr::eq(canonical, warm)));
    assert!(std::ptr::eq(
        canonical,
        super::super::infer_scope_types(&db, scope, TypeContext::default())
    ));
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_scope_types_impl",
        Some(scope_key),
        &events,
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_deferred_types",
        Some(definition.as_id()),
        &events,
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(())
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Scope(scope, TypeContext::default()),
        prepared.program_file().file(&db),
        prepared.program_file(),
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_scope();
    assert_eq!(canonical, &ordinary);
    Ok(())
}

/// Defaults refusal preserves the completed annotation child without publishing the parent scope.
/// A same-revision retry refuses again at FunctionDefaults while reusing the annotation child,
/// which omits the default expression. Both attempts leave the scope unpublished and the database idle.
#[test]
fn function_defaults_refusal_reuses_annotations_without_publishing_the_scope() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        "from typing import Any\ndef choose(value: Any = []) -> Any:\n    pass\n",
    )?;
    let prepared = prepare(&db);
    let [Stmt::ImportFrom(_), Stmt::FunctionDef(function)] =
        prepared.parsed_module().syntax().body.as_slice()
    else {
        anyhow::bail!("defaults fixture must contain an import and a function");
    };
    let [parameter] = function.parameters.args.as_slice() else {
        anyhow::bail!("defaults fixture must have exactly one regular parameter");
    };
    let parameter_annotation = parameter
        .annotation()
        .ok_or_else(|| anyhow::anyhow!("defaults fixture parameter must have an annotation"))?;
    let return_annotation = function
        .returns
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("defaults fixture function must have a return annotation"))?;
    let default = parameter
        .default()
        .ok_or_else(|| anyhow::anyhow!("defaults fixture parameter must have a default value"))?;
    let definition = prepared.semantic_index().expect_single_definition(function);
    let scope = ty_python_core::global_scope(&db, prepared.program_file());
    let scope_key = InferScope::Bare(scope).as_id();
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let mut events_db = db.clone();
    let mut completed_child = None;
    observations::reset(None);
    for _ in 0..2 {
        events_db.take_salsa_events();
        assert_eq!(
            controlled_module(&db, &prepared, &funded()),
            Ok(unavailable(OperationId::Deferred(
                DeferredInferenceOperation::FunctionDefaults
            )))
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(())
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, scope_inference_ingredient(&db), scope_key)
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                deferred_definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .is_ok()
        );
        let child = infer_deferred_types(&db, definition);
        assert_eq!(child.expression_type(parameter_annotation), Type::any());
        assert_eq!(child.expression_type(return_annotation), Type::any());
        assert_eq!(child.try_expression_type(default), None);
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_function_default_types",
            None,
            &events,
        );
        if let Some(previous) = completed_child {
            assert!(std::ptr::eq(previous, child));
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_deferred_types",
                Some(definition.as_id()),
                &events,
            );
        } else {
            assert!(
                find_will_execute_event_by_name(
                    &db,
                    "infer_deferred_types",
                    Some(definition.as_id()),
                    &events,
                )
                .is_some()
            );
            completed_child = Some(child);
        }
    }
    Ok(())
}

#[test]
fn module_refusal_after_call_merge_drains_and_reuses_completed_children() {
    let measured = boolean_call_fixture(false);
    let prepared = prepare(&measured);
    observations::reset(None);
    assert!(matches!(
        controlled_module(&measured, &prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let remaining = observations::merged_remaining().unwrap();
    let mut limited = funded();
    limited.semantic_work_limit -= remaining - 1;

    let db = boolean_call_fixture(false);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert!(matches!(
        controlled_module(&db, &prepared, &limited),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            ..
        })
    ));
    assert_eq!(observations::counts().0, 0);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    assert!(matches!(
        controlled_module(&db, &prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
}

#[test]
fn module_cancellation_cleans_up_before_same_revision_retry() {
    let db = boolean_call_fixture(false);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(Some(observations::Event::CallStored));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        controlled_module(&db, &prepared, &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(observations::counts().0, 0);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    assert!(matches!(
        controlled_module(&db, &prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
}

#[test]
fn cold_function_body_publishes_the_full_original_scope_result() {
    let db = boolean_call_fixture(false);
    let prepared = prepare(&db);
    let scope = prepared
        .semantic_index()
        .scope_ids()
        .find(|scope| scope.node(&db).as_function().is_some())
        .unwrap();
    observations::reset(None);
    let result = controlled_scope(&prepared, scope, &funded()).unwrap();
    let AnalysisOutcome::Complete(canonical) = result else {
        panic!("{result:?}");
    };
    assert_eq!(observations::counts().0, 0);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Scope(scope, TypeContext::default()),
        prepared.program_file().file(&db),
        prepared.program_file(),
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_scope();
    assert_eq!(canonical, &ordinary);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let retry = controlled_scope(&prepared, scope, &funded()).unwrap();
    assert!(matches!(retry, AnalysisOutcome::Complete(warm) if std::ptr::eq(canonical, warm)));
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
}

#[test]
fn cold_class_body_publishes_the_full_original_scope_result() {
    let mut db = setup_db();
    db.write_file("src/main.py", "class Product:\n    True\n")
        .unwrap();
    let prepared = prepare(&db);
    let scope = prepared
        .semantic_index()
        .scope_ids()
        .find(|scope| scope.node(&db).as_class().is_some())
        .unwrap();
    observations::reset(None);
    let result = controlled_scope(&prepared, scope, &funded()).unwrap();
    let AnalysisOutcome::Complete(canonical) = result else {
        panic!("{result:?}");
    };
    assert_eq!(observations::counts().0, 0);
    assert_eq!(canonical.expressions.iter().len(), 1);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Scope(scope, TypeContext::default()),
        prepared.program_file().file(&db),
        prepared.program_file(),
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_scope();
    assert_eq!(canonical, &ordinary);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let retry = controlled_scope(&prepared, scope, &funded()).unwrap();
    assert!(matches!(retry, AnalysisOutcome::Complete(warm) if std::ptr::eq(canonical, warm)));
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
}

#[test]
fn file_refusal_after_scope_merges_cleans_up_and_reuses_canonical_scopes() {
    let measured = boolean_call_fixture(false);
    let prepared = prepare(&measured);
    observations::reset(None);
    let result = check_file_with_policy(&prepared, &funded());
    assert!(
        matches!(&result, Ok(AnalysisOutcome::Complete(Ok(_)))),
        "{result:?}"
    );
    let remaining = observations::file_scope_remaining().unwrap();
    let mut limited = funded();
    limited.semantic_work_limit -= remaining - 1;

    let db = boolean_call_fixture(false);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let result = check_file_with_policy(&prepared, &limited);
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                ..
            })
        ),
        "{result:?}"
    );
    assert_eq!(observations::counts().0, 0);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let result = check_file_with_policy(&prepared, &funded());
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty())
    );
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "infer_scope_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_expression_types_impl", None, &events);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
}

#[test]
fn file_cancellation_after_module_merge_cleans_up_and_retries() {
    let db = boolean_call_fixture(false);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(Some(observations::Event::FileScopeMerged));
    let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
        check_file_with_policy(&prepared, &funded())
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(observations::counts().0, 0);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let result = check_file_with_policy(&prepared, &funded());
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty())
    );
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let events = events_db.take_salsa_events();
    let module = ty_python_core::global_scope(&db, prepared.program_file());
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_scope_types_impl",
        Some(InferScope::Bare(module).as_id()),
        &events,
    );
}

fn controlled_runtime_visibility<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    definition: Definition<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<bool>, AnalysisFailure> {
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
        assert_eq!(
            routes.runtime_visibility.database_key(definition.as_id()),
            runtime_visibility_ingredient(session.db()).database_key_index(definition.as_id())
        );
        let values = &values;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            access.runtime_visibility(definition).await
        })
    })
}

#[test]
fn cold_runtime_visibility_publishes_and_reuses_the_canonical_result() {
    for (path, source) in [
        ("src/main.py", "class Product:\n    pass\n"),
        ("src/main.pyi", "class Product:\n    pass\n"),
        ("src/main.pyi", "class _Product:\n    pass\n"),
    ] {
        let mut db = setup_db();
        db.write_file(path, source).unwrap();
        let file = system_path_to_file(&db, path).unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let definition = prepared.semantic_index().expect_single_definition(class);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        assert_eq!(
            controlled_runtime_visibility(&prepared, definition, &funded()),
            Ok(AnalysisOutcome::Complete(true))
        );
        let events = events_db.take_salsa_events();
        for query in ["may_exist_at_runtime", "infer_definition_types"] {
            assert!(
                find_will_execute_event_by_name(&db, query, Some(definition.as_id()), &events)
                    .is_some()
            );
        }
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        let created = observations::counts().1;
        assert!(created > 0);

        assert!(may_exist_at_runtime(&db, definition));
        assert_eq!(
            controlled_runtime_visibility(&prepared, definition, &funded()),
            Ok(AnalysisOutcome::Complete(true))
        );
        let events = events_db.take_salsa_events();
        for query in ["may_exist_at_runtime", "infer_definition_types"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(observations::counts().0, 0);
        assert_eq!(observations::counts().1, created);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();

        let mut ordinary_db = setup_db();
        ordinary_db.write_file(path, source).unwrap();
        let file = system_path_to_file(&ordinary_db, path).unwrap();
        let ordinary_prepared = prepare_file(&ordinary_db, file).unwrap();
        let Stmt::ClassDef(class) = &ordinary_prepared.parsed_module().syntax().body[0] else {
            panic!("fixture class");
        };
        let definition = ordinary_prepared
            .semantic_index()
            .expect_single_definition(class);
        assert!(may_exist_at_runtime(&ordinary_db, definition));
    }
}

#[test]
fn cold_type_checking_visibility_publishes_false_without_definition_inference() {
    let source =
        "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    class Product:\n        pass\n";
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    let prepared = prepare(&db);
    let Stmt::If(statement) = &prepared.parsed_module().syntax().body[1] else {
        panic!("fixture conditional");
    };
    let Stmt::ClassDef(class) = &statement.body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        controlled_runtime_visibility(&prepared, definition, &funded()),
        Ok(AnalysisOutcome::Complete(false))
    );
    let events = events_db.take_salsa_events();
    assert!(
        find_will_execute_event_by_name(
            &db,
            "may_exist_at_runtime",
            Some(definition.as_id()),
            &events
        )
        .is_some()
    );
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
    assert_eq!(observations::counts(), (0, 0, 0));
    assert_no_active_attempt();

    assert!(!may_exist_at_runtime(&db, definition));
    assert_eq!(
        controlled_runtime_visibility(&prepared, definition, &funded()),
        Ok(AnalysisOutcome::Complete(false))
    );
    let events = events_db.take_salsa_events();
    for query in ["may_exist_at_runtime", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts(), (0, 0, 0));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();

    let mut ordinary_db = setup_db();
    ordinary_db.write_file("src/main.py", source).unwrap();
    let ordinary_prepared = prepare(&ordinary_db);
    let Stmt::If(statement) = &ordinary_prepared.parsed_module().syntax().body[1] else {
        panic!("fixture conditional");
    };
    let Stmt::ClassDef(class) = &statement.body[0] else {
        panic!("fixture class");
    };
    let definition = ordinary_prepared
        .semantic_index()
        .expect_single_definition(class);
    assert!(!may_exist_at_runtime(&ordinary_db, definition));
}

#[test]
fn runtime_visibility_work_refusal_before_fetch_retries_same_revision() {
    let db = class_fixture();
    let prepared = prepare(&db);
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        controlled_runtime_visibility(
            &prepared,
            definition,
            &AnalysisPolicy {
                semantic_work_limit: 0,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        })
    );
    let events = events_db.take_salsa_events();
    for query in ["may_exist_at_runtime", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts(), (0, 0, 0));
    assert_no_active_attempt();

    assert_eq!(
        controlled_runtime_visibility(&prepared, definition, &funded()),
        Ok(AnalysisOutcome::Complete(true))
    );
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    events_db.take_salsa_events();
    assert!(may_exist_at_runtime(&db, definition));
    let events = events_db.take_salsa_events();
    assert_function_query_was_not_run_by_name(&db, "may_exist_at_runtime", None, &events);
}

fn project_builtin_class_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/__builtins__.pyi", "class Product:\n    pass\n")
        .unwrap();
    db.write_file("src/main.py", "left = right = Product\n")
        .unwrap();
    db
}

#[test]
fn cold_implicit_name_resolves_a_project_builtin_and_reuses_runtime_visibility() {
    let db = project_builtin_class_fixture();
    let prepared = prepare(&db);
    let expr = expression(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
    let Ok(AnalysisOutcome::Complete(Type::ClassLiteral(ClassLiteral::Static(class)))) = result
    else {
        panic!("{result:?}");
    };
    assert_eq!(class.name(&db), "Product");
    let builtin_file = system_path_to_file(&db, "src/__builtins__.pyi").unwrap();
    assert_eq!(class.program_file(&db).file(&db), builtin_file);
    let builtin_prepared = prepare_file(&db, builtin_file).unwrap();
    let Stmt::ClassDef(statement) = &builtin_prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = builtin_prepared
        .semantic_index()
        .expect_single_definition(statement);
    let events = events_db.take_salsa_events();
    for query in ["may_exist_at_runtime", "infer_definition_types"] {
        assert!(
            find_will_execute_event_by_name(&db, query, Some(definition.as_id()), &events)
                .is_some()
        );
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let created = observations::counts().1;

    assert!(may_exist_at_runtime(&db, definition));
    let ordinary = infer_expression_types(&db, expr, TypeContext::default());
    let expected = Type::ClassLiteral(ClassLiteral::Static(class));
    assert_eq!(ordinary.expression_type(expr.node_ref(&db)), expected);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(expected))
    );
    let events = events_db.take_salsa_events();
    for query in [
        "may_exist_at_runtime",
        "infer_definition_types",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_eq!(observations::counts().1, created);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();

    let ordinary_db = project_builtin_class_fixture();
    let expr = expression(&ordinary_db);
    let inference = infer_expression_types(&ordinary_db, expr, TypeContext::default());
    let Type::ClassLiteral(ClassLiteral::Static(class)) =
        inference.expression_type(expr.node_ref(&ordinary_db))
    else {
        panic!("fixture builtin class");
    };
    assert_eq!(class.name(&ordinary_db), "Product");
    assert_eq!(
        class.program_file(&ordinary_db).file(&ordinary_db),
        system_path_to_file(&ordinary_db, "src/__builtins__.pyi").unwrap()
    );
}

#[test]
fn cold_deferred_base_resolves_a_project_builtin_and_reuses_runtime_visibility() {
    let mut db = setup_db();
    db.write_file("src/__builtins__.pyi", "class Product:\n    pass\n")
        .unwrap();
    db.write_file("src/main.pyi", "class Derived(Product):\n    pass\n")
        .unwrap();
    let file = system_path_to_file(&db, "src/main.pyi").unwrap();
    let prepared = prepare_file(&db, file).unwrap();
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let result = controlled_class_bases(&prepared, definition, false);
    let Ok(AnalysisOutcome::Complete((_, bases, _, None, None))) = result else {
        panic!("{result:?}");
    };
    let [Type::ClassLiteral(ClassLiteral::Static(product))] = bases else {
        panic!("{bases:?}");
    };
    assert_eq!(product.name(&db), "Product");
    let builtin_file = system_path_to_file(&db, "src/__builtins__.pyi").unwrap();
    assert_eq!(product.program_file(&db).file(&db), builtin_file);
    let builtin_prepared = prepare_file(&db, builtin_file).unwrap();
    let Stmt::ClassDef(class) = &builtin_prepared.parsed_module().syntax().body[0] else {
        panic!("fixture builtin class");
    };
    let builtin_definition = builtin_prepared
        .semantic_index()
        .expect_single_definition(class);
    let events = events_db.take_salsa_events();
    for query in ["may_exist_at_runtime", "infer_definition_types"] {
        assert!(
            find_will_execute_event_by_name(&db, query, Some(builtin_definition.as_id()), &events)
                .is_some()
        );
    }
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    let created = observations::counts().1;
    assert!(may_exist_at_runtime(&db, builtin_definition));
    let retry = controlled_class_bases(&prepared, definition, false);
    assert_eq!(retry, result);
    let events = events_db.take_salsa_events();
    for query in [
        "may_exist_at_runtime",
        "infer_definition_types",
        "infer_deferred_types",
        "explicit_bases_inner",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_eq!(observations::counts().1, created);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
