//! Constructed contexts isolate identity collection, canonical reconstruction and local buffer refusal.

use super::*;
use crate::types::class::identity::ClassIdentityEffects;
use crate::types::generics::identity::observations::{Event, OperationLifetime, Recording};
use crate::types::typevar::{ParamSpecAttrKind, TypeVarIdentity, TypeVarInstance};
use crate::types::{GenericAlias, Specialization, StaticClassLiteral};

/// Runs the production identity collection while observing its enclosing operation's retirement.
#[derive(Clone, Copy, Debug)]
struct ContextRequest<'db>(GenericContext<'db>);

impl<'db> MemberOperation<'db> for ContextRequest<'db> {
    type Output = Specialization<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let _lifetime = OperationLifetime::new(access.db());
        SourceEffects::new(access, program)
            .identity_specialization(self.0)
            .await
    }
}

/// Uses the same class-identity hook as Self, with explicitly constructed origin and context metadata.
#[derive(Clone, Copy, Debug)]
struct AliasRequest<'db> {
    origin: StaticClassLiteral<'db>,
    context: GenericContext<'db>,
}

impl<'db> MemberOperation<'db> for AliasRequest<'db> {
    type Output = ClassType<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        ClassIdentityEffects::identity_alias(
            &SourceEffects::new(access, program),
            self.origin,
            self.context,
        )
        .await
    }
}

/// Builds canonical metadata without inferring definitions, bounds, defaults or an identity result.
/// Mixed input includes every variable kind, fresh/bound occurrences, and both ParamSpec attributes.
fn constructed_context<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    empty: bool,
) -> (GenericContext<'db>, Vec<BoundTypeVarInstance<'db>>) {
    let program = prepared.program_file().program(db);
    let binding = BindingContext::Synthetic(program);
    let mut variables = Vec::new();
    if !empty {
        for (name, kind) in [
            ("T", TypeVarKind::LegacyTypeVar),
            ("U", TypeVarKind::Pep695TypeVar),
            ("P", TypeVarKind::LegacyParamSpec),
            ("Q", TypeVarKind::Pep695ParamSpec),
            ("Ts", TypeVarKind::LegacyTypeVarTuple),
            ("Us", TypeVarKind::Pep695TypeVarTuple),
            ("Self", TypeVarKind::TypingSelf),
        ] {
            let variable = TypeVarInstance::new(
                db,
                TypeVarIdentity::new(db, Name::new_static(name), None, kind),
                None,
                Some(TypeVarVariance::Invariant),
                None,
            );
            variables.push(BoundTypeVarInstance::new(
                db,
                variable,
                binding,
                None,
                TypeVarNonce::NONE,
            ));
        }
        let first = variables[0];
        variables.push(BoundTypeVarInstance::new(
            db,
            first.typevar(db),
            binding,
            None,
            TypeVarNonce::NONE.increment(),
        ));
        variables.push(BoundTypeVarInstance::new(
            db,
            first.typevar(db),
            BindingContext::Definition(seed(prepared, "target").method),
            None,
            TypeVarNonce::NONE,
        ));
        let paramspec = variables[2].typevar(db);
        for attribute in [ParamSpecAttrKind::Args, ParamSpecAttrKind::Kwargs] {
            variables.push(BoundTypeVarInstance::new(
                db,
                paramspec,
                binding,
                Some(attribute),
                TypeVarNonce::NONE,
            ));
        }
    }
    let context = GenericContext::from_typevar_instances(
        db,
        &ProgramEnvironment::from_program(program),
        variables.iter().copied(),
    );
    assert_eq!(context.variables(db).collect::<Vec<_>>(), variables);
    (context, variables)
}

/// Checks the complete argument order and all specialization fields before ordinary parity.
fn assert_identity<'db>(
    db: &'db TestDb,
    context: GenericContext<'db>,
    variables: &[BoundTypeVarInstance<'db>],
    actual: Specialization<'db>,
) {
    let expected = variables
        .iter()
        .copied()
        .map(Type::TypeVar)
        .collect::<Vec<_>>();
    assert_eq!(actual.generic_context(db), context);
    assert_eq!(actual.types(db), expected.as_slice());
    assert_eq!(actual.materialization_kind(db), None);
    assert_eq!(
        actual,
        Specialization::new(db, context, expected.as_slice(), None, None)
    );
    assert_eq!(actual, context.identity_specialization(db));
}

/// Empty and mixed constructed contexts retain exact bound occurrences and canonical None fields.
/// This isolates collection semantics; the separate cold Self producer controls cover source inference.
#[test_case::test_case(true; "empty context")]
#[test_case::test_case(false; "all occurrence metadata")]
fn constructed_context_retains_exact_identity(empty: bool) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let (context, variables) = constructed_context(&db, &prepared, empty);
    let revision = salsa::plumbing::current_revision(&db);
    let result = controlled_member_operation(&prepared, ContextRequest(context), &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = result else {
        panic!("constructed identity: {result:?}");
    };
    assert_identity(&db, context, &variables, actual);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// The real identity-alias hook retains its supplied class and context, including a present empty context.
/// Both inputs are constructed canonical metadata; this does not test the class-context query.
#[test_case::test_case(true; "present empty context")]
#[test_case::test_case(false; "mixed context")]
fn constructed_alias_preserves_origin(empty: bool) {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let (context, variables) = constructed_context(&db, &prepared, empty);
    let origin = StaticClassLiteral::new(
        &db,
        Name::new_static("Product"),
        seed(&prepared, "target").class_scope,
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        true,
        false,
        false,
    );
    let result =
        controlled_member_operation(&prepared, AliasRequest { origin, context }, &funded());
    let Ok(AnalysisOutcome::Complete(ClassType::Generic(alias))) = result else {
        panic!("constructed alias: {result:?}");
    };
    assert_eq!(alias.origin(&db), origin);
    assert_identity(&db, context, &variables, alias.specialization(&db));
    assert_eq!(
        alias,
        GenericAlias::new(&db, origin, context.identity_specialization(&db))
    );
    assert_no_active_attempt();
}

/// Selects a partially filled buffer or its final ownership transfer to boxing.
#[derive(Clone, Copy, Debug)]
enum Boundary {
    SecondAppend,
    Box,
}

impl Boundary {
    fn before(self, events: &[Event]) -> bool {
        events.iter().any(|event| match (self, event) {
            (Self::SecondAppend, Event::BeforeAppend(storage)) => storage.len == 1,
            (Self::Box, Event::BeforeBox(_)) => true,
            _ => false,
        })
    }

    fn after(self, events: &[Event]) -> bool {
        events.iter().any(|event| match (self, event) {
            (Self::SecondAppend, Event::AfterAppend(storage)) => storage.len == 2,
            (Self::Box, Event::BoxTransferred) => true,
            _ => false,
        })
    }
}

/// Measures whether the selected append or boxing operation completes under one numeric policy.
/// Each sample uses a separate database and checks the boundary's after observation.
fn reaches(boundary: Boundary, policy: &AnalysisPolicy) -> bool {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let (context, _) = constructed_context(&db, &prepared, false);
    let recording = Recording::start(&db);
    let _result = controlled_member_operation(&prepared, ContextRequest(context), policy);
    let events = recording.events();
    drop(recording);
    assert_no_active_attempt();
    boundary.after(&events)
}

/// Each independent ceiling refuses before append/boxing, drains the local buffer before its caller,
/// and permits an exact-context retry in the same revision. Inputs are constructed; no query-parent
/// publication or source-cold completion is inferred from this local collection harness.
#[test_case::test_case(Boundary::SecondAppend, Resource::Work; "partial buffer work")]
#[test_case::test_case(Boundary::SecondAppend, Resource::Bytes; "partial buffer bytes")]
#[test_case::test_case(Boundary::Box, Resource::Work; "box transfer work")]
#[test_case::test_case(Boundary::Box, Resource::Bytes; "box transfer bytes")]
fn refused_collection_drains_before_caller_and_retries(boundary: Boundary, resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches(boundary, &resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches(boundary, &resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let (context, variables) = constructed_context(&db, &prepared, false);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start(&db);
    let result =
        controlled_member_operation(&prepared, ContextRequest(context), &resource.policy(low));
    let events = recording.events();
    drop(recording);
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("identity resource refusal: {result:?}");
    };
    assert_eq!(reason, resource.reason());
    assert!(boundary.before(&events), "{events:?}");
    assert!(!boundary.after(&events), "{events:?}");
    let buffer = events
        .iter()
        .position(|event| *event == Event::ArgumentsRetired { transferred: false })
        .expect("local buffer retirement");
    let caller = events
        .iter()
        .position(|event| *event == Event::OperationRetired)
        .expect("enclosing operation retirement");
    assert!(buffer < caller, "{events:?}");
    assert_no_active_attempt();
    assert_eq!(context.variables(&db).collect::<Vec<_>>(), variables);
    let recording = Recording::start(&db);
    let retry = controlled_member_operation(&prepared, ContextRequest(context), &funded());
    let events = recording.events();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(actual)) = retry else {
        panic!("identity retry: {retry:?}");
    };
    assert!(boundary.after(&events), "{events:?}");
    assert_identity(&db, context, &variables, actual);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
