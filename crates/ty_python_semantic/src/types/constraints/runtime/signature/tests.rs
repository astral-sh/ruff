use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::task::Poll;

use salsa::execution_probe::{
    Demand, ExecutionAdmission, ExecutionWork, RegistryBuilder, RunError, RunResult, TaskEndpoint,
};
use salsa::prepared_source_probe::Stamp;

use super::super::tests::{Admission, CursorLifetime, StructuralBoundary, assert_same};
use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::types::callable::{CallableType, CallableTypeKind, CallableTypes};
use crate::types::constraints::typevar_equivalence::tests::{State, assert_consistent, variable};
use crate::types::constraints::{
    ConstraintFold, ConstraintSet, ConstraintSetBuilder, NodeId, SourceOrderId,
};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::GenericContext;
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::signatures::effects::{
    ConstraintBound, LegacyInlineEffects, SignatureEffect, SignatureEffects, try_poll_immediate,
};
use crate::types::signatures::{
    CallableSignature, ConcatenateTail, Parameter, Parameters, Signature, SignatureRelationVisitor,
};
use crate::types::typevar::TypeVarKind;
use crate::types::{ApplyTypeMappingVisitor, BoundTypeVarInstance, Type};

#[derive(Default)]
pub(super) struct Observations<'run, 'db: 'run> {
    pub(super) current_site: Cell<Option<SignatureSite>>,
    pub(super) occurrence: Cell<usize>,
    pub(super) accepted_pushes: Cell<usize>,
    pub(super) observer:
        Option<&'run dyn Fn(&TaskEndpoint<'run, 'db>, SignatureBoundary<'db>) -> RunResult<()>>,
    pub(super) structural_observer:
        Option<&'run dyn Fn(&TaskEndpoint<'run, 'db>, StructuralBoundary) -> RunResult<()>>,
    pub(super) structural_lifetime: Option<&'run CursorLifetime>,
    pub(super) equivalence_lifetime: Option<&'run CursorLifetime>,
    pub(super) push_lifetime: Option<&'run PushLifetime>,
}

type Prefix = (NodeId, Option<SourceOrderId>, u8);

#[derive(Default)]
pub(super) struct PushLifetime {
    live: Cell<usize>,
    drops: Cell<usize>,
    builder: Cell<*const ()>,
    fold: Cell<*const ()>,
    accumulator_len: Cell<usize>,
    first: Cell<Option<Prefix>>,
    last_len: Cell<usize>,
    last_first: Cell<Option<Prefix>>,
}

pub(super) struct PushBorrowGuard<'obs, 'fold, 'db, 'c> {
    fold: &'fold mut ConstraintFold<'db, 'c>,
    lifetime: Option<&'obs PushLifetime>,
}

impl<'obs, 'fold, 'db, 'c> PushBorrowGuard<'obs, 'fold, 'db, 'c> {
    pub(super) fn new(
        fold: &'fold mut ConstraintFold<'db, 'c>,
        lifetime: Option<&'obs PushLifetime>,
    ) -> Self {
        if let Some(lifetime) = lifetime {
            lifetime.live.set(lifetime.live.get() + 1);
            lifetime
                .builder
                .set(std::ptr::from_ref(fold.builder).cast());
            lifetime.fold.set(std::ptr::from_ref(&*fold).cast());
            lifetime.accumulator_len.set(fold.accumulator.len());
            lifetime.first.set(fold.accumulator.first().copied());
        }
        Self { fold, lifetime }
    }
}

impl<'db, 'c> Deref for PushBorrowGuard<'_, '_, 'db, 'c> {
    type Target = ConstraintFold<'db, 'c>;

    fn deref(&self) -> &Self::Target {
        self.fold
    }
}

impl DerefMut for PushBorrowGuard<'_, '_, '_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.fold
    }
}

impl Drop for PushBorrowGuard<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if let Some(lifetime) = self.lifetime {
            lifetime.last_len.set(self.fold.accumulator.len());
            lifetime
                .last_first
                .set(self.fold.accumulator.first().copied());
            lifetime.live.set(lifetime.live.get() - 1);
            lifetime.drops.set(lifetime.drops.get() + 1);
        }
    }
}

macro_rules! with_checker {
    ($db:expr, $builder:expr, $checker:ident, $body:block) => {{
        let env = $db.program_environment();
        let relations = HasRelationToVisitor::default($builder);
        let disjoint = IsDisjointVisitor::default($builder);
        let signatures = SignatureRelationVisitor::default();
        let mapping = ApplyTypeMappingVisitor::new(&env);
        let $checker = TypeRelationChecker::constraint_set_assignability(
            &env,
            $builder,
            &relations,
            &disjoint,
            &signatures,
            &mapping,
        );
        $body
    }};
}

struct Operands<'db> {
    variables: [BoundTypeVarInstance<'db>; 8],
    values: [CallableType<'db>; 8],
}

impl<'db> Operands<'db> {
    fn new(db: &'db TestDb) -> Self {
        let variables = std::array::from_fn(|index| {
            variable(db, &format!("P{index}"), TypeVarKind::Pep695ParamSpec)
        });
        let values = variables.map(|variable| Self::value(db, variable));
        Self { variables, values }
    }

    fn value(db: &'db TestDb, variable: BoundTypeVarInstance<'db>) -> CallableType<'db> {
        CallableType::paramspec_value_from_signatures(
            db,
            CallableSignature::single(Signature::new(
                Parameters::paramspec(db, variable),
                Type::unknown(),
            )),
        )
    }

    fn sources(&self, count: usize) -> CallableTypes<'db> {
        CallableTypes::from_elements(self.values[..count].iter().copied())
    }

    fn target(&self) -> CallableType<'db> {
        self.values[7]
    }
}

fn ordinary<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    sources: &CallableTypes<'db>,
    target: CallableType<'db>,
) -> ConstraintSet<'db, 'c> {
    with_checker!(db, builder, checker, {
        match try_poll_immediate(checker.check_callables_vs_callable_with(
            db,
            &LegacyInlineEffects,
            sources,
            target,
        )) {
            Poll::Ready(Ok(result)) => result,
            Poll::Ready(Err(never)) => match never {},
            Poll::Pending => panic!("ordinary finite comparison must complete in its first poll"),
        }
    })
}

fn assert_storage(actual: &ConstraintSetBuilder<'_>, expected: &ConstraintSetBuilder<'_>) {
    let actual = actual.storage.borrow();
    let expected = expected.storage.borrow();
    assert_eq!(State::capture(&actual), State::capture(&expected));
    assert_eq!(actual.typevar_cache, expected.typevar_cache);
    assert_eq!(actual.constraint_cache, expected.constraint_cache);
    assert_eq!(actual.node_cache, expected.node_cache);
    assert_eq!(actual.source_order_cache, expected.source_order_cache);
    assert_eq!(
        actual.constraint_bound_depth_cache,
        expected.constraint_bound_depth_cache
    );
    assert_eq!(actual.never_satisfied_cache, expected.never_satisfied_cache);
    assert_eq!(actual.negate_cache, expected.negate_cache);
    assert_eq!(actual.or_cache, expected.or_cache);
    assert_eq!(actual.and_cache, expected.and_cache);
    assert_eq!(actual.exists_cache, expected.exists_cache);
}

fn assert_original_empty(builder: &ConstraintSetBuilder<'_>) {
    let storage = builder.storage.borrow();
    assert!(storage.compacted.is_none());
    assert!(storage.typevars.is_empty());
    assert!(storage.constraints.is_empty());
    assert!(storage.nodes.is_empty());
    assert!(storage.source_orders.is_empty());
}

fn assert_no_query_execution(reader: &mut TestDb) {
    let events = reader.take_salsa_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, salsa::EventKind::WillCheckCancellation))
    );
    assert!(
        events
            .iter()
            .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
}

fn assert_idle(db: &TestDb, builder: &ConstraintSetBuilder<'_>) {
    assert!(!expansion_probe::active());
    assert!(!salsa::attempt_probe::is_incomplete(db));
    assert!(builder.storage.try_borrow_mut().is_ok());
    assert_consistent(db, &builder.storage.borrow());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Admitted {
    site: Option<SignatureSite>,
    occurrence: usize,
    work: ExecutionWork,
    accepted_pushes: usize,
}

struct Ledger<'a, 'run, 'db: 'run> {
    admission: Admission<'a, 'db>,
    observations: &'a Observations<'run, 'db>,
    events: RefCell<Vec<Admitted>>,
    semantic: Cell<usize>,
    refuse: Option<usize>,
}

impl<'a, 'run, 'db: 'run> Ledger<'a, 'run, 'db> {
    fn new(
        db: &'db TestDb,
        builder: &'a ConstraintSetBuilder<'db>,
        observations: &'a Observations<'run, 'db>,
    ) -> Self {
        Self {
            admission: Admission::new(db, builder),
            observations,
            events: RefCell::default(),
            semantic: Cell::new(0),
            refuse: None,
        }
    }
}

impl ExecutionAdmission for Ledger<'_, '_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let site = self.observations.current_site.get();
        self.events.borrow_mut().push(Admitted {
            site,
            occurrence: self.observations.occurrence.get(),
            work,
            accepted_pushes: self.observations.accepted_pushes.get(),
        });
        self.admission.admit(work)?;
        if site.is_some()
            && matches!(
                work,
                ExecutionWork::Work { .. } | ExecutionWork::Resource { .. }
            )
        {
            let index = self.semantic.get();
            self.semantic.set(index + 1);
            if self.refuse == Some(index) {
                return Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Allowance,
                ));
            }
        }
        Ok(())
    }
}

fn evaluate<'run, 'db: 'run, 'c: 'run>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    sources: &'run CallableTypes<'db>,
    target: CallableType<'db>,
    admission: &'run dyn ExecutionAdmission,
    observations: &'run Observations<'run, 'db>,
) -> (
    Result<RunResult<ConstraintSet<'db, 'c>>, Incomplete>,
    Option<RunError>,
) {
    let mut error = None;
    let result = expansion_probe::run(db, usize::MAX, || {
        let result = (|| {
            RegistryBuilder::new(db, admission)?
                .seal()?
                .run(move |endpoint| async move {
                    with_checker!(db, builder, checker, {
                        compare_registered(
                            db,
                            endpoint,
                            &checker,
                            sources,
                            target,
                            Some(observations),
                        )
                        .await
                    })
                })
        })();
        error = result.as_ref().err().copied();
        result
    })
    .0;
    (result, error)
}

fn semantic_events(events: &[Admitted]) -> Vec<Admitted> {
    events
        .iter()
        .copied()
        .filter(|event| {
            event.site.is_some()
                && matches!(
                    event.work,
                    ExecutionWork::Work { .. } | ExecutionWork::Resource { .. }
                )
        })
        .collect()
}

fn assert_single_task(events: &[Admitted]) {
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.work, ExecutionWork::Task { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.work == ExecutionWork::Poll)
            .count(),
        1
    );
    let poll = events
        .iter()
        .position(|event| event.work == ExecutionWork::Poll)
        .unwrap();
    assert!(events[poll + 1..].iter().all(|event| event.site.is_some()));
}

#[test]
fn finite_paramspec_consumer_matches_ordinary_builder_and_sources() {
    let db = setup_db();
    let operands = Operands::new(&db);
    let sources = operands.sources(7);
    let ordinary_builder = ConstraintSetBuilder::new();
    let expected = ordinary(&db, &ordinary_builder, &sources, operands.target());
    let builder = ConstraintSetBuilder::new();
    assert_original_empty(&builder);
    let mut reader = db.clone();
    let requests = RefCell::new(Vec::new());
    let structural = RefCell::new(Vec::new());
    let pair_lifetime = CursorLifetime::default();
    let structural_lifetime = CursorLifetime::default();
    let push_lifetime = PushLifetime::default();
    let observer = |_: &TaskEndpoint<'_, '_>, event| {
        assert!(builder.storage.try_borrow_mut().is_ok());
        requests.borrow_mut().push(event);
        Ok(())
    };
    let structural_observer = |_: &TaskEndpoint<'_, '_>, event| {
        assert!(builder.storage.try_borrow_mut().is_ok());
        structural.borrow_mut().push(event);
        Ok(())
    };
    let observations = Observations {
        observer: Some(&observer),
        structural_observer: Some(&structural_observer),
        equivalence_lifetime: Some(&pair_lifetime),
        structural_lifetime: Some(&structural_lifetime),
        push_lifetime: Some(&push_lifetime),
        ..Observations::default()
    };
    let ledger = Ledger::new(&db, &builder, &observations);
    reader.take_salsa_events();
    let (actual, error) = evaluate(
        &db,
        &builder,
        &sources,
        operands.target(),
        &ledger,
        &observations,
    );
    let actual = actual.unwrap().unwrap();
    assert_eq!(error, None);
    assert_eq!(
        (actual.node, actual.source_order),
        (expected.node, expected.source_order)
    );
    assert_storage(&builder, &ordinary_builder);
    assert_single_task(&ledger.events.borrow());
    assert_no_query_execution(&mut reader);
    assert_eq!(observations.accepted_pushes.get(), 7);
    assert_eq!(pair_lifetime.live.get(), 0);
    assert_eq!(pair_lifetime.drops_started.get(), 7);
    assert_eq!(push_lifetime.live.get(), 0);
    assert_eq!(push_lifetime.drops.get(), 7);
    let requests = requests.borrow();
    let original_checker = requests
        .iter()
        .find_map(|event| match event {
            SignatureBoundary::Request {
                site: SignatureSite::EntryMode,
                checker,
                ..
            } => *checker,
            _ => None,
        })
        .unwrap();
    let sites: Vec<_> = requests
        .iter()
        .filter_map(|event| match event {
            SignatureBoundary::Request {
                site,
                builder: observed,
                checker,
                ..
            } => {
                assert_eq!(*observed, std::ptr::from_ref(&builder).cast());
                if let Some(checker) = checker {
                    assert_eq!(*checker, original_checker);
                }
                Some(*site)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        &sites[..9],
        &[
            SignatureSite::EntryMode,
            SignatureSite::TargetCapability,
            SignatureSite::SourceCapability(0),
            SignatureSite::SourceCapability(1),
            SignatureSite::SourceCapability(2),
            SignatureSite::SourceCapability(3),
            SignatureSite::SourceCapability(4),
            SignatureSite::SourceCapability(5),
            SignatureSite::SourceCapability(6)
        ]
    );
    for (site, expected) in [
        (SignatureSite::Equivalence, 7),
        (SignatureSite::Combine, 14),
        (SignatureSite::TrivialPredicate, 7),
        (SignatureSite::Push, 7),
        (SignatureSite::Finish, 1),
        (SignatureSite::ExpandParameters, 14),
        (SignatureSite::NormalizeParameters, 7),
    ] {
        assert_eq!(
            sites.iter().filter(|actual| **actual == site).count(),
            expected,
            "{site:?}"
        );
    }
    let pairs: Vec<_> = requests
        .iter()
        .filter_map(|event| match event {
            SignatureBoundary::Request {
                site: SignatureSite::Equivalence,
                operands,
                ..
            } => Some(*operands),
            _ => None,
        })
        .collect();
    for (index, pair) in pairs.iter().enumerate() {
        assert_eq!(
            *pair,
            [
                Some(Type::TypeVar(operands.variables[index])),
                Some(Type::TypeVar(operands.variables[7]))
            ]
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|event| matches!(
                event,
                SignatureBoundary::AfterEquivalenceAdvanceReturned {
                    outer_complete: true
                }
            ))
            .count(),
        7
    );
    assert!(requests.iter().any(|event| matches!(
        event,
        SignatureBoundary::AfterEquivalenceAdvanceReturned {
            outer_complete: false
        }
    )));
    assert_eq!(
        requests
            .iter()
            .filter(|event| matches!(
                event,
                SignatureBoundary::BeforeEquivalenceRetirementAcceptance
            ))
            .count(),
        7
    );
    drop(requests);
    let admissions = ledger.events.borrow();
    let mut equivalent_prefixes: Vec<_> = admissions
        .iter()
        .filter(|event| event.site == Some(SignatureSite::Equivalence))
        .map(|event| event.accepted_pushes)
        .collect();
    equivalent_prefixes.dedup();
    assert_eq!(equivalent_prefixes, (0..7).collect::<Vec<_>>());
    let mut previous_occurrence = None;
    for event in admissions.iter() {
        if previous_occurrence == Some(event.occurrence) {
            continue;
        }
        previous_occurrence = Some(event.occurrence);
        let fixed = match event.site {
            Some(SignatureSite::EntryMode) => SignatureWork::EntryMode,
            Some(SignatureSite::TargetCapability | SignatureSite::SourceCapability(_)) => {
                SignatureWork::CallableShape
            }
            Some(SignatureSite::ExpandParameters) => SignatureWork::ExpandParameters,
            Some(SignatureSite::NormalizeParameters) => SignatureWork::NormalizeParameters,
            Some(SignatureSite::Equivalence) => SignatureWork::ConstraintBound,
            Some(SignatureSite::TrivialPredicate) => SignatureWork::TrivialPredicate,
            Some(SignatureSite::AliasIdentity) => SignatureWork::AliasIdentity,
            Some(SignatureSite::Unsupported(_)) => SignatureWork::Unsupported,
            _ => continue,
        };
        assert_eq!(
            event.work,
            ExecutionWork::Work {
                units: fixed.units()
            }
        );
    }
    assert!(structural.borrow().iter().any(|event| matches!(
        event,
        StructuralBoundary::AfterAdvanceReturned {
            operation: super::super::tests::StructuralOperation::Push,
            outer_complete: false
        }
    )));
    assert!(structural.borrow().iter().any(|event| matches!(
        event,
        StructuralBoundary::AfterAdvanceReturned {
            operation: super::super::tests::StructuralOperation::Finish,
            outer_complete: false
        }
    )));
    drop(admissions);
    assert_same(actual, ordinary(&db, &builder, &sources, operands.target()));
    let before = State::capture(&builder.storage.borrow());
    with_checker!(&db, &builder, checker, {
        let admission = Admission::new(&db, &builder);
        let warm = expansion_probe::run(&db, usize::MAX, || {
            super::run(&db, &admission, &checker, &sources, operands.target(), None)
        })
        .0
        .unwrap()
        .unwrap();
        assert_same(actual, warm);
    });
    assert_eq!(State::capture(&builder.storage.borrow()), before);
    assert_idle(&db, &builder);
}

fn completed<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    sources: &CallableTypes<'db>,
    target: CallableType<'db>,
) -> ConstraintSet<'db, 'c> {
    with_checker!(db, builder, checker, {
        let admission = Admission::new(db, builder);
        expansion_probe::run(db, usize::MAX, || {
            super::run(db, &admission, &checker, sources, target, None)
        })
        .0
        .unwrap()
        .unwrap()
    })
}

#[test]
fn finite_paramspec_identity_order_and_high_ids_match_ordinary() {
    let db = setup_db();
    let operands = Operands::new(&db);
    for order in [vec![0], (0..7).collect(), (0..7).rev().collect()] {
        let sources =
            CallableTypes::from_elements(order.iter().map(|index| operands.values[*index]));
        let expected_builder = ConstraintSetBuilder::new();
        let builder = ConstraintSetBuilder::new();
        let target = if order.len() == 1 {
            operands.values[0]
        } else {
            operands.target()
        };
        let expected = ordinary(&db, &expected_builder, &sources, target);
        let actual = completed(&db, &builder, &sources, target);
        assert_eq!(
            (actual.node, actual.source_order),
            (expected.node, expected.source_order)
        );
        assert_storage(&builder, &expected_builder);
        assert_same(actual, completed(&db, &builder, &sources, target));
        if order.len() == 1 {
            assert!(actual.is_trivially_always_satisfied());
            let storage = builder.storage.borrow();
            assert_eq!(storage.typevars.raw, [operands.variables[0]]);
            assert!(
                storage.constraints.is_empty()
                    && storage.nodes.is_empty()
                    && storage.source_orders.is_empty()
            );
        }
    }
    let (builder, expected_builder) = (ConstraintSetBuilder::new(), ConstraintSetBuilder::new());
    for (left, right) in [(0, 1), (1, 0), (0, 1)] {
        let sources = CallableTypes::one(operands.values[left]);
        let expected = ordinary(&db, &expected_builder, &sources, operands.values[right]);
        let actual = completed(&db, &builder, &sources, operands.values[right]);
        assert_eq!(
            (actual.node, actual.source_order),
            (expected.node, expected.source_order)
        );
        assert_storage(&builder, &expected_builder);
    }
    let (builder, expected_builder) = (ConstraintSetBuilder::new(), ConstraintSetBuilder::new());
    for index in 0..65 * usize::BITS as usize + 1 {
        let variable = variable(
            &db,
            &format!("prefix-{index}"),
            TypeVarKind::Pep695ParamSpec,
        );
        builder.storage.borrow_mut().intern_typevar(&db, variable);
        expected_builder
            .storage
            .borrow_mut()
            .intern_typevar(&db, variable);
    }
    let sources = operands.sources(3);
    assert!(builder.storage.borrow().constraints.is_empty());
    let expected = ordinary(&db, &expected_builder, &sources, operands.target());
    let actual = completed(&db, &builder, &sources, operands.target());
    assert_eq!(
        (actual.node, actual.source_order),
        (expected.node, expected.source_order)
    );
    assert_storage(&builder, &expected_builder);
    assert!(
        builder
            .storage
            .borrow()
            .supports
            .iter()
            .any(|support| support.words().len() > 64)
    );
    assert_idle(&db, &builder);
}

fn unsupported(effect: SignatureEffect) -> Incomplete {
    Incomplete::UnsupportedSignatureOperation(effect)
}

fn rejected_entry<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
    sources: &CallableTypes<'db>,
    target: CallableType<'db>,
    mode: bool,
    expected: SignatureEffect,
) -> Vec<SignatureSite> {
    let before = State::capture(&builder.storage.borrow());
    let sites = RefCell::new(Vec::new());
    let observer = |_: &TaskEndpoint<'_, '_>, event| {
        if let SignatureBoundary::Request { site, .. } = event {
            sites.borrow_mut().push(site);
        }
        Ok(())
    };
    let observations = Observations {
        observer: Some(&observer),
        ..Observations::default()
    };
    let admission = Admission::new(db, builder);
    let mut raw = None;
    let outcome = with_checker!(db, builder, checker, {
        let mut checker = checker;
        if mode {
            checker.typevar_evaluation = TypeVarEvaluation::Eager;
        }
        expansion_probe::run(db, usize::MAX, || {
            let result = super::run(
                db,
                &admission,
                &checker,
                sources,
                target,
                Some(&observations),
            );
            raw = result.as_ref().err().copied();
            result
        })
        .0
    });
    assert_eq!(outcome.err(), Some(unsupported(expected)));
    assert_eq!(
        raw,
        Some(RunError::Refused(
            salsa::attempt_probe::Incomplete::Interrupted
        ))
    );
    assert_eq!(State::capture(&builder.storage.borrow()), before);
    assert!(sites.borrow().iter().all(|site| matches!(
        site,
        SignatureSite::EntryMode
            | SignatureSite::TargetCapability
            | SignatureSite::SourceCapability(_)
    )));
    assert!(!expansion_probe::active());
    sites.into_inner()
}

#[test]
fn finite_signature_entry_rejects_unsupported_before_comparison() {
    let db = setup_db();
    let operands = Operands::new(&db);
    let env = db.program_environment();
    let regular = CallableType::new(
        &db,
        CallableSignature::single(Signature::new(
            Parameters::paramspec(&db, operands.variables[0]),
            Type::unknown(),
        )),
        CallableTypeKind::Regular,
    );
    let empty = CallableType::paramspec_value_from_signatures(
        &db,
        CallableSignature::single(Signature::new(Parameters::empty(), Type::unknown())),
    );
    let prefix = CallableType::paramspec_value_from_signatures(
        &db,
        CallableSignature::single(Signature::new(
            Parameters::concatenate(
                &db,
                vec![Parameter::positional_only(None).with_annotated_type(Type::unknown())],
                ConcatenateTail::ParamSpec(operands.variables[0]),
            ),
            Type::unknown(),
        )),
    );
    let overloaded = CallableType::paramspec_value_from_signatures(
        &db,
        CallableSignature::from_overloads([
            Signature::new(
                Parameters::paramspec(&db, operands.variables[0]),
                Type::unknown(),
            ),
            Signature::new(
                Parameters::paramspec(&db, operands.variables[1]),
                Type::unknown(),
            ),
        ]),
    );
    let generic = CallableType::paramspec_value_from_signatures(
        &db,
        CallableSignature::single(Signature::new_generic(
            Some(GenericContext::from_typevar_instances(
                &db,
                &env,
                [operands.variables[0]],
            )),
            Parameters::paramspec(&db, operands.variables[0]),
            Type::unknown(),
        )),
    );
    let receiver = ConstraintSetBuilder::new()
        .into_owned(|builder| ordinary(&db, builder, &operands.sources(1), operands.target()));
    let receiver = CallableType::paramspec_value_from_signatures(
        &db,
        CallableSignature::single(
            Signature::new(
                Parameters::paramspec(&db, operands.variables[0]),
                Type::unknown(),
            )
            .with_probe_receiver_constraints(receiver),
        ),
    );
    let mut reader = db.clone();
    for invalid in [regular, empty, prefix, overloaded, generic, receiver] {
        let builder = ConstraintSetBuilder::new();
        let sources = CallableTypes::from_elements([
            operands.values[1],
            operands.values[2],
            invalid,
            operands.values[3],
        ]);
        reader.take_salsa_events();
        let sites = rejected_entry(
            &db,
            &builder,
            &sources,
            operands.target(),
            false,
            SignatureEffect::EntryShape,
        );
        assert_eq!(
            sites,
            [
                SignatureSite::EntryMode,
                SignatureSite::TargetCapability,
                SignatureSite::SourceCapability(0),
                SignatureSite::SourceCapability(1),
                SignatureSite::SourceCapability(2)
            ]
        );
        assert_no_query_execution(&mut reader);
        assert_eq!(
            rejected_entry(
                &db,
                &builder,
                &operands.sources(3),
                invalid,
                false,
                SignatureEffect::EntryShape
            ),
            [SignatureSite::EntryMode, SignatureSite::TargetCapability]
        );
    }
    let builder = ConstraintSetBuilder::new();
    assert_eq!(
        rejected_entry(
            &db,
            &builder,
            &operands.sources(3),
            operands.target(),
            true,
            SignatureEffect::CheckerMode
        ),
        [SignatureSite::EntryMode]
    );
    let owned = ConstraintSetBuilder::new()
        .into_owned(|builder| ordinary(&db, builder, &operands.sources(1), operands.target()));
    owned.query(|builder, _| {
        assert!(builder.storage.borrow().compacted.is_some());
        assert!(builder.storage.borrow().typevar_cache.is_empty());
        assert_eq!(
            rejected_entry(
                &db,
                builder,
                &operands.sources(3),
                operands.target(),
                false,
                SignatureEffect::CompactedBuilder
            ),
            [SignatureSite::EntryMode]
        );
        assert!(builder.storage.borrow().typevar_cache.is_empty());
        assert!(builder.storage.borrow().constraint_cache.is_empty());
    });
}

#[derive(Clone, Copy, Debug)]
enum UnsupportedOperation {
    Relate,
    Never,
    Always,
    Concrete,
    Lower,
    Upper,
    Alias,
    Expand,
    Normalize,
}

impl UnsupportedOperation {
    fn effect(self) -> SignatureEffect {
        match self {
            Self::Relate => SignatureEffect::Relation,
            Self::Never | Self::Always => SignatureEffect::ConstraintSatisfiability,
            Self::Concrete | Self::Lower | Self::Upper => SignatureEffect::ConstraintConstruction,
            Self::Alias => SignatureEffect::AliasResolution,
            Self::Expand => SignatureEffect::ParameterExpansion,
            Self::Normalize => SignatureEffect::VariadicNormalization,
        }
    }
}

#[test]
fn finite_signature_effects_reject_unsupported_without_fallback() {
    let db = setup_db();
    let operands = Operands::new(&db);
    let sources = operands.sources(1);
    let malformed = Parameters::empty();
    let valid = Parameters::paramspec(&db, operands.variables[0]);
    let mut reader = db.clone();
    for operation in [
        UnsupportedOperation::Relate,
        UnsupportedOperation::Never,
        UnsupportedOperation::Always,
        UnsupportedOperation::Concrete,
        UnsupportedOperation::Lower,
        UnsupportedOperation::Upper,
        UnsupportedOperation::Alias,
        UnsupportedOperation::Expand,
        UnsupportedOperation::Normalize,
    ] {
        let builder = ConstraintSetBuilder::new();
        let nonterminal = ordinary(&db, &builder, &sources, operands.target());
        assert!(!nonterminal.node.is_terminal());
        let before = State::capture(&builder.storage.borrow());
        let admission = Admission::new(&db, &builder);
        let after = Cell::new(false);
        let mut raw = None;
        reader.take_salsa_events();
        let outcome = expansion_probe::run(&db, usize::MAX, || {
            let (db, builder, operands, sources, malformed, valid, after) = (
                &db, &builder, &operands, &sources, &malformed, &valid, &after,
            );
            let result: RunResult<()> =
                RegistryBuilder::new(db, &admission)?
                    .seal()?
                    .run(|endpoint| async move {
                        with_checker!(db, builder, checker, {
                            let provider = admit_provider(
                                db,
                                endpoint,
                                &checker,
                                sources,
                                operands.target(),
                                None,
                            )
                            .await?;
                            match operation {
                                UnsupportedOperation::Relate => {
                                    provider
                                        .relate(db, &checker, Type::unknown(), Type::unknown())
                                        .await?;
                                }
                                UnsupportedOperation::Never => {
                                    provider.is_never(db, &checker, nonterminal).await?;
                                }
                                UnsupportedOperation::Always => {
                                    provider.is_always(db, &checker, nonterminal).await?;
                                }
                                UnsupportedOperation::Concrete => {
                                    provider
                                        .constraint_bound(
                                            db,
                                            &checker,
                                            ConstraintBound::Equivalent,
                                            operands.variables[0],
                                            Type::unknown(),
                                        )
                                        .await?;
                                }
                                UnsupportedOperation::Lower | UnsupportedOperation::Upper => {
                                    let kind = if matches!(operation, UnsupportedOperation::Lower) {
                                        ConstraintBound::Lower
                                    } else {
                                        ConstraintBound::Upper
                                    };
                                    provider
                                        .constraint_bound(
                                            db,
                                            &checker,
                                            kind,
                                            operands.variables[0],
                                            Type::TypeVar(operands.variables[7]),
                                        )
                                        .await?;
                                }
                                UnsupportedOperation::Alias => {
                                    provider
                                        .resolve_alias(db, &checker, Type::unknown())
                                        .await?;
                                }
                                UnsupportedOperation::Expand => {
                                    provider.expand_parameters(db, &checker, malformed).await?;
                                }
                                UnsupportedOperation::Normalize => {
                                    provider
                                        .normalize_variadic_parameters(
                                            db,
                                            &checker,
                                            malformed.clone(),
                                            valid.clone(),
                                        )
                                        .await?;
                                }
                            }
                            after.set(true);
                            Ok(())
                        })
                    });
            raw = result.as_ref().err().copied();
            result
        })
        .0;
        assert_eq!(
            outcome,
            Err(unsupported(operation.effect())),
            "{operation:?}"
        );
        assert_eq!(
            raw,
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Interrupted
            ))
        );
        assert!(!after.get());
        assert_eq!(State::capture(&builder.storage.borrow()), before);
        assert_no_query_execution(&mut reader);
        assert_idle(&db, &builder);
    }
    let builder = ConstraintSetBuilder::new();
    let admission = Admission::new(&db, &builder);
    let result = expansion_probe::run(&db, usize::MAX, || {
        let (db, builder, operands, sources) = (&db, &builder, &operands, &sources);
        RegistryBuilder::new(db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                with_checker!(db, builder, checker, {
                    let provider =
                        admit_provider(db, endpoint, &checker, sources, operands.target(), None)
                            .await?;
                    for value in [false, true] {
                        let set = ConstraintSet::from_bool(builder, value);
                        assert_eq!(provider.is_always(db, &checker, set).await?, value);
                        assert_eq!(provider.is_never(db, &checker, set).await?, !value);
                    }
                    let variable = Type::TypeVar(operands.variables[0]);
                    assert_eq!(
                        provider.resolve_alias(db, &checker, variable).await?,
                        variable
                    );
                    Ok(())
                })
            })
    })
    .0;
    assert_eq!(result, Ok(Ok(())));
    assert_original_empty(&builder);
}

#[test]
fn every_finite_signature_admission_can_refuse_and_retry() {
    let db = setup_db();
    let operands = Operands::new(&db);
    let sources = operands.sources(3);
    let expected_builder = ConstraintSetBuilder::new();
    let expected = ordinary(&db, &expected_builder, &sources, operands.target());
    let baseline_builder = ConstraintSetBuilder::new();
    let observations = Observations::default();
    let ledger = Ledger::new(&db, &baseline_builder, &observations);
    let actual = evaluate(
        &db,
        &baseline_builder,
        &sources,
        operands.target(),
        &ledger,
        &observations,
    )
    .0
    .unwrap()
    .unwrap();
    assert_eq!(
        (actual.node, actual.source_order),
        (expected.node, expected.source_order)
    );
    let baseline = semantic_events(&ledger.events.borrow());
    for site in [
        SignatureSite::EntryMode,
        SignatureSite::TargetCapability,
        SignatureSite::SourceCapability(2),
        SignatureSite::ExpandParameters,
        SignatureSite::NormalizeParameters,
        SignatureSite::Equivalence,
        SignatureSite::TrivialPredicate,
        SignatureSite::Combine,
        SignatureSite::Push,
        SignatureSite::Finish,
    ] {
        assert!(
            baseline.iter().any(|event| event.site == Some(site)),
            "unreached {site:?}"
        );
    }
    assert!(
        baseline
            .iter()
            .any(|event| matches!(event.work, ExecutionWork::Resource { .. }))
    );
    let stamp = Stamp::current(&db);
    for refuse in 0..baseline.len() {
        let builder = ConstraintSetBuilder::new();
        let observations = Observations::default();
        let mut ledger = Ledger::new(&db, &builder, &observations);
        ledger.refuse = Some(refuse);
        let (outcome, raw) = evaluate(
            &db,
            &builder,
            &sources,
            operands.target(),
            &ledger,
            &observations,
        );
        assert_eq!(
            outcome.err(),
            Some(Incomplete::Allowance),
            "admission {refuse}"
        );
        assert_eq!(
            raw,
            Some(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance
            ))
        );
        assert_eq!(
            semantic_events(&ledger.events.borrow()),
            baseline[..=refuse],
            "admission {refuse}"
        );
        assert_idle(&db, &builder);
        assert_eq!(Stamp::current(&db), stamp);
        let retry = completed(&db, &builder, &sources, operands.target());
        assert_eq!(
            (retry.node, retry.source_order),
            (expected.node, expected.source_order)
        );
        assert_storage(&builder, &expected_builder);
        assert_same(retry, ordinary(&db, &builder, &sources, operands.target()));
        assert_idle(&db, &builder);
        assert_eq!(Stamp::current(&db), stamp);
    }
}

#[derive(Default)]
struct ChildObservation {
    drops: Cell<usize>,
    factory_ran: Cell<bool>,
    saw_fold: Cell<usize>,
    saw_cursor: Cell<usize>,
    saw_equivalence: Cell<usize>,
    saw_storage: Cell<bool>,
    prefix: Cell<Option<Prefix>>,
    pair_requests: Cell<usize>,
    finish_requests: Cell<usize>,
}

struct PendingConsumerChild {
    builder: &'static ConstraintSetBuilder<'static>,
    push: &'static PushLifetime,
    cursor: &'static CursorLifetime,
    equivalence: &'static CursorLifetime,
    observed: &'static ChildObservation,
}

impl Drop for PendingConsumerChild {
    fn drop(&mut self) {
        self.observed.drops.set(self.observed.drops.get() + 1);
        self.observed.saw_fold.set(self.push.live.get());
        self.observed.saw_cursor.set(self.cursor.live.get());
        self.observed
            .saw_equivalence
            .set(self.equivalence.live.get());
        self.observed
            .saw_storage
            .set(self.builder.storage.try_borrow_mut().is_ok());
        self.observed.prefix.set(self.push.first.get());
    }
}

struct ChildAdmission {
    ledger: Ledger<'static, 'static, 'static>,
    endpoint: &'static RefCell<Option<TaskEndpoint<'static, 'static>>>,
    demand: &'static RefCell<Option<Demand<()>>>,
    push: &'static PushLifetime,
    cursor: &'static CursorLifetime,
    equivalence: &'static CursorLifetime,
    observed: &'static ChildObservation,
    fired: Cell<bool>,
}

impl ExecutionAdmission for ChildAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.ledger.admit(work)?;
        if !self.fired.get()
            && self.ledger.observations.current_site.get() == Some(SignatureSite::Push)
            && self.ledger.observations.accepted_pushes.get() == 1
            && matches!(work, ExecutionWork::Work { .. })
            && self
                .ledger
                .admission
                .builder
                .storage
                .try_borrow_mut()
                .is_err()
        {
            self.fired.set(true);
            let child = PendingConsumerChild {
                builder: self.ledger.admission.builder,
                push: self.push,
                cursor: self.cursor,
                equivalence: self.equivalence,
                observed: self.observed,
            };
            let endpoint = self.endpoint.borrow().clone().unwrap();
            let observed = self.observed;
            let demand = endpoint.demand(move || {
                observed.factory_ran.set(true);
                async move {
                    drop(child);
                    Ok(())
                }
            })?;
            *self.demand.borrow_mut() = Some(demand);
            return Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            ));
        }
        Ok(())
    }
}

struct ClearConsumerSlots {
    endpoint: &'static RefCell<Option<TaskEndpoint<'static, 'static>>>,
    demand: &'static RefCell<Option<Demand<()>>>,
}

impl Drop for ClearConsumerSlots {
    fn drop(&mut self) {
        self.demand.borrow_mut().take();
        self.endpoint.borrow_mut().take();
    }
}

#[test]
fn finite_consumer_child_drops_before_the_real_fold_and_push_cursor() {
    let db: &'static TestDb = Box::leak(Box::new(setup_db()));
    let builder: &'static ConstraintSetBuilder<'static> =
        Box::leak(Box::new(ConstraintSetBuilder::new()));
    let operands = Operands::new(db);
    let sources: &'static CallableTypes<'static> = Box::leak(Box::new(operands.sources(3)));
    let target = operands.target();
    let push: &'static PushLifetime = Box::leak(Box::default());
    let cursor: &'static CursorLifetime = Box::leak(Box::default());
    let equivalence: &'static CursorLifetime = Box::leak(Box::default());
    let observed: &'static ChildObservation = Box::leak(Box::default());
    let endpoint: &'static RefCell<Option<TaskEndpoint<'static, 'static>>> =
        Box::leak(Box::default());
    let demand: &'static RefCell<Option<Demand<()>>> = Box::leak(Box::default());
    let clear = ClearConsumerSlots { endpoint, demand };
    let observer = Box::leak(Box::new(
        move |current: &TaskEndpoint<'static, 'static>, boundary| {
            *endpoint.borrow_mut() = Some(current.clone());
            match boundary {
                SignatureBoundary::Request {
                    site: SignatureSite::Equivalence,
                    ..
                } => observed.pair_requests.set(observed.pair_requests.get() + 1),
                SignatureBoundary::Request {
                    site: SignatureSite::Finish,
                    ..
                } => observed
                    .finish_requests
                    .set(observed.finish_requests.get() + 1),
                _ => {}
            }
            Ok(())
        },
    ));
    let observations: &'static Observations<'static, 'static> = Box::leak(Box::new(Observations {
        observer: Some(observer),
        structural_lifetime: Some(cursor),
        equivalence_lifetime: Some(equivalence),
        push_lifetime: Some(push),
        ..Observations::default()
    }));
    let admission: &'static ChildAdmission = Box::leak(Box::new(ChildAdmission {
        ledger: Ledger::new(db, builder, observations),
        endpoint,
        demand,
        push,
        cursor,
        equivalence,
        observed,
        fired: Cell::new(false),
    }));
    assert_original_empty(builder);
    let stamp = Stamp::current(db);
    let (outcome, raw) = evaluate(db, builder, sources, target, admission, observations);
    assert_eq!(outcome.err(), Some(Incomplete::Allowance));
    assert_eq!(
        raw,
        Some(RunError::Refused(
            salsa::attempt_probe::Incomplete::Allowance
        ))
    );
    assert!(admission.fired.get());
    assert_eq!(observed.drops.get(), 1);
    assert!(!observed.factory_ran.get());
    assert_eq!(observed.saw_fold.get(), 1);
    assert_eq!(observed.saw_cursor.get(), 1);
    assert_eq!(observed.saw_equivalence.get(), 0);
    assert!(observed.saw_storage.get());
    assert_eq!(push.builder.get(), std::ptr::from_ref(builder).cast());
    assert!(!push.fold.get().is_null());
    assert_eq!(push.accumulator_len.get(), 1);
    assert_eq!(observed.prefix.get(), push.last_first.get());
    assert_eq!(push.last_len.get(), 1);
    assert_eq!(push.live.get(), 0);
    assert_eq!(push.drops.get(), 2);
    assert_eq!(cursor.live.get(), 0);
    assert_eq!(equivalence.live.get(), 0);
    assert_eq!(equivalence.drops_started.get(), 2);
    assert_eq!(observed.pair_requests.get(), 2);
    assert_eq!(observed.finish_requests.get(), 0);
    assert_eq!(observations.accepted_pushes.get(), 1);
    let events = admission.ledger.events.borrow();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.work, ExecutionWork::Task { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.work == ExecutionWork::Poll)
            .count(),
        1
    );
    drop(events);
    drop(clear);
    assert!(endpoint.borrow().is_none() && demand.borrow().is_none());
    assert_idle(db, builder);
    assert_eq!(Stamp::current(db), stamp);
    let first = ordinary(db, builder, &CallableTypes::one(operands.values[0]), target);
    assert_eq!(
        observed.prefix.get(),
        Some((first.node, first.source_order, 0))
    );
    let retry = completed(db, builder, sources, target);
    assert_same(retry, ordinary(db, builder, sources, target));
    assert_idle(db, builder);
    assert_eq!(Stamp::current(db), stamp);
}
