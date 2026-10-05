use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::mapping::source::observations as mapping_observations;
use crate::types::mapping::source::observations::{
    BindingContextSnapshot, CleanupBoundary, CleanupDrop, Lookup, OwnedMappingSnapshot,
};
use crate::types::typevar::{
    BindingContext, ParamSpecAttrKind, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
    TypeVarKind, TypeVarNonce, bound_typevar_default_ingredient, lazy_typevar_default_ingredient,
};
use crate::types::{KnownInstanceType, MaterializationOperation, TypeMapping};

thread_local! {
    static BINDINGS: Cell<usize> = const { Cell::new(0) };
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
}

pub(in crate::types::infer) fn observe_binding(
    db: &dyn Db,
    _default: Type<'_>,
    _binding: BindingContext<'_>,
) {
    if BINDINGS.replace(BINDINGS.get() + 1) == 0 {
        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
    }
    if CANCEL.replace(false) {
        db.cancellation_token().cancel();
    }
}

fn reset(cancel: bool) {
    BINDINGS.set(0);
    REMAINING.set(None);
    CANCEL.set(cancel);
    observations::reset(None);
    mapping_observations::reset(None);
}

fn fixture() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "src/main.pyi",
        "from typing import Any, Generic, TypeVar\nT = TypeVar(\"T\", default=Any)\nclass Product(Generic[T]): ...\n",
    )?;
    Ok(db)
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_eq!(mapping_observations::tuple_snapshot().live, 0);
    assert_eq!(mapping_observations::set_snapshot().live, 0);
    assert_no_active_attempt();
}

fn expected_mode(binding: BindingContext<'_>) -> OwnedMappingSnapshot {
    OwnedMappingSnapshot::BindLegacyTypevars(match binding {
        BindingContext::Definition(definition) => {
            BindingContextSnapshot::Definition(definition.as_id())
        }
        BindingContext::Synthetic(program) => BindingContextSnapshot::Synthetic(program.as_id()),
    })
}

fn assert_mapping(binding: BindingContext<'_>, minimum_children: usize) {
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1, "{snapshot:?}");
    let root = snapshot.roots[0].unwrap();
    assert_eq!(root.mapping, expected_mode(binding));
    assert!(root.default_context);
    assert!(snapshot.child_count >= minimum_children, "{snapshot:?}");
    for child in &snapshot.children[..snapshot.child_count] {
        let child = child.unwrap();
        assert_eq!(child.visitor, root.visitor);
        assert_eq!(child.mapping, root.mapping);
        assert!(child.default_context);
    }
}

#[test]
fn cold_present_bound_default_matches_ordinary_and_reuses_the_final_memo() -> anyhow::Result<()> {
    let db = fixture()?;
    let prepared = bound_defaults::prepare_fixture(&db);
    let revision = salsa::plumbing::current_revision(&db);
    reset(false);
    let (result, _) = bound_defaults::completed_attempt(&db, &prepared);
    let Ok(AnalysisOutcome::Complete((class, context, variable, Some(default)))) = result else {
        anyhow::bail!("{result:?}");
    };
    assert_eq!(default, Type::any());
    assert_eq!(BINDINGS.get(), 1);
    assert_mapping(variable.binding_context(&db), 0);
    let ingredient = bound_typevar_default_ingredient(&db);
    let key = ingredient.database_key_index(variable.as_id());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, variable.as_id())
            .unwrap()
            .database_key(),
        key
    );
    assert_eq!(variable.default_type(&db), Some(default));
    assert_cleanup();

    let ordinary_db = fixture()?;
    let ordinary_prepared = bound_defaults::prepare_fixture(&ordinary_db);
    let definition = bound_defaults::selected_definition(&ordinary_prepared);
    let Some(ClassLiteral::Static(ordinary_class)) =
        infer_definition_types(&ordinary_db, definition).original_class_type(definition)
    else {
        anyhow::bail!("ordinary fixture class");
    };
    let ordinary_variable = ordinary_class
        .generic_context(&ordinary_db)
        .unwrap()
        .variables(&ordinary_db)
        .next()
        .unwrap();
    assert_eq!(
        ordinary_variable.default_type(&ordinary_db),
        Some(Type::any())
    );

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset(false);
    assert_eq!(
        bound_defaults::controlled_default(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete((
            class,
            context,
            variable,
            Some(default)
        )))
    );
    assert_eq!(BINDINGS.get(), 0);
    assert_eq!(mapping_observations::snapshot().root_count, 0);
    assert!(!bound_defaults::query_ran(
        &db,
        variable.as_id(),
        &events_db.take_salsa_events()
    ));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
    Ok(())
}

#[test]
fn binding_parent_refusal_and_native_cancellation_preserve_completed_children_for_retry()
-> anyhow::Result<()> {
    let measured = fixture()?;
    let prepared = bound_defaults::prepare_fixture(&measured);
    reset(false);
    let (result, preceding) = bound_defaults::completed_attempt(&measured, &prepared);
    assert!(
        matches!(result, Ok(AnalysisOutcome::Complete((_, _, _, Some(_))))),
        "{result:?}"
    );
    let spent = funded().semantic_work_limit - REMAINING.get().unwrap();

    for cancel in [false, true] {
        let db = fixture()?;
        let prepared = bound_defaults::prepare_fixture(&db);
        let revision = salsa::plumbing::current_revision(&db);
        for _ in 0..preceding {
            reset(false);
            assert_eq!(
                bound_defaults::controlled_default(&prepared, &funded()),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
            assert_cleanup();
        }
        reset(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: spent,
                ..funded()
            }
        };
        let interrupted = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            bound_defaults::controlled_default(&prepared, &policy)
        }));
        if cancel {
            assert!(
                matches!(interrupted, Err(salsa::Cancelled::Local)),
                "{interrupted:?}"
            );
        } else {
            assert_eq!(
                interrupted.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
            assert_eq!(REMAINING.get(), Some(0));
        }
        assert_eq!(BINDINGS.get(), 1);
        assert_cleanup();
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let definition = bound_defaults::selected_definition(&prepared);
        let Some(ClassLiteral::Static(class)) =
            infer_definition_types(&db, definition).original_class_type(definition)
        else {
            anyhow::bail!("completed source class child");
        };
        let variable = class
            .generic_context(&db)
            .unwrap()
            .variables(&db)
            .next()
            .unwrap();
        let inspected = events_db.take_salsa_events();
        for query in ["infer_definition_types", "static_class_generic_context"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &inspected);
        }
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                lazy_typevar_default_ingredient(&db),
                variable.typevar(&db).as_id()
            )
            .is_ok()
        );
        let final_memo = FinalSourceMemo::certify(
            &db as &dyn Db,
            bound_typevar_default_ingredient(&db),
            variable.as_id(),
        )
        .map(|_| ());
        if cancel {
            // Salsa finishes cycle-capable queries before delivering local cancellation.
            assert_eq!(final_memo, Ok(()));
        } else {
            assert_eq!(final_memo, Err(FinalSourceError::MissingMemo));
        }
        events_db.take_salsa_events();
        reset(false);
        let (retried, _) = bound_defaults::completed_attempt(&db, &prepared);
        assert!(
            matches!(retried, Ok(AnalysisOutcome::Complete((_, _, retried_variable, Some(default)))) if retried_variable == variable && default == Type::any()),
            "{retried:?}"
        );
        assert_eq!(BINDINGS.get(), usize::from(!cancel));
        let events = events_db.take_salsa_events();
        for query in [
            "infer_definition_types",
            "infer_deferred_types",
            "static_class_generic_context",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        let raw_key =
            lazy_typevar_default_ingredient(&db).database_key_index(variable.typevar(&db).as_id());
        assert!(!events.iter().any(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == raw_key)));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
    Ok(())
}

fn controlled_mapping<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    binding: BindingContext<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
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
                    .bind_legacy_typevars(
                        ty,
                        &ProgramEnvironment::from_file(prepared.program_file()),
                        binding,
                    )
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
        assert_eq!(pools, [1, 0, 0, 1, 0]);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(mapping_observations::tuple_snapshot().live, 0);
        assert_eq!(mapping_observations::set_snapshot().live, 0);
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

fn raw<'db>(db: &'db dyn Db, kind: TypeVarKind, name: &'static str) -> TypeVarInstance<'db> {
    TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static(name), None, kind),
        None,
        None,
        None,
    )
}

fn wrap<'db>(db: &'db dyn Db, mut ty: Type<'db>, depth: usize) -> Type<'db> {
    for _ in 0..depth {
        ty = TypeFormType::from_type_expression(db, ty);
    }
    ty
}

#[test]
fn definition_and_synthetic_binding_preserve_full_instances_and_existing_bound_handles()
-> anyhow::Result<()> {
    for synthetic in [false, true] {
        let db = fixture()?;
        let prepared = bound_defaults::prepare_fixture(&db);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let definition = bound_defaults::selected_definition(&prepared);
        let program = prepared.program_file().program(&db);
        let (binding, original_binding) = if synthetic {
            (
                BindingContext::Synthetic(program),
                BindingContext::Definition(definition),
            )
        } else {
            (
                BindingContext::Definition(definition),
                BindingContext::Synthetic(program),
            )
        };
        let first = raw(&db, TypeVarKind::LegacyTypeVar, "T");
        let second = TypeVarInstance::new(
            &db,
            first.identity(&db),
            None,
            None,
            Some(TypeVarDefaultEvaluation::Eager(Type::any())),
        );
        let paramspec = raw(&db, TypeVarKind::LegacyParamSpec, "P");
        let bound = BoundTypeVarInstance::new(
            &db,
            paramspec,
            original_binding,
            Some(ParamSpecAttrKind::Args),
            TypeVarNonce::NONE.increment(),
        );
        for (index, (input, minimum_children)) in [
            (Type::KnownInstance(KnownInstanceType::TypeVar(first)), 0),
            (Type::TypeVar(bound), 0),
            (
                wrap(
                    &db,
                    Type::heterogeneous_tuple(
                        &db,
                        &env,
                        [
                            Type::KnownInstance(KnownInstanceType::TypeVar(first)),
                            Type::KnownInstance(KnownInstanceType::TypeVar(second)),
                            Type::TypeVar(bound),
                        ],
                    ),
                    2,
                ),
                5,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            reset(false);
            let controlled = controlled_mapping(&prepared, input, binding, &funded());
            let expected = input.apply_type_mapping(
                &db,
                &env,
                &TypeMapping::BindLegacyTypevars(binding),
                TypeContext::default(),
            );
            assert_eq!(controlled, Ok(AnalysisOutcome::Complete(expected)));
            if index == 1 {
                assert_eq!(expected, Type::TypeVar(bound));
            }
            assert_mapping(binding, minimum_children);
            assert_cleanup();
        }
        let expected_first =
            BoundTypeVarInstance::new(&db, first, binding, None, TypeVarNonce::NONE);
        let expected_second =
            BoundTypeVarInstance::new(&db, second, binding, None, TypeVarNonce::NONE);
        assert_ne!(expected_first, expected_second);
        assert_eq!(bound.binding_context(&db), original_binding);
        assert_eq!(bound.paramspec_attr(&db), Some(ParamSpecAttrKind::Args));
        assert_eq!(bound.freshness(&db), TypeVarNonce::NONE.increment());
        assert_eq!(expected_first.typevar(&db), first);
        assert_eq!(expected_second.typevar(&db), second);
        assert_eq!(expected_first.paramspec_attr(&db), None);
        assert_eq!(expected_first.freshness(&db), TypeVarNonce::NONE);
    }
    Ok(())
}

#[test]
fn nested_binding_finish_refusals_drain_before_retry() -> anyhow::Result<()> {
    for boundary in [CleanupBoundary::Finish, CleanupBoundary::Resource] {
        let db = fixture()?;
        let prepared = bound_defaults::prepare_fixture(&db);
        let binding = BindingContext::Synthetic(prepared.program_file().program(&db));
        let variable = raw(&db, TypeVarKind::LegacyTypeVar, "T");
        let input = wrap(
            &db,
            Type::KnownInstance(KnownInstanceType::TypeVar(variable)),
            3,
        );
        let revision = salsa::plumbing::current_revision(&db);
        reset(false);
        mapping_observations::reset_cleanup(boundary, 3);
        let refused = capture(&db, || {
            controlled_mapping(&prepared, input, binding, &funded())
        })
        .unwrap();
        assert_eq!(
            refused.value,
            Ok(AnalysisOutcome::Incomplete {
                reason: if boundary == CleanupBoundary::Resource {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: ()
            })
        );
        assert_mapping(binding, 3);
        let cleanup = mapping_observations::cleanup_snapshot();
        assert_eq!(cleanup.finishes, 3);
        assert_eq!((cleanup.queued, cleanup.started), (1, 0));
        assert!(!cleanup.prepared);
        assert!(!cleanup.committed);
        assert_eq!(cleanup.drop_count, 2);
        assert_eq!(
            cleanup.drops,
            [Some(CleanupDrop::Child), Some(CleanupDrop::Owner)]
        );
        assert_eq!(cleanup.child.unwrap().root, Lookup::Original);
        assert_eq!(cleanup.owner.unwrap().root, Lookup::Absent { active: 0 });
        assert_eq!(
            (cleanup.child.unwrap().active, cleanup.owner.unwrap().active),
            (Some(1), Some(0))
        );
        assert_eq!(cleanup.owner.unwrap().cache_len, 2);
        if boundary == CleanupBoundary::Resource {
            assert!(cleanup.resource.is_some_and(|bytes| bytes > 0));
        }
        assert_cleanup();
        reset(false);
        let retried = controlled_mapping(&prepared, input, binding, &funded());
        let expected = wrap(
            &db,
            Type::TypeVar(BoundTypeVarInstance::new(
                &db,
                variable,
                binding,
                None,
                TypeVarNonce::NONE,
            )),
            3,
        );
        assert_eq!(retried, Ok(AnalysisOutcome::Complete(expected)));
        assert_mapping(binding, 3);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
    Ok(())
}

#[test]
fn nested_binding_native_cancellation_drains_and_retries() -> anyhow::Result<()> {
    let db = fixture()?;
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let binding = BindingContext::Definition(bound_defaults::selected_definition(&prepared));
    let variable = raw(&db, TypeVarKind::LegacyTypeVar, "T");
    let unbound = Type::KnownInstance(KnownInstanceType::TypeVar(variable));
    let input = Type::heterogeneous_tuple(&db, &env, [unbound, wrap(&db, unbound, 4)]);
    let revision = salsa::plumbing::current_revision(&db);
    reset(false);
    mapping_observations::reset(Some(3));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_mapping(&prepared, input, binding, &funded())
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert_mapping(binding, 3);
    let tuple = mapping_observations::tuple_snapshot();
    assert_eq!(tuple.push_count, 1);
    assert_eq!(tuple.partial_drops, 1);
    assert_eq!(tuple.last_dropped_len, Some(1));
    assert_cleanup();
    reset(false);
    let retried = controlled_mapping(&prepared, input, binding, &funded());
    let bound = Type::TypeVar(BoundTypeVarInstance::new(
        &db,
        variable,
        binding,
        None,
        TypeVarNonce::NONE,
    ));
    let expected = Type::heterogeneous_tuple(&db, &env, [bound, wrap(&db, bound, 4)]);
    assert_eq!(retried, Ok(AnalysisOutcome::Complete(expected)));
    assert_mapping(binding, 6);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
    Ok(())
}

/// Subclass mapping retains its unsupported continuation after nominal default binding is enabled.
#[test]
fn unsupported_binding_continuation_retains_its_mapping_operation() -> anyhow::Result<()> {
    let db = fixture()?;
    let prepared = bound_defaults::prepare_fixture(&db);
    let binding = BindingContext::Synthetic(prepared.program_file().program(&db));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = KnownClass::Int.to_subclass_of(&db, &env);
    reset(false);
    assert_eq!(
        controlled_mapping(&prepared, input, binding, &funded()),
        Ok(unavailable(OperationId::LegacyTypeVarBinding(
            MaterializationOperation::LegacyContinuation
        )))
    );
    assert_mapping(binding, 0);
    assert_cleanup();
    Ok(())
}

mod callables;
