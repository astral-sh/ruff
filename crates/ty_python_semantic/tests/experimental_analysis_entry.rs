#![cfg(feature = "experimental-analysis")]

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{InMemorySystem, MemoryFileSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::Stmt;
use ty_project::watch::ChangeEvent;
use ty_project::{CheckMode, Db as _, ProjectDatabase, ProjectMetadata};
use ty_python_core::ExpressionNodeKey;
use ty_python_semantic::analysis::{
    AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, OperationId, PreparedAnalysisFile,
    check_file_with_policy, expression_type_with_policy, prepare_file,
};
use ty_python_semantic::types::Type;
use ty_python_semantic::{Db, HasType, ProgramEnvironment, SemanticModel};

fn database(source: &str) -> anyhow::Result<ProjectDatabase> {
    let fs = MemoryFileSystem::with_current_directory("/src");
    fs.write_file("/src/main.py", source)?;
    ProjectDatabase::fallible(
        ProjectMetadata::new("analysis-entry", SystemPathBuf::from("/src")),
        InMemorySystem::from_memory_fs(fs),
    )
}

fn prepare(db: &ProjectDatabase) -> anyhow::Result<PreparedAnalysisFile<'_>> {
    let file = system_path_to_file(db, "/src/main.py")?;
    prepare_file(db, file).map_err(|error| anyhow::anyhow!("{error:?}"))
}

fn expression(prepared: &PreparedAnalysisFile<'_>) -> ExpressionNodeKey {
    let module = prepared.parsed_module();
    let Some(Stmt::Assign(assignment)) = module.syntax().body.last() else {
        panic!("fixture ends with a shared assignment");
    };
    assignment.value.as_ref().into()
}

fn funded() -> AnalysisPolicy {
    AnalysisPolicy {
        semantic_work_limit: 1_000_000,
        requested_bytes_limit: 16 * 1024 * 1024,
    }
}

#[test]
fn cold_retry_still_reaches_the_canonical_provider() -> anyhow::Result<()> {
    let db = database("left = right = [1]\n")?;
    let prepared = prepare(&db)?;
    let expression = expression(&prepared);
    for policy in [
        funded(),
        AnalysisPolicy {
            semantic_work_limit: 200_000,
            ..funded()
        },
    ] {
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(OperationId::ExpressionKind),
                completed: (),
            }),
        );
    }
    Ok(())
}

#[test]
fn ordinary_recovery_then_canonical_warm_completion() -> anyhow::Result<()> {
    let db = database("left = right = [1]\n")?;
    let prepared = prepare(&db)?;
    let expression = expression(&prepared);
    assert!(matches!(
        expression_type_with_policy(&prepared, expression, &funded()),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::UnavailableOperation(OperationId::ExpressionKind),
            ..
        }),
    ));
    let file = system_path_to_file(&db, "/src/main.py")?;
    let program_file = Db::program_file(&db, file);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let Stmt::Assign(assignment) = &module.syntax().body[0] else {
        panic!("fixture starts with a shared assignment");
    };
    let ordinary = assignment
        .value
        .inferred_type(&SemanticModel::new(&db, program_file))
        .unwrap();
    assert_eq!(
        expression_type_with_policy(&prepared, expression, &funded()),
        Ok(AnalysisOutcome::Complete(ordinary))
    );
    Ok(())
}

#[test]
fn resource_causes_precede_unavailable_source_work() -> anyhow::Result<()> {
    let db = database("left = right = [1]\n")?;
    let prepared = prepare(&db)?;
    let expression = expression(&prepared);
    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: 0,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: 0,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            })
        );
    }
    assert!(matches!(
        expression_type_with_policy(&prepared, expression, &funded()),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::UnavailableOperation(OperationId::ExpressionKind),
            ..
        })
    ));
    Ok(())
}

fn ordinary_type(db: &ProjectDatabase) -> Type<'_> {
    let file = system_path_to_file(db, "/src/main.py").unwrap();
    let program_file = Db::program_file(db, file);
    let module = parsed_module(db, program_file.python_file(db)).load(db);
    let Some(Stmt::Assign(assignment)) = module.syntax().body.last() else {
        panic!("fixture ends with a shared assignment");
    };
    assignment
        .value
        .inferred_type(&SemanticModel::new(db, program_file))
        .unwrap()
}

#[test]
fn cold_boolean_literals_complete_with_canonical_types() -> anyhow::Result<()> {
    for (source, value) in [
        ("left = right = True\n", true),
        ("left = right = False\n", false),
    ] {
        let db = database(source)?;
        let prepared = prepare(&db)?;
        let expression = expression(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let expected = Type::bool_literal(value);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );

        // An independent ordinary database provides an oracle without warming this expression.
        let ordinary_db = database(source)?;
        assert_eq!(ordinary_type(&ordinary_db), expected);
        assert_eq!(ordinary_type(&db), expected);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

fn defaulted_generic_database(source: &str) -> anyhow::Result<ProjectDatabase> {
    let fs = MemoryFileSystem::with_current_directory("/src");
    fs.write_file("/src/main.py", source)?;
    fs.write_file("/src/ty.toml", "[environment]\npython-version = '3.13'\n")?;
    let system = InMemorySystem::from_memory_fs(fs);
    let metadata = ProjectMetadata::discover(SystemPath::new("/src"), &system)?;
    ProjectDatabase::fallible(metadata, system)
}

#[test]
fn cold_generic_defaults_use_earlier_specialized_arguments() -> anyhow::Result<()> {
    for (source, expected) in [
        (
            "from typing import Generic, TypeVar\n\
             T = TypeVar(\"T\")\n\
             U = TypeVar(\"U\", default=T)\n\
             class Product(Generic[T, U]): ...\n\
             class Leaf: ...\n\
             left = right = Product[Leaf]\n",
            "<class 'Product[Leaf, Leaf]'>",
        ),
        (
            "from typing import Generic, TypeVar\n\
             T = TypeVar(\"T\")\n\
             U = TypeVar(\"U\", default=T)\n\
             V = TypeVar(\"V\", default=U)\n\
             class Product(Generic[T, U, V]): ...\n\
             class Leaf: ...\n\
             left = right = Product[Leaf]\n",
            "<class 'Product[Leaf, Leaf, Leaf]'>",
        ),
    ] {
        let db = defaulted_generic_database(source)?;
        let prepared = prepare(&db)?;
        let expression = expression(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let captured = salsa::prepared_source_probe::capture(&db, || {
            expression_type_with_policy(&prepared, expression, &funded())
        })
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let Ok(AnalysisOutcome::Complete(actual)) = captured.value else {
            anyhow::bail!("cold default specialization: {:?}", captured.value);
        };
        assert!(matches!(actual, Type::GenericAlias(_)));
        captured
            .check_root_reads()
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        salsa::prepared_source_probe::assert_no_active_attempt();

        let file = system_path_to_file(&db, "/src/main.py")?;
        let env = ProgramEnvironment::from_file(Db::program_file(&db, file));
        assert_eq!(actual.display(&db, &env).to_string(), expected);
        assert_eq!(ordinary_type(&db), actual);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &funded()),
            Ok(AnalysisOutcome::Complete(actual)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        let ordinary_db = defaulted_generic_database(source)?;
        let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
        let ordinary_env =
            ProgramEnvironment::from_file(Db::program_file(&ordinary_db, ordinary_file));
        assert_eq!(
            ordinary_type(&ordinary_db)
                .display(&ordinary_db, &ordinary_env)
                .to_string(),
            expected,
        );
    }
    Ok(())
}

fn imported_boolean_database(
    dependency: &str,
) -> anyhow::Result<(ProjectDatabase, MemoryFileSystem)> {
    let fs = MemoryFileSystem::with_current_directory("/src");
    fs.write_file(
        "/src/main.py",
        "from dependency import left\nresult = alias = left\n",
    )?;
    fs.write_file("/src/dependency.py", dependency)?;
    let db = ProjectDatabase::fallible(
        ProjectMetadata::new("analysis-entry", SystemPathBuf::from("/src")),
        InMemorySystem::from_memory_fs(fs.clone()),
    )?;
    Ok((db, fs))
}

fn check_imported_boolean(
    db: &ProjectDatabase,
    prepared: &PreparedAnalysisFile<'_>,
    expected: bool,
) -> anyhow::Result<()> {
    let revision = salsa::plumbing::current_revision(db);
    let captured = salsa::prepared_source_probe::capture(db, || {
        expression_type_with_policy(prepared, expression(prepared), &funded())
    })
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(
        captured.value,
        Ok(AnalysisOutcome::Complete(Type::bool_literal(expected))),
    );
    captured
        .check_root_reads()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(db), revision);
    Ok(())
}

#[test]
fn imported_boolean_edits_complete_with_canonical_types() -> anyhow::Result<()> {
    for (dependency, value) in [
        ("left = right = False\n# unchanged value\n", false),
        ("left = right = True\n", true),
    ] {
        let (mut db, fs) = imported_boolean_database("left = right = False\n")?;
        let previous_revision = salsa::plumbing::current_revision(&db);
        {
            let prepared = prepare(&db)?;
            check_imported_boolean(&db, &prepared, false)?;
        }

        fs.write_file("/src/dependency.py", dependency)?;
        db.apply_changes(&[ChangeEvent::file_content_changed(SystemPathBuf::from(
            "/src/dependency.py",
        ))]);
        let prepared = prepare(&db)?;
        assert_ne!(salsa::plumbing::current_revision(&db), previous_revision);
        check_imported_boolean(&db, &prepared, value)?;
        check_imported_boolean(&db, &prepared, value)?;

        let (ordinary_db, _) = imported_boolean_database(dependency)?;
        assert_eq!(ordinary_type(&ordinary_db), Type::bool_literal(value));
    }
    Ok(())
}

fn check_imported_boolean_file(
    db: &ProjectDatabase,
    prepared: &PreparedAnalysisFile<'_>,
    expected: bool,
) -> anyhow::Result<()> {
    let revision = salsa::plumbing::current_revision(db);
    let captured =
        salsa::prepared_source_probe::capture(db, || check_file_with_policy(prepared, &funded()))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert!(
        matches!(&captured.value, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty()),
        "{:?}",
        captured.value,
    );
    captured
        .check_root_reads()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;

    // Read the types only after the whole-file entry has completed its cold work.
    let Some(Stmt::Assign(assignment)) = prepared.parsed_module().syntax().body.last() else {
        anyhow::bail!("fixture ends with a shared assignment");
    };
    let file = system_path_to_file(db, "/src/main.py")?;
    let model = SemanticModel::new(db, Db::program_file(db, file));
    for target in &assignment.targets {
        assert_eq!(
            target.inferred_type(&model),
            Some(Type::bool_literal(expected))
        );
    }
    check_imported_boolean(db, prepared, expected)?;
    assert_eq!(salsa::plumbing::current_revision(db), revision);
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(db, || ()),
        Ok(()),
    );
    Ok(())
}

#[test]
fn imported_boolean_file_edits_complete_from_a_cold_entry() -> anyhow::Result<()> {
    for (dependency, expected) in [
        ("left = right = False\n# unchanged value\n", false),
        ("left = right = True\n", true),
    ] {
        let (mut db, fs) = imported_boolean_database("left = right = False\n")?;
        let previous_revision = salsa::plumbing::current_revision(&db);
        {
            let prepared = prepare(&db)?;
            check_imported_boolean_file(&db, &prepared, false)?;
        }

        fs.write_file("/src/dependency.py", dependency)?;
        db.apply_changes(&[ChangeEvent::file_content_changed(SystemPathBuf::from(
            "/src/dependency.py",
        ))]);
        let prepared = prepare(&db)?;
        assert_ne!(salsa::plumbing::current_revision(&db), previous_revision);
        check_imported_boolean_file(&db, &prepared, expected)?;
        check_imported_boolean_file(&db, &prepared, expected)?;

        let (ordinary_db, _) = imported_boolean_database(dependency)?;
        let ordinary_prepared = prepare(&ordinary_db)?;
        let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
        let ordinary_program_file = Db::program_file(&ordinary_db, ordinary_file);
        assert!(
            ty_python_semantic::check_file(&ordinary_db, ordinary_program_file)
                .unwrap()
                .is_empty()
        );
        let Some(Stmt::Assign(assignment)) = ordinary_prepared.parsed_module().syntax().body.last()
        else {
            anyhow::bail!("fixture ends with a shared assignment");
        };
        let model = SemanticModel::new(&ordinary_db, ordinary_program_file);
        for target in &assignment.targets {
            assert_eq!(
                target.inferred_type(&model),
                Some(Type::bool_literal(expected))
            );
        }
    }
    Ok(())
}

#[test]
fn cold_middle_boolean_operands_complete_after_same_revision_refusal() -> anyhow::Result<()> {
    for (operator, tail, interrupt) in [
        ("and", "True", true),
        ("or", "False", true),
        ("and", "True", false),
        ("or", "False", false),
    ] {
        let source =
            format!("def choose(value):\n    return value {operator} value {operator} {tail}\n");
        let db = database(&source)?;
        let prepared = prepare(&db)?;
        let Some(Stmt::FunctionDef(function)) = prepared.parsed_module().syntax().body.first()
        else {
            anyhow::bail!("fixture begins with a function");
        };
        let Some(Stmt::Return(statement)) = function.body.first() else {
            anyhow::bail!("fixture function returns a Boolean expression");
        };
        let Some(ruff_python_ast::Expr::BoolOp(boolean)) = statement.value.as_deref() else {
            anyhow::bail!("fixture return value is a Boolean expression");
        };
        let Some(middle) = boolean.values.get(1) else {
            anyhow::bail!("fixture has a middle operand");
        };
        let revision = salsa::plumbing::current_revision(&db);
        let initial_policy = if interrupt {
            AnalysisPolicy {
                semantic_work_limit: 100_000,
                ..funded()
            }
        } else {
            funded()
        };
        let initial = expression_type_with_policy(&prepared, middle.into(), &initial_policy);
        if interrupt {
            assert_eq!(
                initial,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
                "{operator} operand with the limited policy",
            );
        } else {
            assert!(
                matches!(&initial, Ok(AnalysisOutcome::Complete(_))),
                "{operator} cold operand did not complete: {initial:?}",
            );
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(()),
        );
        let outcome = expression_type_with_policy(&prepared, middle.into(), &funded());
        let Ok(AnalysisOutcome::Complete(inferred)) = outcome else {
            anyhow::bail!("{operator} operand retry did not complete: {outcome:?}");
        };
        if !interrupt {
            assert_eq!(initial, Ok(AnalysisOutcome::Complete(inferred)));
        }
        assert_ne!(inferred, Type::unknown());
        assert_ne!(inferred, Type::Never);

        // The ordinary read follows controlled completion, so it cannot supply a warm result.
        let file = system_path_to_file(&db, "/src/main.py")?;
        let model = SemanticModel::new(&db, Db::program_file(&db, file));
        assert_eq!(middle.inferred_type(&model), Some(inferred));
        assert_eq!(
            expression_type_with_policy(&prepared, middle.into(), &funded()),
            Ok(AnalysisOutcome::Complete(inferred)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_eq!(
            salsa::prepared_source_probe::try_with_preparation(&db, || ()),
            Ok(())
        );
    }
    Ok(())
}

fn refused_boolean_retries(
    limited: AnalysisPolicy,
    reason: AnalysisIncomplete,
) -> anyhow::Result<()> {
    for (source, value) in [
        ("left = right = True\n", true),
        ("left = right = False\n", false),
    ] {
        let db = database(source)?;
        let prepared = prepare(&db)?;
        let expression = expression(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &limited),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: (),
            }),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        let expected = Type::bool_literal(value);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(ordinary_type(&db), expected);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

#[test]
fn cold_boolean_work_refusal_allows_same_revision_retry() -> anyhow::Result<()> {
    refused_boolean_retries(
        AnalysisPolicy {
            semantic_work_limit: 0,
            ..funded()
        },
        AnalysisIncomplete::WorkLimit,
    )
}

#[test]
fn cold_boolean_allocation_refusal_allows_same_revision_retry() -> anyhow::Result<()> {
    refused_boolean_retries(
        AnalysisPolicy {
            requested_bytes_limit: 0,
            ..funded()
        },
        AnalysisIncomplete::RequestedAllocationLimit,
    )
}

const FUNCTION_VALUE_SOURCES: [&str; 2] = [
    "def choose(value):\n    return value\nleft = right = choose\n",
    "def choose(value: bool = True) -> bool:\n    return value\nleft = right = choose\n",
];

#[test]
fn cold_class_value_completes_after_same_revision_refusal() -> anyhow::Result<()> {
    let db = database("class Product:\n    pass\nleft = right = Product\n")?;
    let prepared = prepare(&db)?;
    let expression = expression(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: 0,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: 0,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            }),
        );
    }
    let result = expression_type_with_policy(&prepared, expression, &funded());
    let Ok(AnalysisOutcome::Complete(inferred)) = result else {
        panic!("{result:?}");
    };
    assert!(inferred.as_class_literal().is_some());
    assert_eq!(ordinary_type(&db), inferred);
    assert_eq!(
        expression_type_with_policy(&prepared, expression, &funded()),
        Ok(AnalysisOutcome::Complete(inferred)),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

#[test]
fn cold_function_values_complete_with_canonical_types() -> anyhow::Result<()> {
    for source in FUNCTION_VALUE_SOURCES {
        let db = database(source)?;
        let prepared = prepare(&db)?;
        let expression = expression(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        let outcome = expression_type_with_policy(&prepared, expression, &funded())
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let AnalysisOutcome::Complete(inferred) = outcome else {
            panic!("expected a completed function value, got {outcome:?}");
        };
        assert!(inferred.as_function_literal().is_some());
        assert_eq!(ordinary_type(&db), inferred);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &funded()),
            Ok(AnalysisOutcome::Complete(inferred)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        // Interned handles belong to their database, so compare the independently inferred kind.
        let ordinary_db = database(source)?;
        assert!(ordinary_type(&ordinary_db).as_function_literal().is_some());
    }
    Ok(())
}

fn refused_function_retries(
    limited: AnalysisPolicy,
    reason: AnalysisIncomplete,
) -> anyhow::Result<()> {
    for source in FUNCTION_VALUE_SOURCES {
        let db = database(source)?;
        let prepared = prepare(&db)?;
        let expression = expression(&prepared);
        let revision = salsa::plumbing::current_revision(&db);
        assert_eq!(
            expression_type_with_policy(&prepared, expression, &limited),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: (),
            }),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        let outcome = expression_type_with_policy(&prepared, expression, &funded())
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let AnalysisOutcome::Complete(inferred) = outcome else {
            panic!("expected a completed retry, got {outcome:?}");
        };
        assert!(inferred.as_function_literal().is_some());
        assert_eq!(ordinary_type(&db), inferred);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

#[test]
fn cold_function_work_refusal_allows_same_revision_retry() -> anyhow::Result<()> {
    refused_function_retries(
        AnalysisPolicy {
            semantic_work_limit: 0,
            ..funded()
        },
        AnalysisIncomplete::WorkLimit,
    )
}

#[test]
fn cold_function_allocation_refusal_allows_same_revision_retry() -> anyhow::Result<()> {
    refused_function_retries(
        AnalysisPolicy {
            requested_bytes_limit: 0,
            ..funded()
        },
        AnalysisIncomplete::RequestedAllocationLimit,
    )
}

#[test]
fn cold_calls_complete_and_reuse_their_canonical_results() -> anyhow::Result<()> {
    for source in [
        "def choose(value):\n    return value\nchoose(True)\n",
        "def choose(value):\n    return value\nchoose(value=True)\n",
        "def choose(value):\n    return value\nchoose(choose(True))\n",
    ] {
        check_cold_call(source)?;
    }
    Ok(())
}

fn check_cold_call(source: &str) -> anyhow::Result<()> {
    let db = database(source)?;
    let prepared = prepare(&db)?;
    let module = prepared.parsed_module();
    let Some(Stmt::Expr(statement)) = module.syntax().body.last() else {
        panic!("fixture ends with a call");
    };
    let call = statement.value.as_call_expr().unwrap();
    let expression = statement.value.as_ref().into();
    let callee = call.func.as_ref().into();
    let revision = salsa::plumbing::current_revision(&db);
    let complete = Ok(AnalysisOutcome::Complete(Type::unknown()));
    assert_eq!(
        expression_type_with_policy(&prepared, expression, &funded()),
        complete,
    );

    let outcome = expression_type_with_policy(&prepared, callee, &funded())
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let AnalysisOutcome::Complete(inferred) = outcome else {
        panic!("expected the completed callee, got {outcome:?}");
    };
    assert!(inferred.as_function_literal().is_some());
    assert_eq!(
        expression_type_with_policy(&prepared, expression, &funded()),
        complete,
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

#[test]
fn preparation_refuses_installed_attempts_before_reading_source() -> anyhow::Result<()> {
    for refused in [false, true] {
        for foreign in [false, true] {
            let db = database("left = right = True\n")?;
            let other = database("left = right = False\n")?;
            let target = if foreign { &other } else { &db };
            let file = system_path_to_file(target, "/src/main.py")?;
            let captured = salsa::prepared_source_probe::capture(&db, || {
                salsa::attempt_probe::try_with_attempt(&db, 0, || {
                    if refused {
                        assert_eq!(
                            salsa::attempt_probe::charge(&db, 1),
                            Err(salsa::attempt_probe::Incomplete::Allowance)
                        );
                    }
                    assert_eq!(
                        prepare_file(target, file).err(),
                        Some(salsa::prepared_source_probe::PreparationError::ActiveAttempt)
                    );
                })
            })
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            assert!(captured.reads.is_empty());
            assert_eq!(
                captured.value,
                Ok(if refused {
                    salsa::attempt_probe::AttemptOutcome::Incomplete(
                        salsa::attempt_probe::Incomplete::Allowance,
                    )
                } else {
                    salsa::attempt_probe::AttemptOutcome::Complete(())
                })
            );
            let prepared = prepare(target)?;
            assert_eq!(
                expression_type_with_policy(&prepared, expression(&prepared), &funded()),
                Ok(AnalysisOutcome::Complete(Type::bool_literal(!foreign)))
            );
        }
    }
    Ok(())
}

#[test]
fn fresh_preparation_uses_changed_source_and_settings() -> anyhow::Result<()> {
    let fs = MemoryFileSystem::with_current_directory("/src");
    fs.write_file("/src/main.py", "left = right = True\n")?;
    fs.write_file(
        "/src/ty.toml",
        "[analysis]\nrespect-type-ignore-comments = true\n",
    )?;
    let system = InMemorySystem::from_memory_fs(fs.clone());
    let metadata = ProjectMetadata::discover(&SystemPathBuf::from("/src"), &system)?;
    let mut db = ProjectDatabase::fallible(metadata, system)?;
    let file = system_path_to_file(&db, "/src/main.py")?;
    let revision = salsa::plumbing::current_revision(&db);
    {
        let prepared = prepare(&db)?;
        assert_eq!(
            expression_type_with_policy(&prepared, expression(&prepared), &funded()),
            Ok(AnalysisOutcome::Complete(Type::bool_literal(true)))
        );
    }

    fs.write_file("/src/main.py", "left = right = False\n")?;
    db.apply_changes(&[ChangeEvent::file_content_changed(SystemPathBuf::from(
        "/src/main.py",
    ))]);
    fs.write_file(
        "/src/ty.toml",
        "[analysis]\nrespect-type-ignore-comments = false\n",
    )?;
    db.apply_changes(&[ChangeEvent::file_content_changed(SystemPathBuf::from(
        "/src/ty.toml",
    ))]);
    db.set_check_mode(CheckMode::OpenFiles);
    db.project().open_file(&mut db, file);

    let prepared = prepare(&db)?;
    assert_ne!(salsa::plumbing::current_revision(&db), revision);
    assert!(!db.analysis_settings(file).respect_type_ignore_comments);
    assert!(db.is_open_file(file));
    assert_eq!(
        expression_type_with_policy(&prepared, expression(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::bool_literal(false)))
    );
    Ok(())
}

#[test]
fn cold_file_check_completes_through_the_library_entry() -> anyhow::Result<()> {
    for source in [
        "def choose(value):\n    return value\nchoose(True)\n",
        "def choose(value):\n    return value\nchoose(value=True)\n",
        "def choose(value):\n    return value\nchoose(choose(True))\n",
    ] {
        let db = database(source)?;
        let prepared = prepare(&db)?;
        let revision = salsa::plumbing::current_revision(&db);
        let result = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = result else {
            panic!("{result:?}");
        };
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        let file = system_path_to_file(&db, "/src/main.py")?;
        let ordinary = ty_python_semantic::check_file(&db, Db::program_file(&db, file)).unwrap();
        assert_eq!(diagnostics, ordinary);
        let retry = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
        assert!(
            matches!(retry, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty())
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

fn reusable_property_values<'db>(
    db: &'db ProjectDatabase,
    prepared: &PreparedAnalysisFile<'db>,
) -> anyhow::Result<(Type<'db>, Type<'db>)> {
    let [Stmt::FunctionDef(function), Stmt::Assign(assignment)] =
        prepared.parsed_module().syntax().body.as_slice()
    else {
        anyhow::bail!("fixture defines a getter and assigns its property");
    };
    let [target] = assignment.targets.as_slice() else {
        anyhow::bail!("fixture assigns the property to one name");
    };
    let file = system_path_to_file(db, "/src/main.py")?;
    let model = SemanticModel::new(db, Db::program_file(db, file));
    let Some(getter @ Type::FunctionLiteral(_)) = function.inferred_type(&model) else {
        anyhow::bail!("getter definition must bind a function literal");
    };
    let Some(ty @ Type::PropertyInstance(property)) = target.inferred_type(&model) else {
        anyhow::bail!("assignment must bind a property");
    };
    assert_eq!(property.getter(db), Some(getter));
    assert_eq!(property.setter(db), None);
    assert_eq!(property.deleter(db), None);
    Ok((getter, ty))
}

/// A cold file check constructs a reusable property that retains the supplied getter.
/// Rechecking the file reuses its completed scope memos in the same revision.
#[test]
fn cold_reusable_property_file_check_preserves_the_canonical_getter() -> anyhow::Result<()> {
    let source = "def getter(instance):\n    return True\nready = property(getter)\n";
    let db = database(source)?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    let cold =
        salsa::prepared_source_probe::capture(&db, || check_file_with_policy(&prepared, &funded()))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = &cold.value else {
        anyhow::bail!("cold reusable property file check: {:?}", cold.value);
    };
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    cold.check_root_reads()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    salsa::prepared_source_probe::assert_no_active_attempt();
    let (getter, property) = reusable_property_values(&db, &prepared)?;

    let ordinary_db = database(source)?;
    let ordinary_prepared = prepare(&ordinary_db)?;
    let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
    let ordinary_program_file = Db::program_file(&ordinary_db, ordinary_file);
    let ordinary = ty_python_semantic::check_file(&ordinary_db, ordinary_program_file)
        .map_err(|diagnostic| anyhow::anyhow!("{diagnostic:?}"))?;
    assert_eq!(diagnostics, &ordinary);
    let (ordinary_getter, ordinary_property) =
        reusable_property_values(&ordinary_db, &ordinary_prepared)?;
    let file = system_path_to_file(&db, "/src/main.py")?;
    let env = ProgramEnvironment::from_file(Db::program_file(&db, file));
    let ordinary_env = ProgramEnvironment::from_file(ordinary_program_file);
    for (actual, expected) in [(getter, ordinary_getter), (property, ordinary_property)] {
        assert_eq!(
            actual.display(&db, &env).to_string(),
            expected.display(&ordinary_db, &ordinary_env).to_string(),
        );
    }

    let retry =
        salsa::prepared_source_probe::capture(&db, || check_file_with_policy(&prepared, &funded()))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(retry.value, cold.value);
    let mut reused_scopes = 0;
    for read in &retry.reads {
        if salsa::Database::ingredient_debug_name(&db, read.key.ingredient_index())
            == "infer_scope_types_impl"
        {
            assert_eq!(read.status, salsa::prepared_source_probe::Status::Final);
            assert!(cold.reads.iter().any(|cold_read| {
                cold_read.key == read.key
                    && cold_read.memo_address == read.memo_address
                    && cold_read.status == salsa::prepared_source_probe::Status::Final
            }));
            reused_scopes += 1;
        }
    }
    assert!(reused_scopes > 0);
    assert_eq!(
        reusable_property_values(&db, &prepared)?,
        (getter, property)
    );
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(()),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

const PROPERTY_FILE: &str =
    "class Example:\n    @property\n    def ready(self):\n        return True\n";

/// A cold file check constructs the property's getter and publishes the ordinary definition memo.
/// Rechecking the file reuses its completed scope memos in the same revision.
#[test]
fn cold_property_file_check_preserves_the_canonical_getter() -> anyhow::Result<()> {
    let db = database(PROPERTY_FILE)?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    let cold =
        salsa::prepared_source_probe::capture(&db, || check_file_with_policy(&prepared, &funded()))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = &cold.value else {
        anyhow::bail!("cold property file check: {:?}", cold.value);
    };
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    cold.check_root_reads()
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    salsa::prepared_source_probe::assert_no_active_attempt();

    let [Stmt::ClassDef(class)] = prepared.parsed_module().syntax().body.as_slice() else {
        anyhow::bail!("fixture contains one class");
    };
    let [Stmt::FunctionDef(function)] = class.body.as_slice() else {
        anyhow::bail!("fixture class contains one property getter");
    };
    let file = system_path_to_file(&db, "/src/main.py")?;
    let model = SemanticModel::new(&db, Db::program_file(&db, file));
    let canonical = salsa::prepared_source_probe::capture(&db, || function.inferred_type(&model))
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let Some(Type::PropertyInstance(property)) = canonical.value else {
        anyhow::bail!(
            "expected the property's canonical type, got {:?}",
            canonical.value
        );
    };
    let Some(getter @ Type::FunctionLiteral(_)) = property.getter(&db) else {
        anyhow::bail!("expected a function-literal getter");
    };
    assert_eq!(property.setter(&db), None);
    assert_eq!(property.deleter(&db), None);
    let definition_read = canonical
        .reads
        .iter()
        .find(|read| {
            salsa::Database::ingredient_debug_name(&db, read.key.ingredient_index())
                == "infer_definition_types"
        })
        .ok_or_else(|| anyhow::anyhow!("ordinary property lookup must read its definition memo"))?;
    assert_eq!(
        definition_read.status,
        salsa::prepared_source_probe::Status::Final
    );
    assert!(cold.reads.iter().any(|read| {
        read.key == definition_read.key
            && read.memo_address == definition_read.memo_address
            && read.status == salsa::prepared_source_probe::Status::Final
    }));

    let ordinary_db = database(PROPERTY_FILE)?;
    let ordinary_prepared = prepare(&ordinary_db)?;
    let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
    let ordinary_program_file = Db::program_file(&ordinary_db, ordinary_file);
    let ordinary = ty_python_semantic::check_file(&ordinary_db, ordinary_program_file)
        .map_err(|diagnostic| anyhow::anyhow!("{diagnostic:?}"))?;
    assert_eq!(diagnostics, &ordinary);
    let [Stmt::ClassDef(ordinary_class)] =
        ordinary_prepared.parsed_module().syntax().body.as_slice()
    else {
        anyhow::bail!("ordinary fixture contains one class");
    };
    let [Stmt::FunctionDef(ordinary_function)] = ordinary_class.body.as_slice() else {
        anyhow::bail!("ordinary fixture class contains one property getter");
    };
    let ordinary_model = SemanticModel::new(&ordinary_db, ordinary_program_file);
    let Some(Type::PropertyInstance(ordinary_property)) =
        ordinary_function.inferred_type(&ordinary_model)
    else {
        anyhow::bail!("ordinary lookup must return a property");
    };
    let Some(ordinary_getter @ Type::FunctionLiteral(_)) = ordinary_property.getter(&ordinary_db)
    else {
        anyhow::bail!("ordinary property must retain a function-literal getter");
    };
    assert_eq!(
        getter
            .display(&db, &model.program_environment())
            .to_string(),
        ordinary_getter
            .display(&ordinary_db, &ordinary_model.program_environment())
            .to_string(),
    );

    let retry =
        salsa::prepared_source_probe::capture(&db, || check_file_with_policy(&prepared, &funded()))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(retry.value, cold.value);
    let mut reused_scopes = 0;
    for read in &retry.reads {
        if salsa::Database::ingredient_debug_name(&db, read.key.ingredient_index())
            == "infer_scope_types_impl"
        {
            assert_eq!(read.status, salsa::prepared_source_probe::Status::Final);
            assert!(cold.reads.iter().any(|cold_read| {
                cold_read.key == read.key && cold_read.memo_address == read.memo_address
            }));
            reused_scopes += 1;
        }
    }
    assert!(reused_scopes > 0);
    assert_eq!(
        function.inferred_type(&model),
        Some(Type::PropertyInstance(property))
    );
    assert_eq!(property.getter(&db), Some(getter));
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(()),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

/// Cancelling a property file check after its first scope merge leaves it retryable in the same revision.
#[test]
#[cfg(feature = "testing")]
fn cancelled_property_file_check_retries_through_the_library_entry() -> anyhow::Result<()> {
    let db = database(PROPERTY_FILE)?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    let (result, cancelled) =
        ty_python_semantic::analysis::testing::with_scope_merge_cancellation(|| {
            salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                check_file_with_policy(&prepared, &funded())
            }))
        });
    assert!(
        cancelled,
        "cancellation must follow a completed scope merge: {result:?}"
    );
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(()),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let result = check_file_with_policy(&prepared, &funded());
    let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = result else {
        anyhow::bail!("expected a completed property retry, got {result:?}");
    };
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let ordinary_db = database(PROPERTY_FILE)?;
    let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
    let ordinary =
        ty_python_semantic::check_file(&ordinary_db, Db::program_file(&ordinary_db, ordinary_file))
            .map_err(|diagnostic| anyhow::anyhow!("{diagnostic:?}"))?;
    assert_eq!(diagnostics, ordinary);
    assert_eq!(
        check_file_with_policy(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete(Ok(diagnostics))),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

#[test]
#[cfg(feature = "testing")]
fn cancelled_file_check_retries_through_the_library_entry() -> anyhow::Result<()> {
    let source = "def choose(value):\n    return value\nchoose(choose(True))\n";
    let db = database(source)?;
    let prepared = prepare(&db)?;
    let revision = salsa::plumbing::current_revision(&db);
    let (result, cancelled) =
        ty_python_semantic::analysis::testing::with_scope_merge_cancellation(|| {
            salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded())
            }))
        });
    assert!(
        cancelled,
        "cancellation must follow a completed scope merge"
    );
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(&db, || ()),
        Ok(()),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let result = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
    let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = result else {
        anyhow::bail!("expected a completed retry, got {result:?}");
    };
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    salsa::prepared_source_probe::assert_no_active_attempt();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);

    let ordinary_db = database(source)?;
    let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
    let ordinary =
        ty_python_semantic::check_file(&ordinary_db, Db::program_file(&ordinary_db, ordinary_file))
            .map_err(|diagnostic| anyhow::anyhow!("{diagnostic:?}"))?;
    assert_eq!(diagnostics, ordinary);
    assert_eq!(
        ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete(Ok(diagnostics))),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

#[test]
fn cold_boolean_file_retries_reuse_structural_preparation() -> anyhow::Result<()> {
    for (operator, tail) in [("and", "True"), ("or", "False")] {
        let db = database(&format!(
            "def choose(value):\n    return value {operator} value {operator} {tail}\n"
        ))?;
        let prepared = prepare(&db)?;
        let cloned = prepared.clone();
        let revision = salsa::plumbing::current_revision(&db);
        let first = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
        assert!(
            matches!(
                first,
                Ok(AnalysisOutcome::Complete(Ok(_)))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        ..
                    })
            ),
            "{operator}: {first:?}",
        );
        salsa::prepared_source_probe::try_with_preparation(&db, || ())
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        // Retained preparation and completed queries let retries advance even when newly
        // reached semantic work exhausts another attempt.
        for (attempt, handle) in [&cloned, &prepared, &cloned].into_iter().enumerate() {
            let retry = ty_python_semantic::analysis::check_file_with_policy(handle, &funded());
            assert!(
                matches!(
                    retry,
                    Ok(AnalysisOutcome::Complete(Ok(ref diagnostics))) if diagnostics.is_empty()
                ) || (attempt == 0
                    && matches!(
                        retry,
                        Ok(AnalysisOutcome::Incomplete {
                            reason: AnalysisIncomplete::WorkLimit,
                            ..
                        })
                    )),
                "{operator}: {retry:?}",
            );
            salsa::prepared_source_probe::try_with_preparation(&db, || ())
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
        let file = system_path_to_file(&db, "/src/main.py")?;
        let ordinary = ty_python_semantic::check_file(&db, Db::program_file(&db, file))
            .map_err(|diagnostic| anyhow::anyhow!("{diagnostic:?}"))?;
        assert!(ordinary.is_empty(), "{operator}: {ordinary:?}");
    }
    Ok(())
}

#[test]
fn cold_class_file_refusal_retries_through_the_library_entry() -> anyhow::Result<()> {
    for interrupt in [false, true] {
        let db = database("class Product:\n    pass\nProduct\n")?;
        let prepared = prepare(&db)?;
        let revision = salsa::plumbing::current_revision(&db);
        if interrupt {
            let policy = AnalysisPolicy {
                semantic_work_limit: 100_000,
                ..funded()
            };
            assert_eq!(
                ty_python_semantic::analysis::check_file_with_policy(&prepared, &policy),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            );
            assert_eq!(
                salsa::prepared_source_probe::try_with_preparation(&db, || ()),
                Ok(()),
            );
        }
        let result = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
        let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = result else {
            panic!("{result:?}");
        };
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let file = system_path_to_file(&db, "/src/main.py")?;
        let ordinary = ty_python_semantic::check_file(&db, Db::program_file(&db, file)).unwrap();
        assert_eq!(diagnostics, ordinary);
        let repeated = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
        assert!(
            matches!(repeated, Ok(AnalysisOutcome::Complete(Ok(diagnostics))) if diagnostics.is_empty())
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
    Ok(())
}

#[test]
fn refused_file_check_retries_in_the_same_revision() -> anyhow::Result<()> {
    for source in [
        "def choose(value):\n    return value\nchoose(True)\n",
        "def choose(value):\n    return value\nchoose(value=True)\n",
        "def choose(value):\n    return value\nchoose(choose(True))\n",
    ] {
        for (policy, reason) in [
            (
                AnalysisPolicy {
                    semantic_work_limit: 0,
                    ..funded()
                },
                AnalysisIncomplete::WorkLimit,
            ),
            (
                AnalysisPolicy {
                    semantic_work_limit: 50_000,
                    ..funded()
                },
                AnalysisIncomplete::WorkLimit,
            ),
            (
                AnalysisPolicy {
                    requested_bytes_limit: 0,
                    ..funded()
                },
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
            (
                AnalysisPolicy {
                    requested_bytes_limit: 64 * 1024,
                    ..funded()
                },
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
        ] {
            let db = database(source)?;
            let prepared = prepare(&db)?;
            let revision = salsa::plumbing::current_revision(&db);
            assert_eq!(
                ty_python_semantic::analysis::check_file_with_policy(&prepared, &policy),
                Ok(AnalysisOutcome::Incomplete {
                    reason,
                    completed: ()
                }),
                "{source} with {policy:?}",
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_eq!(
                salsa::prepared_source_probe::try_with_preparation(&db, || ()),
                Ok(()),
            );

            let retry = ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded());
            let Ok(AnalysisOutcome::Complete(Ok(diagnostics))) = retry else {
                anyhow::bail!("expected a completed retry, got {retry:?}");
            };
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            let file = system_path_to_file(&db, "/src/main.py")?;
            let program_file = Db::program_file(&db, file);
            let ordinary_db = database(source)?;
            let ordinary_file = system_path_to_file(&ordinary_db, "/src/main.py")?;
            let ordinary = ty_python_semantic::check_file(
                &ordinary_db,
                Db::program_file(&ordinary_db, ordinary_file),
            )
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            assert_eq!(diagnostics, ordinary);

            let module = prepared.parsed_module();
            let Some(Stmt::Expr(statement)) = module.syntax().body.last() else {
                anyhow::bail!("fixture ends with a call");
            };
            let ordinary_type = statement
                .value
                .inferred_type(&SemanticModel::new(&db, program_file))
                .ok_or_else(|| anyhow::anyhow!("call has no inferred type"))?;
            assert_eq!(ordinary_type, Type::unknown());
            assert_eq!(
                expression_type_with_policy(&prepared, statement.value.as_ref().into(), &funded()),
                Ok(AnalysisOutcome::Complete(ordinary_type)),
            );
            assert_eq!(
                ty_python_semantic::analysis::check_file_with_policy(&prepared, &funded()),
                Ok(AnalysisOutcome::Complete(Ok(ordinary))),
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
        }
    }
    Ok(())
}
