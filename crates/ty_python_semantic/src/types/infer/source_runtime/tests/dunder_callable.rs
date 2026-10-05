//! Observes dunder callable publication and builder cleanup through the real source provider.
//! The delegating effects add lifetime observations; they do not manufacture yields or results.

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;
use std::slice;

use ruff_python_ast::name::Name;
use salsa::plumbing::{Ingredient, ZalsaDatabase};
use ty_python_core::scope::NodeWithScopeRef;

use super::function_decorators::{database, prepared};
use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::class::dunder_callable::{
    DunderCallableEffects, DunderCallableFacts, DunderCallableTransform, dunder_callable_with,
};
use crate::types::class::member_lookup::into_function_like_callable;
use crate::types::class::own_member::into_dunder_paramspec_callable;
use crate::types::relation::redundancy_ingredient;
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::set_theoretic::{IntersectionBuilder, RecursivelyDefined, UnionBuilder};
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::type_alias::{PEP695TypeAliasType, TypeAliasType};
use crate::types::typevar::{
    BindingContext, TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, IntersectionType, NegativeIntersectionElements, UnionType,
};

/// Records cleanup order without retaining the child future or builder it describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cleanup {
    ChildDropped { live_builders: usize },
    BuilderDropped { live_children: usize },
}

/// Collects mutation and builder observations from one traversal.
#[derive(Debug, Default)]
struct Progress {
    before_kind: Cell<usize>,
    after_kind: Cell<usize>,
    live_builders: Cell<usize>,
    populated_builders: Cell<usize>,
    created_builders: Cell<usize>,
    live_children: Cell<usize>,
    nested_pending: Cell<usize>,
    completed: Cell<bool>,
    cleanup: RefCell<Vec<Cleanup>>,
}

/// Watches canonical redundancy queries requested by an insertion with two populated builders.
struct QueryObservation {
    cancellation: salsa::CancellationToken,
    ingredient: salsa::IngredientIndex,
    watching: bool,
    pending: bool,
    cancel: bool,
    entries: usize,
    cancelled: bool,
}

thread_local! {
    static QUERY: RefCell<Option<QueryObservation>> = const { RefCell::new(None) };
}

/// Records when the real redundancy query starts after its insertion returned Pending,
/// and cancels at that entry when requested by the cancellation control.
fn observe_query_entry(event: &salsa::EventKind) {
    if let salsa::EventKind::WillExecute { database_key } = *event {
        QUERY.with_borrow_mut(|observation| {
            if let Some(observation) = observation
                && observation.watching
                && observation.pending
                && database_key.ingredient_index() == observation.ingredient
            {
                observation.entries += 1;
                if std::mem::take(&mut observation.cancel) {
                    observation.cancelled = true;
                    observation.cancellation.cancel();
                }
            }
        });
    }
}

/// Limits query observation to one nested traversal, including interruption cleanup.
#[derive(Debug)]
struct QueryRecording;

impl QueryRecording {
    fn start(db: &TestDb, stop: Stop) -> Self {
        QUERY.with_borrow_mut(|observation| {
            assert!(observation.is_none());
            *observation = Some(QueryObservation {
                cancellation: db.cancellation_token(),
                ingredient: redundancy_ingredient(db).ingredient_index(),
                watching: false,
                pending: false,
                cancel: stop == Stop::Cancel,
                entries: 0,
                cancelled: false,
            });
        });
        Self
    }

    fn snapshot(&self) -> (usize, bool) {
        QUERY.with_borrow(|observation| {
            observation
                .as_ref()
                .map(|observation| (observation.entries, observation.cancelled))
                .unwrap_or_default()
        })
    }
}

impl Drop for QueryRecording {
    fn drop(&mut self) {
        QUERY.with_borrow_mut(|observation| *observation = None);
    }
}

/// Retains the observation window until the insertion's own future has drained.
#[derive(Debug)]
struct InsertionWatch;

impl InsertionWatch {
    fn start(nested: bool) -> Self {
        QUERY.with_borrow_mut(|observation| {
            if let Some(observation) = observation {
                observation.watching = nested;
                observation.pending = false;
            }
        });
        Self
    }

    fn pending(&self) {
        QUERY.with_borrow_mut(|observation| {
            if let Some(observation) = observation {
                observation.pending = true;
            }
        });
    }
}

impl Drop for InsertionWatch {
    fn drop(&mut self) {
        QUERY.with_borrow_mut(|observation| {
            if let Some(observation) = observation {
                observation.watching = false;
            }
        });
    }
}

/// Tracks a test wrapper's lifetime after the actual builder has released its storage.
#[derive(Debug)]
struct BuilderLifetime<'a> {
    progress: &'a Progress,
    populated: bool,
}

impl<'a> BuilderLifetime<'a> {
    fn new(progress: &'a Progress) -> Self {
        progress.live_builders.set(progress.live_builders.get() + 1);
        progress
            .created_builders
            .set(progress.created_builders.get() + 1);
        Self {
            progress,
            populated: false,
        }
    }

    fn populated(&mut self) {
        if !self.populated {
            self.populated = true;
            self.progress
                .populated_builders
                .set(self.progress.populated_builders.get() + 1);
        }
    }
}

impl Drop for BuilderLifetime<'_> {
    fn drop(&mut self) {
        self.progress
            .cleanup
            .borrow_mut()
            .push(Cleanup::BuilderDropped {
                live_children: self.progress.live_children.get(),
            });
        self.progress
            .live_builders
            .set(self.progress.live_builders.get() - 1);
        if self.populated {
            self.progress
                .populated_builders
                .set(self.progress.populated_builders.get() - 1);
        }
    }
}

/// Keeps the actual builder ahead of its observation guard in field destruction order.
#[derive(Debug)]
struct ObservedBuilder<'a, T> {
    builder: T,
    lifetime: BuilderLifetime<'a>,
}

/// Records child-future retirement before its enclosing builder can be destroyed.
#[derive(Debug)]
struct ChildLifetime<'a>(&'a Progress);

impl<'a> ChildLifetime<'a> {
    fn new(progress: &'a Progress) -> Self {
        progress.live_children.set(progress.live_children.get() + 1);
        Self(progress)
    }
}

impl Drop for ChildLifetime<'_> {
    fn drop(&mut self) {
        self.0.live_children.set(self.0.live_children.get() - 1);
        self.0.cleanup.borrow_mut().push(Cleanup::ChildDropped {
            live_builders: self.0.live_builders.get(),
        });
    }
}

/// Delegates semantic operations and admission to the source provider while observing test owners.
struct Observing<'a, 'access, 'run, 'db: 'run, A> {
    source: SourceEffects<'access, 'run, 'db, A>,
    progress: &'a Progress,
}

impl<'a, 'run, 'db: 'run, A: SourceAccess<'run, 'db>> DunderCallableEffects<'db>
    for Observing<'a, '_, 'run, 'db, A>
{
    type Error = RunError;
    type Union = ObservedBuilder<'a, UnionBuilder<'db>>;
    type Intersection = ObservedBuilder<'a, IntersectionBuilder<'db>>;

    async fn checkpoint(&self) -> RunResult<()> {
        DunderCallableEffects::checkpoint(&self.source).await
    }

    async fn callable_kind(&self, callable: CallableType<'db>) -> RunResult<CallableTypeKind> {
        DunderCallableEffects::callable_kind(&self.source, callable).await
    }

    async fn signatures(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        DunderCallableEffects::signatures(&self.source, callable).await
    }

    async fn single_paramspec(&self, signatures: &CallableSignature<'db>) -> RunResult<bool> {
        DunderCallableEffects::single_paramspec(&self.source, signatures).await
    }

    async fn next_signature(
        &self,
        cursor: &mut slice::Iter<'db, Signature<'db>>,
    ) -> RunResult<Option<&'db Signature<'db>>> {
        DunderCallableEffects::next_signature(&self.source, cursor).await
    }

    async fn with_kind(
        &self,
        callable: CallableType<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<Type<'db>> {
        self.progress
            .before_kind
            .set(self.progress.before_kind.get() + 1);
        let result = DunderCallableEffects::with_kind(&self.source, callable, kind).await?;
        self.progress
            .after_kind
            .set(self.progress.after_kind.get() + 1);
        Ok(result)
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        DunderCallableEffects::union_elements(&self.source, union).await
    }

    async fn next_union(
        &self,
        cursor: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        DunderCallableEffects::next_union(&self.source, cursor).await
    }

    async fn new_union(&self) -> RunResult<Self::Union> {
        let builder = DunderCallableEffects::new_union(&self.source).await?;
        Ok(ObservedBuilder {
            builder,
            lifetime: BuilderLifetime::new(self.progress),
        })
    }

    async fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> RunResult<()> {
        let _lifetime = ChildLifetime::new(self.progress);
        let watch = InsertionWatch::start(self.progress.populated_builders.get() >= 2);
        {
            let mut child = std::pin::pin!(DunderCallableEffects::union_add(
                &self.source,
                &mut builder.builder,
                ty
            ));
            poll_fn(|context| {
                let result = child.as_mut().poll(context);
                if result.is_pending() && self.progress.populated_builders.get() >= 2 {
                    self.progress
                        .nested_pending
                        .set(self.progress.nested_pending.get() + 1);
                    watch.pending();
                }
                result
            })
            .await?;
        }
        if !builder.builder.is_empty() {
            builder.lifetime.populated();
        }
        Ok(())
    }

    async fn union_recursion(&self, union: UnionType<'db>) -> RunResult<RecursivelyDefined> {
        DunderCallableEffects::union_recursion(&self.source, union).await
    }

    async fn finish_union(
        &self,
        builder: Self::Union,
        recursion: RecursivelyDefined,
    ) -> RunResult<Type<'db>> {
        let ObservedBuilder { builder, lifetime } = builder;
        let result = DunderCallableEffects::finish_union(&self.source, builder, recursion).await;
        drop(lifetime);
        result
    }

    async fn new_intersection(&self) -> RunResult<Self::Intersection> {
        let builder = DunderCallableEffects::new_intersection(&self.source).await?;
        Ok(ObservedBuilder {
            builder,
            lifetime: BuilderLifetime::new(self.progress),
        })
    }

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        DunderCallableEffects::positive_elements(&self.source, intersection).await
    }

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        DunderCallableEffects::negative_elements(&self.source, intersection).await
    }

    async fn next_intersection(&self, cursor: &mut Elements<'db>) -> RunResult<Option<Type<'db>>> {
        DunderCallableEffects::next_intersection(&self.source, cursor).await
    }

    async fn add_positive(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> RunResult<()> {
        DunderCallableEffects::add_positive(&self.source, &mut builder.builder, ty).await?;
        builder.lifetime.populated();
        Ok(())
    }

    async fn add_negative(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> RunResult<()> {
        DunderCallableEffects::add_negative(&self.source, &mut builder.builder, ty).await?;
        builder.lifetime.populated();
        Ok(())
    }

    async fn finish_intersection(&self, builder: Self::Intersection) -> RunResult<Type<'db>> {
        let ObservedBuilder { builder, lifetime } = builder;
        let result = DunderCallableEffects::finish_intersection(&self.source, builder).await;
        drop(lifetime);
        result
    }

    async fn transform(
        &self,
        ty: Type<'db>,
        transform: DunderCallableTransform,
    ) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| dunder_callable_with(ty, transform, DunderCallableFacts, self))
            .await?
            .await
    }
}

/// Runs a direct transformation so later constructor operations cannot mask its result.
#[derive(Clone, Copy, Debug)]
struct Request<'a, 'db> {
    input: Type<'db>,
    transform: DunderCallableTransform,
    progress: &'a Progress,
}

impl<'db> MemberOperation<'db> for Request<'_, 'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = Observing {
            source: SourceEffects::new(access, program),
            progress: self.progress,
        };
        let result =
            dunder_callable_with(self.input, self.transform, DunderCallableFacts, &effects).await?;
        self.progress.completed.set(true);
        Ok(result)
    }
}

/// Builds a regular canonical callable with one object parameter and a distinct literal return.
fn callable(db: &dyn Db, result: i64) -> Type<'_> {
    Type::Callable(CallableType::single(
        db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::object())
            ]),
            Type::int_literal(result),
        ),
    ))
}

/// Builds a regular canonical callable whose sole signature uses a bare ParamSpec,
/// allowing the DunderParamSpec mode to publish its dedicated callable kind.
fn paramspec_callable<'db>(db: &'db dyn Db, program: Program<'db>) -> Type<'db> {
    let raw = TypeVarInstance::new(
        db,
        TypeVarIdentity::new(
            db,
            Name::new_static("P"),
            None,
            TypeVarKind::LegacyParamSpec,
        ),
        None,
        None,
        None,
    );
    let variable = BoundTypeVarInstance::new(
        db,
        raw,
        BindingContext::Synthetic(program),
        None,
        TypeVarNonce::NONE,
    );
    Type::Callable(CallableType::single(
        db,
        Signature::new(Parameters::paramspec(db, variable), Type::object()),
    ))
}

/// Keeps the inner union unflattened until the shared traversal visits it with an outer builder live.
fn nested_union(db: &dyn Db) -> Type<'_> {
    let inner = Type::Union(UnionType::new(
        db,
        Box::from([callable(db, 1), callable(db, 2)]),
        RecursivelyDefined::No,
    ));
    Type::Union(UnionType::new(
        db,
        Box::from([callable(db, 0), inner]),
        RecursivelyDefined::Yes,
    ))
}

fn ordinary<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    input: Type<'db>,
    transform: DunderCallableTransform,
) -> Type<'db> {
    match transform {
        DunderCallableTransform::DunderParamSpec => into_dunder_paramspec_callable(db, env, input),
        DunderCallableTransform::FunctionLike => into_function_like_callable(db, env, input),
    }
}

/// Checks that observed children drained before their actual builder storage was released.
fn assert_drained(progress: &Progress) {
    assert_eq!(progress.live_builders.get(), 0);
    assert_eq!(progress.populated_builders.get(), 0);
    assert_eq!(progress.live_children.get(), 0);
    let cleanup = progress.cleanup.borrow();
    assert!(
        cleanup.iter().all(|event| match event {
            Cleanup::BuilderDropped { live_children } => *live_children == 0,
            Cleanup::ChildDropped { .. } => true,
        }),
        "{progress:?}"
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Chooses one resource to reduce while retaining the normal allowance for the other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            Self::Bytes => AnalysisPolicy {
                requested_bytes_limit: limit,
                ..funded()
            },
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Counts canonical callable identities without requesting any inferred source signatures.
fn callable_count(db: &TestDb) -> usize {
    CallableType::ingredient(db.zalsa())
        .entries(db.zalsa())
        .count()
}

fn mutation_input<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    transform: DunderCallableTransform,
) -> Type<'db> {
    match transform {
        DunderCallableTransform::FunctionLike => callable(db, 0),
        DunderCallableTransform::DunderParamSpec => {
            paramspec_callable(db, prepared.program_file().program(db))
        }
    }
}

/// Reports whether one request at the supplied resource limit publishes a callable identity.
/// A new canonical identity proves publication even if a later result transfer is refused.
fn publishes(
    resource: Resource,
    limit: usize,
    transform: DunderCallableTransform,
) -> anyhow::Result<bool> {
    let db = database("pass\n");
    let prepared = prepared(&db);
    let input = mutation_input(&db, &prepared, transform);
    let before = callable_count(&db);
    let progress = Progress::default();
    observations::reset(None);
    let outcome = controlled_member_operation(
        &prepared,
        Request {
            input,
            transform,
            progress: &progress,
        },
        &resource.policy(limit),
    );
    assert_drained(&progress);
    match outcome {
        Ok(AnalysisOutcome::Complete(_)) => assert!(progress.completed.get()),
        Ok(AnalysisOutcome::Incomplete {
            reason,
            completed: (),
        }) => assert_eq!(reason, resource.reason()),
        Err(error) => anyhow::bail!("callable publication measurement: {error:?}"),
    }
    Ok(callable_count(&db) > before)
}

/// Independent work and byte refusal precedes canonical callable publication in both modes.
/// Retrying the same input at the same database revision produces the ordinary transformation.
#[test_case::test_case(Resource::Work, DunderCallableTransform::FunctionLike; "function work")]
#[test_case::test_case(Resource::Bytes, DunderCallableTransform::FunctionLike; "function bytes")]
#[test_case::test_case(Resource::Work, DunderCallableTransform::DunderParamSpec; "paramspec work")]
#[test_case::test_case(Resource::Bytes, DunderCallableTransform::DunderParamSpec; "paramspec bytes")]
fn callable_publication_refusal_and_retry(
    resource: Resource,
    transform: DunderCallableTransform,
) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(!publishes(resource, low, transform)?);
    assert!(publishes(resource, high, transform)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if publishes(resource, middle, transform)? {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database("pass\n");
    let prepared = prepared(&db);
    let input = mutation_input(&db, &prepared, transform);
    let before = callable_count(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Request {
                input,
                transform,
                progress: &progress
            },
            &resource.policy(low)
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert!(progress.before_kind.get() > 0, "{progress:?}");
    assert_eq!(progress.after_kind.get(), 0);
    assert_eq!(callable_count(&db), before);
    assert_drained(&progress);

    let retry = Progress::default();
    observations::reset(None);
    let outcome = controlled_member_operation(
        &prepared,
        Request {
            input,
            transform,
            progress: &retry,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(result)) = outcome else {
        anyhow::bail!("funded callable retry: {outcome:?}");
    };
    assert_eq!(retry.after_kind.get(), 1);
    assert_eq!(
        result,
        ordinary(
            &db,
            &ProgramEnvironment::from_file(prepared.program_file()),
            input,
            transform
        )
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained(&retry);
    Ok(())
}

/// Selects whether the canonical child runs to completion or receives cancellation at entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    Complete,
    Cancel,
}

/// A real nested union insertion suspends while two populated builders remain alive.
/// Cancellation drains that child before either builder, and a same-revision retry reconstructs
/// the ordinary result. Cancellation follows a canonical redundancy query entering after the
/// production insertion future returned Pending with both builders populated.
#[test_case::test_case(Stop::Complete; "complete")]
#[test_case::test_case(Stop::Cancel; "cancel and retry")]
fn nested_builder_suspension_and_retry(stop: Stop) -> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.pyi", "pass\n")
        .with_salsa_event_callback(observe_query_entry)
        .build()?;
    let prepared = prepared(&db);
    let input = nested_union(&db);
    let transform = DunderCallableTransform::FunctionLike;
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    observations::reset(None);
    let recording = QueryRecording::start(&db, stop);
    let outcome = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(
            &prepared,
            Request {
                input,
                transform,
                progress: &progress,
            },
            &funded(),
        )
    }));
    let (entries, cancelled) = recording.snapshot();
    drop(recording);
    assert!(progress.nested_pending.get() > 0, "{progress:?}");
    assert!(
        entries > 0,
        "no canonical redundancy child began after the nested insertion suspended"
    );
    assert_eq!(progress.created_builders.get(), 2);
    assert!(
        progress
            .cleanup
            .borrow()
            .contains(&Cleanup::ChildDropped { live_builders: 2 }),
        "{progress:?}"
    );
    assert_drained(&progress);
    if stop == Stop::Cancel {
        assert!(cancelled);
        assert!(
            matches!(outcome, Err(salsa::Cancelled::Local)),
            "{outcome:?}"
        );
        assert!(!progress.completed.get());
    } else {
        let Ok(Ok(AnalysisOutcome::Complete(result))) = outcome else {
            anyhow::bail!("nested callable traversal: {outcome:?}");
        };
        assert_eq!(
            result,
            ordinary(
                &db,
                &ProgramEnvironment::from_file(prepared.program_file()),
                input,
                transform
            )
        );
    }

    let retry = Progress::default();
    observations::reset(None);
    let outcome = controlled_member_operation(
        &prepared,
        Request {
            input,
            transform,
            progress: &retry,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(result)) = outcome else {
        anyhow::bail!("nested callable retry: {outcome:?}");
    };
    assert_eq!(
        result,
        ordinary(
            &db,
            &ProgramEnvironment::from_file(prepared.program_file()),
            input,
            transform
        )
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained(&retry);
    Ok(())
}

/// An unchanged alias still triggers union rebuilding, whose unsupported alias insertion reports
/// `OperationId::Union` exactly. Repeating the request cannot turn that refusal into a partially
/// published result.
#[test]
fn alias_rebuilding_preserves_child_refusal() -> anyhow::Result<()> {
    let db = database("type Alias = int\n");
    let prepared = prepared(&db);
    let Some(Stmt::TypeAlias(alias)) = prepared.parsed_module().syntax().body.first() else {
        anyhow::bail!("fixture alias is missing");
    };
    let index = prepared.semantic_index();
    let scope = index.scope_id(index.node_scope(NodeWithScopeRef::TypeAlias(alias)));
    let alias = Type::TypeAlias(TypeAliasType::PEP695(PEP695TypeAliasType::new(
        &db,
        Name::new_static("Alias"),
        scope,
        None,
        None,
    )));
    let input = Type::Union(UnionType::new(
        &db,
        Box::from([Type::int_literal(0), alias]),
        RecursivelyDefined::No,
    ));
    let transform = DunderCallableTransform::DunderParamSpec;
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Request {
                input,
                transform,
                progress: &progress
            },
            &funded()
        ),
        Ok(unavailable(OperationId::Union))
    );
    assert_eq!(progress.created_builders.get(), 1);
    assert!(!progress.completed.get());
    assert_drained(&progress);

    let retry = Progress::default();
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Request {
                input,
                transform,
                progress: &retry
            },
            &funded()
        ),
        Ok(unavailable(OperationId::Union))
    );
    assert_eq!(retry.created_builders.get(), 1);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained(&retry);
    Ok(())
}

/// FunctionLike traversal retains a regular callable in the negative terms of an intersection.
/// Its parameter would trigger conversion if visited, so the unchanged negative and absent kind
/// update establish that only positive terms are transformed. The eager builder still drains.
#[test]
fn intersection_negative_callable_is_preserved() -> anyhow::Result<()> {
    let db = database("pass\n");
    let prepared = prepared(&db);
    let negative = callable(&db, 0);
    let input = Type::Intersection(IntersectionType::new(
        &db,
        FxOrderSet::default(),
        NegativeIntersectionElements::Single(negative),
    ));
    let transform = DunderCallableTransform::FunctionLike;
    let progress = Progress::default();
    observations::reset(None);
    let outcome = controlled_member_operation(
        &prepared,
        Request {
            input,
            transform,
            progress: &progress,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(Type::Intersection(result))) = outcome else {
        anyhow::bail!("negative callable intersection: {outcome:?}");
    };
    assert!(result.positive(&db).is_empty());
    assert_eq!(
        result.negative(&db).iter().copied().collect::<Vec<_>>(),
        [negative]
    );
    assert_eq!(progress.before_kind.get(), 0);
    assert_eq!(progress.after_kind.get(), 0);
    assert_eq!(progress.created_builders.get(), 1);
    assert_eq!(
        Type::Intersection(result),
        ordinary(
            &db,
            &ProgramEnvironment::from_file(prepared.program_file()),
            input,
            transform
        )
    );
    assert_drained(&progress);
    Ok(())
}
