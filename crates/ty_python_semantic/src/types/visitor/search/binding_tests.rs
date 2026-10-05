use std::cell::Cell;

use ruff_db::files::system_path_to_file;
use salsa::Database as _;
use ty_python_core::ProgramFile;

use super::{SearchControl, SearchWork, Unrestricted};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::constructor::expansion_probe::{self, AttemptSearchControl, Incomplete};
use crate::types::mapping::attempt::UnsupportedMappingOperation;
use crate::types::typevar::BindingContext;
use crate::types::{
    BoundMethodType, BoundTypeVarInstance, ClassType, KnownBoundMethodType, Parameters,
    SelfBinding, Signature, Type, TypeContext, TypeFormType, TypeMapping,
};

#[derive(Default)]
struct RecordingSearch {
    work: Vec<SearchWork>,
    refuse_at: Option<usize>,
}

impl SearchControl for RecordingSearch {
    type Error = SearchWork;

    fn admit(&mut self, work: SearchWork) -> Result<(), SearchWork> {
        let index = self.work.len();
        self.work.push(work);
        if self.refuse_at == Some(index) {
            Err(work)
        } else {
            Ok(())
        }
    }
}

fn self_variable<'db>(db: &'db TestDb, upper_bound: Type<'db>) -> Type<'db> {
    let env = db.program_environment();
    Type::TypeVar(BoundTypeVarInstance::synthetic_self(
        db,
        upper_bound,
        BindingContext::Synthetic(env.program(db)),
    ))
}

fn wrapped<'db>(db: &'db TestDb, ty: Type<'db>) -> Type<'db> {
    Type::TypeForm(TypeFormType::new(db, ty))
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

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_file("/src/binding.py", "class Owner: ...\ndef function(): ...\n")
        .build()
}

fn symbol<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/binding.py")?,
        env.program(db),
    );
    global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined()
        .ok_or_else(|| anyhow::anyhow!("missing source symbol {name}"))
}

fn owner(db: &TestDb) -> anyhow::Result<Type<'_>> {
    let class = symbol(db, "Owner")?
        .as_class_literal()
        .ok_or_else(|| anyhow::anyhow!("Owner did not produce a class literal"))?;
    Ok(Type::instance(
        db,
        &db.program_environment(),
        ClassType::NonGeneric(class),
    ))
}

#[test]
fn negative_searches_do_not_prepare_or_map_the_receiver() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = Type::object();
    let variable = self_variable(&db, receiver);
    let function = symbol(&db, "function")?;
    assert!(matches!(function, Type::FunctionLiteral(_)));
    let callable =
        Type::function_like_callable(&db, Signature::new(Parameters::standard([]), variable));
    let method = Type::BoundMethod(BoundMethodType::from_callable(
        &db,
        callable,
        env.program(&db),
        receiver,
    ));
    for (ty, advances) in [
        (function, Some(1)),
        (method, Some(1)),
        (
            Type::KnownBoundMethod(KnownBoundMethodType::ConstraintSetLowerBound),
            Some(1),
        ),
        (callable, Some(1)),
        (Type::object(), Some(2)),
        (wrapped(&db, Type::int_literal(1)), None),
    ] {
        executions(&db);
        let calls = Cell::new(0);
        let mut control = RecordingSearch::default();
        let result = ty.try_bind_self_typevars(&db, &env, receiver, &mut control, |_, _| {
            calls.set(calls.get() + 1);
            Ok(Type::unknown())
        });
        let reads = executions(&db);
        assert_eq!(result, Ok(ty));
        assert_eq!(calls.get(), 0);
        assert!(reads.is_empty(), "{reads:?}");
        if let Some(advances) = advances {
            assert_eq!(control.work, vec![SearchWork::Advance; advances]);
        }
    }
    Ok(())
}

#[test]
fn positive_search_calls_the_mapper_once_with_unchanged_handles() {
    let db = setup_db();
    let env = db.program_environment();
    let receiver = Type::object();
    let root = wrapped(&db, self_variable(&db, receiver));
    let mapped = Type::int_literal(42);
    let calls = Cell::new(0);
    let mut complete = RecordingSearch::default();
    let once = vec![root, receiver];
    executions(&db);
    let result = root.try_bind_self_typevars(&db, &env, receiver, &mut complete, |ty, received| {
        assert_eq!([ty, received], [root, receiver]);
        drop(once);
        calls.set(calls.get() + 1);
        Ok(mapped)
    });
    let reads = executions(&db);
    assert_eq!(result, Ok(mapped));
    assert_eq!(calls.get(), 1);
    assert_eq!(complete.work.last(), Some(&SearchWork::Predicate));
    assert!(
        !complete
            .work
            .iter()
            .any(|work| matches!(work, SearchWork::Semantic(_)))
    );
    assert!(reads.is_empty(), "{reads:?}");

    for index in 0..complete.work.len() {
        let mut control = RecordingSearch {
            refuse_at: Some(index),
            ..RecordingSearch::default()
        };
        calls.set(0);
        let result = root.try_bind_self_typevars(&db, &env, receiver, &mut control, |_, _| {
            calls.set(calls.get() + 1);
            Ok(mapped)
        });
        assert_eq!(result, Err(complete.work[index]));
        assert_eq!(control.work, complete.work[..=index]);
        assert_eq!(calls.get(), 0, "refused search step {index}");
    }
}

#[test]
fn mapping_refusal_propagates_and_marks_the_installed_attempt() {
    let db = setup_db();
    let env = db.program_environment();
    let receiver = Type::object();
    let root = wrapped(&db, self_variable(&db, receiver));
    let reason = Incomplete::UnsupportedMappingOperation(UnsupportedMappingOperation::ChildMapping);
    let calls = Cell::new(0);
    executions(&db);
    let (outcome, _) = expansion_probe::run_mro(&db, 1_000, || {
        let mut search = AttemptSearchControl::new(&db);
        let result =
            root.try_bind_self_typevars(&db, &env, receiver, &mut search, |ty, received| {
                assert_eq!([ty, received], [root, receiver]);
                calls.set(calls.get() + 1);
                Err(expansion_probe::refuse(&db, reason))
            });
        assert_eq!(result, Err(reason));
        assert!(salsa::attempt_probe::is_incomplete(&db));
        result
    });
    let reads = executions(&db);
    assert_eq!(outcome, Err(reason));
    assert_eq!(calls.get(), 1);
    assert!(reads.is_empty(), "{reads:?}");
}

fn original_mapping<'db>(db: &'db TestDb, root: Type<'db>, receiver: Type<'db>) -> Type<'db> {
    let env = db.program_environment();
    root.apply_type_mapping(
        db,
        &env,
        &TypeMapping::BindSelf(SelfBinding::new(db, &env, receiver, None)),
        TypeContext::default(),
    )
}

#[test]
fn ordinary_callback_matches_the_original_mapping_result() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let receiver = owner(&db)?;
    let root = wrapped(&db, self_variable(&db, receiver));
    let actual =
        root.try_bind_self_typevars(&db, &env, receiver, &mut Unrestricted, |ty, received| {
            Ok(ty.bind_self_typevars_after_search(&db, &env, received))
        });
    let expected = original_mapping(&db, root, receiver);
    assert_eq!(actual, Ok(expected));
    assert_eq!(expected, wrapped(&db, receiver));
    assert_eq!(root.bind_self_typevars(&db, &env, receiver), expected);
    Ok(())
}

fn cold_mapping_reads(callback: bool) -> anyhow::Result<Vec<String>> {
    let db = database()?;
    let env = db.program_environment();
    // A no-base source class leaves its MRO cold when constructing the receiver instance.
    let receiver = owner(&db)?;
    let root = wrapped(&db, self_variable(&db, receiver));
    executions(&db);
    if callback {
        let _ =
            root.try_bind_self_typevars(&db, &env, receiver, &mut Unrestricted, |ty, received| {
                Ok(ty.bind_self_typevars_after_search(&db, &env, received))
            });
    } else {
        let _ = original_mapping(&db, root, receiver);
    }
    Ok(executions(&db))
}

#[test]
fn ordinary_callback_preserves_independent_cold_mapping_read_order() -> anyhow::Result<()> {
    let expected = cold_mapping_reads(false)?;
    let actual = cold_mapping_reads(true)?;
    assert_eq!(actual, expected);
    for owner in [
        "class_mro_literals",
        "try_mro_unspecialized",
        "known_class_to_class_literal",
    ] {
        assert!(actual.iter().any(|name| name.contains(owner)), "{actual:?}");
    }
    Ok(())
}
