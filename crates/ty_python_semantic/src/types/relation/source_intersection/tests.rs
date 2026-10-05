use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;

use super::*;
use crate::FxOrderSet;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::{ConstraintSetBuilder, IteratorConstraintsExtension};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeRelation};
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ErrorContext, ErrorContextTree, KnownClass,
    TypeVarVariance,
};

fn intersection<'db>(db: &'db TestDb, positive: &[Type<'db>]) -> IntersectionType<'db> {
    IntersectionType::new(
        db,
        FxOrderSet::from_iter(positive.iter().copied()),
        NegativeIntersectionElements::Empty,
    )
}

fn with_checker<'db>(
    db: &'db TestDb,
    check: impl FnOnce(&mut TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let relations = HasRelationToVisitor::default(&constraints);
    let disjointness = IsDisjointVisitor::default(&constraints);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    check(&mut TypeRelationChecker::new(
        &env,
        TypeRelation::Subtyping,
        &constraints,
        TypeVarSet::None,
        &relations,
        &disjointness,
        &signatures,
        &mapping,
    ))
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

fn completed<T>(result: Result<T, Refused>) -> anyhow::Result<T> {
    result.map_err(|error| anyhow::anyhow!("unexpected effect failure: {error:?}"))
}

fn ready<T>(future: impl Future<Output = Result<T, Refused>>) -> Result<T, Refused> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => Err(Refused::UnexpectedPending),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Finite,
    Pair(Type<'db>, Type<'db>),
    PositiveElements,
    PositiveElementsOrObject,
    Next,
    FoldStart,
    FoldPush,
    FoldFinish,
    IsTriviallyAlways,
    Never,
    Disjoin,
    ShouldExpand,
    IsInferable(BoundTypeVarInstance<'db>),
    NewtypeBase,
    Expand,
    ContextStart,
    Capture,
    HasCollected,
    IsNever,
    Report,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refused {
    Effect(usize),
    MissingInput,
    UnexpectedPending,
}

struct Retained<'state, T> {
    inner: T,
    live: &'state Cell<usize>,
}

impl<T> Drop for Retained<'_, T> {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

struct Recording<'db, 'check, 'a, 'c> {
    inline: InlineSourceIntersectionEffects<'db, 'check, 'a, 'c>,
    events: RefCell<Vec<Event<'db>>>,
    pair_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    pair_contexts: Vec<Option<ErrorContext<'db>>>,
    pairs: Cell<usize>,
    finite_result: Option<Option<Type<'db>>>,
    expansion_eligible: Option<bool>,
    expanded: Option<Type<'db>>,
    live_folds: Cell<usize>,
    live_contexts: Cell<usize>,
    refuse_at: Option<usize>,
    pause_before: Cell<Option<Type<'db>>>,
}

impl<'db, 'check, 'a, 'c> Recording<'db, 'check, 'a, 'c> {
    fn new(db: &'db TestDb, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self {
            inline: InlineSourceIntersectionEffects::new(db, checker),
            events: RefCell::default(),
            pair_results: None,
            pair_contexts: vec![],
            pairs: Cell::new(0),
            finite_result: None,
            expansion_eligible: None,
            expanded: None,
            live_folds: Cell::new(0),
            live_contexts: Cell::new(0),
            refuse_at: None,
            pause_before: Cell::new(None),
        }
    }

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

    fn evaluate(
        &self,
        source: IntersectionType<'db>,
        target: Type<'db>,
        asynchronous: bool,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if asynchronous {
            ready(check_source_intersection_with(source, target, self))
        } else {
            check_source_intersection_sync(source, target, self)
        }
    }

    fn eligibility(
        &self,
        source: IntersectionType<'db>,
        asynchronous: bool,
    ) -> Result<bool, Refused> {
        if asynchronous {
            ready(should_expand_source_intersection_with(source, self))
        } else {
            should_expand_source_intersection_sync(source, self)
        }
    }
}

macro_rules! recording_methods {
    ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty => $event:expr;)*) => {
        $(fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
            self.record($event)?;
            Ok(infallible(self.inline.$name($($argument),*)))
        })*
    };
}

impl<'c, 'db: 'c, 'check, 'a> SynchronousSourceIntersectionEffects<'c, 'db>
    for Recording<'db, 'check, 'a, 'c>
{
    type Error = Refused;
    type Elements<'state> = <InlineSourceIntersectionEffects<'db, 'check, 'a, 'c> as SynchronousSourceIntersectionEffects<'c, 'db>>::Elements<'state> where Self: 'state;
    type Fold<'state>
        = Retained<'state, ConstraintFold<'db, 'c>>
    where
        Self: 'state;
    type Context<'state> = Retained<'state, <InlineSourceIntersectionEffects<'db, 'check, 'a, 'c> as SynchronousSourceIntersectionEffects<'c, 'db>>::Context<'state>> where Self: 'state;

    recording_methods! {
        fn positive_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_> => Event::PositiveElements;
        fn positive_elements_or_object(intersection: IntersectionType<'db>) -> Self::Elements<'_> => Event::PositiveElementsOrObject;
        fn is_trivially_always_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsTriviallyAlways;
        fn never() -> ConstraintSet<'db, 'c> => Event::Never;
        fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c> => Event::Disjoin;
        fn typevar_is_inferable(typevar: BoundTypeVarInstance<'db>) -> bool => Event::IsInferable(typevar);
        fn newtype_concrete_base(newtype: NewType<'db>) -> Type<'db> => Event::NewtypeBase;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsNever;
    }

    fn finite_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::Finite)?;
        Ok(self
            .finite_result
            .unwrap_or_else(|| infallible(self.inline.finite_alternatives(intersection))))
    }

    fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Pair(source, target))?;
        let index = self.pairs.replace(self.pairs.get() + 1);
        let result = match &self.pair_results {
            Some(values) => values.get(index).copied().ok_or(Refused::MissingInput)?,
            None => infallible(self.inline.pair(source, target)),
        };
        if let Some(Some(context)) = self.pair_contexts.get(index)
            && let Some(tree) = self.inline.checker.report_context()
        {
            tree.push(context.clone());
        }
        Ok(result)
    }

    fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::Next)?;
        Ok(infallible(self.inline.next_element(elements)))
    }

    fn fold_start(&self) -> Result<Self::Fold<'_>, Refused> {
        self.record(Event::FoldStart)?;
        self.live_folds.set(self.live_folds.get() + 1);
        Ok(Retained {
            inner: infallible(self.inline.fold_start()),
            live: &self.live_folds,
        })
    }

    fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        self.record(Event::FoldPush)?;
        Ok(infallible(self.inline.fold_push(&mut fold.inner, next)))
    }

    fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::FoldFinish)?;
        Ok(infallible(self.inline.fold_finish(&mut fold.inner)))
    }

    fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Refused> {
        self.record(Event::ShouldExpand)?;
        match self.expansion_eligible {
            Some(value) => Ok(value),
            None => should_expand_source_intersection_sync(intersection, self),
        }
    }

    fn expand_intersection(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Type<'db>, Refused> {
        self.record(Event::Expand)?;
        Ok(self
            .expanded
            .unwrap_or_else(|| infallible(self.inline.expand_intersection(intersection))))
    }

    fn context_start(&self) -> Result<Self::Context<'_>, Refused> {
        self.record(Event::ContextStart)?;
        self.live_contexts.set(self.live_contexts.get() + 1);
        Ok(Retained {
            inner: infallible(self.inline.context_start()),
            live: &self.live_contexts,
        })
    }

    fn capture_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
    ) -> Result<(), Refused> {
        self.record(Event::Capture)?;
        Ok(infallible(self.inline.capture_context(&mut context.inner)))
    }

    fn has_collected_context<'state>(
        &'state self,
        context: &Self::Context<'state>,
    ) -> Result<bool, Refused> {
        self.record(Event::HasCollected)?;
        Ok(infallible(
            self.inline.has_collected_context(&context.inner),
        ))
    }

    fn report_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> Result<(), Refused> {
        self.record(Event::Report)?;
        Ok(infallible(self.inline.report_context(
            &mut context.inner,
            intersection,
            target,
        )))
    }
}

macro_rules! asynchronous_methods {
    ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty;)*) => {
        $(async fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
            SynchronousSourceIntersectionEffects::$name(self, $($argument),*)
        })*
    };
}

impl<'c, 'db: 'c> SourceIntersectionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = <Self as SynchronousSourceIntersectionEffects<'c, 'db>>::Elements<'state>
    where
        Self: 'state;
    type Fold<'state>
        = <Self as SynchronousSourceIntersectionEffects<'c, 'db>>::Fold<'state>
    where
        Self: 'state;
    type Context<'state>
        = <Self as SynchronousSourceIntersectionEffects<'c, 'db>>::Context<'state>
    where
        Self: 'state;

    asynchronous_methods! {
        fn finite_alternatives(intersection: IntersectionType<'db>) -> Option<Type<'db>>;
        fn positive_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_>;
        fn positive_elements_or_object(intersection: IntersectionType<'db>) -> Self::Elements<'_>;
        fn fold_start() -> Self::Fold<'_>;
        fn is_trivially_always_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
        fn never() -> ConstraintSet<'db, 'c>;
        fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c>;
        fn typevar_is_inferable(typevar: BoundTypeVarInstance<'db>) -> bool;
        fn newtype_concrete_base(newtype: NewType<'db>) -> Type<'db>;
        fn expand_intersection(intersection: IntersectionType<'db>) -> Type<'db>;
        fn context_start() -> Self::Context<'_>;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
    }

    async fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if self.pause_before.get() == Some(source) {
            self.pause_before.set(None);
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
        SynchronousSourceIntersectionEffects::pair(self, source, target)
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        SynchronousSourceIntersectionEffects::next_element(self, elements)
    }

    async fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        SynchronousSourceIntersectionEffects::fold_push(self, fold, next)
    }

    async fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        SynchronousSourceIntersectionEffects::fold_finish(self, fold)
    }

    async fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Refused> {
        self.record(Event::ShouldExpand)?;
        match self.expansion_eligible {
            Some(value) => Ok(value),
            None => should_expand_source_intersection_with(intersection, self).await,
        }
    }

    async fn capture_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
    ) -> Result<(), Refused> {
        SynchronousSourceIntersectionEffects::capture_context(self, context)
    }

    async fn has_collected_context<'state>(
        &'state self,
        context: &Self::Context<'state>,
    ) -> Result<bool, Refused> {
        SynchronousSourceIntersectionEffects::has_collected_context(self, context)
    }

    async fn report_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> Result<(), Refused> {
        SynchronousSourceIntersectionEffects::report_context(self, context, intersection, target)
    }
}

fn constraints<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
) -> [ConstraintSet<'db, 'c>; 4] {
    let env = db.program_environment();
    let int = KnownClass::Int.to_instance(db, &env);
    ["A", "B", "C", "D"].map(|name| {
        let variable = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, variable, int)
    })
}

fn fold<'db, 'c>(
    db: &'db dyn Db,
    builder: &'c ConstraintSetBuilder<'db>,
    values: &[ConstraintSet<'db, 'c>],
) -> ConstraintSet<'db, 'c> {
    values.iter().copied().when_any(db, builder, |value| value)
}

fn child_context<'db>(index: usize) -> ErrorContext<'db> {
    ErrorContext::NotAssignableToNOtherUnionElements { n: index }
}

#[test]
fn finite_alternatives_are_literal_only_and_return_before_structural_work() -> anyhow::Result<()> {
    let db = setup_db();
    let positive = Type::AlwaysTruthy;
    let source = intersection(&db, &[positive]);
    let alternatives = Type::bool_literal(true);
    with_checker(&db, |checker| {
        for target in [Type::int_literal(1), Type::object()] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.finite_result = Some(Some(alternatives));
                effects.pair_results = Some(vec![checker.always()]);
                let actual = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(checker.always()));
                let expected = if matches!(target, Type::LiteralValue(_)) {
                    vec![Event::Finite, Event::Pair(alternatives, target)]
                } else {
                    vec![
                        Event::ContextStart,
                        Event::PositiveElementsOrObject,
                        Event::FoldStart,
                        Event::Next,
                        Event::Pair(positive, target),
                        Event::Capture,
                        Event::FoldPush,
                        Event::IsTriviallyAlways,
                        Event::HasCollected,
                    ]
                };
                assert_eq!(*effects.events.borrow(), expected);
                assert_eq!(effects.live_folds.get(), 0);
                assert_eq!(effects.live_contexts.get(), 0);
            }
        }
        Ok(())
    })
}

#[test]
fn empty_positives_compare_one_implicit_object_and_disjoin_never() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(&db, &[]);
    let target = Type::AlwaysFalsy;
    with_checker(&db, |checker| {
        let [a, ..] = constraints(&db, checker.constraints);
        let expected = a.or(&db, checker.constraints, || checker.never());
        for asynchronous in [false, true] {
            let mut effects = Recording::new(&db, checker);
            effects.pair_results = Some(vec![a]);
            let actual = completed(effects.evaluate(source, target, asynchronous))?;
            assert!(actual.ownership_probe_same_set(expected));
            assert_eq!(
                *effects.events.borrow(),
                [
                    Event::ContextStart,
                    Event::PositiveElementsOrObject,
                    Event::FoldStart,
                    Event::Next,
                    Event::Pair(Type::object(), target),
                    Event::Capture,
                    Event::FoldPush,
                    Event::Next,
                    Event::FoldFinish,
                    Event::IsTriviallyAlways,
                    Event::ShouldExpand,
                    Event::PositiveElements,
                    Event::Next,
                    Event::Never,
                    Event::Disjoin,
                    Event::HasCollected
                ]
            );
        }
        Ok(())
    })
}

#[test]
fn expansion_stays_outside_the_positive_fold_and_preserves_source_history() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(
        &db,
        &[
            Type::AlwaysTruthy,
            Type::AlwaysFalsy,
            Type::literal_string(),
        ],
    );
    let expanded = Type::int_literal(7);
    let target = Type::object();
    with_checker(&db, |checker| {
        let values = constraints(&db, checker.constraints);
        let expected =
            fold(&db, checker.constraints, &values[..3]).or(&db, checker.constraints, || values[3]);
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        assert!(!expected.ownership_probe_same_set(fold(&db, checker.constraints, &values)));
        let reversed = [values[2], values[1], values[0], values[3]];
        let reversed_expected =
            fold(&db, checker.constraints, &reversed[..3])
                .or(&db, checker.constraints, || reversed[3]);
        assert!(!expected.ownership_probe_same_set(reversed_expected));
        for (values, expected) in [(values, expected), (reversed, reversed_expected)] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.pair_results = Some(values.to_vec());
                effects.expansion_eligible = Some(true);
                effects.expanded = Some(expanded);
                let actual = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(expected));
                let events = effects.events.borrow();
                assert_eq!(
                    &events[events.len() - 6..],
                    [
                        Event::IsTriviallyAlways,
                        Event::ShouldExpand,
                        Event::Expand,
                        Event::Pair(expanded, target),
                        Event::Disjoin,
                        Event::HasCollected
                    ]
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| **event == Event::FoldStart)
                        .count(),
                    1
                );
            }
        }
        Ok(())
    })
}

#[test]
fn expansion_eligibility_scans_raw_positives_and_stops_at_the_first_fixed_typevar()
-> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, |checker| {
        let variable = |name| {
            BoundTypeVarInstance::synthetic(
                &db,
                checker.env,
                Name::new_static(name),
                TypeVarVariance::Invariant,
            )
        };
        let inferable = variable("Inferable");
        let fixed = variable("Fixed");
        let later = variable("Later");
        checker.inferable = TypeVarSet::from_typevars(&db, [inferable]);
        for (positive, expected, events) in [
            (vec![], false, vec![Event::PositiveElements, Event::Next]),
            (
                vec![Type::object(), Type::TypeVar(inferable)],
                false,
                vec![
                    Event::PositiveElements,
                    Event::Next,
                    Event::Next,
                    Event::IsInferable(inferable),
                    Event::Next,
                ],
            ),
            (
                vec![
                    Type::TypeVar(inferable),
                    Type::TypeVar(fixed),
                    Type::TypeVar(later),
                ],
                true,
                vec![
                    Event::PositiveElements,
                    Event::Next,
                    Event::IsInferable(inferable),
                    Event::Next,
                    Event::IsInferable(fixed),
                ],
            ),
        ] {
            let source = intersection(&db, &positive);
            for asynchronous in [false, true] {
                let effects = Recording::new(&db, checker);
                assert_eq!(
                    completed(effects.eligibility(source, asynchronous))?,
                    expected
                );
                assert_eq!(*effects.events.borrow(), events);
                for index in 0..events.len() {
                    let mut refused = Recording::new(&db, checker);
                    refused.refuse_at = Some(index);
                    assert_eq!(
                        refused.eligibility(source, asynchronous),
                        Err(Refused::Effect(index))
                    );
                    assert_eq!(*refused.events.borrow(), events[..=index]);
                }
            }
        }
        Ok(())
    })
}

#[test]
fn saturation_captures_the_last_child_context_and_keeps_the_diagnostic_tail() -> anyhow::Result<()>
{
    let db = setup_db();
    let positive = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let source = intersection(&db, &positive);
    let target = Type::object();
    with_checker(&db, |checker| {
        checker.context_tree = Some(ErrorContextTree::new(checker.relation));
        let [a, ..] = constraints(&db, checker.constraints);
        for values in [
            vec![checker.always()],
            vec![a, a.negate(&db, checker.constraints)],
        ] {
            let expected = fold(&db, checker.constraints, &values);
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.pair_results = Some(values.clone());
                effects.pair_contexts = (1..=values.len())
                    .map(|index| Some(child_context(index)))
                    .collect();
                let actual = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(expected));
                let mut events = vec![
                    Event::ContextStart,
                    Event::PositiveElementsOrObject,
                    Event::FoldStart,
                ];
                for &element in &positive[..values.len()] {
                    events.extend([
                        Event::Next,
                        Event::Pair(element, target),
                        Event::Capture,
                        Event::FoldPush,
                    ]);
                }
                events.extend([
                    Event::IsTriviallyAlways,
                    Event::HasCollected,
                    Event::IsNever,
                ]);
                assert_eq!(*effects.events.borrow(), events);
                assert!(
                    checker
                        .report_context()
                        .is_some_and(ErrorContextTree::is_empty)
                );
                assert_eq!(effects.live_folds.get(), 0);
                assert_eq!(effects.live_contexts.get(), 0);
            }
        }
        Ok(())
    })
}

#[test]
fn full_unsatisfiability_reports_only_collected_positive_contexts() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(&db, &[Type::AlwaysTruthy, Type::AlwaysFalsy]);
    let target = Type::object();
    with_checker(&db, |checker| {
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            checker.env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let bound = |class: KnownClass| {
            ConstraintSet::constrain_typevar_equivalence_bound(
                &db,
                checker.env,
                checker.constraints,
                variable,
                class.to_instance(&db, checker.env),
            )
        };
        let impossible =
            bound(KnownClass::Int).and(&db, checker.constraints, || bound(KnownClass::Str));
        assert!(!impossible.is_trivially_never_satisfied());
        assert!(impossible.is_never_satisfied(&db, checker.env));
        for enabled in [None, Some(false), Some(true)] {
            for positive_contexts in [false, true] {
                for asynchronous in [false, true] {
                    let mut checker = checker.clone();
                    checker.context_tree = enabled.map(|enabled| {
                        let tree = ErrorContextTree::new(checker.relation);
                        tree.set_enabled(enabled);
                        tree
                    });
                    let mut effects = Recording::new(&db, &checker);
                    effects.pair_results = Some(vec![impossible, checker.never(), checker.never()]);
                    effects.pair_contexts = vec![
                        positive_contexts.then(|| child_context(1)),
                        positive_contexts.then(|| child_context(2)),
                        Some(child_context(3)),
                    ];
                    effects.expansion_eligible = Some(true);
                    effects.expanded = Some(Type::int_literal(7));
                    let actual = completed(effects.evaluate(source, target, asynchronous))?;
                    assert!(actual.ownership_probe_same_set(impossible));
                    let reported = enabled == Some(true) && positive_contexts;
                    assert_eq!(effects.events.borrow().contains(&Event::IsNever), reported);
                    assert_eq!(effects.events.borrow().contains(&Event::Report), reported);
                    if let Some(actual_context) = checker.report_context() {
                        let expected = if positive_contexts {
                            let expected = ErrorContextTree::new(checker.relation);
                            expected.set(
                                ErrorContext::NoIntersectionElementAssignableToTarget {
                                    intersection: Type::Intersection(source),
                                    target,
                                },
                                [1, 2].map(|index| {
                                    ErrorContextTree::from_context(
                                        child_context(index),
                                        checker.relation,
                                    )
                                }),
                            );
                            expected
                        } else {
                            ErrorContextTree::from_context(child_context(3), checker.relation)
                        };
                        assert_eq!(*actual_context, expected);
                    }
                }
            }
        }
        Ok(())
    })
}

#[test]
fn ordinary_lazy_comparisons_keep_nonterminal_constraints() -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, |checker| {
        checker.typevar_evaluation = crate::types::relation::TypeVarEvaluation::Lazy;
        let variables = ["T", "U"].map(|name| {
            BoundTypeVarInstance::synthetic(
                &db,
                checker.env,
                Name::new_static(name),
                TypeVarVariance::Invariant,
            )
        });
        let source = intersection(&db, &variables.map(Type::TypeVar));
        let target = KnownClass::Int.to_instance(&db, checker.env);
        let expected = source
            .positive_elements_or_object(&db)
            .when_any(&db, checker.constraints, |element| {
                checker.check_type_pair(&db, element, target)
            })
            .or(&db, checker.constraints, || {
                checker.check_type_pair(
                    &db,
                    source.with_expanded_typevars_and_newtypes(&db, checker.env),
                    target,
                )
            });
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        for variable in variables {
            assert!(expected.mentions_typevar(&db, variable));
        }
        for asynchronous in [false, true] {
            let effects = Recording::new(&db, checker);
            let actual = completed(effects.evaluate(source, target, asynchronous))?;
            assert!(actual.ownership_probe_same_set(expected));
        }
        Ok(())
    })
}

#[test]
fn refused_effects_release_retained_state_and_allow_retry() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(&db, &[Type::AlwaysTruthy, Type::AlwaysFalsy]);
    let target = Type::int_literal(1);
    with_checker(&db, |checker| {
        checker.context_tree = Some(ErrorContextTree::new(checker.relation));
        let recording = || {
            let mut effects = Recording::new(&db, checker);
            effects.finite_result = Some(None);
            effects.pair_results = Some(vec![checker.never(); 3]);
            effects.pair_contexts = vec![
                Some(child_context(1)),
                Some(child_context(2)),
                Some(child_context(3)),
            ];
            effects.expansion_eligible = Some(true);
            effects.expanded = Some(Type::int_literal(2));
            effects
        };
        let successful = recording();
        let expected = completed(successful.evaluate(source, target, false))?;
        let events = successful.events.into_inner();
        assert_eq!(events.last(), Some(&Event::Report));
        for asynchronous in [false, true] {
            for index in 0..events.len() {
                if let Some(context) = checker.report_context() {
                    context.take();
                }
                let mut effects = recording();
                effects.refuse_at = Some(index);
                assert!(
                    matches!(effects.evaluate(source, target, asynchronous), Err(Refused::Effect(actual)) if actual == index)
                );
                assert_eq!(*effects.events.borrow(), events[..=index]);
                assert_eq!(effects.live_folds.get(), 0);
                assert_eq!(effects.live_contexts.get(), 0);
                if let Some(context) = checker.report_context() {
                    context.take();
                }
                let retry = recording();
                assert!(
                    completed(retry.evaluate(source, target, asynchronous))?
                        .ownership_probe_same_set(expected)
                );
                assert_eq!(*retry.events.borrow(), events);
                assert_eq!(retry.live_folds.get(), 0);
                assert_eq!(retry.live_contexts.get(), 0);
            }
        }
        Ok(())
    })
}

#[test]
fn suspended_child_keeps_state_without_borrowing_storage_and_drop_allows_retry()
-> anyhow::Result<()> {
    let db = setup_db();
    let positive = [Type::AlwaysTruthy, Type::AlwaysFalsy];
    let source = intersection(&db, &positive);
    let target = Type::object();
    with_checker(&db, |checker| {
        checker.context_tree = Some(ErrorContextTree::new(checker.relation));
        let [a, b, ..] = constraints(&db, checker.constraints);
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![a, b]);
        effects.pair_contexts = vec![Some(child_context(1)), Some(child_context(2))];
        effects.expansion_eligible = Some(false);
        effects.pause_before.set(Some(positive[1]));
        {
            let mut future = pin!(check_source_intersection_with(source, target, &effects));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(effects.pairs.get(), 1);
            assert_eq!(effects.live_folds.get(), 1);
            assert_eq!(effects.live_contexts.get(), 1);
            assert!(
                !a.or(&db, checker.constraints, || b)
                    .is_trivially_always_satisfied()
            );
            assert!(
                checker
                    .report_context()
                    .is_some_and(ErrorContextTree::is_empty)
            );
        }
        assert_eq!(effects.live_folds.get(), 0);
        assert_eq!(effects.live_contexts.get(), 0);
        let mut retry = Recording::new(&db, checker);
        retry.pair_results = Some(vec![a, b]);
        retry.expansion_eligible = Some(false);
        let expected = fold(&db, checker.constraints, &[a, b])
            .or(&db, checker.constraints, || checker.never());
        assert!(
            completed(retry.evaluate(source, target, true))?.ownership_probe_same_set(expected)
        );
        Ok(())
    })
}
