//! Canonical class-header query identity, publication and retained-source interruption controls.

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};

use super::*;
use crate::types::class::context::pep695::observations::{self as header_observations, Stage};
use crate::types::class::pep695_generic_context_ingredient;
use crate::types::typevar::TypeVarKind;

/// Keeps the source class key with its copied context for exact-key checks after the cold run.
#[derive(Debug, Eq, PartialEq)]
struct Context<'db> {
    class: StaticClassLiteral<'db>,
    context: Option<GenericContext<'db>>,
}

/// Requests the class definition and its inner context without ordinary semantic preparation.
#[derive(Clone, Copy, Debug)]
struct ContextRequest<'db>(Definition<'db>);

impl<'db> MemberOperation<'db> for ContextRequest<'db> {
    type Output = Context<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let inference = access.definition(self.0).await?;
        let effects = SourceEffects::new(access, program);
        let class = effects
            .local_with_fixed_transfers(4, 0, || {
                inference
                    .original_class_type(self.0)
                    .and_then(ClassLiteral::as_static)
                    .ok_or(RunError::Contract(
                        "class-context fixture has no static class",
                    ))
            })
            .await??;
        let context = access.pep695_class_context(class).await?;
        effects
            .local_with_fixed_transfers(3, 0, || Context { class, context })
            .await
    }
}

/// Captures only a structural definition key; semantic inference starts in `ContextRequest::run`.
fn context_request<'db>(prepared: &PreparedAnalysisFile<'db>) -> ContextRequest<'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .first()
        .and_then(Stmt::as_class_def_stmt)
        .expect("class-context fixture class");
    ContextRequest(prepared.semantic_index().expect_single_definition(class))
}

/// Stores finite passive observations and an optional cancellation request at real `Pending`.
#[derive(Clone, Debug)]
struct Journal {
    events: [Option<(salsa::Id, Stage)>; 32],
    len: usize,
    overflowed: bool,
    cancel: Option<salsa::CancellationToken>,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Records the event and optionally requests cancellation on an actual header `Pending`.
fn record(class: salsa::Id, stage: Stage) {
    let cancel = JOURNAL.with_borrow_mut(|slot| {
        let Some(journal) = slot else {
            return None;
        };
        if let Some(entry) = journal.events.get_mut(journal.len) {
            *entry = Some((class, stage));
            journal.len += 1;
        } else {
            journal.overflowed = true;
        }
        if stage == Stage::HeaderPending {
            journal.cancel.take()
        } else {
            None
        }
    });
    if let Some(cancel) = cancel {
        cancel.cancel();
    }
}

/// Restores the previous passive callback when a control completes or unwinds.
#[derive(Debug)]
struct Recording(Option<fn(salsa::Id, Stage)>);

impl Recording {
    fn start(cancel: Option<salsa::CancellationToken>) -> Self {
        JOURNAL.with_borrow_mut(|slot| {
            *slot = Some(Journal {
                events: [None; 32],
                len: 0,
                overflowed: false,
                cancel,
            })
        });
        Self(header_observations::set_observer(Some(record)))
    }

    fn journal(&self) -> Journal {
        JOURNAL.with_borrow(|slot| slot.clone().expect("active class-context recording"))
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        header_observations::set_observer(self.0);
        JOURNAL.with_borrow_mut(|slot| *slot = None);
    }
}

/// Locates the first event for this one-class query.
fn event(journal: &Journal, stage: Stage) -> Option<(usize, salsa::Id)> {
    journal
        .events
        .iter()
        .enumerate()
        .find_map(|(index, entry)| {
            entry
                .filter(|(_, observed)| *observed == stage)
                .map(|(class, _)| (index, class))
        })
}

/// Checks that the header's borrowed future exits before source-owner retirement begins.
fn assert_header_retired(journal: &Journal) {
    assert!(!journal.overflowed, "{journal:?}");
    let (source, key) = event(journal, Stage::SourceRetained).expect("retained class source");
    let (entered, header_key) =
        event(journal, Stage::HeaderEntered).expect("header future entered");
    let (retired, retired_key) =
        event(journal, Stage::HeaderRetired).expect("header future retired");
    let (owner, owner_key) =
        event(journal, Stage::SourceRetiring).expect("source retirement entry");
    assert_eq!((key, key, key), (header_key, retired_key, owner_key));
    assert!(
        source < entered && entered < retired && retired < owner,
        "{journal:?}"
    );
    assert_no_active_attempt();
    assert_eq!(observations::counts().0, 0);
}

/// A cold inner query certifies its exact class key and preserves absence, kind, source order and class binding.
/// Ordinary comparison occurs after the controlled query; it does not prepare its inputs.
#[test_case::test_case("class Plain: pass\n", None; "absent list")]
#[test_case::test_case("class Box[T, *Ts, **P]: pass\n", Some(&[("T", TypeVarKind::Pep695TypeVar), ("Ts", TypeVarKind::Pep695TypeVarTuple), ("P", TypeVarKind::Pep695ParamSpec)]); "source order")]
fn cold_inner_context_has_canonical_identity(
    source: &str,
    expected: Option<&[(&str, TypeVarKind)]>,
) {
    let db = database(source);
    let prepared = prepare(&db);
    let request = context_request(&prepared);
    observations::reset(None);
    let recording = Recording::start(None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let journal = recording.journal();
    drop(recording);
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = cold.value else {
        panic!("{:?}", cold.value);
    };
    assert_header_retired(&journal);
    let ingredient = pep695_generic_context_ingredient(&db);
    let key = ingredient.database_key_index(result.class.as_id());
    assert!(cold.reads.iter().any(|read| read.key == key));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, result.class.as_id()).is_ok());
    let actual = result.context.map(|context| {
        context
            .variables(&db)
            .map(|variable| {
                (
                    variable.name(&db).as_str().to_owned(),
                    variable.kind(&db),
                    variable.binding_context(&db),
                    variable.freshness(&db),
                )
            })
            .collect::<Vec<_>>()
    });
    let expected = expected.map(|variables| {
        variables
            .iter()
            .map(|(name, kind)| {
                (
                    (*name).to_owned(),
                    *kind,
                    BindingContext::Definition(request.0),
                    TypeVarNonce::NONE,
                )
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(actual, expected);
    assert_eq!(result.context, result.class.pep695_generic_context(&db));
}

/// Tests whether a fresh attempt reaches the retained-source boundary under a numeric policy.
fn reaches_source(policy: &AnalysisPolicy) -> bool {
    let db = database("class Box[T]: pass\n");
    let prepared = prepare(&db);
    observations::reset(None);
    let recording = Recording::start(None);
    let result = controlled_member_operation(&prepared, context_request(&prepared), policy);
    let journal = recording.journal();
    drop(recording);
    drop(result);
    assert!(!journal.overflowed);
    assert_no_active_attempt();
    event(&journal, Stage::SourceRetained).is_some()
}

/// Independent work and byte refusal after source retention leaves the inner query unpublished.
/// Calibration uses separate cold databases; only the explicit retry reuses completed child memos.
#[test_case::test_case(Resource::Work; "work")]
#[test_case::test_case(Resource::Bytes; "bytes")]
fn refused_inner_context_keeps_its_key_for_retry(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(reaches_source(&resource.policy(high)));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_source(&resource.policy(middle)) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database("class Box[T]: pass\n");
    let prepared = prepare(&db);
    let request = context_request(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(None);
    let result = controlled_member_operation(&prepared, request, &resource.policy(high));
    let journal = recording.journal();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    let (_, key) = event(&journal, Stage::SourceRetained).expect("retained class source");
    assert!(
        event(&journal, Stage::HeaderEntered).is_none(),
        "{journal:?}"
    );
    assert!(
        event(&journal, Stage::SourceRetiring).is_some(),
        "{journal:?}"
    );
    let ingredient = pep695_generic_context_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_no_active_attempt();
    observations::reset(None);
    let retry = controlled_member_operation(&prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(result)) = retry else {
        panic!("{retry:?}");
    };
    assert_eq!(result.class.as_id(), key);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok());
    assert_eq!(result.context, result.class.pep695_generic_context(&db));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Real header-child suspension retains the source borrow through cancellation and drainage.
/// A completed canonical memo may survive deferred cancellation; an unfinished memo must be absent.
#[test]
fn cancelled_header_child_retires_before_source_and_retries_exact_key() {
    let db = database("class Box[T]: pass\n");
    let prepared = prepare(&db);
    let request = context_request(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(Some(db.cancellation_token()));
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    let journal = recording.journal();
    drop(recording);
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert!(
        event(&journal, Stage::HeaderPending).is_some(),
        "{journal:?}"
    );
    assert_header_retired(&journal);
    let (_, key) = event(&journal, Stage::HeaderEntered).unwrap();
    let ingredient = pep695_generic_context_ingredient(&db);
    let published = FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok();
    if published {
        assert!(event(&journal, Stage::HeaderReady).is_some(), "{journal:?}");
    } else {
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).map(|_| ()),
            Err(FinalSourceError::MissingMemo)
        );
    }
    observations::reset(None);
    let recording = Recording::start(None);
    let retry = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let retry_journal = recording.journal();
    drop(recording);
    retry.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = retry.value else {
        panic!("{:?}", retry.value);
    };
    assert_eq!(result.class.as_id(), key);
    assert_eq!(
        event(&retry_journal, Stage::HeaderEntered).is_none(),
        published
    );
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key).is_ok());
    assert_eq!(result.context, result.class.pep695_generic_context(&db));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
