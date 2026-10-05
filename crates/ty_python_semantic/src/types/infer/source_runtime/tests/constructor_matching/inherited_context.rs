//! Internal controls for stored function contexts and their real constructor-member adapter.
//!
//! Function identities, public signatures, and contexts are constructed before each attempt.
//! The absent-storage case leaves its real last-definition signature query unevaluated. These
//! controls do not establish cold constructor matching; the parent module retains those cases.

use ty_python_core::node_key::NodeKey;
use ty_python_core::scope::NodeWithScopeKey;

use super::*;
use crate::types::class::own_member::OwnMemberEffects;
use crate::types::function::inherited_context::fixtures::{self, LiteralKind};
use crate::types::function::inherited_context::observations::{
    self as context_observations, Stage,
};
use crate::types::function::{
    FunctionDecorators, FunctionLiteral, FunctionType, OverloadLiteral,
    function_last_definition_signature_ingredient,
};

const SOURCE: &str = "def source(value): ...\n";

/// Selects the constructed implementation-storage and public-signature scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Storage {
    Empty,
    Multiple,
    Absent,
    Ignored,
    EmptyPublic,
}

/// Retains canonical input handles across the real constructor-context member effect.
#[derive(Clone, Copy, Debug)]
struct ContextRequest<'db> {
    function: FunctionType<'db>,
    inherited: GenericContext<'db>,
}

impl<'db> MemberOperation<'db> for ContextRequest<'db> {
    type Output = FunctionType<'db>;

    /// Runs the same admitted transformation used by generic constructor member lookup.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        OwnMemberEffects::constructor_context(
            &SourceEffects::new(access, program),
            self.function,
            self.inherited,
        )
        .await
    }
}

/// Holds independently inspectable input metadata, including the exact optional storage state.
#[derive(Debug)]
struct Input<'db> {
    request: ContextRequest<'db>,
    literal: FunctionLiteral<'db>,
    public: CallableSignature<'db>,
    implementations: Option<Box<[CallableType<'db>]>>,
    variables: [BoundTypeVarInstance<'db>; 3],
}

/// Constructs an eager variable identity without inferring a declaration or annotation.
fn variable<'db>(
    db: &'db TestDb,
    program: Program<'db>,
    name: &'static str,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new_static(name), None, TypeVarKind::LegacyTypeVar),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(program),
        None,
        TypeVarNonce::NONE,
    )
}

/// Constructs stored canonical metadata while leaving the source implementation signature cold.
/// Shared variables occur in both contexts so the output must deduplicate identities in order.
fn input<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    storage: Storage,
) -> Input<'db> {
    let index = prepared.semantic_index();
    let source = prepared.parsed_module().syntax().body[0]
        .as_function_def_stmt()
        .expect("source implementation declaration");
    let definition = index.expect_single_definition(source);
    let scope = index
        .scope_id(index.node_scope_by_key(NodeWithScopeKey::Function(NodeKey::from_node(source))));
    let overload = OverloadLiteral::new(
        db,
        source.name.id.clone(),
        None,
        scope,
        FunctionDecorators::empty(),
        None,
        None,
        false,
    );
    let program = prepared.program_file().program(db);
    let env = ProgramEnvironment::from_program(program);
    let local = variable(db, program, "Local");
    let shared = variable(db, program, "Shared");
    let outer = variable(db, program, "Outer");
    let inherited = GenericContext::from_typevar_instances(db, &env, [shared, outer]);
    let constraints =
        ConstraintSetBuilder::new().into_owned(|builder| ConstraintSet::from_bool(builder, false));
    let first = Signature::new_generic(
        Some(GenericContext::from_typevar_instances(
            db,
            &env,
            [local, shared],
        )),
        Parameters::standard([
            Parameter::positional_only(Some(Name::new_static("value")))
                .with_annotated_type(Type::TypeVar(local))
                .with_default_type(Type::bool_literal(true))
                .with_definition(Some(definition)),
            Parameter::keyword_only(Name::new_static("flag"))
                .with_annotated_type(Type::bool_literal(false)),
        ]),
        Type::TypeVar(shared),
    )
    .with_definition(Some(definition))
    .with_source_overload_index(Some(4))
    .with_probe_receiver_constraints(constraints);
    let second = Signature::new_generic(
        Some(GenericContext::from_typevar_instances(
            db,
            &env,
            [outer, local],
        )),
        Parameters::gradual_form(),
        Type::unknown(),
    )
    .into_paramspec_value()
    .with_source_overload_index(Some(8));
    let third = Signature::recursion_recovery();
    let public = match storage {
        Storage::EmptyPublic => CallableSignature {
            overloads: Default::default(),
        },
        Storage::Empty | Storage::Multiple | Storage::Absent | Storage::Ignored => {
            CallableSignature {
                overloads: [first, second, third].into_iter().collect(),
            }
        }
    };
    let implementations = match storage {
        Storage::Empty => Some(Vec::new().into_boxed_slice()),
        Storage::Multiple | Storage::Ignored => Some(
            vec![
                CallableType::new(db, public.clone(), CallableTypeKind::ClassMethodLike)
                    .with_deprecated(db, overload),
                CallableType::new(
                    db,
                    CallableSignature::single(Signature::new(
                        Parameters::empty(),
                        Type::bool_literal(false),
                    )),
                    CallableTypeKind::StaticMethodLike,
                ),
            ]
            .into_boxed_slice(),
        ),
        Storage::Absent | Storage::EmptyPublic => None,
    };
    let kind = match storage {
        Storage::Empty | Storage::Multiple | Storage::Absent => LiteralKind::SeparateImplementation,
        Storage::Ignored | Storage::EmptyPublic => LiteralKind::Single,
    };
    let function = fixtures::function(
        db,
        overload,
        kind,
        public.clone(),
        implementations.clone(),
        Some(CallableTypeKind::FunctionLike),
    );
    Input {
        request: ContextRequest {
            function,
            inherited,
        },
        literal: function.literal(db),
        public,
        implementations,
        variables: [local, shared, outer],
    }
}

/// Builds the expected public signatures by changing only their ordered generic-context handles.
fn expected_public<'db>(db: &'db TestDb, input: &Input<'db>) -> CallableSignature<'db> {
    let mut expected = input.public.clone();
    let env = db.program_environment();
    let [local, shared, outer] = input.variables;
    let contexts = [
        GenericContext::from_typevar_instances(db, &env, [local, shared, outer]),
        GenericContext::from_typevar_instances(db, &env, [outer, local, shared]),
        input.request.inherited,
    ];
    for (signature, context) in expected.overloads.iter_mut().zip(contexts) {
        signature.generic_context = Some(context);
    }
    expected
}

/// Verifies literal, descriptor, and original stored owners remain intact after an attempt.
fn assert_original<'db>(db: &'db TestDb, input: &Input<'db>) {
    let function = input.request.function;
    assert_eq!(function.literal(db), input.literal);
    assert_eq!(
        function.descriptor_kind(db),
        Some(CallableTypeKind::FunctionLike)
    );
    assert_eq!(function.updated_signature(db), Some(&input.public));
    assert_eq!(
        function.updated_implementation_callables(db),
        input.implementations.as_deref()
    );
}

/// Checks full stored metadata and canonical identity, with ordinary execution only after the result.
fn assert_result<'db>(
    db: &'db TestDb,
    input: &Input<'db>,
    storage: Storage,
    result: FunctionType<'db>,
) {
    assert_original(db, input);
    assert_eq!(result.literal(db), input.literal);
    assert_eq!(
        result.descriptor_kind(db),
        Some(CallableTypeKind::FunctionLike)
    );
    let public = expected_public(db, input);
    assert_eq!(result.updated_signature(db), Some(&public));
    let expected_implementations = match storage {
        Storage::Empty => Some(Vec::new().into_boxed_slice()),
        Storage::Multiple => Some(
            vec![
                CallableType::new(db, public.clone(), CallableTypeKind::ClassMethodLike)
                    .with_deprecated(db, input.literal.last_definition),
                CallableType::new(
                    db,
                    CallableSignature::single(Signature::new_generic(
                        Some(input.request.inherited),
                        Parameters::empty(),
                        Type::bool_literal(false),
                    )),
                    CallableTypeKind::StaticMethodLike,
                ),
            ]
            .into_boxed_slice(),
        ),
        Storage::Absent => {
            let mut signature = input.request.function.last_definition_signature(db).clone();
            assert!(signature.generic_context.is_none());
            assert_eq!(signature.parameters().len(), 1);
            assert_eq!(
                signature
                    .parameters()
                    .iter()
                    .next()
                    .and_then(|parameter| parameter.name())
                    .map(|name| name.as_str()),
                Some("value")
            );
            assert_eq!(signature.return_ty, Type::unknown());
            signature.generic_context = Some(input.request.inherited);
            Some(vec![CallableType::single(db, signature)].into_boxed_slice())
        }
        Storage::Ignored | Storage::EmptyPublic => None,
    };
    assert_eq!(
        result.updated_implementation_callables(db),
        expected_implementations.as_deref()
    );
    let kind = match storage {
        Storage::Empty | Storage::Multiple | Storage::Absent => LiteralKind::SeparateImplementation,
        Storage::Ignored | Storage::EmptyPublic => LiteralKind::Single,
    };
    assert_eq!(
        result,
        fixtures::function(
            db,
            input.literal.last_definition,
            kind,
            public,
            expected_implementations,
            Some(CallableTypeKind::FunctionLike),
        )
    );
    assert_eq!(
        result,
        input
            .request
            .function
            .with_inherited_generic_context(db, input.request.inherited)
    );
}

/// Stores bounded passive events and watches only the input function's last-definition query.
#[derive(Clone, Debug)]
struct Journal {
    function: salsa::Id,
    events: [Option<Stage>; 512],
    len: usize,
    overflowed: bool,
    live_children: usize,
    pending_children: [bool; 16],
    malformed_lifetime: bool,
    retiring_live_children: Option<usize>,
    child_key: salsa::DatabaseKeyIndex,
    canonical_entries: usize,
    canonical_entry_had_live_pending_child: bool,
    cancel_on_entry: Option<salsa::CancellationToken>,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Records actual provider and reconstruction boundaries without requesting work or changing limits.
fn record(function: salsa::Id, stage: Stage) {
    JOURNAL.with_borrow_mut(|slot| {
        let Some(journal) = slot else {
            return;
        };
        if journal.function != function {
            return;
        }
        if let Some(entry) = journal.events.get_mut(journal.len) {
            *entry = Some(stage);
            journal.len += 1;
        } else {
            journal.overflowed = true;
        }
        match stage {
            Stage::ChildEntered => {
                if let Some(pending) = journal.pending_children.get_mut(journal.live_children) {
                    *pending = false;
                } else {
                    journal.malformed_lifetime = true;
                }
                journal.live_children += 1;
            }
            Stage::ChildPending => {
                if let Some(index) = journal.live_children.checked_sub(1)
                    && let Some(pending) = journal.pending_children.get_mut(index)
                {
                    *pending = true;
                } else {
                    journal.malformed_lifetime = true;
                }
            }
            Stage::ChildRetired => {
                if let Some(remaining) = journal.live_children.checked_sub(1) {
                    journal.live_children = remaining;
                    if let Some(pending) = journal.pending_children.get_mut(remaining) {
                        *pending = false;
                    }
                } else {
                    journal.malformed_lifetime = true;
                }
            }
            Stage::Retiring => journal.retiring_live_children = Some(journal.live_children),
            Stage::Retained
            | Stage::BeforeUpdated
            | Stage::AfterUpdated
            | Stage::BeforeIntern
            | Stage::AfterIntern => {}
        }
    });
}

/// Cancels at an actual canonical child entry only when its provider was observed pending and live.
fn record_query(event: &salsa::EventKind) {
    let salsa::EventKind::WillExecute { database_key } = event else {
        return;
    };
    let cancel = JOURNAL.with_borrow_mut(|slot| {
        let Some(journal) = slot else {
            return None;
        };
        if journal.child_key != *database_key {
            return None;
        }
        journal.canonical_entries += 1;
        let retained_pending = journal
            .live_children
            .checked_sub(1)
            .and_then(|index| journal.pending_children.get(index))
            .copied()
            .unwrap_or(false);
        journal.canonical_entry_had_live_pending_child |= retained_pending;
        if retained_pending {
            journal.cancel_on_entry.take()
        } else {
            None
        }
    });
    if let Some(cancel) = cancel {
        cancel.cancel();
    }
}

/// Restores the passive observer and clears its retained cancellation token on scope exit.
#[derive(Debug)]
struct Recording(Option<fn(salsa::Id, Stage)>);

impl Recording {
    /// Records this constructed function and optionally cancels at real fallback entry after its
    /// provider has returned `Pending` and while that provider remains live.
    fn start(
        db: &TestDb,
        request: ContextRequest<'_>,
        cancel: Option<salsa::CancellationToken>,
    ) -> Self {
        JOURNAL.with_borrow_mut(|slot| {
            *slot = Some(Journal {
                function: request.function.as_id(),
                events: [None; 512],
                len: 0,
                overflowed: false,
                live_children: 0,
                pending_children: [false; 16],
                malformed_lifetime: false,
                retiring_live_children: None,
                child_key: function_last_definition_signature_ingredient(db)
                    .database_key_index(request.function.as_id()),
                canonical_entries: 0,
                canonical_entry_had_live_pending_child: false,
                cancel_on_entry: cancel,
            });
        });
        Self(context_observations::set_observer(Some(record)))
    }

    /// Copies passive evidence before the observer is removed.
    fn journal(&self) -> Journal {
        JOURNAL.with_borrow(|slot| slot.clone().expect("active inherited-context recording"))
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        context_observations::set_observer(self.0);
        JOURNAL.with_borrow_mut(|slot| *slot = None);
    }
}

/// Builds a fresh database with observation limited to canonical query entry events.
fn context_database() -> TestDb {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_salsa_event_callback(record_query)
        .with_file("src/main.py", SOURCE)
        .build()
        .unwrap()
}

/// Checks provider drainage before enclosing function-future retirement; no parent query is created here.
fn assert_drained(journal: &Journal) {
    assert!(!journal.overflowed, "{journal:?}");
    assert!(!journal.malformed_lifetime, "{journal:?}");
    assert_eq!(journal.live_children, 0, "{journal:?}");
    assert_eq!(journal.retiring_live_children, Some(0), "{journal:?}");
    assert_eq!(
        journal.events.first(),
        Some(&Some(Stage::Retained)),
        "{journal:?}"
    );
    assert_eq!(
        journal
            .events
            .iter()
            .filter(|stage| **stage == Some(Stage::Retiring))
            .count(),
        1,
        "{journal:?}"
    );
    assert_no_active_attempt();
    assert_eq!(observations::counts().0, 0);
}

/// Public overloads preserve ordered identity deduplication and all other metadata. Separate
/// implementations preserve exact storage presence, callable order, kind, and deprecation handles.
/// All input metadata is constructed; only absent storage requests the real source signature.
#[test_case::test_case(Storage::Empty; "present empty implementation list")]
#[test_case::test_case(Storage::Multiple; "multiple retained implementation callables")]
#[test_case::test_case(Storage::Absent; "actual last-definition fallback")]
#[test_case::test_case(Storage::Ignored; "single literal ignores stored implementation payload")]
#[test_case::test_case(Storage::EmptyPublic; "empty public overload set")]
fn stored_context_branches_preserve_complete_metadata(storage: Storage) {
    let db = context_database();
    let prepared = prepare(&db);
    let input = input(&db, &prepared, storage);
    let ingredient = function_last_definition_signature_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.request.function.as_id())
            .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    observations::reset(None);
    let recording = Recording::start(&db, input.request, None);
    let controlled = capture(&db, || {
        controlled_member_operation(&prepared, input.request, &funded())
    })
    .unwrap();
    let journal = recording.journal();
    drop(recording);
    let uses_fallback = storage == Storage::Absent;
    if uses_fallback {
        controlled.check_root_reads().unwrap();
    } else {
        assert_eq!(
            controlled.check_root_reads(),
            Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
        );
        assert!(controlled.reads.is_empty());
    }
    let Ok(AnalysisOutcome::Complete(result)) = controlled.value else {
        panic!("{:?}", controlled.value);
    };
    assert_drained(&journal);
    assert!(
        journal.events.contains(&Some(Stage::AfterIntern)),
        "{journal:?}"
    );
    assert_eq!(
        journal.canonical_entries,
        usize::from(uses_fallback),
        "{journal:?}"
    );
    assert_eq!(
        controlled
            .reads
            .iter()
            .any(|read| read.key == journal.child_key),
        uses_fallback
    );
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.request.function.as_id())
            .is_ok(),
        uses_fallback
    );
    assert_result(&db, &input, storage, result);
}

/// Selects local aggregate construction or completion of the function interner provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Boundary {
    Updated,
    Intern,
}

impl Boundary {
    const fn before(self) -> Stage {
        match self {
            Self::Updated => Stage::BeforeUpdated,
            Self::Intern => Stage::BeforeIntern,
        }
    }

    const fn after(self) -> Stage {
        match self {
            Self::Updated => Stage::AfterUpdated,
            Self::Intern => Stage::AfterIntern,
        }
    }
}

/// Reports whether a fresh attempt reaches the completion marker after the selected boundary.
/// Numeric-limit calibration uses constructed stored inputs and an independent database per attempt.
fn reaches(boundary: Boundary, policy: &AnalysisPolicy) -> bool {
    let db = context_database();
    let prepared = prepare(&db);
    let input = input(&db, &prepared, Storage::Multiple);
    observations::reset(None);
    let recording = Recording::start(&db, input.request, None);
    let _result = controlled_member_operation(&prepared, input.request, policy);
    let journal = recording.journal();
    drop(recording);
    assert_no_active_attempt();
    assert!(!journal.overflowed);
    journal.events.contains(&Some(boundary.after()))
}

/// Independent work and byte refusal retains the original function and drains provider futures
/// before enclosing retirement. The identical input is then retried in the same database revision.
/// This adapter has no canonical parent memo; completion means that it returned a function handle.
#[test_case::test_case(Boundary::Updated, Resource::Work; "aggregate work")]
#[test_case::test_case(Boundary::Updated, Resource::Bytes; "aggregate bytes")]
#[test_case::test_case(Boundary::Intern, Resource::Work; "function interner provider work")]
#[test_case::test_case(Boundary::Intern, Resource::Bytes; "function interner provider bytes")]
fn numeric_refusal_retains_input_for_same_revision_retry(boundary: Boundary, resource: Resource) {
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
    let db = context_database();
    let prepared = prepare(&db);
    let input = input(&db, &prepared, Storage::Multiple);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(&db, input.request, None);
    let result = controlled_member_operation(&prepared, input.request, &resource.policy(low));
    let journal = recording.journal();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert!(
        journal.events.contains(&Some(boundary.before())),
        "{journal:?}"
    );
    assert!(
        !journal.events.contains(&Some(boundary.after())),
        "{journal:?}"
    );
    assert_original(&db, &input);
    assert_drained(&journal);
    observations::reset(None);
    let recording = Recording::start(&db, input.request, None);
    let retry = controlled_member_operation(&prepared, input.request, &funded());
    let retry_journal = recording.journal();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(result)) = retry else {
        panic!("{retry:?}");
    };
    assert_drained(&retry_journal);
    assert!(retry_journal.events.contains(&Some(Stage::AfterIntern)));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_result(&db, &input, Storage::Multiple, result);
}

/// A real fallback query runs while its provider is pending and retained. Cancellation drains
/// that provider before its enclosing future, then the same input retries without a revision change.
/// If cancellation allows the fallback child to complete, its memo may already be reusable.
#[test]
fn actual_fallback_pending_cancellation_drains_before_retry() {
    let db = context_database();
    let prepared = prepare(&db);
    let input = input(&db, &prepared, Storage::Absent);
    let revision = salsa::plumbing::current_revision(&db);
    let ingredient = function_last_definition_signature_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.request.function.as_id())
            .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    observations::reset(None);
    let recording = Recording::start(&db, input.request, Some(db.cancellation_token()));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, input.request, &funded())
    }));
    let journal = recording.journal();
    drop(recording);
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert!(
        journal.canonical_entry_had_live_pending_child,
        "{journal:?}"
    );
    assert_eq!(journal.canonical_entries, 1, "{journal:?}");
    assert!(
        !journal.events.contains(&Some(Stage::AfterIntern)),
        "{journal:?}"
    );
    assert_original(&db, &input);
    assert_drained(&journal);
    let child_completed =
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.request.function.as_id())
            .is_ok();
    observations::reset(None);
    let recording = Recording::start(&db, input.request, None);
    let retry = capture(&db, || {
        controlled_member_operation(&prepared, input.request, &funded())
    })
    .unwrap();
    let retry_journal = recording.journal();
    drop(recording);
    retry.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = retry.value else {
        panic!("{:?}", retry.value);
    };
    assert_drained(&retry_journal);
    assert_eq!(
        retry_journal.canonical_entries,
        usize::from(!child_completed)
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, input.request.function.as_id())
            .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_result(&db, &input, Storage::Absent, result);
}
