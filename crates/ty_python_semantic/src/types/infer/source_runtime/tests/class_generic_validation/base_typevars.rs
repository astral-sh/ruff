//! Checks ordered base-variable collection and reports produced from enclosing variable identities.
//!
//! Rust controls expose complete bound handles, retained owners and independent resource refusals,
//! which mdtests cannot observe. Source fixtures run without ordinary inference before either attempt.

use std::panic::AssertUnwindSafe;

use ty_python_core::definition::Definition;
use ty_python_core::scope::NodeWithScopeKind;

use super::interruptions::{Resource, assert_cleanup};
use super::*;
use crate::FxIndexSet;
use crate::types::generics::Specialization;
use crate::types::infer::infer_scope_types;
use crate::types::infer::local_with_fixed_transfers_at;
use crate::types::infer::source_runtime::tests::nominal_members::{
    MemberOperation, controlled_member_operation,
};
use crate::types::tuple::TupleType;
use crate::types::type_alias::PEP695TypeAliasType;
use crate::types::typevar::{
    TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation,
    TypeVarNonce,
};
use crate::types::{
    BindingContext, GenericAlias, KnownInstanceType, ParamSpecAttrKind, TypeAliasType,
};

/// Stores only copied handle identities and expansion keys; observing never reads semantic fields.
#[derive(Clone, Debug, Default)]
struct CollectionJournal {
    retained: Vec<Vec<salsa::Id>>,
    finished: Option<Vec<salsa::Id>>,
    expanded_aliases: Vec<salsa::Id>,
    cancel: Option<salsa::CancellationToken>,
    cancelled: bool,
}

thread_local! {
    static COLLECTION: RefCell<Option<(salsa::Id, CollectionJournal)>> = const { RefCell::new(None) };
}

/// Records the owned set only after insertion succeeds and can cancel while that set is nonempty.
pub(in crate::types::infer) fn collector_retained(
    class: StaticClassLiteral<'_>,
    variables: &FxIndexSet<BoundTypeVarInstance<'_>>,
) {
    COLLECTION.with_borrow_mut(|slot| {
        if let Some((owner, journal)) = slot
            && *owner == class.as_id()
        {
            journal
                .retained
                .push(variables.iter().map(|variable| variable.as_id()).collect());
            if !variables.is_empty()
                && let Some(cancel) = journal.cancel.take()
            {
                journal.cancelled = true;
                cancel.cancel();
            }
        }
    });
}

/// Records the final ordered set before the completed collector transfers it to its caller.
pub(in crate::types::infer) fn collector_finished(
    class: StaticClassLiteral<'_>,
    variables: &FxIndexSet<BoundTypeVarInstance<'_>>,
) {
    COLLECTION.with_borrow_mut(|slot| {
        if let Some((owner, journal)) = slot
            && *owner == class.as_id()
        {
            assert!(journal.finished.is_none());
            journal.finished = Some(variables.iter().map(|variable| variable.as_id()).collect());
        }
    });
}

/// Records first visits to generic aliases, so repeated bases cannot hide a reset of the seen set.
pub(in crate::types::infer) fn collector_expanded(class: StaticClassLiteral<'_>, ty: Type<'_>) {
    COLLECTION.with_borrow_mut(|slot| {
        if let Some((owner, journal)) = slot
            && *owner == class.as_id()
            && let Type::GenericAlias(alias) = ty
        {
            journal.expanded_aliases.push(alias.as_id());
        }
    });
}

/// Restores the passive collector recorder on return or cancellation unwind.
#[derive(Debug)]
struct CollectionRecording;

impl CollectionRecording {
    /// Observes one syntax-constructed class handle without requesting its definition or base types.
    fn start(class: StaticClassLiteral<'_>, cancel: Option<salsa::CancellationToken>) -> Self {
        COLLECTION.with_borrow_mut(|slot| {
            assert!(slot.is_none());
            *slot = Some((
                class.as_id(),
                CollectionJournal {
                    cancel,
                    ..CollectionJournal::default()
                },
            ));
        });
        Self
    }

    /// Copies evidence while the recorder is installed.
    fn journal(&self) -> CollectionJournal {
        COLLECTION.with_borrow(|slot| {
            slot.as_ref()
                .map(|(_, journal)| journal.clone())
                .unwrap_or_default()
        })
    }
}

impl Drop for CollectionRecording {
    fn drop(&mut self) {
        COLLECTION.with_borrow_mut(|slot| *slot = None);
    }
}

/// Builds a fresh Python 3.13 database, optionally disabling the shadow-report lint.
fn fixture(source: &str, disabled: &[&str]) -> anyhow::Result<TestDb> {
    let registry = crate::default_lint_registry();
    let mut rules = RuleSelection::from_registry(registry);
    for code in disabled {
        rules.disable(registry.get(code)?);
    }
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_rule_selection(rules)
        .with_file("src/main.py", source)
        .build()
}

/// Constructs the exact undecorated fixture class identity from syntax and scope-index fields.
/// This leaves both its definition query and the enclosing scope inference cold.
fn class_identity<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    range: TextRange,
) -> anyhow::Result<(StaticClassLiteral<'db>, Definition<'db>)> {
    let scope = prepared
        .semantic_index()
        .scope_ids()
        .find(|scope| {
            scope.node(db).as_class().is_some_and(|node| {
                StaticClassLiteral::header_range_from_node(node.node(prepared.parsed_module()))
                    == range
            })
        })
        .ok_or_else(|| anyhow::anyhow!("fixture target has no class body scope"))?;
    let Some(node) = scope.node(db).as_class() else {
        anyhow::bail!("fixture target scope is not a class");
    };
    let node = node.node(prepared.parsed_module());
    assert!(node.decorator_list.is_empty());
    let definition = prepared.semantic_index().expect_single_definition(node);
    let class = StaticClassLiteral::new(
        db,
        node.name.id.clone(),
        scope,
        None,
        None,
        false,
        None,
        None,
        false,
        false,
        node.type_params.is_some(),
        node.arguments
            .as_deref()
            .is_some_and(|args| !args.args.is_empty()),
        false,
    );
    Ok((class, definition))
}

/// Runs supplied types through the production collector constructor, walker, insertion and finish.
#[derive(Debug)]
struct Collect<'a, 'db> {
    class: StaticClassLiteral<'db>,
    bases: &'a [Type<'db>],
}

impl<'db> MemberOperation<'db> for Collect<'_, 'db> {
    type Output = FxIndexSet<BoundTypeVarInstance<'db>>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = local_with_fixed_transfers_at(access.endpoint(), 2, 0, || {
            SourceEffects::new(access, program)
        })
        .await?;
        effects
            .collect_base_variables_from_types(
                self.class,
                ProgramEnvironment::from_program(program),
                self.bases,
            )
            .await
    }
}

/// Creates a canonical input occurrence without evaluating any source definition or type default.
fn variable<'db>(
    db: &'db TestDb,
    program: Program<'db>,
    name: &'static str,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new_static(name), None, TypeVarKind::Pep695TypeVar),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(program),
        None,
        TypeVarNonce::NONE,
    )
}

/// Requires the controlled collector to return the exact ordered occurrences and finish its owners.
fn collect<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    class: StaticClassLiteral<'db>,
    bases: &[Type<'db>],
    expected: &[BoundTypeVarInstance<'db>],
) -> anyhow::Result<CollectionJournal> {
    observations::reset(None);
    let recording = CollectionRecording::start(class, None);
    let result = controlled_member_operation(prepared, Collect { class, bases }, &funded());
    let journal = recording.journal();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(variables)) = result else {
        anyhow::bail!("base collector did not complete: {result:?}\n{journal:?}");
    };
    assert_eq!(variables.iter().copied().collect::<Vec<_>>(), expected);
    assert_eq!(
        journal.finished,
        Some(expected.iter().map(|variable| variable.as_id()).collect())
    );
    assert_no_active_attempt();
    assert_eq!(observations::counts().0, 0);
    assert_eq!(
        salsa::prepared_source_probe::try_with_preparation(db, || ()),
        Ok(())
    );
    Ok(journal)
}

/// Repeated bases share their seen state, while complete bound occurrences retain metadata and order.
/// Distinct binding contexts, ParamSpec attributes, freshness and stored defaults remain distinct;
/// the variable stored inside a bound occurrence's default is not itself collected.
#[test]
fn repeated_bases_preserve_full_occurrences_and_shared_seen_state() -> anyhow::Result<()> {
    let db = fixture("class Owner: ...\n", &[])?;
    let prepared = prepare(&db);
    let (_, range) = fixture_target(&db, &prepared, &["Owner"])?;
    let (class, definition) = class_identity(&db, &prepared, range)?;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let program = prepared.program_file().program(&db);
    let hidden = variable(&db, program, "Hidden");
    let identity = TypeVarIdentity::new(
        &db,
        Name::new_static("P"),
        None,
        TypeVarKind::Pep695ParamSpec,
    );
    let raw = TypeVarInstance::new(
        &db,
        identity,
        Some(TypeVarBoundOrConstraintsEvaluation::from(
            TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(hidden)),
        )),
        None,
        Some(TypeVarDefaultEvaluation::Eager(Type::TypeVar(hidden))),
    );
    let changed_raw = TypeVarInstance::new(
        &db,
        identity,
        None,
        None,
        Some(TypeVarDefaultEvaluation::Eager(Type::bool_literal(false))),
    );
    let first = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Synthetic(program),
        Some(ParamSpecAttrKind::Args),
        TypeVarNonce::NONE,
    );
    let changed_default = BoundTypeVarInstance::new(
        &db,
        changed_raw,
        BindingContext::Synthetic(program),
        Some(ParamSpecAttrKind::Args),
        TypeVarNonce::NONE,
    );
    let changed_binding = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Definition(definition),
        Some(ParamSpecAttrKind::Args),
        TypeVarNonce::NONE,
    );
    let changed_attribute = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Synthetic(program),
        Some(ParamSpecAttrKind::Kwargs),
        TypeVarNonce::NONE,
    );
    let changed_freshness = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Synthetic(program),
        Some(ParamSpecAttrKind::Args),
        TypeVarNonce::NONE.increment(),
    );
    let formal = variable(&db, program, "Formal");
    let context = GenericContext::from_typevar_instances(&db, &env, [formal]);
    let repeated_alias = GenericAlias::new(
        &db,
        class,
        context.specialize(&db, vec![Type::TypeVar(first)]),
    );
    let repeated = Type::GenericAlias(repeated_alias);
    let bases = [
        repeated,
        Type::TypeVar(changed_default),
        repeated,
        Type::TypeVar(changed_binding),
        Type::TypeVar(changed_attribute),
        Type::TypeVar(changed_freshness),
    ];
    let journal = collect(
        &db,
        &prepared,
        class,
        &bases,
        &[
            first,
            changed_default,
            changed_binding,
            changed_attribute,
            changed_freshness,
        ],
    )?;
    assert_eq!(
        journal
            .expanded_aliases
            .iter()
            .filter(|key| **key == repeated_alias.as_id())
            .count(),
        1
    );
    assert_eq!(first.identity(&db), changed_default.identity(&db));
    assert_ne!(first, changed_default);
    Ok(())
}

/// A generic alias contributes only stored arguments, excluding its formal and tuple-only children.
/// Generic declarations outside an alias still contribute their variables to the same ordered set.
#[test]
fn stored_alias_arguments_exclude_formals_and_tuple_children() -> anyhow::Result<()> {
    let db = fixture("class Owner: ...\n", &[])?;
    let prepared = prepare(&db);
    let (_, range) = fixture_target(&db, &prepared, &["Owner"])?;
    let (class, _) = class_identity(&db, &prepared, range)?;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let program = prepared.program_file().program(&db);
    let formal = variable(&db, program, "Formal");
    let actual = variable(&db, program, "Actual");
    let tuple_only = variable(&db, program, "TupleOnly");
    let declaration = variable(&db, program, "Declared");
    let context = GenericContext::from_typevar_instances(&db, &env, [formal]);
    let alias = Type::GenericAlias(GenericAlias::new(
        &db,
        class,
        Specialization::new(
            &db,
            context,
            vec![Type::TypeVar(actual)].into_boxed_slice(),
            None,
            Some(TupleType::heterogeneous(
                &db,
                &env,
                [Type::TypeVar(tuple_only)],
            )),
        ),
    ));
    let declarations = Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(
        GenericContext::from_typevar_instances(&db, &env, [declaration]),
    ));
    collect(
        &db,
        &prepared,
        class,
        &[alias, declarations],
        &[actual, declaration],
    )?;
    Ok(())
}

/// Selects the annotation and runtime representations that share a terminal alias expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasRepresentation {
    Annotation,
    Runtime,
}

/// Both alias representations terminate without evaluating their lazy value, then resume later bases.
#[test_case::test_case(AliasRepresentation::Annotation; "annotation alias")]
#[test_case::test_case(AliasRepresentation::Runtime; "runtime TypeAliasType")]
fn terminal_alias_resumes_the_pending_base_scan(
    representation: AliasRepresentation,
) -> anyhow::Result<()> {
    let db = fixture("class Owner: ...\ntype Ignored = Missing\n", &[])?;
    let prepared = prepare(&db);
    let (_, range) = fixture_target(&db, &prepared, &["Owner"])?;
    let (class, _) = class_identity(&db, &prepared, range)?;
    let rhs_scope = prepared
        .semantic_index()
        .scope_ids()
        .find(|scope| matches!(scope.node(&db), NodeWithScopeKind::TypeAlias(_)))
        .ok_or_else(|| anyhow::anyhow!("fixture has no alias RHS scope"))?;
    let alias = TypeAliasType::PEP695(PEP695TypeAliasType::new(
        &db,
        Name::new_static("Ignored"),
        rhs_scope,
        None,
        None,
    ));
    let first = match representation {
        AliasRepresentation::Annotation => Type::TypeAlias(alias),
        AliasRepresentation::Runtime => {
            Type::KnownInstance(KnownInstanceType::TypeAliasType(alias))
        }
    };
    let later = variable(&db, prepared.program_file().program(&db), "Later");
    collect(
        &db,
        &prepared,
        class,
        &[first, Type::TypeVar(later)],
        &[later],
    )?;
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            InferScope::Bare(rhs_scope).as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    Ok(())
}

/// Keeps the completed collection and full reports available for exact same-revision comparisons.
#[derive(Debug, PartialEq)]
struct ScanResult {
    variables: Vec<salsa::Id>,
    diagnostics: Vec<Diagnostic>,
    used: usize,
    base_reports: usize,
}

/// Captures completed base checks before a later unsupported operation stops scope publication.
fn completed_scans<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    scope: ScopeId<'db>,
    class: StaticClassLiteral<'db>,
    range: TextRange,
) -> anyhow::Result<ScanResult> {
    let revision = salsa::plumbing::current_revision(db);
    observations::reset(None);
    let recording = Recording::start(prepared.program_file().file(db));
    let collection = CollectionRecording::start(class, None);
    let result = controlled_scope(prepared, scope, &funded());
    let journal = collection.journal();
    let events = recording.take_events();
    drop(collection);
    drop(recording);
    assert_cleanup(db, scope, revision);
    assert!(
        matches!(
            result,
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::UnavailableOperation(_),
                ..
            })
        ),
        "{result:?}\n{events:?}\n{journal:?}"
    );
    let Some(variables) = journal.finished else {
        anyhow::bail!("base collection did not finish: {events:?}");
    };
    assert!(!variables.is_empty());
    let completed = events.iter().filter(|event| matches!(event,
        Event::Completed(owner, ValidationStage::BaseShadowing, _, _) if *owner == class.as_id()
    )).count();
    assert_eq!(completed, variables.len(), "{events:?}");
    let Some(Event::Completed(owner, ValidationStage::BaseShadowing, diagnostics, used)) =
        events.last()
    else {
        anyhow::bail!("base shadowing did not finish: {events:?}");
    };
    assert_eq!(*owner, class.as_id());
    let diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.id() == DiagnosticId::lint("shadowed-type-variable")
                && diagnostic.primary_span().and_then(|span| span.range()) == Some(range)
        })
        .cloned()
        .collect();
    let base_reports = events
        .iter()
        .filter(|event| {
            matches!(event,
                Event::Inserted(owner, ValidationStage::BaseShadowing, _) if *owner == class.as_id()
            )
        })
        .count();
    Ok(ScanResult {
        variables,
        diagnostics,
        used: *used,
        base_reports,
    })
}

/// Checks complete target diagnostics and suppression use in an independent ordinary database.
fn ordinary_reports(
    source: &str,
    path: &[&str],
    disabled: &[&str],
    expected: &ScanResult,
) -> anyhow::Result<()> {
    let db = fixture(source, disabled)?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, path)?;
    let inference = infer_scope_types(&db, scope, TypeContext::default());
    assert_eq!(
        inference
            .diagnostics()
            .map(TypeCheckDiagnostics::used_len)
            .unwrap_or(0),
        expected.used
    );
    let ordinary = crate::check_file_unwrap(&db, prepared.program_file())
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.id() == DiagnosticId::lint("shadowed-type-variable")
                && diagnostic.primary_span().and_then(|span| span.range()) == Some(range)
        })
        .collect::<Vec<_>>();
    assert_eq!(expected.diagnostics, ordinary);
    Ok(())
}

/// Base shadowing uses source identity, including every matching enclosing context, and preserves
/// full diagnostics and suppressions across cold inference, canonical-child reuse and fresh ordinary checking.
/// In the distinct-identity fixture, the base makes Target generic with LegacyT's declared name T.
/// Own-name shadowing reports that name; base shadowing adds no report for its distinct source identity.
#[test_case::test_case(
    "class Outer[T]:\n    class Target(list[T]): ...\n", &["Outer", "Target"], &[], 1, 1, 0;
    "same source identity"
)]
#[test_case::test_case(
    "from typing import TypeVar\nLegacyT = TypeVar(\"T\")\nclass Outer[T]:\n    class Target(list[LegacyT]): ...\n",
    &["Outer", "Target"], &[], 0, 1, 0;
    "same name different source identity"
)]
#[test_case::test_case(
    "from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Outer(Generic[T]):\n    class Middle(Generic[T]):\n        class Target(list[T]): ...\n",
    &["Outer", "Middle", "Target"], &[], 2, 2, 0;
    "every enclosing identity match"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Target(list[T]): ...  # ty: ignore[shadowed-type-variable]\n",
    &["Outer", "Target"], &[], 0, 0, 1;
    "base report suppression"
)]
#[test_case::test_case(
    "class Outer[T]:\n    class Target(list[T]): ...\n", &["Outer", "Target"], &["shadowed-type-variable"], 0, 0, 0;
    "base report lint disabled"
)]
fn cold_base_shadowing_matches_fresh_ordinary(
    source: &str,
    path: &[&str],
    disabled: &[&str],
    expected_base_reports: usize,
    expected_reports: usize,
    expected_used: usize,
) -> anyhow::Result<()> {
    let db = fixture(source, disabled)?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, path)?;
    let (class, definition) = class_identity(&db, &prepared, range)?;
    let ingredient = definition_inference_ingredient(&db);
    assert_cleanup(&db, scope, salsa::plumbing::current_revision(&db));
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, definition.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    let cold = completed_scans(&db, &prepared, scope, class, range)?;
    assert_eq!(cold.base_reports, expected_base_reports);
    assert_eq!(cold.diagnostics.len(), expected_reports);
    assert_eq!(cold.used, expected_used);
    assert!(cold.diagnostics.iter().all(|diagnostic| diagnostic.headline_message()
        == "Generic class `Target` uses type variable `T` already bound by an enclosing scope"));
    let Ok(certified) = FinalSourceMemo::certify(&db as &dyn Db, ingredient, definition.as_id())
    else {
        anyhow::bail!("base scan did not retain the completed canonical class definition");
    };
    assert_eq!(
        certified.database_key(),
        ingredient.database_key_index(definition.as_id())
    );
    let canonical = infer_definition_types(&db, definition);
    assert_eq!(
        canonical.original_class_type(definition),
        Some(ClassLiteral::Static(class))
    );
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let retry = completed_scans(&db, &prepared, scope, class, range)?;
    assert_eq!(cold, retry);
    assert!(std::ptr::eq(
        canonical,
        infer_definition_types(&db, definition)
    ));
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(definition.as_id()),
        &events_db.take_salsa_events(),
    );
    ordinary_reports(source, path, disabled, &cold)
}

const RETAINED_SOURCE: &str = "class Outer[T, U]:\n    class Target(dict[T, U]): ...\n";
const RETAINED_PATH: &[&str] = &["Outer", "Target"];

/// Cancellation while the collector owns its first variable drains the pending walk and result set.
/// The same-revision retry completes both variables and reproduces the full ordinary reports.
#[test]
fn cancelled_retained_collection_retries() -> anyhow::Result<()> {
    let db = fixture(RETAINED_SOURCE, &[])?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, RETAINED_PATH)?;
    let (class, _) = class_identity(&db, &prepared, range)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = CollectionRecording::start(class, Some(db.cancellation_token()));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_scope(&prepared, scope, &funded())
    }));
    let journal = recording.journal();
    drop(recording);
    assert!(journal.cancelled, "{result:?}\n{journal:?}");
    assert!(
        matches!(result, Err(salsa::Cancelled::Local)),
        "{result:?}\n{journal:?}"
    );
    assert_eq!(journal.retained.len(), 1, "{journal:?}");
    assert_eq!(journal.retained[0].len(), 1);
    assert!(journal.finished.is_none());
    assert_cleanup(&db, scope, revision);
    let retry = completed_scans(&db, &prepared, scope, class, range)?;
    assert_eq!(retry.variables.len(), 2);
    assert_eq!(retry.diagnostics.len(), 2);
    ordinary_reports(RETAINED_SOURCE, RETAINED_PATH, &[], &retry)
}

/// Observes the second retained variable under a numeric limit on an independent cold database.
fn retains_second(resource: Resource, limit: usize) -> anyhow::Result<bool> {
    let db = fixture(RETAINED_SOURCE, &[])?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, RETAINED_PATH)?;
    let (class, _) = class_identity(&db, &prepared, range)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = CollectionRecording::start(class, None);
    let _result = controlled_scope(&prepared, scope, &resource.policy(limit));
    let journal = recording.journal();
    drop(recording);
    assert_cleanup(&db, scope, revision);
    Ok(journal
        .retained
        .iter()
        .any(|variables| variables.len() >= 2))
}

/// Independent work and byte limits refuse after the first retention, leaving no enclosing scope memo.
/// The unchanged normal policy then retries the collected order and full reports in the same revision.
#[test_case::test_case(Resource::Work; "retained collector work")]
#[test_case::test_case(Resource::Bytes; "retained collector bytes")]
fn refused_retained_collection_retries(resource: Resource) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(retains_second(resource, high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if retains_second(resource, middle)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = fixture(RETAINED_SOURCE, &[])?;
    let prepared = prepare(&db);
    let (scope, range) = fixture_target(&db, &prepared, RETAINED_PATH)?;
    let (class, _) = class_identity(&db, &prepared, range)?;
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = CollectionRecording::start(class, None);
    let result = controlled_scope(&prepared, scope, &resource.policy(low));
    let journal = recording.journal();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        }),
        "{journal:?}"
    );
    assert_eq!(journal.retained.len(), 1, "{journal:?}");
    assert_eq!(journal.retained[0].len(), 1);
    assert!(journal.finished.is_none());
    assert_cleanup(&db, scope, revision);
    let retry = completed_scans(&db, &prepared, scope, class, range)?;
    assert_eq!(retry.variables.len(), 2);
    assert_eq!(retry.diagnostics.len(), 2);
    ordinary_reports(RETAINED_SOURCE, RETAINED_PATH, &[], &retry)
}
