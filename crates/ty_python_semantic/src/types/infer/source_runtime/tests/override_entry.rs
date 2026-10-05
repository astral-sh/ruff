use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::prepared_source_probe::{Read, Status};

use super::*;

const METHOD_MEMBER: &str = "class Product:\n    def ready(self):\n        pass\n";

fn database(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db
}

fn assert_parent_unpublished(
    db: &TestDb,
    prepared: &PreparedAnalysisFile<'_>,
) -> salsa::DatabaseKeyIndex {
    let module = FileScopeId::global().to_scope_id(db, prepared.program_file());
    let ingredient = scope_inference_ingredient(db);
    assert_eq!(
        FinalSourceMemo::certify(db as &dyn Db, ingredient, module.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    ingredient.database_key_index(module.as_id())
}

fn member_key<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> MemberLookupKey<'db> {
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture class");
    };
    let definition = prepared.semantic_index().expect_single_definition(class);
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id(),
        )
        .is_ok()
    );
    let Some(class) = infer_definition_types(db, definition).original_class_type(definition) else {
        panic!("completed fixture class");
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    MemberLookupKey::new(
        db,
        prepared.program_file().program(db),
        Type::instance(db, &env, ClassType::NonGeneric(class)),
        name,
        MemberLookupPolicy::default(),
    )
}

fn assert_read(
    reads: &[Read],
    stamp: Stamp,
    key: salsa::DatabaseKeyIndex,
    parent: Option<salsa::DatabaseKeyIndex>,
) -> Read {
    let Some(read) = reads
        .iter()
        .find(|read| read.key == key && read.parent == parent)
    else {
        panic!("missing canonical dependency {parent:?} -> {key:?}: {reads:?}");
    };
    assert_eq!(read.status, Status::Final);
    assert_eq!(read.stamp, stamp);
    *read
}

fn assert_lookup_memos(db: &TestDb, key: MemberLookupKey<'_>) {
    assert!(FinalSourceMemo::certify(db as &dyn Db, member_lookup_ingredient(db), key.as_id()).is_ok());
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            crate::types::class_member_lookup_ingredient(db),
            key.as_id(),
        )
        .is_ok()
    );
}

fn assert_cleanup(db: &TestDb) {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(db, || ()),
        Ok(()),
    );
}

#[test]
fn override_entry_uses_class_mro_for_new_and_instance_lookup_for_methods() {
    for (name, source) in [
        (
            "__new__",
            "class Product:\n    def __new__(cls):\n        pass\n",
        ),
        ("ready", METHOD_MEMBER),
    ] {
        let measured = database(source);
        let measured_prepared = prepare(&measured);
        assert_parent_unpublished(&measured, &measured_prepared);
        observations::reset(None);
        let measured_result = controlled_module(&measured, &measured_prepared, &funded());
        assert!(measured_result.is_ok(), "{measured_result:?}");
        let (_, Some(remaining)) = observations::override_member_lookup_progress() else {
            panic!("{name}: missing returned lookup");
        };
        assert_cleanup(&measured);
        let limited = AnalysisPolicy {
            semantic_work_limit: funded().semantic_work_limit - remaining,
            ..funded()
        };

        let db = database(source);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let parent = assert_parent_unpublished(&db, &prepared);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(None);
        let cold = capture(&db, || controlled_module(&db, &prepared, &limited)).unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
            "{name}",
        );
        assert_eq!(assert_parent_unpublished(&db, &prepared), parent);
        let key = member_key(&db, &prepared, name);
        let member_ingredient = member_lookup_ingredient(&db);
        let member = member_ingredient.database_key_index(key.as_id());
        let events = events_db.take_salsa_events();
        assert_eq!(observations::override_member_lookup_progress(), (1, Some(0)));
        if name == "__new__" {
            assert_eq!(
                FinalSourceMemo::certify(&db as &dyn Db, member_ingredient, key.as_id()).map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
            assert!(!cold.reads.iter().any(|read| read.key == member));
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    crate::types::class_member_lookup_ingredient(&db),
                    key.as_id(),
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
            assert_function_query_was_not_run_by_name(
                &db,
                "member_lookup_with_policy_inner",
                Some(key.as_id()),
                &events,
            );
            let class = crate::types::class_member_lookup_ingredient(&db)
                .database_key_index(key.as_id());
            assert!(
                !events.iter().any(|event| matches!(event.kind,
                    salsa::EventKind::WillExecute { database_key } if database_key == class)),
            );
        } else {
            assert_lookup_memos(&db, key);
            assert_read(&cold.reads, cold.stamp, member, Some(parent));
            assert!(
                find_will_execute_event_by_name(
                    &db,
                    "member_lookup_with_policy_inner",
                    Some(key.as_id()),
                    &events,
                )
                .is_some()
            );
            let class_ingredient = crate::types::class_member_lookup_ingredient(&db);
            assert_read(
                &cold.reads,
                cold.stamp,
                class_ingredient.database_key_index(key.as_id()),
                Some(member),
            );
            assert!(FinalSourceMemo::certify(&db as &dyn Db, class_ingredient, key.as_id()).is_ok());
        }
        assert!(!observations::override_members().0.is_empty());
        assert_cleanup(&db);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

#[test]
fn override_entry_preserves_dunder_class_handling_before_canonical_lookup() {
    let db = database("class Product:\n    __class__ = True\n");
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let parent = assert_parent_unpublished(&db, &prepared);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || check_file_with_policy(&prepared, &funded())).unwrap();
    assert_eq!(
        cold.value,
        Ok(unavailable(OperationId::MemberLookup(
            GeneralMemberOperation::DunderClass,
        ))),
    );
    assert_eq!(observations::override_member_lookup_progress(), (0, None));
    assert_eq!(assert_parent_unpublished(&db, &prepared), parent);
    let key = member_key(&db, &prepared, "__class__");
    let ingredient = member_lookup_ingredient(&db);
    assert!(
        !cold
            .reads
            .iter()
            .any(|read| read.key == ingredient.database_key_index(key.as_id()))
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, key.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "member_lookup_with_policy_inner",
        Some(key.as_id()),
        &events_db.take_salsa_events(),
    );
    assert!(!observations::override_members().0.is_empty());
    assert_cleanup(&db);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[derive(Clone, Copy, Debug)]
enum Stop {
    Work,
    Cancel,
}

#[test]
fn returned_override_lookup_retains_children_and_retries_after_interruption() {
    let measured = database(METHOD_MEMBER);
    let measured_prepared = prepare(&measured);
    assert_parent_unpublished(&measured, &measured_prepared);
    observations::reset(None);
    assert!(matches!(
        controlled_module(&measured, &measured_prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let (count, Some(remaining)) = observations::override_member_lookup_progress() else {
        panic!("method lookup must return before the override continuation");
    };
    assert_eq!(count, 1);
    assert_cleanup(&measured);
    let limited = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - remaining,
        ..funded()
    };

    for stop in [Stop::Work, Stop::Cancel] {
        let db = database(METHOD_MEMBER);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let parent = assert_parent_unpublished(&db, &prepared);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        observations::reset(
            matches!(stop, Stop::Cancel).then_some(observations::Event::OverrideMemberLookupReady),
        );
        let policy = if matches!(stop, Stop::Work) {
            limited
        } else {
            funded()
        };
        let cold = capture(&db, || {
            salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled_module(&db, &prepared, &policy)
            }))
        })
        .unwrap();
        match stop {
            Stop::Work => {
                assert_eq!(
                    cold.value.unwrap(),
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    }),
                );
                assert_eq!(observations::override_member_lookup_progress(), (1, Some(0)));
            }
            Stop::Cancel => {
                assert!(
                    matches!(cold.value, Err(salsa::Cancelled::Local)),
                    "{:?}",
                    cold.value,
                );
            }
        }
        assert_eq!(observations::override_member_lookup_progress().0, 1);
        assert!(!observations::override_members().0.is_empty());
        assert_cleanup(&db);
        match stop {
            Stop::Work => assert_eq!(assert_parent_unpublished(&db, &prepared), parent),
            Stop::Cancel => assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    scope_inference_ingredient(&db),
                    parent.key_index(),
                )
                .is_ok()
            ),
        }
        let key = member_key(&db, &prepared, "ready");
        let member = member_lookup_ingredient(&db).database_key_index(key.as_id());
        let class =
            crate::types::class_member_lookup_ingredient(&db).database_key_index(key.as_id());
        let cold_read = assert_read(&cold.reads, cold.stamp, member, Some(parent));
        assert_read(&cold.reads, cold.stamp, class, Some(member));
        assert_lookup_memos(&db, key);
        let events = events_db.take_salsa_events();
        for query in [member, class] {
            assert!(
                events.iter().any(|event| matches!(event.kind,
                    salsa::EventKind::WillExecute { database_key } if database_key == query)),
                "{stop:?}: {query:?}",
            );
        }
        assert_eq!(salsa::plumbing::current_revision(&db), revision);

        let completed_parent = matches!(stop, Stop::Cancel).then(|| {
            let module = FileScopeId::global().to_scope_id(&db, prepared.program_file());
            let retained = capture(&db, || {
                crate::types::infer::infer_scope_types(&db, module, TypeContext::default())
            })
            .unwrap();
            assert_read(&retained.reads, retained.stamp, parent, None)
        });
        observations::reset(None);
        let retry = capture(&db, || controlled_module(&db, &prepared, &funded())).unwrap();
        assert!(matches!(retry.value, Ok(AnalysisOutcome::Complete(_))));
        let retry_read = if let Some(completed_parent) = completed_parent {
            assert_eq!(observations::override_member_lookup_progress(), (0, None));
            let parent_read = assert_read(&retry.reads, retry.stamp, parent, None);
            assert_eq!(parent_read.stamp, completed_parent.stamp);
            assert_eq!(parent_read.memo_address, completed_parent.memo_address);
            assert!(
                !retry
                    .reads
                    .iter()
                    .any(|read| read.key == member || read.key == class)
            );
            let retained = capture(&db, || {
                crate::types::member_lookup_with_policy_inner(&db, key)
            })
            .unwrap();
            assert_read(&retained.reads, retained.stamp, member, None)
        } else {
            assert_eq!(observations::override_member_lookup_progress().0, 1);
            assert_read(&retry.reads, retry.stamp, member, Some(parent))
        };
        assert_eq!(retry_read.stamp, cold_read.stamp);
        assert_eq!(retry_read.memo_address, cold_read.memo_address);
        assert_lookup_memos(&db, key);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                scope_inference_ingredient(&db),
                parent.key_index(),
            )
            .is_ok()
        );
        let events = events_db.take_salsa_events();
        for query in [member, class] {
            assert!(
                !events.iter().any(|event| matches!(event.kind,
                    salsa::EventKind::WillExecute { database_key } if database_key == query)),
                "{stop:?}: {query:?}",
            );
        }
        assert_eq!(
            find_will_execute_event_by_name(
                &db,
                "infer_scope_types_impl",
                Some(parent.key_index()),
                &events,
            )
            .is_some(),
            matches!(stop, Stop::Work)
        );
        assert_cleanup(&db);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}
