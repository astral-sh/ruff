use std::panic::AssertUnwindSafe;

use ruff_db::diagnostic::Severity;
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::analysis::AssignmentValidationOperation;
use crate::lint::{LintSource, RuleSelection};
use crate::types::infer::builder::source_definition::controlled::declaration_observations;
use crate::types::{TypeAndQualifiers, TypeQualifiers};
use crate::db::tests::TestDbBuilder;
use crate::types::class::code_generator_of_static_class_ingredient;
use crate::types::infer::builder::annotated_assignment::AnnotatedAssignmentOperation;
use crate::types::infer::nearest_enclosing_class;
use ty_python_core::scope::{NodeWithScopeRef, ScopeId};
use ty_python_core::SemanticIndex;

fn fixture(name: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.pyi",
        format!(
            "class Leaf: ...\n\
             value: Leaf\n\
             factory: type[Leaf]\n\
             left = right = {name}\n"
        ),
    )
    .unwrap();
    db
}

fn prepare_fixture(db: &TestDb) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, "src/main.pyi").unwrap();
    prepare_file(db, file).unwrap()
}

fn annotation<'ast>(
    prepared: &'ast PreparedAnalysisFile<'_>,
    name: &str,
) -> &'ast ast::StmtAnnAssign {
    prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(|statement| {
            if let Stmt::AnnAssign(assignment) = statement
                && let ast::Expr::Name(target) = assignment.target.as_ref()
                && target.id.as_str() == name
            {
                Some(assignment)
            } else {
                None
            }
        })
        .unwrap()
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

#[test]
fn cold_type_expression_queries_preserve_their_canonical_identity_and_payload() {
    for (name, expected) in [("value", "Leaf"), ("factory", "type[Leaf]")] {
        let db = fixture(name);
        let prepared = prepare_fixture(&db);
        let node = annotation(&prepared, name).annotation.as_ref();
        let key = node.into();
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, key, &funded());
        let Ok(AnalysisOutcome::Complete(ty)) = result else {
            panic!("{name}: {result:?}");
        };
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(ty.display(&db, &env).to_string(), expected);
        assert_cleanup();
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "infer_expression_types_impl", None, &events)
                .is_some()
        );
        let expression = prepared.semantic_index().expression(node);
        let canonical = infer_expression_types(&db, expression, TypeContext::default());
        assert_eq!(canonical.expression_type(node), ty);
        assert_eq!(
            expression_type_with_policy(&prepared, key, &funded()),
            result
        );
        let events = events_db.take_salsa_events();
        for query in [
            "infer_expression_types_impl",
            "infer_definition_types",
            "static_class_generic_context",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();

        let program_file = prepared.program_file();
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Expression(expression, TypeContext::default()),
            program_file.file(&db),
            program_file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_expression();
        assert_eq!(canonical, &ordinary);
    }
}

#[test]
fn cold_annotations_preserve_canonical_payloads_and_reuse_completed_queries() {
    for (name, expected) in [("value", "Leaf"), ("factory", "type[Leaf]")] {
        let db = fixture(name);
        let prepared = prepare_fixture(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let result = expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
        let Ok(AnalysisOutcome::Complete(ty)) = result else {
            panic!("{name}: {result:?}");
        };
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(ty.display(&db, &env).to_string(), expected);
        assert_cleanup();

        let events = events_db.take_salsa_events();
        for query in ["infer_definition_types", "static_class_generic_context"] {
            assert!(find_will_execute_event_by_name(&db, query, None, &events).is_some());
        }
        let node = annotation(&prepared, name);
        let definition = prepared.semantic_index().expect_single_definition(node);
        let canonical = infer_definition_types(&db, definition);
        assert_eq!(canonical.completed_binding(definition), Some(ty));
        assert_eq!(
            canonical.completed_declaration(definition),
            Some(crate::types::TypeAndQualifiers::declared(ty)),
        );
        assert_eq!(
            canonical.try_expression_type(node.annotation.as_ref()),
            Some(ty)
        );
        assert_eq!(
            canonical.try_expression_type(node.target.as_ref()),
            Some(ty)
        );
        let expression = prepared
            .semantic_index()
            .expression(expression_key(&prepared));
        let canonical_expression = infer_expression_types(&db, expression, TypeContext::default());
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            result,
        );
        let events = events_db.take_salsa_events();
        for query in [
            "infer_definition_types",
            "infer_expression_types_impl",
            "static_class_generic_context",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();

        let program_file = prepared.program_file();
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            program_file.file(&db),
            program_file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(canonical, &ordinary);
        let ordinary_expression = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Expression(expression, TypeContext::default()),
            program_file.file(&db),
            program_file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_expression();
        assert_eq!(canonical_expression, &ordinary_expression);

        let ordinary_db = fixture(name);
        let ordinary_prepared = prepare_fixture(&ordinary_db);
        let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
        let expression = ordinary_prepared
            .semantic_index()
            .expression(expression_key(&ordinary_prepared));
        let ordinary = infer_expression_types(&ordinary_db, expression, TypeContext::default());
        assert_eq!(
            ordinary
                .expression_type(expression_key(&ordinary_prepared))
                .display(&ordinary_db, &ordinary_env)
                .to_string(),
            expected,
        );
    }
}

#[test]
fn interrupted_annotations_drain_owners_and_retry_with_completed_children() {
    for (name, expected) in [("value", "Leaf"), ("factory", "type[Leaf]")] {
        let measured = fixture(name);
        let measured_prepared = prepare_fixture(&measured);
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
        let boundaries = [
            (
                observations::Event::AnnotationCompleted,
                observations::annotation_completed(),
            ),
            (
                observations::Event::AnnotatedDefinitionStored,
                observations::annotated_definition_stored(),
            ),
        ];
        for (event, (count, remaining)) in boundaries {
            assert!(count > 0);
            let work = funded().semantic_work_limit - remaining.unwrap();
            for cancel in [false, true] {
                let db = fixture(name);
                let prepared = prepare_fixture(&db);
                let revision = salsa::plumbing::current_revision(&db);
                let mut events_db = db.clone();
                let definition = prepared
                    .semantic_index()
                    .expect_single_definition(annotation(&prepared, name));
                let Some(Stmt::ClassDef(leaf)) = prepared.parsed_module().syntax().body.first()
                else {
                    panic!("fixture Leaf class");
                };
                let leaf_definition = prepared.semantic_index().expect_single_definition(leaf);
                observations::reset(cancel.then_some(event));
                let policy = if cancel {
                    funded()
                } else {
                    AnalysisPolicy {
                        semantic_work_limit: work,
                        ..funded()
                    }
                };
                let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                    expression_type_with_policy(&prepared, expression_key(&prepared), &policy)
                }));
                match result {
                    Err(salsa::Cancelled::Local) if cancel => {}
                    Ok(result) if !cancel => assert_eq!(
                        result,
                        Ok(AnalysisOutcome::Incomplete {
                            reason: AnalysisIncomplete::WorkLimit,
                            completed: (),
                        }),
                    ),
                    other => panic!("{name}, {event:?}, cancel={cancel}: {other:?}"),
                }
                let (count, remaining) = match event {
                    observations::Event::AnnotationCompleted => {
                        observations::annotation_completed()
                    }
                    observations::Event::AnnotatedDefinitionStored => {
                        observations::annotated_definition_stored()
                    }
                    _ => panic!("fixture annotation event"),
                };
                assert!(count > 0);
                assert_cleanup();
                assert!(
                    FinalSourceMemo::certify(
                        &db as &dyn Db,
                        definition_inference_ingredient(&db),
                        leaf_definition.as_id(),
                    )
                    .is_ok()
                );
                if !cancel {
                    assert_eq!(remaining, Some(0));
                    assert_eq!(
                        FinalSourceMemo::certify(
                            &db as &dyn Db,
                            definition_inference_ingredient(&db),
                            definition.as_id(),
                        )
                        .map(|_| ()),
                        Err(FinalSourceError::MissingMemo),
                    );
                }
                events_db.take_salsa_events();
                observations::reset(None);
                let result =
                    expression_type_with_policy(&prepared, expression_key(&prepared), &funded());
                let Ok(AnalysisOutcome::Complete(ty)) = result else {
                    panic!("{result:?}");
                };
                let env = ProgramEnvironment::from_file(prepared.program_file());
                assert_eq!(ty.display(&db, &env).to_string(), expected);
                assert_eq!(salsa::plumbing::current_revision(&db), revision);
                assert_cleanup();
                let events = events_db.take_salsa_events();
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_definition_types",
                    Some(leaf_definition.as_id()),
                    &events,
                );
                if !cancel {
                    assert!(
                        find_will_execute_event_by_name(
                            &db,
                            "infer_definition_types",
                            Some(definition.as_id()),
                            &events,
                        )
                        .is_some()
                    );
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct DeclarationRequest<'db>(Definition<'db>);

impl<'db> nominal_members::MemberOperation<'db> for DeclarationRequest<'db> {
    type Output = &'db DefinitionInference<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.definition(self.0).await
    }
}

fn declaration_fixture(prefix: &str, annotation: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", format!("{prefix}flag: {annotation}\n"))
        .unwrap();
    db
}

fn prepare_declaration(db: &TestDb) -> PreparedAnalysisFile<'_> {
    prepare_file(db, system_path_to_file(db, "src/main.py").unwrap()).unwrap()
}

fn declaration_request<'db>(prepared: &PreparedAnalysisFile<'db>) -> DeclarationRequest<'db> {
    DeclarationRequest(
        prepared
            .semantic_index()
            .expect_single_definition(annotation(prepared, "flag")),
    )
}

fn declaration_run<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<&'db DefinitionInference<'db>>, AnalysisFailure> {
    nominal_members::controlled_member_operation(prepared, declaration_request(prepared), policy)
}

fn assert_declaration_missing(db: &TestDb, definition: Definition<'_>) {
    assert_eq!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
}

/// Ordinary-source annotations preserve their declaration without creating a binding, including
/// after a compatible prior binding; completed controlled and ordinary requests reuse one memo.
#[test]
fn declaration_only_preserves_prior_bindings_and_canonical_payloads() {
    for prefix in ["", "flag = True\n", "if True:\n    flag = True\n"] {
        let db = declaration_fixture(prefix, "bool");
        let prepared = prepare_declaration(&db);
        let definition = declaration_request(&prepared).0;
        let revision = salsa::plumbing::current_revision(&db);
        assert_declaration_missing(&db, definition);
        observations::reset(None);
        declaration_observations::reset(definition, false);
        let result = declaration_run(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete(inference)) = result else {
            panic!("{prefix:?}: {result:?}");
        };
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let declared = inference.completed_declaration(definition).unwrap();
        assert_eq!(declared.inner_type().display(&db, &env).to_string(), "bool");
        assert_eq!(inference.completed_binding(definition), None);
        let node = annotation(&prepared, "flag");
        assert_eq!(
            inference.try_expression_type(node.target.as_ref()),
            Some(declared.inner_type())
        );
        assert!(declaration_observations::state().stored);
        assert_eq!(
            declaration_observations::state().fallbacks,
            usize::from(prefix.is_empty())
        );
        assert_cleanup();
        assert!(std::ptr::eq(
            inference,
            infer_definition_types(&db, definition)
        ));
        let repeat = declaration_run(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete(repeated)) = repeat else {
            panic!("{repeat:?}");
        };
        assert!(std::ptr::eq(inference, repeated));
        let file = prepared.program_file();
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            file.file(&db),
            file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(inference, &ordinary);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Controlled inference of an incompatible prior binding reaches the unsupported diagnostic before
/// storing a declaration; ordinary inference records the Unknown fallback and annotated target type.
#[test]
fn invalid_declaration_refuses_before_storage_and_preserves_ordinary_fallback() {
    let db = declaration_fixture("flag = True\n", "str");
    let prepared = prepare_declaration(&db);
    let definition = declaration_request(&prepared).0;
    observations::reset(None);
    declaration_observations::reset(definition, false);
    assert_eq!(declaration_run(&prepared, &funded()), Ok(unavailable(OperationId::AnnotatedAssignment(
        crate::types::infer::builder::annotated_assignment::AnnotatedAssignmentOperation::Diagnostic,
    ))));
    assert!(!declaration_observations::state().before);
    assert_declaration_missing(&db, definition);
    assert_cleanup();
    let ordinary = infer_definition_types(&db, definition);
    assert_eq!(
        ordinary.completed_declaration(definition),
        Some(TypeAndQualifiers::declared(Type::unknown()))
    );
    assert_eq!(ordinary.completed_binding(definition), None);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        ordinary
            .try_expression_type(annotation(&prepared, "flag").target.as_ref())
            .unwrap()
            .display(&db, &env)
            .to_string(),
        "str"
    );
}

fn storage_budget(bytes: bool, limit: usize) -> AnalysisPolicy {
    if bytes {
        AnalysisPolicy {
            requested_bytes_limit: limit,
            ..funded()
        }
    } else {
        AnalysisPolicy {
            semantic_work_limit: limit,
            ..funded()
        }
    }
}

fn reaches_declaration_storage(bytes: bool, limit: usize) -> bool {
    let db = declaration_fixture("", "bool");
    let prepared = prepare_declaration(&db);
    observations::reset(None);
    declaration_observations::reset(declaration_request(&prepared).0, false);
    let result = declaration_run(&prepared, &storage_budget(bytes, limit));
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(_)) | Ok(AnalysisOutcome::Incomplete { .. })
        ),
        "{result:?}"
    );
    assert_cleanup();
    declaration_observations::state().stored
}

/// Real work and requested-byte limits reject the final declaration append after semantic checking;
/// no parent memo is published, and the same Definition succeeds on a same-revision retry.
#[test]
fn declaration_storage_refusal_drains_and_retries() {
    for bytes in [false, true] {
        let mut low = 0;
        let mut high = if bytes {
            funded().requested_bytes_limit
        } else {
            funded().semantic_work_limit
        };
        assert!(reaches_declaration_storage(bytes, high));
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if reaches_declaration_storage(bytes, middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        let db = declaration_fixture("", "bool");
        let prepared = prepare_declaration(&db);
        let definition = declaration_request(&prepared).0;
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        declaration_observations::reset(definition, false);
        assert_eq!(
            declaration_run(&prepared, &storage_budget(bytes, low)),
            Ok(AnalysisOutcome::Incomplete {
                reason: if bytes {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: (),
            })
        );
        let storage = declaration_observations::state();
        assert!(storage.before, "bytes={bytes}: {storage:?}");
        assert!(!storage.stored);
        assert!(storage.work > 0 && storage.bytes > 0);
        assert_declaration_missing(&db, definition);
        assert_cleanup();
        declaration_observations::reset(definition, false);
        assert!(matches!(
            declaration_run(&prepared, &funded()),
            Ok(AnalysisOutcome::Complete(_))
        ));
        assert!(declaration_observations::state().stored);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Cancellation after appending a declaration drains active owners; a same-revision retry retains
/// the annotation-only result.
#[test]
fn declaration_storage_cancellation_drains_and_retries() {
    let db = declaration_fixture("", "bool");
    let prepared = prepare_declaration(&db);
    let definition = declaration_request(&prepared).0;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    declaration_observations::reset(definition, true);
    assert!(matches!(
        salsa::Cancelled::catch(AssertUnwindSafe(|| declaration_run(&prepared, &funded()))),
        Err(salsa::Cancelled::Local)
    ));
    assert!(declaration_observations::state().stored);
    assert_cleanup();
    declaration_observations::reset(definition, false);
    let result = declaration_run(&prepared, &funded());
    let Ok(AnalysisOutcome::Complete(inference)) = result else {
        panic!("{result:?}");
    };
    assert_eq!(inference.completed_binding(definition), None);
    assert!(inference.completed_declaration(definition).is_some());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Defines one unvalued class annotation under Python 3.13 so typing.Required is available.
fn class_qualifier_fixture(header: &str, qualifier: &str) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(ast::PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file("src/main.py", format!("from typing import ClassVar, Required\nclass Owner{header}:\n    flag: {qualifier}[bool]\n")).unwrap();
    db
}

/// Selects the fixture class without inferring its definition.
fn owner_class<'ast>(prepared: &'ast PreparedAnalysisFile<'_>) -> &'ast ast::StmtClassDef {
    prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .rev()
        .find_map(Stmt::as_class_def_stmt)
        .unwrap()
}

/// Selects the nested annotation definition without preparing any semantic child.
fn class_declaration_request<'db>(prepared: &PreparedAnalysisFile<'db>) -> DeclarationRequest<'db> {
    let assignment = owner_class(prepared)
        .body
        .iter()
        .find_map(Stmt::as_ann_assign_stmt)
        .unwrap();
    DeclarationRequest(
        prepared
            .semantic_index()
            .expect_single_definition(assignment),
    )
}

/// Runs the nested declaration as a canonical controlled definition request.
fn class_declaration_run<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<&'db DefinitionInference<'db>>, AnalysisFailure> {
    nominal_members::controlled_member_operation(
        prepared,
        class_declaration_request(prepared),
        policy,
    )
}

/// Requires the class child to be complete before reading its original static identity.
fn completed_owner<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> StaticClassLiteral<'db> {
    let definition = prepared
        .semantic_index()
        .expect_single_definition(owner_class(prepared));
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id()
        )
        .is_ok()
    );
    infer_definition_types(db, definition)
        .original_class_type(definition)
        .and_then(ClassLiteral::as_static)
        .unwrap()
}

/// A cold ClassVar declaration uses the original class and canonical definition memo. Plain classes
/// skip code-generator inference, while an explicit object base completes that canonical child.
#[test]
fn class_qualifiers_preserve_cold_declarations_and_canonical_children() {
    for header in ["", "(object)"] {
        for ordinary_first in [false, true] {
            let db = class_qualifier_fixture(header, "ClassVar");
            let prepared = prepare_declaration(&db);
            let definition = class_declaration_request(&prepared).0;
            assert_declaration_missing(&db, definition);
            let ordinary = ordinary_first.then(|| infer_definition_types(&db, definition));
            let mut events_db = db.clone();
            events_db.take_salsa_events();
            observations::reset(None);
            let result = class_declaration_run(&prepared, &funded());
            let Ok(AnalysisOutcome::Complete(inference)) = result else {
                panic!("{header}, ordinary_first={ordinary_first}: {result:?}");
            };
            let events = events_db.take_salsa_events();
            let class = completed_owner(&db, &prepared);
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let declared = inference.completed_declaration(definition).unwrap();
            assert_eq!(declared.qualifiers, TypeQualifiers::CLASS_VAR);
            assert_eq!(declared.inner_type().display(&db, &env).to_string(), "bool");
            assert_eq!(inference.completed_binding(definition), None);
            assert!(std::ptr::eq(
                inference,
                infer_definition_types(&db, definition)
            ));
            if let Some(ordinary) = ordinary {
                assert!(std::ptr::eq(inference, ordinary));
            }
            let executed = find_will_execute_event_by_name(
                &db,
                "code_generator_of_static_class",
                Some(class.as_id()),
                &events,
            )
            .is_some();
            assert_eq!(executed, !ordinary_first && !header.is_empty());
            if ordinary_first {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_definition_types",
                    Some(definition.as_id()),
                    &events,
                );
            }
            let repeat = class_declaration_run(&prepared, &funded());
            let Ok(AnalysisOutcome::Complete(repeated)) = repeat else {
                panic!("{repeat:?}");
            };
            assert!(std::ptr::eq(inference, repeated));
            assert_cleanup();
        }
    }
}

/// A Required annotation in a plain class reaches the named diagnostic refusal before declaration
/// storage or parent publication, including when the ordinary diagnostic is explicitly suppressed.
#[test]
fn class_qualifier_diagnostics_refuse_before_storage() {
    for suppressed in [false, true] {
        let mut db = class_qualifier_fixture("", "Required");
        if suppressed {
            db.write_file("src/main.py", "from typing import Required\nclass Owner:\n    flag: Required[bool]  # ty: ignore[invalid-type-form]\n").unwrap();
        }
        let prepared = prepare_declaration(&db);
        let definition = class_declaration_request(&prepared).0;
        observations::reset(None);
        declaration_observations::reset(definition, false);
        assert_eq!(
            class_declaration_run(&prepared, &funded()),
            Ok(unavailable(OperationId::AnnotatedAssignment(
                AnnotatedAssignmentOperation::Diagnostic
            )))
        );
        assert!(!declaration_observations::state().before);
        assert_declaration_missing(&db, definition);
        completed_owner(&db, &prepared);
        assert_cleanup();
    }
}

/// Measures whether a cold class annotation reaches its final declaration append.
fn reaches_class_declaration_storage(bytes: bool, limit: usize) -> bool {
    let db = class_qualifier_fixture("(object)", "ClassVar");
    let prepared = prepare_declaration(&db);
    observations::reset(None);
    declaration_observations::reset(class_declaration_request(&prepared).0, false);
    let result = class_declaration_run(&prepared, &storage_budget(bytes, limit));
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(_)) | Ok(AnalysisOutcome::Incomplete { .. })
        ),
        "{result:?}"
    );
    assert_cleanup();
    declaration_observations::state().stored
}

/// Independent work and byte limits interrupt declaration storage after both the original class
/// and code-generator children complete; funded retries reuse those children in the same revision.
#[test]
fn class_qualifier_children_survive_storage_refusal_and_retry() {
    for bytes in [false, true] {
        let mut low = 0;
        let mut high = if bytes {
            funded().requested_bytes_limit
        } else {
            funded().semantic_work_limit
        };
        assert!(reaches_class_declaration_storage(bytes, high));
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if reaches_class_declaration_storage(bytes, middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        let db = class_qualifier_fixture("(object)", "ClassVar");
        let prepared = prepare_declaration(&db);
        let definition = class_declaration_request(&prepared).0;
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        declaration_observations::reset(definition, false);
        assert_eq!(
            class_declaration_run(&prepared, &storage_budget(bytes, low)),
            Ok(AnalysisOutcome::Incomplete {
                reason: if bytes {
                    AnalysisIncomplete::RequestedAllocationLimit
                } else {
                    AnalysisIncomplete::WorkLimit
                },
                completed: (),
            })
        );
        let state = declaration_observations::state();
        assert!(state.before && !state.stored, "{state:?}");
        assert_declaration_missing(&db, definition);
        let class = completed_owner(&db, &prepared);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                code_generator_of_static_class_ingredient(&db),
                class.as_id()
            )
            .is_ok()
        );
        assert_cleanup();
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        declaration_observations::reset(definition, false);
        let result = class_declaration_run(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete(inference)) = result else {
            panic!("{result:?}");
        };
        assert!(std::ptr::eq(
            inference,
            infer_definition_types(&db, definition)
        ));
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(
            &db,
            "code_generator_of_static_class",
            Some(class.as_id()),
            &events,
        );
        let class_definition = prepared
            .semantic_index()
            .expect_single_definition(owner_class(&prepared));
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(class_definition.as_id()),
            &events,
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Exercises ancestor selection within the real controlled source runtime.
#[derive(Debug)]
struct NearestClassRequest<'index, 'db> {
    index: &'index SemanticIndex<'db>,
    scope: ScopeId<'db>,
}

impl<'db> nominal_members::MemberOperation<'db> for NearestClassRequest<'_, 'db> {
    type Output = Option<StaticClassLiteral<'db>>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        SourceEffects::new(access, program)
            .nearest_enclosing_class(self.index, self.scope)
            .await
    }
}

/// Controlled nearest-class lookup includes the class itself and crosses nested function and
/// type-parameter scopes while retaining the nearest class.
#[test]
fn nearest_class_preserves_scope_order() {
    let mut db = TestDbBuilder::new()
        .with_python_version(ast::PythonVersion::PY313)
        .build()
        .unwrap();
    db.write_file(
        "src/main.py",
        "class Outer:\n    def method[T](self):\n        def nested(): ...\n    class Inner: ...\n",
    )
    .unwrap();
    let prepared = prepare_declaration(&db);
    let outer = owner_class(&prepared);
    let method = outer
        .body
        .iter()
        .find_map(Stmt::as_function_def_stmt)
        .unwrap();
    let nested = method
        .body
        .iter()
        .find_map(Stmt::as_function_def_stmt)
        .unwrap();
    let inner = outer.body.iter().find_map(Stmt::as_class_def_stmt).unwrap();
    let index = prepared.semantic_index();
    for (node, expected) in [
        (NodeWithScopeRef::Class(outer), "Outer"),
        (NodeWithScopeRef::Function(nested), "Outer"),
        (NodeWithScopeRef::Class(inner), "Inner"),
    ] {
        let scope = index.scope_id(index.node_scope(node));
        observations::reset(None);
        let result = nominal_members::controlled_member_operation(
            &prepared,
            NearestClassRequest { index, scope },
            &funded(),
        );
        let Ok(AnalysisOutcome::Complete(Some(class))) = result else {
            panic!("{expected}: {result:?}");
        };
        assert_eq!(class.name(&db).as_str(), expected);
        assert_eq!(nearest_enclosing_class(&db, index, scope), Some(class));
        assert_cleanup();
    }

}

/// Creates an assignment fixture with the requested unsound-assignment rule severity.
fn assignment_validation_fixture(path: &str, source: &str, unsound: Option<Severity>) -> TestDb {
    let registry = crate::default_lint_registry();
    let mut rules = RuleSelection::from_registry(registry);
    if let Some(severity) = unsound {
        rules.enable(
            registry.get("unsound-assignment").unwrap(),
            severity,
            LintSource::File,
        );
    }
    TestDbBuilder::new()
        .with_file(path, source)
        .with_rule_selection(rules)
        .build()
        .unwrap()
}

fn assignment_validation_node<'ast>(
    prepared: &'ast PreparedAnalysisFile<'_>,
) -> &'ast ast::StmtAssign {
    let Some(Stmt::Assign(assignment)) = prepared.parsed_module().syntax().body.last() else {
        panic!("fixture's final assignment");
    };
    assignment
}

/// A separate declaration and assignment use shared validation while preserving the inferred
/// binding and target expression. Disabled unsound checks, stub files, and gradual targets each
/// allow completion, and repeated requests reuse the canonical definition result.
#[test]
fn assignment_validation_preserves_real_assignment_payloads_and_canonical_reuse() {
    for (path, source, unsound) in [
        ("/src/main.py", "flag: bool\nflag = True\n", None),
        (
            "/src/main.pyi",
            "flag: bool\nflag = True\n",
            Some(Severity::Warning),
        ),
        (
            "/src/main.py",
            "from typing import Any\nflag: Any\nflag = True\n",
            Some(Severity::Warning),
        ),
    ] {
        let db = assignment_validation_fixture(path, source, unsound);
        let prepared = prepare_file(&db, system_path_to_file(&db, path).unwrap()).unwrap();
        let assignment = assignment_validation_node(&prepared);
        let definition = assignment_definition(&prepared, assignment);
        let request = DeclarationRequest(definition);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        assert_declaration_missing(&db, definition);
        events_db.take_salsa_events();
        observations::reset(None);
        let result = nominal_members::controlled_member_operation(&prepared, request, &funded());
        let Ok(AnalysisOutcome::Complete(inference)) = result else {
            panic!("{path}, unsound={unsound:?}, {source:?}: {result:?}");
        };
        let value = Type::bool_literal(true);
        assert_eq!(inference.completed_binding(definition), Some(value));
        assert_eq!(inference.completed_declaration(definition), None);
        assert_eq!(
            inference.try_expression_type(&assignment.targets[0]),
            Some(value)
        );
        assert_eq!(
            inference.try_expression_type(&*assignment.value),
            Some(value)
        );
        assert_cleanup();
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events,
            )
            .is_some()
        );
        assert!(std::ptr::eq(
            inference,
            infer_definition_types(&db, definition)
        ));
        let repeat = nominal_members::controlled_member_operation(&prepared, request, &funded());
        let Ok(AnalysisOutcome::Complete(repeated)) = repeat else {
            panic!("{repeat:?}");
        };
        assert!(std::ptr::eq(inference, repeated));
        let events = events_db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "infer_definition_types", None, &events);
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
        assert_eq!(inference, &ordinary);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// An incompatible assignment reaches invalid reporting before the optional unsound checks;
/// a compatible non-stub assignment with that rule enabled reaches the fully-static check.
/// Each unavailable child leaves the parent unpublished on repeated same-revision requests.
#[test]
fn assignment_validation_refuses_at_the_first_unavailable_child() {
    for (source, operation) in [
        (
            "flag: str\nflag = True\n",
            AssignmentValidationOperation::InvalidDiagnostic,
        ),
        (
            "flag: bool\nflag = True\n",
            AssignmentValidationOperation::FullyStatic,
        ),
    ] {
        let db = assignment_validation_fixture("/src/main.py", source, Some(Severity::Warning));
        let prepared =
            prepare_file(&db, system_path_to_file(&db, "/src/main.py").unwrap()).unwrap();
        let definition = assignment_definition(&prepared, assignment_validation_node(&prepared));
        let declaration = declaration_request(&prepared).0;
        let request = DeclarationRequest(definition);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        for attempt in 0..2 {
            observations::reset(None);
            events_db.take_salsa_events();
            assert_eq!(
                nominal_members::controlled_member_operation(&prepared, request, &funded()),
                Ok(unavailable(OperationId::AssignmentValidation(operation))),
                "{source:?}",
            );
            assert_declaration_missing(&db, definition);
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    definition_inference_ingredient(&db),
                    declaration.as_id(),
                )
                .is_ok()
            );
            assert_cleanup();
            let events = events_db.take_salsa_events();
            assert!(
                find_will_execute_event_by_name(
                    &db,
                    "infer_definition_types",
                    Some(definition.as_id()),
                    &events,
                )
                .is_some()
            );
            if attempt > 0 {
                assert_function_query_was_not_run_by_name(
                    &db,
                    "infer_definition_types",
                    Some(declaration.as_id()),
                    &events,
                );
            }
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
}

/// Returns whether both entries for the stub's `value: Leaf` definition were stored under `policy`.
fn reaches_declaration_binding_storage(policy: &AnalysisPolicy) -> bool {
    let db = fixture("value");
    let prepared = prepare_fixture(&db);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(annotation(&prepared, "value"));
    observations::reset(None);
    declaration_observations::reset(definition, false);
    let result = nominal_members::controlled_member_operation(
        &prepared,
        DeclarationRequest(definition),
        policy,
    );
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Complete(_)) | Ok(AnalysisOutcome::Incomplete { .. })
        ),
        "{result:?}",
    );
    assert_cleanup();
    declaration_observations::state().stored
}

/// Work and byte limits refuse before either entry for the stub's `value: Leaf` definition is stored.
/// Salsa defers local cancellation until definition inference finishes, so cancellation after
/// both writes preserves a completed memo. Same-revision retries reuse inference of `Leaf`
/// after refusal and reuse the completed declaration and binding after cancellation.
#[test]
fn declaration_binding_storage_limits_and_cancellation_preserve_retry() {
    let mut cases = Vec::new();
    for bytes in [false, true] {
        let mut low = 0;
        let mut high = if bytes {
            funded().requested_bytes_limit
        } else {
            funded().semantic_work_limit
        };
        assert!(reaches_declaration_binding_storage(&storage_budget(
            bytes, high
        )));
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if reaches_declaration_binding_storage(&storage_budget(bytes, middle)) {
                high = middle;
            } else {
                low = middle;
            }
        }
        let reason = if bytes {
            AnalysisIncomplete::RequestedAllocationLimit
        } else {
            AnalysisIncomplete::WorkLimit
        };
        cases.push((storage_budget(bytes, low), Some(reason)));
    }
    cases.push((funded(), None));
    for (policy, reason) in cases {
        let db = fixture("value");
        let prepared = prepare_fixture(&db);
        let node = annotation(&prepared, "value");
        let definition = prepared.semantic_index().expect_single_definition(node);
        let Some(Stmt::ClassDef(class)) = prepared.parsed_module().syntax().body.first() else {
            panic!("fixture's Leaf class");
        };
        let child = prepared.semantic_index().expect_single_definition(class);
        let request = DeclarationRequest(definition);
        let revision = salsa::plumbing::current_revision(&db);
        assert_declaration_missing(&db, definition);
        observations::reset(None);
        declaration_observations::reset(definition, reason.is_none());
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            nominal_members::controlled_member_operation(&prepared, request, &policy)
        }));
        match result {
            Ok(result) if let Some(reason) = reason => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason,
                    completed: ()
                }),
            ),
            Err(salsa::Cancelled::Local) if reason.is_none() => {}
            other => panic!("reason={reason:?}: {other:?}"),
        }
        let state = declaration_observations::state();
        assert!(
            state.before && state.work > 0 && state.bytes > 0,
            "{state:?}"
        );
        assert_eq!(state.stored, reason.is_none());
        let expected_memo = if reason.is_some() {
            Err(FinalSourceError::MissingMemo)
        } else {
            Ok(())
        };
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id(),
            )
            .map(|_| ()),
            expected_memo,
            "reason={reason:?}",
        );
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                child.as_id(),
            )
            .is_ok()
        );
        assert_cleanup();
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        declaration_observations::reset(definition, false);
        let result = nominal_members::controlled_member_operation(&prepared, request, &funded());
        let Ok(AnalysisOutcome::Complete(inference)) = result else {
            panic!("{result:?}");
        };
        let declared = inference.completed_declaration(definition).unwrap();
        assert_eq!(
            inference.completed_binding(definition),
            Some(declared.inner_type())
        );
        assert_eq!(
            inference.try_expression_type(&*node.target),
            Some(declared.inner_type())
        );
        assert_eq!(declaration_observations::state().stored, reason.is_some());
        assert!(std::ptr::eq(
            inference,
            infer_definition_types(&db, definition)
        ));
        let events = events_db.take_salsa_events();
        if reason.is_none() {
            assert_function_query_was_not_run_by_name(
                &db,
                "infer_definition_types",
                Some(definition.as_id()),
                &events,
            );
        }
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_definition_types",
            Some(child.as_id()),
            &events,
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// A valued annotation keeps its declared type, selected binding, and target expression separate;
/// completed controlled requests reuse the ordinary canonical definition and RHS memos.
#[test]
fn annotated_values_preserve_distinct_binding_and_expression_results() {
    for (annotation_type, expected_binding) in [("bool", "Literal[True]"), ("Any", "Any")] {
        let mut db = setup_db();
        db.write_file(
            "src/main.py",
            format!("from typing import Any\nclass Container:\n    def method(self):\n        flag: {annotation_type} = True\n"),
        )
        .unwrap();
        let prepared = prepare_declaration(&db);
        let [Stmt::ImportFrom(_), Stmt::ClassDef(class)] =
            prepared.parsed_module().suite().as_slice()
        else {
            panic!("fixture must contain an import and a class");
        };
        let [Stmt::FunctionDef(method)] = class.body.as_slice() else {
            panic!("fixture class must contain a method");
        };
        let [Stmt::AnnAssign(node)] = method.body.as_slice() else {
            panic!("fixture method must contain a valued annotation");
        };
        let Some(value) = node.value.as_deref() else {
            panic!("fixture must have an assigned value");
        };
        let definition = prepared.semantic_index().expect_single_definition(node);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let result = nominal_members::controlled_member_operation(
            &prepared,
            DeclarationRequest(definition),
            &funded(),
        );
        let Ok(AnalysisOutcome::Complete(inference)) = result else {
            panic!("{annotation_type}: {result:?}");
        };
        assert_cleanup();
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let Some(declared) = inference.completed_declaration(definition) else {
            panic!("valued annotation must retain its declaration");
        };
        assert_eq!(
            declared.inner_type().display(&db, &env).to_string(),
            annotation_type
        );
        assert_eq!(declared.qualifiers, TypeQualifiers::empty());
        let Some(binding) = inference.completed_binding(definition) else {
            panic!("valued annotation must store its selected binding");
        };
        assert_eq!(binding.display(&db, &env).to_string(), expected_binding);
        assert_eq!(
            inference.try_expression_type(&*node.target),
            Some(Type::bool_literal(true))
        );
        assert_eq!(
            inference.try_expression_type(value),
            Some(Type::bool_literal(true))
        );
        if annotation_type == "Any" {
            assert_ne!(Some(binding), inference.try_expression_type(&*node.target));
        }

        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(&db, "infer_definition_types", None, &events).is_some()
        );
        assert_eq!(infer_definition_types(&db, definition), inference);
        assert_eq!(
            nominal_members::controlled_member_operation(
                &prepared,
                DeclarationRequest(definition),
                &funded()
            ),
            result
        );
        let Some(expression) = prepared.semantic_index().try_expression(value) else {
            panic!("annotated RHS in a method must have a canonical expression");
        };
        let rhs = infer_expression_types(
            &db,
            expression,
            TypeContext::new(Some(declared.inner_type())),
        );
        assert_eq!(rhs.expression_type(value), Type::bool_literal(true));
        let events = events_db.take_salsa_events();
        for query in ["infer_definition_types", "infer_expression_types_impl"] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();

        let file = prepared.program_file();
        let ordinary = TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Definition(definition),
            file.file(&db),
            file,
            prepared.semantic_index(),
            prepared.parsed_module(),
        )
        .finish_definition(definition);
        assert_eq!(inference, &ordinary);
    }
}

/// An invalid assigned value reaches the assignment diagnostic boundary after RHS inference;
/// refusing that diagnostic leaves the canonical definition unpublished on every retry.
#[test]
fn annotated_value_diagnostic_refusal_does_not_publish_a_definition() {
    let mut db = setup_db();
    db.write_file("src/main.py", "flag: None = True\n").unwrap();
    let prepared = prepare_declaration(&db);
    let definition = declaration_request(&prepared).0;
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        observations::reset(None);
        let result = declaration_run(&prepared, &funded());
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::UnavailableOperation(
                        OperationId::AssignmentValidation(
                            AssignmentValidationOperation::InvalidDiagnostic,
                        )
                    ),
                    ..
                })
            ),
            "{result:?}"
        );
        assert_declaration_missing(&db, definition);
        assert_cleanup();
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
