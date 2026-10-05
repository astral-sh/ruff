//! Constructed signatures isolate receiver installation from source inference and later matching.

use std::cell::Cell;

use super::*;
use crate::types::function::source::FunctionSignatureEffects;
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance};

/// Borrows a caller-owned signature through the production controlled installation adapter.
struct Install<'a, 'db> {
    signature: &'a mut Signature<'db>,
    receiver: Type<'db>,
}

impl<'db> MemberOperation<'db> for Install<'_, 'db> {
    type Output = ();

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<()>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let env = ProgramEnvironment::from_program(program);
        FunctionSignatureEffects::apply_implicit_receiver(
            &effects,
            access.db(),
            &env,
            self.signature,
            self.receiver,
        )
        .await
    }
}

/// Constructs an eager variable without inferring a source definition or bound.
fn variable<'db>(
    db: &'db TestDb,
    program: Program<'db>,
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
        BindingContext::Synthetic(program),
        None,
        TypeVarNonce::NONE,
    )
}

/// Selects absent context, insertion before old variables, or retention of an existing Self identity.
#[derive(Clone, Copy, Debug)]
enum ContextCase {
    Absent,
    Prepend,
    ExistingIdentity,
}

/// Installation preserves variable order and retains the old context when its Self identity already
/// exists. It leaves the original signature's shared parameter buffer unchanged. A differently bound
/// Self occurrence tests underlying-identity deduplication.
#[test_case::test_case(ContextCase::Absent, false; "instance no context")]
#[test_case::test_case(ContextCase::Prepend, false; "instance Self first")]
#[test_case::test_case(ContextCase::ExistingIdentity, false; "instance existing Self")]
#[test_case::test_case(ContextCase::Absent, true; "class no context")]
#[test_case::test_case(ContextCase::Prepend, true; "class Self first")]
#[test_case::test_case(ContextCase::ExistingIdentity, true; "class existing Self")]
fn constructed_context_order_and_identity(case: ContextCase, class_receiver: bool) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let env = ProgramEnvironment::from_program(program);
    let keys = seed(&prepared, "target");
    let receiver_variable = variable(&db, program, "Self", TypeVarKind::TypingSelf);
    let previous_self = BoundTypeVarInstance::new(
        &db,
        receiver_variable.typevar(&db),
        BindingContext::Definition(keys.method),
        None,
        TypeVarNonce::NONE.increment(),
    );
    let first = variable(&db, program, "T", TypeVarKind::LegacyTypeVar);
    let last = variable(&db, program, "U", TypeVarKind::LegacyTypeVar);
    let context = match case {
        ContextCase::Absent => None,
        ContextCase::Prepend => Some(GenericContext::from_typevar_instances(
            &db,
            &env,
            [first, last],
        )),
        ContextCase::ExistingIdentity => Some(GenericContext::from_typevar_instances(
            &db,
            &env,
            [first, previous_self, last],
        )),
    };
    let receiver = if class_receiver {
        SubclassOfType::from(&db, &env, receiver_variable)
    } else {
        Type::TypeVar(receiver_variable)
    };
    let input = Signature::new_generic(
        context,
        Parameters::standard([
            Parameter::positional_or_keyword(Name::new_static("receiver"))
                .with_definition(Some(keys.method)),
            Parameter::positional_or_keyword(Name::new_static("value"))
                .with_annotated_type(Type::bool_literal(false))
                .with_default_type(Type::bool_literal(true)),
        ]),
        Type::bool_literal(false),
    )
    .with_definition(Some(keys.method))
    .with_source_overload_index(Some(3));
    let unchanged = format!("{input:#?}");
    let mut actual = input.clone();
    let revision = salsa::plumbing::current_revision(&db);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Install {
                signature: &mut actual,
                receiver
            },
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(())),
    );
    let actual_context = actual.generic_context.expect("receiver context");
    let expected = match case {
        ContextCase::Absent => vec![receiver_variable],
        ContextCase::Prepend => vec![receiver_variable, first, last],
        ContextCase::ExistingIdentity => {
            assert_eq!(Some(actual_context), context);
            assert_ne!(previous_self, receiver_variable);
            vec![first, previous_self, last]
        }
    };
    assert_eq!(actual_context.variables(&db).collect::<Vec<_>>(), expected);
    assert_eq!(actual.parameters()[0].annotated_type(), receiver);
    assert_eq!(actual.parameters()[1], input.parameters()[1]);
    assert_eq!(actual.return_ty, input.return_ty);
    assert_eq!(actual.definition, input.definition);
    assert_eq!(format!("{input:#?}"), unchanged);
    let mut ordinary = input.clone();
    ordinary.add_implicit_self_annotation(&db, &env, || Some(receiver));
    assert_eq!(actual, ordinary);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Selects each independent receiver-eligibility condition and the successful case.
#[derive(Clone, Copy, Debug)]
enum Eligibility {
    Empty,
    KeywordOnly,
    ExplicitUnknown,
    ExplicitConcrete,
    InferredConcrete,
    Eligible,
}

/// Constructs only parameter metadata, keeping each eligibility condition independently observable.
fn parameters<'db>(case: Eligibility) -> Parameters<'db> {
    let positional = Parameter::positional_or_keyword(Name::new_static("self"));
    match case {
        Eligibility::Empty => Parameters::empty(),
        Eligibility::KeywordOnly => {
            Parameters::standard([Parameter::keyword_only(Name::new_static("self"))])
        }
        Eligibility::ExplicitUnknown => {
            Parameters::standard([positional.with_annotated_type(Type::unknown())])
        }
        Eligibility::ExplicitConcrete => {
            Parameters::standard([positional.with_annotated_type(Type::bool_literal(false))])
        }
        Eligibility::InferredConcrete => {
            Parameters::standard([positional.with_inferred_type(Type::bool_literal(false))])
        }
        Eligibility::Eligible => Parameters::standard([positional]),
    }
}

/// The ordinary FnOnce receiver callback is skipped for ineligible parameters and called once for
/// eligible input; the controlled adapter preserves the same signature and concrete receiver result.
#[test_case::test_case(Eligibility::Empty; "no parameter")]
#[test_case::test_case(Eligibility::KeywordOnly; "not positional")]
#[test_case::test_case(Eligibility::ExplicitUnknown; "explicit Unknown")]
#[test_case::test_case(Eligibility::ExplicitConcrete; "explicit concrete")]
#[test_case::test_case(Eligibility::InferredConcrete; "inferred concrete")]
#[test_case::test_case(Eligibility::Eligible; "eligible")]
fn receiver_callback_is_lazy_and_once(case: Eligibility) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let input = Signature::new(parameters(case), Type::bool_literal(false));
    let receiver = Type::bool_literal(true);
    let calls = Cell::new(0);
    let token = String::from("once-only receiver");
    let mut ordinary = input.clone();
    ordinary.add_implicit_self_annotation(&db, &env, || {
        drop(token);
        calls.set(calls.get() + 1);
        Some(receiver)
    });
    let mut actual = input.clone();
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Install {
                signature: &mut actual,
                receiver
            },
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(())),
    );
    let eligible = matches!(case, Eligibility::Eligible);
    assert_eq!(calls.get(), usize::from(eligible));
    assert_eq!(actual, ordinary);
    if eligible {
        assert_eq!(actual.parameters()[0].annotated_type(), receiver);
    } else {
        assert_eq!(actual, input);
    }
    assert_eq!(actual.generic_context, None);
    assert_eq!(actual.return_ty, input.return_ty);
    assert_no_active_attempt();
}

/// A once-called ordinary receiver callback returning None leaves the entire signature unchanged.
#[test]
fn absent_receiver_leaves_signature_unchanged() {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut signature =
        Signature::new(parameters(Eligibility::Eligible), Type::bool_literal(false));
    let original = signature.clone();
    let calls = Cell::new(0);
    signature.add_implicit_self_annotation(&db, &env, || {
        calls.set(calls.get() + 1);
        None
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(signature, original);
}
