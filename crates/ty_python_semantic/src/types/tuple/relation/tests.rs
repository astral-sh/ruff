//! Execution and ownership checks that Python-level tuple relation assertions cannot observe.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;

use super::*;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::{ConstraintSetBuilder, IteratorConstraintsExtension};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::tuple::FixedLengthTuple;
use crate::types::typevar::{
    BindingContext, TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarNonce, TypeVarSet,
};
use crate::types::{ApplyTypeMappingVisitor, KnownClass, TypeVarVariance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Driver {
    Synchronous,
    Asynchronous,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event<'db> {
    Checkpoint,
    Spec(TupleType<'db>),
    Mode,
    HasContext,
    ReportLength(usize, TupleLength),
    ReportElement(Type<'db>, Type<'db>, usize, usize),
    Constant(bool),
    IsNever,
    IsNeverSatisfied,
    Pair(Type<'db>, Type<'db>),
    PairWithoutContext(Type<'db>, Type<'db>),
    Conjoin,
    FoldStart,
    FoldPush,
    FoldFinish,
    Elements(usize),
    Next(Direction, usize),
    NextZip(usize, usize),
    NextLongest(Direction, usize, usize),
    SamePack(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>),
    Inferable(BoundTypeVarInstance<'db>),
    Gradual(VariableSegment<'db>),
    EmptyProtocol,
    PackFixed(Vec<Type<'db>>),
    PackVariable(Vec<Type<'db>>, VariableSegment<'db>, Vec<Type<'db>>),
    NormalizedStart(NormalizedPart),
    NormalizedStep(NormalizedPart),
    NormalizedDecision(Type<'db>, bool),
    Equivalent(Type<'db>, Type<'db>),
    NextNormalized,
    NextNormalizedPair,
    BufferStart,
    BufferPush(Type<'db>),
    BufferElements(usize),
    FixedPair,
    VariablePair,
    Boundaries(Type<'db>, Type<'db>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refused {
    Effect(usize),
    MissingPairResult,
    UnexpectedPending,
}

/// Counts a retained fold or suffix buffer until its owner completes or is dropped.
#[derive(Debug)]
struct Retained<T> {
    inner: T,
    live: Rc<Cell<usize>>,
}

impl<T> Drop for Retained<T> {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

/// Records each effect before delegation, optionally refusing it or pausing a child comparison.
struct Recording<'db, 'check, 'a, 'c> {
    ordinary: OrdinaryTupleRelations<'check, 'a, 'c, 'db>,
    events: RefCell<Vec<Event<'db>>>,
    pair_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    pairs: Cell<usize>,
    live_folds: Rc<Cell<usize>>,
    live_buffers: Rc<Cell<usize>>,
    refuse_at: Option<usize>,
    pause_before_pair: Cell<Option<usize>>,
    pause_equivalence: Cell<bool>,
}

impl fmt::Debug for Recording<'_, '_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recording")
            .field("events", &self.events)
            .field("pairs", &self.pairs)
            .field("live_folds", &self.live_folds)
            .field("live_buffers", &self.live_buffers)
            .finish_non_exhaustive()
    }
}

impl<'db, 'check, 'a, 'c> Recording<'db, 'check, 'a, 'c> {
    fn new(db: &'db TestDb, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self {
            ordinary: OrdinaryTupleRelations { db, checker },
            events: RefCell::default(),
            pair_results: None,
            pairs: Cell::new(0),
            live_folds: Rc::default(),
            live_buffers: Rc::default(),
            refuse_at: None,
            pause_before_pair: Cell::new(None),
            pause_equivalence: Cell::new(false),
        }
    }

    /// Records admission before an effect can advance a cursor or change a constraint fold.
    fn record(&self, event: Event<'db>) -> Result<(), Refused> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse_at == Some(index) {
            Err(Refused::Effect(index))
        } else {
            Ok(())
        }
    }

    /// Runs the same tuple entry point through either generated execution form.
    fn evaluate(
        &self,
        source: TupleType<'db>,
        target: TupleType<'db>,
        driver: Driver,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        match driver {
            Driver::Synchronous => check_tuple_pair_sync(source, target, self, TupleRelationFacts),
            Driver::Asynchronous => ready(check_tuple_pair_with(
                source,
                target,
                self,
                TupleRelationFacts,
            )),
        }
    }

    /// Verifies that completion, refusal, or cancellation released every owned execution resource.
    fn assert_released(&self) {
        assert_eq!(self.live_folds.get(), 0);
        assert_eq!(self.live_buffers.get(), 0);
    }

    /// Returns semantic child calls in their observed order, excluding local bookkeeping effects.
    fn semantic_events(&self) -> Vec<Event<'db>> {
        self.events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Pair(..)
                | Event::PairWithoutContext(..)
                | Event::SamePack(..)
                | Event::Inferable(..)
                | Event::Gradual(..)
                | Event::EmptyProtocol
                | Event::PackFixed(..)
                | Event::PackVariable(..)
                | Event::Equivalent(..)
                | Event::Boundaries(..) => Some(event.clone()),
                Event::Checkpoint
                | Event::Spec(..)
                | Event::Mode
                | Event::HasContext
                | Event::ReportLength(..)
                | Event::ReportElement(..)
                | Event::Constant(..)
                | Event::IsNever
                | Event::IsNeverSatisfied
                | Event::Conjoin
                | Event::FoldStart
                | Event::FoldPush
                | Event::FoldFinish
                | Event::Elements(..)
                | Event::Next(..)
                | Event::NextZip(..)
                | Event::NextLongest(..)
                | Event::NormalizedStart(..)
                | Event::NormalizedStep(..)
                | Event::NormalizedDecision(..)
                | Event::NextNormalized
                | Event::NextNormalizedPair
                | Event::BufferStart
                | Event::BufferPush(..)
                | Event::BufferElements(..)
                | Event::FixedPair
                | Event::VariablePair => None,
            })
            .collect()
    }
}

/// Extracts the result of an ordinary adapter whose effects cannot refuse.
fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

/// Polls a test operation that has no deliberately suspended child.
fn ready<T>(future: impl Future<Output = Result<T, Refused>>) -> Result<T, Refused> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => Err(Refused::UnexpectedPending),
    }
}

/// Converts an unexpected effect refusal into a test failure with the refusal's location.
fn completed<T>(result: Result<T, Refused>) -> anyhow::Result<T> {
    result.map_err(|error| anyhow::anyhow!("unexpected tuple effect failure: {error:?}"))
}

/// Suspends a child once so its caller must retain the surrounding tuple operation.
async fn pause_once() {
    let mut pause = true;
    poll_fn(|context| {
        if std::mem::take(&mut pause) {
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}

macro_rules! ordinary_methods {
    ($(fn $name:ident($($argument:ident: $ty:ty),* $(,)?) -> $result:ty => $event:expr;)*) => {
        $(fn $name(&self, $($argument: $ty),*) -> Result<$result, Refused> {
            self.record($event)?;
            Ok(infallible(self.ordinary.$name($($argument),*)))
        })*
    };
}

impl<'c, 'db: 'c> SynchronousTupleRelationEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Fold = Retained<ConstraintFold<'db, 'c>>;
    type Buffer = Retained<Vec<Type<'db>>>;

    ordinary_methods! {
        fn checkpoint() -> () => Event::Checkpoint;
        fn spec(tuple: TupleType<'db>) -> &'db TupleSpec<'db> => Event::Spec(tuple);
        fn mode() -> (TypeRelation, TypeVarEvaluation) => Event::Mode;
        fn has_context() -> bool => Event::HasContext;
        fn report_length(source_len: usize, target_len: TupleLength) -> () => Event::ReportLength(source_len, target_len);
        fn report_element(source: Type<'db>, target: Type<'db>, index: usize, count: usize) -> () => Event::ReportElement(source, target, index, count);
        fn constant(value: bool) -> ConstraintSet<'db, 'c> => Event::Constant(value);
        fn is_never(value: ConstraintSet<'db, 'c>) -> bool => Event::IsNever;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsNeverSatisfied;
        fn pair_without_context(source: Type<'db>, target: Type<'db>) -> ConstraintSet<'db, 'c> => Event::PairWithoutContext(source, target);
        fn conjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c> => Event::Conjoin;
        fn same_pack(source: BoundTypeVarInstance<'db>, target: BoundTypeVarInstance<'db>) -> bool => Event::SamePack(source, target);
        fn inferable(pack: BoundTypeVarInstance<'db>) -> bool => Event::Inferable(pack);
        fn gradual_element(segment: VariableSegment<'db>) -> Option<Type<'db>> => Event::Gradual(segment);
        fn empty_protocol() -> Type<'db> => Event::EmptyProtocol;
        fn pack_fixed(elements: &[Type<'db>]) -> Type<'db> => Event::PackFixed(elements.to_vec());
        fn pack_variable(prefix: &[Type<'db>], variable: VariableSegment<'db>, suffix: &[Type<'db>]) -> Type<'db> => Event::PackVariable(prefix.to_vec(), variable, suffix.to_vec());
        fn equivalent(element: Type<'db>, variable: Type<'db>) -> bool => Event::Equivalent(element, variable);
        fn normalized_step(elements: &mut NormalizedElements<'_, 'db>) -> Option<NormalizedStep<'db>> => Event::NormalizedStep(elements.part);
        fn normalized_decision(elements: &mut NormalizedElements<'_, 'db>, element: Type<'db>, equivalent: bool) -> ControlFlow<Option<Type<'db>>> => Event::NormalizedDecision(element, equivalent);
    }

    fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Pair(source, target))?;
        let index = self.pairs.replace(self.pairs.get() + 1);
        match &self.pair_results {
            Some(results) => results
                .get(index)
                .copied()
                .ok_or(Refused::MissingPairResult),
            None => Ok(infallible(self.ordinary.pair(source, target))),
        }
    }

    fn fold_start(&self) -> Result<Self::Fold, Refused> {
        self.record(Event::FoldStart)?;
        let inner = infallible(self.ordinary.fold_start());
        assert!(std::ptr::eq(
            inner.builder(),
            self.ordinary.checker.constraints
        ));
        self.live_folds.set(self.live_folds.get() + 1);
        Ok(Retained {
            inner,
            live: Rc::clone(&self.live_folds),
        })
    }

    fn fold_push(
        &self,
        fold: &mut Self::Fold,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        self.record(Event::FoldPush)?;
        Ok(infallible(self.ordinary.fold_push(&mut fold.inner, next)))
    }

    fn fold_finish(&self, fold: &mut Self::Fold) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::FoldFinish)?;
        Ok(infallible(self.ordinary.fold_finish(&mut fold.inner)))
    }

    fn elements<'a>(
        &self,
        elements: &'a [Type<'db>],
    ) -> Result<slice::Iter<'a, Type<'db>>, Refused> {
        self.record(Event::Elements(elements.len()))?;
        Ok(infallible(self.ordinary.elements(elements)))
    }

    fn next(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
        direction: Direction,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::Next(direction, elements.len()))?;
        Ok(infallible(self.ordinary.next(elements, direction)))
    }

    fn next_zip(
        &self,
        source: &mut slice::Iter<'_, Type<'db>>,
        target: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Refused> {
        self.record(Event::NextZip(source.len(), target.len()))?;
        Ok(infallible(self.ordinary.next_zip(source, target)))
    }

    fn next_longest(
        &self,
        source: &mut slice::Iter<'_, Type<'db>>,
        target: &mut slice::Iter<'_, Type<'db>>,
        direction: Direction,
    ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Refused> {
        self.record(Event::NextLongest(direction, source.len(), target.len()))?;
        Ok(infallible(
            self.ordinary.next_longest(source, target, direction),
        ))
    }

    fn normalized_start<'a>(
        &self,
        tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        variable: Option<Type<'db>>,
        part: NormalizedPart,
    ) -> Result<NormalizedElements<'a, 'db>, Refused> {
        self.record(Event::NormalizedStart(part))?;
        Ok(infallible(
            self.ordinary.normalized_start(tuple, variable, part),
        ))
    }

    fn next_normalized(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::NextNormalized)?;
        next_normalized_sync(elements, self)
    }

    fn next_normalized_pair(
        &self,
        source: &mut NormalizedElements<'_, 'db>,
        target: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Refused> {
        self.record(Event::NextNormalizedPair)?;
        next_normalized_pair_sync(source, target, self, TupleRelationFacts)
    }

    fn buffer_start(&self) -> Result<Self::Buffer, Refused> {
        self.record(Event::BufferStart)?;
        self.live_buffers.set(self.live_buffers.get() + 1);
        Ok(Retained {
            inner: infallible(self.ordinary.buffer_start()),
            live: Rc::clone(&self.live_buffers),
        })
    }

    fn buffer_push(&self, buffer: &mut Self::Buffer, element: Type<'db>) -> Result<(), Refused> {
        self.record(Event::BufferPush(element))?;
        Ok(infallible(
            self.ordinary.buffer_push(&mut buffer.inner, element),
        ))
    }

    fn buffer_elements<'a>(
        &self,
        buffer: &'a Self::Buffer,
    ) -> Result<slice::Iter<'a, Type<'db>>, Refused>
    where
        'db: 'a,
    {
        self.record(Event::BufferElements(buffer.inner.len()))?;
        Ok(infallible(self.ordinary.buffer_elements(&buffer.inner)))
    }

    fn fixed_pair(
        &self,
        source: &[Type<'db>],
        target: &TupleSpec<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::FixedPair)?;
        check_fixed_pair_sync(source, target, self, TupleRelationFacts)
    }

    fn variable_pair(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &TupleSpec<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::VariablePair)?;
        check_variable_pair_sync(source, target, self, TupleRelationFacts)
    }

    fn boundaries(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        source_variable: Type<'db>,
        target_variable: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Boundaries(source_variable, target_variable))?;
        check_boundaries_sync(
            source,
            target,
            source_variable,
            target_variable,
            self,
            TupleRelationFacts,
        )
    }
}

macro_rules! asynchronous_methods {
    ($(fn $name:ident($($argument:ident: $ty:ty),* $(,)?) -> $result:ty;)*) => {
        $(async fn $name(&self, $($argument: $ty),*) -> Result<$result, Refused> {
            SynchronousTupleRelationEffects::$name(self, $($argument),*)
        })*
    };
}

impl<'c, 'db: 'c> TupleRelationEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Fold = Retained<ConstraintFold<'db, 'c>>;
    type Buffer = Retained<Vec<Type<'db>>>;

    asynchronous_methods! {
        fn checkpoint() -> ();
        fn spec(tuple: TupleType<'db>) -> &'db TupleSpec<'db>;
        fn mode() -> (TypeRelation, TypeVarEvaluation);
        fn has_context() -> bool;
        fn report_length(source_len: usize, target_len: TupleLength) -> ();
        fn report_element(source: Type<'db>, target: Type<'db>, index: usize, count: usize) -> ();
        fn constant(value: bool) -> ConstraintSet<'db, 'c>;
        fn is_never(value: ConstraintSet<'db, 'c>) -> bool;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
        fn pair_without_context(source: Type<'db>, target: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn conjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c>;
        fn fold_start() -> Self::Fold;
        fn fold_push(fold: &mut Self::Fold, next: ConstraintSet<'db, 'c>) -> ControlFlow<ConstraintSet<'db, 'c>>;
        fn fold_finish(fold: &mut Self::Fold) -> ConstraintSet<'db, 'c>;
        fn next(elements: &mut slice::Iter<'_, Type<'db>>, direction: Direction) -> Option<Type<'db>>;
        fn next_zip(source: &mut slice::Iter<'_, Type<'db>>, target: &mut slice::Iter<'_, Type<'db>>) -> Option<(Type<'db>, Type<'db>)>;
        fn next_longest(source: &mut slice::Iter<'_, Type<'db>>, target: &mut slice::Iter<'_, Type<'db>>, direction: Direction) -> Option<EitherOrBoth<Type<'db>, Type<'db>>>;
        fn same_pack(source: BoundTypeVarInstance<'db>, target: BoundTypeVarInstance<'db>) -> bool;
        fn inferable(pack: BoundTypeVarInstance<'db>) -> bool;
        fn gradual_element(segment: VariableSegment<'db>) -> Option<Type<'db>>;
        fn empty_protocol() -> Type<'db>;
        fn pack_fixed(elements: &[Type<'db>]) -> Type<'db>;
        fn pack_variable(prefix: &[Type<'db>], variable: VariableSegment<'db>, suffix: &[Type<'db>]) -> Type<'db>;
        fn normalized_step(elements: &mut NormalizedElements<'_, 'db>) -> Option<NormalizedStep<'db>>;
        fn normalized_decision(elements: &mut NormalizedElements<'_, 'db>, element: Type<'db>, equivalent: bool) -> ControlFlow<Option<Type<'db>>>;
        fn buffer_start() -> Self::Buffer;
        fn buffer_push(buffer: &mut Self::Buffer, element: Type<'db>) -> ();
    }

    async fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if self.pause_before_pair.get() == Some(self.pairs.get()) {
            self.pause_before_pair.set(None);
            pause_once().await;
        }
        SynchronousTupleRelationEffects::pair(self, source, target)
    }

    async fn equivalent(&self, element: Type<'db>, variable: Type<'db>) -> Result<bool, Refused> {
        if self.pause_equivalence.replace(false) {
            pause_once().await;
        }
        SynchronousTupleRelationEffects::equivalent(self, element, variable)
    }

    async fn elements<'a>(
        &self,
        elements: &'a [Type<'db>],
    ) -> Result<slice::Iter<'a, Type<'db>>, Refused> {
        SynchronousTupleRelationEffects::elements(self, elements)
    }

    async fn normalized_start<'a>(
        &self,
        tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        variable: Option<Type<'db>>,
        part: NormalizedPart,
    ) -> Result<NormalizedElements<'a, 'db>, Refused> {
        SynchronousTupleRelationEffects::normalized_start(self, tuple, variable, part)
    }

    async fn next_normalized(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::NextNormalized)?;
        next_normalized_with(elements, self).await
    }

    async fn next_normalized_pair(
        &self,
        source: &mut NormalizedElements<'_, 'db>,
        target: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Refused> {
        self.record(Event::NextNormalizedPair)?;
        next_normalized_pair_with(source, target, self, TupleRelationFacts).await
    }

    async fn buffer_elements<'a>(
        &self,
        buffer: &'a Self::Buffer,
    ) -> Result<slice::Iter<'a, Type<'db>>, Refused>
    where
        'db: 'a,
    {
        SynchronousTupleRelationEffects::buffer_elements(self, buffer)
    }

    async fn fixed_pair(
        &self,
        source: &[Type<'db>],
        target: &TupleSpec<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::FixedPair)?;
        check_fixed_pair_with(source, target, self, TupleRelationFacts).await
    }

    async fn variable_pair(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &TupleSpec<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::VariablePair)?;
        check_variable_pair_with(source, target, self, TupleRelationFacts).await
    }

    async fn boundaries(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        source_variable: Type<'db>,
        target_variable: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Boundaries(source_variable, target_variable))?;
        check_boundaries_with(
            source,
            target,
            source_variable,
            target_variable,
            self,
            TupleRelationFacts,
        )
        .await
    }
}

/// Provides fresh relation owners while all effects in one test retain the same checker and builder.
fn with_checker<'db>(
    db: &'db TestDb,
    context: bool,
    check: impl FnOnce(&mut TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let builder = ConstraintSetBuilder::new();
    let relation = HasRelationToVisitor::default(&builder);
    let disjointness = IsDisjointVisitor::default(&builder);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    let mut checker = if context {
        TypeRelationChecker::assignability_with_context(
            &env, &builder, &relation, &disjointness, &signatures, &mapping,
        )
    } else {
        TypeRelationChecker::new(
            &env,
            TypeRelation::Assignability,
            &builder,
            TypeVarSet::None,
            &relation,
            &disjointness,
            &signatures,
            &mapping,
        )
    };
    check(&mut checker)
}

/// Constructs a symbolic pack occurrence with explicit freshness for identity-order assertions.
fn pack<'db>(db: &'db TestDb, freshness: TypeVarNonce) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new_static("Ts"),
                None,
                TypeVarKind::Pep695TypeVarTuple,
            ),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(db.program_environment().program(db)),
        None,
        freshness,
    )
}

/// Builds independent nonterminal constraints to make retained fold contents observable.
fn constraints<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
) -> [ConstraintSet<'db, 'c>; 2] {
    let env = db.program_environment();
    let int = KnownClass::Int.to_instance(db, &env);
    ["T", "U"].map(|name| {
        let variable = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, variable, int)
    })
}

/// Builds a fixed target specification without canonical tuple normalization.
fn fixed<'db>(elements: impl IntoIterator<Item = Type<'db>>) -> TupleSpec<'db> {
    Tuple::Fixed(FixedLengthTuple::from_elements(elements))
}

/// Verifies that a fixed-length mismatch reports context before returning, without opening cursors.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn length_mismatch_precedes_element_work(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, true, |checker| {
        let effects = Recording::new(&db, checker);
        let target = fixed([Type::int_literal(2)]);
        let result = match driver {
            Driver::Synchronous => {
                check_fixed_pair_sync(&[], &target, &effects, TupleRelationFacts)
            }
            Driver::Asynchronous => ready(check_fixed_pair_with(
                &[],
                &target,
                &effects,
                TupleRelationFacts,
            )),
        };
        assert!(completed(result)?.ownership_probe_same_set(checker.never()));
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint,
                Event::HasContext,
                Event::Mode,
                Event::ReportLength(0, TupleLength::Fixed(1)),
                Event::Constant(false),
                Event::IsNever
            ]
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies that an impossible element reports its one-based position before stopping the fold.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn fixed_elements_stop_after_the_first_impossible_constraint(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, true, |checker| {
        let source = [Type::int_literal(1), Type::int_literal(2)];
        let target = fixed([Type::int_literal(3), Type::int_literal(4)]);
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![checker.never()]);
        let result = match driver {
            Driver::Synchronous => {
                check_fixed_pair_sync(&source, &target, &effects, TupleRelationFacts)
            }
            Driver::Asynchronous => ready(check_fixed_pair_with(
                &source,
                &target,
                &effects,
                TupleRelationFacts,
            )),
        };
        assert!(completed(result)?.ownership_probe_same_set(checker.never()));
        assert_eq!(
            *effects.events.borrow(),
            [
                Event::Checkpoint,
                Event::HasContext,
                Event::Constant(true),
                Event::IsNever,
                Event::Elements(2),
                Event::Elements(2),
                Event::FoldStart,
                Event::NextZip(2, 2),
                Event::Pair(source[0], Type::int_literal(3)),
                Event::HasContext,
                Event::IsNeverSatisfied,
                Event::ReportElement(source[0], Type::int_literal(3), 1, 2),
                Event::FoldPush,
                Event::Conjoin
            ]
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies that fixed sources consume the prefix, reversed suffix, then only the remaining middle.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn fixed_to_variable_preserves_both_cursor_ends(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        let source = TupleType::heterogeneous(&db, checker.env, (1..=4).map(Type::int_literal));
        let target_spec = VariableLengthTuple::mixed(
            [Type::int_literal(5)],
            VariableSegment::Homogeneous(Type::int_literal(8)),
            [Type::int_literal(6), Type::int_literal(7)],
        );
        let target = TupleType::new(&db, checker.env, &target_spec);
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![checker.always(); 4]);
        assert!(
            completed(effects.evaluate(source, target, driver))?
                .ownership_probe_same_set(checker.always())
        );
        assert_eq!(
            effects.semantic_events(),
            [(1, 5), (4, 7), (3, 6), (2, 8)].map(|(source, target)| Event::Pair(
                Type::int_literal(source),
                Type::int_literal(target)
            ))
        );
        assert_eq!(effects.pairs.get(), 4);
        effects.assert_released();
        Ok(())
    })
}

/// Verifies that identical pack occurrences use raw endpoints before consulting lazy inference mode.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn same_pack_precedes_lazy_packing(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        checker.typevar_evaluation = TypeVarEvaluation::Lazy;
        let pack = pack(&db, TypeVarNonce::NONE);
        let source = TupleType::new(
            &db,
            checker.env,
            &VariableLengthTuple::mixed(
                [Type::int_literal(1)],
                VariableSegment::TypeVarTuple(pack),
                [Type::int_literal(2), Type::int_literal(3)],
            ),
        );
        let target = TupleType::new(
            &db,
            checker.env,
            &VariableLengthTuple::mixed(
                [Type::int_literal(4)],
                VariableSegment::TypeVarTuple(pack),
                [Type::int_literal(5), Type::int_literal(6)],
            ),
        );
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![checker.always(); 3]);
        completed(effects.evaluate(source, target, driver))?;
        assert_eq!(
            effects.semantic_events(),
            [
                Event::SamePack(pack, pack),
                Event::Pair(Type::int_literal(1), Type::int_literal(4)),
                Event::Pair(Type::int_literal(2), Type::int_literal(5)),
                Event::Pair(Type::int_literal(3), Type::int_literal(6))
            ]
        );
        assert!(!effects.events.borrow().contains(&Event::Mode));
        effects.assert_released();
        Ok(())
    })
}

/// Verifies that fresh occurrences of one pack identity reach lazy packing instead of the same-pack branch.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn pack_freshness_is_part_of_the_branch_identity(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        checker.typevar_evaluation = TypeVarEvaluation::Lazy;
        let source_pack = pack(&db, TypeVarNonce::NONE);
        let target_pack = pack(&db, TypeVarNonce::NONE.increment());
        let source = TupleType::new(
            &db,
            checker.env,
            &VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(source_pack), []),
        );
        let target = TupleType::new(
            &db,
            checker.env,
            &VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(target_pack), []),
        );
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![checker.always()]);
        completed(effects.evaluate(source, target, driver))?;
        assert_eq!(
            effects.semantic_events(),
            [
                Event::SamePack(source_pack, target_pack),
                Event::PackVariable(vec![], VariableSegment::TypeVarTuple(source_pack), vec![]),
                Event::Pair(Type::tuple(source), Type::TypeVar(target_pack))
            ]
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies that variable sources fail subtyping against fixed targets before gradual-element lookup.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn variable_to_fixed_subtyping_stops_before_prenormalization(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        checker.relation = TypeRelation::Subtyping;
        let source = VariableLengthTuple::new([], VariableSegment::Homogeneous(Type::any()), []);
        let target = fixed([Type::int_literal(1)]);
        let effects = Recording::new(&db, checker);
        let result = match driver {
            Driver::Synchronous => {
                check_variable_pair_sync(&source, &target, &effects, TupleRelationFacts)
            }
            Driver::Asynchronous => ready(check_variable_pair_with(
                &source,
                &target,
                &effects,
                TupleRelationFacts,
            )),
        };
        assert!(completed(result)?.ownership_probe_same_set(checker.never()));
        assert_eq!(
            *effects.events.borrow(),
            [Event::Checkpoint, Event::Mode, Event::Constant(false)]
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies equivalence checks precede moved endpoints and are repeated independently for suffix views.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn prenormalized_children_preserve_prefix_suffix_and_variable_order(
    driver: Driver,
) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        let source = TupleType::new(
            &db,
            checker.env,
            &VariableLengthTuple::mixed(
                [Type::int_literal(10)],
                VariableSegment::Homogeneous(Type::int_literal(20)),
                [Type::int_literal(20), Type::int_literal(21)],
            ),
        );
        let target = TupleType::new(
            &db,
            checker.env,
            &VariableLengthTuple::mixed(
                [Type::int_literal(30)],
                VariableSegment::Homogeneous(Type::int_literal(40)),
                [Type::int_literal(40), Type::int_literal(41)],
            ),
        );
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![checker.always(); 4]);
        completed(effects.evaluate(source, target, driver))?;
        let pair =
            |source, target| Event::Pair(Type::int_literal(source), Type::int_literal(target));
        let equivalent = |source, target| {
            Event::Equivalent(Type::int_literal(source), Type::int_literal(target))
        };
        assert_eq!(
            effects.semantic_events(),
            [
                pair(10, 30),
                equivalent(20, 20),
                equivalent(40, 40),
                pair(20, 40),
                equivalent(21, 20),
                equivalent(41, 40),
                equivalent(20, 20),
                equivalent(21, 20),
                equivalent(40, 40),
                equivalent(41, 40),
                pair(21, 41),
                pair(20, 40)
            ]
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies a suspended second element retains the first constraint and both cursor positions.
#[test]
fn suspended_pair_resumes_the_existing_fold_and_leaves_builder_storage_available()
-> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        let source = [Type::int_literal(1), Type::int_literal(2)];
        let target = fixed([Type::int_literal(3), Type::int_literal(4)]);
        let [a, b] = constraints(&db, checker.constraints);
        let expected = [a, b]
            .into_iter()
            .when_all(&db, checker.constraints, |value| value);
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![a, b]);
        effects.pause_before_pair.set(Some(1));
        {
            let mut future = pin!(check_fixed_pair_with(
                &source,
                &target,
                &effects,
                TupleRelationFacts
            ));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(effects.pairs.get(), 1);
            assert_eq!(effects.live_folds.get(), 1);
            assert_eq!(effects.events.borrow().last(), Some(&Event::NextZip(1, 1)));
            // Combining constraints requires mutable storage access while the child is pending.
            //
            assert!(
                !a.or(&db, checker.constraints, || b)
                    .is_trivially_always_satisfied()
            );
            let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            else {
                anyhow::bail!("the paused comparison must finish on the next poll");
            };
            assert!(completed(result)?.ownership_probe_same_set(expected));
        }
        assert_eq!(
            effects.semantic_events(),
            [
                Event::Pair(source[0], Type::int_literal(3)),
                Event::Pair(source[1], Type::int_literal(4))
            ]
        );
        assert_eq!(
            effects
                .events
                .borrow()
                .iter()
                .filter(|event| **event == Event::FoldStart)
                .count(),
            1
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies a suffix element consumed before a pending equivalence child is yielded exactly once.
#[test]
fn suspended_equivalence_retains_the_consumed_suffix_element() -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        let element = Type::int_literal(1);
        let tail = Type::int_literal(2);
        let source =
            VariableLengthTuple::new([], VariableSegment::Homogeneous(element), [element, tail]);
        let effects = Recording::new(&db, checker);
        let mut cursor = completed(SynchronousTupleRelationEffects::normalized_start(
            &effects,
            &source,
            None,
            NormalizedPart::Prefix,
        ))?;
        effects.events.borrow_mut().clear();
        effects.pause_equivalence.set(true);
        {
            let mut future = pin!(next_normalized_with(&mut cursor, &effects));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(
                *effects.events.borrow(),
                [Event::NormalizedStep(NormalizedPart::Prefix)]
            );
            let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            else {
                anyhow::bail!("the paused equivalence check must finish on the next poll");
            };
            assert_eq!(completed(result)?, Some(element));
        }
        assert_eq!(cursor.suffix.as_slice(), &[tail]);
        assert_eq!(
            effects.semantic_events(),
            [Event::Equivalent(element, element)]
        );
        effects.assert_released();
        Ok(())
    })
}

/// Verifies real lazy element constraints retain the original builder, decision node, and source order.
#[test_case::test_case(Driver::Synchronous; "synchronous")]
#[test_case::test_case(Driver::Asynchronous; "asynchronous")]
fn lazy_element_constraints_keep_their_identity(driver: Driver) -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        checker.typevar_evaluation = TypeVarEvaluation::Lazy;
        let variables = ["T", "U"].map(|name| {
            BoundTypeVarInstance::synthetic(
                &db,
                checker.env,
                Name::new_static(name),
                TypeVarVariance::Invariant,
            )
        });
        checker.inferable = TypeVarSet::from_typevars(&db, variables);
        let source_elements = [
            KnownClass::Int.to_instance(&db, checker.env),
            KnownClass::Str.to_instance(&db, checker.env),
        ];
        let target_elements = variables.map(Type::TypeVar);
        let expected = source_elements.into_iter().zip(target_elements).when_all(
            &db,
            checker.constraints,
            |(source, target)| checker.check_type_pair(&db, source, target),
        );
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        assert!(expected.mentions_typevar(&db, variables[0]));
        assert!(expected.mentions_typevar(&db, variables[1]));
        let source = TupleType::heterogeneous(&db, checker.env, source_elements);
        let target = TupleType::heterogeneous(&db, checker.env, target_elements);
        let effects = Recording::new(&db, checker);
        assert!(
            completed(effects.evaluate(source, target, driver))?.ownership_probe_same_set(expected)
        );
        effects.assert_released();
        Ok(())
    })
}

/// Selects tuple branches with different retained resources and semantic dependencies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RefusalCase {
    FixedElements,
    FixedPack,
    GradualToFixed,
    Prenormalized,
    SamePack,
    FreshPack,
    LengthContext,
    ElementContext,
    TargetPackBoundaries,
    SourcePackBoundaries,
}

/// Constructs one branch's operands and mode before admission or refusal is observed.
fn refusal_operands<'db>(
    db: &'db TestDb,
    checker: &mut TypeRelationChecker<'_, '_, 'db>,
    case: RefusalCase,
) -> (TupleType<'db>, TupleType<'db>) {
    let fixed = |elements: &[i64]| {
        TupleType::heterogeneous(
            db,
            checker.env,
            elements.iter().copied().map(Type::int_literal),
        )
    };
    let variable = |prefix: &[i64], segment, suffix: &[i64]| {
        TupleType::new(
            db,
            checker.env,
            &VariableLengthTuple::mixed(
                prefix.iter().copied().map(Type::int_literal),
                segment,
                suffix.iter().copied().map(Type::int_literal),
            ),
        )
    };
    let original = pack(db, TypeVarNonce::NONE);
    match case {
        RefusalCase::FixedElements => (fixed(&[1, 2]), fixed(&[3, 4])),
        RefusalCase::FixedPack => (
            fixed(&[1, 2, 3]),
            variable(&[4], VariableSegment::TypeVarTuple(original), &[5]),
        ),
        RefusalCase::GradualToFixed => (
            variable(&[1], VariableSegment::Homogeneous(Type::any()), &[2]),
            fixed(&[3, 4, 5]),
        ),
        RefusalCase::Prenormalized => (
            variable(
                &[10],
                VariableSegment::Homogeneous(Type::int_literal(20)),
                &[20, 21],
            ),
            variable(
                &[30],
                VariableSegment::Homogeneous(Type::int_literal(40)),
                &[40, 41],
            ),
        ),
        RefusalCase::SamePack => (
            variable(&[1], VariableSegment::TypeVarTuple(original), &[2]),
            variable(&[3], VariableSegment::TypeVarTuple(original), &[4]),
        ),
        RefusalCase::FreshPack => {
            checker.typevar_evaluation = TypeVarEvaluation::Lazy;
            (
                variable(&[1], VariableSegment::TypeVarTuple(original), &[2]),
                variable(
                    &[],
                    VariableSegment::TypeVarTuple(pack(db, TypeVarNonce::NONE.increment())),
                    &[],
                ),
            )
        }
        RefusalCase::LengthContext => {
            (fixed(&[]), fixed(&[1]))
        }
        RefusalCase::ElementContext => {
            (fixed(&[1]), fixed(&[2]))
        }
        RefusalCase::TargetPackBoundaries => (
            variable(&[1], VariableSegment::Homogeneous(Type::any()), &[]),
            variable(&[], VariableSegment::TypeVarTuple(original), &[2]),
        ),
        RefusalCase::SourcePackBoundaries => (
            variable(&[1], VariableSegment::TypeVarTuple(original), &[]),
            variable(&[], VariableSegment::Homogeneous(Type::any()), &[2]),
        ),
    }
}

/// Verifies every reached effect returns its refusal without advancing to later work and allows retry.
#[test_case::test_case(Driver::Synchronous, RefusalCase::FixedElements; "sync fixed")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::FixedElements; "async fixed")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::FixedPack; "sync fixed pack")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::FixedPack; "async fixed pack")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::GradualToFixed; "sync gradual fixed")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::GradualToFixed; "async gradual fixed")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::Prenormalized; "sync normalized")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::Prenormalized; "async normalized")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::SamePack; "sync same pack")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::SamePack; "async same pack")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::FreshPack; "sync fresh pack")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::FreshPack; "async fresh pack")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::LengthContext; "sync length context")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::LengthContext; "async length context")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::ElementContext; "sync element context")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::ElementContext; "async element context")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::TargetPackBoundaries; "sync target pack boundaries")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::TargetPackBoundaries; "async target pack boundaries")]
#[test_case::test_case(Driver::Synchronous, RefusalCase::SourcePackBoundaries; "sync source pack boundaries")]
#[test_case::test_case(Driver::Asynchronous, RefusalCase::SourcePackBoundaries; "async source pack boundaries")]
fn reached_effect_refusal_stops_and_releases_state(
    driver: Driver,
    case: RefusalCase,
) -> anyhow::Result<()> {
    let db = setup_db();
    let context = matches!(case, RefusalCase::LengthContext | RefusalCase::ElementContext);
    with_checker(&db, context, |checker| {
        let (source, target) = refusal_operands(&db, checker, case);
        let recording = || {
            let mut effects = Recording::new(&db, checker);
            let result = match case {
                RefusalCase::ElementContext => checker.never(),
                RefusalCase::FixedElements
                | RefusalCase::FixedPack
                | RefusalCase::GradualToFixed
                | RefusalCase::Prenormalized
                | RefusalCase::SamePack
                | RefusalCase::FreshPack
                | RefusalCase::LengthContext
                | RefusalCase::TargetPackBoundaries
                | RefusalCase::SourcePackBoundaries => checker.always(),
            };
            effects.pair_results = Some(vec![result; 8]);
            effects
        };
        let baseline = recording();
        let expected = completed(baseline.evaluate(source, target, driver))?;
        baseline.assert_released();
        let events = baseline.events.into_inner();
        // Each refusal uses the successful transcript as its ordered list of reached boundaries.
        // The retry shares the same checker and builder, making leaked state observable.
        for index in 0..events.len() {
            let mut effects = recording();
            effects.refuse_at = Some(index);
            let result = effects.evaluate(source, target, driver);
            assert_eq!(result.err(), Some(Refused::Effect(index)));
            assert_eq!(*effects.events.borrow(), events[..=index]);
            effects.assert_released();
            let retry = recording();
            assert!(
                completed(retry.evaluate(source, target, driver))?
                    .ownership_probe_same_set(expected)
            );
            assert_eq!(*retry.events.borrow(), events);
            retry.assert_released();
        }
        Ok(())
    })
}

/// Verifies cancelling a pending middle comparison releases its fold and buffered suffix before retry.
#[test]
fn dropped_pair_releases_the_suffix_buffer_and_fold() -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, false, |checker| {
        let (source, target) = refusal_operands(&db, checker, RefusalCase::GradualToFixed);
        let [a, b] = constraints(&db, checker.constraints);
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![a, checker.always(), b]);
        effects.pause_before_pair.set(Some(2));
        {
            let mut future = pin!(check_tuple_pair_with(
                source,
                target,
                &effects,
                TupleRelationFacts
            ));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(effects.pairs.get(), 2);
            assert_eq!(effects.live_folds.get(), 1);
            assert_eq!(effects.live_buffers.get(), 1);
            assert!(
                !a.or(&db, checker.constraints, || b)
                    .is_trivially_always_satisfied()
            );
        }
        effects.assert_released();
        let mut retry = Recording::new(&db, checker);
        retry.pair_results = Some(vec![a, checker.always(), b]);
        let expected = a.and(&db, checker.constraints, || b);
        assert!(
            completed(retry.evaluate(source, target, Driver::Asynchronous))?
                .ownership_probe_same_set(expected)
        );
        assert_eq!(retry.pairs.get(), 3);
        retry.assert_released();
        Ok(())
    })
}
