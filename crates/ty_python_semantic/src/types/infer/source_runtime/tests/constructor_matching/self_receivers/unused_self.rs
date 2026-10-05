//! Constructed signatures exercise unused-Self binding and constructor context merging.
//! Ordinary comparisons follow controlled execution; fixture construction is not cold inference.

use super::*;
use crate::types::call::bind::constructor_preparation::{
    ReceiverBindingEffects, bind_callable_unused_self_with,
};
use crate::types::legacy_inline;
use crate::types::local_transfer::boxed_future_with_fixed_transfers_at;
use crate::types::signatures::constructor_preparation::{
    InlineConstructorSignatureEffects, bind_unused_self_with,
};
use crate::types::signatures::{ParameterDefault, ParameterKind};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance};

/// Borrows the original signature until the complete unused-Self operation returns a replacement.
#[derive(Clone, Copy, Debug)]
struct Bind<'a, 'db> {
    signature: &'a Signature<'db>,
    receiver: Type<'db>,
}

impl<'db> MemberOperation<'db> for Bind<'_, 'db> {
    type Output = Option<Signature<'db>>;

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
        boxed_future_with_fixed_transfers_at(access.endpoint(), Ok((0, 0)), || {
            bind_unused_self_with(access.db(), &env, self.signature, self.receiver, &effects)
        })
        .await?
        .await
    }
}

/// Constructs a declaration with explicit domain/default metadata and a method-local identity.
/// Each identity retains the supplied source definition so bound-default evaluation can recover
/// the environment even for an eager default. Bounds and defaults do not change that identity.
fn variable<'db>(
    db: &'db TestDb,
    definition: Definition<'db>,
    name: &'static str,
    kind: TypeVarKind,
    bounds: Option<TypeVarBoundOrConstraints<'db>>,
    default: Option<TypeVarDefaultEvaluation<'db>>,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new_static(name),
                Some(definition),
                kind,
            ),
            bounds.map(TypeVarBoundOrConstraintsEvaluation::Eager),
            Some(TypeVarVariance::Covariant),
            default,
        ),
        BindingContext::Definition(definition),
        None,
        TypeVarNonce::NONE.increment(),
    )
}

/// Selects the stored signature flags preserved by a successful replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignatureKind {
    Regular,
    ParamSpecValue,
    RecursionRecovery,
}

/// Builds a signature eligible for unused-Self binding, with source metadata, an eager default on
/// `value`, and impossible receiver-constraint metadata.
fn eligible<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    kind: SignatureKind,
) -> Signature<'db> {
    let definition = seed(prepared, "target").method;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let receiver = variable(
        db,
        definition,
        "Self",
        TypeVarKind::TypingSelf,
        Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(
            true,
        ))),
        None,
    );
    let context = GenericContext::from_typevar_instances(db, &env, [receiver]);
    let signature = Signature::new_generic(
        Some(context),
        Parameters::standard([
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_inferred_type(Type::TypeVar(receiver))
                .with_definition(Some(definition)),
            Parameter::keyword_only(Name::new_static("value"))
                .with_annotated_type(Type::bool_literal(false))
                .with_default_type(Type::TypeVar(receiver))
                .with_definition(Some(definition)),
        ]),
        Type::unknown(),
    )
    .with_definition(Some(definition))
    .with_source_overload_index(Some(4))
    .with_probe_receiver_constraints(OwnedConstraintSet::default());
    match kind {
        SignatureKind::Regular => signature,
        SignatureKind::ParamSpecValue => signature.into_paramspec_value(),
        SignatureKind::RecursionRecovery => signature.with_recursion_recovery(),
    }
}

/// Binding substitutes the receiver and eager default, keeps the receiver parameter, and preserves
/// definition, overload, parameter, constraint and signature-flag metadata. The removed declaration
/// leaves a canonical present-but-empty context.
#[test_case::test_case(SignatureKind::Regular; "regular signature")]
#[test_case::test_case(SignatureKind::ParamSpecValue; "ParamSpec value flag")]
#[test_case::test_case(SignatureKind::RecursionRecovery; "recursion recovery flag")]
fn complete_unused_self_preserves_signature_metadata(kind: SignatureKind) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = eligible(&db, &prepared, kind);
    let original = input.clone();
    let receiver = Type::bool_literal(true);
    mapping_observations::reset(None);
    let result = controlled_member_operation(
        &prepared,
        Bind {
            signature: &input,
            receiver,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(Some(actual))) = result else {
        panic!("unused Self replacement: {result:?}");
    };
    assert_eq!(input, original);
    assert_eq!(actual.parameters().len(), input.parameters().len());
    assert_eq!(actual.parameters()[0].annotated_type(), receiver);
    assert_eq!(actual.parameters()[1].eager_default_type(), Some(receiver));
    assert_eq!(actual.definition, input.definition);
    assert_eq!(actual.source_overload_index(), Some(4));
    assert_eq!(actual.receiver_constraints(), input.receiver_constraints());
    assert_eq!(
        actual.generic_context,
        Some(GenericContext::from_typevar_instances(&db, &env, []))
    );
    assert_eq!(
        actual.is_recursion_recovery(),
        kind == SignatureKind::RecursionRecovery
    );
    let ordinary = legacy_inline(bind_unused_self_with(
        &db,
        &env,
        &input,
        receiver,
        &InlineConstructorSignatureEffects,
    ));
    assert_eq!(Some(actual), ordinary);
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1);
    assert!(snapshot.child_count >= 3);
    let root = snapshot.roots[0].expect("signature mapping root");
    assert!(
        snapshot.children[..snapshot.child_count]
            .iter()
            .flatten()
            .all(|child| child.visitor == root.visitor && child.mapping == root.mapping)
    );
    assert_no_active_attempt();
}

/// Unused-Self substitution preserves a source parameter's deferred default and metadata without
/// inferring its unresolved expression. Ordinary source-signature construction supplies the
/// parameter descriptor; only the subsequent substitution is the controlled operation.
#[test]
fn unused_self_keeps_deferred_parameter_defaults() {
    let db = database("class Product:\n    def target(self, value=missing_default): ...\n");
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let function = crate::types::binding_type(&db, definition)
        .as_function_literal()
        .expect("fixture source function");
    let source = function.last_definition_signature(&db);
    let deferred = source.parameters()[1].clone();
    let ParameterKind::PositionalOrKeyword {
        default_type: Some(ParameterDefault::Deferred(parameter)),
        ..
    } = deferred.kind()
    else {
        panic!("expected source deferred default");
    };
    let parameter = *parameter;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let receiver = variable(
        &db,
        definition,
        "Self",
        TypeVarKind::TypingSelf,
        Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(
            true,
        ))),
        None,
    );
    let context = GenericContext::from_typevar_instances(&db, &env, [receiver]);
    let input = Signature::new_generic(
        Some(context),
        Parameters::standard([
            Parameter::positional_only(Some(Name::new_static("self")))
                .with_inferred_type(Type::TypeVar(receiver)),
            deferred.clone(),
        ]),
        Type::unknown(),
    )
    .with_definition(Some(definition));
    let mut reader = db.clone();
    reader.take_salsa_events();
    let result = controlled_member_operation(
        &prepared,
        Bind {
            signature: &input,
            receiver: Type::bool_literal(true),
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(Some(actual))) = result else {
        panic!("unused Self with deferred default: {result:?}");
    };
    let events = reader.take_salsa_events();
    assert_eq!(actual.parameters()[1], deferred);
    assert_eq!(actual.parameters()[1].eager_default_type(), None);
    assert!(
        find_will_execute_event_by_name(
            &db,
            "parameter_default_type",
            Some(parameter.as_id()),
            &events
        )
        .is_none()
    );
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_function_default_types",
            Some(definition.as_id()),
            &events
        )
        .is_none()
    );
    let ordinary = legacy_inline(bind_unused_self_with(
        &db,
        &env,
        &input,
        Type::bool_literal(true),
        &InlineConstructorSignatureEffects,
    ));
    assert_eq!(Some(actual), ordinary);
    assert_no_active_attempt();
}

/// Selects a condition that rules out substitution before a later declaration's default is read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Ineligible {
    NoContext,
    NoReceiver,
    KeywordReceiver,
    ParameterSelf,
    ReturnSelf,
    NonSelfReceiver,
    NoBound,
    RejectedBound,
    BoundReferencesSelf,
    ConstraintsReferenceSelf,
    DefaultReferencesSelf,
}

/// Ineligible receivers return no replacement and leave later defaults unevaluated. References to
/// Self in another annotation or declaration prevent binding, and a rejected upper bound stops
/// before scanning declarations. The lazy declaration is a sentinel for that ordering.
/// Reading the eager default invokes `BindLegacyTypevars`; its Self reference survives that pass
/// and prevents the `Single` substitution from being reached.
#[test_case::test_case(Ineligible::NoContext; "absent context")]
#[test_case::test_case(Ineligible::NoReceiver; "absent receiver")]
#[test_case::test_case(Ineligible::KeywordReceiver; "nonpositional receiver")]
#[test_case::test_case(Ineligible::ParameterSelf; "Self in another parameter")]
#[test_case::test_case(Ineligible::ReturnSelf; "Self in return")]
#[test_case::test_case(Ineligible::NonSelfReceiver; "ordinary type variable receiver")]
#[test_case::test_case(Ineligible::NoBound; "receiver without upper bound")]
#[test_case::test_case(Ineligible::RejectedBound; "receiver fails upper bound")]
#[test_case::test_case(Ineligible::BoundReferencesSelf; "Self in another bound")]
#[test_case::test_case(Ineligible::ConstraintsReferenceSelf; "Self in another constraint")]
#[test_case::test_case(Ineligible::DefaultReferencesSelf; "Self in another default")]
fn ineligible_unused_self_preserves_laziness(case: Ineligible) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let definition = seed(&prepared, "target").method;
    let receiver_kind = if case == Ineligible::NonSelfReceiver {
        TypeVarKind::LegacyTypeVar
    } else {
        TypeVarKind::TypingSelf
    };
    let bound = match case {
        Ineligible::NoBound => None,
        Ineligible::RejectedBound => Some(TypeVarBoundOrConstraints::UpperBound(
            Type::bool_literal(false),
        )),
        Ineligible::NoContext
        | Ineligible::NoReceiver
        | Ineligible::KeywordReceiver
        | Ineligible::ParameterSelf
        | Ineligible::ReturnSelf
        | Ineligible::NonSelfReceiver
        | Ineligible::BoundReferencesSelf
        | Ineligible::ConstraintsReferenceSelf
        | Ineligible::DefaultReferencesSelf => Some(TypeVarBoundOrConstraints::UpperBound(
            Type::bool_literal(true),
        )),
    };
    let receiver = variable(&db, definition, "Self", receiver_kind, bound, None);
    let (other_bound, other_default) = match case {
        Ineligible::BoundReferencesSelf => (
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(
                receiver,
            ))),
            None,
        ),
        Ineligible::ConstraintsReferenceSelf => (
            Some(TypeVarBoundOrConstraints::Constraints(
                TypeVarConstraints::new(
                    &db,
                    Box::from([Type::bool_literal(false), Type::TypeVar(receiver)]),
                ),
            )),
            None,
        ),
        Ineligible::DefaultReferencesSelf => (
            None,
            Some(TypeVarDefaultEvaluation::Eager(Type::TypeVar(receiver))),
        ),
        Ineligible::NoContext
        | Ineligible::NoReceiver
        | Ineligible::KeywordReceiver
        | Ineligible::ParameterSelf
        | Ineligible::ReturnSelf
        | Ineligible::NonSelfReceiver
        | Ineligible::NoBound
        | Ineligible::RejectedBound => (None, None),
    };
    let other = variable(
        &db,
        definition,
        "T",
        TypeVarKind::LegacyTypeVar,
        other_bound,
        other_default,
    );
    let later = variable(
        &db,
        definition,
        "Later",
        TypeVarKind::LegacyTypeVar,
        None,
        Some(TypeVarDefaultEvaluation::Lazy),
    );
    let context = (case != Ineligible::NoContext)
        .then(|| GenericContext::from_typevar_instances(&db, &env, [receiver, other, later]));
    let parameter = if case == Ineligible::KeywordReceiver {
        Parameter::keyword_only(Name::new_static("self"))
    } else {
        Parameter::positional_only(Some(Name::new_static("self")))
    }
    .with_annotated_type(Type::TypeVar(receiver));
    let parameters = match case {
        Ineligible::NoReceiver => Parameters::empty(),
        Ineligible::ParameterSelf => Parameters::standard([
            parameter,
            Parameter::positional_only(None).with_annotated_type(Type::TypeVar(receiver)),
        ]),
        Ineligible::NoContext
        | Ineligible::KeywordReceiver
        | Ineligible::ReturnSelf
        | Ineligible::NonSelfReceiver
        | Ineligible::NoBound
        | Ineligible::RejectedBound
        | Ineligible::BoundReferencesSelf
        | Ineligible::ConstraintsReferenceSelf
        | Ineligible::DefaultReferencesSelf => Parameters::standard([parameter]),
    };
    let returned = if case == Ineligible::ReturnSelf {
        Type::TypeVar(receiver)
    } else {
        Type::unknown()
    };
    let signature = Signature::new_generic(context, parameters, returned);
    let original = signature.clone();
    let ingredient = bound_typevar_default_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, later.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    mapping_observations::reset(None);
    let actual = controlled_member_operation(
        &prepared,
        Bind {
            signature: &signature,
            receiver: Type::bool_literal(true),
        },
        &funded(),
    );
    assert_eq!(actual, Ok(AnalysisOutcome::Complete(None)));
    let expected_mapping = match case {
        Ineligible::DefaultReferencesSelf => Some(OwnedMappingSnapshot::BindLegacyTypevars(
            mapping_observations::BindingContextSnapshot::Definition(definition.as_id()),
        )),
        Ineligible::NoContext
        | Ineligible::NoReceiver
        | Ineligible::KeywordReceiver
        | Ineligible::ParameterSelf
        | Ineligible::ReturnSelf
        | Ineligible::NonSelfReceiver
        | Ineligible::NoBound
        | Ineligible::RejectedBound
        | Ineligible::BoundReferencesSelf
        | Ineligible::ConstraintsReferenceSelf => None,
    };
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(
        snapshot.root_count,
        usize::from(expected_mapping.is_some()),
        "{snapshot:?}",
    );
    assert_eq!(
        snapshot.roots[0].map(|root| root.mapping),
        expected_mapping,
        "{snapshot:?}",
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, later.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_eq!(signature, original);
    let ordinary = legacy_inline(bind_unused_self_with(
        &db,
        &env,
        &signature,
        Type::bool_literal(true),
        &InlineConstructorSignatureEffects,
    ));
    assert_eq!(ordinary, None);
    assert_no_active_attempt();
}

/// Calls the constructor's context-merge adapter with existing stored handles.
#[derive(Clone, Copy, Debug)]
struct Merge<'db> {
    existing: Option<GenericContext<'db>>,
    incoming: GenericContext<'db>,
}

impl<'db> MemberOperation<'db> for Merge<'db> {
    type Output = GenericContext<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        ReceiverBindingEffects::merge_generic_context(
            &SourceEffects::new(access, program),
            access.db(),
            self.existing,
            self.incoming,
        )
        .await
    }
}

/// Selects direct incoming-handle return, an empty existing context, or ordered duplicate merging.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExistingContext {
    Absent,
    Empty,
    Populated,
}

/// Constructor merging retains the incoming handle when no context exists. Otherwise it preserves
/// existing-before-incoming order and installs an incoming declaration at the old position of the
/// same full bound identity, including incoming bounds/default metadata.
#[test_case::test_case(ExistingContext::Absent; "incoming handle")]
#[test_case::test_case(ExistingContext::Empty; "empty existing context")]
#[test_case::test_case(ExistingContext::Populated; "ordered identity replacement")]
fn constructor_merge_preserves_identity_order_and_canonical_result(case: ExistingContext) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = variable(&db, definition, "A", TypeVarKind::LegacyTypeVar, None, None);
    let original = variable(&db, definition, "B", TypeVarKind::LegacyTypeVar, None, None);
    let last = variable(&db, definition, "C", TypeVarKind::LegacyTypeVar, None, None);
    let incoming_value = variable(
        &db,
        definition,
        "B",
        TypeVarKind::LegacyTypeVar,
        Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(
            true,
        ))),
        Some(TypeVarDefaultEvaluation::Eager(Type::bool_literal(false))),
    );
    assert_ne!(original, incoming_value);
    assert_eq!(original.identity(&db), incoming_value.identity(&db));
    let incoming = GenericContext::from_typevar_instances(&db, &env, [incoming_value, last]);
    let existing = match case {
        ExistingContext::Absent => None,
        ExistingContext::Empty => Some(GenericContext::from_typevar_instances(&db, &env, [])),
        ExistingContext::Populated => Some(GenericContext::from_typevar_instances(
            &db,
            &env,
            [first, original],
        )),
    };
    let result = controlled_member_operation(&prepared, Merge { existing, incoming }, &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = result else {
        panic!("constructor context merge: {result:?}");
    };
    let expected = match case {
        ExistingContext::Absent | ExistingContext::Empty => {
            assert_eq!(actual, incoming);
            vec![incoming_value, last]
        }
        ExistingContext::Populated => vec![first, incoming_value, last],
    };
    assert_eq!(actual.variables(&db).collect::<Vec<_>>(), expected);
    assert_eq!(
        actual,
        GenericContext::from_typevar_instances(&db, &env, expected)
    );
    let ordinary = legacy_inline(ReceiverBindingEffects::merge_generic_context(
        &InlineConstructorSignatureEffects,
        &db,
        existing,
        incoming,
    ));
    assert_eq!(actual, ordinary);
    assert_no_active_attempt();
}

/// Retains one caller-owned overload while the real constructor adapter prepares and installs it.
#[derive(Debug)]
struct Install<'a, 'db> {
    binding: &'a mut CallableBinding<'db>,
    receiver: Type<'db>,
}

impl<'db> MemberOperation<'db> for Install<'_, 'db> {
    type Output = ();

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
        boxed_future_with_fixed_transfers_at(access.endpoint(), Ok((0, 0)), || {
            bind_callable_unused_self_with(access.db(), &env, self.binding, self.receiver, &effects)
        })
        .await?
        .await
    }
}

/// Runs the real adapter once on a fresh constructed input under the supplied policy and returns
/// whether it installed the receiver annotation.
fn installs_with(policy: &AnalysisPolicy) -> bool {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let signature = eligible(&db, &prepared, SignatureKind::Regular);
    let mut binding = CallableBinding::from_overloads(Type::unknown(), [signature]);
    let result = controlled_member_operation(
        &prepared,
        Install {
            binding: &mut binding,
            receiver: Type::bool_literal(true),
        },
        policy,
    );
    assert!(
        result.is_ok(),
        "installation calibration failed: {result:?}"
    );
    binding.overloads()[0].signature.parameters()[0].annotated_type() == Type::bool_literal(true)
}

/// Independent work and byte refusals stop before the overload is installed. The unchanged binding
/// completes with the identical receiver in the same revision, preserving ordinary binding metadata.
/// This direct operation has no canonical parent query whose memo could be published.
#[test_case::test_case(Resource::Work; "unused Self installation work")]
#[test_case::test_case(Resource::Bytes; "unused Self installation bytes")]
fn unused_self_installation_refuses_without_replacing_original(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(installs_with(&resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if installs_with(&resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let signature = eligible(&db, &prepared, SignatureKind::Regular);
    let source = CallableBinding::from_overloads(Type::unknown(), [signature])
        .with_bound_type(Type::bool_literal(true));
    let mut actual = source.clone();
    let revision = salsa::plumbing::current_revision(&db);
    mapping_observations::reset(None);
    let refused = controlled_member_operation(
        &prepared,
        Install {
            binding: &mut actual,
            receiver: Type::bool_literal(true),
        },
        &resource.policy(low),
    );
    assert_eq!(
        refused,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert_eq!(format!("{actual:#?}"), format!("{source:#?}"));
    assert_eq!(mapping_observations::mapping_snapshot().root_count, 1);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Install {
                binding: &mut actual,
                receiver: Type::bool_literal(true),
            },
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(()))
    );
    assert_eq!(actual.bound_type, source.bound_type);
    assert_eq!(
        actual.overloads()[0].signature.parameters().len(),
        source.overloads()[0].signature.parameters().len()
    );
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut ordinary = source;
    legacy_inline(bind_callable_unused_self_with(
        &db,
        &env,
        &mut ordinary,
        Type::bool_literal(true),
        &InlineConstructorSignatureEffects,
    ));
    assert_eq!(format!("{actual:#?}"), format!("{ordinary:#?}"));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
