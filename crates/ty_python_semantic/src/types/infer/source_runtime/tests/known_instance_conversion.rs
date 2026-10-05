use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use salsa::execution_probe::FinalSourceMemo;
use ty_python_core::scope::ScopeId;

use super::*;
use crate::types::infer::InferenceFlags;
use crate::types::known_instance::{InternedType, UnionTypeInstance};
use crate::types::type_expression_conversion::TypeConversionOperation;
use crate::types::typevar::TypeVarNonce;
use crate::types::{
    BindingContext, BoundTypeVarInstance, CallableType, InvalidTypeExpression,
    InvalidTypeExpressionError, KnownInstanceType,
};

#[derive(Clone, Copy)]
enum Input<'db> {
    Known(KnownInstanceType<'db>),
    Expression(Expression<'db>, ExpressionNodeKey),
}

#[derive(Default)]
struct Progress {
    remaining: Cell<Option<usize>>,
    pools: Cell<Option<[usize; 5]>>,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    input: Input<'db>,
    scope: ScopeId<'db>,
    binding: Option<Definition<'db>>,
    pressure: Option<ExecutionLimits>,
    policy: &AnalysisPolicy,
    progress: &Progress,
) -> Result<AnalysisOutcome<Result<Type<'db>, InvalidTypeExpressionError<'db>>>, AnalysisFailure> {
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
                let ty = match input {
                    Input::Known(known) => Type::KnownInstance(known),
                    Input::Expression(expression, key) => {
                        let inference = access
                            .expression(expression, TypeContext::default())
                            .await?;
                        access
                            .endpoint
                            .local_call(|| {
                                access.endpoint.admit_work(2)?;
                                access.endpoint.check_completion()?;
                                Ok(inference.expression_type(key))
                            })
                            .await
                    }
                };
                access
                    .endpoint
                    .local_call(|| {
                        progress.remaining.set(
                            salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db()),
                        );
                        if let Some(pressure) = pressure {
                            access.endpoint.admit_work(pressure.semantic_work)?;
                            access.endpoint.admit(ExecutionWork::Resource {
                                requested_bytes: pressure.requested_bytes,
                            })?;
                        }
                        access.endpoint.check_completion()
                    })
                    .await;
                let effects = SourceEffects::new(&access, session.program());
                ty.in_type_expression_with(
                    session.db(),
                    scope,
                    binding,
                    InferenceFlags::empty(),
                    &effects,
                )
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

fn variable_fixture() -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.py",
        "from typing import Generic, TypeVar\n\
         T = TypeVar(\"T\")\n\
         class Box(Generic[T]): ...\n\
         left = right = T\n",
    )
    .unwrap();
    db
}

fn selected_input<'db>(prepared: &PreparedAnalysisFile<'db>) -> Input<'db> {
    let key = expression_key(prepared);
    Input::Expression(prepared.semantic_index().expression(key), key)
}

fn assert_cleanup(progress: &Progress) {
    assert!(progress.pools.get().is_some());
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn known_instance_conversion_preserves_handles_inner_values_and_semantic_errors() {
    let db = fixture();
    let prepared = prepare(&db);
    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let callable = CallableType::unknown(&db);
    let inner = InternedType::new(&db, Type::Never);
    let invalid = KnownInstanceType::Range { is_non_empty: true };
    let cases = [
        (
            KnownInstanceType::Callable(callable),
            Ok(Type::Callable(callable)),
        ),
        (KnownInstanceType::Literal(inner), Ok(Type::Never)),
        (KnownInstanceType::Annotated(inner), Ok(Type::Never)),
        (
            KnownInstanceType::LiteralStringAlias(inner),
            Ok(Type::Never),
        ),
        (
            invalid,
            Err(InvalidTypeExpressionError {
                fallback_type: Type::unknown(),
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::InvalidType(Type::KnownInstance(invalid), scope,)
                ],
            }),
        ),
    ];
    for (known, expected) in cases {
        observations::reset(None);
        let progress = Progress::default();
        let result = capture(&db, || {
            controlled(
                &prepared,
                Input::Known(known),
                scope,
                None,
                None,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        assert_eq!(
            result.value,
            Ok(AnalysisOutcome::Complete(expected.clone()))
        );
        assert!(result.reads.is_empty());
        assert_eq!(progress.pools.get(), Some([0; 5]));
        assert_cleanup(&progress);
        assert_eq!(
            Type::KnownInstance(known).in_type_expression(
                &db,
                scope,
                None,
                InferenceFlags::empty()
            ),
            expected,
        );
    }
}

#[test]
fn known_instance_conversion_refuses_stored_unions_and_recursive_metatypes_precisely() {
    let db = fixture();
    let prepared = prepare(&db);
    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let union = UnionTypeInstance::new(&db, None, Ok(Type::Never));
    let inner = InternedType::new(&db, Type::Never);
    let revision = salsa::plumbing::current_revision(&db);
    for (known, operation) in [
        (
            KnownInstanceType::UnionType(union),
            TypeConversionOperation::KnownInstanceUnionResult,
        ),
        (
            KnownInstanceType::TypeGenericAlias(inner),
            TypeConversionOperation::KnownInstanceMetaType,
        ),
    ] {
        for _ in 0..2 {
            observations::reset(None);
            let progress = Progress::default();
            assert_eq!(
                controlled(
                    &prepared,
                    Input::Known(known),
                    scope,
                    None,
                    None,
                    &funded(),
                    &progress,
                ),
                Ok(unavailable(OperationId::TypeConversion(operation)))
            );
            assert_eq!(progress.pools.get(), Some([0; 5]));
            assert_cleanup(&progress);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}

#[test]
fn raw_typevar_conversion_uses_canonical_binding_and_preserves_unbound_identity() {
    let db = variable_fixture();
    let prepared = prepare(&db);
    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let input = selected_input(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let progress = Progress::default();
    let unbound = capture(&db, || {
        controlled(&prepared, input, scope, None, None, &funded(), &progress)
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete(Ok(Type::KnownInstance(KnownInstanceType::TypeVar(raw))))) =
        unbound.value
    else {
        panic!("raw TypeVar conversion: {:?}", unbound.value);
    };
    assert_eq!(unbound.check_root_reads(), Ok(()));
    assert_cleanup(&progress);
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(Stmt::as_class_def_stmt)
        .unwrap();
    let binding = prepared.semantic_index().expect_single_definition(class);
    let progress = Progress::default();
    let converted = controlled(
        &prepared,
        input,
        scope,
        Some(binding),
        None,
        &funded(),
        &progress,
    );
    let Ok(AnalysisOutcome::Complete(Ok(Type::TypeVar(bound)))) = converted else {
        panic!("bound TypeVar conversion: {converted:?}");
    };
    assert_cleanup(&progress);
    assert_eq!(bound.typevar(&db), raw);
    assert_eq!(
        bound.binding_context(&db),
        BindingContext::Definition(binding)
    );
    assert_eq!(bound.paramspec_attr(&db), None);
    assert_eq!(bound.freshness(&db), TypeVarNonce::NONE);
    assert_eq!(
        bound,
        BoundTypeVarInstance::new(
            &db,
            raw,
            BindingContext::Definition(binding),
            None,
            TypeVarNonce::NONE,
        )
    );
    let ingredient = expression_inference_ingredient(&db);
    let key = expression_key(&prepared);
    let expression = prepared.semantic_index().expression(key);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            ingredient,
            InferExpression::Bare(expression).as_id(),
        )
        .is_ok()
    );
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    assert_eq!(
        infer_expression_types(&db, expression, TypeContext::default()).expression_type(key),
        Type::KnownInstance(KnownInstanceType::TypeVar(raw))
    );
    assert_eq!(
        Type::KnownInstance(KnownInstanceType::TypeVar(raw)).in_type_expression(
            &db,
            scope,
            Some(binding),
            InferenceFlags::empty(),
        ),
        Ok(Type::TypeVar(bound))
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_expression_types_impl",
        None,
        &events_db.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn conversion_pressure_retires_input_owners_and_retries_at_the_same_revision() {
    for (pressure, reason) in [
        (
            ExecutionLimits {
                semantic_work: funded().semantic_work_limit,
                requested_bytes: 0,
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            ExecutionLimits {
                semantic_work: 0,
                requested_bytes: funded().requested_bytes_limit,
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        let db = variable_fixture();
        let prepared = prepare(&db);
        let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
        let input = selected_input(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                input,
                scope,
                None,
                Some(pressure),
                &funded(),
                &progress,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
        assert!(progress.remaining.get().is_some());
        assert!(observations::counts().1 > 0);
        assert_cleanup(&progress);
        let expression = prepared
            .semantic_index()
            .expression(expression_key(&prepared));
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                expression_inference_ingredient(&db),
                InferExpression::Bare(expression).as_id(),
            )
            .is_ok()
        );
        observations::reset(None);
        let progress = Progress::default();
        let retry = controlled(&prepared, input, scope, None, None, &funded(), &progress);
        assert!(
            matches!(
                retry,
                Ok(AnalysisOutcome::Complete(Ok(Type::KnownInstance(
                    KnownInstanceType::TypeVar(_)
                ))))
            ),
            "{retry:?}"
        );
        assert_eq!(progress.pools.get(), Some([0; 5]));
        assert_eq!(observations::counts(), (0, 0, 0));
        assert_cleanup(&progress);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn parent_fixture() -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.py",
        "from typing import Generic, TypeVar\n\
         T = TypeVar(\"T\")\n\
         class Parent(Generic[T]): ...\n\
         class Child(Parent[T]): ...\n\
         class Leaf: ...\n\
         left = right = Child[Leaf]\n",
    )
    .unwrap();
    db
}

fn assert_parent_identity<'db>(db: &'db TestDb, alias: GenericAlias<'db>) {
    let child = alias.origin(db);
    assert_eq!(child.name(db), "Child");
    let context = alias.specialization(db).generic_context(db);
    assert_eq!(child.generic_context(db), Some(context));
    let variables: Vec<_> = context.variables(db).collect();
    let [variable] = variables.as_slice() else {
        panic!("Child has one generic parameter");
    };
    assert_eq!(
        variable.binding_context(db),
        BindingContext::Definition(child.definition(db))
    );
    let bases = child.explicit_bases(db);
    let [Type::GenericAlias(parent)] = bases.as_ref() else {
        panic!("Child has one specialized parent: {bases:?}");
    };
    assert_eq!(parent.origin(db).name(db), "Parent");
    assert_eq!(
        parent.specialization(db).types(db),
        &[Type::TypeVar(*variable)]
    );
    let parent_variable = parent
        .specialization(db)
        .generic_context(db)
        .variables(db)
        .next()
        .unwrap();
    assert_eq!(parent_variable.typevar(db), variable.typevar(db));
    assert_ne!(parent_variable.identity(db), variable.identity(db));
    assert_eq!(
        parent_variable.binding_context(db),
        BindingContext::Definition(parent.origin(db).definition(db))
    );
}

#[test]
fn cold_generic_parent_preserves_bound_identity_and_matches_independent_ordinary_inference() {
    let ordinary_db = parent_fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_key = expression_key(&ordinary_prepared);
    let ordinary_expression = ordinary_prepared.semantic_index().expression(ordinary_key);
    let ordinary =
        infer_expression_types(&ordinary_db, ordinary_expression, TypeContext::default())
            .expression_type(ordinary_key);
    let Type::GenericAlias(ordinary_alias) = ordinary else {
        panic!("ordinary generic parent: {ordinary:?}");
    };
    assert_parent_identity(&ordinary_db, ordinary_alias);

    let db = parent_fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let key = expression_key(&prepared);
    let expression = prepared.semantic_index().expression(key);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let mut completed = None;
    for _ in 0..4 {
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, key, &funded());
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        match result {
            Ok(AnalysisOutcome::Complete(Type::GenericAlias(alias))) => {
                completed = Some(alias);
                break;
            }
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }) => {}
            other => panic!("cold generic parent: {other:?}"),
        }
    }
    let alias = completed.expect("generic parent completes within four funded caller attempts");
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            InferExpression::Bare(expression).as_id(),
        )
        .is_ok()
    );
    let context = static_class_generic_context_ingredient(&db);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, context, alias.origin(&db).as_id()).is_ok());
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            explicit_bases_ingredient(&db),
            alias.origin(&db).as_id(),
        )
        .is_ok()
    );
    assert!(
        find_will_execute_event_by_name(
            &db,
            "static_class_generic_context",
            None,
            &events_db.take_salsa_events(),
        )
        .is_some()
    );
    assert_parent_identity(&db, alias);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    assert_eq!(
        Type::GenericAlias(alias).display(&db, &env).to_string(),
        ordinary.display(&ordinary_db, &ordinary_env).to_string()
    );
    events_db.take_salsa_events();
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(&prepared, key, &funded()),
        Ok(AnalysisOutcome::Complete(Type::GenericAlias(alias)))
    );
    assert_eq!(observations::counts(), (0, 0, 0));
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_expression_types_impl",
        None,
        &events_db.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
