use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use salsa::execution_probe::FinalSourceMemo;
use ty_python_core::scope::ScopeId;

use super::*;
use crate::types::infer::InferenceFlags;
use crate::types::special_form::TypeQualifier;
use crate::types::type_expression_conversion::TypeConversionOperation;
use crate::types::{
    DynamicType, InvalidTypeExpression, InvalidTypeExpressionError, SpecialFormType, TypingModule,
};

#[derive(Clone, Copy)]
enum Input<'db> {
    SpecialForm(SpecialFormType),
    Expression(Expression<'db>, ExpressionNodeKey),
}

#[derive(Clone, Copy, Default)]
enum Interruption {
    #[default]
    None,
    WorkRemaining(usize),
    Allocation,
    CancelBeforeConversion,
    CancelAfterConversion,
}

#[derive(Default)]
struct Progress {
    remaining: Cell<Option<usize>>,
    pools: Cell<Option<[usize; 5]>>,
    entered: Cell<bool>,
    completed: Cell<bool>,
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    scope: ScopeId<'db>,
    input: Input<'db>,
    flags: InferenceFlags,
    interruption: Interruption,
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
                    Input::SpecialForm(form) => Type::SpecialForm(form),
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
                        let remaining =
                            salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db());
                        progress.remaining.set(remaining);
                        match interruption {
                            Interruption::WorkRemaining(keep) => {
                                let remaining = remaining.ok_or(RunError::Contract(
                                    "conversion runs inside an active attempt",
                                ))?;
                                access.endpoint.admit_work(remaining.saturating_sub(keep))?;
                            }
                            Interruption::Allocation => {
                                access.endpoint.admit(ExecutionWork::Resource {
                                    requested_bytes: policy.requested_bytes_limit,
                                })?;
                            }
                            Interruption::CancelBeforeConversion => {
                                session.db().cancellation_token().cancel();
                            }
                            Interruption::None | Interruption::CancelAfterConversion => {}
                        }
                        access.endpoint.check_completion()
                    })
                    .await;
                progress.entered.set(true);
                let effects = SourceEffects::new(&access, session.program());
                let converted = ty
                    .in_type_expression_with(session.db(), scope, None, flags, &effects)
                    .await?;
                progress.completed.set(true);
                if matches!(interruption, Interruption::CancelAfterConversion) {
                    access
                        .endpoint
                        .local_call(|| {
                            session.db().cancellation_token().cancel();
                            access.endpoint.check_completion()
                        })
                        .await;
                }
                Ok(converted)
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

fn assert_cleanup(progress: &Progress) {
    assert!(progress.pools.get().is_some());
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn selected_input<'db>(prepared: &PreparedAnalysisFile<'db>) -> Input<'db> {
    let key = expression_key(prepared);
    Input::Expression(prepared.semantic_index().expression(key), key)
}

fn error<'db>(
    invalid: InvalidTypeExpression<'db>,
    fallback_type: Type<'db>,
) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
    Err(InvalidTypeExpressionError {
        fallback_type,
        invalid_expressions: smallvec::smallvec_inline![invalid],
    })
}

#[test]
fn finite_special_forms_preserve_values_and_exact_semantic_errors() {
    let db = fixture();
    let prepared = prepare(&db);

    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let mut cases = vec![
        (SpecialFormType::Never, Ok(Type::Never)),
        (SpecialFormType::NoReturn, Ok(Type::Never)),
        (SpecialFormType::LiteralString, Ok(Type::literal_string())),
        (SpecialFormType::Any, Ok(Type::any())),
        (SpecialFormType::Unknown, Ok(Type::unknown())),
        (SpecialFormType::AlwaysTruthy, Ok(Type::AlwaysTruthy)),
        (SpecialFormType::AlwaysFalsy, Ok(Type::AlwaysFalsy)),
        (
            SpecialFormType::TypeAlias,
            error(InvalidTypeExpression::TypeAlias, Type::unknown()),
        ),
        (
            SpecialFormType::Protocol,
            error(InvalidTypeExpression::Protocol, Type::unknown()),
        ),
        (
            SpecialFormType::Generic,
            error(InvalidTypeExpression::Generic, Type::unknown()),
        ),
        (
            SpecialFormType::Annotated,
            error(
                InvalidTypeExpression::RequiresTwoArguments(SpecialFormType::Annotated),
                Type::unknown(),
            ),
        ),
    ];
    for form in [SpecialFormType::Divergent, SpecialFormType::Todo] {
        cases.push((
            form,
            error(
                InvalidTypeExpression::InvalidType(Type::SpecialForm(form), scope),
                Type::unknown(),
            ),
        ));
    }
    for module in [TypingModule::Typing, TypingModule::TypingExtensions] {
        cases.push((
            SpecialFormType::TypedDict(module),
            error(InvalidTypeExpression::TypedDict, Type::unknown()),
        ));
    }
    for form in [
        SpecialFormType::Literal,
        SpecialFormType::Union,
        SpecialFormType::Intersection,
    ] {
        cases.push((
            form,
            error(
                InvalidTypeExpression::RequiresArguments(form),
                Type::unknown(),
            ),
        ));
    }
    for form in [
        SpecialFormType::Optional,
        SpecialFormType::Not,
        SpecialFormType::Top,
        SpecialFormType::Bottom,
        SpecialFormType::TypeOf,
        SpecialFormType::TypeIs,
        SpecialFormType::TypeGuard,
        SpecialFormType::Unpack,
        SpecialFormType::CallableTypeOf,
        SpecialFormType::RegularCallableTypeOf,
    ] {
        cases.push((
            form,
            error(
                InvalidTypeExpression::RequiresOneArgument(form),
                Type::unknown(),
            ),
        ));
    }
    for qualifier in [
        TypeQualifier::ReadOnly,
        TypeQualifier::Final,
        TypeQualifier::ClassVar,
        TypeQualifier::Required,
        TypeQualifier::NotRequired,
        TypeQualifier::InitVar,
    ] {
        cases.push((
            SpecialFormType::TypeQualifier(qualifier),
            error(
                InvalidTypeExpression::TypeQualifier(qualifier),
                Type::unknown(),
            ),
        ));
    }
    for (form, expected) in cases {
        observations::reset(None);
        let progress = Progress::default();
        let converted = capture(&db, || {
            controlled(
                &prepared,
                scope,
                Input::SpecialForm(form),
                InferenceFlags::empty(),
                Interruption::None,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        assert_eq!(
            converted.value,
            Ok(AnalysisOutcome::Complete(expected.clone())),
            "{form:?}",
        );
        assert!(converted.reads.is_empty(), "{form:?}");
        assert_eq!(progress.pools.get(), Some([0; 5]));
        assert!(progress.completed.get());
        assert_cleanup(&progress);
        assert_eq!(
            Type::SpecialForm(form).in_type_expression(&db, scope, None, InferenceFlags::empty()),
            expected,
            "{form:?}",
        );
    }
}

#[test]
fn concatenate_and_self_flags_preserve_error_payloads_and_fallbacks() {
    let db = fixture();
    let prepared = prepare(&db);

    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let concatenate_fallback = Type::Dynamic(DynamicType::InvalidConcatenateUnknown);
    for (form, flags, expected) in [
        (
            SpecialFormType::Concatenate,
            InferenceFlags::empty(),
            error(InvalidTypeExpression::Concatenate, concatenate_fallback),
        ),
        (
            SpecialFormType::Concatenate,
            InferenceFlags::IN_VALID_CONCATENATE_CONTEXT,
            error(
                InvalidTypeExpression::RequiresTwoArguments(SpecialFormType::Concatenate),
                concatenate_fallback,
            ),
        ),
        (
            SpecialFormType::TypingSelf,
            InferenceFlags::IN_TYPE_ALIAS,
            error(
                InvalidTypeExpression::TypingSelfInTypeAlias,
                Type::unknown(),
            ),
        ),
        (
            SpecialFormType::TypingSelf,
            InferenceFlags::IN_TYPE_ALIAS
                | InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER
                | InferenceFlags::IN_RETURN_TYPE,
            error(
                InvalidTypeExpression::TypingSelfInTypeAlias,
                Type::unknown(),
            ),
        ),
    ] {
        observations::reset(None);
        let progress = Progress::default();
        let converted = capture(&db, || {
            controlled(
                &prepared,
                scope,
                Input::SpecialForm(form),
                flags,
                Interruption::None,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        assert_eq!(
            converted.value,
            Ok(AnalysisOutcome::Complete(expected.clone()))
        );
        assert!(converted.reads.is_empty());
        assert_eq!(progress.pools.get(), Some([0; 5]));
        assert_cleanup(&progress);
        assert_eq!(
            Type::SpecialForm(form).in_type_expression(&db, scope, None, flags),
            expected
        );
    }
}

#[test]
fn special_form_children_preserve_precise_unavailable_operations() {
    let db = fixture();
    let prepared = prepare(&db);

    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    for (form, operation) in [
        (
            SpecialFormType::TypingSelf,
            TypeConversionOperation::SpecialFormSelf,
        ),
        (
            SpecialFormType::TypingCallable,
            TypeConversionOperation::SpecialFormCallable,
        ),
        (
            SpecialFormType::CollectionsAbcCallable,
            TypeConversionOperation::SpecialFormCallable,
        ),
    ] {
        for _ in 0..2 {
            observations::reset(None);
            let progress = Progress::default();
            assert_eq!(
                controlled(
                    &prepared,
                    scope,
                    Input::SpecialForm(form),
                    InferenceFlags::empty(),
                    Interruption::None,
                    &funded(),
                    &progress,
                ),
                Ok(unavailable(OperationId::TypeConversion(operation))),
            );
            assert!(progress.entered.get());
            assert!(!progress.completed.get());
            assert_eq!(progress.pools.get(), Some([0; 5]));
            assert_cleanup(&progress);
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}

#[test]
fn tuple_and_typeform_conversions_use_canonical_identities() {
    for form in [SpecialFormType::Tuple, SpecialFormType::TypeForm] {
        let db = fixture();
        let prepared = prepare(&db);

        let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let progress = Progress::default();
        let converted = capture(&db, || {
            controlled(
                &prepared,
                scope,
                Input::SpecialForm(form),
                InferenceFlags::empty(),
                Interruption::None,
                &funded(),
                &progress,
            )
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete(Ok(ty))) = converted.value else {
            panic!("{form:?}: {:?}", converted.value);
        };
        assert!(converted.reads.is_empty());
        assert_cleanup(&progress);
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let expected = if form == SpecialFormType::Tuple {
            assert_eq!(
                ty.exact_tuple_instance_spec(&db).as_deref(),
                Some(&TupleSpec::homogeneous(Type::unknown())),
            );
            Type::homogeneous_tuple(&db, &env, Type::unknown())
        } else {
            let Type::TypeForm(type_form) = ty else {
                panic!("TypeForm conversion: {ty:?}");
            };
            assert_eq!(type_form.type_argument(&db), Type::any());
            TypeFormType::from_type_expression(&db, Type::any())
        };
        assert_eq!(ty, expected);
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                scope,
                Input::SpecialForm(form),
                InferenceFlags::empty(),
                Interruption::None,
                &funded(),
                &progress,
            ),
            Ok(AnalysisOutcome::Complete(Ok(expected))),
        );
        assert_cleanup(&progress);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn literal_string_fixture() -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.py",
        "from typing import LiteralString\nleft = right = LiteralString\n",
    )
    .unwrap();
    db
}

#[test]
fn cold_literal_string_matches_an_independent_ordinary_database() {
    let ordinary_db = literal_string_fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_key = expression_key(&ordinary_prepared);
    let ordinary_expression = ordinary_prepared.semantic_index().expression(ordinary_key);
    let ordinary =
        infer_expression_types(&ordinary_db, ordinary_expression, TypeContext::default())
            .expression_type(ordinary_key);
    assert_eq!(ordinary, Type::SpecialForm(SpecialFormType::LiteralString));
    let ordinary_scope =
        FileScopeId::global().to_scope_id(&ordinary_db, ordinary_prepared.program_file());
    assert_eq!(
        ordinary.in_type_expression(&ordinary_db, ordinary_scope, None, InferenceFlags::empty()),
        Ok(Type::literal_string()),
    );

    let db = literal_string_fixture();
    let prepared = prepare(&db);

    let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let progress = Progress::default();
    let converted = capture(&db, || {
        controlled(
            &prepared,
            scope,
            selected_input(&prepared),
            InferenceFlags::empty(),
            Interruption::None,
            &funded(),
            &progress,
        )
    })
    .unwrap();
    assert_eq!(
        converted.value,
        Ok(AnalysisOutcome::Complete(Ok(Type::literal_string()))),
    );
    assert_eq!(converted.check_root_reads(), Ok(()));
    assert!(observations::counts().1 > 0);
    assert_cleanup(&progress);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn dispatch_admission_preserves_semantic_errors_and_unavailable_children_on_retry() {
    for form in [
        SpecialFormType::LiteralString,
        SpecialFormType::Concatenate,
        SpecialFormType::TypingCallable,
    ] {
        let db = fixture();
        let prepared = prepare(&db);

        let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let progress = Progress::default();
        assert_eq!(
            controlled(
                &prepared,
                scope,
                Input::SpecialForm(form),
                InferenceFlags::empty(),
                Interruption::WorkRemaining(0),
                &funded(),
                &progress,
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
        );
        assert!(progress.entered.get());
        assert!(!progress.completed.get());
        assert_eq!(progress.pools.get(), Some([0; 5]));
        assert_cleanup(&progress);
        let progress = Progress::default();
        let retry = controlled(
            &prepared,
            scope,
            Input::SpecialForm(form),
            InferenceFlags::empty(),
            Interruption::None,
            &funded(),
            &progress,
        );
        let expected = match form {
            SpecialFormType::LiteralString => AnalysisOutcome::Complete(Ok(Type::literal_string())),
            SpecialFormType::Concatenate => AnalysisOutcome::Complete(error(
                InvalidTypeExpression::Concatenate,
                Type::Dynamic(DynamicType::InvalidConcatenateUnknown),
            )),
            _ => unavailable(OperationId::TypeConversion(
                TypeConversionOperation::SpecialFormCallable,
            )),
        };
        assert_eq!(retry, Ok(expected));
        assert_cleanup(&progress);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn conversion_interruption_retires_input_owners_and_reuses_the_completed_memo() {
    for interruption in [
        Interruption::WorkRemaining(0),
        Interruption::Allocation,
        Interruption::CancelBeforeConversion,
        Interruption::CancelAfterConversion,
    ] {
        let db = literal_string_fixture();
        let prepared = prepare(&db);

        let scope = FileScopeId::global().to_scope_id(&db, prepared.program_file());
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let progress = Progress::default();
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(
                &prepared,
                scope,
                selected_input(&prepared),
                InferenceFlags::empty(),
                interruption,
                &funded(),
                &progress,
            )
        }));
        match interruption {
            Interruption::CancelBeforeConversion | Interruption::CancelAfterConversion => {
                assert!(matches!(result, Err(salsa::Cancelled::Local)));
            }
            _ => {
                let Ok(result) = result else {
                    panic!("admission must return an incomplete result");
                };
                assert_eq!(
                    result,
                    Ok(AnalysisOutcome::Incomplete {
                        reason: if matches!(interruption, Interruption::Allocation) {
                            AnalysisIncomplete::RequestedAllocationLimit
                        } else {
                            AnalysisIncomplete::WorkLimit
                        },
                        completed: (),
                    }),
                );
            }
        }
        assert!(progress.remaining.get().is_some());
        assert_eq!(
            progress.completed.get(),
            matches!(interruption, Interruption::CancelAfterConversion),
        );
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
        assert_eq!(
            controlled(
                &prepared,
                scope,
                selected_input(&prepared),
                InferenceFlags::empty(),
                Interruption::None,
                &funded(),
                &progress,
            ),
            Ok(AnalysisOutcome::Complete(Ok(Type::literal_string()))),
        );
        assert_eq!(observations::counts(), (0, 0, 0));
        assert_eq!(progress.pools.get(), Some([0; 5]));
        assert_cleanup(&progress);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
