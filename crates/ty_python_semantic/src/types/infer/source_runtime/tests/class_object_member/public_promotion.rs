//! Exercises public promotion's controlled children, retained mapping, and publication boundaries.
//! Inputs constructed before a run test provider behavior; the parent module separately covers
//! cold inference of real metaclass members without preparing their inferred types beforehand.

mod function_signatures;

use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;
use std::pin::pin;

use super::*;
use crate::FxOrderSet;
use crate::place::source_effects::PublicLookupEffects;
use crate::types::literal::LiteralValueType;
use crate::types::mapping::effects::MappingOperation;
use crate::types::mapping::source::observations as mapping_observations;
use crate::types::mapping::source::observations::OwnedMappingSnapshot;
use crate::types::mapping::source::public_promotion_observations::{
    self as promotion_observations, ChildAction, Event, Recording, Snapshot, Stage,
};
use crate::types::promotion::{InlinePublicPromotionEffects, inline_public_promotion_result};
use crate::types::set_theoretic::{IntersectionType, RecursivelyDefined, UnionType};
use crate::types::{
    MaterializationOperation, NegativeIntersectionElements, PromotionKind, PromotionMode,
    TypeFormType, TypeMapping,
};

/// Selects full public promotion or regular mapping with an explicit retained mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PromotionRoute {
    Public,
    Regular(PromotionMode),
}

/// Supplies preconstructed input to production promotion and observes its actual polling results.
#[derive(Clone, Copy, Debug)]
struct PromotionRequest<'db> {
    input: Type<'db>,
    route: PromotionRoute,
}

impl<'db> MemberOperation<'db> for PromotionRequest<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        let operation = async {
            match self.route {
                PromotionRoute::Public => {
                    effects
                        .promote_public_type(access.db(), &env, self.input)
                        .await
                }
                PromotionRoute::Regular(mode) => {
                    effects.promote_regular(self.input, &env, mode).await
                }
            }
        };
        let mut operation = pin!(operation);
        poll_fn(|context| {
            let result = operation.as_mut().poll(context);
            if result.is_pending() {
                promotion_observations::pending();
            }
            result
        })
        .await
    }
}

/// Creates source input with canonical-query entry observation enabled and no inferred type memos.
fn fixture(source: &str) -> anyhow::Result<TestDb> {
    Ok(TestDbBuilder::new()
        .with_file("src/main.py", source)
        .with_salsa_event_callback(promotion_observations::query_event)
        .build()?)
}

/// Starts independent observations of the mapping visitor and source attempt.
fn reset() {
    observations::reset(None);
    mapping_observations::reset(None);
}

/// Uses the ordinary public-promotion algorithm only after the controlled attempt has finished.
fn ordinary_public<'db>(
    db: &'db TestDb,
    env: &ProgramEnvironment<'db>,
    input: Type<'db>,
) -> Type<'db> {
    inline_public_promotion_result(input.promote_public_sync(
        db,
        env,
        &InlinePublicPromotionEffects,
    ))
}

/// Extracts a completed type, preserving unavailable children and resource refusals as test failures.
fn completed<'db>(
    result: Result<AnalysisOutcome<Type<'db>>, AnalysisFailure>,
) -> anyhow::Result<Type<'db>> {
    match result {
        Ok(AnalysisOutcome::Complete(ty)) => Ok(ty),
        result => anyhow::bail!("public promotion did not complete: {result:?}"),
    }
}

/// Checks zero active transformation scopes recorded at mapping-entry retirement, then checks local
/// buffer drainage after the attempt.
/// The existing harness separately checks that the registered source routes release their owners.
fn assert_drained(snapshot: &Snapshot) {
    assert_eq!(snapshot.live_mappings, 0, "{snapshot:?}");
    let entered = snapshot
        .events
        .iter()
        .filter_map(|event| match event {
            Event::MappingEntered { visitor, mode } => Some((*visitor, *mode)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let retired = snapshot
        .events
        .iter()
        .filter_map(|event| match event {
            Event::MappingRetired {
                visitor,
                mode,
                active: Some(0),
            } => Some((*visitor, *mode)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(entered, retired, "{snapshot:?}");
    let tuple = mapping_observations::tuple_snapshot();
    assert_eq!(tuple.live, 0);
    assert_eq!(tuple.created, tuple.dropped);
    let sets = mapping_observations::set_snapshot();
    assert_eq!(sets.live, 0);
    assert_eq!(sets.created, sets.dropped);
    super::assert_cleanup();
}

/// Checks that regular promotion's nested mapping requests retain its root visitor, default
/// context, and selected mode. Canonical child queries may start other mappings with their own visitors.
fn assert_retained_mapping(mode: PromotionMode, minimum_children: usize) {
    let snapshot = mapping_observations::mapping_snapshot();
    assert!(snapshot.root_count > 0, "{snapshot:?}");
    assert!(snapshot.root_count <= snapshot.roots.len(), "{snapshot:?}");
    let root = snapshot.roots[0].expect("the mapping root was not observed");
    assert_eq!(root.mapping, OwnedMappingSnapshot::PromoteRegular(mode));
    assert!(root.default_context);
    assert!(
        snapshot.roots[1..snapshot.root_count].iter().all(|other| {
            other.is_some_and(|other| {
                !matches!(other.mapping, OwnedMappingSnapshot::PromoteRegular(_))
            })
        }),
        "{snapshot:?}"
    );
    assert!(
        snapshot.child_count <= snapshot.children.len(),
        "{snapshot:?}"
    );
    assert!(
        snapshot.children[..snapshot.child_count].iter().all(Option::is_some),
        "{snapshot:?}"
    );
    let mut children = snapshot.children[..snapshot.child_count]
        .iter()
        .flatten()
        .filter(|child| {
            child.visitor == root.visitor
                || matches!(child.mapping, OwnedMappingSnapshot::PromoteRegular(_))
        });
    assert!(children.clone().count() >= minimum_children, "{snapshot:?}");
    assert!(
        children.all(|child| {
            child.visitor == root.visitor
                && child.mapping == root.mapping
                && child.default_context
        }),
        "{snapshot:?}"
    );
}

/// Chooses each scalar fallback and one explicitly unpromotable scalar input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scalar {
    Bool,
    Int,
    Str,
    Bytes,
    Unpromotable,
}

impl Scalar {
    const fn known(self) -> KnownClass {
        match self {
            Self::Bool | Self::Unpromotable => KnownClass::Bool,
            Self::Int => KnownClass::Int,
            Self::Str => KnownClass::Str,
            Self::Bytes => KnownClass::Bytes,
        }
    }

    fn input(self, db: &TestDb) -> Type<'_> {
        match self {
            Self::Bool => Type::bool_literal(true),
            Self::Int => Type::int_literal(7),
            Self::Str => Type::string_literal(db, "value"),
            Self::Bytes => Type::bytes_literal(db, b"value"),
            Self::Unpromotable => Type::LiteralValue(LiteralValueType::unpromotable(true)),
        }
    }
}

/// Preconstructed scalar literals request cold canonical fallback instances through the source
/// provider. An explicitly unpromotable literal completes unchanged without requesting that child.
#[test_case::test_case(Scalar::Bool; "bool fallback")]
#[test_case::test_case(Scalar::Int; "int fallback")]
#[test_case::test_case(Scalar::Str; "str fallback")]
#[test_case::test_case(Scalar::Bytes; "bytes fallback")]
#[test_case::test_case(Scalar::Unpromotable; "unpromotable literal")]
fn scalar_provider_preserves_canonical_fallbacks(case: Scalar) -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let env = ProgramEnvironment::from_program(program);
    let input = case.input(&db);
    let argument = KnownClassArgument::new(&db, case.known(), program);
    let ingredient = known_class_to_instance_ingredient(&db);
    let key = ingredient.database_key_index(argument.as_id());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    reset();
    let recording = Recording::start(&db, Some(key), ChildAction::Observe);
    let result = capture(&db, || {
        controlled_member_operation(
            &prepared,
            PromotionRequest {
                input,
                route: PromotionRoute::Public,
            },
            &funded(),
        )
    })
    .map_err(|error| anyhow::anyhow!("controlled read capture failed: {error:?}"))?;
    let snapshot = recording.snapshot();
    drop(recording);
    if case != Scalar::Unpromotable {
        assert_eq!(result.check_root_reads(), Ok(()));
    }
    let actual = completed(result.value)?;
    if case == Scalar::Unpromotable {
        assert_eq!(actual, input);
        assert_eq!(snapshot.count(Stage::ScalarRequest(case.known())), 0);
        assert!(!result.reads.iter().any(|read| read.key == key));
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
    } else {
        assert_eq!(snapshot.count(Stage::ScalarRequest(case.known())), 1);
        assert!(result.reads.iter().any(|read| read.key == key));
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
        assert_eq!(actual, case.known().to_instance(&db, &env));
    }
    assert_eq!(snapshot.count(Stage::SingletonUnion), 0);
    assert_eq!(actual, ordinary_public(&db, &env, input));
    assert_retained_mapping(PromotionMode::On, 0);
    assert_drained(&snapshot);
    Ok(())
}

/// Selects finite nominal representations and unchanged dynamic leaves for preconstructed inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Finite {
    None,
    Object,
    Bool,
    Custom,
    ExplicitAny,
    Float,
    Complex,
    EmptyTuple,
    Any,
    Unknown,
}

impl Finite {
    fn input<'db>(
        self,
        db: &'db TestDb,
        prepared: &PreparedAnalysisFile<'db>,
    ) -> anyhow::Result<Type<'db>> {
        let env = ProgramEnvironment::from_file(prepared.program_file());
        Ok(match self {
            Self::None => KnownClass::NoneType.to_instance(db, &env),
            Self::Object => Type::object(),
            Self::Bool => KnownClass::Bool.to_instance(db, &env),
            Self::Custom | Self::ExplicitAny => {
                let name = Name::new_static("value");
                let request = super::request(prepared, &name, Route::Canonical);
                let Type::ClassLiteral(class) = crate::types::binding_type(db, request.definition)
                else {
                    anyhow::bail!("fixture class did not produce a class literal");
                };
                Type::instance(db, &env, ClassType::NonGeneric(class))
            }
            Self::Float => KnownClass::Float.to_instance(db, &env),
            Self::Complex => KnownClass::Complex.to_instance(db, &env),
            Self::EmptyTuple => Type::empty_tuple(db, &env),
            Self::Any => Type::any(),
            Self::Unknown => Type::unknown(),
        })
    }
}

/// Preconstructed nominal inputs complete finite dispatch; Any and Unknown remain unchanged.
/// None requests canonical singleton widening, numeric inputs preserve ordinary unions, and an
/// ordinary custom class obtains its non-enum answer through the source metadata provider. An instance
/// whose class inherits Any retains that flag and the canonical explicit-Any wrapper during reconstruction.
#[test_case::test_case(Finite::None; "nominal singleton")]
#[test_case::test_case(Finite::Object; "object")]
#[test_case::test_case(Finite::Bool; "known nonsingleton")]
#[test_case::test_case(Finite::Custom; "custom nonenum")]
#[test_case::test_case(Finite::ExplicitAny; "explicit Any inheritance")]
#[test_case::test_case(Finite::Float; "numeric float")]
#[test_case::test_case(Finite::Complex; "numeric complex")]
#[test_case::test_case(Finite::EmptyTuple; "empty exact tuple")]
#[test_case::test_case(Finite::Any; "any")]
#[test_case::test_case(Finite::Unknown; "unknown")]
fn finite_provider_preserves_nominal_dispatch(case: Finite) -> anyhow::Result<()> {
    let source = if case == Finite::ExplicitAny {
        "from typing import Any\nclass Product(Any): pass\n"
    } else {
        "class Product: pass\n"
    };
    let db = fixture(source)?;
    let prepared = prepare(&db);
    let input = case.input(&db, &prepared)?;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let actual = completed(controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Public,
        },
        &funded(),
    ))?;
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(
        snapshot.count(Stage::SingletonUnion),
        usize::from(case == Finite::None)
    );
    assert_eq!(actual, ordinary_public(&db, &env, input));
    if case == Finite::ExplicitAny {
        assert!(
            input
                .as_nominal_instance()
                .is_some_and(|instance| instance.inherits_from_explicit_any())
        );
        assert!(
            actual
                .as_nominal_instance()
                .is_some_and(|instance| instance.inherits_from_explicit_any())
        );
        assert_eq!(actual, input);
    }
    assert_eq!(snapshot.count(Stage::Transferred), 1);
    assert_retained_mapping(PromotionMode::On, 0);
    assert_drained(&snapshot);
    Ok(())
}

/// Distinguishes structural recursion from the final top-level singleton operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Composition {
    Tuple,
    Union,
    Intersection,
    TypeForm,
}

impl Composition {
    fn input<'db>(self, db: &'db TestDb, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let literal = Type::bool_literal(true);
        match self {
            Self::Tuple => Type::heterogeneous_tuple(
                db,
                env,
                [KnownClass::NoneType.to_instance(db, env), literal],
            ),
            Self::Union => Type::Union(UnionType::new(
                db,
                Box::<[Type<'db>]>::from([KnownClass::NoneType.to_instance(db, env), literal]),
                RecursivelyDefined::No,
            )),
            Self::Intersection => Type::Intersection(IntersectionType::new(
                db,
                FxOrderSet::from_iter([literal]),
                NegativeIntersectionElements::Single(Type::int_literal(7)),
            )),
            Self::TypeForm => TypeFormType::from_type_expression(db, literal),
        }
    }
}

/// Preconstructed structures promote nested literals with one retained visitor. Nested None stays
/// narrow because public singleton widening applies only to the mapped top level; regular promotion
/// omits negative intersection contributions instead of requesting their scalar fallback.
#[test_case::test_case(Composition::Tuple; "exact tuple delegation")]
#[test_case::test_case(Composition::Union; "top level union")]
#[test_case::test_case(Composition::Intersection; "negative intersection omitted")]
#[test_case::test_case(Composition::TypeForm; "type form child")]
fn structural_children_retain_regular_promotion(case: Composition) -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = case.input(&db, &env);
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let actual = completed(controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Public,
        },
        &funded(),
    ))?;
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(actual, ordinary_public(&db, &env, input));
    assert_ne!(actual, input);
    assert_eq!(snapshot.count(Stage::SingletonUnion), 0);
    assert_eq!(snapshot.count(Stage::ScalarRequest(KnownClass::Bool)), 1);
    assert_eq!(snapshot.count(Stage::ScalarRequest(KnownClass::Int)), 0);
    assert_retained_mapping(PromotionMode::On, 1);
    assert_drained(&snapshot);
    Ok(())
}

/// Both copied regular-promotion modes survive a retained type-form child. Off keeps its literal;
/// On widens it, and both children use the root's descriptor and visitor rather than recapturing On.
#[test_case::test_case(PromotionMode::On; "promotion on")]
#[test_case::test_case(PromotionMode::Off; "promotion off")]
fn retained_child_preserves_the_selected_mode(mode: PromotionMode) -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = TypeFormType::from_type_expression(&db, Type::bool_literal(true));
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let actual = completed(controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Regular(mode),
        },
        &funded(),
    ))?;
    let snapshot = recording.snapshot();
    drop(recording);
    let ordinary = input.apply_type_mapping(
        &db,
        &env,
        &TypeMapping::Promote(mode, PromotionKind::Regular),
        TypeContext::default(),
    );
    assert_eq!(actual, ordinary);
    assert_eq!(actual == input, mode == PromotionMode::Off);
    assert_retained_mapping(mode, 1);
    assert_drained(&snapshot);
    Ok(())
}

/// A preconstructed function inside a type form maps its cold signature before callable conversion.
/// Completion drains both transformation scopes and preserves the ordinary public result.
#[test]
fn nested_function_maps_before_callable_conversion() -> anyhow::Result<()> {
    let db = fixture("def function(): ...\n")?;
    let prepared = prepare(&db);
    let [Stmt::FunctionDef(function)] = prepared.parsed_module().syntax().body.as_slice() else {
        anyhow::bail!("fixture must contain one function");
    };
    let definition = prepared.semantic_index().expect_single_definition(function);
    let input =
        TypeFormType::from_type_expression(&db, crate::types::binding_type(&db, definition));
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let result = controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Public,
        },
        &funded(),
    );
    let snapshot = recording.snapshot();
    drop(recording);
    let actual = completed(result)?;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(actual, ordinary_public(&db, &env, input));
    assert_eq!(snapshot.count(Stage::Transferred), 1);
    assert_drained(&snapshot);
    Ok(())
}

/// Counts admitted public transfers on a fresh real metaclass-member request for limit calibration.
fn completed_public_transfers(policy: &AnalysisPolicy) -> anyhow::Result<usize> {
    let db = fixture(EXPLICIT_META)?;
    let prepared = prepare(&db);
    let name = Name::new_static("value");
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let _result = controlled_member_operation(
        &prepared,
        super::request(&prepared, &name, Route::Canonical),
        policy,
    );
    let snapshot = recording.snapshot();
    drop(recording);
    assert_drained(&snapshot);
    Ok(snapshot.count(Stage::Transferred))
}

/// Finds the real Product.value query key after an interrupted cold request, without executing it.
fn member_key(db: &TestDb, program: Program<'_>) -> anyhow::Result<salsa::Id> {
    let mut entries = MemberLookupKey::ingredient(db.zalsa()).entries(db.zalsa()).filter(|entry| {
        let (key_program, ty, name, policy) = entry.value().fields();
        *key_program == program && name.as_str() == "value" && *policy == MemberLookupPolicy::default()
            && matches!(ty, Type::ClassLiteral(ClassLiteral::Static(class)) if class.name(db) == "Product")
    });
    let id = entries
        .next()
        .map(|entry| entry.key().key_index())
        .ok_or_else(|| anyhow::anyhow!("the real member query did not intern its key"))?;
    assert!(entries.next().is_none());
    Ok(id)
}

/// Independent work and byte limits refuse public promotion's final transfer inside a cold real
/// member query. Its exact parent key remains unpublished, then completes in the same revision.
#[test_case::test_case(Resource::Work; "semantic work")]
#[test_case::test_case(Resource::Bytes; "requested bytes")]
fn refused_public_transfer_withholds_member_memo_and_retries(
    resource: Resource,
) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = resource.limit();
    let transfers = completed_public_transfers(&resource.policy(high))?;
    assert!(transfers > 0);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if completed_public_transfers(&resource.policy(middle))? == transfers {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture(EXPLICIT_META)?;
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let name = Name::new_static("value");
    let request = super::request(&prepared, &name, Route::Canonical);
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let result = controlled_member_operation(&prepared, request, &resource.policy(low));
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert_eq!(
        snapshot.count(Stage::FinalTransfer),
        transfers,
        "{snapshot:?}"
    );
    assert_eq!(
        snapshot.count(Stage::Transferred),
        transfers - 1,
        "{snapshot:?}"
    );
    assert_drained(&snapshot);
    let id = member_key(&db, program)?;
    let ingredient = class_member_lookup_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let retry = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .map_err(|error| anyhow::anyhow!("controlled retry capture failed: {error:?}"))?;
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(retry.check_root_reads(), Ok(()));
    let Ok(AnalysisOutcome::Complete(resolved)) = retry.value else {
        anyhow::bail!(
            "same-revision public-member retry failed: {:?}",
            retry.value
        );
    };
    let canonical = MemberLookupKey::new(
        &db,
        program,
        resolved.receiver,
        "value",
        MemberLookupPolicy::default(),
    );
    assert_eq!(canonical.as_id(), id);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert!(
        retry
            .reads
            .iter()
            .any(|read| read.key == ingredient.database_key_index(id))
    );
    assert_eq!(
        resolved.member,
        super::ordinary(&db, &prepared, resolved.receiver, &name, Route::Canonical)
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained(&snapshot);
    Ok(())
}

/// A preconstructed type form keeps its mapping entry alive while the promotion future returns
/// Pending before the cold Bool child enters canonical execution. Cancellation drains active
/// transformation scopes before that entry retires; a funded retry in the same revision still
/// promotes the nested literal.
#[test]
fn cancelled_canonical_child_drains_mapping_and_retries() -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let env = ProgramEnvironment::from_program(program);
    let input = TypeFormType::from_type_expression(&db, Type::bool_literal(true));
    let argument = KnownClassArgument::new(&db, KnownClass::Bool, program);
    let ingredient = known_class_to_instance_ingredient(&db);
    let key = ingredient.database_key_index(argument.as_id());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let revision = salsa::plumbing::current_revision(&db);
    reset();
    let recording = Recording::start(&db, Some(key), ChildAction::Cancel);
    let request = PromotionRequest {
        input,
        route: PromotionRoute::Public,
    };
    let interrupted = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    let snapshot = recording.snapshot();
    drop(recording);
    assert!(
        matches!(interrupted, Err(salsa::Cancelled::Local)),
        "{interrupted:?}"
    );
    let child = snapshot
        .events
        .iter()
        .position(|event| matches!(event, Event::ChildEntered { live_mappings: 1 }))
        .ok_or_else(|| {
            anyhow::anyhow!("canonical child never entered with its mapping retained: {snapshot:?}")
        })?;
    assert!(
        snapshot.events[..child]
            .iter()
            .any(|event| matches!(event, Event::Pending { live_mappings: 1 })),
        "{snapshot:?}"
    );
    assert!(
        snapshot.events[child + 1..].iter().any(|event| matches!(
            event,
            Event::MappingRetired {
                active: Some(0),
                ..
            }
        )),
        "{snapshot:?}"
    );
    assert_retained_mapping(PromotionMode::On, 1);
    assert_drained(&snapshot);
    reset();
    let recording = Recording::start(&db, Some(key), ChildAction::Observe);
    let actual = completed(controlled_member_operation(&prepared, request, &funded()))?;
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(actual, ordinary_public(&db, &env, input));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_retained_mapping(PromotionMode::On, 1);
    assert_drained(&snapshot);
    Ok(())
}
