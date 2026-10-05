use std::panic::AssertUnwindSafe;

use salsa::Database as _;
use ty_python_core::platform::PythonPlatform;

use super::member_lookup::alias;
use super::*;
use crate::Program;
use crate::types::MaterializationKind;
use crate::types::MemberLookupPolicy;
use crate::types::callable::scheduled_probe::member_lookup::{
    LookupFailure, LookupOperation, class_member_from_mro,
};
use crate::types::callable::scheduled_probe::mro::{
    PreparedMroWork, PreparedStaticMroEffects, static_mro, work_units,
};
use crate::types::generics::Specialization;
use crate::types::mro::construction::{StaticMroEffects, StaticMroWork};
use crate::types::mro::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::tuple::TupleType;

#[derive(Clone, Copy)]
struct Request<'db> {
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
}

type Answer<'db> = Result<Result<Mro<'db>, StaticMroError<'db>>, LookupFailure<'db>>;

fn run_construction<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    request: Request<'db>,
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Answer<'db>> {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            static_mro(db, env, router, request.class, request.specialization)
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "source reads: {:?}",
        observed.reads
    );
    assert!(!router.consumer_active.get());
    assert!(observed.value.graph.pending.is_empty());
    assert!(observed.value.graph.mapping_pending.is_empty());
    observed.value
}

fn source_class<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &str,
) -> StaticClassLiteral<'db> {
    let Type::ClassLiteral(ClassLiteral::Static(class)) =
        explicit_global_symbol(db, file, name).place.expect_type()
    else {
        panic!("expected a static class: {name}");
    };
    class
}

#[test]
fn prepared_empty_base_construction_matches_complete_canonical_mros() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/mro.py",
        "class Root: ...\nclass Generic[T]:\n    value: T\n",
    )?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro.py")?,
        env.program(&db),
    );
    let root = source_class(&db, file, "Root");
    let generic = source_class(&db, file, "Generic");
    let object = KnownClass::Object.try_to_class_literal(&db, &env).unwrap();
    let prepared = Rc::new(
        PreparedDeclarations::prepare_construction_only(&db, file, [root, generic, object])
            .unwrap(),
    );
    assert!(prepared.classes.is_empty());
    assert!(prepared.globals.is_empty());
    assert!(prepared.signatures.is_empty());
    let context = prepared.context(generic).unwrap().unwrap();
    let supplied = context.specialize(&db, &[KnownClass::Int.to_instance(&db, &env)]);
    let materialized = context
        .specialize(&db, &[Type::any()])
        .with_materialization_kind(&db, Some(MaterializationKind::Top));
    let ClassType::Generic(tuple) =
        TupleType::heterogeneous(&db, &env, [Type::any()]).to_class_type(&db)
    else {
        panic!("the canonical tuple class is generic");
    };
    let tuple_specialization = tuple.specialization(&db);
    assert!(tuple_specialization.tuple(&db).is_some());

    // The constructor accepts the caller's specialization unchanged. Its caller owns tuple
    // runtime normalization, including for a supplied context from another same-program class.
    for (request, entries) in [
        (
            Request {
                class: root,
                specialization: None,
            },
            2,
        ),
        (
            Request {
                class: object,
                specialization: None,
            },
            1,
        ),
        (
            Request {
                class: generic,
                specialization: Some(supplied),
            },
            3,
        ),
        (
            Request {
                class: generic,
                specialization: Some(materialized),
            },
            3,
        ),
        (
            Request {
                class: generic,
                specialization: Some(tuple_specialization),
            },
            3,
        ),
    ] {
        let full = run_construction(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            100_000,
            (false, false),
        );
        let mro = full
            .consumer
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap();
        assert_eq!(mro.len(), entries);
        assert_eq!(
            mro,
            &Mro::of_static_class(&db, request.class, request.specialization).unwrap()
        );
        if let Some(specialization) = request.specialization {
            let ClassBase::Class(ClassType::Generic(alias)) = mro[0] else {
                panic!("a supplied generic root retains its alias");
            };
            assert_eq!(alias.specialization(&db), specialization);
            assert_eq!(
                alias.specialization(&db).tuple(&db),
                specialization.tuple(&db)
            );
            assert_eq!(
                alias.specialization(&db).materialization_kind(&db),
                specialization.materialization_kind(&db)
            );
        }
        for budget in [0, 1, full.work() - 1] {
            let short = run_construction(
                &db,
                &env,
                Rc::clone(&prepared),
                request,
                budget,
                (false, false),
            );
            assert!(short.graph.exhausted);
            assert!(short.consumer.is_none());
        }
        for order in [(false, false), (true, false), (false, true), (true, true)] {
            let retry =
                run_construction(&db, &env, Rc::clone(&prepared), request, full.work(), order);
            assert_eq!(retry.consumer, full.consumer);
            assert_eq!(retry.work(), full.work());
        }
    }
    Ok(())
}

#[test]
fn object_does_not_demand_the_environment_object_fact() -> anyhow::Result<()> {
    let db = TestDbBuilder::new().build()?;
    let env = db.program_environment();
    let object = KnownClass::Object.try_to_class_literal(&db, &env).unwrap();
    let mut prepared =
        PreparedDeclarations::prepare_construction_only(&db, object.program_file(&db), [object])
            .unwrap();
    prepared.object_base = None;
    assert!(prepared.classes.is_empty());
    let result = run_construction(
        &db,
        &env,
        Rc::new(prepared),
        Request {
            class: object,
            specialization: None,
        },
        100_000,
        (false, false),
    );
    assert_eq!(
        result.consumer,
        Some(Ok(Ok(Mro::from([ClassBase::Class(
            ClassType::NonGeneric(object.into())
        )]))))
    );
    Ok(())
}

#[test]
fn inherited_forwarding_construction_and_lookup_preserve_original_base_facts() -> anyhow::Result<()>
{
    for markdown in [PEP695, LEGACY] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        let source = format!(
            "{}\ndef construction_root(factory: type[Forward[list[int], int]]): ...\n",
            code(markdown, CASES[1])?
        );
        db.write_file("/src/mro.py", source)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let forward = class_named(&prepared, "Forward");
        let base = class_named(&prepared, "Base");
        prepared.prepare_construction(forward).unwrap();
        prepared.prepare_construction(base).unwrap();
        let function = prepared.globals["construction_root"]
            .value
            .place
            .expect_type()
            .as_function_literal()
            .unwrap();
        let supplied = alias(
            prepared.signature(function).unwrap().overloads[0]
                .parameters()
                .get_positional(0)
                .unwrap()
                .annotated_type(),
        );
        assert_eq!(supplied.origin(&db), forward);
        let Type::GenericAlias(original_base) = prepared.explicit_bases(forward).unwrap()[0] else {
            panic!("Forward declares a specialized Base");
        };
        assert_eq!(original_base.origin(&db), base);
        assert_eq!(
            prepared.converted_explicit_base(forward, 0).unwrap(),
            Some(ClassBase::Class(ClassType::Generic(original_base)))
        );
        for facts in prepared.classes.values_mut() {
            facts.proper_mro = None;
        }
        let prepared = Rc::new(prepared);
        let request = Request {
            class: forward,
            specialization: Some(supplied.specialization(&db)),
        };
        let full = run_construction(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            10_000_000,
            (false, false),
        );
        let mro = full
            .consumer
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap();
        for order in [(false, false), (true, false), (false, true), (true, true)] {
            let retry =
                run_construction(&db, &env, Rc::clone(&prepared), request, full.work(), order);
            assert_eq!(retry.consumer, full.consumer);
            assert_eq!(retry.work(), full.work());
        }

        let router = Router::with_declarations(&db, &env, Rc::clone(&prepared)).unwrap();
        let observed = probe::capture(&db, || {
            run_with(&db, &env, &router, 10_000_000, false, false, |router| {
                class_member_from_mro(
                    &db,
                    &env,
                    router,
                    forward,
                    request.specialization,
                    "__init__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                )
            })
            .unwrap()
        })
        .unwrap();
        assert!(
            observed.reads.is_empty(),
            "source reads: {:?}",
            observed.reads
        );
        assert_eq!(
            observed.value.consumer,
            Some(Ok(forward.class_member_from_mro(
                &db,
                &env,
                "__init__",
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                forward.iter_mro(&db, request.specialization),
            )))
        );
        assert_eq!(
            mro,
            &Mro::of_static_class(&db, forward, request.specialization).unwrap()
        );
    }
    Ok(())
}

#[test]
fn prepared_construction_preserves_missing_inputs_and_invalid_conversions() -> anyhow::Result<()> {
    #[derive(Clone, Copy)]
    enum Expected {
        MissingBaseContext,
        CanonicalMro,
        Unsupported(LookupOperation),
    }

    for (source, expected) in [
        (
            "class Base: ...\nclass Root(Base): ...\n",
            Expected::MissingBaseContext,
        ),
        (
            "from typing import Any\nclass Root(Any): ...\n",
            Expected::CanonicalMro,
        ),
        (
            "class Root(1): ...\n",
            Expected::Unsupported(LookupOperation::StaticMroErrorConstruction),
        ),
        (
            "class Root[T]: ...\n",
            Expected::Unsupported(LookupOperation::DefaultSpecialization),
        ),
    ] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/mro.py", source)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let root = source_class(&db, file, "Root");
        let mut prepared =
            PreparedDeclarations::prepare_construction_only(&db, file, [root]).unwrap();
        if matches!(
            expected,
            Expected::Unsupported(LookupOperation::StaticMroErrorConstruction)
        ) {
            assert_eq!(prepared.converted_explicit_base(root, 0), Ok(None));
            assert!(
                matches!(PreparedDeclarations::prepare(&db, file), Err(PreparationError::InvalidMro(class)) if class == root)
            );
        }
        let request = Request {
            class: root,
            specialization: None,
        };
        let full = run_construction(
            &db,
            &env,
            Rc::new(prepared),
            request,
            100_000,
            (false, false),
        );
        let expected = match expected {
            Expected::Unsupported(operation) => Err(LookupFailure::Unsupported(operation)),
            Expected::CanonicalMro => Ok(Mro::of_static_class(&db, root, None)),
            Expected::MissingBaseContext => Err(LookupFailure::Missing(MissingDeclaration(
                DeclarationKey::Context(source_class(&db, file, "Base")),
            ))),
        };
        assert_eq!(full.consumer, Some(expected));
        prepared = PreparedDeclarations::prepare_construction_only(&db, file, [root]).unwrap();
        if !prepared.explicit_bases(root).unwrap().is_empty() {
            prepared
                .mro_construction
                .get_mut(&root)
                .unwrap()
                .converted_explicit_bases[0] = None;
            let missing = run_construction(
                &db,
                &env,
                Rc::new(prepared),
                request,
                100_000,
                (false, false),
            );
            assert_eq!(
                missing.consumer,
                Some(Err(LookupFailure::Missing(MissingDeclaration(
                    DeclarationKey::ConvertedExplicitBase(root, 0)
                ))))
            );
        }
    }

    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/mro.py", "class Root: ...\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro.py")?,
        env.program(&db),
    );
    let root = source_class(&db, file, "Root");
    let request = Request {
        class: root,
        specialization: None,
    };
    let mut prepared = PreparedDeclarations::prepare_construction_only(&db, file, [root]).unwrap();
    prepared.object_base = None;
    let missing = run_construction(
        &db,
        &env,
        Rc::new(prepared),
        request,
        100_000,
        (false, false),
    );
    assert_eq!(
        missing.consumer,
        Some(Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::ObjectBase(env.program(&db))
        ))))
    );
    let prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let missing = run_construction(
        &db,
        &env,
        Rc::new(prepared),
        request,
        100_000,
        (false, false),
    );
    assert_eq!(
        missing.consumer,
        Some(Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::ExplicitBases(root)
        ))))
    );
    let prepared = PreparedDeclarations::prepare_construction_only(&db, file, []).unwrap();
    let missing = run_construction(
        &db,
        &env,
        Rc::new(prepared),
        request,
        100_000,
        (false, false),
    );
    assert_eq!(
        missing.consumer,
        Some(Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::Context(root)
        ))))
    );
    Ok(())
}

#[test]
fn prepared_dependencies_preserve_missing_tails_and_opaque_errors() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new().build()?;
    db.write_file("/src/mro.py", "class Root: ...\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/mro.py")?,
        env.program(&db),
    );
    let root = source_class(&db, file, "Root");
    let class = ClassType::NonGeneric(root.into());
    let base = ClassBase::Class(class);
    let prepared =
        Rc::new(PreparedDeclarations::prepare_construction_only(&db, file, [root]).unwrap());
    let router = Router::with_declarations(&db, &env, prepared).unwrap();
    let observed = probe::capture(&db, || {
        run_with(&db, &env, &router, 100_000, false, false, |router| async {
            let work = PreparedMroWork::consumer(router);
            let effects = PreparedStaticMroEffects::new(&db, &env, &work);
            effects
                .checkpoint(StaticMroWork::StaticCycleRequest)
                .await
                .unwrap();
            assert_eq!(effects.static_mro_is_cycle(root, None).await, Ok(false));
            effects
                .checkpoint(StaticMroWork::SingleBaseCollectionRequest)
                .await
                .unwrap();
            assert_eq!(
                effects
                    .collect_single_base_mro(&env, class, base, None)
                    .await,
                Err(LookupFailure::Missing(MissingDeclaration(
                    DeclarationKey::ProperMro(root)
                )))
            );
            effects
                .checkpoint(StaticMroWork::BaseCollectionRequest)
                .await
                .unwrap();
            assert_eq!(
                effects.collect_base_mro(&env, base, None).await,
                Err(LookupFailure::Missing(MissingDeclaration(
                    DeclarationKey::ProperMro(root)
                )))
            );
            effects
                .checkpoint(StaticMroWork::DirectBaseSpecializationRequest)
                .await
                .unwrap();
            assert_eq!(effects.specialize_base(base, None).await, Ok(base));
            effects
                .checkpoint(StaticMroWork::ErrorRequest)
                .await
                .unwrap();
            assert_eq!(
                effects
                    .make_error(&env, class, StaticMroErrorKind::InheritanceCycle)
                    .await,
                Err(LookupFailure::Unsupported(
                    LookupOperation::StaticMroErrorConstruction
                ))
            );
            effects
                .checkpoint(StaticMroWork::ErrorDetailsRequest)
                .await
                .unwrap();
            assert_eq!(
                effects.failed_c3(&env, root, class, &[], &[]).await,
                Err(LookupFailure::Unsupported(
                    LookupOperation::StaticMroErrorDetails
                ))
            );
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "source reads: {:?}",
        observed.reads
    );
    assert_eq!(observed.value.consumer, Some(()));
    Ok(())
}

#[test]
fn construction_work_rejects_overflow_before_admitting_storage() {
    for work in [
        StaticMroWork::GenericProtocolScan { len: usize::MAX },
        StaticMroWork::GenericAliasScan { len: usize::MAX },
        StaticMroWork::SequenceStart { bases: usize::MAX },
        StaticMroWork::SequenceStart { bases: usize::MAX - 1 },
        StaticMroWork::ResolvedBaseAppend {
            prefix_len: usize::MAX,
            capacity: usize::MAX,
        },
        StaticMroWork::InvalidBaseAppend {
            prefix_len: usize::MAX,
            capacity: usize::MAX,
        },
        StaticMroWork::InvalidBasesBox {
            len: usize::MAX,
            capacity: usize::MAX,
        },
        StaticMroWork::SequenceAppend {
            prefix_len: usize::MAX,
            capacity: usize::MAX,
        },
        StaticMroWork::DirectSequenceCapacity { len: usize::MAX },
        StaticMroWork::FixedMro {
            entries: usize::MAX,
        },
    ] {
        assert_eq!(work_units(work), Err(Boundary::CostOverflow));
    }
}

#[test]
fn construction_admission_rejects_foreign_roots_and_specializations() -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/mro.py", "class Root[T]: ...\n")
        .build()?;
    let env = db.program_environment();
    let python_file = system_path_to_file(&db, "/src/mro.py")?;
    let file = ProgramFile::new(&db, python_file, env.program(&db));
    let root = source_class(&db, file, "Root");
    let other_program = Program::new(&db, &PythonPlatform::All, env.resolver_environment(&db));
    let other_file = ProgramFile::new(&db, python_file, other_program);
    let foreign = source_class(&db, other_file, "Root");
    let foreign_specialization = foreign
        .generic_context(&db)
        .unwrap()
        .identity_specialization(&db);
    let prepared =
        Rc::new(PreparedDeclarations::prepare_construction_only(&db, file, [root]).unwrap());
    for request in [
        Request {
            class: foreign,
            specialization: None,
        },
        Request {
            class: root,
            specialization: Some(foreign_specialization),
        },
    ] {
        let result = run_construction(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            100_000,
            (false, false),
        );
        assert_eq!(
            result.consumer,
            Some(Err(LookupFailure::Boundary(Boundary::ProgramDomain)))
        );
    }
    Ok(())
}

#[test]
fn cancelled_construction_does_not_publish_a_partial_mro() -> anyhow::Result<()> {
    for source in [
        "class Root: ...\n",
        "class Base: ...\nclass Root(Base, Base): ...\n",
    ] {
        let mut db = TestDbBuilder::new().build()?;
        db.write_file("/src/mro.py", source)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let root = source_class(&db, file, "Root");
        let prepared =
            Rc::new(PreparedDeclarations::prepare_construction_only(&db, file, [root]).unwrap());
        let request = Request {
            class: root,
            specialization: None,
        };
        let full = run_construction(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            100_000,
            (false, false),
        );
        let cut = full.graph.boundaries[full.graph.boundaries.len() - 2];
        let router = Router::with_declarations(&db, &env, prepared).unwrap();
        router.cancel_at(cut, db.cancellation_token());
        let published = Cell::new(false);
        let observed = probe::capture(&db, || {
            salsa::Cancelled::catch(AssertUnwindSafe(|| {
                run_with(&db, &env, &router, 100_000, false, false, |router| async {
                    let result = static_mro(&db, &env, router, root, None).await;
                    published.set(true);
                    result
                })
            }))
        })
        .unwrap();
        assert!(
            observed.reads.is_empty(),
            "source reads: {:?}",
            observed.reads
        );
        assert!(matches!(observed.value, Err(salsa::Cancelled::Local)));
        assert!(!published.get());
        assert!(!router.consumer_active.get());
    }
    Ok(())
}
