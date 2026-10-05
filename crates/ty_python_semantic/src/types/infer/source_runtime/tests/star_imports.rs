use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use salsa::execution_probe::FinalSourceMemo;

use super::*;
use crate::dunder_all::{dunder_all_names, dunder_all_names_ingredient};
use crate::place::{
    Definedness, Place, Provenance, PublicTypePolicy, RequiresExplicitReExport, TypeOrigin,
    imported_symbol,
};
use crate::reachability::source::{
    OrdinaryReachabilityEffects, ReachabilityFacts, analyze_single_sync, infallible,
};
use crate::types::DescriptorOperation;

#[derive(Clone, Copy)]
enum Request<'name, 'db> {
    Predicate(Predicate<'db>),
    Imported {
        file: Option<ProgramFile<'db>>,
        name: &'name str,
        reexport: Option<RequiresExplicitReExport>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value<'db> {
    Predicate(Truthiness),
    Place(PlaceAndQualifiers<'db>),
}

fn database(source: &str, modules: &[(&str, &str)]) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    for (path, source) in modules {
        db.write_file(*path, *source).unwrap();
    }
    db
}

fn predicate<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> Predicate<'db> {
    let table = prepared.semantic_index().place_table(FileScopeId::global());
    *prepared.semantic_index().use_def_map(FileScopeId::global()).predicates().iter()
        .find(|predicate| matches!(predicate.node,
            PredicateNode::StarImportPlaceholder(star) if table.symbol(star.symbol_id(db)).name().as_str() == name))
        .unwrap()
}

fn ordinary_predicate<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> Truthiness {
    infallible(analyze_single_sync(
        &ProgramEnvironment::from_file(prepared.program_file()),
        &predicate(db, prepared, name),
        ReachabilityFacts,
        &OrdinaryReachabilityEffects::new(db),
    ))
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    request: Request<'_, 'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Value<'db>>, AnalysisFailure> {
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
        let weak = Rc::downgrade(&routes);
        let query_routes = Rc::clone(&routes);
        let values = &values;
        let result = catch_unwind(AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes: query_routes,
                    values,
                };
                let effects = SourceEffects::new(&access, session.program());
                let env = ProgramEnvironment::from_file(prepared.program_file());
                match request {
                    Request::Predicate(predicate) => {
                        crate::reachability::source::analyze_single_with(
                            &env,
                            &predicate,
                            ReachabilityFacts,
                            &effects,
                        )
                        .await
                        .map(Value::Predicate)
                    }
                    Request::Imported {
                        file,
                        name,
                        reexport,
                    } => crate::place::imported_symbol_with(
                        session.db(),
                        &env,
                        &effects,
                        file,
                        name,
                        reexport,
                    )
                    .await
                    .map(Value::Place),
                }
            })
        }));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(Rc::strong_count(&routes), 1);
        drop(routes);
        assert!(weak.upgrade().is_none());
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

#[test]
fn cold_star_predicates_preserve_membership_and_ignore_polarity() {
    let source = "value = False\nfrom dependency import *\nleft = right = value\n";
    for (dependency, expected) in [
        ("value = True\n", Truthiness::AlwaysTrue),
        (
            "value = True\n__all__ = ['value']\n",
            Truthiness::AlwaysTrue,
        ),
        ("value = True\n__all__ = []\n", Truthiness::AlwaysFalse),
        (
            "value = True\n__all__ = ['other']\n",
            Truthiness::AlwaysFalse,
        ),
        ("value = True\n__all__ = [1]\n", Truthiness::AlwaysTrue),
    ] {
        for positive in [true, false] {
            let db = database(source, &[("src/dependency.py", dependency)]);
            let prepared = prepare(&db);
            let mut predicate = predicate(&db, &prepared, "value");
            predicate.is_positive = positive;
            observations::reset(None);
            let cold = capture(&db, || {
                controlled(&prepared, Request::Predicate(predicate), &funded())
            })
            .unwrap();
            assert_eq!(
                cold.value,
                Ok(AnalysisOutcome::Complete(Value::Predicate(expected))),
                "{dependency}, positive={positive}"
            );
            cold.check_root_reads().unwrap();
            let ordinary_db = database(source, &[("src/dependency.py", dependency)]);
            let ordinary_prepared = prepare(&ordinary_db);
            assert_eq!(
                ordinary_predicate(&ordinary_db, &ordinary_prepared, "value"),
                expected
            );
            assert_no_active_attempt();
        }
    }
}

#[test]
fn excluded_star_names_skip_place_inference_and_publish_canonical_exports() {
    let db = database(
        "value = False\nfrom dependency import *\nleft = right = value\n",
        &[("src/dependency.py", "value = unsupported()\n__all__ = []\n")],
    );
    let prepared = prepare(&db);
    let predicate = predicate(&db, &prepared, "value");
    let PredicateNode::StarImportPlaceholder(star) = predicate.node else {
        panic!("star predicate")
    };
    let dependency = star.referenced_file(&db);
    observations::reset(None);
    let cold = capture(&db, || {
        controlled(&prepared, Request::Predicate(predicate), &funded())
    })
    .unwrap();
    assert_eq!(
        cold.value,
        Ok(AnalysisOutcome::Complete(Value::Predicate(
            Truthiness::AlwaysFalse
        )))
    );
    cold.check_root_reads().unwrap();
    assert!(
        !cold
            .reads
            .iter()
            .any(|read| db.ingredient_debug_name(read.key.ingredient_index()) == "place_by_id")
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            dunder_all_names_ingredient(&db),
            dependency.as_id()
        )
        .is_ok()
    );
    let mut reader = db.clone();
    reader.take_salsa_events();
    assert!(dunder_all_names(&reader, dependency).unwrap().is_empty());
    let events = reader.take_salsa_events();
    assert_function_query_was_not_run_by_name(&reader, "dunder_all_names", None, &events);
    assert_no_active_attempt();
}

#[test]
fn cold_exported_stub_imports_complete_through_canonical_places() {
    for stub in [
        "from dependency import value\n__all__ = ['value']\n",
        "from dependency import value as value\n",
    ] {
        let db = database(
            "left = right = True\n",
            &[
                ("src/exporter.pyi", stub),
                ("src/dependency.py", "value = True\n"),
            ],
        );
        let file = system_path_to_file(&db, "src/exporter.pyi").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        observations::reset(None);
        let cold = capture(&db, || {
            controlled(
                &prepared,
                Request::Imported {
                    file: Some(prepared.program_file()),
                    name: "value",
                    reexport: None,
                },
                &funded(),
            )
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete(Value::Place(actual))) = cold.value else {
            panic!("{stub}: {:?}", cold.value)
        };
        assert!(
            matches!(actual.place, Place::Defined(place) if place.ty == Type::bool_literal(true) && place.definedness == Definedness::AlwaysDefined)
        );
        cold.check_root_reads().unwrap();
        let ordinary_db = database(
            "left = right = True\n",
            &[
                ("src/exporter.pyi", stub),
                ("src/dependency.py", "value = True\n"),
            ],
        );
        let file = system_path_to_file(&ordinary_db, "src/exporter.pyi").unwrap();
        let ordinary_prepared = prepare_file(&ordinary_db, file).unwrap();
        let expected = imported_symbol(
            &ordinary_db,
            &ProgramEnvironment::from_file(ordinary_prepared.program_file()),
            Some(ordinary_prepared.program_file()),
            "value",
            None,
        );
        assert!(
            matches!((actual.place, expected.place), (Place::Defined(a), Place::Defined(b)) if a.ty == b.ty && a.definedness == b.definedness && a.origin == b.origin && a.public_type_policy == b.public_type_policy)
        );
        assert_eq!(actual.qualifiers, expected.qualifiers);
        assert_no_active_attempt();
    }
}

#[test]
fn cold_star_imports_preserve_prior_bindings_through_the_library_entry() {
    for (exports, expected) in [("", true), ("__all__ = []\n", false)] {
        let source = "value = False\nfrom dependency import *\nleft = right = value\n";
        let dependency = format!("value = True\n{exports}");
        let db = database(source, &[("src/dependency.py", &dependency)]);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let cold = capture(&db, || {
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
        })
        .unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(Type::bool_literal(expected)))
        );
        cold.check_root_reads().unwrap();
        assert_eq!(
            expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
            cold.value
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn module_special_fallbacks_preserve_namespace_package_types() {
    for namespace in [false, true] {
        for name in ["__file__", "__getattr__", "__builtins__"] {
            let db = database("pass\n", &[]);
            let prepared = prepare(&db);
            let file = (!namespace).then_some(prepared.program_file());
            observations::reset(None);
            let cold = capture(&db, || {
                controlled(
                    &prepared,
                    Request::Imported {
                        file,
                        name,
                        reexport: None,
                    },
                    &funded(),
                )
            })
            .unwrap();
            let Ok(AnalysisOutcome::Complete(Value::Place(actual))) = cold.value else {
                panic!("{name}, namespace={namespace}: {:?}", cold.value)
            };
            let env = ProgramEnvironment::from_file(prepared.program_file());
            let expected = imported_symbol(&db, &env, file, name, None);
            assert_eq!(actual, expected);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn module_fallbacks_retain_exact_unavailable_member_operations_on_retry() {
    for (source, name, operation) in [
        (
            "def condition(): ...\nif condition():\n    value = True\n",
            "value",
            OperationId::MemberLookup(GeneralMemberOperation::NominalEnumMember),
        ),
        (
            "pass\n",
            "__dict__",
            OperationId::Descriptor(DescriptorOperation::ClassMember),
        ),
        (
            "pass\n",
            "__class__",
            OperationId::MemberLookup(GeneralMemberOperation::DunderClass),
        ),
    ] {
        let db = database(source, &[]);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let cold = capture(&db, || {
            controlled(
                &prepared,
                Request::Imported {
                    file: Some(prepared.program_file()),
                    name,
                    reexport: None,
                },
                &funded(),
            )
        })
        .unwrap();
        assert_eq!(
            cold.value,
            Ok(unavailable(operation)),
            "{name}, source={source:?}",
        );
        cold.check_root_reads().unwrap();
        assert_eq!(
            controlled(
                &prepared,
                Request::Imported {
                    file: Some(prepared.program_file()),
                    name,
                    reexport: None
                },
                &funded()
            ),
            cold.value
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// A conditional module binding retains its inferred type and remains possibly undefined when
/// `ModuleType` supplies no fallback member. Lookup checks for that fallback because the assignment
/// may not execute. Cold controlled lookup matches ordinary inference and publishes a canonical
/// member memo reused by ordinary and controlled lookup.
#[test]
fn cold_conditional_module_binding_preserves_definedness_and_reuses_fallback_memos() {
    let source = "def condition(): ...\nif condition():\n    conditional = True\n";
    let db = database(source, &[]);
    let prepared = prepare(&db);
    let file = prepared.program_file();
    let request = Request::Imported {
        file: Some(file),
        name: "conditional",
        reexport: None,
    };
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || controlled(&prepared, request, &funded())).unwrap();
    let Ok(AnalysisOutcome::Complete(Value::Place(actual))) = cold.value else {
        panic!("conditional module binding: {:?}", cold.value);
    };
    let Place::Defined(actual_place) = actual.place else {
        panic!("conditional module binding: {actual:?}");
    };
    assert_eq!(actual_place.ty, Type::bool_literal(true));
    assert_eq!(actual_place.definedness, Definedness::PossiblyUndefined);
    assert_eq!(actual_place.origin, TypeOrigin::Inferred);
    assert_eq!(actual_place.public_type_policy, PublicTypePolicy::Raw);
    assert!(matches!(
        actual_place.provenance,
        Provenance::SingleDefinition(_)
    ));
    assert!(actual.qualifiers.is_empty());
    cold.check_root_reads().unwrap();
    let env = ProgramEnvironment::from_file(file);
    let key = MemberLookupKey::new(
        &db,
        file.program(&db),
        KnownClass::ModuleType.to_instance(&db, &env),
        "conditional",
        MemberLookupPolicy::NO_GETATTR_LOOKUP,
    );
    let member_ingredient = member_lookup_ingredient(&db);
    let member_key = member_ingredient.database_key_index(key.as_id());
    let cold_read = cold
        .reads
        .iter()
        .find(|read| read.key == member_key && read.parent.is_none())
        .unwrap();
    assert!(FinalSourceMemo::certify(&db as &dyn Db, member_ingredient, key.as_id()).is_ok());
    let class_ingredient = crate::types::class_member_lookup_ingredient(&db);
    let class_key = class_ingredient.database_key_index(key.as_id());
    assert!(
        cold.reads
            .iter()
            .any(|read| { read.key == class_key && read.parent == Some(member_key) })
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, class_ingredient, key.as_id()).is_ok());
    assert!(
        find_will_execute_event_by_name(
            &db,
            "member_lookup_with_policy_inner",
            Some(key.as_id()),
            &events_db.take_salsa_events(),
        )
        .is_some()
    );

    let ordinary_db = database(source, &[]);
    let ordinary_prepared = prepare(&ordinary_db);
    let expected = imported_symbol(
        &ordinary_db,
        &ProgramEnvironment::from_file(ordinary_prepared.program_file()),
        Some(ordinary_prepared.program_file()),
        "conditional",
        None,
    );
    let Place::Defined(expected_place) = expected.place else {
        panic!("ordinary conditional module binding: {expected:?}");
    };
    assert_eq!(actual_place.ty, expected_place.ty);
    assert_eq!(actual_place.definedness, expected_place.definedness);
    assert_eq!(actual_place.origin, expected_place.origin);
    assert_eq!(
        actual_place.public_type_policy,
        expected_place.public_type_policy
    );
    assert!(matches!(
        expected_place.provenance,
        Provenance::SingleDefinition(_)
    ));
    assert_eq!(actual.qualifiers, expected.qualifiers);
    let native = capture(&db, || {
        imported_symbol(&db, &env, Some(file), "conditional", None)
    })
    .unwrap();
    native.check_root_reads().unwrap();
    assert_eq!(native.value, actual);
    assert!(
        native.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    observations::reset(None);
    let warm = capture(&db, || controlled(&prepared, request, &funded())).unwrap();
    warm.check_root_reads().unwrap();
    assert_eq!(warm.value, cold.value);
    assert!(
        warm.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    let events = events_db.take_salsa_events();
    for query in [
        "member_lookup_with_policy_inner",
        "class_member_with_policy_inner",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(observations::counts().0, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn interrupted_star_import_inference_cleans_up_and_retries_in_the_same_revision() {
    let source = "value = False\nfrom dependency import *\nleft = right = value\n";
    let modules = [("src/dependency.py", "value = True\n__all__ = ['value']\n")];
    let measured = database(source, &modules);
    let prepared = prepare(&measured);
    observations::reset(None);
    let measured_request = Request::Predicate(predicate(&measured, &prepared, "value"));
    assert_eq!(
        controlled(&prepared, measured_request, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Predicate(
            Truthiness::AlwaysTrue
        )))
    );
    let stored_work = funded().semantic_work_limit - observations::stored_remaining().unwrap();

    for cancel in [false, true] {
        let db = database(source, &modules);
        let prepared = prepare(&db);
        let predicate = predicate(&db, &prepared, "value");
        let PredicateNode::StarImportPlaceholder(star) = predicate.node else {
            panic!("star predicate")
        };
        let dependency = star.referenced_file(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let request = Request::Predicate(predicate);
        observations::reset(if cancel {
            Some(observations::Event::Stored)
        } else {
            None
        });
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: stored_work,
                ..funded()
            }
        };
        let stopped =
            salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, request, &policy)));
        if cancel {
            assert!(
                matches!(stopped, Err(salsa::Cancelled::Local)),
                "{stopped:?}"
            );
        } else {
            assert_eq!(
                stopped.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
        }
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                dunder_all_names_ingredient(&db),
                dependency.as_id()
            )
            .is_ok()
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();
        observations::reset(None);
        assert_eq!(
            controlled(&prepared, request, &funded()),
            Ok(AnalysisOutcome::Complete(Value::Predicate(
                Truthiness::AlwaysTrue
            )))
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}
