use super::*;
use crate::db::tests::{TestDbBuilder, setup_db};
use crate::place::{typing_extensions_symbol, typing_symbol};
use crate::types::type_alias::PEP695TypeAliasType;
use ruff_db::system::DbWithWritableSystem as _;
use ruff_python_ast as ast;
use ruff_python_ast::PythonVersion;
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
    fn normalize<'db>(
        db: &'db dyn crate::Db,
        ty: Type<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Type<'db>> {
        ty.recursive_type_normalized_impl(
            db,
            div,
            nested,
            &RecursiveTypeNormalizationVisitor::default(),
        )
    }

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
    assert_eq!(normalize(&db, top_div, div, true), None);
    assert_eq!(normalize(&db, bottom_div, div, true), None);

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
    let normalized = normalize(&db, nested_rec, div, false).unwrap();
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
    let normalized = normalize(&db, recursive_tuple, div, false).unwrap();
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
    let normalized = normalize(&db, recursive_dict, div, false).unwrap();
    assert_eq!(normalized.display(&db).to_string(), "dict[str, Divergent]");

    let union = UnionType::from_elements(&db, [div, KnownClass::Int.to_instance(&db)]);
    assert_eq!(union.display(&db).to_string(), "Divergent | int");
    let normalized = normalize(&db, union, div, false).unwrap();
    assert_eq!(normalized.display(&db).to_string(), "int");

    // The same can be said about intersections for the `Never` type.
    let intersection = IntersectionType::from_elements(&db, [Type::Never, div]);
    assert_eq!(intersection.display(&db).to_string(), "Never");

    let intersection = IntersectionType::from_elements(&db, [div, Type::Never]);
    assert_eq!(intersection.display(&db).to_string(), "Never");
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
