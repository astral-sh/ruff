use std::panic::AssertUnwindSafe;

use salsa::Database as _;

use super::member_lookup::{alias, own_initializer};
use super::*;
use crate::db::tests::TestDb;
use crate::place::{
    DefinedPlace, Definedness, LookupError, Place, PlaceAndQualifiers, Provenance,
    PublicTypePolicy, TypeOrigin,
};
use crate::types::callable::scheduled_probe::mapping::{PromotionFactKey, SemanticOwner};
use crate::types::callable::scheduled_probe::member_lookup::{
    LookupFailure, LookupOperation, PreparedPublicLookupEffects,
};
use crate::types::enums::is_single_member_enum;
use crate::types::mapping::effects::MappingOperation;
use crate::types::{ClassType, GenericAlias, TypeAndQualifiers, TypeQualifiers};

const INPUTS: &str = r#"
from enum import Enum

class Plain: ...

class One(Enum):
    MEMBER = 1

class Many(Enum):
    FIRST = 1
    SECOND = 2

class Box[T]:
    value: T
"#;

fn fixture() -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/public.py", INPUTS)?;
    Ok(db)
}

fn prepare<'db>(db: &'db TestDb, env: &ProgramEnvironment<'db>) -> PreparedDeclarations<'db> {
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/public.py").unwrap(),
        env.program(db),
    );
    PreparedDeclarations::prepare(db, file).unwrap()
}

fn promoted(ty: Type<'_>) -> PlaceAndQualifiers<'_> {
    Place::Defined(DefinedPlace {
        ty,
        origin: TypeOrigin::Inferred,
        definedness: Definedness::AlwaysDefined,
        public_type_policy: PublicTypePolicy::Promote,
        provenance: Provenance::MultipleDefinitions,
    })
    .with_qualifiers(TypeQualifiers::CLASS_VAR)
}

#[derive(Clone, Copy)]
enum Request<'db> {
    Member(PlaceAndQualifiers<'db>),
    Fallback(LookupError<'db>, PlaceAndQualifiers<'db>),
}

impl<'db> Request<'db> {
    async fn evaluate(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        router: &Router<'db, '_>,
    ) -> Result<PlaceAndQualifiers<'db>, LookupFailure<'db>> {
        let effects = PreparedPublicLookupEffects { router };
        let result = match self {
            Self::Member(member) => member.into_lookup_result_with(db, env, &effects).await?,
            Self::Fallback(prior, fallback) => {
                prior
                    .or_fall_back_to_with(db, env, &effects, fallback)
                    .await?
            }
        };
        Ok(result.into())
    }
}

fn run_public<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    request: Request<'db>,
    budget: usize,
    order: (bool, bool),
) -> ConsumerSnapshot<'db, Result<PlaceAndQualifiers<'db>, LookupFailure<'db>>> {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            request.evaluate(db, env, router)
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

fn consumer_work<R>(result: &ConsumerSnapshot<'_, R>) -> Vec<usize> {
    let mut units: Vec<_> = result
        .semantic_work_polls
        .keys()
        .filter(|work| matches!(work.owner, SemanticOwner::Consumer { .. }))
        .map(|work| work.units)
        .collect();
    units.sort_unstable();
    units
}

fn check_nominal_promotion<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    ty: Type<'db>,
    singleton: bool,
    mapping_boundary: Option<MappingOperation>,
) {
    let instance = ty.as_nominal_instance().unwrap();
    let router = Router::with_declarations(db, env, Rc::clone(&prepared)).unwrap();
    let classification = probe::capture(db, || {
        instance.is_singleton_with(db, &PreparedPublicLookupEffects { router: &router })
    })
    .unwrap();
    assert!(
        classification.reads.is_empty(),
        "{ty:?}: {:?}",
        classification.reads
    );
    assert_eq!(classification.value, Ok(singleton), "{ty:?}");

    let input = promoted(ty);
    let full = run_public(
        db,
        env,
        prepared,
        Request::Member(input),
        10_000,
        (false, false),
    );
    let ordinary = PlaceAndQualifiers::from(input.into_lookup_result(db, env));
    assert_eq!(instance.is_singleton(db), singleton, "{ty:?}");
    assert_eq!(ordinary.place.raw_type() != Some(ty), singleton, "{ty:?}");
    if let Some(operation) = mapping_boundary {
        assert_eq!(
            full.consumer,
            Some(Err(LookupFailure::Boundary(Boundary::MappingOperation(
                operation
            )))),
            "{ty:?}"
        );
        assert_eq!(consumer_work(&full), [4]);
    } else if singleton {
        assert_eq!(
            full.consumer,
            Some(Err(LookupFailure::Unsupported(
                LookupOperation::UnionNormalization
            ))),
            "{ty:?}"
        );
        assert_eq!(consumer_work(&full), [3, 4, 4, 8]);
    } else {
        assert_eq!(full.consumer, Some(Ok(ordinary)), "{ty:?}");
        assert_eq!(consumer_work(&full), [3, 4, 8]);
    }
}

#[test]
fn public_promotion_nominal_special_cases_do_not_read_enum_facts() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let mut prepared = prepare(&db, &env);
    prepared.promotion.enum_singletons.clear();
    let prepared = Rc::new(prepared);
    for (ty, singleton, mapping_boundary) in [
        (Type::object(), false, None),
        // Exact tuples classify without source queries, but regular tuple mapping is unavailable.
        (
            Type::empty_tuple(&db, &env),
            false,
            Some(MappingOperation::Tuple),
        ),
        (Type::sys_version_info(), true, None),
        (KnownClass::NoneType.to_instance(&db, &env), true, None),
        (KnownClass::EllipsisType.to_instance(&db, &env), true, None),
        (KnownClass::NoDefaultType.to_instance(&db, &env), true, None),
        (
            KnownClass::NotImplementedType.to_instance(&db, &env),
            true,
            None,
        ),
        (KnownClass::Int.to_instance(&db, &env), false, None),
    ] {
        check_nominal_promotion(
            &db,
            &env,
            Rc::clone(&prepared),
            ty,
            singleton,
            mapping_boundary,
        );
    }
    Ok(())
}

#[test]
fn public_promotion_uses_canonical_enum_alias_and_exhaustiveness_facts() -> anyhow::Result<()> {
    let mut db = fixture()?;
    db.write_file(
        "/src/public.py",
        r#"
from enum import Enum, EnumMeta, Flag, IntFlag
from typing import Any

class AliasedOne(Enum):
    MEMBER = 1
    ALIAS = 1

class AliasedMany(Enum):
    FIRST = 1
    ALIAS = 1
    SECOND = 2

class OneFlag(Flag):
    MEMBER = 1

class OneIntFlag(IntFlag):
    MEMBER = 1

class InertMeta(EnumMeta): ...

class InertMetaOne(Enum, metaclass=InertMeta):
    MEMBER = 1

class NewMeta(EnumMeta):
    def __new__(metacls, name, bases, namespace, **kwargs): ...

class NewMetaOne(Enum, metaclass=NewMeta):
    MEMBER = 1

class PrepareMeta(EnumMeta):
    @classmethod
    def __prepare__(metacls, name: str, bases: tuple[type, ...], **kwargs: Any) -> Any: ...

class PrepareMetaOne(Enum, metaclass=PrepareMeta):
    MEMBER = 1
"#,
    )?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    for (name, singleton) in [
        ("AliasedOne", true),
        ("AliasedMany", false),
        ("OneFlag", false),
        ("OneIntFlag", false),
        ("InertMetaOne", true),
        ("NewMetaOne", false),
        ("PrepareMetaOne", false),
    ] {
        let class = ClassLiteral::Static(class_named(&prepared, name));
        let ty = Type::instance(&db, &env, ClassType::NonGeneric(class));
        check_nominal_promotion(&db, &env, Rc::clone(&prepared), ty, singleton, None);
        assert_eq!(is_single_member_enum(&db, class), singleton, "{name}");
        assert_eq!(
            prepared.promotion_enum_singleton(class),
            Ok(singleton),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn prepared_public_conversion_preserves_forwarding_member_metadata() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/forwarding.py", code(markdown, CASES[2])?)?;
        let env = db.program_environment();
        let file = ProgramFile::new(
            &db,
            system_path_to_file(&db, "/src/forwarding.py")?,
            env.program(&db),
        );
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let check = prepared.globals["check"]
            .value
            .place
            .expect_type()
            .as_function_literal()
            .unwrap();
        let forward = alias(
            prepared.signature(check).unwrap().overloads[0]
                .parameters()
                .get_positional(0)
                .unwrap()
                .annotated_type(),
        );
        let router = Router::with_declarations(&db, &env, Rc::clone(&prepared)).unwrap();
        let observed = probe::capture(&db, || {
            run_with(&db, &env, &router, 100_000, false, false, |router| async {
                let first = own_initializer(&db, &env, router, forward).await?;
                let next = alias(first.inner.place.raw_type().unwrap());
                let second = own_initializer(&db, &env, router, next).await?;
                let public_first = Request::Member(first.inner)
                    .evaluate(&db, &env, router)
                    .await?;
                let public_second = Request::Member(second.inner)
                    .evaluate(&db, &env, router)
                    .await?;
                Ok::<_, LookupFailure<'_>>((first, second, public_first, public_second))
            })
            .unwrap()
        })
        .unwrap();
        assert!(
            observed.reads.is_empty(),
            "source reads: {:?}",
            observed.reads
        );
        let (first, second, public_first, public_second) =
            observed.value.consumer.unwrap().unwrap();
        for (member, expected_policy, public) in [
            (first, PublicTypePolicy::Raw, public_first),
            (second, PublicTypePolicy::Promote, public_second),
        ] {
            let Place::Defined(raw) = member.inner.place else {
                panic!("defined initializer")
            };
            let Place::Defined(converted) = public.place else {
                panic!("defined public initializer")
            };
            assert_eq!(raw.public_type_policy, expected_policy);
            assert_eq!(converted.public_type_policy, PublicTypePolicy::Raw);
            assert_eq!(converted.ty, raw.ty);
            assert_eq!(
                public,
                PlaceAndQualifiers::from(member.inner.into_lookup_result(&db, &env))
            );
            assert_eq!(converted.origin, raw.origin);
            assert_eq!(converted.provenance, raw.provenance);
            assert_eq!(converted.definedness, raw.definedness);
            assert_eq!(public.qualifiers, member.inner.qualifiers);
        }

        let mut without_promotion = PreparedDeclarations::prepare(&db, file).unwrap();
        without_promotion.promotion = PromotionFacts::default();
        let raw = run_public(
            &db,
            &env,
            Rc::new(without_promotion),
            Request::Member(first.inner),
            100,
            (false, false),
        );
        assert_eq!(raw.consumer, Some(Ok(public_first)));
        assert!(raw.graph.mapping_values.is_empty());
        assert!(consumer_work(&raw).is_empty());
    }
    Ok(())
}

#[test]
fn public_promotion_classifies_the_widened_root_and_preserves_possible_definitions()
-> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    let int = prepared.promotion_scalar_fallback(KnownClass::Int).unwrap();
    for definedness in [Definedness::AlwaysDefined, Definedness::PossiblyUndefined] {
        let mut input = promoted(Type::int_literal(1));
        if let Place::Defined(place) = &mut input.place {
            place.definedness = definedness;
        }
        let request = Request::Member(input);
        let full = run_public(
            &db,
            &env,
            Rc::clone(&prepared),
            request,
            10_000,
            (false, false),
        );
        let result = full.consumer.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(result.place.raw_type(), Some(int));
        assert_eq!(
            *result,
            PlaceAndQualifiers::from(input.into_lookup_result(&db, &env))
        );
        // A literal is not nominal. Classification is therefore charged only after widening it.
        assert_eq!(consumer_work(&full), [3, 4, 8]);
        for order in [(false, false), (true, false), (false, true), (true, true)] {
            let retry = run_public(&db, &env, Rc::clone(&prepared), request, full.work(), order);
            assert_eq!(retry.consumer, full.consumer);
            assert_eq!(retry.work(), full.work());
            assert_eq!(consumer_work(&retry), consumer_work(&full));
        }
        for budget in full
            .graph
            .boundaries
            .iter()
            .copied()
            .filter(|work| *work < full.work())
        {
            let short = run_public(
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
    }
    Ok(())
}

#[test]
fn public_singleton_facts_are_exact_and_apply_only_to_the_root() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    for name in ["Plain", "Many", "One"] {
        let class = ClassLiteral::Static(class_named(&prepared, name));
        let input = Type::instance(&db, &env, ClassType::NonGeneric(class));
        let full = run_public(
            &db,
            &env,
            Rc::clone(&prepared),
            Request::Member(promoted(input)),
            10_000,
            (false, false),
        );
        if name == "One" {
            assert_eq!(
                full.consumer,
                Some(Err(LookupFailure::Unsupported(
                    LookupOperation::UnionNormalization
                )))
            );
            assert_eq!(consumer_work(&full), [3, 4, 4, 8]);
            assert!(input.is_singleton(&db, &env));
        } else {
            assert_eq!(
                full.consumer
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .place
                    .raw_type(),
                Some(input)
            );
            assert_eq!(consumer_work(&full), [3, 4, 8]);
        }
    }

    let mut missing = prepare(&db, &env);
    let one = ClassLiteral::Static(class_named(&missing, "One"));
    let one_instance = Type::instance(&db, &env, ClassType::NonGeneric(one));
    let box_class = class_named(&missing, "Box");
    let context = missing.context(box_class).unwrap().unwrap();
    let nested = Type::instance(
        &db,
        &env,
        ClassType::Generic(GenericAlias::new(
            &db,
            box_class,
            context.specialize(&db, [one_instance].as_slice()),
        )),
    );
    missing.promotion.enum_singletons.remove(&one);
    let missing = Rc::new(missing);
    let nested_result = run_public(
        &db,
        &env,
        Rc::clone(&missing),
        Request::Member(promoted(nested)),
        10_000,
        (false, false),
    );
    assert_eq!(
        nested_result
            .consumer
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .place
            .raw_type(),
        Some(nested)
    );
    assert_eq!(consumer_work(&nested_result), [3, 4, 8]);
    let root_result = run_public(
        &db,
        &env,
        missing,
        Request::Member(promoted(one_instance)),
        10_000,
        (false, false),
    );
    assert_eq!(
        root_result.consumer,
        Some(Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::Promotion(PromotionFactKey::EnumSingleton(one)),
        ))))
    );
    assert_eq!(consumer_work(&root_result), [3, 4, 8]);
    assert!(
        root_result
            .graph
            .mapping_values
            .values()
            .any(|result| *result == Ok(one_instance))
    );
    Ok(())
}

#[test]
fn public_fallback_converts_before_combining_and_reserves_union_once() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    let int = prepared.promotion_scalar_fallback(KnownClass::Int).unwrap();
    let prior = LookupError::PossiblyUndefined(TypeAndQualifiers::new(
        int,
        TypeOrigin::Declared,
        TypeQualifiers::empty(),
    ));
    let input = promoted(Type::int_literal(1));
    let full = run_public(
        &db,
        &env,
        Rc::clone(&prepared),
        Request::Fallback(prior, input),
        10_000,
        (false, false),
    );
    assert_eq!(
        full.consumer,
        Some(Err(LookupFailure::Unsupported(
            LookupOperation::UnionNormalization
        )))
    );
    assert_eq!(consumer_work(&full), [3, 4, 4, 8]);

    let raw = Place::declared(int).with_qualifiers(TypeQualifiers::empty());
    let raw_union = run_public(
        &db,
        &env,
        Rc::clone(&prepared),
        Request::Fallback(prior, raw),
        1_000,
        (false, false),
    );
    assert_eq!(
        raw_union.consumer,
        Some(Err(LookupFailure::Unsupported(
            LookupOperation::UnionNormalization
        )))
    );
    assert_eq!(consumer_work(&raw_union), [4]);
    assert!(raw_union.graph.mapping_values.is_empty());

    let mut missing = prepare(&db, &env);
    missing.promotion.scalar_fallbacks.remove(&KnownClass::Int);
    let failed = run_public(
        &db,
        &env,
        Rc::new(missing),
        Request::Fallback(prior, input),
        10_000,
        (false, false),
    );
    assert_eq!(
        failed.consumer,
        Some(Err(LookupFailure::Missing(MissingDeclaration(
            DeclarationKey::Promotion(PromotionFactKey::ScalarFallback(KnownClass::Int)),
        ))))
    );
    assert_eq!(consumer_work(&failed), [4]);
    for order in [(true, false), (false, true), (true, true)] {
        let repeated = run_public(
            &db,
            &env,
            Rc::clone(&prepared),
            Request::Fallback(prior, input),
            full.work(),
            order,
        );
        assert_eq!(repeated.consumer, full.consumer);
        assert_eq!(repeated.work(), full.work());
        assert_eq!(consumer_work(&repeated), [3, 4, 4, 8]);
    }
    for budget in full
        .graph
        .boundaries
        .iter()
        .copied()
        .filter(|work| *work < full.work())
    {
        let short = run_public(
            &db,
            &env,
            Rc::clone(&prepared),
            Request::Fallback(prior, input),
            budget,
            (false, false),
        );
        assert!(short.consumer.is_none());
        assert!(short.graph.exhausted);
    }
    Ok(())
}

#[test]
fn cancellation_after_regular_mapping_does_not_publish_a_public_member() -> anyhow::Result<()> {
    let db = fixture()?;
    let env = db.program_environment();
    let prepared = Rc::new(prepare(&db, &env));
    let request = Request::Member(promoted(Type::int_literal(1)));
    let full = run_public(&db, &env, prepared, request, 10_000, (false, false));
    let cut = full.graph.boundaries[full.graph.boundaries.len() - 2];

    // Cancellation remains set on a database after its unwind, so this evaluation owns its database.
    let cancelled_db = fixture()?;
    let cancelled_env = cancelled_db.program_environment();
    let prepared = Rc::new(prepare(&cancelled_db, &cancelled_env));
    let router = Router::with_declarations(&cancelled_db, &cancelled_env, prepared).unwrap();
    router.cancel_at(cut, cancelled_db.cancellation_token());
    let published = Cell::new(false);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        run_with(
            &cancelled_db,
            &cancelled_env,
            &router,
            10_000,
            false,
            false,
            |router| async {
                let result = Request::Member(promoted(Type::int_literal(1)))
                    .evaluate(&cancelled_db, &cancelled_env, router)
                    .await;
                published.set(true);
                result
            },
        )
    }));
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)));
    assert!(!published.get());
    assert!(!router.consumer_active.get());
    assert!(
        router
            .mappings
            .borrow()
            .values()
            .any(|entry| matches!(entry.answer, Some(Ok(_))))
    );
    Ok(())
}
