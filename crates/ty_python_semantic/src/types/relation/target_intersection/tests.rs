use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;

use super::*;
use crate::FxOrderSet;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::{ConstraintSetBuilder, IteratorConstraintsExtension};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor, TypeVarEvaluation};
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, ErrorContextTree, KnownClass, TypeVarVariance,
};

fn intersection<'db>(
    db: &'db TestDb,
    positive: &[Type<'db>],
    negative: &[Type<'db>],
) -> IntersectionType<'db> {
    IntersectionType::new(
        db,
        FxOrderSet::from_iter(positive.iter().copied()),
        match negative {
            [] => NegativeIntersectionElements::Empty,
            [ty] => NegativeIntersectionElements::Single(*ty),
            _ => NegativeIntersectionElements::Multiple(FxOrderSet::from_iter(
                negative.iter().copied(),
            )),
        },
    )
}

fn with_checker<'db>(
    db: &'db TestDb,
    relation: TypeRelation,
    check: impl FnOnce(&TypeRelationChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let relations = HasRelationToVisitor::default(&constraints);
    let disjointness = IsDisjointVisitor::default(&constraints);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    check(&TypeRelationChecker::new(
        &env,
        relation,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    PositiveElements,
    NegativeElements,
    Next,
    Positive(Type<'db>, Type<'db>),
    HasContext,
    IsNever,
    Context(Type<'db>, Type<'db>, IntersectionType<'db>),
    FoldStart,
    FoldPush,
    FoldFinish,
    IsTriviallyNever,
    Bottom(Type<'db>),
    Disjoint(Type<'db>, Type<'db>),
    Conjoin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refused {
    Effect(usize),
    MissingInput,
    UnexpectedPending,
}

struct RetainedFold<'state, 'db, 'c> {
    inner: ConstraintFold<'db, 'c>,
    live: &'state Cell<usize>,
}

impl Drop for RetainedFold<'_, '_, '_> {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}

struct Recording<'db, 'check, 'a, 'c> {
    inline: InlineTargetIntersectionEffects<'db, 'check, 'a, 'c>,
    events: RefCell<Vec<Event<'db>>>,
    positive_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    negative_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    positives: Cell<usize>,
    negatives: Cell<usize>,
    live_folds: Cell<usize>,
    pause_before: Cell<Option<Type<'db>>>,
    refuse_at: Option<usize>,
}

impl<'db, 'check, 'a, 'c> Recording<'db, 'check, 'a, 'c> {
    fn new(db: &'db TestDb, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self {
            inline: InlineTargetIntersectionEffects::new(db, checker),
            events: RefCell::default(),
            positive_results: None,
            negative_results: None,
            positives: Cell::new(0),
            negatives: Cell::new(0),
            live_folds: Cell::new(0),
            pause_before: Cell::new(None),
            refuse_at: None,
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
        source: Type<'db>,
        target: IntersectionType<'db>,
        asynchronous: bool,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        let relation = self.inline.checker.relation;
        if !asynchronous {
            return check_target_intersection_sync(source, target, relation, self);
        }
        match pin!(check_target_intersection_with(
            source, target, relation, self
        ))
        .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => Err(Refused::UnexpectedPending),
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

impl<'c, 'db: 'c> SynchronousTargetIntersectionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = Elements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = RetainedFold<'state, 'db, 'c>
    where
        Self: 'state;

    recording_methods! {
        fn positive_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_> => Event::PositiveElements;
        fn negative_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_> => Event::NegativeElements;
        fn has_context() -> bool => Event::HasContext;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsNever;
        fn report_context(source: Type<'db>, positive: Type<'db>, intersection: IntersectionType<'db>) -> () => Event::Context(source, positive, intersection);
        fn is_trivially_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsTriviallyNever;
        fn bottom_materialization(ty: Type<'db>) -> Type<'db> => Event::Bottom(ty);
        fn conjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c> => Event::Conjoin;
    }

    fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::Next)?;
        Ok(infallible(self.inline.next_element(elements)))
    }

    fn positive_pair(
        &self,
        source: Type<'db>,
        positive: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Positive(source, positive))?;
        let index = self.positives.replace(self.positives.get() + 1);
        match &self.positive_results {
            Some(values) => values.get(index).copied().ok_or(Refused::MissingInput),
            None => Ok(infallible(self.inline.positive_pair(source, positive))),
        }
    }

    fn disjoint_pair(
        &self,
        source: Type<'db>,
        negative: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Disjoint(source, negative))?;
        let index = self.negatives.replace(self.negatives.get() + 1);
        match &self.negative_results {
            Some(values) => values.get(index).copied().ok_or(Refused::MissingInput),
            None => Ok(infallible(self.inline.disjoint_pair(source, negative))),
        }
    }

    fn fold_start(&self) -> Result<Self::Fold<'_>, Refused> {
        self.record(Event::FoldStart)?;
        self.live_folds.set(self.live_folds.get() + 1);
        Ok(RetainedFold {
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
}

macro_rules! asynchronous_methods {
    ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty;)*) => {
        $(async fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
            SynchronousTargetIntersectionEffects::$name(self, $($argument),*)
        })*
    };
}

impl<'c, 'db: 'c> TargetIntersectionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = Elements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = RetainedFold<'state, 'db, 'c>
    where
        Self: 'state;

    asynchronous_methods! {
        fn positive_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_>;
        fn negative_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_>;
        fn has_context() -> bool;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
        fn report_context(source: Type<'db>, positive: Type<'db>, intersection: IntersectionType<'db>) -> ();
        fn is_trivially_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
        fn bottom_materialization(ty: Type<'db>) -> Type<'db>;
        fn disjoint_pair(source: Type<'db>, negative: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn conjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c>;
        fn fold_start() -> Self::Fold<'_>;
    }

    async fn positive_pair(
        &self,
        source: Type<'db>,
        positive: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if self.pause_before.get() == Some(positive) {
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
        SynchronousTargetIntersectionEffects::positive_pair(self, source, positive)
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        SynchronousTargetIntersectionEffects::next_element(self, elements)
    }

    async fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        SynchronousTargetIntersectionEffects::fold_push(self, fold, next)
    }

    async fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        SynchronousTargetIntersectionEffects::fold_finish(self, fold)
    }
}

fn constraints<'db, 'c>(
    db: &'db TestDb,
    builder: &'c ConstraintSetBuilder<'db>,
) -> [ConstraintSet<'db, 'c>; 6] {
    let env = db.program_environment();
    let int = KnownClass::Int.to_instance(db, &env);
    ["A", "B", "C", "D", "E", "F"].map(|name| {
        let variable = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static(name),
            TypeVarVariance::Invariant,
        );
        ConstraintSet::constrain_typevar_equivalence_bound(db, &env, builder, variable, int)
    })
}

#[test]
fn ordered_phases_materialize_only_for_assignability() -> anyhow::Result<()> {
    let db = setup_db();
    let source = Type::any();
    let positive = [Type::AlwaysTruthy, Type::literal_string()];
    let negative = [Type::unknown(), Type::bool_literal(false)];
    for relation in [
        TypeRelation::Assignability,
        TypeRelation::Subtyping,
        TypeRelation::Redundancy { pure: true },
        TypeRelation::Redundancy { pure: false },
        TypeRelation::SubtypingAssuming,
    ] {
        with_checker(&db, relation, |checker| {
            for (positive, negative) in [(positive.as_slice(), negative.as_slice()), (&[], &[])] {
                let target = intersection(&db, positive, negative);
                let mut expected = vec![Event::PositiveElements, Event::FoldStart];
                for &element in positive {
                    expected.extend([
                        Event::Next,
                        Event::Positive(source, element),
                        Event::HasContext,
                        Event::FoldPush,
                    ]);
                }
                expected.extend([Event::Next, Event::FoldFinish, Event::IsTriviallyNever]);
                let materialized = if relation.is_assignability() {
                    expected.push(Event::Bottom(source));
                    source.bottom_materialization(&db, checker.env)
                } else {
                    source
                };
                expected.extend([Event::NegativeElements, Event::FoldStart]);
                for &element in negative {
                    expected.push(Event::Next);
                    let element = if relation.is_assignability() {
                        expected.push(Event::Bottom(element));
                        element.bottom_materialization(&db, checker.env)
                    } else {
                        element
                    };
                    expected.extend([Event::Disjoint(materialized, element), Event::FoldPush]);
                }
                expected.extend([Event::Next, Event::FoldFinish, Event::Conjoin]);
                for asynchronous in [false, true] {
                    let mut effects = Recording::new(&db, checker);
                    effects.positive_results = Some(vec![checker.always(); positive.len()]);
                    effects.negative_results = Some(vec![checker.always(); negative.len()]);
                    assert!(
                        completed(effects.evaluate(source, target, asynchronous))?
                            .is_trivially_always_satisfied()
                    );
                    assert_eq!(*effects.events.borrow(), expected);
                    assert_eq!(effects.live_folds.get(), 0);
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn separate_folds_preserve_constraint_identity_and_source_history() -> anyhow::Result<()> {
    fn fold<'db, 'c>(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        values: &[ConstraintSet<'db, 'c>],
    ) -> ConstraintSet<'db, 'c> {
        values.iter().copied().when_all(db, builder, |value| value)
    }

    let db = setup_db();
    let elements = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let target = intersection(&db, &elements, &elements);
    with_checker(&db, TypeRelation::Subtyping, |checker| {
        let values = constraints(&db, checker.constraints);
        let expected =
            fold(&db, checker.constraints, &values[..3]).and(&db, checker.constraints, || {
                fold(&db, checker.constraints, &values[3..])
            });
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        assert!(!expected.ownership_probe_same_set(fold(&db, checker.constraints, &values)));
        let reversed = [
            values[2], values[1], values[0], values[5], values[4], values[3],
        ];
        let reverse_result =
            fold(&db, checker.constraints, &reversed[..3]).and(&db, checker.constraints, || {
                fold(&db, checker.constraints, &reversed[3..])
            });
        assert!(!expected.ownership_probe_same_set(reverse_result));
        for (values, expected) in [(values, expected), (reversed, reverse_result)] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.positive_results = Some(values[..3].to_vec());
                effects.negative_results = Some(values[3..].to_vec());
                let actual = completed(effects.evaluate(Type::object(), target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(expected));
                assert_eq!(
                    effects
                        .events
                        .borrow()
                        .iter()
                        .filter(|event| **event == Event::FoldStart)
                        .count(),
                    2
                );
            }
        }
        Ok(())
    })
}

#[test]
fn saturated_positives_skip_the_remaining_elements_and_negative_phase() -> anyhow::Result<()> {
    let db = setup_db();
    let elements = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let target = intersection(&db, &elements, &[Type::unknown()]);
    with_checker(&db, TypeRelation::Assignability, |checker| {
        let [a, ..] = constraints(&db, checker.constraints);
        for (values, visited) in [
            (vec![checker.never()], 1),
            (vec![a, a.negate(&db, checker.constraints)], 2),
        ] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.positive_results = Some(values.clone());
                assert!(
                    completed(effects.evaluate(Type::any(), target, asynchronous))?
                        .is_trivially_never_satisfied()
                );
                assert_eq!(effects.positives.get(), visited);
                assert_eq!(effects.negatives.get(), 0);
                assert_eq!(
                    effects.events.borrow().last(),
                    Some(&Event::IsTriviallyNever)
                );
                assert!(!effects.events.borrow().iter().any(|event| matches!(
                    event,
                    Event::NegativeElements | Event::Bottom(_) | Event::Conjoin | Event::FoldFinish
                )));
                assert_eq!(effects.live_folds.get(), 0);
            }
        }
        Ok(())
    })
}

#[test]
fn full_unsatisfiability_reports_context_without_skipping_negative_work() -> anyhow::Result<()> {
    let db = setup_db();
    let source = Type::object();
    let positive = Type::AlwaysTruthy;
    let target = intersection(&db, &[positive], &[Type::AlwaysFalsy]);
    with_checker(&db, TypeRelation::Subtyping, |checker| {
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
        assert!(impossible.is_never_satisfied(&db, checker.env));
        assert!(!impossible.is_trivially_never_satisfied());
        for enabled in [None, Some(false), Some(true)] {
            for asynchronous in [false, true] {
                let mut checker = checker.clone();
                checker.context_tree = enabled.map(|enabled| {
                    let tree = ErrorContextTree::new(checker.relation);
                    tree.set_enabled(enabled);
                    tree
                });
                let mut effects = Recording::new(&db, &checker);
                effects.positive_results = Some(vec![impossible]);
                effects.negative_results = Some(vec![checker.always()]);
                let result = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(result.ownership_probe_same_set(impossible));
                let events = effects.events.borrow();
                assert_eq!(events.contains(&Event::IsNever), enabled == Some(true));
                assert_eq!(
                    events.contains(&Event::Context(source, positive, target)),
                    enabled == Some(true)
                );
                assert!(events.contains(&Event::NegativeElements));
                assert_eq!(events.last(), Some(&Event::Conjoin));
                if let Some(context) = checker.report_context() {
                    let expected = ErrorContextTree::from_context(
                        ErrorContext::NotAssignableToIntersectionElement {
                            source,
                            element: positive,
                            intersection: Type::Intersection(target),
                        },
                        checker.relation,
                    );
                    assert_eq!(*context, expected);
                    assert_eq!(
                        &events[4..7],
                        [
                            Event::HasContext,
                            Event::IsNever,
                            Event::Context(source, positive, target)
                        ]
                    );
                    assert_eq!(events[7], Event::FoldPush);
                }
            }
        }
        Ok(())
    })
}

#[test]
fn ordinary_positive_children_preserve_lazy_inference_and_builder() -> anyhow::Result<()> {
    let db = setup_db();
    with_checker(&db, TypeRelation::Subtyping, |checker| {
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            checker.env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let mut checker = checker.clone();
        checker.inferable = TypeVarSet::from_typevars(&db, [variable]);
        checker.typevar_evaluation = TypeVarEvaluation::Lazy;
        let source = KnownClass::Int.to_instance(&db, checker.env);
        let target = intersection(&db, &[Type::TypeVar(variable)], &[]);
        let expected = target
            .positive(&db)
            .iter()
            .when_all(&db, checker.constraints, |&positive| {
                checker.check_type_pair(&db, source, positive)
            })
            .and(&db, checker.constraints, || checker.always());
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        assert!(expected.mentions_typevar(&db, variable));
        for asynchronous in [false, true] {
            let effects = Recording::new(&db, &checker);
            let result = completed(effects.evaluate(source, target, asynchronous))?;
            assert!(result.ownership_probe_same_set(expected));
            assert!(result.mentions_typevar(&db, variable));
        }
        Ok(())
    })
}

#[test]
fn every_reached_effect_refuses_without_advancing_and_allows_retry() -> anyhow::Result<()> {
    let db = setup_db();
    let source = Type::any();
    let target = intersection(&db, &[Type::object()], &[Type::unknown()]);
    with_checker(&db, TypeRelation::Assignability, |checker| {
        let successful = Recording::new(&db, checker);
        let expected = completed(successful.evaluate(source, target, false))?;
        let events = successful.events.into_inner();
        assert_eq!(events.last(), Some(&Event::Conjoin));
        for asynchronous in [false, true] {
            for index in 0..events.len() {
                let mut effects = Recording::new(&db, checker);
                effects.refuse_at = Some(index);
                assert!(
                    matches!(effects.evaluate(source, target, asynchronous), Err(Refused::Effect(actual)) if actual == index)
                );
                assert_eq!(*effects.events.borrow(), events[..=index]);
                assert_eq!(effects.live_folds.get(), 0);
                let retry = Recording::new(&db, checker);
                assert!(
                    completed(retry.evaluate(source, target, asynchronous))?
                        .ownership_probe_same_set(expected)
                );
            }
        }
        Ok(())
    })
}

#[test]
fn pending_positive_retains_the_fold_without_borrowing_constraint_storage() -> anyhow::Result<()> {
    let db = setup_db();
    let positive = [Type::AlwaysTruthy, Type::AlwaysFalsy];
    let target = intersection(&db, &positive, &[]);
    with_checker(&db, TypeRelation::Subtyping, |checker| {
        let [a, b, ..] = constraints(&db, checker.constraints);
        let mut effects = Recording::new(&db, checker);
        effects.positive_results = Some(vec![a, b]);
        effects.pause_before.set(Some(positive[1]));
        {
            let mut future = pin!(check_target_intersection_with(
                Type::object(),
                target,
                checker.relation,
                &effects
            ));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(effects.positives.get(), 1);
            assert_eq!(effects.live_folds.get(), 1);
            let while_pending = a.and(&db, checker.constraints, || b);
            assert!(!while_pending.is_trivially_never_satisfied());
        }
        assert_eq!(effects.live_folds.get(), 0);
        let mut retry = Recording::new(&db, checker);
        retry.positive_results = Some(vec![a, b]);
        let expected = a.and(&db, checker.constraints, || b);
        assert!(
            completed(retry.evaluate(Type::object(), target, true))?
                .ownership_probe_same_set(expected)
        );
        Ok(())
    })
}
