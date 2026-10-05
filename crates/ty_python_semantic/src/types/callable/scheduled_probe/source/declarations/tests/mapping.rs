use std::panic::AssertUnwindSafe;

use super::*;
use crate::db::tests::TestDb;
use crate::types::callable::scheduled_probe::mapping::{
    MappingAnswer, MappingFailure, MappingRequest, PromotionFactKey, SemanticOwner,
};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::literal::{LiteralValueType, LiteralValueTypeKind};
use crate::types::mapping::effects::MappingOperation;
use crate::types::{
    BoundTypeVarInstance, ClassType, GenericAlias, PromotionKind, PromotionMode, SubclassOfInner,
    TypeContext, TypeMapping, TypeVarVariance,
};
use ruff_db::testing::assert_function_query_was_not_run_by_name;
use ruff_python_ast::name::Name;
use salsa::Database;
use salsa::plumbing::AsId;

fn run_mapping<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    ty: Type<'db>,
    specialization: Specialization<'db>,
    budget: usize,
) -> (
    MappingRequest<'db>,
    ConsumerSnapshot<'db, MappingAnswer<'db>>,
) {
    run_mapping_ordered(
        db,
        env,
        prepared,
        ty,
        specialization,
        budget,
        (false, false),
    )
}

fn run_mapping_ordered<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    ty: Type<'db>,
    specialization: Specialization<'db>,
    budget: usize,
    order: (bool, bool),
) -> (
    MappingRequest<'db>,
    ConsumerSnapshot<'db, MappingAnswer<'db>>,
) {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let root = router.mapping_root(ty, specialization, true).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            router.consumer_mapping_demand(root)
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "mapping entered a semantic query: {:?}",
        observed.reads
    );
    (root, observed.value)
}

fn run_promotion<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    ty: Type<'db>,
    mode: PromotionMode,
    budget: usize,
) -> (
    MappingRequest<'db>,
    ConsumerSnapshot<'db, MappingAnswer<'db>>,
) {
    run_promotion_ordered(db, env, prepared, ty, mode, budget, (false, false))
}

fn run_promotion_ordered<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    prepared: Rc<PreparedDeclarations<'db>>,
    ty: Type<'db>,
    mode: PromotionMode,
    budget: usize,
    order: (bool, bool),
) -> (
    MappingRequest<'db>,
    ConsumerSnapshot<'db, MappingAnswer<'db>>,
) {
    let router = Router::with_declarations(db, env, prepared).unwrap();
    let root = router.promotion_root_with_mode(ty, mode).unwrap();
    let observed = probe::capture(db, || {
        run_with(db, env, &router, budget, order.0, order.1, |router| {
            router.consumer_mapping_demand(root)
        })
        .unwrap()
    })
    .unwrap();
    assert!(
        observed.reads.is_empty(),
        "promotion entered a semantic query: {:?}",
        observed.reads
    );
    (root, observed.value)
}

fn semantic_units<R>(result: &ConsumerSnapshot<'_, R>) -> usize {
    result
        .semantic_work_polls
        .keys()
        .map(|work| work.units)
        .sum()
}

fn specialized_class(ty: Type<'_>) -> ClassType<'_> {
    let Type::SubclassOf(subclass) = ty else {
        panic!("expected a class-object type");
    };
    let SubclassOfInner::Class(class) = subclass.subclass_of() else {
        panic!("expected a nominal class");
    };
    class
}

#[test]
fn prepared_owner_mapping_completes_real_initializer_substitutions() -> anyhow::Result<()> {
    for markdown in [PEP695, LEGACY] {
        let mut previous_first_work = 0;
        let mut second_work = None;
        let mut fact_counts = None;
        for depth in [4, 8, 16, 32] {
            let source = code(markdown, CASES[2])?.replace(
                "list[list[list[list[V]]]]",
                &format!("{}V{}", "list[".repeat(depth), "]".repeat(depth)),
            );
            let mut db = TestDbBuilder::new()
                .with_python_version(PythonVersion::PY313)
                .build()?;
            db.write_file("/src/forwarding.py", source)?;
            let file = system_path_to_file(&db, "/src/forwarding.py")?;
            let env = db.program_environment();
            let file = ProgramFile::new(&db, file, env.program(&db));
            let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
            let counts = (prepared.classes.len(), prepared.signatures.len());
            if let Some(previous) = fact_counts {
                assert_eq!(counts, previous);
            }
            fact_counts = Some(counts);
            let check = prepared.globals["check"]
                .value
                .place
                .expect_type()
                .as_function_literal()
                .unwrap();
            let receiver = prepared.signature(check).unwrap().overloads[0]
                .parameters()
                .get_positional(0)
                .unwrap()
                .annotated_type();
            let forward = specialized_class(receiver).into_generic_alias().unwrap();
            let raw_forward = prepared.namespace(forward.origin(&db), "__init__").unwrap();
            let raw_forward_type = raw_forward.ignore_possibly_undefined().unwrap();

            // The first queued execution precedes the synchronous oracle, including on a cold
            // specialization. The second owner comes from the result built by that execution.
            let (root, full) = run_mapping(
                &db,
                &env,
                Rc::clone(&prepared),
                raw_forward_type,
                forward.specialization(&db),
                100_000,
            );
            let first = full.consumer.unwrap().unwrap();
            assert_eq!(full.graph.mapping_values.get(&root), Some(&Ok(first)));
            assert!(full.graph.mapping_pending.is_empty());
            assert_eq!(full.mapping_polls.len(), depth + 3);
            assert!(!full.semantic_work_polls.is_empty());
            assert!(full.work() > previous_first_work);
            previous_first_work = full.work();
            let c = specialized_class(first).into_generic_alias().unwrap();
            let raw_c = prepared.namespace(c.origin(&db), "__init__").unwrap();
            let raw_c_type = raw_c.ignore_possibly_undefined().unwrap();
            let (_, next) = run_mapping(
                &db,
                &env,
                Rc::clone(&prepared),
                raw_c_type,
                c.specialization(&db),
                100_000,
            );
            let second = next.consumer.unwrap().unwrap();
            assert_eq!(next.mapping_polls.len(), 2);
            if let Some(work) = second_work {
                assert_eq!(next.work(), work);
            }
            second_work = Some(next.work());

            let (_, promoted) = run_promotion(
                &db,
                &env,
                Rc::clone(&prepared),
                second,
                PromotionMode::On,
                100_000,
            );
            assert_eq!(promoted.consumer, Some(Ok(second.promote(&db, &env))));
            assert!(promoted.graph.mapping_pending.is_empty());

            assert_eq!(
                first,
                raw_forward_type.apply_optional_owner_specialization_to_member(
                    &db,
                    Some(forward.specialization(&db))
                )
            );
            assert_eq!(
                second,
                raw_c_type.apply_optional_owner_specialization_to_member(
                    &db,
                    Some(c.specialization(&db))
                )
            );
            assert_eq!(
                raw_forward.map_type(|_| first),
                raw_forward.map_type(|ty| ty.apply_optional_owner_specialization_to_member(
                    &db,
                    Some(forward.specialization(&db))
                ))
            );
            assert_eq!(
                raw_c.map_type(|_| second),
                raw_c.map_type(|ty| ty.apply_optional_owner_specialization_to_member(
                    &db,
                    Some(c.specialization(&db))
                ))
            );

            for budget in [0, 1, full.work() - 1] {
                let (short_root, short) = run_mapping(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    raw_forward_type,
                    forward.specialization(&db),
                    budget,
                );
                assert!(short.consumer.is_none());
                assert!(short.graph.exhausted);
                if let Some(answer) = short.graph.mapping_values.get(&short_root) {
                    assert_eq!(*answer, Ok(first));
                    assert!(!short.graph.mapping_pending.contains(&short_root));
                }
            }
            for _ in 0..2 {
                let (_, retry) = run_mapping(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    raw_forward_type,
                    forward.specialization(&db),
                    full.work(),
                );
                assert_eq!(retry.consumer, Some(Ok(first)));
                assert_eq!(retry.work(), full.work());
            }
            for order in [(true, false), (false, true), (true, true)] {
                let (_, reordered) = run_mapping_ordered(
                    &db,
                    &env,
                    Rc::clone(&prepared),
                    raw_forward_type,
                    forward.specialization(&db),
                    full.work(),
                    order,
                );
                assert_eq!(reordered.consumer, Some(Ok(first)));
                assert_eq!(reordered.work(), full.work());
            }
            eprintln!(
                "depth={depth}: first={} second={} classes={} signatures={}",
                full.work(),
                next.work(),
                prepared.classes.len(),
                prepared.signatures.len()
            );
        }
    }
    Ok(())
}

#[test]
fn prepared_owner_mapping_rejects_unprepared_operations_and_foreign_domains() -> anyhow::Result<()>
{
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/owner.py",
        "from typing import Self\nclass Owner[T]:\n    retained: Self\n",
    )?;
    db.write_file("/src/cold.py", "def callback(value: int) -> str: ...\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/owner.py")?,
        env.program(&db),
    );
    let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
    let owner = class_named(&prepared, "Owner");
    let specialization = prepared.classes[&owner]
        .context
        .value
        .unwrap()
        .specialize(&db, [KnownClass::Int.to_instance(&db, &env)].as_slice());
    let cold_file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/cold.py")?,
        env.program(&db),
    );
    let cold = explicit_global_symbol(&db, cold_file, "callback")
        .place
        .expect_type()
        .as_function_literal()
        .unwrap();
    assert_function_query_was_not_run_by_name(
        &db,
        "FunctionType < 'db >::literal_signature_",
        Some(cold.as_id()),
        &db.clone().take_salsa_events(),
    );
    let retained = prepared
        .namespace(owner, "retained")
        .unwrap()
        .ignore_possibly_undefined()
        .unwrap();
    assert!(
        retained
            .as_typevar()
            .is_some_and(|ty| ty.typevar(&db).is_self(&db))
    );
    for (ty, boundary) in [
        (
            Type::FunctionLiteral(cold),
            MappingOperation::FunctionParamSpecPrelude,
        ),
        (retained, MappingOperation::RetainedSelf),
    ] {
        let (_, result) = run_mapping(&db, &env, Rc::clone(&prepared), ty, specialization, 1_000);
        assert_eq!(
            result.consumer,
            Some(Err(MappingFailure::Boundary(Boundary::MappingOperation(
                boundary
            ))))
        );
    }

    let first_router = Router::with_declarations(&db, &env, Rc::clone(&prepared)).unwrap();
    let foreign = first_router
        .mapping_root(retained, specialization, true)
        .unwrap();
    let second_router = Router::with_declarations(&db, &env, prepared).unwrap();
    let observed = probe::capture(&db, || {
        run_with(&db, &env, &second_router, 1_000, false, false, |router| {
            router.consumer_mapping_demand(foreign)
        })
        .unwrap()
    })
    .unwrap();
    assert!(observed.reads.is_empty());
    assert_eq!(
        observed.value.consumer,
        Some(Err(MappingFailure::Boundary(Boundary::MappingDomain)))
    );
    assert!(observed.value.graph.mapping_values.is_empty());
    assert!(observed.value.graph.mapping_pending.is_empty());
    Ok(())
}

#[test]
fn prepared_owner_mapping_accounts_for_late_changes_in_wide_arguments() -> anyhow::Result<()> {
    let mut previous_work = 0;
    for width in [2, 64] {
        let parameters = (0..width)
            .map(|i| format!("A{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix = "int, ".repeat(width - 1);
        let source = format!(
            "class Wide[{parameters}]: ...\nclass Owner[T]:\n    changed: Wide[{prefix}T]\n    unchanged: Wide[{prefix}int]\n"
        );
        let mut db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()?;
        db.write_file("/src/mapping.py", source)?;
        let file = system_path_to_file(&db, "/src/mapping.py")?;
        let env = db.program_environment();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
        let owner = class_named(&prepared, "Owner");
        let specialization = prepared.classes[&owner]
            .context
            .value
            .unwrap()
            .specialize(&db, [KnownClass::Str.to_instance(&db, &env)].as_slice());
        let mut widths_work = Vec::new();
        for name in ["changed", "unchanged"] {
            let input = prepared
                .namespace(owner, name)
                .unwrap()
                .ignore_possibly_undefined()
                .unwrap();
            let (_, full) = run_mapping(
                &db,
                &env,
                Rc::clone(&prepared),
                input,
                specialization,
                100_000,
            );
            let mapped = full.consumer.unwrap().unwrap();
            assert_eq!(
                mapped,
                input.apply_optional_owner_specialization_to_member(&db, Some(specialization))
            );
            assert_eq!(mapped == input, name == "unchanged");
            widths_work.push(full.work());
            if name == "changed" {
                assert!(full.work() > previous_work);
                if previous_work > 0 {
                    let (root, short) = run_mapping(
                        &db,
                        &env,
                        Rc::clone(&prepared),
                        input,
                        specialization,
                        previous_work,
                    );
                    assert!(short.consumer.is_none());
                    assert!(!short.graph.mapping_values.contains_key(&root));
                }
                previous_work = full.work();
                // Exhaust each round, including reservations before copying and interning.
                // A finished root may still be waiting for its consumer to resume.
                let mut incomplete_roots = 0;
                for budget in full
                    .graph
                    .boundaries
                    .iter()
                    .copied()
                    .filter(|budget| *budget < full.work())
                {
                    let (root, short) = run_mapping(
                        &db,
                        &env,
                        Rc::clone(&prepared),
                        input,
                        specialization,
                        budget,
                    );
                    assert!(short.consumer.is_none());
                    if let Some(answer) = short.graph.mapping_values.get(&root) {
                        assert_eq!(*answer, Ok(mapped));
                        assert!(!short.graph.mapping_pending.contains(&root));
                    } else {
                        incomplete_roots += 1;
                    }
                    let (_, retry) = run_mapping(
                        &db,
                        &env,
                        Rc::clone(&prepared),
                        input,
                        specialization,
                        full.work(),
                    );
                    assert_eq!(retry.consumer, Some(Ok(mapped)));
                }
                assert!(incomplete_roots > 1);
            }
        }
        assert!(widths_work[0] > widths_work[1]);
        eprintln!(
            "width={width}: changed={} unchanged={}",
            widths_work[0], widths_work[1]
        );
    }
    Ok(())
}

#[test]
fn prepared_promotion_widens_scalars_and_retains_literals_without_facts() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("/src/promotion.py", "class Owner: ...\n")?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/promotion.py")?,
        env.program(&db),
    );
    let mut prepared = Rc::new(PreparedDeclarations::prepare(&db, file).unwrap());
    let literals = [
        (Type::string_literal(&db, "value"), KnownClass::Str),
        (Type::bool_literal(true), KnownClass::Bool),
        (Type::int_literal(42), KnownClass::Int),
        (Type::bytes_literal(&db, b"value"), KnownClass::Bytes),
        (
            Type::LiteralValue(LiteralValueType::promotable(
                LiteralValueTypeKind::LiteralString,
            )),
            KnownClass::Str,
        ),
    ];

    for (literal, class) in literals {
        let (root, full) = run_promotion(
            &db,
            &env,
            Rc::clone(&prepared),
            literal,
            PromotionMode::On,
            1_000,
        );
        let expected = prepared.promotion_scalar_fallback(class).unwrap();
        assert_ne!(literal, expected);
        assert_eq!(full.consumer, Some(Ok(expected)));
        assert_eq!(full.graph.mapping_values.get(&root), Some(&Ok(expected)));
        assert_eq!(semantic_units(&full), 12);
        assert_eq!(literal.promote(&db, &env), expected);
    }

    Rc::get_mut(&mut prepared)
        .unwrap()
        .promotion
        .scalar_fallbacks
        .clear();
    for (literal, class) in literals {
        let unpromotable =
            Type::LiteralValue(literal.as_literal_value().unwrap().to_unpromotable());
        for (input, mode) in [
            (literal, PromotionMode::Off),
            (unpromotable, PromotionMode::On),
        ] {
            let (_, retained) = run_promotion(&db, &env, Rc::clone(&prepared), input, mode, 1_000);
            assert_eq!(retained.consumer, Some(Ok(input)));
            assert_eq!(semantic_units(&retained), 8);
        }
        let (root, missing) = run_promotion(
            &db,
            &env,
            Rc::clone(&prepared),
            literal,
            PromotionMode::On,
            1_000,
        );
        let failure = Err(MappingFailure::MissingPromotionFact(
            PromotionFactKey::ScalarFallback(class),
        ));
        assert_eq!(missing.consumer, Some(failure));
        assert_eq!(missing.graph.mapping_values.get(&root), Some(&failure));
        assert_eq!(semantic_units(&missing), 11);
    }
    Ok(())
}

#[test]
fn prepared_promotion_follows_variance_through_nested_arguments() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    let source = r#"
from typing import Generic, TypeVar

Co = TypeVar("Co", covariant=True)
OtherCo = TypeVar("OtherCo", covariant=True)
Contra = TypeVar("Contra", contravariant=True)
Inv = TypeVar("Inv")
Unused = TypeVar("Unused", infer_variance=True)

class Covariant(Generic[Co]): ...
class Contravariant(Generic[Contra]): ...
class Invariant(Generic[Inv]): ...
class Mixed(Generic[Co, Contra, OtherCo]): ...
class UnusedLegacyInferred(Generic[Unused]): ...
class UnusedInferred[T]: ...

class Inferred[T]:
    def get(self) -> T: ...
"#;
    db.write_file("/src/variance.py", source)?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/variance.py")?,
        env.program(&db),
    );
    let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    // Python has no explicit bivariant declaration. Give this otherwise unused generic
    // origin a synthetic parameter so both providers read its actual bivariant variance.
    let bivariant_origin = class_named(&prepared, "UnusedInferred");
    let bivariant_variable = BoundTypeVarInstance::synthetic(
        &db,
        &env,
        Name::new_static("Bivariant"),
        TypeVarVariance::Bivariant,
    );
    let bivariant_context = GenericContext::from_typevar_instances(&db, &env, [bivariant_variable]);
    let bivariant_fact = prepared.classes[&bivariant_origin]
        .context
        .derive(
            &db,
            DeclarationKey::Promotion(PromotionFactKey::Variance(bivariant_variable)),
            || bivariant_variable.variance(&db),
        )
        .unwrap();
    assert_eq!(bivariant_fact.value, TypeVarVariance::Bivariant);
    prepared
        .promotion
        .variances
        .insert(bivariant_variable, bivariant_fact);
    let prepared = Rc::new(prepared);
    assert_eq!(
        prepared.promotion_variance(bivariant_variable).unwrap(),
        TypeVarVariance::Bivariant,
    );
    let instance = |name, argument| {
        let origin = class_named(&prepared, name);
        let context = prepared.context(origin).unwrap().unwrap();
        Type::instance(
            &db,
            &env,
            ClassType::Generic(GenericAlias::new(
                &db,
                origin,
                context.specialize(&db, [argument].as_slice()),
            )),
        )
    };
    let bivariant_instance = |argument| {
        Type::instance(
            &db,
            &env,
            ClassType::Generic(GenericAlias::new(
                &db,
                bivariant_origin,
                bivariant_context.specialize(&db, [argument].as_slice()),
            )),
        )
    };
    // Inference resolves an unused parameter's bivariance to covariance, unlike the
    // explicit bivariant parameter above.
    for (name, variance) in [
        ("Covariant", TypeVarVariance::Covariant),
        ("Contravariant", TypeVarVariance::Contravariant),
        ("Invariant", TypeVarVariance::Invariant),
        ("Inferred", TypeVarVariance::Covariant),
        ("UnusedLegacyInferred", TypeVarVariance::Covariant),
        ("UnusedInferred", TypeVarVariance::Covariant),
    ] {
        let variable = prepared
            .context(class_named(&prepared, name))
            .unwrap()
            .unwrap()
            .variables(&db)
            .next()
            .unwrap();
        assert_eq!(prepared.promotion_variance(variable).unwrap(), variance);
        assert_eq!(variable.variance(&db), variance);
    }
    let literal = Type::int_literal(42);
    let widened = prepared.promotion_scalar_fallback(KnownClass::Int).unwrap();
    let nested = |leaf| instance("Contravariant", instance("Invariant", leaf));
    for (input, expected, mode) in [
        (
            instance("Covariant", literal),
            instance("Covariant", widened),
            PromotionMode::On,
        ),
        (
            instance("Contravariant", literal),
            instance("Contravariant", literal),
            PromotionMode::On,
        ),
        (
            instance("Invariant", literal),
            instance("Invariant", literal),
            PromotionMode::On,
        ),
        (
            instance("Inferred", literal),
            instance("Inferred", widened),
            PromotionMode::On,
        ),
        (
            instance("UnusedLegacyInferred", literal),
            instance("UnusedLegacyInferred", widened),
            PromotionMode::On,
        ),
        (
            instance("UnusedLegacyInferred", literal),
            instance("UnusedLegacyInferred", literal),
            PromotionMode::Off,
        ),
        (
            instance("UnusedInferred", literal),
            instance("UnusedInferred", widened),
            PromotionMode::On,
        ),
        (
            instance("UnusedInferred", literal),
            instance("UnusedInferred", literal),
            PromotionMode::Off,
        ),
        (
            bivariant_instance(literal),
            bivariant_instance(widened),
            PromotionMode::On,
        ),
        (
            bivariant_instance(literal),
            bivariant_instance(literal),
            PromotionMode::Off,
        ),
        (nested(literal), nested(widened), PromotionMode::On),
        (
            instance("Invariant", literal),
            instance("Invariant", widened),
            PromotionMode::Off,
        ),
    ] {
        let (_, full) = run_promotion(&db, &env, Rc::clone(&prepared), input, mode, 10_000);
        assert_eq!(full.consumer, Some(Ok(expected)));
        assert!(full.graph.mapping_pending.is_empty());
        assert_eq!(
            input.apply_type_mapping(
                &db,
                &env,
                &TypeMapping::Promote(mode, PromotionKind::Regular),
                TypeContext::default(),
            ),
            expected,
        );

        if input == nested(literal) {
            for budget in [0, 1, full.work() - 1] {
                let (root, short) =
                    run_promotion(&db, &env, Rc::clone(&prepared), input, mode, budget);
                assert!(short.consumer.is_none());
                assert!(short.graph.exhausted);
                if let Some(answer) = short.graph.mapping_values.get(&root) {
                    assert_eq!(*answer, Ok(expected));
                }
            }
            let (_, retry) =
                run_promotion(&db, &env, Rc::clone(&prepared), input, mode, full.work());
            assert_eq!(retry.consumer, Some(Ok(expected)));
            assert_eq!(retry.work(), full.work());

            let names = [
                "variance reservation requested",
                "variance granted before fact read",
                "variance read before child grant",
                "child granted before demand",
                "child registered",
                "fallback reservation requested",
                "fallback granted before fact read",
                "fallback read before publication",
            ];
            let mut cancellation_cuts = [None; 8];
            for budget in full.graph.boundaries.iter().copied() {
                let (root, short) =
                    run_promotion(&db, &env, Rc::clone(&prepared), input, mode, budget);
                let reservations = |request| {
                    short
                        .semantic_work_polls
                        .keys()
                        .filter(|work| work.owner == SemanticOwner::Mapping(request))
                        .count()
                };
                let root_progress = (
                    short.mapping_polls.get(&root).copied().unwrap_or_default(),
                    reservations(root),
                );
                let scalar_progress = if short.mapping_polls.len() == 3 {
                    short.mapping_polls.iter().find_map(|(request, polls)| {
                        (*request != root && *polls <= 3)
                            .then_some((*polls, reservations(*request)))
                    })
                } else {
                    None
                };
                // A granted reservation and the following mapping poll are separate rounds.
                // The poll counts identify both sides of each synchronous fact read or demand.
                let matches = [
                    root_progress == (5, 4),
                    root_progress == (5, 5),
                    root_progress == (6, 5),
                    root_progress == (6, 6),
                    root_progress == (7, 6) && short.graph.mapping_pending.len() == 2,
                    scalar_progress == Some((2, 1)),
                    scalar_progress == Some((2, 2)),
                    scalar_progress == Some((3, 2)),
                ];
                for (cut, matches) in cancellation_cuts.iter_mut().zip(matches) {
                    if matches && cut.is_none() {
                        assert!(!short.graph.mapping_values.contains_key(&root));
                        *cut = Some(short.work());
                    }
                }
                if cancellation_cuts.iter().all(Option::is_some) {
                    break;
                }
            }
            for (name, cut) in names.into_iter().zip(cancellation_cuts) {
                let cut = cut.unwrap_or_else(|| panic!("missing cancellation cut: {name}"));
                // Local cancellation remains set after catching its unwind. Each replay owns
                // a fresh database, and only router cells are inspected after cancellation.
                let mut cancelled_db = TestDbBuilder::new()
                    .with_python_version(PythonVersion::PY313)
                    .build()?;
                cancelled_db.write_file("/src/variance.py", source)?;
                let cancelled_env = cancelled_db.program_environment();
                let file = ProgramFile::new(
                    &cancelled_db,
                    system_path_to_file(&cancelled_db, "/src/variance.py")?,
                    cancelled_env.program(&cancelled_db),
                );
                let prepared = Rc::new(PreparedDeclarations::prepare(&cancelled_db, file).unwrap());
                let mut input = literal;
                for name in ["Invariant", "Contravariant"] {
                    let origin = class_named(&prepared, name);
                    let context = prepared.context(origin).unwrap().unwrap();
                    input = Type::instance(
                        &cancelled_db,
                        &cancelled_env,
                        ClassType::Generic(GenericAlias::new(
                            &cancelled_db,
                            origin,
                            context.specialize(&cancelled_db, [input].as_slice()),
                        )),
                    );
                }
                let router =
                    Router::with_declarations(&cancelled_db, &cancelled_env, prepared).unwrap();
                let root = router.promotion_root(input).unwrap();
                router.cancel_at(cut, cancelled_db.cancellation_token());
                let completed = Cell::new(false);
                let consumer_lifetime = Rc::new(());
                let retained = Rc::clone(&consumer_lifetime);
                let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                    run_with(
                        &cancelled_db,
                        &cancelled_env,
                        &router,
                        10_000,
                        false,
                        false,
                        |router| async {
                            let answer = router.consumer_mapping_demand(root).await;
                            drop(retained);
                            completed.set(true);
                            answer
                        },
                    )
                }));
                assert!(matches!(cancelled, Err(salsa::Cancelled::Local)), "{name}");
                assert!(!completed.get(), "{name}");
                assert_eq!(Rc::strong_count(&consumer_lifetime), 1, "{name}");
                assert!(!router.consumer_active.get(), "{name}");
                assert!(router.mappings.borrow()[&root].answer.is_none(), "{name}");
            }
        }
    }

    let mixed = |arguments| {
        let origin = class_named(&prepared, "Mixed");
        let context = prepared.context(origin).unwrap().unwrap();
        Type::instance(
            &db,
            &env,
            ClassType::Generic(GenericAlias::new(
                &db,
                origin,
                context.specialize(&db, &arguments),
            )),
        )
    };
    let shared = instance("Invariant", Type::int_literal(7));
    let input = mixed([shared; 3]);
    let expected = mixed([shared, instance("Invariant", widened), shared]);
    let mut baseline = None;
    for order in [(false, false), (true, false), (false, true), (true, true)] {
        let (root, result) = run_promotion_ordered(
            &db,
            &env,
            Rc::clone(&prepared),
            input,
            PromotionMode::On,
            10_000,
            order,
        );
        assert_eq!(result.consumer, Some(Ok(expected)));
        assert_eq!(result.graph.mapping_values.get(&root), Some(&Ok(expected)));
        assert!(result.graph.mapping_pending.is_empty());
        // The shared instance and its literal each need one task per mode. The repeated
        // covariant argument reuses the first argument's completed mapping.
        assert_eq!(result.mapping_polls.len(), 5);
        assert_eq!(result.graph.mapping_values.len(), 5);
        let work = (result.work(), semantic_units(&result));
        if let Some(baseline) = baseline {
            assert_eq!(work, baseline);
        } else {
            baseline = Some(work);
        }
    }
    assert_ne!(input, expected);
    assert_eq!(
        input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default(),
        ),
        expected,
    );
    Ok(())
}

#[test]
fn prepared_promotion_propagates_missing_argument_facts() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/missing.py",
        "from typing import Generic, TypeVar\nT = TypeVar('T', covariant=True)\nclass Box(Generic[T]): ...\n",
    )?;
    let env = db.program_environment();
    let file = ProgramFile::new(
        &db,
        system_path_to_file(&db, "/src/missing.py")?,
        env.program(&db),
    );
    for missing_variance in [true, false] {
        let mut prepared = PreparedDeclarations::prepare(&db, file).unwrap();
        let origin = class_named(&prepared, "Box");
        let context = prepared.context(origin).unwrap().unwrap();
        let variable = context.variables(&db).next().unwrap();
        let input = Type::instance(
            &db,
            &env,
            ClassType::Generic(GenericAlias::new(
                &db,
                origin,
                context.specialize(&db, [Type::int_literal(42)].as_slice()),
            )),
        );
        let key = if missing_variance {
            prepared.promotion.variances.remove(&variable);
            PromotionFactKey::Variance(variable)
        } else {
            prepared.promotion.scalar_fallbacks.remove(&KnownClass::Int);
            PromotionFactKey::ScalarFallback(KnownClass::Int)
        };
        let (root, missing) = run_promotion(
            &db,
            &env,
            Rc::new(prepared),
            input,
            PromotionMode::On,
            1_000,
        );
        let failure = Err(MappingFailure::MissingPromotionFact(key));
        assert_eq!(missing.consumer, Some(failure));
        assert_eq!(missing.graph.mapping_values.get(&root), Some(&failure));
        assert!(missing.graph.mapping_values.values().all(Result::is_err));
        assert_eq!(
            missing.mapping_polls.len(),
            if missing_variance { 1 } else { 2 }
        );
        assert!(missing.graph.mapping_pending.is_empty());
    }
    Ok(())
}
