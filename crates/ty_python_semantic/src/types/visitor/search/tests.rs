use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::btree_map;
use std::convert::Infallible;
use std::future::{Future, poll_fn};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use super::super::{CollectedTypes, SmallSet, TypeCollector, admit_type_walk_access_with};
use crate::types::KnownBoundMethodType;
use crate::types::protocol_class::ProtocolMemberData;

use super::{
    BoundMethodType, BoundSuperType, EnumComplementType, FieldInstance, FunctionType,
    FunctoolsPartialInstance, FxOrderSet, InternedConstraintSetSolution, InternedType,
    IntersectionType, MethodWrapper, NamedTupleField, NamedTupleSpec, NegativeIntersectionElements,
    NominalVisitorChildren, NonAtomicType, OrdinaryTypeWalk, PropertyInstanceClass,
    PropertyInstanceType, ProtocolVisitorChildren, SlotDescriptorType, Specialization,
    StoredTypeSequence, SyncTypeDepthEffects, SyncTypeWalkEffects, TypeDepthEffects, TypeGuardType,
    TypeIsType, TypeSearchDecision, TypeSearchDescent, TypeSearchEffects, TypeSupportEffects,
    TypeVarSolution, TypeWalkCursor, TypeWalkEffects, TypeWalkEvent, TypeWalkFacts,
    TypeWalkFieldOperation, TypeWalkPolicy, TypeWalkWork, UnionType, UnionTypeInstance, WalkAction,
    enter_depth_active_with, leave_depth_active_with, reserve_walk_pending_with, search_type_with, static_eligible_with,
    support_type_with, type_depth_sync, type_depth_with,
};
use crate::types::constraints::OwnedConstraintTypeCursor;
use crate::types::constraints::control::{
    AllocationKind, TddControl, TddError, TddWork, Unrestricted as UnrestrictedCollections,
    unrestricted,
};
use crate::types::protocol_class::{ProtocolInterfaceView, ProtocolMember};
use crate::types::{
    ClassType, NominalInstanceType, ProtocolInstanceType, RecursiveType, SubclassOfInner,
    SubclassOfType, TypeAliasType, todo_type,
};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::super::{TypeSearchMode, recursive_search_reference};
use super::{SearchControl, SearchOperation, SearchWork, Unrestricted, search};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::callable::CallableTypeKind;
use crate::types::constructor::expansion_probe::{AttemptSearchControl, Incomplete};
use crate::types::newtype::{NewType, NewTypeBase};
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::typed_dict::{
    SynthesizedTypedDictKind, SynthesizedTypedDictType, TypedDictFieldBuilder, TypedDictOpenness,
    TypedDictSchema,
};
use crate::types::typevar::{
    BindingContext, TypeVarBoundOrConstraints, TypeVarConstraints, TypeVarDefaultEvaluation,
    TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BoundTypeVarInstance, CallableSignature, CallableType, ClassLiteral, GenericAlias,
    GenericContext, KnownClass, KnownInstanceType, Parameter, Parameters, Signature, Type,
    TypeFormType, TypedDictType,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
enum Engine {
    Recursive,
    Cursor,
}

fn modes() -> [(TypeSearchMode, &'static str); 3] {
    [
        (TypeSearchMode::SkipLazyAttributes, "skip lazy"),
        (TypeSearchMode::IncludeLazyAttributes, "include lazy"),
        (TypeSearchMode::IncludeAliasArguments, "alias arguments"),
    ]
}

fn trace<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    mode: TypeSearchMode,
    stop_at: Option<usize>,
    engine: Engine,
) -> (Option<usize>, Vec<Type<'db>>) {
    let visited = RefCell::new(Vec::new());
    let query = |ty| {
        let mut visited = visited.borrow_mut();
        let index = visited.len();
        visited.push(ty);
        (stop_at == Some(index)).then_some(index)
    };
    let found = match engine {
        Engine::Recursive => recursive_search_reference(db, env, ty, mode, query),
        Engine::Cursor => match search(db, env, ty, mode, query, &mut Unrestricted) {
            Ok(found) => found,
            Err(error) => match error {},
        },
    };
    (found, visited.into_inner())
}

#[track_caller]
fn assert_equivalent<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    mode: TypeSearchMode,
    label: &str,
) -> Vec<Type<'db>> {
    let expected = trace(db, env, ty, mode, None, Engine::Recursive);
    let actual = trace(db, env, ty, mode, None, Engine::Cursor);
    assert_eq!(actual, expected, "{label}: complete traversal");
    assert_eq!(expected.0, None);
    eprintln!(
        "WALK_BASELINE search mode={label:?} root={:?} visits={:?}",
        ty.display(db, env).to_string(),
        expected
            .1
            .iter()
            .map(|ty| ty.display(db, env).to_string())
            .collect::<Vec<_>>()
    );
    for index in 0..expected.1.len() {
        let expected_match = trace(db, env, ty, mode, Some(index), Engine::Recursive);
        let actual_match = trace(db, env, ty, mode, Some(index), Engine::Cursor);
        assert_eq!(actual_match, expected_match, "{label}: match at {index}");
        assert_eq!(actual_match.0, Some(index), "{label}: match at {index}");
        assert_eq!(actual_match.1, expected.1[..=index]);
    }
    expected.1
}

fn source_db(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/search.py", source)
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = system_path_to_file(db, "/src/search.py")?;
    let module = ProgramFile::new(db, file, env.program(db));
    Ok(global_symbol(db, module, name).place.expect_type())
}

fn eager_context<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> GenericContext<'db> {
    let declarations = [
        (
            "T",
            TypeVarBoundOrConstraints::Constraints(TypeVarConstraints::new(
                db,
                vec![Type::int_literal(1), Type::int_literal(2)].into_boxed_slice(),
            )),
            Type::int_literal(3),
        ),
        (
            "U",
            TypeVarBoundOrConstraints::UpperBound(Type::int_literal(4)),
            Type::int_literal(5),
        ),
    ];
    GenericContext::from_typevar_instances(
        db,
        env,
        declarations.into_iter().map(|(name, bounds, default)| {
            let variable = TypeVarInstance::new(
                db,
                TypeVarIdentity::new(db, Name::new_static(name), None, TypeVarKind::Pep695TypeVar),
                Some(bounds.into()),
                None,
                Some(TypeVarDefaultEvaluation::Eager(default)),
            );
            BoundTypeVarInstance::new(
                db,
                variable,
                BindingContext::Synthetic(env.program(db)),
                None,
                TypeVarNonce::NONE,
            )
        }),
    )
}

#[test]
fn shared_descendants_preserve_repeated_predicate_calls() {
    let db = setup_db();
    let env = db.program_environment();
    let leaf = Type::int_literal(1);
    let shared = Type::TypeForm(TypeFormType::new(&db, leaf));
    let root = Type::tuple(TupleType::heterogeneous(&db, &env, [shared, shared, leaf]));
    for (mode, label) in modes() {
        let actual = assert_equivalent(&db, &env, root, mode, label);
        assert_eq!(actual, [root, shared, leaf, shared, leaf]);
        let (found, visited) = trace(&db, &env, root, mode, Some(3), Engine::Cursor);
        assert_eq!(found, Some(3));
        assert_eq!(visited, [root, shared, leaf, shared]);
    }

    let children: Vec<_> = (0..12)
        .map(|index| Type::TypeForm(TypeFormType::new(&db, Type::int_literal(index))))
        .collect();
    let root = Type::tuple(TupleType::heterogeneous(
        &db,
        &env,
        children.iter().copied().chain(children.iter().copied()),
    ));
    for (mode, label) in modes() {
        let actual = assert_equivalent(&db, &env, root, mode, label);
        assert_eq!(actual.len(), 1 + 2 * children.len() + children.len());
    }
}

#[test]
fn eager_declarations_precede_specialization_arguments() -> anyhow::Result<()> {
    let db = source_db("class Scope[T, U]: ...\n")?;
    let env = db.program_environment();
    let Type::ClassLiteral(ClassLiteral::Static(origin)) = symbol(&db, "Scope")? else {
        anyhow::bail!("Scope did not produce a static class literal");
    };
    let context = eager_context(&db, &env);
    let alias = Type::GenericAlias(GenericAlias::new(
        &db,
        origin,
        context.specialize(&db, vec![Type::int_literal(6), Type::int_literal(7)]),
    ));
    let expected: Vec<_> = std::iter::once(alias)
        .chain((1..=7).map(Type::int_literal))
        .collect();
    for (mode, label) in modes() {
        assert_eq!(assert_equivalent(&db, &env, alias, mode, label), expected);
    }

    let other = Type::GenericAlias(GenericAlias::new(
        &db,
        origin,
        context.specialize(&db, vec![Type::int_literal(8), Type::int_literal(9)]),
    ));
    let shared = Type::tuple(TupleType::heterogeneous(&db, &env, [alias, other, alias]));
    for (mode, label) in modes() {
        let visited = assert_equivalent(&db, &env, shared, mode, label);
        let mut expected = vec![shared, alias];
        expected.extend((1..=7).map(Type::int_literal));
        expected.push(other);
        expected.extend((1..=5).map(Type::int_literal));
        expected.extend([Type::int_literal(8), Type::int_literal(9), alias]);
        assert_eq!(visited, expected);
    }

    let declaration = Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(context));
    let expected: Vec<_> = std::iter::once(declaration)
        .chain((1..=5).map(Type::int_literal))
        .collect();
    for (mode, label) in modes() {
        assert_eq!(
            assert_equivalent(&db, &env, declaration, mode, label),
            expected
        );
    }
    Ok(())
}

#[test]
fn lazy_declarations_preserve_bounds_defaults_and_modes() -> anyhow::Result<()> {
    let db = source_db(
        "class Scope[T: int = int, U: (bytes, str) = bytes]: ...\nvalue: Scope[int, str]\n",
    )?;
    let env = db.program_environment();
    let root = symbol(&db, "value")?;
    let Type::NominalInstance(_) = root else {
        anyhow::bail!("value did not produce a nominal instance");
    };
    for (mode, label) in modes() {
        let visited = assert_equivalent(&db, &env, root, mode, label);
        assert!(!visited.iter().any(|ty| matches!(ty, Type::TypeVar(_))));
        let bytes = KnownClass::Bytes.to_instance(&db, &env);
        assert_eq!(
            visited.contains(&bytes),
            matches!(mode, TypeSearchMode::IncludeLazyAttributes),
        );
    }
    Ok(())
}

#[test]
fn aliases_distinguish_unused_arguments_and_runtime_values() -> anyhow::Result<()> {
    let db = source_db(
        r#"
from typing import TypeAliasType, TypeVar

type Drop[T] = int
pep695: Drop[bytes]
T = TypeVar("T")
ManualDrop = TypeAliasType("ManualDrop", int, type_params=(T,))
manual: ManualDrop[bytes]
"#,
    )?;
    let env = db.program_environment();
    let argument = KnownClass::Bytes.to_instance(&db, &env);
    let value = KnownClass::Int.to_instance(&db, &env);
    for name in ["pep695", "manual"] {
        let root = symbol(&db, name)?;
        let Type::TypeAlias(alias) = root else {
            anyhow::bail!("{name} did not retain its alias type");
        };
        for (mode, label) in modes() {
            let visited = assert_equivalent(&db, &env, root, mode, label);
            match mode {
                TypeSearchMode::SkipLazyAttributes => assert_eq!(visited, [root]),
                TypeSearchMode::IncludeLazyAttributes => {
                    assert!(visited.contains(&value));
                    assert!(!visited.contains(&argument));
                }
                TypeSearchMode::IncludeAliasArguments => {
                    assert!(visited.contains(&argument));
                    assert!(!visited.contains(&value));
                }
            }
        }

        let runtime = Type::KnownInstance(KnownInstanceType::TypeAliasType(alias));
        for (mode, label) in modes() {
            let visited = assert_equivalent(&db, &env, runtime, mode, label);
            assert!(!visited.contains(&root));
            assert!(!visited.contains(&argument));
            if matches!(mode, TypeSearchMode::IncludeLazyAttributes) {
                assert!(visited.contains(&value));
            } else {
                assert_eq!(visited, [runtime]);
            }
        }
    }
    Ok(())
}

#[test]
fn callable_components_exclude_defaults_and_paramspec_returns() {
    let db = setup_db();
    let env = db.program_environment();
    let context = eager_context(&db, &env);
    let default_marker = Type::int_literal(99);
    let signatures = CallableSignature::from_overloads([
        Signature::new_generic(
            Some(context),
            Parameters::standard([
                Parameter::positional_only(None)
                    .with_annotated_type(Type::int_literal(6))
                    .with_default_type(default_marker),
                Parameter::positional_only(None)
                    .with_annotated_type(Type::int_literal(7))
                    .with_default_type(default_marker),
            ]),
            Type::int_literal(8),
        ),
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::int_literal(9))
            ]),
            Type::int_literal(10),
        ),
    ]);
    let callable = CallableType::new(&db, signatures, CallableTypeKind::Regular);
    for root in [
        Type::Callable(callable),
        Type::KnownInstance(KnownInstanceType::Callable(callable)),
    ] {
        let expected: Vec<_> = std::iter::once(root)
            .chain((1..=10).map(Type::int_literal))
            .collect();
        for (mode, label) in modes() {
            let visited = assert_equivalent(&db, &env, root, mode, label);
            assert_eq!(visited, expected);
            assert!(!visited.contains(&default_marker));
        }
    }

    let paramspec = Type::paramspec_value_callable(
        &db,
        Parameters::standard([Parameter::positional_only(None)
            .with_annotated_type(Type::int_literal(11))
            .with_default_type(default_marker)]),
    );
    for (mode, label) in modes() {
        assert_eq!(
            assert_equivalent(&db, &env, paramspec, mode, label),
            [paramspec, Type::int_literal(11)],
        );
    }
}

#[test]
fn functions_walk_only_updated_signature_components() -> anyhow::Result<()> {
    let db = source_db("def f(value: bytes) -> str: ...\n")?;
    let env = db.program_environment();
    let original = symbol(&db, "f")?;
    let Type::FunctionLiteral(function) = original else {
        anyhow::bail!("f did not produce a function literal");
    };
    for (mode, label) in modes() {
        assert_eq!(
            assert_equivalent(&db, &env, original, mode, label),
            [original]
        );
    }

    let implementation = CallableType::single(
        &db,
        Signature::new(Parameters::standard([]), Type::int_literal(11)),
    );
    let updated = Type::FunctionLiteral(
        function
            .with_inherited_generic_context(&db, eager_context(&db, &env))
            .with_implementation_callables(&db, Box::new([implementation, implementation])),
    );
    let declaration_markers: Vec<_> = (1..=5).map(Type::int_literal).collect();
    for (mode, label) in modes() {
        let visited = assert_equivalent(&db, &env, updated, mode, label);
        assert!(!visited.contains(&Type::Callable(implementation)));
        assert_eq!(visited[visited.len() - 2..], [Type::int_literal(11); 2]);
        assert_eq!(visited[1..=5], declaration_markers);
    }
    Ok(())
}

#[test]
fn receiver_constraints_precede_annotated_parameters() -> anyhow::Result<()> {
    let db = source_db("def f[T: int](self: T, value: bytes) -> str: ...\n")?;
    let env = db.program_environment();
    let Type::FunctionLiteral(function) = symbol(&db, "f")? else {
        anyhow::bail!("f did not produce a function literal");
    };
    let signature = function.literal(&db).last_definition.signature(&db);
    let variable = signature.parameters()[0].annotated_type();
    let receiver = KnownClass::Int.to_instance(&db, &env);
    let bound = signature.bind_self_with_receiver(&db, &env, Some(receiver), Some(receiver));
    assert!(bound.receiver_constraint_types().any(|ty| ty == variable));
    let root = Type::Callable(CallableType::single(&db, bound));
    let parameter = KnownClass::Bytes.to_instance(&db, &env);
    for (mode, label) in modes() {
        let visited = assert_equivalent(&db, &env, root, mode, label);
        let variable_index = visited.iter().position(|ty| *ty == variable);
        let parameter_index = visited.iter().position(|ty| *ty == parameter);
        assert!(variable_index.is_some());
        assert!(parameter_index.is_some());
        assert!(variable_index < parameter_index);
    }
    Ok(())
}

#[test]
fn protocol_modes_distinguish_arguments_from_sorted_members() -> anyhow::Result<()> {
    let db = source_db(
        r#"
from typing import Protocol

class P[T](Protocol):
    z: str
    a: bytes

value: P[int]
"#,
    )?;
    let env = db.program_environment();
    let root = symbol(&db, "value")?;
    let Type::ProtocolInstance(_) = root else {
        anyhow::bail!("value did not produce a protocol instance");
    };
    let argument = KnownClass::Int.to_instance(&db, &env);
    let first_member = KnownClass::Bytes.to_instance(&db, &env);
    let second_member = KnownClass::Str.to_instance(&db, &env);
    for (mode, label) in modes() {
        let visited = assert_equivalent(&db, &env, root, mode, label);
        if matches!(mode, TypeSearchMode::IncludeLazyAttributes) {
            assert!(!visited.contains(&argument));
            let first_index = visited.iter().position(|ty| *ty == first_member);
            let second_index = visited.iter().position(|ty| *ty == second_member);
            assert!(first_index.is_some());
            assert!(second_index.is_some());
            assert!(first_index < second_index);
        } else {
            assert!(visited.contains(&argument));
            assert!(!visited.contains(&first_member));
            assert!(!visited.contains(&second_member));
        }
    }

    let synthesized = Type::protocol_with_readonly_members(
        &db,
        &env,
        [("z", Type::int_literal(2)), ("a", Type::int_literal(1))],
    );
    for (mode, label) in modes() {
        assert_eq!(
            assert_equivalent(&db, &env, synthesized, mode, label),
            [synthesized, Type::int_literal(1), Type::int_literal(2)],
        );
    }
    Ok(())
}

#[test]
fn typed_dicts_preserve_class_fields_and_explicit_extra_items() -> anyhow::Result<()> {
    let db = source_db(
        r#"
from typing_extensions import TypedDict

class Data(TypedDict, extra_items=int):
    z: str
    a: bytes

value: Data
"#,
    )?;
    let env = db.program_environment();
    let root = symbol(&db, "value")?;
    let Type::TypedDict(TypedDictType::Class(class)) = root else {
        anyhow::bail!("value did not produce a class TypedDict");
    };
    let first_member = KnownClass::Bytes.to_instance(&db, &env);
    let second_member = KnownClass::Str.to_instance(&db, &env);
    let extra = KnownClass::Int.to_instance(&db, &env);
    for (mode, label) in modes() {
        let visited = assert_equivalent(&db, &env, root, mode, label);
        if matches!(mode, TypeSearchMode::IncludeLazyAttributes) {
            let members: Vec<_> = visited
                .iter()
                .copied()
                .filter(|ty| [first_member, second_member, extra].contains(ty))
                .collect();
            assert_eq!(members, [first_member, second_member, extra]);
        } else {
            assert_eq!(visited, [root, class.into()]);
        }
    }

    let items: TypedDictSchema<'_> = [
        (
            Name::new_static("z"),
            TypedDictFieldBuilder::new(Type::int_literal(2)).build(),
        ),
        (
            Name::new_static("a"),
            TypedDictFieldBuilder::new(Type::int_literal(1)).build(),
        ),
    ]
    .into_iter()
    .collect();
    for (openness, extra) in [
        (TypedDictOpenness::ImplicitlyOpen, None),
        (
            TypedDictOpenness::extra(&db, Type::int_literal(3), false),
            Some(Type::int_literal(3)),
        ),
    ] {
        let synthesized =
            Type::TypedDict(TypedDictType::Synthesized(SynthesizedTypedDictType::new(
                &db,
                items.clone(),
                SynthesizedTypedDictKind::Schema,
                openness,
            )));
        let expected: Vec<_> = [synthesized, Type::int_literal(1), Type::int_literal(2)]
            .into_iter()
            .chain(extra)
            .collect();
        for (mode, label) in modes() {
            assert_eq!(
                assert_equivalent(&db, &env, synthesized, mode, label),
                expected
            );
        }
    }
    Ok(())
}

#[test]
fn newtypes_distinguish_lazy_bases_and_runtime_components() -> anyhow::Result<()> {
    let db = source_db("from typing import NewType\nUser = NewType(\"User\", int)\n")?;
    let env = db.program_environment();
    let Type::KnownInstance(KnownInstanceType::NewType(lazy)) = symbol(&db, "User")? else {
        anyhow::bail!("User did not produce a NewType declaration");
    };
    let eager = NewType::new(
        &db,
        Name::new_static("Eager"),
        lazy.definition(&db),
        Some(NewTypeBase::NewType(lazy)),
    );
    let base = KnownClass::Int.to_instance(&db, &env);
    for declaration in [lazy, eager] {
        for root in [
            Type::NewTypeInstance(declaration),
            Type::KnownInstance(KnownInstanceType::NewType(declaration)),
        ] {
            for (mode, label) in modes() {
                let visited = assert_equivalent(&db, &env, root, mode, label);
                assert_eq!(
                    visited.contains(&base),
                    matches!(mode, TypeSearchMode::IncludeLazyAttributes),
                );
                if declaration == eager {
                    assert!(visited.contains(&Type::NewTypeInstance(lazy)));
                } else if !matches!(mode, TypeSearchMode::IncludeLazyAttributes) {
                    assert_eq!(visited, [root]);
                }
                if matches!(root, Type::KnownInstance(_)) {
                    assert!(!visited.contains(&Type::NewTypeInstance(declaration)));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn installed_attempt_admits_stored_newtype_base_handles() -> anyhow::Result<()> {
    let db = source_db(
        "from typing import NewType\nclass Base: ...\nInner = NewType(\"Inner\", Base)\nOuter = NewType(\"Outer\", Inner)\n",
    )?;
    let env = db.program_environment();
    let Type::KnownInstance(KnownInstanceType::NewType(inner)) = symbol(&db, "Inner")? else {
        anyhow::bail!("Inner did not produce a NewType declaration");
    };
    let Type::KnownInstance(KnownInstanceType::NewType(outer)) = symbol(&db, "Outer")? else {
        anyhow::bail!("Outer did not produce a NewType declaration");
    };
    let eager = NewType::new(
        &db,
        outer.name(&db),
        outer.definition(&db),
        Some(NewTypeBase::NewType(inner)),
    );
    let nested = Type::NewTypeInstance(inner);
    for root in [
        Type::NewTypeInstance(eager),
        Type::KnownInstance(KnownInstanceType::NewType(eager)),
    ] {
        for mode in [
            TypeSearchMode::SkipLazyAttributes,
            TypeSearchMode::IncludeAliasArguments,
        ] {
            for find_nested in [false, true] {
                let visited = RefCell::new(Vec::new());
                let outcome = salsa::attempt_probe::try_with_attempt(&db, 1000, || {
                    search(
                        &db,
                        &env,
                        root,
                        mode,
                        |ty| {
                            visited.borrow_mut().push(ty);
                            find_nested && ty == nested
                        },
                        &mut AttemptSearchControl::new(&db),
                    )
                });
                assert_eq!(
                    outcome,
                    Ok(salsa::attempt_probe::AttemptOutcome::Complete(Ok(
                        find_nested
                    )))
                );
                assert_eq!(visited.into_inner(), [root, nested]);
            }
            assert_eq!(
                assert_equivalent(&db, &env, root, mode, "stored NewType base"),
                [root, nested]
            );
        }
    }
    Ok(())
}

fn assert_no_query_execution(db: &TestDb, event_reader: &mut TestDb) {
    let executed: Vec<_> = event_reader
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| match event.kind {
            salsa::EventKind::WillExecute { database_key } => Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            ),
            _ => None,
        })
        .collect();
    assert!(
        executed.is_empty(),
        "unexpected query execution: {executed:?}"
    );
}

#[test]
fn installed_attempt_refuses_eager_newtype_instance_conversion() -> anyhow::Result<()> {
    let db = source_db(
        "from typing import NewType\nclass Base: ...\nToken = NewType(\"Token\", Base)\n",
    )?;
    let env = db.program_environment();
    let Type::KnownInstance(KnownInstanceType::NewType(declaration)) = symbol(&db, "Token")? else {
        anyhow::bail!("Token did not produce a NewType declaration");
    };
    let Some(base_class) = symbol(&db, "Base")?.to_class_type(&db) else {
        anyhow::bail!("Base did not produce a class type");
    };
    let eager = NewType::new(
        &db,
        declaration.name(&db),
        declaration.definition(&db),
        Some(NewTypeBase::ClassType(base_class)),
    );
    let mut event_reader = db.clone();
    for root in [
        Type::NewTypeInstance(eager),
        Type::KnownInstance(KnownInstanceType::NewType(eager)),
    ] {
        for (mode, label) in modes() {
            let visited = RefCell::new(Vec::new());
            event_reader.clear_salsa_events();
            let outcome = salsa::attempt_probe::try_with_attempt(&db, 1000, || {
                let result = search(
                    &db,
                    &env,
                    root,
                    mode,
                    |ty| {
                        visited.borrow_mut().push(ty);
                        false
                    },
                    &mut AttemptSearchControl::new(&db),
                );
                assert_eq!(
                    result,
                    Err(Incomplete::UnsupportedSearchOperation(
                        SearchOperation::NewTypeInstance
                    ))
                );
                assert!(salsa::attempt_probe::is_incomplete(&db));
            });
            assert_eq!(
                outcome,
                Ok(salsa::attempt_probe::AttemptOutcome::Incomplete(
                    salsa::attempt_probe::Incomplete::Interrupted
                ))
            );
            assert_eq!(visited.into_inner(), [root]);
            assert_no_query_execution(&db, &mut event_reader);

            let visited = assert_equivalent(&db, &env, root, mode, label);
            assert!(visited.contains(&Type::instance(&db, &env, base_class)));
        }
    }
    Ok(())
}

#[test]
fn completed_self_search_can_refuse_before_mapping() {
    let db = setup_db();
    let env = db.program_environment();
    let receiver = Type::object();
    let self_type = Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        &db,
        receiver,
        BindingContext::Synthetic(env.program(&db)),
    ));
    let root = Type::TypeForm(TypeFormType::new(&db, self_type));
    let without_self = Type::TypeForm(TypeFormType::new(&db, Type::int_literal(1)));
    assert_eq!(
        root.try_contains_self(&db, &env, &mut Unrestricted),
        Ok(true)
    );
    let mut event_reader = db.clone();
    event_reader.clear_salsa_events();
    let outcome = salsa::attempt_probe::try_with_attempt(&db, 1000, || {
        let mut control = AttemptSearchControl::new(&db);
        assert_eq!(
            without_self.try_bind_self_typevars(&db, &env, receiver, &mut control, |_, _| {
                Err(crate::types::constructor::expansion_probe::refuse(
                    &db,
                    Incomplete::Interrupted,
                ))
            }),
            Ok(without_self)
        );
        assert_eq!(
            root.try_bind_self_typevars(&db, &env, receiver, &mut control, |_, _| {
                Err(crate::types::constructor::expansion_probe::refuse(
                    &db,
                    Incomplete::Interrupted,
                ))
            }),
            Err(Incomplete::Interrupted)
        );
        assert!(salsa::attempt_probe::is_incomplete(&db));
    });
    assert_eq!(
        outcome,
        Ok(salsa::attempt_probe::AttemptOutcome::Incomplete(
            salsa::attempt_probe::Incomplete::Interrupted
        ))
    );
    assert_no_query_execution(&db, &mut event_reader);
    assert_eq!(
        root.bind_self_typevars(&db, &env, receiver),
        Type::TypeForm(TypeFormType::new(&db, receiver))
    );
}

#[derive(Debug, PartialEq, Eq)]
enum ColdEvent {
    Read(String),
    Predicate(usize),
}

fn take_lazy_reads(db: &TestDb, event_reader: &mut TestDb) -> Vec<ColdEvent> {
    event_reader
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            let name = db.ingredient_debug_name(database_key.ingredient_index());
            let name = name
                .rsplit("::")
                .next()
                .unwrap_or(name.as_ref())
                .trim_end_matches('_');
            matches!(name, "lazy_bound_unchecked" | "lazy_default_unchecked")
                .then(|| ColdEvent::Read(name.to_owned()))
        })
        .collect()
}

fn cold_declaration_reads(
    engine: Engine,
    mode: TypeSearchMode,
    stop_at: Option<usize>,
) -> anyhow::Result<(Option<usize>, usize, Vec<ColdEvent>)> {
    let db = source_db(
        "class First: ...\nclass Second: ...\nclass Scope[T: First = First, U: Second = Second]: ...\n",
    )?;
    let event_reader = RefCell::new(db.clone());
    let env = db.program_environment();
    let Type::ClassLiteral(ClassLiteral::Static(class)) = symbol(&db, "Scope")? else {
        anyhow::bail!("Scope did not produce a static class literal");
    };
    let Some(context) = class.generic_context(&db) else {
        anyhow::bail!("Scope did not retain its generic context");
    };
    let root = Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(context));
    event_reader.borrow_mut().clear_salsa_events();
    let timeline = RefCell::new(Vec::new());
    let predicate_count = Cell::new(0);
    let query = |_| {
        let mut timeline = timeline.borrow_mut();
        timeline.extend(take_lazy_reads(&db, &mut event_reader.borrow_mut()));
        let index = predicate_count.get();
        predicate_count.set(index + 1);
        timeline.push(ColdEvent::Predicate(index));
        (stop_at == Some(index)).then_some(index)
    };
    let found = match engine {
        Engine::Recursive => recursive_search_reference(&db, &env, root, mode, query),
        Engine::Cursor => match search(&db, &env, root, mode, query, &mut Unrestricted) {
            Ok(found) => found,
            Err(error) => match error {},
        },
    };
    timeline
        .borrow_mut()
        .extend(take_lazy_reads(&db, &mut event_reader.borrow_mut()));
    Ok((found, predicate_count.get(), timeline.into_inner()))
}

#[test]
fn cold_lazy_reads_match_even_after_the_first_predicate_match() -> anyhow::Result<()> {
    for (mode, label) in modes() {
        for stop_at in [None, Some(0), Some(1)] {
            let expected = cold_declaration_reads(Engine::Recursive, mode, stop_at)?;
            let actual = cold_declaration_reads(Engine::Cursor, mode, stop_at)?;
            assert_eq!(actual, expected, "{label}: stop at {stop_at:?}");
            eprintln!("WALK_BASELINE cold mode={label:?} stop={stop_at:?} outcome={actual:?}");
            let reads: Vec<_> = actual
                .2
                .iter()
                .filter_map(|event| match event {
                    ColdEvent::Read(name) => Some(name.as_str()),
                    ColdEvent::Predicate(_) => None,
                })
                .collect();
            if stop_at == Some(0) {
                assert_eq!(actual.0, Some(0));
                assert_eq!(actual.1, 1);
                assert!(reads.is_empty());
            } else if matches!(mode, TypeSearchMode::IncludeLazyAttributes) {
                assert_eq!(
                    reads,
                    [
                        "lazy_bound_unchecked",
                        "lazy_default_unchecked",
                        "lazy_bound_unchecked",
                        "lazy_default_unchecked",
                    ],
                );
                if stop_at.is_some() {
                    assert_eq!(actual.0, Some(1));
                    assert_eq!(actual.1, 2);
                }
            } else {
                assert_eq!(actual.0, None);
                assert_eq!(actual.1, 1);
                assert!(reads.is_empty());
            }
        }
    }
    Ok(())
}

struct Limited {
    allowance: usize,
    accepted: usize,
    peak_pending: usize,
    refused: Option<SearchWork>,
}

impl Limited {
    fn new(allowance: usize) -> Self {
        Self {
            allowance,
            accepted: 0,
            peak_pending: 0,
            refused: None,
        }
    }
}

impl SearchControl for Limited {
    type Error = SearchWork;

    fn admit(&mut self, work: SearchWork) -> Result<(), Self::Error> {
        if self.accepted == self.allowance {
            self.refused.get_or_insert(work);
            return Err(work);
        }
        assert!(self.refused.is_none());
        self.accepted += 1;
        if let SearchWork::PendingFrame { held } = work {
            self.peak_pending = self.peak_pending.max(held + 1);
        }
        Ok(())
    }
}

#[test]
fn refusal_at_each_step_is_distinct_from_a_search_result() {
    let db = setup_db();
    let env = db.program_environment();
    let target = Type::int_literal(1);
    let other = Type::TypeForm(TypeFormType::new(&db, Type::int_literal(2)));
    let root = Type::tuple(TupleType::heterogeneous(&db, &env, [target, other]));
    for matches in [false, true] {
        let query = |ty| matches && ty == target;
        let mut complete = Limited::new(usize::MAX);
        assert_eq!(
            search(
                &db,
                &env,
                root,
                TypeSearchMode::SkipLazyAttributes,
                query,
                &mut complete
            ),
            Ok(matches),
        );
        for allowance in 0..complete.accepted {
            let mut limited = Limited::new(allowance);
            let result = search(
                &db,
                &env,
                root,
                TypeSearchMode::SkipLazyAttributes,
                query,
                &mut limited,
            );
            assert_eq!(result.err(), limited.refused);
            assert!(limited.refused.is_some());
            assert_eq!(limited.accepted, allowance);
            assert!(limited.peak_pending <= allowance);
        }
        let mut retry = Limited::new(complete.accepted);
        assert_eq!(
            search(
                &db,
                &env,
                root,
                TypeSearchMode::SkipLazyAttributes,
                query,
                &mut retry
            ),
            Ok(matches),
        );
        assert_eq!(retry.accepted, complete.accepted);
    }
}

#[test]
fn deep_and_wide_stored_types_use_admitted_pending_work() {
    let db = setup_db();
    let env = db.program_environment();
    let leaf = Type::int_literal(1);
    let depth = 4096;
    let mut deep = leaf;
    for _ in 0..depth {
        deep = Type::TypeForm(TypeFormType::new(&db, deep));
    }
    let wide = Type::tuple(TupleType::heterogeneous(
        &db,
        &env,
        std::iter::repeat_n(deep, 256),
    ));
    for root in [deep, wide] {
        for allowance in [128, 256, 512] {
            let mut limited = Limited::new(allowance);
            assert!(
                search(
                    &db,
                    &env,
                    root,
                    TypeSearchMode::SkipLazyAttributes,
                    |ty| ty == leaf,
                    &mut limited,
                )
                .is_err()
            );
            assert_eq!(limited.accepted, allowance);
            assert!(limited.peak_pending <= allowance);
        }
        let mut complete = Limited::new(100_000);
        assert_eq!(
            search(
                &db,
                &env,
                root,
                TypeSearchMode::SkipLazyAttributes,
                |ty| ty == leaf,
                &mut complete,
            ),
            Ok(true)
        );
        assert!(complete.accepted > depth);
        assert!(complete.peak_pending < 8);
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("source control unexpectedly suspended"),
    }
}
fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}
#[derive(Default)]
struct Collections {
    work: Vec<TddWork>,
    refuse_growth: Option<AllocationKind>,
    refuse_access: Option<usize>,
    accesses: usize,
}
impl TddControl for Collections {
    type Error = TddWork;
    fn admit(&mut self, work: TddWork) -> Result<(), TddWork> {
        self.work.push(work);
        if let TddWork::TypeWalkAccess { .. } = work {
            let index = self.accesses;
            self.accesses += 1;
            if self.refuse_access == Some(index) {
                return Err(work);
            }
        }
        if let TddWork::Grow { allocation, .. } = work
            && self.refuse_growth == Some(allocation)
        {
            return Err(work);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DepthEvent<'db> {
    Nominal(NominalInstanceType<'db>),
    Enter(Type<'db>),
    Leave(Type<'db>),
}
struct PendingWitness(Rc<Cell<usize>>);
impl Drop for PendingWitness {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

/// Search operations whose ordering determines when traversal state is needed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchEvent<'db> {
    DecideVisit(Type<'db>),
    PredicateAdmission,
    Predicate(Type<'db>),
    NewState,
    ScheduleDescent(Type<'db>),
    RememberAdmission,
    Remember(Type<'db>),
    Expand,
    ReadField(TypeWalkFieldOperation),
}

struct WalkApi<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    collections: Collections,
    visited: Vec<Type<'db>>,
    match_type: Option<Type<'db>>,
    search_events: Vec<SearchEvent<'db>>,
    refuse_search_event: Option<usize>,
    enqueued_actions: usize,
    queued_visits: Vec<Type<'db>>,
    field_reads: Vec<TypeWalkFieldOperation>,
    refuse_field: Option<TypeWalkFieldOperation>,
    depth_events: Vec<DepthEvent<'db>>,
    suspend_newtype: bool,
    newtype_calls: usize,
    pending_drops: Rc<Cell<usize>>,
    support_occurrences: Vec<BoundTypeVarInstance<'db>>,
    skipped_lazy: usize,
}
impl<'env, 'db> WalkApi<'env, 'db> {
    fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self {
            db,
            env,
            collections: Collections::default(),
            visited: Vec::new(),
            match_type: None,
            search_events: Vec::new(),
            refuse_search_event: None,
            enqueued_actions: 0,
            queued_visits: Vec::new(),
            field_reads: Vec::new(),
            refuse_field: None,
            depth_events: Vec::new(),
            suspend_newtype: false,
            newtype_calls: 0,
            pending_drops: Rc::default(),
            support_occurrences: Vec::new(),
            skipped_lazy: 0,
        }
    }

    /// Records a search operation and refuses the selected operation before it runs.
    fn search_event(&mut self, event: SearchEvent<'db>) -> Result<(), TddError<TddWork>> {
        let index = self.search_events.len();
        self.search_events.push(event);
        if self.refuse_search_event == Some(index) {
            return Err(TddError::Refused(TddWork::TypeWalkAccess { units: 1 }));
        }
        Ok(())
    }

    fn read_field(&mut self, operation: TypeWalkFieldOperation) -> Result<(), TddError<TddWork>> {
        self.search_event(SearchEvent::ReadField(operation))?;
        self.field_reads.push(operation);
        if self.refuse_field == Some(operation) {
            return Err(TddError::Refused(TddWork::TypeWalkAccess { units: 1 }));
        }
        Ok(())
    }

    fn ordinary<'a>(
        &self,
        control: &'a mut Unrestricted,
    ) -> OrdinaryTypeWalk<'a, 'env, 'db, Unrestricted, ()> {
        OrdinaryTypeWalk {
            db: self.db,
            env: self.env,
            control,
            query: (),
        }
    }
}
impl<'db> TypeWalkEffects<'db> for WalkApi<'_, 'db> {
    type Error = TddError<TddWork>;
    async fn union_elements(
        &mut self,
        ty: UnionType<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.read_field(TypeWalkFieldOperation::UnionElements)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).union_elements(ty),
        ))
    }
    async fn intersection_positive(
        &mut self,
        ty: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::IntersectionPositive)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).intersection_positive(ty),
        ))
    }
    async fn intersection_negative(
        &mut self,
        ty: IntersectionType<'db>,
    ) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::IntersectionNegative)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).intersection_negative(ty),
        ))
    }
    async fn enum_rest(
        &mut self,
        ty: EnumComplementType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::EnumRest)?;
        Ok(infallible(self.ordinary(&mut Unrestricted).enum_rest(ty)))
    }
    async fn function_signature(
        &mut self,
        ty: FunctionType<'db>,
    ) -> Result<Option<&'db CallableSignature<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::FunctionSignature)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).function_signature(ty),
        ))
    }
    async fn function_implementations(
        &mut self,
        ty: FunctionType<'db>,
    ) -> Result<Option<&'db [CallableType<'db>]>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::FunctionImplementations)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted)
                .function_implementations(ty),
        ))
    }
    async fn callable_signatures(
        &mut self,
        ty: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::CallableSignatures)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).callable_signatures(ty),
        ))
    }
    async fn method_func(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::MethodFunc)?;
        Ok(infallible(self.ordinary(&mut Unrestricted).method_func(ty)))
    }
    async fn method_self(&mut self, ty: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::MethodSelf)?;
        Ok(infallible(self.ordinary(&mut Unrestricted).method_self(ty)))
    }
    async fn method_receiver(
        &mut self,
        ty: BoundMethodType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::MethodReceiver)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).method_receiver(ty),
        ))
    }
    async fn bound_super_children(
        &mut self,
        ty: BoundSuperType<'db>,
    ) -> Result<[Option<Type<'db>>; 3], Self::Error> {
        self.read_field(TypeWalkFieldOperation::BoundSuperChildren)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).bound_super_children(ty),
        ))
    }
    async fn alias_specialization(
        &mut self,
        ty: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::AliasSpecialization)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).alias_specialization(ty),
        ))
    }
    async fn specialization_context(
        &mut self,
        ty: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::SpecializationContext)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).specialization_context(ty),
        ))
    }
    async fn specialization_types(
        &mut self,
        ty: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.read_field(TypeWalkFieldOperation::SpecializationTypes)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).specialization_types(ty),
        ))
    }
    async fn specialization_tuple(
        &mut self,
        ty: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::SpecializationTuple)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).specialization_tuple(ty),
        ))
    }
    async fn context_variable(
        &mut self,
        ty: GenericContext<'db>,
        index: usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::ContextVariable)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).context_variable(ty, index),
        ))
    }
    async fn bound_typevar(
        &mut self,
        ty: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::BoundTypeVar)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).bound_typevar(ty),
        ))
    }
    async fn eager_typevar_bounds(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<(Option<TypeVarBoundOrConstraints<'db>>, bool), Self::Error> {
        self.read_field(TypeWalkFieldOperation::EagerTypeVarBounds)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).eager_typevar_bounds(ty),
        ))
    }
    async fn eager_typevar_default(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<(Option<Type<'db>>, bool), Self::Error> {
        self.read_field(TypeWalkFieldOperation::EagerTypeVarDefault)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).eager_typevar_default(ty),
        ))
    }
    async fn constraint_elements(
        &mut self,
        ty: TypeVarConstraints<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.read_field(TypeWalkFieldOperation::ConstraintElements)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).constraint_elements(ty),
        ))
    }
    async fn nominal_children(
        &mut self,
        ty: NominalInstanceType<'db>,
    ) -> Result<NominalVisitorChildren<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::NominalChildren)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).nominal_children(ty),
        ))
    }
    async fn protocol_children(
        &mut self,
        ty: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolVisitorChildren<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::ProtocolChildren)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).protocol_children(ty),
        ))
    }
    async fn property_class(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<PropertyInstanceClass<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::PropertyClass)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).property_class(ty),
        ))
    }
    async fn property_getter(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::PropertyGetter)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).property_getter(ty),
        ))
    }
    async fn property_setter(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::PropertySetter)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).property_setter(ty),
        ))
    }
    async fn property_deleter(
        &mut self,
        ty: PropertyInstanceType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::PropertyDeleter)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).property_deleter(ty),
        ))
    }
    async fn slot_value(&mut self, ty: SlotDescriptorType<'db>) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::SlotValue)?;
        Ok(infallible(self.ordinary(&mut Unrestricted).slot_value(ty)))
    }
    async fn type_is_argument(&mut self, ty: TypeIsType<'db>) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::TypeIsArgument)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).type_is_argument(ty),
        ))
    }
    async fn type_guard_return(
        &mut self,
        ty: TypeGuardType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::TypeGuardReturn)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).type_guard_return(ty),
        ))
    }
    async fn type_form_argument(
        &mut self,
        ty: TypeFormType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::TypeFormArgument)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).type_form_argument(ty),
        ))
    }
    async fn alias_arguments(
        &mut self,
        ty: TypeAliasType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::AliasArguments)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).alias_arguments(ty),
        ))
    }
    async fn recursive_arguments(
        &mut self,
        ty: RecursiveType<'db>,
    ) -> Result<Option<Specialization<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::RecursiveArguments)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).recursive_arguments(ty),
        ))
    }
    async fn interface_members(
        &mut self,
        ty: ProtocolInterfaceView<'db>,
    ) -> Result<btree_map::Iter<'db, Name, ProtocolMemberData<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::InterfaceMembers)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).interface_members(ty),
        ))
    }
    async fn synthesized_typed_dict_items(
        &mut self,
        ty: SynthesizedTypedDictType<'db>,
    ) -> Result<&'db TypedDictSchema<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::SynthesizedTypedDictItems)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted)
                .synthesized_typed_dict_items(ty),
        ))
    }
    async fn synthesized_typed_dict_openness(
        &mut self,
        ty: SynthesizedTypedDictType<'db>,
    ) -> Result<TypedDictOpenness<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::SynthesizedTypedDictOpenness)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted)
                .synthesized_typed_dict_openness(ty),
        ))
    }
    async fn eager_newtype_base(
        &mut self,
        ty: NewType<'db>,
    ) -> Result<Option<NewTypeBase<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::EagerNewTypeBase)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).eager_newtype_base(ty),
        ))
    }
    async fn solution_bindings(
        &mut self,
        ty: InternedConstraintSetSolution<'db>,
    ) -> Result<&'db [TypeVarSolution<'db>], Self::Error> {
        self.read_field(TypeWalkFieldOperation::SolutionBindings)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).solution_bindings(ty),
        ))
    }
    async fn field_default(
        &mut self,
        ty: FieldInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::FieldDefault)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).field_default(ty),
        ))
    }
    async fn field_converter(
        &mut self,
        ty: FieldInstance<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::FieldConverter)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).field_converter(ty),
        ))
    }
    async fn union_value(
        &mut self,
        ty: UnionTypeInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::UnionValue)?;
        Ok(infallible(self.ordinary(&mut Unrestricted).union_value(ty)))
    }
    async fn interned_type(&mut self, ty: InternedType<'db>) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::InternedType)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).interned_type(ty),
        ))
    }
    async fn named_tuple_fields(
        &mut self,
        ty: NamedTupleSpec<'db>,
    ) -> Result<&'db [NamedTupleField<'db>], Self::Error> {
        self.read_field(TypeWalkFieldOperation::NamedTupleFields)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).named_tuple_fields(ty),
        ))
    }
    async fn partial_callable(
        &mut self,
        ty: FunctoolsPartialInstance<'db>,
    ) -> Result<CallableType<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::PartialCallable)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).partial_callable(ty),
        ))
    }
    async fn method_wrapper_type(
        &mut self,
        ty: MethodWrapper<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.read_field(TypeWalkFieldOperation::MethodWrapperType)?;
        Ok(infallible(
            self.ordinary(&mut Unrestricted).method_wrapper_type(ty),
        ))
    }
    async fn remember_type(
        &mut self,
        seen: &mut TypeCollector<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.search_event(SearchEvent::Remember(ty))?;
        seen.type_was_already_seen_with(ty, &mut self.collections)
    }

    async fn push_action(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Self::Error> {
        super::push_type_walk_action_with(cursor, action, TypeWalkFacts, self).await
    }
    async fn push_visit(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        super::push_type_walk_visit_with(cursor, ty, TypeWalkFacts, self).await
    }
    async fn push_tuple(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        tuple: &'db TupleSpec<'db>,
    ) -> Result<(), Self::Error> {
        super::push_type_walk_tuple_with(cursor, tuple, TypeWalkFacts, self).await
    }
    async fn expand_children(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        kind: NonAtomicType<'db>,
        policy: TypeWalkPolicy,
    ) -> Result<(), Self::Error> {
        self.search_event(SearchEvent::Expand)?;
        super::expand_type_children_with(cursor, kind, policy, TypeWalkFacts, self).await
    }
    async fn expand_wrapper(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        wrapper: KnownBoundMethodType<'db>,
    ) -> Result<(), Self::Error> {
        super::expand_method_wrapper_children_with(cursor, wrapper, TypeWalkFacts, self).await
    }
    async fn expand_known(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        known: KnownInstanceType<'db>,
    ) -> Result<(), Self::Error> {
        super::expand_known_instance_children_with(cursor, known, TypeWalkFacts, self).await
    }
    async fn next_event(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        policy: TypeWalkPolicy,
    ) -> Result<Option<TypeWalkEvent<'db>>, Self::Error> {
        super::next_type_walk_event_with(cursor, policy, TypeWalkFacts, self).await
    }
    async fn checkpoint(&mut self, work: TypeWalkWork) -> Result<(), Self::Error> {
        match work {
            TypeWalkWork::Search(SearchWork::Predicate) => {
                self.search_event(SearchEvent::PredicateAdmission)
            }
            TypeWalkWork::Search(SearchWork::RememberType) => {
                self.search_event(SearchEvent::RememberAdmission)
            }
            TypeWalkWork::Search(
                SearchWork::PendingFrame { .. } | SearchWork::Advance | SearchWork::Semantic(_),
            )
            | TypeWalkWork::DepthVisit
            | TypeWalkWork::DepthEnter
            | TypeWalkWork::DepthExit => Ok(()),
        }
    }
    async fn take_action(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
    ) -> Result<Option<WalkAction<'db>>, Self::Error> {
        if cursor.pending.is_empty() {
            return Ok(None);
        }
        self.checkpoint(TypeWalkWork::Search(SearchWork::Advance))
            .await?;
        Ok(cursor.pending.pop())
    }
    async fn enqueue(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Self::Error> {
        self.enqueued_actions += 1;
        if let WalkAction::Visit(ty) = &action {
            self.queued_visits.push(*ty);
        }
        self.checkpoint(TypeWalkWork::Search(SearchWork::PendingFrame {
            held: cursor.pending.len(),
        }))
        .await?;
        reserve_walk_pending_with(cursor, 1, &mut self.collections)?;
        cursor.pending.push(action);
        Ok(())
    }
    async fn enqueue_visits<const N: usize>(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        children: [Option<Type<'db>>; N],
    ) -> Result<(), Self::Error> {
        for ty in children.into_iter().rev().flatten() {
            self.enqueue(cursor, WalkAction::Visit(ty)).await?;
        }
        Ok(())
    }
    async fn next_stored(
        &mut self,
        types: &mut StoredTypeSequence<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(types.next_type())
    }
    async fn next_member(
        &mut self,
        members: &mut std::collections::btree_map::Iter<'db, Name, ProtocolMemberData<'db>>,
    ) -> Result<Option<(&'db Name, &'db ProtocolMemberData<'db>)>, Self::Error> {
        Ok(members.next())
    }
    async fn constraint_type_step(
        &mut self,
        cursor: &mut OwnedConstraintTypeCursor<'db, 'db>,
    ) -> Result<Option<Option<[Type<'db>; 2]>>, Self::Error> {
        cursor.next_with(&mut self.collections)
    }
    async fn protocol_interface(
        &mut self,
        ty: ProtocolInstanceType<'db>,
    ) -> Result<ProtocolInterfaceView<'db>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).protocol_interface(ty),
        ))
    }
    async fn protocol_member_types(
        &mut self,
        member: ProtocolMember<'db, 'db>,
    ) -> Result<[Option<Type<'db>>; 6], Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted)
                .protocol_member_types(member),
        ))
    }
    async fn typevar_bounds(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).typevar_bounds(ty),
        ))
    }
    async fn typevar_default(
        &mut self,
        ty: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).typevar_default(ty),
        ))
    }
    async fn alias_value(&mut self, ty: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(infallible(self.ordinary(&mut Unrestricted).alias_value(ty)))
    }
    async fn recursive_unfold(&mut self, ty: RecursiveType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).recursive_unfold(ty),
        ))
    }
    async fn typed_dict_items(
        &mut self,
        ty: TypedDictType<'db>,
    ) -> Result<&'db TypedDictSchema<'db>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).typed_dict_items(ty),
        ))
    }
    async fn typed_dict_extra(
        &mut self,
        ty: TypedDictType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).typed_dict_extra(ty),
        ))
    }
    async fn newtype_base(&mut self, ty: NewType<'db>) -> Result<NewTypeBase<'db>, Self::Error> {
        Ok(infallible(
            self.ordinary(&mut Unrestricted).newtype_base(ty),
        ))
    }
    async fn newtype_instance(&mut self, base: NewTypeBase<'db>) -> Result<Type<'db>, Self::Error> {
        self.newtype_calls += 1;
        if self.suspend_newtype {
            let _witness = PendingWitness(self.pending_drops.clone());
            let mut pending = true;
            poll_fn(|cx| {
                if pending {
                    pending = false;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
        }
        Ok(infallible(
            self.ordinary(&mut Unrestricted).newtype_instance(base),
        ))
    }
}
impl<'db> TypeSearchEffects<'db, bool> for WalkApi<'_, 'db> {
    async fn new_state(&mut self) -> Result<(TypeWalkCursor<'db>, TypeCollector<'db>), Self::Error> {
        self.search_event(SearchEvent::NewState)?;
        Ok((TypeWalkFacts.empty_cursor(), TypeWalkFacts.empty_seen()))
    }

    async fn predicate(&mut self, ty: Type<'db>) -> Result<bool, Self::Error> {
        self.search_event(SearchEvent::Predicate(ty))?;
        self.visited.push(ty);
        Ok(self.match_type == Some(ty))
    }

    async fn decide_visit(
        &mut self,
        ty: Type<'db>,
        policy: TypeWalkPolicy,
        found: bool,
    ) -> Result<TypeSearchDecision<'db, bool>, Self::Error> {
        self.search_event(SearchEvent::DecideVisit(ty))?;
        super::decide_type_search_visit_with(ty, policy, found, TypeWalkFacts, self).await
    }

    async fn schedule_descent(
        &mut self,
        cursor: &mut TypeWalkCursor<'db>,
        seen: &mut TypeCollector<'db>,
        descent: TypeSearchDescent<'db>,
    ) -> Result<(), Self::Error> {
        self.search_event(SearchEvent::ScheduleDescent(descent.ty))?;
        super::schedule_type_search_descent_with::<bool, _>(
            cursor,
            seen,
            descent,
            TypeWalkFacts,
            self,
        )
        .await
    }
}
impl<'db> TypeSupportEffects<'db> for WalkApi<'_, 'db> {
    async fn record_occurrence(
        &mut self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<(), Self::Error> {
        self.support_occurrences.push(typevar);
        Ok(())
    }
    async fn skipped_lazy(&mut self) -> Result<(), Self::Error> {
        self.skipped_lazy += 1;
        Ok(())
    }
}
impl<'db> TypeDepthEffects<'db> for WalkApi<'_, 'db> {
    async fn nominal_class(
        &mut self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        self.depth_events.push(DepthEvent::Nominal(instance));
        Ok(infallible(
            self.ordinary(&mut Unrestricted).nominal_class(instance),
        ))
    }
    async fn enter_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.depth_events.push(DepthEvent::Enter(ty));
        enter_depth_active_with(active, ty, &mut self.collections)
    }
    async fn leave_active(
        &mut self,
        active: &mut FxHashSet<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        self.depth_events.push(DepthEvent::Leave(ty));
        leave_depth_active_with(active, ty, &mut self.collections)
    }
}

/// Atomic misses and matching roots finish after one predicate without traversal state.
#[test]
fn terminal_roots_avoid_search_state() {
    let db = setup_db();
    let env = db.program_environment();
    let atomic = Type::int_literal(7);
    let non_atomic = Type::TypeForm(TypeFormType::new(&db, atomic));
    let assert_terminal = |root, match_type| {
        let mut effects = WalkApi::new(&db, &env);
        effects.match_type = match_type;
        assert_eq!(
            ready(search_type_with(
                root,
                TypeSearchMode::SkipLazyAttributes,
                TypeWalkFacts,
                &mut effects,
            )),
            Ok(match_type.is_some()),
        );
        assert_eq!(effects.visited, [root]);
        assert_eq!(
            effects.search_events,
            [
                SearchEvent::DecideVisit(root),
                SearchEvent::PredicateAdmission,
                SearchEvent::Predicate(root),
            ]
        );
        assert_eq!(effects.enqueued_actions, 0);
        assert!(effects.field_reads.is_empty());
        assert!(effects.collections.work.is_empty());
    };
    assert_terminal(atomic, None);
    assert_terminal(atomic, Some(atomic));
    assert_terminal(non_atomic, Some(non_atomic));
}

/// A nonmatching non-atomic root initializes state once and is remembered before expansion.
#[test]
fn root_descent_initializes_once_and_remembers_before_expansion() {
    let db = setup_db();
    let env = db.program_environment();
    let leaf = Type::int_literal(7);
    let root = Type::TypeForm(TypeFormType::new(&db, leaf));
    let mut effects = WalkApi::new(&db, &env);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut effects,
        )),
        Ok(false),
    );
    assert_eq!(
        effects.search_events,
        [
            SearchEvent::DecideVisit(root),
            SearchEvent::PredicateAdmission,
            SearchEvent::Predicate(root),
            SearchEvent::NewState,
            SearchEvent::ScheduleDescent(root),
            SearchEvent::RememberAdmission,
            SearchEvent::Remember(root),
            SearchEvent::Expand,
            SearchEvent::ReadField(TypeWalkFieldOperation::TypeFormArgument),
            SearchEvent::DecideVisit(leaf),
            SearchEvent::PredicateAdmission,
            SearchEvent::Predicate(leaf),
        ]
    );
    assert_eq!(effects.visited, [root, leaf]);
    assert_eq!(effects.queued_visits, [leaf]);
}

/// Refusing root helpers, predicates, or state setup aborts the search and permits a clean retry.
#[test_case::test_case(0; "decision helper")]
#[test_case::test_case(1; "predicate admission")]
#[test_case::test_case(2; "predicate")]
#[test_case::test_case(3; "state initialization")]
#[test_case::test_case(4; "scheduling helper")]
#[test_case::test_case(5; "remember admission")]
#[test_case::test_case(6; "remember")]
fn root_search_refusal_retries_cleanly(refuse_event: usize) {
    let db = setup_db();
    let env = db.program_environment();
    let leaf = Type::int_literal(7);
    let root = Type::TypeForm(TypeFormType::new(&db, leaf));
    let mut complete = WalkApi::new(&db, &env);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut complete,
        )),
        Ok(false),
    );
    let mut refused = WalkApi::new(&db, &env);
    refused.refuse_search_event = Some(refuse_event);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut refused,
        )),
        Err(TddError::Refused(TddWork::TypeWalkAccess { units: 1 })),
    );
    assert_eq!(refused.search_events, complete.search_events[..=refuse_event]);
    assert_eq!(refused.enqueued_actions, 0);
    assert!(refused.field_reads.is_empty());
    assert!(refused.collections.work.is_empty());

    let mut retry = WalkApi::new(&db, &env);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut retry,
        )),
        Ok(false),
    );
    assert_eq!(retry.search_events, complete.search_events);
    assert_eq!(retry.visited, complete.visited);
    assert_eq!(retry.queued_visits, complete.queued_visits);
    assert_eq!(retry.collections.work, complete.collections.work);
}

/// In IncludeAliasArguments mode, a nonmatching alias without arguments reads them,
/// then initializes state and remembers its key.
#[test]
fn alias_without_arguments_still_initializes_search_state() -> anyhow::Result<()> {
    let db = source_db("type Alias = int\nvalue: Alias\n")?;
    let env = db.program_environment();
    let root = symbol(&db, "value")?;
    let Type::TypeAlias(_) = root else {
        anyhow::bail!("value did not retain its alias type");
    };
    let mut effects = WalkApi::new(&db, &env);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::IncludeAliasArguments,
            TypeWalkFacts,
            &mut effects,
        )),
        Ok(false),
    );
    assert_eq!(
        effects.search_events,
        [
            SearchEvent::DecideVisit(root),
            SearchEvent::PredicateAdmission,
            SearchEvent::Predicate(root),
            SearchEvent::ReadField(TypeWalkFieldOperation::AliasArguments),
            SearchEvent::NewState,
            SearchEvent::ScheduleDescent(root),
            SearchEvent::RememberAdmission,
            SearchEvent::Remember(root),
        ]
    );
    assert_eq!(effects.visited, [root]);
    assert_eq!(effects.enqueued_actions, 0);
    assert!(!effects.collections.work.is_empty());
    Ok(())
}

/// A nonmatching root keeps its exact result when it visits no descendants.
/// Negative zero compares equal to `f64::default()`, but has a distinct sign bit.
#[test]
fn default_equivalent_root_result_preserves_its_value() -> anyhow::Result<()> {
    let db = source_db("type Alias = int\nvalue: Alias\n")?;
    let env = db.program_environment();
    let alias = symbol(&db, "value")?;
    let Type::TypeAlias(_) = alias else {
        anyhow::bail!("value did not retain its alias type");
    };
    let assert_negative_zero = |root| {
        let result = infallible(search(
            &db,
            &env,
            root,
            TypeSearchMode::IncludeAliasArguments,
            |_| -0.0_f64,
            &mut Unrestricted,
        ));
        assert_eq!(result.to_bits(), (-0.0_f64).to_bits());
    };
    assert_negative_zero(Type::int_literal(7));
    assert_negative_zero(alias);
    Ok(())
}

#[test]
fn stored_field_reads_preserve_container_order_and_refusal() {
    let db = setup_db();
    let env = db.program_environment();
    let first = Type::TypeForm(TypeFormType::new(&db, Type::int_literal(1)));
    let second = Type::TypeForm(TypeFormType::new(&db, Type::int_literal(2)));
    let third = Type::TypeForm(TypeFormType::new(&db, Type::int_literal(3)));
    let tuple = Type::tuple(TupleType::heterogeneous(&db, &env, [first]));
    let intersection = Type::Intersection(IntersectionType::new(
        &db,
        FxOrderSet::from_iter([tuple]),
        NegativeIntersectionElements::Single(second),
    ));
    let root = Type::Union(UnionType::new(
        &db,
        vec![intersection, third].into_boxed_slice(),
        crate::types::set_theoretic::RecursivelyDefined::No,
    ));
    let expected = trace(
        &db,
        &env,
        root,
        TypeSearchMode::SkipLazyAttributes,
        None,
        Engine::Recursive,
    )
    .1;
    let mut refused = WalkApi::new(&db, &env);
    refused.refuse_field = Some(TypeWalkFieldOperation::IntersectionPositive);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut refused
        )),
        Err(TddError::Refused(TddWork::TypeWalkAccess { units: 1 })),
    );
    assert_eq!(refused.visited, [root, intersection]);
    assert_eq!(
        refused.field_reads,
        [
            TypeWalkFieldOperation::UnionElements,
            TypeWalkFieldOperation::IntersectionNegative,
            TypeWalkFieldOperation::IntersectionPositive,
        ]
    );

    let mut complete = WalkApi::new(&db, &env);
    assert_eq!(
        ready(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut complete
        )),
        Ok(false),
    );
    assert_eq!(complete.visited, expected);
    assert_eq!(
        complete.field_reads,
        [
            TypeWalkFieldOperation::UnionElements,
            TypeWalkFieldOperation::IntersectionNegative,
            TypeWalkFieldOperation::IntersectionPositive,
            TypeWalkFieldOperation::NominalChildren,
            TypeWalkFieldOperation::TypeFormArgument,
            TypeWalkFieldOperation::TypeFormArgument,
            TypeWalkFieldOperation::TypeFormArgument,
        ]
    );
}

#[test]
fn stored_field_reads_follow_generic_declaration_policy() -> anyhow::Result<()> {
    let db = source_db("class Scope[T, U]: ...\n")?;
    let env = db.program_environment();
    let Type::ClassLiteral(ClassLiteral::Static(origin)) = symbol(&db, "Scope")? else {
        anyhow::bail!("Scope did not produce a static class literal");
    };
    let context = eager_context(&db, &env);
    let alias = Type::GenericAlias(GenericAlias::new(
        &db,
        origin,
        context.specialize(&db, vec![Type::int_literal(6), Type::int_literal(7)]),
    ));
    let mut search = WalkApi::new(&db, &env);
    assert_eq!(
        ready(search_type_with(
            alias,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut search
        )),
        Ok(false),
    );
    assert_eq!(
        search.field_reads,
        [
            TypeWalkFieldOperation::AliasSpecialization,
            TypeWalkFieldOperation::SpecializationContext,
            TypeWalkFieldOperation::ContextVariable,
            TypeWalkFieldOperation::BoundTypeVar,
            TypeWalkFieldOperation::EagerTypeVarBounds,
            TypeWalkFieldOperation::ConstraintElements,
            TypeWalkFieldOperation::EagerTypeVarDefault,
            TypeWalkFieldOperation::ContextVariable,
            TypeWalkFieldOperation::BoundTypeVar,
            TypeWalkFieldOperation::EagerTypeVarBounds,
            TypeWalkFieldOperation::EagerTypeVarDefault,
            TypeWalkFieldOperation::ContextVariable,
            TypeWalkFieldOperation::SpecializationTuple,
            TypeWalkFieldOperation::SpecializationTypes,
        ]
    );

    let mut support = WalkApi::new(&db, &env);
    assert_eq!(
        ready(support_type_with(alias, TypeWalkFacts, &mut support)),
        Ok(())
    );
    assert_eq!(
        support.field_reads,
        [
            TypeWalkFieldOperation::AliasSpecialization,
            TypeWalkFieldOperation::SpecializationTypes,
        ]
    );
    Ok(())
}

#[test]
fn depth_effects_preserve_active_scopes() {
    let db = setup_db();
    let env = db.program_environment();
    let variable = BoundTypeVarInstance::synthetic_self(
        &db,
        Type::object(),
        BindingContext::Synthetic(env.program(&db)),
    );
    let shared = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(variable)));
    let deeper = Type::TypeForm(TypeFormType::new(&db, shared));
    let tuple = Type::tuple(TupleType::heterogeneous(&db, &env, [shared, deeper]));
    let signature = Signature::new(Parameters::empty(), tuple);
    let root = Type::Callable(CallableType::single(&db, signature));
    let expected = infallible(type_depth_sync(
        root,
        TypeWalkFacts,
        &mut OrdinaryTypeWalk {
            db: &db,
            env: &env,
            control: &mut Unrestricted,
            query: (),
        },
    ));
    let mut effects = WalkApi::new(&db, &env);
    assert_eq!(
        ready(type_depth_with(
            root,
            TypeWalkFacts,
            &mut effects
        )),
        Ok(expected)
    );
    assert!(expected.0 > 2 && expected.1 > 2);
    let baseline_events = effects
        .depth_events
        .iter()
        .map(|event| match event {
            DepthEvent::Nominal(instance) => format!(
                "nominal {}",
                Type::NominalInstance(*instance).display(&db, &env)
            ),
            DepthEvent::Enter(ty) => format!("enter {}", ty.display(&db, &env)),
            DepthEvent::Leave(ty) => format!("leave {}", ty.display(&db, &env)),
        })
        .collect::<Vec<_>>();
    eprintln!("WALK_BASELINE depth active={expected:?} events={baseline_events:?}");
    let shared_events: Vec<_> = effects
        .depth_events
        .iter()
        .copied()
        .filter(
            |event| matches!(event, DepthEvent::Enter(ty) | DepthEvent::Leave(ty) if *ty == shared),
        )
        .collect();
    assert_eq!(
        shared_events,
        [
            DepthEvent::Enter(shared),
            DepthEvent::Leave(shared),
            DepthEvent::Enter(shared),
            DepthEvent::Leave(shared)
        ]
    );
    let mut active = FxHashSet::default();
    assert!(unrestricted(enter_depth_active_with(
        &mut active,
        shared,
        &mut UnrestrictedCollections
    )));
    assert!(!unrestricted(enter_depth_active_with(
        &mut active,
        shared,
        &mut UnrestrictedCollections
    )));
    unrestricted(leave_depth_active_with(
        &mut active,
        shared,
        &mut UnrestrictedCollections,
    ));
    assert!(unrestricted(enter_depth_active_with(
        &mut active,
        shared,
        &mut UnrestrictedCollections
    )));
}

#[test]
fn depth_nominal_request_precedes_active_guard() -> anyhow::Result<()> {
    let db = source_db("class Plain: ...\nclass Box[T]: ...\nplain: Plain\nbox: Box[int]\n")?;
    let env = db.program_environment();
    for (name, generic) in [("plain", false), ("box", true)] {
        let root = symbol(&db, name)?;
        let Type::NominalInstance(instance) = root else {
            anyhow::bail!("{name} is not a nominal instance");
        };
        let mut effects = WalkApi::new(&db, &env);
        let depth = ready(type_depth_with(
            root,
            TypeWalkFacts,
            &mut effects,
        ));
        assert!(depth.is_ok());
        eprintln!("WALK_BASELINE depth nominal={name} generic={generic} value={depth:?}");
        assert_eq!(
            effects.depth_events.first(),
            Some(&DepthEvent::Nominal(instance))
        );
        assert_eq!(
            effects.depth_events.contains(&DepthEvent::Enter(root)),
            generic
        );
    }
    let variable = BoundTypeVarInstance::synthetic_self(
        &db,
        Type::object(),
        BindingContext::Synthetic(env.program(&db)),
    );
    let mut effects = WalkApi::new(&db, &env);
    assert_eq!(
        ready(type_depth_with(
            Type::TypeVar(variable),
            TypeWalkFacts,
            &mut effects
        )),
        Ok((0, 0))
    );
    assert!(effects.depth_events.is_empty());
    Ok(())
}

#[test]
fn type_walk_semantic_reply_resumes_once() -> anyhow::Result<()> {
    let db = source_db("from typing import NewType\nUser = NewType(\"User\", int)\n")?;
    let env = db.program_environment();
    let Type::KnownInstance(KnownInstanceType::NewType(lazy)) = symbol(&db, "User")? else {
        anyhow::bail!("User is not a NewType declaration");
    };
    let base = lazy.base(&db);
    let eager = Type::NewTypeInstance(NewType::new(
        &db,
        Name::new_static("Eager"),
        lazy.definition(&db),
        Some(base),
    ));
    let sibling = Type::int_literal(17);
    let root = Type::tuple(TupleType::heterogeneous(&db, &env, [eager, sibling]));
    let expected = trace(
        &db,
        &env,
        root,
        TypeSearchMode::SkipLazyAttributes,
        None,
        Engine::Cursor,
    )
    .1;
    let mut effects = WalkApi::new(&db, &env);
    effects.suspend_newtype = true;
    {
        let mut future = std::pin::pin!(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut effects
        ));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok(false))
        );
    }
    assert_eq!(effects.newtype_calls, 1);
    assert_eq!(effects.pending_drops.get(), 1);
    assert_eq!(effects.visited, expected);
    assert_eq!(effects.visited.last(), Some(&sibling));
    let mut abandoned = WalkApi::new(&db, &env);
    abandoned.suspend_newtype = true;
    let drops = abandoned.pending_drops.clone();
    {
        let mut future = std::pin::pin!(search_type_with(
            root,
            TypeSearchMode::SkipLazyAttributes,
            TypeWalkFacts,
            &mut abandoned
        ));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(drops.get(), 0);
    }
    assert_eq!(drops.get(), 1);
    assert_eq!(abandoned.newtype_calls, 1);
    assert!(!abandoned.visited.contains(&sibling));
    Ok(())
}

#[test]
fn type_walk_collection_refusal_keeps_owned_state() {
    let db = setup_db();
    let env = db.program_environment();
    let types: Vec<_> = (0..16)
        .map(|n| Type::TypeForm(TypeFormType::new(&db, Type::int_literal(n))))
        .collect();
    let mut cursor = TypeWalkCursor {
        pending: types[..8]
            .iter()
            .copied()
            .map(WalkAction::Visit)
            .collect::<SmallVec<_>>(),
    };
    let capacity = cursor.pending.capacity();
    let mut effects = WalkApi::new(&db, &env);
    effects.collections.refuse_growth = Some(AllocationKind::TypeWalkPending);
    assert!(
        ready(super::push_type_walk_visit_with(
            &mut cursor,
            types[8],
            TypeWalkFacts,
            &mut effects
        ))
        .is_err()
    );
    assert_eq!(cursor.pending.len(), 8);
    assert_eq!(cursor.pending.capacity(), capacity);
    for (action, expected) in cursor.pending.iter().zip(&types) {
        assert!(matches!(action, WalkAction::Visit(ty) if ty == expected));
    }
    assert!(effects.visited.is_empty());

    for refuse_access in [None, Some(0), Some(1)] {
        let mut seen = TypeCollector::default();
        for ty in &types[..8] {
            assert!(!unrestricted(
                seen.type_was_already_seen_with(*ty, &mut UnrestrictedCollections)
            ));
        }
        let mut effects = WalkApi::new(&db, &env);
        effects.collections.refuse_growth = Some(AllocationKind::TypeWalkSeen);
        effects.collections.refuse_access = refuse_access;
        assert!(ready(effects.remember_type(&mut seen, types[8])).is_err());
        let SmallSet::Inline(values) = seen.0.get_mut() else {
            panic!("refused spill changed the representation");
        };
        assert_eq!(values.as_slice(), &types[..8]);
        assert!(!values.spilled());
    }
    let mut active = FxHashSet::default();
    unrestricted(enter_depth_active_with(
        &mut active,
        types[0],
        &mut UnrestrictedCollections,
    ));
    let mut index = 1;
    while active.len() < active.capacity() {
        unrestricted(enter_depth_active_with(
            &mut active,
            types[index],
            &mut UnrestrictedCollections,
        ));
        index += 1;
    }
    let snapshot = active.clone();
    let capacity = active.capacity();
    for refuse_access in [None, Some(0), Some(1)] {
        let mut effects = WalkApi::new(&db, &env);
        effects.collections.refuse_growth = Some(AllocationKind::TypeWalkActive);
        effects.collections.refuse_access = refuse_access;
        assert!(ready(effects.enter_active(&mut active, types[index])).is_err());
        assert_eq!(active, snapshot);
        assert_eq!(active.capacity(), capacity);
    }

    let Type::Dynamic(todo) = todo_type!("a longer inline label") else {
        panic!("Todo must be dynamic");
    };
    let nested = SubclassOfType::from(&db, &env, SubclassOfInner::Dynamic(todo));
    for ty in [todo_type!("x"), todo_type!("a longer inline label"), nested] {
        let mut collections = Collections::default();
        assert!(admit_type_walk_access_with(ty, &mut collections).is_ok());
        assert_eq!(
            collections.work,
            [TddWork::TypeWalkAccess {
                units: 1 + ty.inline_payload_bytes()
            }]
        );
        let mut refused = CollectedTypes::default();
        let mut incoming = Collections {
            refuse_access: Some(0),
            ..Collections::default()
        };
        assert!(refused.insert_with(ty, &mut incoming).is_err());
        let SmallSet::Inline(values) = refused else {
            panic!("incoming payload refusal changed representation");
        };
        assert!(values.is_empty());
        assert_eq!(
            incoming.work,
            [TddWork::TypeWalkAccess {
                units: 1 + ty.inline_payload_bytes()
            }]
        );
        let mut seen = CollectedTypes::default();
        assert!(seen.insert(ty));
        for value in &types[..7] {
            assert!(seen.insert(*value));
        }
        let mut collections = Collections {
            refuse_access: Some(1),
            ..Collections::default()
        };
        assert!(seen.insert_with(types[8], &mut collections).is_err());
        let SmallSet::Inline(values) = seen else {
            panic!("retained payload refusal spilled the set");
        };
        assert_eq!(values[0], ty);
        assert_eq!(values.len(), 8);
        assert_eq!(
            collections.work.last(),
            Some(&TddWork::TypeWalkAccess {
                units: 1 + ty.inline_payload_bytes()
            })
        );
    }
}

#[test]
fn shared_policy_preserves_declarations_and_recursive_skips() -> anyhow::Result<()> {
    let db = source_db(
        "class Scope[T, U]: ...\nRecursive = tuple[int, \"Recursive | None\"]\nrecursive: Recursive\n",
    )?;
    let env = db.program_environment();
    let Type::ClassLiteral(ClassLiteral::Static(origin)) = symbol(&db, "Scope")? else {
        anyhow::bail!("Scope did not produce a static class literal");
    };
    let alias = GenericAlias::new(
        &db,
        origin,
        eager_context(&db, &env).specialize(&db, vec![Type::int_literal(6), Type::int_literal(7)]),
    );
    let Type::Recursive(recursive) = symbol(&db, "recursive")? else {
        anyhow::bail!("the recursive annotation must produce a closed Recursive type");
    };
    for (policy, expected) in [
        (
            TypeWalkPolicy::support(),
            vec![Type::int_literal(6), Type::int_literal(7)],
        ),
        (
            TypeWalkPolicy::eligibility(),
            (1..=7).map(Type::int_literal).collect(),
        ),
    ] {
        let mut cursor = TypeWalkCursor {
            pending: [WalkAction::Expand(NonAtomicType::GenericAlias(alias))]
                .into_iter()
                .collect(),
        };
        let mut control = Unrestricted;
        let mut effects = OrdinaryTypeWalk {
            db: &db,
            env: &env,
            control: &mut control,
            query: (),
        };
        let mut actual = Vec::new();
        while let Some(event) =
            infallible(effects.next_event(&mut cursor, policy))
        {
            match event {
                TypeWalkEvent::Visit(ty) => actual.push(ty),
                _ => panic!(
                    "intentionally omitted declaration metadata does not emit a skipped-lazy event"
                ),
            }
        }
        assert_eq!(actual, expected);
    }
    for (policy, skipped) in [
        (TypeWalkPolicy::support(), true),
        (TypeWalkPolicy::eligibility(), false),
        (TypeWalkPolicy::depth(), false),
    ] {
        let mut cursor = TypeWalkCursor {
            pending: [WalkAction::Expand(NonAtomicType::Recursive(recursive))]
                .into_iter()
                .collect(),
        };
        let mut control = Unrestricted;
        let mut effects = OrdinaryTypeWalk {
            db: &db,
            env: &env,
            control: &mut control,
            query: (),
        };
        let event = infallible(effects.next_event(&mut cursor, policy));
        assert_eq!(matches!(event, Some(TypeWalkEvent::SkippedLazy)), skipped);
        assert!(
            infallible(effects.next_event(&mut cursor, policy)).is_none()
        );
    }
    Ok(())
}

struct CursorFixture<'a> {
    items: &'a [u8],
    next: usize,
    dropped: Rc<Cell<bool>>,
}
impl Drop for CursorFixture<'_> {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}
struct CursorFixtureFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SyncCursorFixtureEffects)]
    trait CursorFixtureEffects {
        type Error;
        #[operation(local)]
        #[progress]
        async fn advance(&mut self, cursor: &mut CursorFixture<'_>) -> Result<Option<u8>, Self::Error>;
    }
    #[finite_capability]
    impl CursorFixtureFacts {
        fn add(&self, total: u16, value: u8) -> u16 { total + u16::from(value) }
    }
    #[synchronous(cursor_fixture_sync)]
    #[capabilities(effects = CursorFixtureEffects, facts = CursorFixtureFacts)]
    #[passive_values()]
    async fn cursor_fixture_with<E: CursorFixtureEffects>(cursor: &mut CursorFixture<'_>, facts: CursorFixtureFacts, effects: &mut E) -> Result<u16, E::Error> {
        #[passive_state]
        let mut total = 0u16;
        #[cursor_loop]
        while let Some(value) = effects.advance(cursor).await? {
            total = facts.add(total, value);
        }
        Ok(total)
    }
}

struct CursorFixtureApi {
    remaining: usize,
    trace: Vec<u8>,
    suspensions: usize,
}
impl SyncCursorFixtureEffects for CursorFixtureApi {
    type Error = usize;
    fn advance(&mut self, cursor: &mut CursorFixture<'_>) -> Result<Option<u8>, usize> {
        if cursor.next == cursor.items.len() {
            return Ok(None);
        }
        if self.remaining == 0 {
            return Err(cursor.next);
        }
        self.remaining -= 1;
        let value = cursor.items[cursor.next];
        cursor.next += 1;
        self.trace.push(value);
        Ok(Some(value))
    }
}
impl CursorFixtureEffects for CursorFixtureApi {
    type Error = usize;
    async fn advance(&mut self, cursor: &mut CursorFixture<'_>) -> Result<Option<u8>, usize> {
        if cursor.next == cursor.items.len() {
            return Ok(None);
        }
        let mut pending = true;
        poll_fn(|cx| {
            if pending {
                pending = false;
                self.suspensions += 1;
                cx.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        SyncCursorFixtureEffects::advance(self, cursor)
    }
}

#[test]
fn generic_cursor_progress_preserves_pending_state_trace_and_refusal() {
    for allowance in [0, 1, 2, 3] {
        let dropped = Rc::new(Cell::new(false));
        let mut cursor = CursorFixture {
            items: &[2, 2, 3],
            next: 0,
            dropped: dropped.clone(),
        };
        let mut ordinary = CursorFixtureApi {
            remaining: allowance,
            trace: Vec::new(),
            suspensions: 0,
        };
        let expected = cursor_fixture_sync(&mut cursor, CursorFixtureFacts, &mut ordinary);
        let mut asynchronous = CursorFixtureApi {
            remaining: allowance,
            trace: Vec::new(),
            suspensions: 0,
        };
        cursor.next = 0;
        let result = {
            let mut future = std::pin::pin!(cursor_fixture_with(
                &mut cursor,
                CursorFixtureFacts,
                &mut asynchronous
            ));
            loop {
                match future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                {
                    Poll::Pending => assert!(!dropped.get()),
                    Poll::Ready(value) => break value,
                }
            }
        };
        assert_eq!(result, expected);
        assert_eq!(asynchronous.trace, ordinary.trace);
        assert_eq!(cursor.next, allowance.min(3));
        assert_eq!(asynchronous.suspensions, (allowance + 1).min(3));
        assert_eq!(result, if allowance < 3 { Err(allowance) } else { Ok(7) });
        assert!(!dropped.get());
    }
    let dropped = Rc::new(Cell::new(false));
    let witness = dropped.clone();
    {
        let mut future = std::pin::pin!(async move {
            let mut cursor = CursorFixture {
                items: &[2, 2, 3],
                next: 0,
                dropped: witness,
            };
            let mut effects = CursorFixtureApi {
                remaining: 3,
                trace: Vec::new(),
                suspensions: 0,
            };
            cursor_fixture_with(&mut cursor, CursorFixtureFacts, &mut effects).await
        });
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(!dropped.get());
    }
    assert!(dropped.get());
}

#[test]
fn support_fold_records_repeated_occurrences_before_seen_expansion() {
    let db = setup_db();
    let env = db.program_environment();
    let [first, nested, last] = ["First", "Nested", "Last"].map(|name| {
        BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static(name),
            crate::types::TypeVarVariance::Invariant,
        )
    });
    let shared = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(nested)));
    let root = Type::tuple(TupleType::heterogeneous(
        &db,
        &env,
        [
            Type::TypeVar(first),
            Type::TypeVar(first),
            shared,
            shared,
            Type::TypeVar(last),
        ],
    ));
    let mut effects = WalkApi::new(&db, &env);
    assert_eq!(
        ready(support_type_with(
            root,
            TypeWalkFacts,
            &mut effects
        )),
        Ok(())
    );
    assert_eq!(effects.support_occurrences, [first, first, nested, last]);
    assert_eq!(effects.skipped_lazy, 0);
    assert!(effects.visited.is_empty());
}

#[test]
fn eligibility_stops_before_later_semantic_children() -> anyhow::Result<()> {
    let db = source_db("from typing import NewType\nUser = NewType(\"User\", int)\n")?;
    let env = db.program_environment();
    let Type::KnownInstance(KnownInstanceType::NewType(lazy)) = symbol(&db, "User")? else {
        anyhow::bail!("User is not a NewType declaration");
    };
    let eager = Type::NewTypeInstance(NewType::new(
        &db,
        Name::new_static("Eager"),
        lazy.definition(&db),
        Some(lazy.base(&db)),
    ));
    let root = Type::tuple(TupleType::heterogeneous(&db, &env, [Type::any(), eager]));
    let mut effects = WalkApi::new(&db, &env);
    effects.suspend_newtype = true;
    assert_eq!(
        ready(static_eligible_with(
            root,
            TypeWalkFacts,
            &mut effects
        )),
        Ok(false)
    );
    assert_eq!(effects.newtype_calls, 0);
    assert!(effects.visited.is_empty());
    Ok(())
}
