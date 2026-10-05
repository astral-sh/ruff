//! Constructed specialization inputs expose context filtering and retained Self metadata.
//! Ordinary comparisons follow controlled execution; these inputs do not represent cold file inference.

use super::*;
use crate::types::mapping::OwnedTypeMapping;
use crate::types::function::{FunctionType, function_literal_signature_ingredient};
use crate::types::function::inherited_context::fixtures::{self, LiteralKind};
use crate::types::typevar::{ParamSpecAttrKind, TypeVarIdentity, TypeVarInstance};
use crate::types::callable::CallableTypeKind;
use crate::types::{CallableSignature, CallableType, Specialization};

/// Applies the production retained mapper to a constructed input without a canonical parent query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Mapping<'db> {
    input: Type<'db>,
    mapping: OwnedTypeMapping<'db, 'db>,
}

impl<'db> Mapping<'db> {
    const fn owned(self) -> OwnedTypeMapping<'db, 'db> {
        self.mapping
    }
}

impl<'db> MemberOperation<'db> for Mapping<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        SourceEffects::new(access, program)
            .apply_mapping(self.input, program, self.owned())
            .await
    }
}

/// Constructs a declaration with explicit domain/default metadata and a stable occurrence identity.
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
            TypeVarIdentity::new(db, Name::new_static(name), None, kind),
            bounds.map(TypeVarBoundOrConstraintsEvaluation::Eager),
            Some(TypeVarVariance::Covariant),
            default,
        ),
        BindingContext::Definition(definition),
        None,
        TypeVarNonce::NONE.increment(),
    )
}

/// Runs one funded mapping and compares the complete result with ordinary execution afterward.
fn complete<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    request: Mapping<'db>,
) -> Type<'db> {
    let revision = salsa::plumbing::current_revision(db);
    let result = controlled_member_operation(prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = result else {
        panic!("specialization did not complete: {result:?}");
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(
        actual,
        request.input.apply_type_mapping(db, &env, &request.owned().into_mapping(), TypeContext::default())
    );
    assert_eq!(salsa::plumbing::current_revision(db), revision);
    assert_no_active_attempt();
    actual
}

/// Selects whether surviving declarations themselves pass through fresh specialization roots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Declarations {
    Original,
    Specialized,
    Removed,
}

/// Context filtering removes concrete or differently identified replacements and keeps declarations
/// with absent or same-bound-identity replacements in stored order. `specialize_self_domain` selects
/// remapping each survivor with a fresh visitor instead of keeping its original handle. Removing
/// every declaration retains Some(empty).
#[test_case::test_case(Declarations::Original; "original surviving declarations")]
#[test_case::test_case(Declarations::Specialized; "fresh mapped surviving declarations")]
#[test_case::test_case(Declarations::Removed; "empty context remains present")]
fn context_filter_preserves_identity_order_and_empty(case: Declarations) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = variable(&db, definition, "A", TypeVarKind::LegacyTypeVar, None, None);
    let retained = variable(&db, definition, "B", TypeVarKind::LegacyTypeVar, None, None);
    let replaced = variable(&db, definition, "C", TypeVarKind::LegacyTypeVar, None, None);
    let last = variable(&db, definition, "D", TypeVarKind::LegacyTypeVar, None, None);
    let same_identity = variable(
        &db, definition, "B", TypeVarKind::LegacyTypeVar,
        Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(true))), None,
    );
    assert_ne!(same_identity, retained);
    assert_eq!(same_identity.identity(&db), retained.identity(&db));
    let original = GenericContext::from_typevar_instances(&db, &env, [first, retained, replaced, last]);
    let (arguments, replacements, expected) = match case {
        Declarations::Original => (
            vec![first, retained, replaced],
            vec![Type::bool_literal(true), Type::TypeVar(same_identity), Type::TypeVar(last)],
            vec![retained, last],
        ),
        Declarations::Specialized => (
            vec![first, retained, replaced],
            vec![Type::bool_literal(true), Type::TypeVar(same_identity), Type::TypeVar(last)],
            vec![same_identity, last],
        ),
        Declarations::Removed => (
            vec![first, retained, replaced, last],
            vec![Type::bool_literal(true); 4],
            vec![],
        ),
    };
    let context = GenericContext::from_typevar_instances(&db, &env, arguments);
    let specialization = Specialization::new(&db, context, replacements.as_slice(), None, None);
    let signature = Signature::new_generic(Some(original), Parameters::empty(), Type::bool_literal(false));
    let input = Type::Callable(CallableType::single(&db, signature));
    mapping_observations::reset(None);
    let actual = complete(&db, &prepared, Mapping {
        input,
        mapping: OwnedTypeMapping::Specialization {
            specialization,
            specialize_self_domain: case == Declarations::Specialized,
            materialization_kind: None,
        },
    });
    let Type::Callable(callable) = actual else { panic!("expected callable"); };
    let signature = callable.signatures(&db).iter().next().expect("single signature");
    let context = signature.generic_context.expect("retained context, including empty");
    assert_eq!(context.variables(&db).collect::<Vec<_>>(), expected);
    let roots = mapping_observations::mapping_snapshot();
    assert_eq!(roots.root_count, if case == Declarations::Specialized { 3 } else { 1 });
    if case == Declarations::Specialized {
        let outer = roots.roots[0].expect("outer visitor");
        let first = roots.roots[1].expect("first surviving declaration");
        let second = roots.roots[2].expect("second surviving declaration");
        assert_ne!(outer.visitor, first.visitor);
        assert_ne!(outer.visitor, second.visitor);
        assert_ne!(first.visitor, second.visitor);
        assert_eq!(first.mapping, second.mapping);
        assert_eq!(first.program, outer.program);
        assert_eq!(second.program, outer.program);
    }
}

/// Selects the identity distinction between a context declaration and the selected Single variable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingleIdentity {
    SameHandle,
    Metadata,
    Binding,
    Freshness,
    Args,
    Kwargs,
}

/// Raw Single lookup matches the complete bound identity, regardless of bounds/default metadata.
/// A different lexical binding, freshness nonce or ParamSpec attribute keeps the original declaration
/// in its original position. These contexts contain no ParamSpec annotations to expand.
#[test_case::test_case(SingleIdentity::SameHandle; "same selected occurrence")]
#[test_case::test_case(SingleIdentity::Metadata; "changed bounds and default")]
#[test_case::test_case(SingleIdentity::Binding; "different lexical binding")]
#[test_case::test_case(SingleIdentity::Freshness; "different freshness")]
#[test_case::test_case(SingleIdentity::Args; "different ParamSpec args attribute")]
#[test_case::test_case(SingleIdentity::Kwargs; "different ParamSpec kwargs attribute")]
fn single_context_lookup_uses_bound_identity(case: SingleIdentity) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let keys = seed(&prepared, "target");
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = variable(&db, keys.method, "A", TypeVarKind::LegacyTypeVar, None, None);
    let last = variable(&db, keys.method, "Z", TypeVarKind::LegacyTypeVar, None, None);
    let kind = match case {
        SingleIdentity::Args | SingleIdentity::Kwargs => TypeVarKind::LegacyParamSpec,
        SingleIdentity::SameHandle | SingleIdentity::Metadata | SingleIdentity::Binding
        | SingleIdentity::Freshness => TypeVarKind::LegacyTypeVar,
    };
    let selected = variable(&db, keys.method, "T", kind, None, None);
    let candidate = match case {
        SingleIdentity::SameHandle => selected,
        SingleIdentity::Metadata => variable(
            &db, keys.method, "T", kind,
            Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(false))),
            Some(TypeVarDefaultEvaluation::Eager(Type::int_literal(9))),
        ),
        SingleIdentity::Binding => BoundTypeVarInstance::new(
            &db, selected.typevar(&db), BindingContext::Definition(keys.class),
            None, selected.freshness(&db),
        ),
        SingleIdentity::Freshness => BoundTypeVarInstance::new(
            &db, selected.typevar(&db), selected.binding_context(&db),
            None, selected.freshness(&db).increment(),
        ),
        SingleIdentity::Args => selected.with_paramspec_attr(&db, ParamSpecAttrKind::Args),
        SingleIdentity::Kwargs => selected.with_paramspec_attr(&db, ParamSpecAttrKind::Kwargs),
    };
    let matches = match case {
        SingleIdentity::SameHandle | SingleIdentity::Metadata => true,
        SingleIdentity::Binding | SingleIdentity::Freshness | SingleIdentity::Args
        | SingleIdentity::Kwargs => false,
    };
    assert_eq!(candidate.identity(&db) == selected.identity(&db), matches);
    if case == SingleIdentity::Metadata {
        assert_ne!(candidate, selected);
    }
    let original = GenericContext::from_typevar_instances(&db, &env, [first, candidate, last]);
    let input = Type::Callable(CallableType::single(&db, Signature::new_generic(
        Some(original), Parameters::empty(), Type::bool_literal(false),
    )));
    mapping_observations::reset(None);
    let actual = complete(&db, &prepared, Mapping {
        input,
        mapping: OwnedTypeMapping::Single { variable: selected, replacement: Type::bool_literal(true) },
    });
    let Type::Callable(callable) = actual else { panic!("expected callable"); };
    let signature = callable.signatures(&db).iter().next().expect("single signature");
    let expected = if matches {
        GenericContext::from_typevar_instances(&db, &env, [first, last])
    } else {
        original
    };
    assert_eq!(signature.generic_context, Some(expected));
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1);
    assert_eq!(snapshot.roots[0].expect("Single root").mapping, OwnedMappingSnapshot::Single {
        variable: selected.as_id(),
    });
}

/// Selects how a matched Single replacement affects the signature's declarations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingleDeclaration {
    SameIdentity,
    DifferentIdentity,
    Concrete,
    Empty,
}

/// Equal-identity replacements retain the original declaration handle; different identities and
/// concrete replacements remove it without reordering survivors. Removing the only declaration
/// preserves the canonical empty context as Some(empty).
#[test_case::test_case(SingleDeclaration::SameIdentity; "original equal-identity declaration")]
#[test_case::test_case(SingleDeclaration::DifferentIdentity; "different replacement identity")]
#[test_case::test_case(SingleDeclaration::Concrete; "concrete replacement")]
#[test_case::test_case(SingleDeclaration::Empty; "canonical Some empty")]
fn single_context_preserves_declarations_and_empty(case: SingleDeclaration) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let first = variable(&db, definition, "A", TypeVarKind::LegacyTypeVar, None, None);
    let selected = variable(&db, definition, "T", TypeVarKind::LegacyTypeVar, None, None);
    let last = variable(&db, definition, "Z", TypeVarKind::LegacyTypeVar, None, None);
    let same_identity = variable(
        &db, definition, "T", TypeVarKind::LegacyTypeVar,
        Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(true))),
        Some(TypeVarDefaultEvaluation::Eager(Type::int_literal(9))),
    );
    assert_ne!(same_identity, selected);
    assert_eq!(same_identity.identity(&db), selected.identity(&db));
    let (original, replacement, expected) = match case {
        SingleDeclaration::SameIdentity => (
            vec![first, selected, last], Type::TypeVar(same_identity), vec![first, selected, last],
        ),
        SingleDeclaration::DifferentIdentity => (
            vec![first, selected, last], Type::TypeVar(last), vec![first, last],
        ),
        SingleDeclaration::Concrete => (
            vec![first, selected, last], Type::bool_literal(true), vec![first, last],
        ),
        SingleDeclaration::Empty => (vec![selected], Type::bool_literal(true), vec![]),
    };
    let original = GenericContext::from_typevar_instances(&db, &env, original);
    let input = Type::Callable(CallableType::single(&db, Signature::new_generic(
        Some(original), Parameters::empty(), Type::bool_literal(false),
    )));
    mapping_observations::reset(None);
    let actual = complete(&db, &prepared, Mapping {
        input, mapping: OwnedTypeMapping::Single { variable: selected, replacement },
    });
    let Type::Callable(callable) = actual else { panic!("expected callable"); };
    let signature = callable.signatures(&db).iter().next().expect("single signature");
    let expected = GenericContext::from_typevar_instances(&db, &env, expected);
    assert_eq!(signature.generic_context, Some(expected));
    assert_eq!(mapping_observations::mapping_snapshot().root_count, 1);
}

/// Substitution returns the replacement handle without recursively replacing the selected variable
/// inside it. The constructed callable deliberately contains that same variable in its return type.
#[test]
fn single_transfers_replacement_without_recursive_substitution() {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let selected = variable(&db, definition, "T", TypeVarKind::LegacyTypeVar, None, None);
    let replacement = Type::Callable(CallableType::single(&db, Signature::new(
        Parameters::empty(), Type::TypeVar(selected),
    )));
    mapping_observations::reset(None);
    let actual = complete(&db, &prepared, Mapping {
        input: Type::TypeVar(selected),
        mapping: OwnedTypeMapping::Single { variable: selected, replacement },
    });
    assert_eq!(actual, replacement);
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1);
    assert_eq!(snapshot.child_count, 0);
}

/// Mapping a ParamSpec annotation still refuses when it tries to strip the ParamSpec attribute.
/// Raw Single lookup in a generic context does not require that operation.
#[test_case::test_case(None; "Single ParamSpec")]
#[test_case::test_case(Some(ParamSpecAttrKind::Args); "Single ParamSpec args")]
#[test_case::test_case(Some(ParamSpecAttrKind::Kwargs); "Single ParamSpec kwargs")]
fn single_paramspec_annotations_keep_refusal(attribute: Option<ParamSpecAttrKind>) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let selected = variable(&db, definition, "P", TypeVarKind::LegacyParamSpec, None, None);
    let candidate = attribute.map(|attribute| selected.with_paramspec_attr(&db, attribute)).unwrap_or(selected);
    let result = controlled_member_operation(&prepared, Mapping {
        input: Type::TypeVar(candidate),
        mapping: OwnedTypeMapping::Single { variable: selected, replacement: Type::bool_literal(true) },
    }, &funded());
    assert_eq!(result, Ok(AnalysisOutcome::Incomplete {
        reason: AnalysisIncomplete::UnavailableOperation(OperationId::Specialization(
            MaterializationOperation::Leaf(MappingOperation::ParamSpec),
        )),
        completed: (),
    }));
    assert_no_active_attempt();
}

/// Selects absent bounds, an upper bound, or ordered constraints, and an unevaluated default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Domain {
    Absent,
    Upper,
    Constraints,
    LazyDefault,
}

/// Selects stored specialization or a handle-only Single substitution of the same variable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Substitution {
    Stored,
    Single,
}

/// Stored specialization reconstructs retained Self by changing only its domain. Single leaves an
/// unrelated Self's domain unchanged. Default descriptors, source identity,
/// variance, lexical binding and freshness survive. Present stored domains use one distinct visitor;
/// every constraint shares that visitor. Absent bounds and Single allocate no domain visitor.
#[test_case::test_case(Domain::Absent, Substitution::Stored; "absent bounds")]
#[test_case::test_case(Domain::Upper, Substitution::Stored; "upper bound")]
#[test_case::test_case(Domain::Constraints, Substitution::Stored; "ordered constraints")]
#[test_case::test_case(Domain::LazyDefault, Substitution::Stored; "stored lazy default remains unresolved")]
#[test_case::test_case(Domain::Absent, Substitution::Single; "Single absent bounds")]
#[test_case::test_case(Domain::Upper, Substitution::Single; "Single unchanged upper bound")]
#[test_case::test_case(Domain::Constraints, Substitution::Single; "Single unchanged ordered constraints")]
#[test_case::test_case(Domain::LazyDefault, Substitution::Single; "Single lazy default remains unresolved")]
fn retained_self_preserves_metadata_and_visitor_scope(domain: Domain, substitution: Substitution) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let argument = variable(&db, definition, "T", TypeVarKind::LegacyTypeVar, None, None);
    let context = GenericContext::from_typevar_instances(&db, &env, [argument]);
    let specialization = Specialization::new(&db, context, [Type::bool_literal(true)].as_slice(), None, None);
    let default = Some(match domain {
        Domain::LazyDefault => TypeVarDefaultEvaluation::Lazy,
        Domain::Absent | Domain::Upper | Domain::Constraints => TypeVarDefaultEvaluation::Eager(Type::TypeVar(argument)),
    });
    let (bounds, expected_bounds, constraint_count) = match domain {
        Domain::Absent => (None, None, 0),
        Domain::Upper | Domain::LazyDefault => (
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(argument))),
            Some(TypeVarBoundOrConstraints::UpperBound(Type::bool_literal(true))),
            0,
        ),
        Domain::Constraints => (
            Some(TypeVarBoundOrConstraints::Constraints(TypeVarConstraints::new(
                &db, Box::from([Type::TypeVar(argument), Type::int_literal(7), Type::TypeVar(argument)]),
            ))),
            Some(TypeVarBoundOrConstraints::Constraints(TypeVarConstraints::new(
                &db, Box::from([Type::bool_literal(true), Type::int_literal(7), Type::bool_literal(true)]),
            ))),
            3,
        ),
    };
    let original = variable(&db, definition, "Self", TypeVarKind::TypingSelf, bounds, default);
    let (mapping, expected) = match substitution {
        Substitution::Stored => (
            OwnedTypeMapping::Specialization {
                specialization, specialize_self_domain: true, materialization_kind: None,
            },
            variable(&db, definition, "Self", TypeVarKind::TypingSelf, expected_bounds, default),
        ),
        Substitution::Single => (
            OwnedTypeMapping::Single { variable: argument, replacement: Type::bool_literal(true) },
            original,
        ),
    };
    mapping_observations::reset(None);
    let actual = complete(&db, &prepared, Mapping {
        input: Type::TypeVar(original), mapping,
    });
    assert_eq!(actual, Type::TypeVar(expected));
    assert_eq!(original.identity(&db), expected.identity(&db));
    assert_eq!(original.freshness(&db), expected.freshness(&db));
    assert_eq!(expected.typevar(&db).explicit_variance(&db), Some(TypeVarVariance::Covariant));
    let snapshot = mapping_observations::mapping_snapshot();
    let maps_domain = substitution == Substitution::Stored && domain != Domain::Absent;
    assert_eq!(snapshot.root_count, if maps_domain { 2 } else { 1 });
    if maps_domain {
        let outer = snapshot.roots[0].expect("outer mapping root");
        let inner = snapshot.roots[1].expect("bound mapping root");
        assert_ne!(outer.visitor, inner.visitor);
        assert_eq!(outer.program, inner.program);
        assert_eq!(inner.mapping, OwnedMappingSnapshot::Specialization {
            specialization: specialization.as_id(), specialize_self_domain: false, materialization_kind: None,
        });
        let children = snapshot.children[..snapshot.child_count].iter().flatten()
            .filter(|child| child.visitor == inner.visitor).collect::<Vec<_>>();
        assert!(children.len() >= constraint_count.max(1));
        assert!(children.iter().all(|child| child.mapping == inner.mapping && child.default_context));
    }
}

/// Selects empty or multiple stored implementation callables.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Implementations {
    Empty,
    Multiple,
}

/// Specialization maps stored public and implementation signatures without changing their order,
/// callable kinds, literal or descriptor override. Defaults and source-overload metadata survive.
/// The separate-implementation flag is constructed metadata, not an overloaded source declaration.
#[test_case::test_case(Implementations::Empty, Substitution::Stored; "empty implementation storage")]
#[test_case::test_case(Implementations::Multiple, Substitution::Stored; "ordered implementation storage")]
#[test_case::test_case(Implementations::Empty, Substitution::Single; "Single empty implementation storage")]
#[test_case::test_case(Implementations::Multiple, Substitution::Single; "Single ordered implementation storage")]
fn stored_function_preserves_payload_metadata(implementations: Implementations, substitution: Substitution) {
    let db = database("class Product:\n    def target(self, value): ...\n");
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let function = crate::types::binding_type(&db, definition).as_function_literal().expect("fixture function");
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let variable = variable(&db, definition, "T", TypeVarKind::LegacyTypeVar, None, None);
    let context = GenericContext::from_typevar_instances(&db, &env, [variable]);
    let specialization = Specialization::new(&db, context, [Type::bool_literal(true)].as_slice(), None, None);
    let public = Signature::new(
        Parameters::standard([Parameter::positional_or_keyword(Name::new_static("value"))
            .with_annotated_type(Type::TypeVar(variable)).with_default_type(Type::int_literal(9))]),
        Type::TypeVar(variable),
    ).with_source_overload_index(Some(4));
    let first = CallableType::new(&db, CallableSignature::single(Signature::new(Parameters::empty(), Type::TypeVar(variable))), CallableTypeKind::StaticMethodLike);
    let second = CallableType::new(&db, CallableSignature::single(Signature::new(Parameters::empty(), Type::int_literal(2))), CallableTypeKind::ClassMethodLike);
    let stored = match implementations {
        Implementations::Empty => Box::default(),
        Implementations::Multiple => Box::from([first, second]),
    };
    let input = fixtures::function(&db, function.literal(&db).last_definition,
        LiteralKind::SeparateImplementation, CallableSignature::single(public), Some(stored),
        Some(CallableTypeKind::ClassMethodLike));
    let mapping = match substitution {
        Substitution::Stored => OwnedTypeMapping::Specialization {
            specialization, specialize_self_domain: false, materialization_kind: None,
        },
        Substitution::Single => OwnedTypeMapping::Single { variable, replacement: Type::bool_literal(true) },
    };
    let actual = complete(&db, &prepared, Mapping { input: Type::FunctionLiteral(input), mapping });
    let Type::FunctionLiteral(actual) = actual else { panic!("expected function"); };
    assert_eq!(actual.literal(&db), input.literal(&db));
    assert_eq!(actual.descriptor_kind(&db), Some(CallableTypeKind::ClassMethodLike));
    let expected_public = Signature::new(
        Parameters::standard([Parameter::positional_or_keyword(Name::new_static("value"))
            .with_annotated_type(Type::bool_literal(true)).with_default_type(Type::int_literal(9))]),
        Type::bool_literal(true),
    ).with_source_overload_index(Some(4));
    assert_eq!(actual.updated_signature(&db), Some(&CallableSignature::single(expected_public)));
    let expected_first = CallableType::new(&db, CallableSignature::single(Signature::new(Parameters::empty(), Type::bool_literal(true))), CallableTypeKind::StaticMethodLike);
    match implementations {
        Implementations::Empty => assert_eq!(actual.updated_implementation_callables(&db), Some([].as_slice())),
        Implementations::Multiple => assert_eq!(actual.updated_implementation_callables(&db), Some([expected_first, second].as_slice())),
    }
}

/// Records only the watched canonical signature request and its enclosing mapping operation.
#[derive(Clone, Debug, Default)]
struct Journal {
    function: Option<salsa::Id>,
    entries: usize,
    pending: usize,
    live_requests: usize,
    retired_requests: usize,
    owner_retired_with_live_requests: Option<usize>,
    returned: bool,
    cancel: Option<salsa::CancellationToken>,
}

thread_local! {
    static JOURNAL: RefCell<Journal> = RefCell::new(Journal::default());
}

/// Starts passive recording and removes any retained cancellation token when recording ends.
#[derive(Debug)]
struct Recording;

impl Recording {
    fn start(function: Option<FunctionType<'_>>, cancel: Option<salsa::CancellationToken>) -> Self {
        JOURNAL.with_borrow_mut(|journal| *journal = Journal {
            function: function.map(|function| function.as_id()), cancel, ..Journal::default()
        });
        mapping_observations::reset(None);
        Self
    }

    fn journal(&self) -> Journal {
        JOURNAL.with_borrow(Clone::clone)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| *journal = Journal::default());
    }
}

/// Records retirement after the awaited mapping future, including its partial results, is dropped.
#[derive(Debug)]
struct MappingLifetime;

impl Drop for MappingLifetime {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| {
            journal.owner_retired_with_live_requests = Some(journal.live_requests);
        });
    }
}

/// Records retirement after the real canonical reply demand has been dropped.
#[derive(Debug)]
struct RequestLifetime(bool);

impl Drop for RequestLifetime {
    fn drop(&mut self) {
        if self.0 {
            JOURNAL.with_borrow_mut(|journal| {
                journal.live_requests = journal.live_requests.checked_sub(1).expect("live observed request");
                journal.retired_requests += 1;
            });
        }
    }
}

/// Observes actual pending polls of the watched canonical signature reply without adding yields.
pub(in crate::types::infer) async fn observe_signature_request<F: Future>(function: salsa::Id, demand: F) -> F::Output {
    let watched = JOURNAL.with_borrow_mut(|journal| {
        let watched = journal.function == Some(function);
        if watched { journal.live_requests += 1; }
        watched
    });
    let _lifetime = RequestLifetime(watched);
    let mut demand = std::pin::pin!(demand);
    poll_fn(|context| {
        let result = demand.as_mut().poll(context);
        if watched && result.is_pending() {
            JOURNAL.with_borrow_mut(|journal| journal.pending += 1);
        }
        result
    }).await
}

/// Records entry into the watched canonical provider and optionally requests cancellation once
/// its caller is already pending. Provider entry and caller suspension are separate observations.
pub(in crate::types::infer) fn signature_entered(function: salsa::Id) {
    let cancel = JOURNAL.with_borrow_mut(|journal| {
        if journal.function != Some(function) { return None; }
        journal.entries += 1;
        if journal.pending > 0 && journal.live_requests > 0 {
            journal.cancel.take()
        } else {
            None
        }
    });
    if let Some(cancel) = cancel { cancel.cancel(); }
}

/// Records mapping completion and the count of live observed signature requests when the enclosing
/// mapping operation retires, retaining its input through actual child execution and drainage.
#[derive(Clone, Copy, Debug)]
struct ObservedMapping<'db>(Mapping<'db>);

impl<'db> MemberOperation<'db> for ObservedMapping<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self, access: &A, program: Program<'db>,
    ) -> RunResult<Self::Output>
    where 'db: 'run,
    {
        let _lifetime = MappingLifetime;
        let result = self.0.run(access, program).await?;
        JOURNAL.with_borrow_mut(|journal| journal.returned = true);
        Ok(result)
    }
}

/// Constructs retained-Self bounds or a Single-mapped signature with three annotation children.
/// With a function, stored specialization reaches it through Self constraints and Single maps it
/// directly. Its canonical signature remains unevaluated until the controlled mapping demands it.
fn interruption_input<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    substitution: Substitution,
    function: Option<FunctionType<'db>>,
) -> Mapping<'db> {
    let definition = seed(prepared, "target").method;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let argument = variable(db, definition, "T", TypeVarKind::LegacyTypeVar, None, None);
    let context = GenericContext::from_typevar_instances(db, &env, [argument]);
    if substitution == Substitution::Single {
        let input = function.map(Type::FunctionLiteral).unwrap_or_else(|| {
            Type::Callable(CallableType::single(db, Signature::new_generic(
                Some(context),
                Parameters::standard([
                    Parameter::positional_or_keyword(Name::new_static("first"))
                        .with_annotated_type(Type::TypeVar(argument)),
                    Parameter::positional_or_keyword(Name::new_static("second"))
                        .with_annotated_type(Type::TypeVar(argument)),
                ]),
                Type::TypeVar(argument),
            )))
        });
        return Mapping {
            input,
            mapping: OwnedTypeMapping::Single { variable: argument, replacement: Type::bool_literal(true) },
        };
    }
    let specialization = Specialization::new(db, context, [Type::bool_literal(true)].as_slice(), None, None);
    let constraints = TypeVarConstraints::new(db, Box::from([
        Type::TypeVar(argument), function.map(Type::FunctionLiteral).unwrap_or(Type::int_literal(7)), Type::TypeVar(argument),
    ]));
    let receiver = variable(db, definition, "Self", TypeVarKind::TypingSelf,
        Some(TypeVarBoundOrConstraints::Constraints(constraints)), None);
    Mapping {
        input: Type::TypeVar(receiver),
        mapping: OwnedTypeMapping::Specialization {
            specialization, specialize_self_domain: true, materialization_kind: None,
        },
    }
}

/// Runs one constructed mapping in a fresh database under the supplied policy and returns whether
/// it completed. Numeric-limit calibration uses this helper without retrying a cold source query.
fn completes_with(policy: &AnalysisPolicy, substitution: Substitution) -> bool {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let request = interruption_input(&db, &prepared, substitution, None);
    let recording = Recording::start(None, None);
    let result = controlled_member_operation(&prepared, ObservedMapping(request), policy);
    let returned = recording.journal().returned;
    assert_eq!(returned, matches!(result, Ok(AnalysisOutcome::Complete(_))));
    assert_no_active_attempt();
    returned
}

/// Independent work and byte refusal withholds the reconstructed Self or Single-mapped signature
/// after its annotation children run. The unchanged input then completes in the same revision. This direct mapper
/// has no canonical parent memo; its visible result is the returned Type handle.
#[test_case::test_case(Resource::Work, Substitution::Stored; "retained Self result work")]
#[test_case::test_case(Resource::Bytes, Substitution::Stored; "retained Self result bytes")]
#[test_case::test_case(Resource::Work, Substitution::Single; "Single signature result work")]
#[test_case::test_case(Resource::Bytes, Substitution::Single; "Single signature result bytes")]
fn numeric_refusal_drains_and_retries(resource: Resource, substitution: Substitution) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(completes_with(&resource.policy(high), substitution));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if completes_with(&resource.policy(middle), substitution) { high = middle; } else { low = middle; }
    }
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let request = interruption_input(&db, &prepared, substitution, None);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start(None, None);
    let refused = controlled_member_operation(&prepared, ObservedMapping(request), &resource.policy(low));
    let journal = recording.journal();
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(refused, Ok(AnalysisOutcome::Incomplete { reason: resource.reason(), completed: () }));
    assert!(!journal.returned);
    assert_eq!(journal.owner_retired_with_live_requests, Some(0));
    let annotation_visitor = match substitution {
        Substitution::Stored => {
            assert_eq!(snapshot.root_count, 2);
            snapshot.roots[1].expect("retained bounds visitor").visitor
        }
        Substitution::Single => {
            assert_eq!(snapshot.root_count, 1);
            snapshot.roots[0].expect("Single signature visitor").visitor
        }
    };
    assert!(snapshot.children[..snapshot.child_count].iter().flatten().filter(|child| child.visitor == annotation_visitor).count() >= 3);
    assert_eq!(request, interruption_input(&db, &prepared, substitution, None));
    drop(recording);
    let recording = Recording::start(None, None);
    let retried = controlled_member_operation(&prepared, ObservedMapping(request), &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = retried else { panic!("retry: {retried:?}"); };
    assert!(recording.journal().returned);
    assert_eq!(recording.journal().owner_retired_with_live_requests, Some(0));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(actual, request.input.apply_type_mapping(&db, &env, &request.owned().into_mapping(), TypeContext::default()));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Both mappings demand a previously unevaluated canonical function signature and
/// complete in one controlled attempt. Their captured reads contain the certified exact child key;
/// ordinary comparison occurs only after that completion.
#[test_case::test_case(Substitution::Stored; "retained Self canonical signature")]
#[test_case::test_case(Substitution::Single; "Single canonical signature")]
fn canonical_signature_child_completes_once(substitution: Substitution) {
    let db = database("class Product:\n    def target(self, value): ...\n");
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let function = crate::types::binding_type(&db, definition).as_function_literal().expect("fixture function identity");
    let request = interruption_input(&db, &prepared, substitution, Some(function));
    let ingredient = function_literal_signature_ingredient(&db);
    assert_eq!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, function.as_id()).map(|_| ()), Err(FinalSourceError::MissingMemo));
    let recording = Recording::start(Some(function), None);
    let captured = capture(&db, || controlled_member_operation(&prepared, ObservedMapping(request), &funded())).unwrap();
    captured.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(actual)) = captured.value else { panic!("canonical signature child: {:?}", captured.value); };
    let journal = recording.journal();
    assert_eq!(journal.entries, 1);
    assert!(journal.pending > 0);
    assert!(journal.retired_requests > 0);
    assert_eq!(journal.owner_retired_with_live_requests, Some(0));
    assert!(journal.returned);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, function.as_id()).is_ok());
    assert!(captured.reads.iter().any(|read| read.key == ingredient.database_key_index(function.as_id())));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(actual, request.input.apply_type_mapping(&db, &env, &request.owned().into_mapping(), TypeContext::default()));
    assert_no_active_attempt();
}

/// A canonical signature child suspends either the retained-bounds or Single function mapper. Cancellation drains the
/// request before its enclosing operation; exact-input retry preserves the revision and ordinary
/// result. A completed child memo may survive cancellation and is reused without provider entry.
#[test_case::test_case(Substitution::Stored; "retained Self canonical cancellation")]
#[test_case::test_case(Substitution::Single; "Single canonical cancellation")]
fn canonical_child_pending_cancellation_and_retry(substitution: Substitution) {
    let db = database("class Product:\n    def target(self, value): ...\n");
    let prepared = prepare(&db);
    let definition = seed(&prepared, "target").method;
    let function = crate::types::binding_type(&db, definition).as_function_literal().expect("fixture function identity");
    let request = interruption_input(&db, &prepared, substitution, Some(function));
    let revision = salsa::plumbing::current_revision(&db);
    let ingredient = function_literal_signature_ingredient(&db);
    assert_eq!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, function.as_id()).map(|_| ()), Err(FinalSourceError::MissingMemo));
    let recording = Recording::start(Some(function), Some(db.cancellation_token()));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, ObservedMapping(request), &funded())
    }));
    let journal = recording.journal();
    assert!(matches!(cancelled, Err(salsa::Cancelled::Local)), "{cancelled:?}");
    assert_eq!(journal.entries, 1, "{journal:?}");
    assert!(journal.pending > 0, "{journal:?}");
    assert!(journal.retired_requests > 0, "{journal:?}");
    assert_eq!(journal.live_requests, 0);
    assert_eq!(journal.owner_retired_with_live_requests, Some(0));
    assert!(!journal.returned);
    let root_count = match substitution {
        Substitution::Stored => 2,
        Substitution::Single => 1,
    };
    assert_eq!(mapping_observations::mapping_snapshot().root_count, root_count);
    assert_eq!(request, interruption_input(&db, &prepared, substitution, Some(function)));
    let completed_child = FinalSourceMemo::certify(&db as &dyn Db, ingredient, function.as_id()).is_ok();
    drop(recording);
    let recording = Recording::start(Some(function), None);
    let retry = capture(&db, || controlled_member_operation(&prepared, ObservedMapping(request), &funded())).unwrap();
    retry.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(actual)) = retry.value else { panic!("retry: {:?}", retry.value); };
    let journal = recording.journal();
    assert_eq!(journal.entries, usize::from(!completed_child));
    assert_eq!(journal.owner_retired_with_live_requests, Some(0));
    assert!(journal.returned);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, function.as_id()).is_ok());
    assert!(retry.reads.iter().any(|read| read.key == ingredient.database_key_index(function.as_id())));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(actual, request.input.apply_type_mapping(&db, &env, &request.owned().into_mapping(), TypeContext::default()));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
