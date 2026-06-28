use super::*;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::{typing_extensions_symbol, typing_symbol};
use crate::types::type_alias::PEP695TypeAliasType;
use ruff_db::system::DbWithWritableSystem as _;
use ruff_python_ast as ast;
use ruff_python_ast::PythonVersion;
use salsa::Database as _;
use salsa::plumbing::AsId as _;
use test_case::test_case;

/// Explicitly test for Python version <3.13 and >=3.13, to ensure that
/// the fallback to `typing_extensions` is working correctly.
/// See [`KnownClass::canonical_module`] for more information.
#[test_case(PythonVersion::PY312)]
#[test_case(PythonVersion::PY313)]
fn no_default_type_is_singleton(python_version: PythonVersion) {
    let db = TestDbBuilder::new()
        .with_python_version(python_version)
        .build()
        .unwrap();

    let no_default = KnownClass::NoDefaultType.to_instance(&db);

    assert!(no_default.is_singleton(&db));
}

#[test]
fn typing_vs_typeshed_no_default() {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();

    let typing_no_default = typing_symbol(&db, "NoDefault").place.expect_type();
    let typing_extensions_no_default = typing_extensions_symbol(&db, "NoDefault")
        .place
        .expect_type();

    assert_eq!(typing_no_default.display(&db).to_string(), "NoDefault");
    assert_eq!(
        typing_extensions_no_default.display(&db).to_string(),
        "NoDefault"
    );
}

fn list_alias<'db>(db: &'db dyn Db, argument: Type<'db>) -> GenericAlias<'db> {
    KnownClass::List
        .to_specialized_class_type(db, &[argument])
        .expect("`list` should accept one type argument")
        .into_generic_alias()
        .expect("a specialized `list` should be a generic alias")
}

fn oscillating_generic_alias_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &Type<'db>,
    current: Type<'db>,
) -> Type<'db> {
    current.cycle_normalized(db, *previous, cycle)
}

#[salsa::tracked(
    cycle_initial=|_, id| Type::divergent(id),
    cycle_fn=oscillating_generic_alias_cycle_recover,
)]
fn oscillating_generic_alias(db: &dyn Db) -> Type<'_> {
    let previous = oscillating_generic_alias(db);
    let argument = if let Type::GenericAlias(alias) = previous
        && alias.specialization(db).types(db) == [Type::unknown()]
    {
        KnownClass::Int.to_instance(db)
    } else {
        Type::unknown()
    };

    list_alias(db, argument).into()
}

#[test]
fn generic_alias_cycle_recovery_normalizes_same_origin_unknown_oscillation() {
    let db = setup_db();
    let Type::GenericAlias(alias) = oscillating_generic_alias(&db) else {
        panic!("cycle recovery should preserve the generic alias");
    };

    assert_eq!(alias.specialization(&db).types(&db), &[Type::unknown()]);
}

#[test]
fn generic_alias_cycle_recovery_rejects_unsafe_merges() {
    let db = setup_db();
    let int = list_alias(&db, KnownClass::Int.to_instance(&db));
    let str = list_alias(&db, KnownClass::Str.to_instance(&db));
    assert!(str.merge_cycle_recovery(&db, int).is_none());

    let generic_context = int.specialization(&db).generic_context(&db);
    let unknown_generic = Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
    assert!(
        int.merge_cycle_recovery(&db, list_alias(&db, unknown_generic))
            .is_none()
    );
}

/// All other tests also make sure that `Type::Todo` works as expected. This particular
/// test makes sure that we handle `Todo` types correctly, even if they originate from
/// different sources.
#[test]
fn todo_types() {
    let db = setup_db();

    let todo1 = todo_type!("1");
    let todo2 = todo_type!("2");

    let int = KnownClass::Int.to_instance(&db);

    assert!(int.is_assignable_to(&db, todo1));

    assert!(todo1.is_assignable_to(&db, int));

    // We lose information when combining several `Todo` types. This is an
    // acknowledged limitation of the current implementation. We cannot
    // easily store the meta information of several `Todo`s in a single
    // variant, as `TodoType` needs to implement `Copy`, meaning it can't
    // contain `Vec`/`Box`/etc., and can't be boxed itself.
    //
    // Lifting this restriction would require us to intern `TodoType` in
    // salsa, but that would mean we would have to pass in `db` everywhere.

    // A union of several `Todo` types collapses to a single `Todo` type:
    assert!(UnionType::from_elements(&db, [todo1, todo2]).is_todo());

    // And similar for intersection types:
    assert!(IntersectionType::from_elements(&db, [todo1, todo2]).is_todo());
    assert!(
        IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_negative(todo2)
            .build()
            .is_todo()
    );
}

#[test]
fn divergent_type() {
    let db = setup_db();
    let div = Type::divergent(salsa::plumbing::Id::from_bits(1));
    assert!(div.is_dynamic());
    assert!(div.has_dynamic(&db));
    let visitor = ApplyTypeMappingVisitor::default();
    let top_div = div.materialize(&db, MaterializationKind::Top, &visitor);
    let bottom_div = div.materialize(&db, MaterializationKind::Bottom, &visitor);

    assert!(top_div.is_divergent());
    assert!(bottom_div.is_divergent());
    assert!(!top_div.is_dynamic());
    assert!(!bottom_div.is_dynamic());
    assert!(!top_div.has_dynamic(&db));
    assert!(!bottom_div.has_dynamic(&db));
    assert!(top_div.is_object());
    assert!(!top_div.is_never());
    assert!(!bottom_div.is_object());
    assert!(bottom_div.is_never());
    assert_eq!(top_div.negate(&db), bottom_div);
    assert_eq!(bottom_div.negate(&db), top_div);
    assert_eq!(IntersectionBuilder::new(&db).add_negative(div).build(), div);
    assert_eq!(
        IntersectionBuilder::new(&db).add_negative(top_div).build(),
        bottom_div
    );
    assert_eq!(
        IntersectionBuilder::new(&db)
            .add_negative(bottom_div)
            .build(),
        top_div
    );
    assert!(
        KnownClass::Int
            .to_instance(&db)
            .is_assignable_to(&db, top_div)
    );
    assert!(!top_div.is_assignable_to(&db, KnownClass::Int.to_instance(&db)));
    assert!(bottom_div.is_assignable_to(&db, KnownClass::Int.to_instance(&db)));
    assert!(
        !KnownClass::Int
            .to_instance(&db)
            .is_assignable_to(&db, bottom_div)
    );
    assert_eq!(
        top_div.member(&db, "__str__").place.expect_type(),
        Type::object().member(&db, "__str__").place.expect_type()
    );
    assert_eq!(
        top_div.member(&db, "__class__").place.expect_type(),
        Type::object().dunder_class(&db)
    );
    assert!(top_div.try_upcast_to_callable(&db).is_none());
    assert!(
        top_div
            .subscript(&db, Type::int_literal(0), ast::ExprContext::Load)
            .is_err()
    );
    assert_eq!(top_div.recursive_type_normalized_impl(&db, div, true), None);
    assert_eq!(
        bottom_div.recursive_type_normalized_impl(&db, div, true),
        None
    );

    // The `Divergent` type must not be eliminated in union with other dynamic types,
    // as this would prevent detection of divergent type inference using `Divergent`.
    let union = UnionType::from_elements(&db, [Type::unknown(), div]);
    assert_eq!(union.display(&db).to_string(), "Unknown | Divergent");

    let union = UnionType::from_elements(&db, [div, Type::unknown()]);
    assert_eq!(union.display(&db).to_string(), "Divergent | Unknown");

    let union = UnionType::from_elements(&db, [div, Type::unknown(), todo_type!("1")]);
    assert_eq!(union.display(&db).to_string(), "Divergent | Unknown");

    assert!(div.is_equivalent_to(&db, div));
    assert!(!div.is_equivalent_to(&db, Type::unknown()));
    assert!(!Type::unknown().is_equivalent_to(&db, div));
    assert!(!div.is_redundant_with(&db, Type::unknown()));
    assert!(!Type::unknown().is_redundant_with(&db, div));

    // `Divergent & T` and `Divergent & ~T` both simplify to `Divergent`, except for the
    // specific case of `Divergent & Never`, which simplifies to `Never`.
    let divergent_intersection = IntersectionBuilder::new(&db)
        .add_positive(div)
        .add_positive(todo_type!("2"))
        .add_negative(todo_type!("3"))
        .build();
    assert_eq!(divergent_intersection, div);
    let divergent_intersection = IntersectionBuilder::new(&db)
        .add_positive(todo_type!("2"))
        .add_negative(todo_type!("3"))
        .add_positive(div)
        .build();
    assert_eq!(divergent_intersection, div);
    let divergent_never_intersection = IntersectionBuilder::new(&db)
        .add_positive(div)
        .add_positive(Type::Never)
        .build();
    assert_eq!(divergent_never_intersection, Type::Never);
    let divergent_never_intersection = IntersectionBuilder::new(&db)
        .add_positive(Type::Never)
        .add_positive(div)
        .build();
    assert_eq!(divergent_never_intersection, Type::Never);

    // The `object` type has a good convergence property, that is, its union with all other types is `object`.
    // (e.g. `object | tuple[Divergent] == object`, `object | tuple[object] == object`)
    // So we can safely eliminate `Divergent`.
    let union = UnionType::from_elements(&db, [div, KnownClass::Object.to_instance(&db)]);
    assert_eq!(union.display(&db).to_string(), "object");

    let union = UnionType::from_elements(&db, [KnownClass::Object.to_instance(&db), div]);
    assert_eq!(union.display(&db).to_string(), "object");

    let recursive = UnionType::from_elements(
        &db,
        [
            KnownClass::List.to_specialized_instance(&db, &[div]),
            Type::none(&db),
        ],
    );
    let nested_rec = KnownClass::List.to_specialized_instance(&db, &[recursive]);
    assert_eq!(
        nested_rec.display(&db).to_string(),
        "list[list[Divergent] | None]"
    );
    let normalized = nested_rec
        .recursive_type_normalized_impl(&db, div, false)
        .unwrap();
    assert_eq!(normalized.display(&db).to_string(), "list[Divergent]");

    let recursive_tuple = Type::heterogeneous_tuple(
        &db,
        [
            UnionType::from_elements(
                &db,
                [
                    KnownClass::Int.to_instance(&db),
                    Type::heterogeneous_tuple(
                        &db,
                        [
                            UnionType::from_elements(&db, [KnownClass::Int.to_instance(&db), div]),
                            KnownClass::Str.to_instance(&db),
                        ],
                    ),
                ],
            ),
            KnownClass::Str.to_instance(&db),
        ],
    );
    let normalized = recursive_tuple
        .recursive_type_normalized_impl(&db, div, false)
        .unwrap();
    assert_eq!(normalized.display(&db).to_string(), "tuple[Divergent, str]");

    let recursive_dict = KnownClass::Dict.to_specialized_instance(
        &db,
        &[
            KnownClass::Str.to_instance(&db),
            UnionType::from_elements(
                &db,
                [
                    KnownClass::Int.to_instance(&db),
                    KnownClass::Dict.to_specialized_instance(
                        &db,
                        &[
                            KnownClass::Str.to_instance(&db),
                            UnionType::from_elements(&db, [KnownClass::Int.to_instance(&db), div]),
                        ],
                    ),
                ],
            ),
        ],
    );
    let normalized = recursive_dict
        .recursive_type_normalized_impl(&db, div, false)
        .unwrap();
    assert_eq!(normalized.display(&db).to_string(), "dict[str, Divergent]");

    let union = UnionType::from_elements(&db, [div, KnownClass::Int.to_instance(&db)]);
    assert_eq!(union.display(&db).to_string(), "Divergent | int");
    for (source, target) in [(div, union), (div, Type::unknown()), (Type::unknown(), div)] {
        let when = source.when_constraint_set_assignable_to_owned(&db, target);
        assert!(when.query(|_builder, when| when.is_always_satisfied(&db)));
    }
    let normalized = union
        .recursive_type_normalized_impl(&db, div, false)
        .unwrap();
    assert_eq!(normalized.display(&db).to_string(), "int");

    // The same can be said about intersections for the `Never` type.
    let intersection = IntersectionType::from_elements(&db, [Type::Never, div]);
    assert_eq!(intersection.display(&db).to_string(), "Never");

    let intersection = IntersectionType::from_elements(&db, [div, Type::Never]);
    assert_eq!(intersection.display(&db).to_string(), "Never");
}

fn divergent_marker(bits: u32) -> DivergentType {
    Type::divergent(salsa::plumbing::Id::from_bits(u64::from(bits)))
        .as_divergent()
        .expect("Type::divergent should create a divergent type")
}

fn recursive_list_type(db: &dyn Db, marker_bits: u32) -> (Type<'_>, RecursiveType<'_>, Type<'_>) {
    let binder = divergent_marker(marker_bits);
    let body = KnownClass::List.to_specialized_instance(
        db,
        &[UnionType::from_elements(
            db,
            [KnownClass::Int.to_instance(db), Type::Divergent(binder)],
        )],
    );
    let recursive_ty = RecursiveType::new(db, binder, RecursiveOrigin::Implicit, body);
    let Type::Recursive(recursive) = recursive_ty else {
        panic!("expected recursive list body to contain its binder");
    };
    (recursive_ty, recursive, body)
}

fn get_pep695_type_alias<'db>(db: &'db TestDb, name: &str) -> PEP695TypeAliasType<'db> {
    let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
    let ty = crate::place::global_symbol(db, module, name)
        .place
        .expect_type();
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(type_alias))) =
        ty
    else {
        panic!("Expected `{name}` to be a type alias");
    };
    type_alias
}

fn query_was_run(
    db: &TestDb,
    query_name_pattern: &str,
    input: salsa::Id,
    events: &[salsa::Event],
) -> bool {
    events.iter().any(|event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            db.ingredient_debug_name(database_key.ingredient_index())
                .contains(query_name_pattern)
                && database_key.key_index() == input
        } else {
            false
        }
    })
}

fn assert_query_was_not_run(
    db: &TestDb,
    query_name_pattern: &str,
    input: salsa::Id,
    events: &[salsa::Event],
) {
    db.attach(|_| {
        assert!(
            !query_was_run(db, query_name_pattern, input, events),
            "Expected query matching {query_name_pattern}({input:?}) not to have run but it did:\n{events:#?}",
        );
    });
}

fn assert_query_was_run(
    db: &TestDb,
    query_name_pattern: &str,
    input: salsa::Id,
    events: &[salsa::Event],
) {
    db.attach(|_| {
        assert!(
            query_was_run(db, query_name_pattern, input, events),
            "Expected query matching {query_name_pattern}({input:?}) to have run but it did not:\n{events:#?}",
        );
    });
}

fn type_inference_query_names_run(db: &TestDb, events: &[salsa::Event]) -> Vec<String> {
    const TYPE_INFERENCE_QUERY_PATTERNS: &[&str] = &[
        "function_known_decorators",
        "infer_definition_types",
        "infer_deferred_types",
        "infer_scope_types_impl",
        "infer_expression_types_impl",
        "infer_expression_type_impl",
        "infer_statement_types_impl",
        "infer_unpack_types",
    ];

    db.attach(|_| {
        events
            .iter()
            .filter_map(|event| {
                let salsa::EventKind::WillExecute { database_key } = event.kind else {
                    return None;
                };

                let query_name = db.ingredient_debug_name(database_key.ingredient_index());
                (query_name.contains("types::infer")
                    || TYPE_INFERENCE_QUERY_PATTERNS
                        .iter()
                        .any(|pattern| query_name.contains(pattern)))
                .then(|| format!("{query_name}({:?})", database_key.key_index()))
            })
            .collect()
    })
}

fn assert_no_type_inference_queries_were_run(db: &TestDb, events: &[salsa::Event]) {
    let type_inference_queries = type_inference_query_names_run(db, events);
    assert!(
        type_inference_queries.is_empty(),
        "Expected fold/unfold not to invoke type inference queries, but it invoked:\n{type_inference_queries:#?}\nAll events:\n{events:#?}",
    );
}

#[test]
fn recursive_type_constructor_elides_unused_binder() {
    let db = setup_db();
    let binder = divergent_marker(10);
    let int = KnownClass::Int.to_instance(&db);

    assert_eq!(
        RecursiveType::new(&db, binder, RecursiveOrigin::Implicit, int),
        int
    );
}

#[test]
fn recursive_type_alias_origin_is_display_only() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
type Alias = int
"#,
    )?;

    let alias = get_pep695_type_alias(&db, "Alias");
    let binder = divergent_marker(16);
    let body = KnownClass::List.to_specialized_instance(
        &db,
        &[UnionType::from_elements(
            &db,
            [KnownClass::Int.to_instance(&db), Type::Divergent(binder)],
        )],
    );
    let implicit = RecursiveType::new(&db, binder, RecursiveOrigin::Implicit, body);
    let alias_origin = RecursiveType::new(
        &db,
        binder,
        RecursiveOrigin::TypeAlias(TypeAliasType::PEP695(alias)),
        body,
    );

    assert!(alias_origin.display(&db).to_string().contains("Alias = "));
    assert!(implicit.is_equivalent_to(&db, alias_origin));
    assert!(
        UnionType::from_elements(&db, [implicit, alias_origin]).is_equivalent_to(&db, implicit)
    );

    Ok(())
}

#[test]
fn recursive_type_fold_and_unfold() {
    let db = setup_db();
    let (recursive_ty, recursive, body) = recursive_list_type(&db, 11);

    let unfolded = recursive.unfolded(&db);
    let expected = KnownClass::List.to_specialized_instance(
        &db,
        &[UnionType::from_elements(
            &db,
            [KnownClass::Int.to_instance(&db), recursive_ty],
        )],
    );

    assert_eq!(unfolded, expected);
    assert_eq!(body.unfold_recursive(&db, recursive), expected);
    assert_eq!(expected.fold_recursive(&db, recursive), recursive_ty);
    assert_eq!(body.fold_recursive(&db, recursive), body);

    let element_ty =
        UnionType::from_elements(&db, [KnownClass::Int.to_instance(&db), recursive_ty]);
    assert_eq!(element_ty.fold_recursive(&db, recursive), element_ty);

    let nested_unfolded = KnownClass::List.to_specialized_instance(&db, &[expected]);
    let nested_folded = KnownClass::List.to_specialized_instance(&db, &[recursive_ty]);
    assert_eq!(
        nested_unfolded.fold_recursive(&db, recursive),
        nested_folded
    );
}

#[test]
fn recursive_type_identity_map_preserves_recursive_type() {
    let db = setup_db();
    let (recursive_ty, recursive, _) = recursive_list_type(&db, 12);
    let identity_mapping = TypeMapping::Promote(PromotionMode::Off, PromotionKind::Regular);

    assert_eq!(
        recursive.map_type(
            &db,
            &identity_mapping,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        ),
        recursive_ty
    );
    assert_eq!(
        recursive_ty.apply_type_mapping(&db, &identity_mapping, TypeContext::default()),
        recursive_ty
    );
}

#[test]
fn recursive_type_subscript_uses_unfolded_structure() {
    let db = setup_db();
    let (recursive_ty, recursive, _) = recursive_list_type(&db, 17);

    let element_ty = recursive_ty
        .subscript(&db, Type::int_literal(0), ast::ExprContext::Load)
        .expect("recursive list should be subscriptable");
    let expected = UnionType::from_elements(&db, [KnownClass::Int.to_instance(&db), recursive_ty]);

    assert!(
        element_ty.is_equivalent_to(&db, expected),
        "got {}, expected {}",
        element_ty.display(&db),
        expected.display(&db)
    );

    let reconstructed = KnownClass::List.to_specialized_instance(&db, &[element_ty]);
    assert_eq!(reconstructed, recursive.unfolded(&db));
    assert_eq!(reconstructed.fold_recursive(&db, recursive), recursive_ty);
}

#[test]
fn recursive_type_iteration_uses_unfolded_structure() {
    let db = setup_db();
    let (recursive_ty, recursive, _) = recursive_list_type(&db, 19);

    let recursive_iter = recursive_ty
        .try_iterate(&db)
        .expect("recursive list should be iterable");
    let unfolded_iter = recursive
        .unfolded(&db)
        .try_iterate(&db)
        .expect("unfolded recursive list should be iterable");

    assert_eq!(recursive_iter.as_ref(), unfolded_iter.as_ref());
}

#[test]
fn recursive_type_does_not_unfold_identity_forever() {
    let db = setup_db();
    let binder = divergent_marker(12);
    let recursive_ty = RecursiveType::new(
        &db,
        binder,
        RecursiveOrigin::Implicit,
        Type::Divergent(binder),
    );
    let Type::Recursive(recursive) = recursive_ty else {
        panic!("expected identity recursive body to contain its binder");
    };

    assert_eq!(recursive.unfolded(&db), recursive_ty);
    assert_eq!(
        recursive.map_if_unfolded(&db, |_| Type::Never),
        None::<Type<'_>>
    );
    assert_eq!(
        recursive.map_or_else(&db, Type::unknown, |_| Type::Never),
        Type::unknown()
    );
}

#[test]
fn recursive_type_relation_uses_unfolded_structure() {
    let db = setup_db();
    let (left_ty, left_recursive, _) = recursive_list_type(&db, 13);
    let (right_ty, _, _) = recursive_list_type(&db, 14);

    assert_ne!(left_ty, right_ty);
    assert!(left_ty.is_equivalent_to(&db, right_ty));
    assert!(left_ty.is_equivalent_to(&db, left_recursive.unfolded(&db)));
}

#[test]
fn recursive_fold_unfold_does_not_expand_type_aliases() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
type Alias = int
"#,
    )?;

    let alias = get_pep695_type_alias(&db, "Alias");
    let alias_id = alias.as_id();
    let alias_ty = Type::TypeAlias(TypeAliasType::PEP695(alias));
    let (_, recursive, _) = recursive_list_type(&db, 15);

    db.clear_salsa_events();
    assert_eq!(alias_ty.unfold_recursive(&db, recursive), alias_ty);
    assert_eq!(alias_ty.fold_recursive(&db, recursive), alias_ty);

    let events = db.take_salsa_events();
    assert_no_type_inference_queries_were_run(&db, &events);
    assert_query_was_not_run(
        &db,
        "PEP695TypeAliasType < 'db >::raw_value_type_",
        alias_id,
        &events,
    );

    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
type Alias = int
"#,
    )?;

    db.clear_salsa_events();
    let alias_id = {
        let alias = get_pep695_type_alias(&db, "Alias");
        let alias_id = alias.as_id();

        let _ = alias.raw_value_type(&db);

        alias_id
    };

    let events = db.take_salsa_events();
    assert_query_was_run(
        &db,
        "PEP695TypeAliasType < 'db >::raw_value_type_",
        alias_id,
        &events,
    );

    Ok(())
}

#[test]
fn recursive_fold_unfold_does_not_build_function_signatures() -> anyhow::Result<()> {
    use crate::place::global_symbol;

    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
def f(x: int) -> int:
    return x
"#,
    )?;

    let module = ruff_db::files::system_path_to_file(&db, "/src/a.py").unwrap();
    let function_ty = global_symbol(&db, module, "f").place.expect_type();
    let Type::FunctionLiteral(function) = function_ty else {
        panic!("Expected `f` to be a function literal");
    };

    let function_id = function.as_id();
    let binder = divergent_marker(18);
    let body = KnownClass::List.to_specialized_instance(
        &db,
        &[UnionType::from_elements(
            &db,
            [Type::Divergent(binder), function_ty],
        )],
    );
    let recursive_ty = RecursiveType::new(&db, binder, RecursiveOrigin::Implicit, body);
    let Type::Recursive(recursive) = recursive_ty else {
        panic!("expected recursive body to contain its binder");
    };

    db.clear_salsa_events();
    let _ = body.unfold_recursive(&db, recursive);
    let _ = recursive_ty.fold_recursive(&db, recursive);

    let events = db.take_salsa_events();
    assert_no_type_inference_queries_were_run(&db, &events);
    assert_query_was_not_run(
        &db,
        "FunctionType < 'db >::signature_",
        function_id,
        &events,
    );

    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
def f(x: int) -> int:
    return x
"#,
    )?;

    db.clear_salsa_events();
    let function_id = {
        let module = ruff_db::files::system_path_to_file(&db, "/src/a.py").unwrap();
        let function_ty = global_symbol(&db, module, "f").place.expect_type();
        let Type::FunctionLiteral(function) = function_ty else {
            panic!("Expected `f` to be a function literal");
        };

        let function_id = function.as_id();

        let _ = function.signature(&db);

        function_id
    };

    let events = db.take_salsa_events();
    assert_query_was_run(
        &db,
        "FunctionType < 'db >::signature_",
        function_id,
        &events,
    );

    Ok(())
}

#[test]
fn type_alias_variance() {
    use crate::db::tests::TestDb;
    use crate::place::global_symbol;

    fn get_type_alias<'db>(db: &'db TestDb, name: &str) -> PEP695TypeAliasType<'db> {
        let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
        let ty = global_symbol(db, module, name).place.expect_type();
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(
            type_alias,
        ))) = ty
        else {
            panic!("Expected `{name}` to be a type alias");
        };
        type_alias
    }
    fn get_bound_typevar<'db>(
        db: &'db TestDb,
        type_alias: PEP695TypeAliasType<'db>,
    ) -> BoundTypeVarInstance<'db> {
        let generic_context = type_alias.generic_context(db).unwrap();
        generic_context.variables(db).next().unwrap()
    }

    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"
from typing import Callable, Concatenate

class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

class Invariant[T]:
    def get(self) -> T:
        raise ValueError
    def set(self, value: T):
        pass

class Bivariant[T]:
    pass

type CovariantAlias[T] = Covariant[T]
type ContravariantAlias[T] = Contravariant[T]
type InvariantAlias[T] = Invariant[T]
type BivariantAlias[T] = Bivariant[T]
type CovariantAliasAlias[T] = CovariantAlias[T]
type ContravariantAliasAlias[T] = ContravariantAlias[T]
type InvariantAliasAlias[T] = InvariantAlias[T]
type BivariantAliasAlias[T] = BivariantAlias[T]
type ParamSpecContravariantAlias[**P] = Callable[P, None]
type ParamSpecConcatenateAlias[**P] = Callable[Concatenate[int, P], None]
type ParamSpecBivariantAlias[**P] = int

type RecursiveAlias[T] = None | list[RecursiveAlias[T]]
type RecursiveAlias2[T] = None | list[T] | list[RecursiveAlias2[T]]
"#,
    )
    .unwrap();
    let covariant = get_type_alias(&db, "CovariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(covariant))
            .variance_of(&db, get_bound_typevar(&db, covariant)),
        TypeVarVariance::Covariant
    );

    let contravariant = get_type_alias(&db, "ContravariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(contravariant))
            .variance_of(&db, get_bound_typevar(&db, contravariant)),
        TypeVarVariance::Contravariant
    );

    let invariant = get_type_alias(&db, "InvariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(invariant))
            .variance_of(&db, get_bound_typevar(&db, invariant)),
        TypeVarVariance::Invariant
    );

    let bivariant = get_type_alias(&db, "BivariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(bivariant))
            .variance_of(&db, get_bound_typevar(&db, bivariant)),
        TypeVarVariance::Bivariant
    );

    let covariant_alias = get_type_alias(&db, "CovariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(covariant_alias))
            .variance_of(&db, get_bound_typevar(&db, covariant_alias)),
        TypeVarVariance::Covariant
    );

    let contravariant_alias = get_type_alias(&db, "ContravariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(contravariant_alias))
            .variance_of(&db, get_bound_typevar(&db, contravariant_alias)),
        TypeVarVariance::Contravariant
    );

    let invariant_alias = get_type_alias(&db, "InvariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(invariant_alias))
            .variance_of(&db, get_bound_typevar(&db, invariant_alias)),
        TypeVarVariance::Invariant
    );

    let bivariant_alias = get_type_alias(&db, "BivariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(bivariant_alias))
            .variance_of(&db, get_bound_typevar(&db, bivariant_alias)),
        TypeVarVariance::Bivariant
    );

    let paramspec_contravariant = get_type_alias(&db, "ParamSpecContravariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(paramspec_contravariant))
            .variance_of(&db, get_bound_typevar(&db, paramspec_contravariant)),
        TypeVarVariance::Contravariant
    );

    let paramspec_concatenate = get_type_alias(&db, "ParamSpecConcatenateAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(paramspec_concatenate))
            .variance_of(&db, get_bound_typevar(&db, paramspec_concatenate)),
        TypeVarVariance::Contravariant
    );

    let paramspec_bivariant = get_type_alias(&db, "ParamSpecBivariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(paramspec_bivariant))
            .variance_of(&db, get_bound_typevar(&db, paramspec_bivariant)),
        TypeVarVariance::Bivariant
    );

    let recursive = get_type_alias(&db, "RecursiveAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(recursive))
            .variance_of(&db, get_bound_typevar(&db, recursive)),
        TypeVarVariance::Bivariant
    );

    let recursive2 = get_type_alias(&db, "RecursiveAlias2");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(recursive2))
            .variance_of(&db, get_bound_typevar(&db, recursive2)),
        TypeVarVariance::Invariant
    );
}

#[test]
fn eager_expansion() {
    use crate::db::tests::TestDb;
    use crate::place::global_symbol;

    fn get_type_alias<'db>(db: &'db TestDb, name: &str) -> Type<'db> {
        let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
        let ty = global_symbol(db, module, name).place.expect_type();
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(
            type_alias,
        ))) = ty
        else {
            panic!("Expected `{name}` to be a type alias");
        };
        Type::TypeAlias(TypeAliasType::PEP695(type_alias))
    }

    let mut db = setup_db();
    db.write_dedented(
        "/src/a.py",
        r#"

type IntStr = int | str
type ListIntStr = list[IntStr]
type RecursiveList[T] = list[T | RecursiveList[T]]
type RecursiveIntList = RecursiveList[int]
type Itself = Itself
type A = B
type B = A
type G[T] = H[T]
type H[T] = G[T]
"#,
    )
    .unwrap();

    let int_str = get_type_alias(&db, "IntStr");
    assert_eq!(
        int_str.expand_eagerly(&db).display(&db).to_string(),
        "int | str",
    );

    let list_int_str = get_type_alias(&db, "ListIntStr");
    assert_eq!(
        list_int_str.expand_eagerly(&db).display(&db).to_string(),
        "list[int | str]",
    );

    let rec_list = get_type_alias(&db, "RecursiveList");
    assert_eq!(
        rec_list.expand_eagerly(&db).display(&db).to_string(),
        "list[Divergent]",
    );

    let rec_int_list = get_type_alias(&db, "RecursiveIntList");
    assert_eq!(
        rec_int_list.expand_eagerly(&db).display(&db).to_string(),
        "list[Divergent]",
    );

    let itself = get_type_alias(&db, "Itself");
    assert_eq!(
        itself.expand_eagerly(&db).display(&db).to_string(),
        "Divergent",
    );

    let a = get_type_alias(&db, "A");
    assert_eq!(a.expand_eagerly(&db).display(&db).to_string(), "Divergent",);

    let b = get_type_alias(&db, "B");
    assert_eq!(b.expand_eagerly(&db).display(&db).to_string(), "Divergent",);

    let g = get_type_alias(&db, "G");
    assert_eq!(g.expand_eagerly(&db).display(&db).to_string(), "Divergent",);

    let h = get_type_alias(&db, "H");
    assert_eq!(h.expand_eagerly(&db).display(&db).to_string(), "Divergent",);
}
