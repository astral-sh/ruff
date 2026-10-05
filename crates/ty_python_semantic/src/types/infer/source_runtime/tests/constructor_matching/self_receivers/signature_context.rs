//! Constructed signature metadata isolates legacy collection and mixed-context decisions.
//! These controls use real ordinary and controlled adapters; they do not claim cold file inference.

use super::*;
use crate::types::generics::signature_context::merge_signature_contexts_with;
use crate::types::local_transfer::boxed_future_with_fixed_transfers_at;
use crate::types::signatures::source::SignatureSourceEffects;
use crate::types::signatures::{ParameterDefault, ParameterKind};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance};

/// Selects collection from a borrowed parameter list or merging of existing context handles.
#[derive(Debug)]
enum ContextOperation<'a, 'db> {
    Collect {
        definition: Definition<'db>,
        parameters: &'a Parameters<'db>,
        return_ty: Type<'db>,
    },
    Merge {
        pep695: Option<GenericContext<'db>>,
        legacy: Option<GenericContext<'db>>,
    },
}

impl<'db> MemberOperation<'db> for ContextOperation<'_, 'db> {
    type Output = Option<GenericContext<'db>>;

    /// Runs the production collection provider or shared merge with SourceEffects.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        match self {
            Self::Collect { definition, parameters, return_ty } => {
                SignatureSourceEffects::legacy_generic_context(
                    &effects, access.db(), definition, parameters, return_ty,
                ).await
            }
            Self::Merge { pep695, legacy } => {
                boxed_future_with_fixed_transfers_at(
                    access.endpoint(),
                    Ok((0, 0)),
                    || merge_signature_contexts_with(pep695, legacy, &effects),
                )
                .await?
                .await
            }
        }
    }
}

/// Creates a variable whose declared kind and lexical owner can be tested independently.
fn variable<'db>(
    db: &'db TestDb,
    definition: Definition<'db>,
    name: &'static str,
    kind: TypeVarKind,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new_static(name), None, kind),
            None,
            Some(TypeVarVariance::Invariant),
            None,
        ),
        BindingContext::Definition(definition),
        None,
        TypeVarNonce::NONE,
    )
}

/// Selects an empty collection, excluded declarations, or ordered annotations/default/return types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionCase {
    Empty,
    Filtered,
    Ordered,
}

/// Collection keeps only legacy/Self variables owned by this definition and preserves first-use
/// order across parameter annotations, eager defaults and the return type. Ordinary comparison
/// follows controlled execution; the constructed metadata is not a cold source-inference fixture.
#[test_case::test_case(CollectionCase::Empty; "empty")]
#[test_case::test_case(CollectionCase::Filtered; "definition and kind filters")]
#[test_case::test_case(CollectionCase::Ordered; "annotation default return order")]
fn collection_order_and_definition_filter(case: CollectionCase) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let keys = seed(&prepared, "target");
    let first = variable(&db, keys.method, "A", TypeVarKind::LegacyTypeVar);
    let default = variable(&db, keys.method, "B", TypeVarKind::LegacyTypeVar);
    let receiver = variable(&db, keys.method, "Self", TypeVarKind::TypingSelf);
    let returned = variable(&db, keys.method, "C", TypeVarKind::LegacyTypeVar);
    let other_owner = variable(&db, keys.class, "Other", TypeVarKind::TypingSelf);
    let pep695 = variable(&db, keys.method, "U", TypeVarKind::Pep695TypeVar);
    let (parameters, return_ty, expected) = match case {
        CollectionCase::Empty => (Parameters::empty(), Type::bool_literal(true), vec![]),
        CollectionCase::Filtered => (
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::TypeVar(other_owner)),
                Parameter::positional_only(None).with_annotated_type(Type::TypeVar(pep695)),
            ]),
            Type::TypeVar(other_owner),
            vec![],
        ),
        CollectionCase::Ordered => (
            Parameters::standard([
                Parameter::positional_only(None)
                    .with_annotated_type(Type::TypeVar(first))
                    .with_default_type(Type::TypeVar(default)),
                Parameter::positional_only(None)
                    .with_annotated_type(Type::TypeVar(receiver))
                    .with_default_type(Type::TypeVar(first)),
            ]),
            Type::TypeVar(returned),
            vec![first, default, receiver, returned],
        ),
    };
    let revision = salsa::plumbing::current_revision(&db);
    let actual = controlled_member_operation(
        &prepared,
        ContextOperation::Collect { definition: keys.method, parameters: &parameters, return_ty },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(actual)) = actual else {
        panic!("context collection did not complete: {actual:?}");
    };
    assert_eq!(actual.is_none(), expected.is_empty());
    assert_eq!(actual.into_iter().flat_map(|context| context.variables(&db)).collect::<Vec<_>>(), expected);
    assert_eq!(actual, GenericContext::from_function_params(&db, keys.method, &parameters, return_ty));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Selects every optional-context branch and the complete sole-Self predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MergeCase {
    Neither,
    PepOnly,
    LegacyOnly,
    EmptyOnly,
    SoleSelf,
    SoleLegacy,
    SelfAndLegacy,
    EmptyLegacy,
}

/// Only a sole typing.Self context is prepended to PEP 695 parameters. Other mixed contexts retain
/// the exact PEP 695 handle; absent sides retain the other handle, including an empty context.
#[test_case::test_case(MergeCase::Neither; "both absent")]
#[test_case::test_case(MergeCase::PepOnly; "PEP only")]
#[test_case::test_case(MergeCase::LegacyOnly; "legacy only")]
#[test_case::test_case(MergeCase::EmptyOnly; "empty only")]
#[test_case::test_case(MergeCase::SoleSelf; "Self then PEP")]
#[test_case::test_case(MergeCase::SoleLegacy; "legacy plus PEP")]
#[test_case::test_case(MergeCase::SelfAndLegacy; "Self and legacy plus PEP")]
#[test_case::test_case(MergeCase::EmptyLegacy; "empty plus PEP")]
fn exact_mixed_context_policy(case: MergeCase) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let keys = seed(&prepared, "target");
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let receiver = variable(&db, keys.method, "Self", TypeVarKind::TypingSelf);
    let legacy_var = variable(&db, keys.method, "T", TypeVarKind::LegacyTypeVar);
    let first = variable(&db, keys.method, "U", TypeVarKind::Pep695TypeVar);
    let last = variable(&db, keys.method, "V", TypeVarKind::Pep695TypeVar);
    let pep = GenericContext::from_typevar_instances(&db, &env, [first, last]);
    let self_context = GenericContext::from_typevar_instances(&db, &env, [receiver]);
    let legacy_context = GenericContext::from_typevar_instances(&db, &env, [legacy_var]);
    let mixed = GenericContext::from_typevar_instances(&db, &env, [receiver, legacy_var]);
    let empty = GenericContext::from_typevar_instances(&db, &env, []);
    let (pep695, legacy, expected) = match case {
        MergeCase::Neither => (None, None, vec![]),
        MergeCase::PepOnly => (Some(pep), None, vec![first, last]),
        MergeCase::LegacyOnly => (None, Some(legacy_context), vec![legacy_var]),
        MergeCase::EmptyOnly => (None, Some(empty), vec![]),
        MergeCase::SoleSelf => (Some(pep), Some(self_context), vec![receiver, first, last]),
        MergeCase::SoleLegacy => (Some(pep), Some(legacy_context), vec![first, last]),
        MergeCase::SelfAndLegacy => (Some(pep), Some(mixed), vec![first, last]),
        MergeCase::EmptyLegacy => (Some(pep), Some(empty), vec![first, last]),
    };
    let actual = controlled_member_operation(&prepared, ContextOperation::Merge { pep695, legacy }, &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = actual else {
        panic!("context merge did not complete: {actual:?}");
    };
    assert_eq!(actual.into_iter().flat_map(|context| context.variables(&db)).collect::<Vec<_>>(), expected);
    match case {
        MergeCase::Neither => assert_eq!(actual, None),
        MergeCase::LegacyOnly | MergeCase::EmptyOnly => assert_eq!(actual, legacy),
        MergeCase::SoleSelf => assert_ne!(actual, pep695),
        MergeCase::PepOnly | MergeCase::SoleLegacy | MergeCase::SelfAndLegacy | MergeCase::EmptyLegacy => assert_eq!(actual, pep695),
    }
    assert_eq!(actual, GenericContext::merge_pep695_and_legacy(&db, pep695, legacy));
    assert_no_active_attempt();
}

/// Source-created defaults retain a deferred descriptor and are not evaluated by collection.
/// The public signature is prepared ordinarily to supply metadata; this separately tests the
/// collector, not a cold signature query. Evaluating the unresolved default would require inference.
#[test]
fn deferred_source_default_is_not_collected() {
    let db = database("class Product:\n    def target(self, value=missing_default): ...\n");
    let prepared = prepare(&db);
    let keys = seed(&prepared, "target");
    let function = crate::types::binding_type(&db, keys.method).as_function_literal().expect("fixture function");
    let signature = function.last_definition_signature(&db);
    let parameters = signature.parameters();
    assert!(parameters[1].has_default());
    assert_eq!(parameters[1].eager_default_type(), None);
    let ParameterKind::PositionalOrKeyword { default_type: Some(ParameterDefault::Deferred(parameter)), .. } = parameters[1].kind() else {
        panic!("expected a deferred source default");
    };
    let mut reader = db.clone();
    reader.take_salsa_events();
    let actual = controlled_member_operation(
        &prepared,
        ContextOperation::Collect { definition: keys.method, parameters, return_ty: signature.return_ty },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(actual)) = actual else {
        panic!("deferred-default collection did not complete: {actual:?}");
    };
    let events = reader.take_salsa_events();
    assert!(find_will_execute_event_by_name(&db, "parameter_default_type", Some(parameter.as_id()), &events).is_none());
    assert!(find_will_execute_event_by_name(&db, "infer_function_default_types", Some(keys.method.as_id()), &events).is_none());
    assert_eq!(actual, GenericContext::from_function_params(&db, keys.method, parameters, signature.return_ty));
    assert_eq!(parameters[1].eager_default_type(), None);
    assert_no_active_attempt();
}
