//! Raw equivalence evidence for runtime element normalization of symbolic tuple packs.

use std::cell::{Cell, RefCell};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use salsa::Database as _;
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::Stamp;

use super::tuple_runtime::{
    TupleRuntimeControl, TupleRuntimeWork, tuple_runtime_element_specialization_with,
};
use super::{GenericContext, Specialization};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::mro::attempt::AttemptMroEffects;
use crate::types::mro::root::{
    MroTailRequest, SynchronousMroRootEffects, mro_first_sync, mro_tail_request_sync,
};
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance, TypeVarNonce};
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassLiteral, IntersectionBuilder, KnownClass,
    KnownInstanceType, MaterializationKind, Type, TypeVarKind, UnionType,
};
use crate::{Db, ProgramEnvironment};

fn variable<'db>(db: &'db TestDb, name: &str, kind: TypeVarKind) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new(name), None, kind),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(db.program_environment().program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

// Retain the full original implementation as the independent raw-result oracle.
fn original<'db>(db: &'db dyn Db, input: Specialization<'db>) -> Specialization<'db> {
    let Some(tuple) = input.tuple_inner(db) else {
        return input;
    };
    if !matches!(tuple.tuple(db), TupleSpec::Variable(tuple) if matches!(tuple.variable(), VariableSegment::TypeVarTuple(_)))
    {
        return input;
    }
    let env = ProgramEnvironment::from_program(input.generic_context(db).program(db));
    Specialization::new(
        db,
        input.generic_context(db),
        [tuple.tuple(db).homogeneous_element_type(db, &env)].as_slice(),
        input.materialization_kind(db),
        None,
    )
}

fn candidate<'db>(db: &'db dyn Db, input: Specialization<'db>) -> Specialization<'db> {
    if input.tuple(db).is_some_and(|tuple| matches!(tuple, TupleSpec::Variable(tuple) if matches!(tuple.variable(), VariableSegment::TypeVarTuple(_)))) {
        Specialization::new(db, input.generic_context(db), [Type::object()].as_slice(), input.materialization_kind(db), None)
    } else {
        input
    }
}

fn compare(db: &TestDb, input: Specialization<'_>, symbolic: bool) {
    let actual = candidate(db, input);
    assert_eq!(
        tuple_runtime_element_specialization_with(db, input, &Control::default()),
        Ok(actual)
    );
    assert_eq!(actual, original(db, input));
    assert_eq!(actual, input.tuple_runtime_element_specialization(db));
    assert_eq!(actual.generic_context(db), input.generic_context(db));
    assert_eq!(
        actual.materialization_kind(db),
        input.materialization_kind(db)
    );
    assert_eq!(candidate(db, actual), actual);
    if symbolic {
        assert_eq!(actual.types(db), [Type::object()]);
        assert_eq!(actual.tuple_inner(db), None);
    } else {
        assert_eq!(actual, input);
    }
}

const SOURCE: &str = r#"
from enum import Enum
from typing import Any, Callable, Generic, Literal, TypeVarTuple
class Choice(Enum):
    A = 1
    B = 2
    C = 3
class Solo(Enum):
    A = 1
class Current[*Ts](tuple[int, *Ts, str]): ...
Ts = TypeVarTuple("Ts")
class Legacy(tuple[int, *Ts, str], Generic[*Ts]): ...
type Alias = int
type Recursive = int | list[Recursive]
plain: int
alias: Alias
recursive: Recursive
nested: list[Alias]
callable: Callable[[int], str]
string: Literal["x"]
bytes_value: Literal[b"x"]
boolean: Literal[True]
enum_a: Literal[Choice.A]
enum_b: Literal[Choice.B]
single: Literal[Solo.A]
choice: Choice
"#;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/tuple_runtime.pyi", SOURCE)
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let file = system_path_to_file(db, "/src/tuple_runtime.pyi")?;
    global_symbol(db, db.program_file(file), name)
        .place
        .ignore_possibly_undefined()
        .ok_or_else(|| anyhow::anyhow!("missing {name}"))
}

#[test]
fn symbolic_packs_absorb_every_prefix_and_suffix_without_changing_raw_fields() -> anyhow::Result<()>
{
    let db = database()?;
    let env = db.program_environment();
    let element = variable(&db, "Element", TypeVarKind::Pep695TypeVar);
    let context = GenericContext::from_typevar_instances(&db, &env, [element]);
    let mut samples = vec![
        Type::Never,
        Type::unknown(),
        Type::any(),
        Type::object(),
        Type::int_literal(1),
        Type::TypeVar(element),
        Type::divergent(context.as_id()),
    ];
    for name in [
        "plain",
        "alias",
        "recursive",
        "nested",
        "callable",
        "string",
        "bytes_value",
        "boolean",
        "enum_a",
        "enum_b",
        "single",
    ] {
        samples.push(symbol(&db, name)?);
    }
    for name in ["alias", "recursive"] {
        assert!(symbol(&db, name)?.is_alias_like(), "{name}");
    }
    for name in ["enum_a", "enum_b", "single"] {
        assert!(symbol(&db, name)?.as_enum_literal().is_some(), "{name}");
    }
    let plain = symbol(&db, "plain")?;
    let intersection = IntersectionBuilder::new(&db, &env)
        .add_positive(plain)
        .add_positive(Type::any())
        .build();
    assert!(matches!(intersection, Type::Intersection(_)));
    samples.extend([
        intersection,
        plain.negate(&db, &env),
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::KnownInstance(KnownInstanceType::Range { is_non_empty: true }),
        Type::KnownInstance(KnownInstanceType::Range {
            is_non_empty: false,
        }),
        KnownClass::Hashable.to_instance(&db, &env),
        Type::divergent(context.as_id()).top_materialization(&db, &env),
        Type::divergent(context.as_id()).bottom_materialization(&db, &env),
    ]);
    let complement = IntersectionBuilder::new(&db, &env)
        .add_positive(symbol(&db, "choice")?)
        .add_negative(symbol(&db, "enum_a")?)
        .build();
    let Type::EnumComplement(compact) = complement else {
        anyhow::bail!("expected an enum complement, got {complement:?}");
    };
    samples.extend([complement, compact.to_intersection(&db, &env)]);
    samples.push(Type::Union(UnionType::new(
        &db,
        [Type::int_literal(1), Type::int_literal(2)].as_slice(),
        RecursivelyDefined::Yes,
    )));
    if let Type::LiteralValue(literal) = Type::int_literal(3) {
        samples.push(Type::LiteralValue(
            literal.with_recursively_defined(RecursivelyDefined::Yes),
        ));
    }
    let width = samples.len();
    for kind in [
        TypeVarKind::Pep695TypeVarTuple,
        TypeVarKind::LegacyTypeVarTuple,
    ] {
        let pack = variable(&db, "Ts", kind);
        for materialization in [
            None,
            Some(MaterializationKind::Top),
            Some(MaterializationKind::Bottom),
        ] {
            for left in 0..width {
                for right in 0..width {
                    let tuple = TupleType::mixed_with_segment(
                        &db,
                        &env,
                        [samples[left]],
                        VariableSegment::TypeVarTuple(pack),
                        [samples[right]],
                    );
                    let input = Specialization::new(
                        &db,
                        context,
                        [Type::any()].as_slice(),
                        materialization,
                        Some(tuple),
                    );
                    compare(&db, input, true);
                }
            }
            for (prefix, suffix) in [
                (&samples[..], &[][..]),
                (&[][..], &samples[..]),
                (&samples[..], &samples[..]),
                (&[][..], &[][..]),
            ] {
                let tuple = TupleType::mixed_with_segment(
                    &db,
                    &env,
                    prefix.iter().copied(),
                    VariableSegment::TypeVarTuple(pack),
                    suffix.iter().copied(),
                );
                compare(
                    &db,
                    Specialization::new(
                        &db,
                        context,
                        [Type::any()].as_slice(),
                        materialization,
                        Some(tuple),
                    ),
                    true,
                );
            }
        }
    }
    Ok(())
}

#[test]
fn ordinary_tuple_shapes_keep_the_original_interned_specialization() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let element = variable(&db, "Element", TypeVarKind::Pep695TypeVar);
    let context = GenericContext::from_typevar_instances(&db, &env, [element]);
    for tuple in [
        None,
        Some(TupleType::empty(&db, &env)),
        Some(TupleType::heterogeneous(
            &db,
            &env,
            [Type::any(), Type::int_literal(1)],
        )),
        Some(TupleType::mixed(
            &db,
            &env,
            [Type::int_literal(2)],
            Type::any(),
            [Type::int_literal(3)],
        )),
    ] {
        for materialization in [
            None,
            Some(MaterializationKind::Top),
            Some(MaterializationKind::Bottom),
        ] {
            compare(
                &db,
                Specialization::new(
                    &db,
                    context,
                    [Type::any()].as_slice(),
                    materialization,
                    tuple,
                ),
                false,
            );
        }
    }
    Ok(())
}

#[test]
fn declared_legacy_and_pep695_tuple_packs_match_the_original() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["Current", "Legacy"] {
        let Type::ClassLiteral(ClassLiteral::Static(class)) = symbol(&db, name)? else {
            anyhow::bail!("not a class")
        };
        let Some(Type::GenericAlias(base)) = class.explicit_bases(&db).first() else {
            anyhow::bail!("not a tuple base")
        };
        compare(&db, base.specialization(&db), true);
        let (outcome, _) = expansion_probe::run_mro(&db, 100_000, || {
            let effects = AttemptMroEffects::new(&db);
            let first = mro_first_sync(
                &db,
                base.origin(&db).into(),
                Some(base.specialization(&db)),
                &effects,
            )?;
            let tail = mro_tail_request_sync(
                &db,
                base.origin(&db).into(),
                Some(base.specialization(&db)),
                &effects,
            )?;
            Ok::<_, Incomplete>((first, tail))
        });
        let (first, tail) = outcome
            .and_then(|value| value)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(Type::from(first), Type::GenericAlias(*base));
        let MroTailRequest::Static(origin, Some(specialization)) = tail else {
            anyhow::bail!("missing normalized tail")
        };
        assert_eq!(origin, base.origin(&db));
        assert_eq!(specialization, original(&db, base.specialization(&db)));
    }
    Ok(())
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            )
        })
        .collect()
}

#[test]
fn discarded_literal_normalization_has_no_dependency_on_its_source_reads() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let element = variable(&db, "Element", TypeVarKind::Pep695TypeVar);
    let pack = variable(&db, "Ts", TypeVarKind::Pep695TypeVarTuple);
    let context = GenericContext::from_typevar_instances(&db, &env, [element]);
    let tuple = TupleType::mixed_with_segment(
        &db,
        &env,
        (0..512).map(Type::int_literal),
        VariableSegment::TypeVarTuple(pack),
        [Type::int_literal(700)],
    );
    let input = Specialization::new(&db, context, [Type::any()].as_slice(), None, Some(tuple));
    executions(&db);
    let control = Control::default();
    let actual = tuple_runtime_element_specialization_with(&db, input, &control)
        .map_err(|work| anyhow::anyhow!("unexpected refusal at {work:?}"))?;
    assert_eq!(
        *control.seen.borrow(),
        [
            TupleRuntimeWork::Inspect,
            TupleRuntimeWork::Intern,
            TupleRuntimeWork::Publish
        ]
    );
    assert!(executions(&db).is_empty());
    let expected = original(&db, input);
    let reads = executions(&db);
    assert!(
        !reads.is_empty(),
        "the old literal normalization must execute real dependencies"
    );
    assert_eq!(actual, expected);
    assert_eq!(actual.types(&db), [Type::object()]);
    Ok(())
}

#[derive(Default)]
struct Control {
    stop_at: Option<usize>,
    seen: RefCell<Vec<TupleRuntimeWork>>,
}

impl TupleRuntimeControl for Control {
    type Error = TupleRuntimeWork;

    fn checkpoint(&self, work: TupleRuntimeWork) -> Result<(), TupleRuntimeWork> {
        let mut seen = self.seen.borrow_mut();
        seen.push(work);
        if self.stop_at == Some(seen.len()) {
            Err(work)
        } else {
            Ok(())
        }
    }
}

#[test]
fn every_normalization_boundary_can_refuse_before_consumption() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let element = variable(&db, "Element", TypeVarKind::Pep695TypeVar);
    let pack = variable(&db, "Ts", TypeVarKind::Pep695TypeVarTuple);
    let context = GenericContext::from_typevar_instances(&db, &env, [element]);
    let tuple = TupleType::mixed_with_segment(
        &db,
        &env,
        [Type::unknown()],
        VariableSegment::TypeVarTuple(pack),
        [Type::int_literal(4)],
    );
    for tuple in [None, Some(tuple)] {
        let input = Specialization::new(
            &db,
            context,
            [Type::any()].as_slice(),
            Some(MaterializationKind::Top),
            tuple,
        );
        let control = Control::default();
        assert_eq!(
            tuple_runtime_element_specialization_with(&db, input, &control),
            Ok(original(&db, input))
        );
        let expected = control.seen.into_inner();
        assert_eq!(
            expected,
            if tuple.is_some() {
                vec![
                    TupleRuntimeWork::Inspect,
                    TupleRuntimeWork::Intern,
                    TupleRuntimeWork::Publish,
                ]
            } else {
                vec![TupleRuntimeWork::Inspect, TupleRuntimeWork::Publish]
            }
        );
        for stop_at in 1..=expected.len() {
            let control = Control {
                stop_at: Some(stop_at),
                ..Control::default()
            };
            let consumed = Cell::new(false);
            let result = tuple_runtime_element_specialization_with(&db, input, &control)
                .inspect(|_| consumed.set(true));
            assert_eq!(result, Err(expected[stop_at - 1]));
            assert!(!consumed.get());
            assert_eq!(*control.seen.borrow(), expected[..stop_at]);
        }
    }
    Ok(())
}

#[test]
fn installed_normalization_spends_parent_allowance_and_retries_without_source_reads()
-> anyhow::Result<()> {
    for symbolic in [false, true] {
        let cost = if symbolic { 5 } else { 2 };
        for allowance in 0..=cost {
            for pre_spent in [0, 7] {
                let db = database()?;
                let env = db.program_environment();
                let element = variable(&db, "Element", TypeVarKind::Pep695TypeVar);
                let pack = variable(&db, "Ts", TypeVarKind::Pep695TypeVarTuple);
                let context = GenericContext::from_typevar_instances(&db, &env, [element]);
                let tuple = if symbolic {
                    Some(TupleType::mixed_with_segment(
                        &db,
                        &env,
                        [Type::unknown()],
                        VariableSegment::TypeVarTuple(pack),
                        [Type::int_literal(4)],
                    ))
                } else {
                    None
                };
                let input = Specialization::new(
                    &db,
                    context,
                    [Type::any()].as_slice(),
                    Some(MaterializationKind::Bottom),
                    tuple,
                );
                executions(&db);
                let stamp = Stamp::current(&db);
                let consumed = Cell::new(false);
                let (result, _) = expansion_probe::run_mro(&db, allowance + pre_spent, || {
                    expansion_probe::charge_work(&db, pre_spent)?;
                    let result = AttemptMroEffects::new(&db).tuple_runtime_specialization(input)?;
                    consumed.set(true);
                    Ok::<_, Incomplete>(result)
                });
                if allowance < cost {
                    assert_eq!(result, Err(Incomplete::Allowance));
                    assert!(!consumed.get());
                } else {
                    assert!(matches!(result, Ok(Ok(_))));
                    assert!(consumed.get());
                }
                let interned_before_refusal = symbolic && allowance >= 4;
                assert_eq!(
                    normalization_allocations(&db)?,
                    usize::from(interned_before_refusal)
                );
                let mut complete = None;
                for retry in 0..2 {
                    let (result, _) = expansion_probe::run_mro(&db, cost, || {
                        AttemptMroEffects::new(&db).tuple_runtime_specialization(input)
                    });
                    let actual = result
                        .and_then(|value| value)
                        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                    assert_eq!(actual.generic_context(&db), context);
                    assert_eq!(
                        actual.materialization_kind(&db),
                        Some(MaterializationKind::Bottom)
                    );
                    assert_eq!(actual.tuple_inner(&db), None);
                    if symbolic {
                        assert_eq!(actual.types(&db), [Type::object()]);
                    } else {
                        assert_eq!(actual, input);
                    }
                    if let Some(previous) = complete {
                        assert_eq!(actual, previous);
                    }
                    complete = Some(actual);
                    assert_eq!(Stamp::current(&db), stamp);
                    assert_eq!(
                        normalization_allocations(&db)?,
                        usize::from(symbolic && !interned_before_refusal && retry == 0),
                    );
                }
                let (result, _) = expansion_probe::run_mro(&db, 100_000, || {
                    expansion_probe::refuse(&db, Incomplete::Interrupted);
                    AttemptMroEffects::new(&db).tuple_runtime_specialization(input)
                });
                assert_eq!(result, Err(Incomplete::Interrupted));
                assert_eq!(normalization_allocations(&db)?, 0);
                // Run the oracle only after the cold attempt and its retries have been observed.
                assert_eq!(complete, Some(original(&db, input)));
            }
        }
    }
    Ok(())
}

fn normalization_allocations(db: &TestDb) -> anyhow::Result<usize> {
    let mut allocations = 0;
    for event in db.clone().take_salsa_events() {
        match event.kind {
            salsa::EventKind::DidInternValue { key, .. } => {
                let name = db.ingredient_debug_name(key.ingredient_index());
                assert!(
                    name.contains("Specialization"),
                    "unexpected interner: {name}"
                );
                allocations += 1;
            }
            salsa::EventKind::WillExecute { database_key } => anyhow::bail!(
                "unexpected query: {}",
                db.ingredient_debug_name(database_key.ingredient_index())
            ),
            _ => {}
        }
    }
    Ok(allocations)
}
