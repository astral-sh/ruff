//! Controls function-signature promotion's canonical queries, metadata, polarity and cleanup.
//! Existing callable mdtests specify Python behavior; these checks inspect runtime state directly.

use super::*;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::function::{
    FunctionType, function_last_definition_signature_ingredient,
    function_literal_signature_ingredient,
};
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};

/// Gets a declaration handle for controlled mapping without requesting its public signature.
fn declared<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> anyhow::Result<FunctionType<'db>> {
    let Some(Stmt::FunctionDef(node)) = prepared.parsed_module().syntax().body.last() else {
        anyhow::bail!("fixture must end with a function");
    };
    crate::types::binding_type(db, prepared.semantic_index().expect_single_definition(node))
        .as_function_literal()
        .ok_or_else(|| anyhow::anyhow!("fixture did not yield a function"))
}

/// Promotes a stored input while recording the real retained mapping's complete lifetime.
fn promote<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    input: Type<'db>,
    mode: PromotionMode,
) -> anyhow::Result<Type<'db>> {
    reset();
    let recording = Recording::start(db, None, ChildAction::Observe);
    let result = controlled_member_operation(
        prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Regular(mode),
        },
        &funded(),
    );
    let snapshot = recording.snapshot();
    drop(recording);
    assert_drained(&snapshot);
    completed(result)
}

/// An initially absent public-signature memo is executed and published under the original
/// function identity before promotion constructs its updated function and callable result.
#[test]
fn cold_function_signature_uses_its_canonical_query() -> anyhow::Result<()> {
    let db = fixture("def target(value): ...\n")?;
    let prepared = prepare(&db);
    let function = declared(&db, &prepared)?;
    let ingredient = function_literal_signature_ingredient(&db);
    let id = function.as_id();
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let result = capture(&db, || {
        controlled_member_operation(
            &prepared,
            PromotionRequest {
                input: Type::FunctionLiteral(function),
                route: PromotionRoute::Regular(PromotionMode::On),
            },
            &funded(),
        )
    })
    .map_err(|error| anyhow::anyhow!("function signature capture: {error:?}"))?;
    let snapshot = recording.snapshot();
    drop(recording);
    let actual = completed(result.value)?;
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert!(
        result
            .reads
            .iter()
            .any(|read| read.key == ingredient.database_key_index(id))
    );
    let Type::Callable(callable) = actual else {
        anyhow::bail!("function did not become callable");
    };
    let [signature] = callable.signatures(&db).overloads.as_slice() else {
        anyhow::bail!("expected one overload");
    };
    assert_eq!(signature.parameters().len(), 1);
    assert_eq!(signature.parameters()[0].annotated_type(), Type::unknown());
    assert_eq!(signature.return_ty, Type::unknown());
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        actual,
        Type::FunctionLiteral(function).apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default()
        )
    );
    assert_drained(&snapshot);
    Ok(())
}

/// Rebuilt overloads preserve defaults, source overload indices, descriptor kind and deprecation.
/// Parameter literals stay narrow under On while returns widen; Off reverses those positions.
#[test_case::test_case(PromotionMode::On; "promotion on")]
#[test_case::test_case(PromotionMode::Off; "promotion off")]
fn callable_metadata_and_polarity_are_preserved(mode: PromotionMode) -> anyhow::Result<()> {
    let db = fixture("def target(): ...\n")?;
    let prepared = prepare(&db);
    let declaration = declared(&db, &prepared)?.literal(&db).last_definition;
    let first = Signature::new(
        Parameters::standard([Parameter::positional_or_keyword(Name::new_static("first"))
            .with_annotated_type(Type::bool_literal(true))
            .with_default_type(Type::bool_literal(false))]),
        Type::bool_literal(false),
    )
    .with_source_overload_index(Some(3));
    let second =
        Signature::new(Parameters::empty(), Type::unknown()).with_source_overload_index(Some(7));
    let callable = CallableType::new(
        &db,
        CallableSignature::from_overloads([first, second]),
        CallableTypeKind::ClassMethodLike,
    )
    .with_deprecated(&db, declaration);
    let actual = promote(&db, &prepared, Type::Callable(callable), mode)?;
    let Type::Callable(mapped) = actual else {
        anyhow::bail!("callable mapping changed its variant");
    };
    assert_eq!(mapped.kind(&db), CallableTypeKind::ClassMethodLike);
    assert_eq!(mapped.deprecated(&db), Some(declaration));
    let overloads = &mapped.signatures(&db).overloads;
    assert_eq!(overloads.len(), 2);
    assert_eq!(overloads[0].source_overload_index(), Some(3));
    assert_eq!(overloads[1].source_overload_index(), Some(7));
    assert_eq!(
        overloads[0].parameters()[0].default_type(&db),
        Some(Type::bool_literal(false))
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let boolean = KnownClass::Bool.to_instance(&db, &env);
    assert_eq!(
        overloads[0].parameters()[0].annotated_type(),
        if mode == PromotionMode::On {
            Type::bool_literal(true)
        } else {
            boolean
        }
    );
    assert_eq!(
        overloads[0].return_ty,
        if mode == PromotionMode::On {
            boolean
        } else {
            Type::bool_literal(false)
        }
    );
    assert_eq!(
        actual,
        Type::Callable(callable).apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(mode, PromotionKind::Regular),
            TypeContext::default()
        )
    );
    Ok(())
}

/// Stored public and implementation signatures of an overloaded function are mapped in order.
/// Off retains the function and descriptor override while preserving defaults and implementation kinds.
#[test]
fn stored_function_payload_preserves_implementation_order() -> anyhow::Result<()> {
    let db = fixture(
        "from typing import overload\n@overload\ndef target(value): ...\n@overload\ndef target(value, extra): ...\ndef target(*args): ...\n",
    )?;
    let prepared = prepare(&db);
    let function = declared(&db, &prepared)?;
    assert!(function.literal(&db).has_separate_implementation(&db));
    let public = Signature::new(
        Parameters::standard(
            [Parameter::positional_or_keyword(Name::new_static("stored"))
                .with_annotated_type(Type::unknown())
                .with_default_type(Type::int_literal(9))],
        ),
        Type::unknown(),
    )
    .with_source_overload_index(Some(4));
    let first = CallableType::new(
        &db,
        CallableSignature::single(Signature::new(
            Parameters::empty(),
            Type::bool_literal(true),
        )),
        CallableTypeKind::StaticMethodLike,
    );
    let second = CallableType::new(
        &db,
        CallableSignature::single(Signature::new(Parameters::empty(), Type::int_literal(2))),
        CallableTypeKind::ClassMethodLike,
    );
    let function = function
        .with_probe_updated_signatures(
            &db,
            CallableSignature::single(public.clone()),
            Box::from([first, second]),
        )
        .probe_descriptor_kind_oracle(&db, CallableTypeKind::ClassMethodLike);
    let actual = promote(
        &db,
        &prepared,
        Type::FunctionLiteral(function),
        PromotionMode::Off,
    )?;
    let Type::FunctionLiteral(mapped) = actual else {
        anyhow::bail!("Off converted the function");
    };
    assert_eq!(
        mapped.updated_signature(&db),
        Some(&CallableSignature::single(public))
    );
    assert_eq!(
        mapped.updated_implementation_callables(&db),
        Some([first, second].as_slice())
    );
    assert_eq!(
        mapped.descriptor_kind(&db),
        Some(CallableTypeKind::ClassMethodLike)
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        actual,
        Type::FunctionLiteral(function).apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::Off, PromotionKind::Regular),
            TypeContext::default()
        )
    );
    Ok(())
}

/// Nested callable parameters flip polarity twice while all mapped children retain the original
/// visitor and context. Both On and Off children are required; a new visitor would replace the
/// transformation cache and recursion-tracking state shared with enclosing signatures.
#[test]
fn nested_callable_children_share_visitor_with_flipped_modes() -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let inner = CallableType::single(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::bool_literal(true))
            ]),
            Type::bool_literal(false),
        ),
    );
    let outer = CallableType::single(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::Callable(inner))
            ]),
            Type::unknown(),
        ),
    );
    let actual = promote(&db, &prepared, Type::Callable(outer), PromotionMode::On)?;
    let mapping = mapping_observations::mapping_snapshot();
    let Some(root) = mapping.roots[0] else {
        anyhow::bail!("missing mapping root");
    };
    let children = mapping.children[..mapping.child_count]
        .iter()
        .flatten()
        .filter(|child| matches!(child.mapping, OwnedMappingSnapshot::PromoteRegular(_)))
        .collect::<Vec<_>>();
    assert!(
        children
            .iter()
            .any(|child| child.mapping == OwnedMappingSnapshot::PromoteRegular(PromotionMode::Off))
    );
    assert!(
        children
            .iter()
            .any(|child| child.mapping == OwnedMappingSnapshot::PromoteRegular(PromotionMode::On))
    );
    assert!(
        children
            .iter()
            .all(|child| child.visitor == root.visitor && child.default_context)
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        actual,
        Type::Callable(outer).apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default()
        )
    );
    Ok(())
}

/// Selects signature descendants without introducing unrelated annotation queries.
#[derive(Clone, Copy, Debug)]
enum Descendant {
    Receiver,
    Starred,
}

/// Empty receiver constraints preserve ordinary promotion; a mapped starred variadic refuses at its exact boundary.
/// The starred annotation is Unknown, so this also protects refusal before exact-tuple inspection.
#[test_case::test_case(Descendant::Receiver; "present empty receiver")]
#[test_case::test_case(Descendant::Starred; "starred variadic without tuple")]
fn selected_descendants_keep_precise_outcomes(case: Descendant) -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let (signature, operation) = match case {
        Descendant::Receiver => (
            Signature::new(Parameters::empty(), Type::unknown())
                .with_probe_receiver_constraints(OwnedConstraintSet::always()),
            None,
        ),
        Descendant::Starred => (
            Signature::new(
                Parameters::standard([
                    Parameter::variadic(Name::new_static("args")).with_starred_annotation()
                ]),
                Type::unknown(),
            ),
            Some(MappingOperation::SignatureStarredExpansion),
        ),
    };
    let input = Type::Callable(CallableType::single(&db, signature));
    reset();
    let recording = Recording::start(&db, None, ChildAction::Observe);
    let result = controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Regular(PromotionMode::On),
        },
        &funded(),
    );
    let snapshot = recording.snapshot();
    drop(recording);
    if let Some(operation) = operation {
        assert_eq!(
            result,
            Ok(unavailable(OperationId::PublicPromotion(
                MaterializationOperation::Leaf(operation)
            )))
        );
    } else {
        let env = ProgramEnvironment::from_file(prepared.program_file());
        let ordinary = input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default(),
        );
        assert_eq!(result, Ok(AnalysisOutcome::Complete(ordinary)));
    }
    assert_drained(&snapshot);
    Ok(())
}

/// Creates two overloads whose first cold child under promotion On is the second return's Bool
/// fallback. Unknown types avoid earlier queries, so the mapper then owns the first completed
/// overload and the second overload's mapped parameters.
fn partial_input(db: &TestDb) -> Type<'_> {
    Type::Callable(CallableType::new(
        db,
        CallableSignature::from_overloads([
            Signature::new(Parameters::empty(), Type::unknown()),
            Signature::new(
                Parameters::standard([Parameter::positional_or_keyword(Name::new_static("value"))
                    .with_annotated_type(Type::unknown())]),
                Type::bool_literal(true),
            ),
        ]),
        CallableTypeKind::Regular,
    ))
}

/// Cancellation while the second overload is being mapped retires the enclosing mapping with no
/// active callable scopes, then the original database completes the same request in the same revision.
/// Partial signatures belong to those helper futures; this observes enclosing scope/attempt cleanup,
/// not separate heap-drop counters for their buffers.
#[test]
fn cancelled_signature_child_drains_partial_results_and_retries() -> anyhow::Result<()> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let input = partial_input(&db);
    let argument = KnownClassArgument::new(&db, KnownClass::Bool, program);
    let ingredient = known_class_to_instance_ingredient(&db);
    let key = ingredient.database_key_index(argument.as_id());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let revision = salsa::plumbing::current_revision(&db);
    let request = PromotionRequest {
        input,
        route: PromotionRoute::Regular(PromotionMode::On),
    };
    reset();
    let recording = Recording::start(&db, Some(key), ChildAction::Cancel);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    let snapshot = recording.snapshot();
    drop(recording);
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let child = snapshot
        .events
        .iter()
        .position(|event| matches!(event, Event::ChildEntered { live_mappings: 1 }))
        .ok_or_else(|| {
            anyhow::anyhow!("no child entered under the live signature mapping: {snapshot:?}")
        })?;
    assert!(
        snapshot.events[..child]
            .iter()
            .any(|event| matches!(event, Event::Pending { live_mappings: 1 }))
    );
    assert_drained(&snapshot);
    let actual = promote(&db, &prepared, input, PromotionMode::On)?;
    let env = ProgramEnvironment::from_program(program);
    assert_eq!(
        actual,
        input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default()
        )
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

/// Observes whether the second overload's cold return child entered under a chosen allowance.
/// Each calibration run uses fresh source and interned inputs, so memo warmth cannot move the boundary.
fn return_child_entered(policy: &AnalysisPolicy) -> anyhow::Result<bool> {
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let input = partial_input(&db);
    let argument = KnownClassArgument::new(&db, KnownClass::Bool, program);
    let key = known_class_to_instance_ingredient(&db).database_key_index(argument.as_id());
    reset();
    let recording = Recording::start(&db, Some(key), ChildAction::Observe);
    let result = controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Regular(PromotionMode::On),
        },
        policy,
    );
    let snapshot = recording.snapshot();
    drop(recording);
    assert_drained(&snapshot);
    match result {
        Ok(AnalysisOutcome::Complete(_))
        | Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit | AnalysisIncomplete::RequestedAllocationLimit,
            ..
        }) => {}
        result => anyhow::bail!("unexpected signature budget calibration: {result:?}"),
    }
    Ok(snapshot
        .events
        .iter()
        .any(|event| matches!(event, Event::ChildEntered { .. })))
}

/// Work and requested-byte refusals before the second return child retain the exact limit reason,
/// leave that canonical child unpublished, and retire the enclosing mapping with zero active scopes
/// before a same-revision retry. As in the cancellation control, buffer cleanup follows the helper
/// futures' ownership; the observation covers scope and attempt cleanup rather than individual drops.
#[test_case::test_case(Resource::Work; "semantic work")]
#[test_case::test_case(Resource::Bytes; "requested bytes")]
fn refused_signature_child_keeps_limits_independent(resource: Resource) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(return_child_entered(&resource.policy(high))?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if return_child_entered(&resource.policy(middle))? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture("pass\n")?;
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let input = partial_input(&db);
    let argument = KnownClassArgument::new(&db, KnownClass::Bool, program);
    let ingredient = known_class_to_instance_ingredient(&db);
    let key = ingredient.database_key_index(argument.as_id());
    let revision = salsa::plumbing::current_revision(&db);
    reset();
    let recording = Recording::start(&db, Some(key), ChildAction::Observe);
    let result = controlled_member_operation(
        &prepared,
        PromotionRequest {
            input,
            route: PromotionRoute::Regular(PromotionMode::On),
        },
        &resource.policy(low),
    );
    let snapshot = recording.snapshot();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert_eq!(snapshot.count(Stage::ScalarRequest(KnownClass::Bool)), 1);
    assert!(
        !snapshot
            .events
            .iter()
            .any(|event| matches!(event, Event::ChildEntered { .. }))
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_drained(&snapshot);
    let actual = promote(&db, &prepared, input, PromotionMode::On)?;
    let env = ProgramEnvironment::from_program(program);
    assert_eq!(
        actual,
        input.apply_type_mapping(
            &db,
            &env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default()
        )
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

/// Fetches an existing function's last-definition signature through its canonical source route.
#[derive(Clone, Copy, Debug)]
struct LastSignatureRequest<'db>(FunctionType<'db>);

impl<'db> MemberOperation<'db> for LastSignatureRequest<'db> {
    type Output = &'db Signature<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        effects
            .last_signature_future(|| access.function_last_definition_signature(self.0))
            .await?
            .await
    }
}

/// Obtains the fixture's function identity without requesting its last-definition signature.
fn last_signature_function<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
) -> anyhow::Result<FunctionType<'db>> {
    declared(db, prepared)
}

/// Requires a cold last-definition memo, fetches it under the supplied function's key, and checks
/// that the ordinary query borrows the same published signature afterward.
fn fetch_cold_last_signature<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    function: FunctionType<'db>,
) -> anyhow::Result<&'db Signature<'db>> {
    let ingredient = function_last_definition_signature_ingredient(db);
    let key = function.as_id();
    assert_eq!(
        FinalSourceMemo::certify(db as &dyn Db, ingredient, key).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    observations::reset(None);
    let captured = capture(db, || {
        controlled_member_operation(prepared, LastSignatureRequest(function), &funded())
    })
    .map_err(|error| anyhow::anyhow!("last-definition read capture: {error:?}"))?;
    let Ok(AnalysisOutcome::Complete(signature)) = captured.value else {
        anyhow::bail!("funded last-definition signature: {:?}", captured.value);
    };
    assert!(FinalSourceMemo::certify(db as &dyn Db, ingredient, key).is_ok());
    assert!(
        captured
            .reads
            .iter()
            .any(|read| read.key == ingredient.database_key_index(key))
    );
    assert!(std::ptr::eq(
        signature,
        function.last_definition_signature(db),
    ));
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
    Ok(signature)
}

/// A function with two stored public overloads publishes the last stored overload through the
/// existing last-definition query without substituting the source signature or first overload.
#[test]
fn last_definition_signature_selects_last_stored_public_overload() -> anyhow::Result<()> {
    let db = fixture("def target(value): ...\n")?;
    let prepared = prepare(&db);
    let original = last_signature_function(&db, &prepared)?;
    let first = Signature::new(Parameters::empty(), Type::bool_literal(false));
    let second = Signature::new(
        Parameters::standard([
            Parameter::positional_or_keyword(Name::new_static("updated"))
                .with_default_type(Type::int_literal(7)),
        ]),
        Type::int_literal(11),
    )
    .with_source_overload_index(Some(5));
    let function = original.with_probe_updated_signatures(
        &db,
        CallableSignature::from_overloads([first.clone(), second.clone()]),
        Box::<[CallableType<'_>]>::from([]),
    );
    assert_ne!(function, original);
    let signature = fetch_cold_last_signature(&db, &prepared, function)?;
    assert_eq!(signature, &second);
    assert_ne!(signature, &first);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_last_definition_signature_ingredient(&db),
            original.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    Ok(())
}

/// A function without stored updates infers its last source definition and publishes the same
/// parameters and return type as ordinary inference in an independent database.
#[test]
fn last_definition_signature_infers_raw_definition() -> anyhow::Result<()> {
    let source = "def target(value): ...\n";
    let db = fixture(source)?;
    let prepared = prepare(&db);
    let function = last_signature_function(&db, &prepared)?;
    assert!(function.updated_signature(&db).is_none());
    let signature = fetch_cold_last_signature(&db, &prepared, function)?;

    let ordinary_db = fixture(source)?;
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_function = last_signature_function(&ordinary_db, &ordinary_prepared)?;
    let expected = ordinary_function.last_definition_signature(&ordinary_db);
    assert_eq!(signature.parameters().len(), expected.parameters().len());
    assert_eq!(signature.parameters().len(), 1);
    assert_eq!(
        signature.parameters()[0].name(),
        expected.parameters()[0].name()
    );
    assert_eq!(
        signature.parameters()[0].annotated_type(),
        expected.parameters()[0].annotated_type()
    );
    assert_eq!(signature.return_ty, expected.return_ty);
    assert_eq!(
        signature.source_overload_index(),
        expected.source_overload_index()
    );
    Ok(())
}
