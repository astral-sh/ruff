use salsa::execution_probe::{FinalSourceError, FinalSourceMemo, VerifyResult};
use salsa::prepared_source_probe::Status;
use ty_python_core::finalized_sources::{place_table_ingredient, use_def_map_ingredient};
use ty_python_core::{PlaceTable, place_table, use_def_map};

use super::super::native_values::OutputProfile;
use super::super::scope_maps::{PlaceTableProvider, UseDefMapProvider};
use super::*;
use crate::types::infer::native_values::{self as inference_profiles, observations as comparisons};
use crate::types::infer::{DefinitionInferenceExtra, infer_scope_types};
use crate::types::todo_type;

#[derive(Clone, Copy, Debug)]
enum Region {
    Scope,
    Definition,
    Expression,
}

impl Region {
    fn name(self) -> &'static str {
        match self {
            Self::Scope => "infer_scope_types_impl",
            Self::Definition => "infer_definition_types",
            Self::Expression => "infer_expression_types_impl",
        }
    }

    fn id(self, prepared: &PreparedAnalysisFile<'_>) -> salsa::Id {
        match self {
            Self::Scope => InferScope::Bare(selected_scope(prepared)).as_id(),
            Self::Definition => selected_definition(prepared).as_id(),
            Self::Expression => InferExpression::Bare(selected_expression(prepared)).as_id(),
        }
    }

    fn ordinary<'db>(self, db: &'db dyn Db, prepared: &PreparedAnalysisFile<'db>) -> Memo<'db> {
        match self {
            Self::Scope => Memo::Scope(infer_scope_types(
                db,
                ty_python_core::global_scope(db, prepared.program_file()),
                TypeContext::default(),
            )),
            Self::Definition => {
                Memo::Definition(infer_definition_types(db, selected_definition(prepared)))
            }
            Self::Expression => Memo::Expression(infer_expression_types(
                db,
                selected_expression(prepared),
                TypeContext::default(),
            )),
        }
    }

    fn certify(
        self,
        db: &dyn Db,
        prepared: &PreparedAnalysisFile<'_>,
    ) -> Result<(), FinalSourceError> {
        match self {
            Self::Scope => {
                FinalSourceMemo::certify(db, scope_inference_ingredient(db), self.id(prepared))
                    .map(|_| ())
            }
            Self::Definition => {
                FinalSourceMemo::certify(db, definition_inference_ingredient(db), self.id(prepared))
                    .map(|_| ())
            }
            Self::Expression => {
                FinalSourceMemo::certify(db, expression_inference_ingredient(db), self.id(prepared))
                    .map(|_| ())
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Memo<'db> {
    Scope(&'db ScopeInference<'db>),
    Definition(&'db DefinitionInference<'db>),
    Expression(&'db ExpressionInference<'db>),
}

impl Memo<'_> {
    fn address(self) -> usize {
        match self {
            Self::Scope(value) => std::ptr::from_ref(value).addr(),
            Self::Definition(value) => std::ptr::from_ref(value).addr(),
            Self::Expression(value) => std::ptr::from_ref(value).addr(),
        }
    }

    fn has_diagnostics(self) -> bool {
        match self {
            Self::Scope(value) => value
                .diagnostics()
                .is_some_and(|value| value.into_iter().len() > 0),
            Self::Definition(value) => match value.extra.as_deref() {
                Some(DefinitionInferenceExtra::Diagnostics(diagnostics)) => {
                    diagnostics.as_ref().into_iter().len() > 0
                }
                Some(DefinitionInferenceExtra::Other(extra)) => {
                    (&extra.diagnostics).into_iter().len() > 0
                }
                _ => false,
            },
            Self::Expression(value) => value
                .extra
                .as_deref()
                .is_some_and(|extra| (&extra.diagnostics).into_iter().len() > 0),
        }
    }

    fn has_constraints(self) -> bool {
        match self {
            Self::Scope(value) => value
                .extra
                .as_deref()
                .is_some_and(|extra| !extra.collection_use_constraints.is_empty()),
            Self::Definition(value) => value
                .extra
                .as_deref()
                .and_then(DefinitionInferenceExtra::collection_use_constraints)
                .is_some_and(|constraints| !constraints.is_empty()),
            Self::Expression(value) => value
                .extra
                .as_deref()
                .is_some_and(|extra| !extra.collection_use_constraints.is_empty()),
        }
    }
}

fn selected_expression<'db>(prepared: &PreparedAnalysisFile<'db>) -> Expression<'db> {
    prepared
        .semantic_index()
        .expression(expression_key(prepared))
}

fn selected_scope<'db>(prepared: &PreparedAnalysisFile<'db>) -> ScopeId<'db> {
    prepared.semantic_index().scope_ids().next().unwrap()
}

fn selected_definition<'db>(prepared: &PreparedAnalysisFile<'db>) -> Definition<'db> {
    match prepared.parsed_module().syntax().body.last().unwrap() {
        Stmt::Assign(assignment) => assignment_definition(prepared, assignment),
        Stmt::AnnAssign(assignment) => prepared
            .semantic_index()
            .expect_single_definition(assignment),
        Stmt::FunctionDef(function) => prepared.semantic_index().expect_single_definition(function),
        _ => panic!("fixture definition"),
    }
}

fn controlled_region<'db>(
    region: Region,
    prepared: &PreparedAnalysisFile<'db>,
    previous_revision: salsa::Revision,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<(Memo<'db>, VerifyResult)>, AnalysisFailure> {
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
            let memo = match region {
                Region::Scope => Memo::Scope(
                    access
                        .scope(selected_scope(prepared), TypeContext::default())
                        .await?,
                ),
                Region::Definition => {
                    Memo::Definition(access.definition(selected_definition(prepared)).await?)
                }
                Region::Expression => Memo::Expression(
                    access
                        .expression(selected_expression(prepared), TypeContext::default())
                        .await?,
                ),
            };
            let verified = access
                .endpoint
                .child_call(|| async {
                    match region {
                        Region::Scope => {
                            access
                                .endpoint
                                .validate_callable(
                                    &access.routes.scope,
                                    region.id(prepared),
                                    previous_revision,
                                )?
                                .await
                        }
                        Region::Definition => {
                            access
                                .endpoint
                                .validate_callable(
                                    &access.routes.definition,
                                    region.id(prepared),
                                    previous_revision,
                                )?
                                .await
                        }
                        Region::Expression => {
                            access
                                .endpoint
                                .validate_callable(
                                    &access.routes.expression,
                                    region.id(prepared),
                                    previous_revision,
                                )?
                                .await
                        }
                    }
                })
                .await;
            Ok((memo, verified))
        })
    })
}

fn invalidated(region: Region, changed: bool) -> (TestDb, salsa::Id, usize, salsa::Revision) {
    let mut db = assignment_fixture(match region {
        Region::Scope => "True\n",
        Region::Definition | Region::Expression => "left = right = True\n",
    });
    let previous_revision = salsa::plumbing::current_revision(&db);
    let (id, old_address) = {
        let prepared = prepare(&db);
        let seeded = capture(&db, || region.ordinary(&db, &prepared)).unwrap();
        seeded.check_root_reads().unwrap();
        region.certify(&db, &prepared).unwrap();
        (region.id(&prepared), seeded.value.address())
    };
    db.write_file(
        "src/main.py",
        match (region, changed) {
            (Region::Scope, false) => "True\n\n",
            (Region::Scope, true) => "False\n",
            (Region::Definition | Region::Expression, false) => "left = right = True\n\n",
            (Region::Definition | Region::Expression, true) => "left = right = False\n",
        },
    )
    .unwrap();
    (db, id, old_address, previous_revision)
}

#[test]
fn ordinary_inference_memos_are_compared_before_publication_and_retry() {
    for (region, changed) in [
        (Region::Expression, false),
        (Region::Expression, true),
        (Region::Definition, false),
        (Region::Definition, true),
        (Region::Scope, false),
        (Region::Scope, true),
    ] {
        let (measured, id, old_address, previous_revision) = invalidated(region, changed);
        let prepared = prepare(&measured);
        assert_eq!(region.id(&prepared), id);
        comparisons::reset(old_address);
        let result = controlled_region(region, &prepared, previous_revision, &funded());
        let Ok(AnalysisOutcome::Complete((_, verified))) = result else {
            panic!("{region:?}: {result:?}");
        };
        assert_eq!(
            matches!(verified, VerifyResult::Changed),
            changed,
            "{region:?}, changed={changed}"
        );
        let comparison = comparisons::snapshot().unwrap();
        assert!(comparison.left == old_address || comparison.right == old_address);
        let quote = comparison.work.unwrap();
        let before_admission =
            funded().semantic_work_limit - comparison.finished_remaining.unwrap();

        let (db, id, old_address, previous_revision) = invalidated(region, changed);
        let prepared = prepare(&db);
        assert_eq!(region.id(&prepared), id);
        let revision = salsa::plumbing::current_revision(&db);
        comparisons::reset(old_address);
        observations::reset(None);
        let refused = controlled_region(
            region,
            &prepared,
            previous_revision,
            &AnalysisPolicy {
                semantic_work_limit: before_admission + quote - 1,
                ..funded()
            },
        );
        assert!(
            matches!(
                refused,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    ..
                })
            ),
            "{region:?}: {refused:?}"
        );
        let comparison = comparisons::snapshot().unwrap();
        assert_eq!(comparison.work, Some(quote));
        assert_eq!(comparison.finished_remaining, Some(quote - 1));
        assert_eq!(
            region.certify(&db, &prepared),
            Err(FinalSourceError::UnverifiedMemo)
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        comparisons::reset(old_address);
        let accepted = capture(&db, || {
            controlled_region(region, &prepared, previous_revision, &funded())
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete((memo, verified))) = accepted.value else {
            panic!("{region:?}: {:?}", accepted.value);
        };
        assert_eq!(
            matches!(verified, VerifyResult::Changed),
            changed,
            "{region:?}, changed={changed}"
        );
        let comparison = comparisons::snapshot().unwrap();
        assert!(comparison.left == old_address || comparison.right == old_address);
        assert!(comparison.work.is_some());
        assert!(accepted.reads.iter().any(|read| read.parent.is_none()
            && read.key.key_index() == id
            && read.status == Status::Final));
        for source in ["parsed_module", "semantic_index"] {
            assert!(accepted.reads.iter().any(|read| {
                db.ingredient_debug_name(read.key.ingredient_index()) == source
                    && read.parent.is_some_and(|parent| {
                        parent.key_index() == id
                            && db.ingredient_debug_name(parent.ingredient_index()) == region.name()
                    })
                    && read.status == Status::Final
            }), "{region:?} did not record its {source} dependency");
        }
        region.certify(&db, &prepared).unwrap();
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();

        let mut events_db = db.clone();
        events_db.take_salsa_events();
        assert_eq!(region.ordinary(&db, &prepared).address(), memo.address());
        assert_function_query_was_not_run_by_name(
            &db,
            region.name(),
            None,
            &events_db.take_salsa_events(),
        );
    }
}

fn controlled_scope_maps<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    previous_revision: salsa::Revision,
) -> Result<
    AnalysisOutcome<(&'db PlaceTable, &'db UseDefMap<'db>, [VerifyResult; 2])>,
    AnalysisFailure,
> {
    with_analysis_session(prepared, &funded(), |session| {
        let db = session.db() as &dyn ty_python_core::Db;
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        registry.enable_structural_dependency_validation()?;
        let places = registry.reserve_callable(db, place_table_ingredient(db))?;
        let uses = registry.reserve_callable(db, use_def_map_ingredient(db))?;
        registry.bind_callable(&places, PlaceTableProvider { session })?;
        registry.bind_callable(&uses, UseDefMapProvider { session })?;
        let scope = selected_scope(prepared).as_id();
        registry.seal()?.run(|endpoint| async move {
            let place_table = endpoint
                .child_call(|| async { endpoint.fetch_ref(&places, scope)?.await })
                .await;
            let use_def_map = endpoint
                .child_call(|| async { endpoint.fetch_ref(&uses, scope)?.await })
                .await;
            let places_verified = endpoint
                .child_call(|| async {
                    endpoint
                        .validate_callable(&places, scope, previous_revision)?
                        .await
                })
                .await;
            let uses_verified = endpoint
                .child_call(|| async {
                    endpoint
                        .validate_callable(&uses, scope, previous_revision)?
                        .await
                })
                .await;
            Ok((
                place_table.as_ref(),
                use_def_map.as_ref(),
                [places_verified, uses_verified],
            ))
        })
    })
}

#[test]
fn scope_map_providers_preserve_canonical_values_and_backdating() {
    for ordinary_first in [false, true] {
        for changed in [false, true] {
            let mut db = assignment_fixture("x: \"int\"\n");
            let previous_revision = salsa::plumbing::current_revision(&db);
            let (scope_id, old_places, old_uses) = {
                let prepared = prepare(&db);
                let scope = selected_scope(&prepared);
                let db_view = &db as &dyn ty_python_core::Db;
                assert!(matches!(
                    FinalSourceMemo::certify(
                        db_view,
                        place_table_ingredient(db_view),
                        scope.as_id()
                    ),
                    Err(FinalSourceError::MissingMemo)
                ));
                assert!(matches!(
                    FinalSourceMemo::certify(
                        db_view,
                        use_def_map_ingredient(db_view),
                        scope.as_id()
                    ),
                    Err(FinalSourceError::MissingMemo)
                ));
                let (places, uses) = if ordinary_first {
                    (place_table(&db, scope), use_def_map(&db, scope))
                } else {
                    let result = controlled_scope_maps(&prepared, previous_revision);
                    let Ok(AnalysisOutcome::Complete((places, uses, _))) = result else {
                        panic!("{result:?}");
                    };
                    assert!(std::ptr::eq(places, place_table(&db, scope)));
                    assert!(std::ptr::eq(uses, use_def_map(&db, scope)));
                    (places, uses)
                };
                (
                    scope.as_id(),
                    std::ptr::from_ref(places).addr(),
                    std::ptr::from_ref(uses).addr(),
                )
            };
            // Equal-width quoted annotations preserve both maps while changing a definition field.
            // Removing the declaration changes both maps, including the retained definition states.
            db.write_file(
                "src/main.py",
                if changed { "pass\n" } else { "x: \"str\"\n" },
            )
            .unwrap();
            let prepared = prepare(&db);
            let scope = selected_scope(&prepared);
            let revision = salsa::plumbing::current_revision(&db);
            assert_eq!(scope.as_id(), scope_id);
            let db_view = &db as &dyn ty_python_core::Db;
            let places_ingredient = place_table_ingredient(db_view);
            let uses_ingredient = use_def_map_ingredient(db_view);
            assert!(matches!(
                FinalSourceMemo::certify(db_view, places_ingredient, scope_id),
                Err(FinalSourceError::UnverifiedMemo)
            ));
            assert!(matches!(
                FinalSourceMemo::certify(db_view, uses_ingredient, scope_id),
                Err(FinalSourceError::UnverifiedMemo)
            ));
            let captured =
                capture(&db, || controlled_scope_maps(&prepared, previous_revision)).unwrap();
            captured.check_root_reads().unwrap();
            let Ok(AnalysisOutcome::Complete((places, uses, verified))) = captured.value else {
                panic!("{:?}", captured.value);
            };
            for verified in verified {
                assert_eq!(matches!(verified, VerifyResult::Changed), changed);
            }
            assert_ne!(std::ptr::from_ref(places).addr(), old_places);
            assert_ne!(std::ptr::from_ref(uses).addr(), old_uses);
            assert!(std::ptr::eq(
                places,
                prepared.semantic_index().place_table(FileScopeId::global())
            ));
            assert!(std::ptr::eq(
                uses,
                prepared.semantic_index().use_def_map(FileScopeId::global())
            ));
            let index_key = semantic_index::prepare_memo(&db, prepared.program_file())
                .unwrap()
                .database_key();
            for parent in [
                places_ingredient.database_key_index(scope_id),
                uses_ingredient.database_key_index(scope_id),
            ] {
                let sources = captured
                    .reads
                    .iter()
                    .filter(|read| read.parent == Some(parent))
                    .collect::<Vec<_>>();
                assert_eq!(sources.len(), 1);
                assert_eq!(sources[0].key, index_key);
                assert_eq!(sources[0].status, Status::Final);
                assert_eq!(sources[0].stamp, captured.stamp);
            }
            let mut events_db = db.clone();
            events_db.take_salsa_events();
            assert!(std::ptr::eq(places, place_table(&db, scope)));
            assert!(std::ptr::eq(uses, use_def_map(&db, scope)));
            let events = events_db.take_salsa_events();
            for name in ["place_table", "use_def_map"] {
                assert_function_query_was_not_run_by_name(&db, name, Some(scope_id), &events);
            }
            FinalSourceMemo::certify(db_view, places_ingredient, scope_id).unwrap();
            FinalSourceMemo::certify(db_view, uses_ingredient, scope_id).unwrap();
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

fn quote_memo<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    memo: Memo<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<usize>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        run.run(|endpoint| async move {
            let work = match memo {
                Memo::Scope(value) => {
                    inference_profiles::quote_scope_comparison(endpoint.clone(), value, value)
                        .await?
                }
                Memo::Definition(value) => {
                    inference_profiles::quote_definition_comparison(endpoint.clone(), value, value)
                        .await?
                }
                Memo::Expression(value) => {
                    inference_profiles::quote_expression_comparison(endpoint.clone(), value, value)
                        .await?
                }
            };
            endpoint.local_call(|| endpoint.admit_work(work)).await;
            Ok(work)
        })
    })
}

#[test]
fn ordinary_diagnostics_and_collection_constraints_require_admitted_scans() {
    let diagnostic_source = "left = right = missing_name_with_a_long_diagnostic_message\n";
    for (region, source, diagnostics) in [
        (Region::Scope, diagnostic_source, true),
        (Region::Definition, diagnostic_source, true),
        (Region::Expression, diagnostic_source, true),
        (Region::Scope, "xs = []\nxs.append(1)\n", false),
        (Region::Expression, "xs = []\nxs.append(1)\n", false),
        (Region::Definition, "xs = []\nys: list[int] = xs\n", false),
    ] {
        let db = assignment_fixture(source);
        let prepared = prepare(&db);
        let memo = region.ordinary(&db, &prepared);
        region.certify(&db, &prepared).unwrap();
        if diagnostics {
            assert!(memo.has_diagnostics(), "{region:?}");
        } else {
            assert!(memo.has_constraints(), "{region:?}");
        }
        comparisons::reset(memo.address());
        let quoted = quote_memo(&prepared, memo, &funded());
        let Ok(AnalysisOutcome::Complete(work)) = quoted else {
            panic!("{region:?}: {quoted:?}");
        };
        if diagnostics {
            assert!(work > diagnostic_source.len() * 2);
        }
        let observation = comparisons::snapshot().unwrap();
        let before_scan = funded().semantic_work_limit - observation.started_remaining.unwrap();
        comparisons::reset(memo.address());
        assert_eq!(
            quote_memo(
                &prepared,
                memo,
                &AnalysisPolicy {
                    semantic_work_limit: before_scan,
                    ..funded()
                }
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            })
        );
        assert!(comparisons::snapshot().unwrap().work.is_none());
        region.certify(&db, &prepared).unwrap();
        assert_no_active_attempt();
        assert_eq!(
            quote_memo(&prepared, memo, &funded()),
            Ok(AnalysisOutcome::Complete(work))
        );
        assert_eq!(region.ordinary(&db, &prepared).address(), memo.address());
    }
}

fn compare_optional_types<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    left: Option<Type<'db>>,
    right: Option<Type<'db>>,
    before_comparison: &Cell<Option<usize>>,
    compared: &Cell<bool>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<(usize, bool)>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
        run.run(|endpoint| async move {
            let work =
                Option::<Type<'db>>::comparison_work(endpoint.clone(), &left, &right).await?;
            let equal = endpoint
                .local_call(|| {
                    before_comparison.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db()),
                    );
                    endpoint.admit_work(work)?;
                    endpoint.check_completion()?;
                    compared.set(true);
                    Ok(left == right)
                })
                .await;
            Ok((work, equal))
        })
    })
}

#[test]
fn optional_type_native_comparison_admits_present_payloads() {
    let db = fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let before_comparison = Cell::new(None);
    let compared = Cell::new(false);
    let empty = compare_optional_types(
        &prepared,
        None,
        None,
        &before_comparison,
        &compared,
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete((empty_work, true))) = empty else {
        panic!("{empty:?}");
    };
    assert!(compared.get());
    let short = todo_type!("default");
    let long = todo_type!("a bound variable default with a longer inline payload");
    for (left, right) in [
        (None, None),
        (Some(short), None),
        (None, Some(long)),
        (Some(short), Some(long)),
        (Some(long), Some(long)),
    ] {
        before_comparison.set(None);
        compared.set(false);
        let result = compare_optional_types(
            &prepared,
            left,
            right,
            &before_comparison,
            &compared,
            &funded(),
        );
        let Ok(AnalysisOutcome::Complete((work, equal))) = result else {
            panic!("{result:?}");
        };
        assert!(compared.get());
        assert_eq!(equal, left == right);
        assert_eq!(
            work,
            empty_work
                + left.map_or(0, Type::inline_payload_bytes)
                + right.map_or(0, Type::inline_payload_bytes),
        );
        let before_work = funded().semantic_work_limit - before_comparison.get().unwrap();
        before_comparison.set(None);
        compared.set(false);
        assert_eq!(
            compare_optional_types(
                &prepared,
                left,
                right,
                &before_comparison,
                &compared,
                &AnalysisPolicy {
                    semantic_work_limit: before_work + work - 1,
                    ..funded()
                },
            ),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: (),
            }),
        );
        assert_eq!(before_comparison.get(), Some(work - 1));
        assert!(!compared.get());
        assert_no_active_attempt();
        assert_eq!(
            compare_optional_types(
                &prepared,
                left,
                right,
                &before_comparison,
                &compared,
                &funded(),
            ),
            result,
        );
        assert!(compared.get());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}
