use super::member_lookup::alias;
use super::*;
use crate::place::PlaceAndQualifiers;
use crate::types::callable::scheduled_probe::member_lookup::{
    LookupFailure, class_member_from_mro,
};
use crate::types::callable::scheduled_probe::mro::PreparedMroWork;
use crate::types::constructor::effects::ConstructorCallableRequest;
use crate::types::instance::effects::InstanceEffect;
use crate::types::{ClassType, GenericAlias, MemberLookupPolicy};

const BUDGET: usize = 10_000_000;
const ORDERS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

#[derive(Clone, Copy, Debug)]
enum Fixture {
    Minimal,
    Nested,
    Diamond,
    Forwarding,
}

impl Fixture {
    fn source(self, legacy: bool) -> anyhow::Result<String> {
        if matches!(self, Self::Forwarding) {
            let mut source = code(if legacy { LEGACY } else { PEP695 }, CASES[1])?;
            source.push_str("\ndef lookup(cls: type[Forward[str, int]]) -> None: ...\n");
            return Ok(source);
        }
        let mut source = if legacy {
            "from typing import Generic, TypeVar\n\n\
             T = TypeVar('T')\nU = TypeVar('U')\n\n\
             class Result(Generic[T]): ...\n\n\
             class Base(Generic[T]):\n    __init__: type[Result[T]]\n\n"
                .to_owned()
        } else {
            "class Result[T]: ...\n\nclass Base[T]:\n    __init__: type[Result[T]]\n\n".to_owned()
        };
        if legacy && matches!(self, Self::Diamond) {
            source.push_str("V = TypeVar('V')\nW = TypeVar('W')\n\n");
        }
        source.push_str(match (self, legacy) {
            (Self::Minimal, false) => "class Child[U](Base[U]): ...\n",
            (Self::Minimal, true) => "class Child(Base[U]): ...\n",
            (Self::Nested, false) => "class Child[U](Base[Result[U]]): ...\n",
            (Self::Nested, true) => "class Child(Base[Result[U]]): ...\n",
            (Self::Diamond, false) => {
                "class Left[U](Base[U]): ...\nclass Right[V](Base[V]): ...\n\
                 class Child[W](Left[W], Right[W]): ...\n"
            }
            (Self::Diamond, true) => {
                "class Left(Base[U]): ...\nclass Right(Base[V]): ...\n\
                 class Child(Left[W], Right[W]): ...\n"
            }
            (Self::Forwarding, _) => return Ok(source),
        });
        source.push_str("\ndef lookup(cls: type[Child[int]]) -> None: ...\n");
        Ok(source)
    }

    fn origins(self) -> &'static [&'static str] {
        match self {
            Self::Minimal | Self::Nested => &["Child", "Base"],
            Self::Diamond => &["Child", "Left", "Right", "Base"],
            Self::Forwarding => &["Forward", "Base"],
        }
    }
}

fn prepare<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    fixture: Fixture,
) -> PreparedDeclarations<'db> {
    let mut prepared = PreparedDeclarations::prepare(db, file).unwrap();
    let origins = fixture
        .origins()
        .iter()
        .map(|name| class_named(&prepared, name))
        .collect::<Vec<_>>();
    prepared.prepare_construction_origins(origins).unwrap();
    for facts in prepared.classes.values_mut() {
        facts.proper_mro = None;
    }
    prepared
}

fn receiver<'db>(prepared: &PreparedDeclarations<'db>) -> Type<'db> {
    let function = prepared.globals["lookup"]
        .value
        .place
        .expect_type()
        .as_function_literal()
        .unwrap();
    prepared.signature(function).unwrap().overloads[0]
        .parameters()
        .get_positional(0)
        .unwrap()
        .annotated_type()
}

type Answer<'db> = Result<(PlaceAndQualifiers<'db>, Vec<ClassBase<'db>>), LookupFailure<'db>>;

fn run<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    supplied: GenericAlias<'db>,
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Answer<'db>> {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| async {
            let member = class_member_from_mro(
                db,
                env,
                router,
                supplied.origin(db),
                Some(supplied.specialization(db)),
                "__init__",
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            )
            .await?;
            let work = PreparedMroWork::consumer(router);
            let result = router
                .demand_static_mro(
                    db,
                    &work,
                    supplied.origin(db),
                    Some(supplied.specialization(db)),
                )
                .await?;
            assert!(!router.static_mro_is_cycle(&work, result).await?);
            let len = router.static_mro_len(&work, result).await?;
            work.checkpoint(
                len.checked_mul(16)
                    .and_then(|units| units.checked_add(8))
                    .ok_or(Boundary::CostOverflow)?,
            )
            .await?;
            let mut entries = Vec::with_capacity(len);
            for index in 0..len {
                let entry = router.static_mro_entry(&work, result, index).await?;
                entries.push(entry.unwrap());
            }
            assert!(router.static_mro_entry(&work, result, len).await?.is_none());
            Ok((member, entries))
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
    observed.value
}

fn assert_member_type<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    prepared: &PreparedDeclarations<'db>,
    fixture: Fixture,
    member: PlaceAndQualifiers<'db>,
) {
    let result = alias(member.place.expect_type());
    let int = KnownClass::Int.to_instance(db, env);
    let arguments = result.specialization(db).types(db);
    if matches!(fixture, Fixture::Forwarding) {
        assert_eq!(result.origin(db), class_named(prepared, "C"));
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0], KnownClass::Str.to_instance(db, env));
        let mut inner = arguments[1];
        for _ in 0..4 {
            let Type::NominalInstance(instance) = inner else {
                panic!("expected a nested list instance");
            };
            let ClassType::Generic(list) = instance.class(db, env) else {
                panic!("expected a specialized list");
            };
            assert_eq!(list.origin(db).known(db), Some(KnownClass::List));
            let arguments = list.specialization(db).types(db);
            assert_eq!(arguments.len(), 1);
            inner = arguments[0];
        }
        assert_eq!(inner, int);
    } else {
        let origin = class_named(prepared, "Result");
        assert_eq!(result.origin(db), origin);
        assert_eq!(arguments.len(), 1);
        if matches!(fixture, Fixture::Nested) {
            let Type::NominalInstance(instance) = arguments[0] else {
                panic!("expected Result[int] as the inherited argument");
            };
            let ClassType::Generic(nested) = instance.class(db, env) else {
                panic!("expected a specialized Result");
            };
            assert_eq!(nested.origin(db), origin);
            assert_eq!(nested.specialization(db).types(db), &[int]);
        } else {
            assert_eq!(arguments, &[int]);
        }
    }
}

#[test]
fn specialized_inherited_initializers_match_canonical_members_and_mros() -> anyhow::Result<()> {
    for legacy in [false, true] {
        for fixture in [
            Fixture::Minimal,
            Fixture::Nested,
            Fixture::Diamond,
            Fixture::Forwarding,
        ] {
            let source = fixture.source(legacy)?;
            let db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .with_file("/src/mro.py", &source)
                .build()?;
            let env = db.program_environment();
            let file = ProgramFile::new(
                &db,
                system_path_to_file(&db, "/src/mro.py")?,
                env.program(&db),
            );
            let prepared = Rc::new(prepare(&db, file, fixture));
            let supplied = alias(receiver(&prepared));
            let root = supplied.origin(&db);
            assert!(prepared.namespace(root, "__init__").unwrap().is_undefined());
            assert!(prepared.code_generator(root).unwrap().is_none());
            let full = run(&db, &env, Rc::clone(&prepared), supplied, BUDGET, ORDERS[0]);
            let (member, entries) = full.consumer.as_ref().unwrap().as_ref().unwrap();
            assert!(full.static_mro_starts.values().all(|starts| *starts == 1));
            assert!(full.graph.static_mro_pending.is_empty());
            assert_eq!(full.static_mro_counts.graph_passes, 0);
            assert_eq!(full.static_mro_counts.sweeps, 0);
            if matches!(
                fixture,
                Fixture::Minimal | Fixture::Nested | Fixture::Forwarding
            ) {
                assert_eq!(full.static_mro_starts.len(), 3);
                let Type::GenericAlias(original_base) = prepared.explicit_bases(root).unwrap()[0]
                else {
                    panic!("the child declares a specialized Base");
                };
                assert!(full.static_mro_starts.keys().any(|request| {
                    request.class() == original_base.origin(&db)
                        && request.specialization() == Some(original_base.specialization(&db))
                }));
            }

            // Compute both ordinary oracles after capture so they cannot hide source reads.
            let specialization = Some(supplied.specialization(&db));
            assert_eq!(
                *member,
                root.class_member_from_mro(
                    &db,
                    &env,
                    "__init__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    root.iter_mro(&db, specialization),
                ),
                "{fixture:?}, legacy={legacy}",
            );
            assert_eq!(
                entries,
                &root.iter_mro(&db, specialization).collect::<Vec<_>>()
            );
            assert_member_type(&db, &env, &prepared, fixture, *member);
            let nominal_origins = entries
                .iter()
                .filter_map(|entry| match entry {
                    ClassBase::Class(class) if !class.is_object(&db) => {
                        Some(class.class_literal(&db).as_static().unwrap())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                nominal_origins,
                fixture
                    .origins()
                    .iter()
                    .map(|name| class_named(&prepared, name))
                    .collect::<Vec<_>>(),
            );
            assert_eq!(entries[entries.len() - 2], ClassBase::Generic);

            for budget in [0, 1, full.work() - 1] {
                let short = run(&db, &env, Rc::clone(&prepared), supplied, budget, ORDERS[0]);
                assert!(short.graph.exhausted);
                assert!(short.consumer.is_none());
            }
            for order in ORDERS {
                let retry = run(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    supplied,
                    full.work(),
                    order,
                );
                assert_eq!(retry.consumer, full.consumer);
                assert_eq!(retry.work(), full.work());
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Omission {
    RootPacket,
    BasePacket,
    RootConversion,
    AncestorConversion,
    Object,
    RootNamespace,
    BaseNamespace,
    RootMembers,
    BaseContext,
}

#[test]
fn missing_inherited_inputs_propagate_the_exact_declaration_key() -> anyhow::Result<()> {
    for legacy in [false, true] {
        let source = Fixture::Diamond.source(legacy)?;
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file("/src/mro.py", &source)
            .build()?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        for omission in [
            Omission::RootPacket,
            Omission::BasePacket,
            Omission::RootConversion,
            Omission::AncestorConversion,
            Omission::Object,
            Omission::RootNamespace,
            Omission::BaseNamespace,
            Omission::RootMembers,
            Omission::BaseContext,
        ] {
            let mut prepared = prepare(&db, file, Fixture::Diamond);
            let supplied = alias(receiver(&prepared));
            let root = supplied.origin(&db);
            let base = class_named(&prepared, "Base");
            let key = match omission {
                Omission::RootPacket | Omission::BasePacket => {
                    let class = if matches!(omission, Omission::RootPacket) {
                        root
                    } else {
                        base
                    };
                    prepared.mro_construction.remove(&class);
                    DeclarationKey::ExplicitBases(class)
                }
                Omission::RootConversion | Omission::AncestorConversion => {
                    let class = if matches!(omission, Omission::RootConversion) {
                        root
                    } else {
                        class_named(&prepared, "Left")
                    };
                    prepared
                        .mro_construction
                        .get_mut(&class)
                        .unwrap()
                        .converted_explicit_bases[0] = None;
                    DeclarationKey::ConvertedExplicitBase(class, 0)
                }
                Omission::Object => {
                    prepared.object_base = None;
                    DeclarationKey::ObjectBase(env.program(&db))
                }
                Omission::RootNamespace | Omission::BaseNamespace => {
                    let class = if matches!(omission, Omission::RootNamespace) {
                        root
                    } else {
                        base
                    };
                    prepared
                        .classes
                        .get_mut(&class)
                        .unwrap()
                        .namespace
                        .remove("__init__");
                    DeclarationKey::Namespace(class, Name::new("__init__"))
                }
                Omission::RootMembers => {
                    prepared.classes.remove(&root);
                    DeclarationKey::ClassInput(root, ClassInput::InheritedGenericContext)
                }
                Omission::BaseContext => {
                    prepared.classes.remove(&base);
                    prepared.mro_construction.remove(&base);
                    DeclarationKey::Context(base)
                }
            };
            let prepared = Rc::new(prepared);
            let full = run(&db, &env, Rc::clone(&prepared), supplied, BUDGET, ORDERS[0]);
            assert_eq!(
                full.consumer,
                Some(Err(LookupFailure::Missing(MissingDeclaration(key)))),
                "{omission:?}, legacy={legacy}",
            );
            for order in ORDERS {
                let retry = run(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    supplied,
                    full.work(),
                    order,
                );
                assert_eq!(retry.consumer, full.consumer);
            }
        }
    }
    Ok(())
}

#[test]
fn inherited_forwarding_constructor_reports_its_next_real_dependency() -> anyhow::Result<()> {
    for legacy in [false, true] {
        let source = Fixture::Forwarding.source(legacy)?;
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file("/src/mro.py", &source)
            .build()?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/mro.py")?,
            env.program(&db),
        );
        let prepared = Rc::new(prepare(&db, file, Fixture::Forwarding));
        let ty = receiver(&prepared);
        let inherited = run(
            &db,
            &env,
            Rc::clone(&prepared),
            alias(ty),
            BUDGET,
            ORDERS[0],
        );
        assert!(inherited.consumer.as_ref().unwrap().is_ok());
        let result = queued(
            &db,
            &env,
            Some(prepared),
            CallableConversionRequest::new(ty, UpcastPolicy::Unsound),
            BUDGET,
        );
        assert_eq!(
            result.consumer,
            Some(Err(Boundary::InstanceEffect(
                InstanceEffect::TypedDictClassification
            )))
        );
        let class = ClassType::Generic(alias(ty));
        assert!(
            result
                .constructor_polls
                .contains_key(&ConstructorCallableRequest {
                    class,
                    receiver: Type::from(class),
                })
        );
    }
    Ok(())
}
