use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;
use salsa::execution_probe::FinalSourceMemo;
use salsa::plumbing::ZalsaDatabase;

use super::*;
use crate::types::generics::defaults::default_specialization_with_effects;
use crate::types::generics::prefix::TypeArgumentPrefix;
use crate::types::mapping::source::observations as mapping_observations;
use crate::types::mapping::source::observations::{
    CleanupBoundary, CleanupDrop, Lookup, OwnedMappingSnapshot,
};
use crate::types::typevar::{
    ParamSpecAttrKind, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarNonce,
    bound_typevar_default_ingredient,
};
use crate::types::{
    ApplySpecialization, BindingContext, MappingOperation, MaterializationOperation, TypeMapping,
};

fn fixture(chained: bool) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.pyi",
        &format!(
            "from typing import Generic, TypeVar\n\
             T = TypeVar(\"T\")\n\
             U = TypeVar(\"U\", default=T)\n\
             {}\
             class Product(Generic[T, U{}]): ...\n\
             class Leaf: ...\n\
             left = right = Product[Leaf]\n",
            if chained {
                "V = TypeVar(\"V\", default=U)\n"
            } else {
                ""
            },
            if chained { ", V" } else { "" },
        ),
    )
    .unwrap();
    db
}

fn reset() {
    observations::reset(None);
    mapping_observations::reset(None);
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_eq!(mapping_observations::tuple_snapshot().live, 0);
    assert_eq!(mapping_observations::set_snapshot().live, 0);
    assert_no_active_attempt();
}

fn class_definition<'db>(prepared: &PreparedAnalysisFile<'db>, name: &str) -> Definition<'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == name)
        .unwrap();
    prepared.semantic_index().expect_single_definition(class)
}

fn class<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> StaticClassLiteral<'db> {
    let definition = class_definition(prepared, name);
    let Some(ClassLiteral::Static(class)) =
        infer_definition_types(db, definition).original_class_type(definition)
    else {
        panic!("fixture static class {name}");
    };
    class
}

fn assert_alias<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    alias: GenericAlias<'db>,
    arity: usize,
) {
    let origin = class(db, prepared, "Product");
    let leaf = class(db, prepared, "Leaf");
    let context = origin.generic_context(db).unwrap();
    let specialization = alias.specialization(db);
    let arguments = specialization.types(db);
    assert_eq!(arguments.len(), arity);
    assert!(arguments.iter().all(|argument| *argument == arguments[0]));
    let Type::NominalInstance(instance) = arguments[0] else {
        panic!(
            "defaulted argument is not a Leaf instance: {:?}",
            arguments[0]
        );
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(instance.class_literal(db, &env), ClassLiteral::Static(leaf));
    assert!(!instance.inherits_from_explicit_any());
    assert_eq!(specialization.generic_context(db), context);
    assert_eq!(specialization.materialization_kind(db), None);
    assert_eq!(specialization.tuple(db), None);
    assert_eq!(
        specialization,
        Specialization::new(
            db,
            context,
            vec![arguments[0]; arity].into_boxed_slice(),
            None,
            None
        ),
    );
    assert_eq!(alias, GenericAlias::new(db, origin, specialization));
    let variables = context.variables(db).collect::<Vec<_>>();
    for index in 1..arity {
        assert_eq!(
            variables[index].default_type(db),
            Some(Type::TypeVar(variables[index - 1]))
        );
        assert!(
            FinalSourceMemo::certify(
                db as &dyn Db,
                bound_typevar_default_ingredient(db),
                variables[index].as_id(),
            )
            .is_ok()
        );
    }
}

#[test]
fn cold_partial_defaults_publish_the_complete_canonical_specialization() {
    for chained in [false, true] {
        let db = fixture(chained);
        let prepared = bound_defaults::prepare_fixture(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let expression = prepared
            .semantic_index()
            .expression(expression_key(&prepared));
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset();
        let captured = capture(&db, || {
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete(Type::GenericAlias(alias))) = captured.value else {
            panic!("cold partial defaults: {:?}", captured.value);
        };
        assert_eq!(captured.check_root_reads(), Ok(()));
        assert_cleanup();
        let arity = if chained { 3 } else { 2 };
        let mappings = mapping_observations::mapping_snapshot();
        let partials = mappings.roots[..mappings.root_count]
            .iter()
            .flatten()
            .filter_map(|root| match root.mapping {
                OwnedMappingSnapshot::Partial {
                    generic_context,
                    owner,
                    len,
                    skip,
                } => {
                    assert!(root.default_context);
                    assert_eq!(skip, None);
                    Some((generic_context, owner, len, root.visitor))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(partials.len(), arity - 1);
        for (index, partial) in partials.iter().enumerate() {
            assert_eq!(partial.0, partials[0].0);
            assert_eq!(partial.1, partials[0].1);
            assert_ne!(partial.1, 0);
            assert_eq!(partial.2, index + 1);
            if index > 0 {
                assert_ne!(partial.3, partials[index - 1].3);
            }
        }
        assert_alias(&db, &prepared, alias, arity);
        let canonical = infer_expression_types(&db, expression, TypeContext::default());
        assert_eq!(
            canonical.expression_type(expression_key(&prepared)),
            Type::GenericAlias(alias)
        );
        events_db.take_salsa_events();
        reset();
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::GenericAlias(alias))),
        );
        assert!(std::ptr::eq(
            canonical,
            infer_expression_types(&db, expression, TypeContext::default())
        ));
        assert_eq!(mapping_observations::mapping_snapshot().root_count, 0);
        let events = events_db.take_salsa_events();
        for query in [
            "infer_expression_types_impl",
            "infer_definition_types",
            "static_class_generic_context",
            "bound_typevar_default_type",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();

        let ordinary_db = fixture(chained);
        let ordinary_prepared = bound_defaults::prepare_fixture(&ordinary_db);
        let ordinary_expression = ordinary_prepared
            .semantic_index()
            .expression(expression_key(&ordinary_prepared));
        let ordinary =
            infer_expression_types(&ordinary_db, ordinary_expression, TypeContext::default());
        let Type::GenericAlias(ordinary_alias) =
            ordinary.expression_type(expression_key(&ordinary_prepared))
        else {
            panic!("ordinary fixture generic alias");
        };
        assert_alias(&ordinary_db, &ordinary_prepared, ordinary_alias, arity);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
        assert_eq!(
            Type::GenericAlias(alias).display(&db, &env).to_string(),
            Type::GenericAlias(ordinary_alias)
                .display(&ordinary_db, &ordinary_env)
                .to_string()
        );
    }
}

#[derive(Clone, Copy)]
enum Action<'args, 'db> {
    Map {
        ty: Type<'db>,
        context: GenericContext<'db>,
        prefix: &'args [Type<'db>],
        skip: Option<usize>,
    },
    Defaults(GenericContext<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Output<'db> {
    Mapped(Type<'db>),
    Defaults(Specialization<'db>),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    action: Action<'_, 'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Output<'db>>, AnalysisFailure> {
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
                let effects = SourceEffects::new(&access, session.program());
                match action {
                    Action::Map {
                        ty,
                        context,
                        prefix,
                        skip,
                    } => {
                        let mut buffer = access
                            .resources()
                            .default_arguments(&access.endpoint, prefix.len())
                            .await?;
                        for &ty in prefix {
                            effects
                                .local_with_fixed_transfers(3, 0, || {
                                    buffer.append(ty).map_err(|_| {
                                        RunError::Contract(
                                            "fixture argument buffer rejected an append",
                                        )
                                    })
                                })
                                .await??;
                        }
                        let prefix = effects
                            .local_with_fixed_transfers(2, 0, || buffer.prefix())
                            .await?;
                        let env = ProgramEnvironment::from_file(prepared.program_file());
                        effects
                            .apply_partial_specialization(ty, &env, context, prefix, skip)
                            .await
                            .map(Output::Mapped)
                    }
                    Action::Defaults(context) => {
                        default_specialization_with_effects(session.db(), context, None, &effects)
                            .await
                            .map(Output::Defaults)
                    }
                }
            })
        }));
        // The argument owner remains retained after all descendant tasks have drained.
        assert_eq!(default_arguments.retained_payload().unwrap().0, 1);
        assert_eq!(observations::counts().0, 0);
        assert_eq!(mapping_observations::tuple_snapshot().live, 0);
        assert_eq!(mapping_observations::set_snapshot().live, 0);
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

fn variable<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    name: &'static str,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::synthetic(db, env, Name::new_static(name), TypeVarVariance::Invariant)
}

fn wrap<'db>(db: &'db dyn Db, mut ty: Type<'db>, depth: usize) -> Type<'db> {
    for _ in 0..depth {
        ty = TypeFormType::from_type_expression(db, ty);
    }
    ty
}

fn ordinary_mapping<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    action: Action<'_, 'db>,
) -> Type<'db> {
    let Action::Map {
        ty,
        context,
        prefix,
        skip,
    } = action
    else {
        panic!("mapping fixture action");
    };
    ty.apply_type_mapping(
        db,
        env,
        &TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: TypeArgumentPrefix::Borrowed(prefix),
            skip,
        }),
        TypeContext::default(),
    )
}

fn assert_mapping(
    context: GenericContext<'_>,
    len: usize,
    skip: Option<usize>,
    minimum_children: usize,
) {
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1);
    let root = snapshot.roots[0].unwrap();
    let OwnedMappingSnapshot::Partial {
        generic_context,
        owner,
        len: actual_len,
        skip: actual_skip,
    } = root.mapping
    else {
        panic!("expected retained partial mapping: {root:?}");
    };
    assert_eq!(generic_context, context.as_id());
    assert_ne!(owner, 0);
    assert_eq!(actual_len, len);
    assert_eq!(actual_skip, skip);
    assert!(root.default_context);
    assert!(snapshot.child_count >= minimum_children);
    for child in snapshot.children[..snapshot.child_count].iter().flatten() {
        assert_eq!(child.mapping, root.mapping);
        assert_eq!(child.visitor, root.visitor);
        assert!(child.default_context);
    }
}

#[test]
fn partial_lookup_preserves_logical_identity_and_unfilled_variables() {
    let db = fixture(false);
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = variable(&db, &env, "A");
    let second = variable(&db, &env, "B");
    let foreign = BoundTypeVarInstance::new(
        &db,
        first.typevar(&db),
        BindingContext::Definition(class_definition(&prepared, "Product")),
        None,
        TypeVarNonce::NONE,
    );
    let fresh = BoundTypeVarInstance::new(
        &db,
        first.typevar(&db),
        first.binding_context(&db),
        None,
        TypeVarNonce::NONE.increment(),
    );
    let retained_self = BoundTypeVarInstance::synthetic_self(
        &db,
        Type::TypeVar(first),
        BindingContext::Synthetic(env.program(&db)),
    );
    let altered = eager_default(&db, first, Type::bool_literal(false));
    assert_ne!(altered, first);
    assert_eq!(altered.identity(&db), first.identity(&db));
    let context = GenericContext::from_typevar_instances(&db, &env, [first, second]);
    let replacement = Type::bool_literal(true);
    for (input, prefix, skip, expected) in [
        (first, vec![replacement], None, replacement),
        (altered, vec![replacement], None, replacement),
        (foreign, vec![replacement], None, Type::TypeVar(foreign)),
        (fresh, vec![replacement], None, Type::TypeVar(fresh)),
        (
            retained_self,
            vec![replacement],
            None,
            Type::TypeVar(retained_self),
        ),
        (second, vec![replacement], None, Type::TypeVar(second)),
        (first, vec![], None, Type::TypeVar(first)),
        (first, vec![], Some(0), Type::Never),
        (
            first,
            vec![Type::TypeVar(second), replacement],
            None,
            Type::TypeVar(second),
        ),
    ] {
        reset();
        let action = Action::Map {
            ty: Type::TypeVar(input),
            context,
            prefix: &prefix,
            skip,
        };
        let captured = capture(&db, || controlled(&prepared, action, &funded())).unwrap();
        assert_eq!(
            captured.value,
            Ok(AnalysisOutcome::Complete(Output::Mapped(expected)))
        );
        assert!(captured.reads.is_empty());
        assert_mapping(context, prefix.len(), skip, 0);
        assert_eq!(ordinary_mapping(&db, &env, action), expected);
        assert_cleanup();
    }
}

#[test]
fn nested_partial_mapping_reuses_the_prefix_and_transfers_replacements_unchanged() {
    let db = fixture(false);
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = variable(&db, &env, "A");
    let second = variable(&db, &env, "B");
    let context = GenericContext::from_typevar_instances(&db, &env, [first, second]);
    let input = wrap(
        &db,
        Type::heterogeneous_tuple(
            &db,
            &env,
            [
                Type::TypeVar(first),
                wrap(&db, Type::TypeVar(first), 2),
                Type::TypeVar(second),
            ],
        ),
        2,
    );
    let prefix = [Type::TypeVar(second), Type::bool_literal(true)];
    let expected = wrap(
        &db,
        Type::heterogeneous_tuple(
            &db,
            &env,
            [
                Type::TypeVar(second),
                wrap(&db, Type::TypeVar(second), 2),
                Type::bool_literal(true),
            ],
        ),
        2,
    );
    let action = Action::Map {
        ty: input,
        context,
        prefix: &prefix,
        skip: None,
    };
    reset();
    let captured = capture(&db, || controlled(&prepared, action, &funded())).unwrap();
    assert_eq!(
        captured.value,
        Ok(AnalysisOutcome::Complete(Output::Mapped(expected)))
    );
    assert!(captured.reads.is_empty());
    assert_mapping(context, 2, None, 7);
    assert_eq!(ordinary_mapping(&db, &env, action), expected);
    assert_cleanup();
}

fn eager_default<'db>(
    db: &'db TestDb,
    variable: BoundTypeVarInstance<'db>,
    default: Type<'db>,
) -> BoundTypeVarInstance<'db> {
    let typevar = variable.typevar(db);
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            typevar.identity(db),
            None,
            typevar.explicit_variance(db),
            Some(TypeVarDefaultEvaluation::Eager(default)),
        ),
        variable.binding_context(db),
        variable.paramspec_attr(db),
        variable.freshness(db),
    )
}

fn nested_default_context<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> (GenericContext<'db>, [BoundTypeVarInstance<'db>; 2]) {
    let context = class(db, prepared, "Product").generic_context(db).unwrap();
    let variables = context.variables(db).collect::<Vec<_>>();
    let first = eager_default(db, variables[0], Type::bool_literal(true));
    let second = eager_default(db, variables[1], wrap(db, Type::TypeVar(first), 3));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    (
        GenericContext::from_typevar_instances(db, &env, [first, second]),
        [first, second],
    )
}

fn has_specialization<'db>(db: &'db TestDb, context: GenericContext<'db>) -> bool {
    Specialization::ingredient(db.zalsa())
        .entries(db.zalsa())
        .any(|entry| entry.value().fields().0 == context)
}

#[test]
fn default_mapping_refusals_drain_before_publication_and_reuse_completed_defaults() {
    for boundary in [CleanupBoundary::Finish, CleanupBoundary::Resource] {
        let db = fixture(false);
        let prepared = bound_defaults::prepare_fixture(&db);
        let (context, variables) = nested_default_context(&db, &prepared);
        let revision = salsa::plumbing::current_revision(&db);
        assert!(!has_specialization(&db, context));
        reset();
        // Binding finishes three wrappers; partial specialization then finishes the same three.
        mapping_observations::reset_cleanup(boundary, 6);
        let refused = capture(&db, || {
            controlled(&prepared, Action::Defaults(context), &funded())
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
                completed: (),
            })
        );
        let cleanup = mapping_observations::cleanup_snapshot();
        assert_eq!(cleanup.finishes, 6);
        assert_eq!((cleanup.queued, cleanup.started), (1, 0));
        assert!(!cleanup.prepared);
        assert!(!cleanup.committed);
        assert_eq!(
            cleanup.drops,
            [Some(CleanupDrop::Child), Some(CleanupDrop::Owner)]
        );
        assert_eq!(cleanup.child.unwrap().root, Lookup::Original);
        assert_eq!(cleanup.owner.unwrap().root, Lookup::Absent { active: 0 });
        assert!(!has_specialization(&db, context));
        for variable in variables {
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    bound_typevar_default_ingredient(&db),
                    variable.as_id()
                )
                .is_ok()
            );
        }
        assert_cleanup();

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        reset();
        let retried = controlled(&prepared, Action::Defaults(context), &funded());
        let expected = Specialization::new(
            &db,
            context,
            Box::from([
                Type::bool_literal(true),
                wrap(&db, Type::bool_literal(true), 3),
            ]),
            None,
            None,
        );
        assert_eq!(
            retried,
            Ok(AnalysisOutcome::Complete(Output::Defaults(expected)))
        );
        assert_eq!(context.default_specialization(&db, None), expected);
        let events = events_db.take_salsa_events();
        for variable in variables {
            assert!(!bound_defaults::query_ran(&db, variable.as_id(), &events));
        }
        let mappings = mapping_observations::mapping_snapshot();
        assert_eq!(mappings.root_count, 2);
        assert!(
            mappings.roots[..2]
                .iter()
                .flatten()
                .all(|root| matches!(root.mapping, OwnedMappingSnapshot::Partial { .. }))
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[test]
fn native_cancellation_with_a_default_mapping_child_active_drains_and_retries() {
    let db = fixture(false);
    let prepared = bound_defaults::prepare_fixture(&db);
    let (context, variables) = nested_default_context(&db, &prepared);
    let revision = salsa::plumbing::current_revision(&db);
    reset();
    // The bound default's three binding children complete before partial mapping begins.
    mapping_observations::reset(Some(5));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Action::Defaults(context), &funded())
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    let mappings = mapping_observations::mapping_snapshot();
    assert!(mappings.child_count >= 5);
    assert!(matches!(
        mappings.children[4].unwrap().mapping,
        OwnedMappingSnapshot::Partial { .. }
    ));
    assert!(!has_specialization(&db, context));
    for variable in variables {
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                bound_typevar_default_ingredient(&db),
                variable.as_id()
            )
            .is_ok()
        );
    }
    assert_cleanup();
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    reset();
    let retried = controlled(&prepared, Action::Defaults(context), &funded());
    let expected = Specialization::new(
        &db,
        context,
        Box::from([
            Type::bool_literal(true),
            wrap(&db, Type::bool_literal(true), 3),
        ]),
        None,
        None,
    );
    assert_eq!(
        retried,
        Ok(AnalysisOutcome::Complete(Output::Defaults(expected)))
    );
    let events = events_db.take_salsa_events();
    for variable in variables {
        assert!(!bound_defaults::query_ran(&db, variable.as_id(), &events));
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

#[test]
fn unsupported_paramspec_attributes_keep_the_precise_partial_mapping_boundary() {
    let db = fixture(false);
    let prepared = bound_defaults::prepare_fixture(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let raw = TypeVarInstance::new(
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
    );
    let binding = BindingContext::Synthetic(env.program(&db));
    let variable = BoundTypeVarInstance::new(&db, raw, binding, None, TypeVarNonce::NONE);
    let replacement_raw = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(
            &db,
            Name::new_static("Q"),
            None,
            TypeVarKind::LegacyParamSpec,
        ),
        None,
        None,
        None,
    );
    let replacement =
        BoundTypeVarInstance::new(&db, replacement_raw, binding, None, TypeVarNonce::NONE);
    let context = GenericContext::from_typevar_instances(&db, &env, [variable]);
    let prefix = [Type::TypeVar(replacement)];
    for attr in [ParamSpecAttrKind::Args, ParamSpecAttrKind::Kwargs] {
        let input = variable.with_paramspec_attr(&db, attr);
        let expected = replacement.with_paramspec_attr(&db, attr);
        let action = Action::Map {
            ty: Type::TypeVar(input),
            context,
            prefix: &prefix,
            skip: None,
        };
        reset();
        assert_eq!(
            controlled(&prepared, action, &funded()),
            Ok(unavailable(OperationId::Specialization(
                MaterializationOperation::Leaf(MappingOperation::ParamSpec)
            )))
        );
        assert_eq!(ordinary_mapping(&db, &env, action), Type::TypeVar(expected));
        assert_mapping(context, 1, None, 0);
        assert_cleanup();
    }
}

mod callables;
