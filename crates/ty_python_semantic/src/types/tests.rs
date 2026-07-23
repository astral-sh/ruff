use super::*;
use crate::SemanticEnvironment;
use crate::db::tests::{TestDbBuilder, setup_db};
use crate::place::{typing_extensions_symbol, typing_symbol};
use crate::types::type_alias::PEP695TypeAliasType;
use ruff_db::system::DbWithWritableSystem as _;
use ruff_python_ast as ast;
use ruff_python_ast::PythonVersion;
use test_case::test_case;
use ty_python_core::ProgramFile;
use ty_python_core::program::Program as ProjectProgram;

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

    let env = db.semantic_environment();
    let no_default = KnownClass::NoDefaultType.to_instance(&env);

    assert!(no_default.is_singleton(&env));
}

#[test]
fn typing_vs_typeshed_no_default() {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()
        .unwrap();

    let typing_no_default = typing_symbol(&db.semantic_environment(), "NoDefault")
        .place
        .expect_type();
    let typing_extensions_no_default =
        typing_extensions_symbol(&db.semantic_environment(), "NoDefault")
            .place
            .expect_type();

    assert_eq!(
        typing_no_default
            .display(&db.semantic_environment())
            .to_string(),
        "NoDefault"
    );
    assert_eq!(
        typing_extensions_no_default
            .display(&db.semantic_environment())
            .to_string(),
        "NoDefault"
    );
}

fn list_alias<'db>(env: &SemanticEnvironment<'db>, argument: Type<'db>) -> GenericAlias<'db> {
    KnownClass::List
        .to_specialized_class_type(env, &[argument])
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
    let env =
        SemanticEnvironment::from_program(db, ProjectProgram::get(db).resolver_environment(db));
    current.cycle_normalized(&env, *previous, cycle)
}

#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, id| Type::divergent(id),
    cycle_fn=oscillating_generic_alias_cycle_recover,
)]
fn oscillating_generic_alias(db: &dyn Db) -> Type<'_> {
    let env =
        SemanticEnvironment::from_program(db, ProjectProgram::get(db).resolver_environment(db));
    let previous = oscillating_generic_alias(db);
    let argument = if let Type::GenericAlias(alias) = previous
        && alias.specialization(db).types(db) == [Type::unknown()]
    {
        KnownClass::Int.to_instance(&env)
    } else {
        Type::unknown()
    };

    list_alias(&env, argument).into()
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
    let env = db.semantic_environment();
    let int = list_alias(&env, KnownClass::Int.to_instance(&env));
    let str = list_alias(&env, KnownClass::Str.to_instance(&env));
    assert!(str.merge_cycle_recovery(&db, int).is_none());

    let generic_context = int.specialization(&db).generic_context(&db);
    let unknown_generic = Type::Dynamic(DynamicType::UnknownGeneric(generic_context));
    assert!(
        int.merge_cycle_recovery(&db, list_alias(&env, unknown_generic))
            .is_none()
    );
}

/// All other tests also make sure that `Type::Todo` works as expected. This particular
/// test makes sure that we handle `Todo` types correctly, even if they originate from
/// different sources.
#[test]
fn todo_types() {
    let db = setup_db();
    let env = db.semantic_environment();

    let todo1 = todo_type!("1");
    let todo2 = todo_type!("2");

    let int = KnownClass::Int.to_instance(&env);

    assert!(int.is_assignable_to(&env, todo1));

    assert!(todo1.is_assignable_to(&env, int));

    // We lose information when combining several `Todo` types. This is an
    // acknowledged limitation of the current implementation. We cannot
    // easily store the meta information of several `Todo`s in a single
    // variant, as `TodoType` needs to implement `Copy`, meaning it can't
    // contain `Vec`/`Box`/etc., and can't be boxed itself.
    //
    // Lifting this restriction would require us to intern `TodoType` in
    // salsa, but that would mean we would have to pass in `db` everywhere.

    // A union of several `Todo` types collapses to a single `Todo` type:
    assert!(UnionType::from_elements(&env, [todo1, todo2]).is_todo());

    // And similar for intersection types:
    assert!(IntersectionType::from_elements(&env, [todo1, todo2]).is_todo());
    assert!(
        IntersectionBuilder::new(&env)
            .add_positive(todo1)
            .add_negative(todo2)
            .build()
            .is_todo()
    );
}

#[test]
fn divergent_type() {
    let db = setup_db();
    let env = db.semantic_environment();
    let div = Type::divergent(salsa::plumbing::Id::from_bits(1));
    assert!(div.is_dynamic());
    assert!(div.has_dynamic(&env));
    let visitor = ApplyTypeMappingVisitor::default();
    let top_div = div.materialize(&env, MaterializationKind::Top, &visitor);
    let bottom_div = div.materialize(&env, MaterializationKind::Bottom, &visitor);

    assert!(top_div.is_divergent());
    assert!(bottom_div.is_divergent());
    assert!(!top_div.is_dynamic());
    assert!(!bottom_div.is_dynamic());
    assert!(!top_div.has_dynamic(&env));
    assert!(!bottom_div.has_dynamic(&env));
    assert!(top_div.is_object());
    assert!(!top_div.is_never());
    assert!(!bottom_div.is_object());
    assert!(bottom_div.is_never());
    assert_eq!(top_div.negate(&env), bottom_div);
    assert_eq!(bottom_div.negate(&env), top_div);
    assert_eq!(
        IntersectionBuilder::new(&env).add_negative(div).build(),
        div
    );
    assert_eq!(
        IntersectionBuilder::new(&env).add_negative(top_div).build(),
        bottom_div
    );
    assert_eq!(
        IntersectionBuilder::new(&env)
            .add_negative(bottom_div)
            .build(),
        top_div
    );
    assert!(
        KnownClass::Int
            .to_instance(&env)
            .is_assignable_to(&env, top_div)
    );
    assert!(!top_div.is_assignable_to(&env, KnownClass::Int.to_instance(&env)));
    assert!(bottom_div.is_assignable_to(&env, KnownClass::Int.to_instance(&env)));
    assert!(
        !KnownClass::Int
            .to_instance(&env)
            .is_assignable_to(&env, bottom_div)
    );
    assert_eq!(
        top_div.member(&env, "__str__").place.expect_type(),
        Type::object().member(&env, "__str__").place.expect_type()
    );
    assert_eq!(
        top_div.member(&env, "__class__",).place.expect_type(),
        Type::object().dunder_class(&env)
    );
    assert!(top_div.try_upcast_to_callable(&env).is_none());
    assert!(
        top_div
            .subscript(&env, Type::int_literal(0), ast::ExprContext::Load,)
            .is_err()
    );
    assert_eq!(
        top_div.recursive_type_normalized_impl(&env, div, true),
        None
    );
    assert_eq!(
        bottom_div.recursive_type_normalized_impl(&env, div, true),
        None
    );

    // The `Divergent` type must not be eliminated in union with other dynamic types,
    // as this would prevent detection of divergent type inference using `Divergent`.
    let union = UnionType::from_elements(&env, [Type::unknown(), div]);
    assert_eq!(
        union.display(&db.semantic_environment()).to_string(),
        "Unknown | Divergent"
    );

    let union = UnionType::from_elements(&env, [div, Type::unknown()]);
    assert_eq!(
        union.display(&db.semantic_environment()).to_string(),
        "Divergent | Unknown"
    );

    let union = UnionType::from_elements(&env, [div, Type::unknown(), todo_type!("1")]);
    assert_eq!(
        union.display(&db.semantic_environment()).to_string(),
        "Divergent | Unknown"
    );

    assert!(div.is_equivalent_to(&env, div));
    assert!(!div.is_equivalent_to(&env, Type::unknown()));
    assert!(!Type::unknown().is_equivalent_to(&env, div));
    assert!(!div.is_redundant_with(&env, Type::unknown()));
    assert!(!Type::unknown().is_redundant_with(&env, div));

    // `Divergent & T` and `Divergent & ~T` both simplify to `Divergent`, except for the
    // specific case of `Divergent & Never`, which simplifies to `Never`.
    let divergent_intersection = IntersectionBuilder::new(&env)
        .add_positive(div)
        .add_positive(todo_type!("2"))
        .add_negative(todo_type!("3"))
        .build();
    assert_eq!(divergent_intersection, div);
    let divergent_intersection = IntersectionBuilder::new(&env)
        .add_positive(todo_type!("2"))
        .add_negative(todo_type!("3"))
        .add_positive(div)
        .build();
    assert_eq!(divergent_intersection, div);
    let divergent_never_intersection = IntersectionBuilder::new(&env)
        .add_positive(div)
        .add_positive(Type::Never)
        .build();
    assert_eq!(divergent_never_intersection, Type::Never);
    let divergent_never_intersection = IntersectionBuilder::new(&env)
        .add_positive(Type::Never)
        .add_positive(div)
        .build();
    assert_eq!(divergent_never_intersection, Type::Never);

    // The `object` type has a good convergence property, that is, its union with all other types is `object`.
    // (e.g. `object | tuple[Divergent] == object`, `object | tuple[object] == object`)
    // So we can safely eliminate `Divergent`.
    let union = UnionType::from_elements(&env, [div, KnownClass::Object.to_instance(&env)]);
    assert_eq!(
        union.display(&db.semantic_environment()).to_string(),
        "object"
    );

    let union = UnionType::from_elements(&env, [KnownClass::Object.to_instance(&env), div]);
    assert_eq!(
        union.display(&db.semantic_environment()).to_string(),
        "object"
    );

    let recursive = UnionType::from_elements(
        &env,
        [
            KnownClass::List.to_specialized_instance(&env, &[div]),
            Type::none(&env),
        ],
    );
    let nested_rec = KnownClass::List.to_specialized_instance(&env, &[recursive]);
    assert_eq!(
        nested_rec.display(&db.semantic_environment()).to_string(),
        "list[list[Divergent] | None]"
    );
    let normalized = nested_rec
        .recursive_type_normalized_impl(&env, div, false)
        .unwrap();
    assert_eq!(
        normalized.display(&db.semantic_environment()).to_string(),
        "list[Divergent]"
    );

    let recursive_tuple = Type::heterogeneous_tuple(
        &db,
        [
            UnionType::from_elements(
                &env,
                [
                    KnownClass::Int.to_instance(&env),
                    Type::heterogeneous_tuple(
                        &db,
                        [
                            UnionType::from_elements(
                                &env,
                                [KnownClass::Int.to_instance(&env), div],
                            ),
                            KnownClass::Str.to_instance(&env),
                        ],
                    ),
                ],
            ),
            KnownClass::Str.to_instance(&env),
        ],
    );
    let normalized = recursive_tuple
        .recursive_type_normalized_impl(&env, div, false)
        .unwrap();
    assert_eq!(
        normalized.display(&db.semantic_environment()).to_string(),
        "tuple[Divergent, str]"
    );

    let recursive_dict = KnownClass::Dict.to_specialized_instance(
        &env,
        &[
            KnownClass::Str.to_instance(&env),
            UnionType::from_elements(
                &env,
                [
                    KnownClass::Int.to_instance(&env),
                    KnownClass::Dict.to_specialized_instance(
                        &env,
                        &[
                            KnownClass::Str.to_instance(&env),
                            UnionType::from_elements(
                                &env,
                                [KnownClass::Int.to_instance(&env), div],
                            ),
                        ],
                    ),
                ],
            ),
        ],
    );
    let normalized = recursive_dict
        .recursive_type_normalized_impl(&env, div, false)
        .unwrap();
    assert_eq!(
        normalized.display(&db.semantic_environment()).to_string(),
        "dict[str, Divergent]"
    );

    let union = UnionType::from_elements(&env, [div, KnownClass::Int.to_instance(&env)]);
    assert_eq!(
        union.display(&db.semantic_environment()).to_string(),
        "Divergent | int"
    );
    for (source, target) in [(div, union), (div, Type::unknown()), (Type::unknown(), div)] {
        let when = source.when_constraint_set_assignable_to_owned(&env, target);
        assert!(when.query(|_builder, when| when.is_always_satisfied(&env)));
    }
    let normalized = union
        .recursive_type_normalized_impl(&env, div, false)
        .unwrap();
    assert_eq!(
        normalized.display(&db.semantic_environment()).to_string(),
        "int"
    );

    // The same can be said about intersections for the `Never` type.
    let intersection = IntersectionType::from_elements(&env, [Type::Never, div]);
    assert_eq!(
        intersection.display(&db.semantic_environment()).to_string(),
        "Never"
    );

    let intersection = IntersectionType::from_elements(&env, [div, Type::Never]);
    assert_eq!(
        intersection.display(&db.semantic_environment()).to_string(),
        "Never"
    );
}

#[test]
fn type_alias_variance() {
    use crate::db::tests::TestDb;
    use crate::place::global_symbol;

    fn get_type_alias<'db>(db: &'db TestDb, name: &str) -> PEP695TypeAliasType<'db> {
        let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
        let module = ProgramFile::new(db, module, db.semantic_environment().program());
        let ty = global_symbol(&db.semantic_environment(), module, name)
            .place
            .expect_type();
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
    ) -> BoundTypeVarIdentity<'db> {
        let generic_context = type_alias
            .generic_context(&db.semantic_environment())
            .unwrap();
        generic_context.variables(db).next().unwrap().identity(db)
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
    let env = db.semantic_environment();
    let covariant = get_type_alias(&db, "CovariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(covariant))
            .variance_of(&env, get_bound_typevar(&db, covariant)),
        TypeVarVariance::Covariant
    );

    let contravariant = get_type_alias(&db, "ContravariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(contravariant))
            .variance_of(&env, get_bound_typevar(&db, contravariant)),
        TypeVarVariance::Contravariant
    );

    let invariant = get_type_alias(&db, "InvariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(invariant))
            .variance_of(&env, get_bound_typevar(&db, invariant)),
        TypeVarVariance::Invariant
    );

    let bivariant = get_type_alias(&db, "BivariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(bivariant))
            .variance_of(&env, get_bound_typevar(&db, bivariant)),
        TypeVarVariance::Bivariant
    );

    let covariant_alias = get_type_alias(&db, "CovariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(covariant_alias))
            .variance_of(&env, get_bound_typevar(&db, covariant_alias)),
        TypeVarVariance::Covariant
    );

    let contravariant_alias = get_type_alias(&db, "ContravariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(contravariant_alias))
            .variance_of(&env, get_bound_typevar(&db, contravariant_alias)),
        TypeVarVariance::Contravariant
    );

    let invariant_alias = get_type_alias(&db, "InvariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(invariant_alias))
            .variance_of(&env, get_bound_typevar(&db, invariant_alias)),
        TypeVarVariance::Invariant
    );

    let bivariant_alias = get_type_alias(&db, "BivariantAliasAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(bivariant_alias))
            .variance_of(&env, get_bound_typevar(&db, bivariant_alias)),
        TypeVarVariance::Bivariant
    );

    let paramspec_contravariant = get_type_alias(&db, "ParamSpecContravariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(paramspec_contravariant))
            .variance_of(&env, get_bound_typevar(&db, paramspec_contravariant)),
        TypeVarVariance::Contravariant
    );

    let paramspec_concatenate = get_type_alias(&db, "ParamSpecConcatenateAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(paramspec_concatenate))
            .variance_of(&env, get_bound_typevar(&db, paramspec_concatenate)),
        TypeVarVariance::Contravariant
    );

    let paramspec_bivariant = get_type_alias(&db, "ParamSpecBivariantAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(paramspec_bivariant))
            .variance_of(&env, get_bound_typevar(&db, paramspec_bivariant)),
        TypeVarVariance::Bivariant
    );

    let recursive = get_type_alias(&db, "RecursiveAlias");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(recursive))
            .variance_of(&env, get_bound_typevar(&db, recursive)),
        TypeVarVariance::Bivariant
    );

    let recursive2 = get_type_alias(&db, "RecursiveAlias2");
    assert_eq!(
        KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(recursive2))
            .variance_of(&env, get_bound_typevar(&db, recursive2)),
        TypeVarVariance::Invariant
    );
}

#[test]
fn eager_expansion() {
    use crate::db::tests::TestDb;
    use crate::place::global_symbol;

    fn get_type_alias<'db>(db: &'db TestDb, name: &str) -> Type<'db> {
        let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
        let module = ProgramFile::new(db, module, db.semantic_environment().program());
        let ty = global_symbol(&db.semantic_environment(), module, name)
            .place
            .expect_type();
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
        int_str
            .expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "int | str",
    );

    let list_int_str = get_type_alias(&db, "ListIntStr");
    assert_eq!(
        list_int_str
            .expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "list[int | str]",
    );

    let rec_list = get_type_alias(&db, "RecursiveList");
    assert_eq!(
        rec_list
            .expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "list[Divergent]",
    );

    let rec_int_list = get_type_alias(&db, "RecursiveIntList");
    assert_eq!(
        rec_int_list
            .expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "list[Divergent]",
    );

    let itself = get_type_alias(&db, "Itself");
    assert_eq!(
        itself
            .expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "Divergent",
    );

    let a = get_type_alias(&db, "A");
    assert_eq!(
        a.expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "Divergent",
    );

    let b = get_type_alias(&db, "B");
    assert_eq!(
        b.expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "Divergent",
    );

    let g = get_type_alias(&db, "G");
    assert_eq!(
        g.expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "Divergent",
    );

    let h = get_type_alias(&db, "H");
    assert_eq!(
        h.expand_eagerly(&db.semantic_environment())
            .display(&db.semantic_environment())
            .to_string(),
        "Divergent",
    );
}
