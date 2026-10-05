use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_python_ast::name::Name;

use super::*;
use crate::FxOrderSet;
use crate::db::tests::{TestDb, setup_db};
use crate::types::constraints::{ConstraintSetBuilder, IteratorConstraintsExtension};
use crate::types::relation::{HasRelationToVisitor, IsDisjointVisitor};
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{ApplyTypeMappingVisitor, BoundTypeVarInstance, KnownClass, TypeVarVariance};

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
    check: impl FnOnce(&mut DisjointnessChecker<'_, '_, 'db>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let env = db.program_environment();
    let constraints = ConstraintSetBuilder::new();
    let relations = HasRelationToVisitor::default(&constraints);
    let disjointness = IsDisjointVisitor::default(&constraints);
    let signatures = SignatureRelationVisitor::default();
    let mapping = ApplyTypeMappingVisitor::new(&env);
    check(&mut DisjointnessChecker::new(
        &env,
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

fn operand_cases<'db>(
    left: IntersectionType<'db>,
    right: IntersectionType<'db>,
    other: Type<'db>,
) -> [(Type<'db>, Type<'db>, DisjointIntersectionOperands<'db>); 3] {
    [
        (
            Type::Intersection(left),
            Type::Intersection(right),
            DisjointIntersectionOperands::Both { left, right },
        ),
        (
            Type::Intersection(left),
            other,
            DisjointIntersectionOperands::Left {
                intersection: left,
                other,
            },
        ),
        (
            other,
            Type::Intersection(left),
            DisjointIntersectionOperands::Right {
                intersection: left,
                other,
            },
        ),
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Finite(IntersectionType<'db>),
    Guard(Type<'db>, Type<'db>),
    PositiveElements(IntersectionType<'db>),
    NegativeElements(IntersectionType<'db>),
    Next,
    Disjoint(Type<'db>, Type<'db>),
    Subtyping(Type<'db>, Type<'db>),
    FoldStart,
    FoldPush,
    FoldFinish,
    IsTriviallyAlways,
    Disjoin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refused {
    MissingInput,
    UnexpectedPending,
}

struct Recording<'db, 'check, 'a, 'c> {
    inline: InlineDisjointIntersectionEffects<'db, 'check, 'a, 'c>,
    events: RefCell<Vec<Event<'db>>>,
    finite_results: Option<Vec<Option<Type<'db>>>>,
    disjoint_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    subtyping_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    finite: Cell<usize>,
    disjoint: Cell<usize>,
    subtyping: Cell<usize>,
}

impl<'db, 'check, 'a, 'c> Recording<'db, 'check, 'a, 'c> {
    fn new(db: &'db TestDb, checker: &'check DisjointnessChecker<'a, 'c, 'db>) -> Self {
        Self {
            inline: InlineDisjointIntersectionEffects::new(db, checker),
            events: RefCell::default(),
            finite_results: None,
            disjoint_results: None,
            subtyping_results: None,
            finite: Cell::new(0),
            disjoint: Cell::new(0),
            subtyping: Cell::new(0),
        }
    }

    fn record(&self, event: Event<'db>) {
        self.events.borrow_mut().push(event);
    }

    fn evaluate(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
        asynchronous: bool,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if asynchronous {
            ready(check_disjoint_intersection_with(
                left, right, operands, self,
            ))
        } else {
            check_disjoint_intersection_sync(left, right, operands, self)
        }
    }

    fn structural(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
        asynchronous: bool,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if asynchronous {
            ready(check_disjoint_intersection_structural_with(
                left, right, operands, self,
            ))
        } else {
            check_disjoint_intersection_structural_sync(left, right, operands, self)
        }
    }

    fn use_results(
        &mut self,
        operands: DisjointIntersectionOperands<'db>,
        first: &[ConstraintSet<'db, 'c>],
        second: &[ConstraintSet<'db, 'c>],
    ) {
        if matches!(operands, DisjointIntersectionOperands::Both { .. }) {
            self.disjoint_results = Some(first.iter().chain(second).copied().collect());
        } else {
            self.disjoint_results = Some(first.to_vec());
            self.subtyping_results = Some(second.to_vec());
        }
    }
}

macro_rules! recording_methods {
    ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty => $event:expr;)*) => {
        $(fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
            self.record($event);
            Ok(infallible(self.inline.$name($($argument),*)))
        })*
    };
}

impl<'c, 'db: 'c> SynchronousDisjointIntersectionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = Elements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;

    recording_methods! {
        fn positive_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_> => Event::PositiveElements(intersection);
        fn negative_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_> => Event::NegativeElements(intersection);
        fn fold_start() -> Self::Fold<'_> => Event::FoldStart;
        fn is_trivially_always_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsTriviallyAlways;
        fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c> => Event::Disjoin;
    }

    fn finite_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::Finite(intersection));
        let index = self.finite.replace(self.finite.get() + 1);
        match &self.finite_results {
            Some(values) => values.get(index).copied().ok_or(Refused::MissingInput),
            None => Ok(infallible(self.inline.finite_alternatives(intersection))),
        }
    }

    fn disjoint_pair(
        &self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Disjoint(left, right));
        let index = self.disjoint.replace(self.disjoint.get() + 1);
        match &self.disjoint_results {
            Some(values) => values.get(index).copied().ok_or(Refused::MissingInput),
            None => Ok(infallible(self.inline.disjoint_pair(left, right))),
        }
    }

    fn guarded_structural(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Guard(left, right));
        check_disjoint_intersection_structural_sync(left, right, operands, self)
    }

    fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        self.record(Event::Next);
        Ok(infallible(self.inline.next_element(elements)))
    }

    fn subtyping_pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Subtyping(source, target));
        let index = self.subtyping.replace(self.subtyping.get() + 1);
        match &self.subtyping_results {
            Some(values) => values.get(index).copied().ok_or(Refused::MissingInput),
            None => Ok(infallible(self.inline.subtyping_pair(source, target))),
        }
    }

    fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        self.record(Event::FoldPush);
        Ok(infallible(self.inline.fold_push(fold, next)))
    }

    fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::FoldFinish);
        Ok(infallible(self.inline.fold_finish(fold)))
    }
}

macro_rules! asynchronous_methods {
    ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty;)*) => {
        $(async fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
            SynchronousDisjointIntersectionEffects::$name(self, $($argument),*)
        })*
    };
}

impl<'c, 'db: 'c> DisjointIntersectionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = Elements<'db>
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;

    asynchronous_methods! {
        fn finite_alternatives(intersection: IntersectionType<'db>) -> Option<Type<'db>>;
        fn disjoint_pair(left: Type<'db>, right: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn positive_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_>;
        fn negative_elements(intersection: IntersectionType<'db>) -> Self::Elements<'_>;
        fn subtyping_pair(source: Type<'db>, target: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn fold_start() -> Self::Fold<'_>;
        fn is_trivially_always_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
        fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c>;
    }

    async fn guarded_structural(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        operands: DisjointIntersectionOperands<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::Guard(left, right));
        check_disjoint_intersection_structural_with(left, right, operands, self).await
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        SynchronousDisjointIntersectionEffects::next_element(self, elements)
    }

    async fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        SynchronousDisjointIntersectionEffects::fold_push(self, fold, next)
    }

    async fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        SynchronousDisjointIntersectionEffects::fold_finish(self, fold)
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

fn fold<'db, 'c>(
    db: &'db dyn Db,
    builder: &'c ConstraintSetBuilder<'db>,
    values: &[ConstraintSet<'db, 'c>],
) -> ConstraintSet<'db, 'c> {
    values.iter().copied().when_any(db, builder, |value| value)
}

#[test]
fn finite_alternatives_preserve_left_precedence_and_operand_order() -> anyhow::Result<()> {
    let db = setup_db();
    let left = intersection(&db, &[Type::AlwaysTruthy], &[]);
    let right = intersection(&db, &[Type::AlwaysFalsy], &[]);
    let left_type = Type::Intersection(left);
    let right_type = Type::Intersection(right);
    let other = Type::object();
    let alternative = Type::bool_literal(true);
    with_checker(&db, |checker| {
        let [result, ..] = constraints(&db, checker.constraints);
        for (left_type, right_type, operands, alternatives, expected) in [
            (
                left_type,
                right_type,
                DisjointIntersectionOperands::Both { left, right },
                vec![Some(alternative), Some(Type::bool_literal(false))],
                vec![
                    Event::Finite(left),
                    Event::Disjoint(alternative, right_type),
                ],
            ),
            (
                left_type,
                right_type,
                DisjointIntersectionOperands::Both { left, right },
                vec![None, Some(alternative)],
                vec![
                    Event::Finite(left),
                    Event::Finite(right),
                    Event::Disjoint(left_type, alternative),
                ],
            ),
            (
                left_type,
                other,
                DisjointIntersectionOperands::Left {
                    intersection: left,
                    other,
                },
                vec![Some(alternative)],
                vec![Event::Finite(left), Event::Disjoint(alternative, other)],
            ),
            (
                other,
                right_type,
                DisjointIntersectionOperands::Right {
                    intersection: right,
                    other,
                },
                vec![Some(alternative)],
                vec![Event::Finite(right), Event::Disjoint(other, alternative)],
            ),
        ] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.finite_results = Some(alternatives.clone());
                effects.disjoint_results = Some(vec![result]);
                let actual =
                    completed(effects.evaluate(left_type, right_type, operands, asynchronous))?;
                assert!(actual.ownership_probe_same_set(result));
                assert_eq!(*effects.events.borrow(), expected);
            }
        }
        Ok(())
    })
}

#[test]
fn structural_phases_preserve_element_and_operand_order() -> anyhow::Result<()> {
    let db = setup_db();
    let positive = [Type::AlwaysTruthy, Type::literal_string()];
    let negative = [Type::unknown(), Type::bool_literal(false)];
    let right_positive = [Type::AlwaysFalsy, Type::bool_literal(true)];
    with_checker(&db, |checker| {
        for (positive, negative, right_positive) in [
            (
                positive.as_slice(),
                negative.as_slice(),
                right_positive.as_slice(),
            ),
            (&[], &[], &[]),
        ] {
            let left_intersection = intersection(&db, positive, negative);
            let right_intersection = intersection(&db, right_positive, &[]);
            for (left, right, operands) in
                operand_cases(left_intersection, right_intersection, Type::object())
            {
                let both = matches!(operands, DisjointIntersectionOperands::Both { .. });
                let other = if both { right } else { Type::object() };
                let mut expected = vec![Event::Finite(left_intersection)];
                if both {
                    expected.push(Event::Finite(right_intersection));
                }
                expected.extend([
                    Event::Guard(left, right),
                    Event::PositiveElements(left_intersection),
                    Event::FoldStart,
                ]);
                for &element in positive {
                    expected.extend([
                        Event::Next,
                        Event::Disjoint(element, other),
                        Event::FoldPush,
                    ]);
                }
                expected.extend([Event::Next, Event::FoldFinish, Event::IsTriviallyAlways]);
                expected.push(if both {
                    Event::PositiveElements(right_intersection)
                } else {
                    Event::NegativeElements(left_intersection)
                });
                expected.push(Event::FoldStart);
                for &element in if both { right_positive } else { negative } {
                    expected.extend([
                        Event::Next,
                        if both {
                            Event::Disjoint(element, left)
                        } else {
                            Event::Subtyping(other, element)
                        },
                        Event::FoldPush,
                    ]);
                }
                expected.extend([Event::Next, Event::FoldFinish, Event::Disjoin]);
                for asynchronous in [false, true] {
                    let mut effects = Recording::new(&db, checker);
                    effects.finite_results = Some(vec![None; if both { 2 } else { 1 }]);
                    effects.use_results(
                        operands,
                        &vec![checker.never(); positive.len()],
                        &vec![
                            checker.never();
                            if both {
                                right_positive.len()
                            } else {
                                negative.len()
                            }
                        ],
                    );
                    assert!(
                        completed(effects.evaluate(left, right, operands, asynchronous))?
                            .ownership_probe_same_set(checker.never())
                    );
                    assert_eq!(*effects.events.borrow(), expected);
                }
            }
        }
        Ok(())
    })
}

#[test]
fn separate_folds_preserve_constraint_identity_and_source_history() -> anyhow::Result<()> {
    let db = setup_db();
    let elements = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let intersection = intersection(&db, &elements, &elements);
    with_checker(&db, |checker| {
        let values = constraints(&db, checker.constraints);
        let expected =
            fold(&db, checker.constraints, &values[..3]).or(&db, checker.constraints, || {
                fold(&db, checker.constraints, &values[3..])
            });
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        assert!(!expected.ownership_probe_same_set(fold(&db, checker.constraints, &values)));
        let reversed = [
            values[2], values[1], values[0], values[5], values[4], values[3],
        ];
        let reverse_result =
            fold(&db, checker.constraints, &reversed[..3]).or(&db, checker.constraints, || {
                fold(&db, checker.constraints, &reversed[3..])
            });
        assert!(!expected.ownership_probe_same_set(reverse_result));
        for (values, expected) in [(values, expected), (reversed, reverse_result)] {
            for (left, right, operands) in operand_cases(intersection, intersection, Type::object())
            {
                for asynchronous in [false, true] {
                    let mut effects = Recording::new(&db, checker);
                    effects.use_results(operands, &values[..3], &values[3..]);
                    let actual =
                        completed(effects.structural(left, right, operands, asynchronous))?;
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
        }
        Ok(())
    })
}

#[test]
fn saturated_first_fold_skips_remaining_elements_and_second_fold() -> anyhow::Result<()> {
    let db = setup_db();
    let elements = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let intersection = intersection(&db, &elements, &elements);
    with_checker(&db, |checker| {
        let [a, ..] = constraints(&db, checker.constraints);
        for values in [
            vec![checker.always()],
            vec![a, a.negate(&db, checker.constraints)],
        ] {
            let expected = fold(&db, checker.constraints, &values);
            for (left, right, operands) in operand_cases(intersection, intersection, Type::object())
            {
                for asynchronous in [false, true] {
                    let mut effects = Recording::new(&db, checker);
                    effects.use_results(operands, &values, &[]);
                    let actual =
                        completed(effects.structural(left, right, operands, asynchronous))?;
                    assert!(actual.ownership_probe_same_set(expected));
                    assert!(actual.is_trivially_always_satisfied());
                    assert_eq!(effects.disjoint.get(), values.len());
                    assert_eq!(effects.subtyping.get(), 0);
                    let events = effects.events.borrow();
                    assert_eq!(events.last(), Some(&Event::IsTriviallyAlways));
                    assert_eq!(
                        events
                            .iter()
                            .filter(|event| **event == Event::FoldStart)
                            .count(),
                        1
                    );
                    assert!(!events.iter().any(|event| matches!(
                        event,
                        Event::NegativeElements(_) | Event::FoldFinish | Event::Disjoin
                    )));
                }
            }
        }
        Ok(())
    })
}

#[test]
fn saturated_second_fold_skips_its_remaining_elements() -> anyhow::Result<()> {
    let db = setup_db();
    let elements = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let intersection = intersection(&db, &elements, &elements);
    with_checker(&db, |checker| {
        let [a, ..] = constraints(&db, checker.constraints);
        let first = [checker.never(); 3];
        for second in [
            vec![checker.always()],
            vec![a, a.negate(&db, checker.constraints)],
        ] {
            let expected =
                fold(&db, checker.constraints, &first).or(&db, checker.constraints, || {
                    fold(&db, checker.constraints, &second)
                });
            for (left, right, operands) in operand_cases(intersection, intersection, Type::object())
            {
                for asynchronous in [false, true] {
                    let mut effects = Recording::new(&db, checker);
                    effects.use_results(operands, &first, &second);
                    let actual =
                        completed(effects.structural(left, right, operands, asynchronous))?;
                    assert!(actual.ownership_probe_same_set(expected));
                    assert!(actual.is_trivially_always_satisfied());
                    assert_eq!(
                        effects.disjoint.get() + effects.subtyping.get(),
                        first.len() + second.len()
                    );
                    let events = effects.events.borrow();
                    assert_eq!(events.last(), Some(&Event::Disjoin));
                    assert_eq!(
                        events
                            .iter()
                            .filter(|event| **event == Event::FoldStart)
                            .count(),
                        2
                    );
                    assert_eq!(
                        events
                            .iter()
                            .filter(|event| **event == Event::FoldFinish)
                            .count(),
                        1
                    );
                }
            }
        }
        Ok(())
    })
}

#[test]
fn ordinary_subtyping_children_preserve_eager_inferable_behavior() -> anyhow::Result<()> {
    let db = setup_db();
    for inferable in [false, true] {
        for intersection_on_left in [false, true] {
            for asynchronous in [false, true] {
                with_checker(&db, |checker| {
                    let variable = BoundTypeVarInstance::synthetic(
                        &db,
                        checker.env,
                        Name::new_static("T"),
                        TypeVarVariance::Invariant,
                    );
                    if inferable {
                        checker.inferable = TypeVarSet::from_typevars(&db, [variable]);
                    }
                    let int = KnownClass::Int.to_instance(&db, checker.env);
                    let other =
                        Type::heterogeneous_tuple(&db, checker.env, [Type::TypeVar(variable)]);
                    let negative = Type::heterogeneous_tuple(&db, checker.env, [int]);
                    let intersection = intersection(&db, &[Type::object()], &[negative]);
                    let (left, right, operands) = if intersection_on_left {
                        (
                            Type::Intersection(intersection),
                            other,
                            DisjointIntersectionOperands::Left {
                                intersection,
                                other,
                            },
                        )
                    } else {
                        (
                            other,
                            Type::Intersection(intersection),
                            DisjointIntersectionOperands::Right {
                                intersection,
                                other,
                            },
                        )
                    };
                    let effects = Recording::new(&db, checker);
                    let actual = completed(effects.evaluate(left, right, operands, asynchronous))?;
                    // Eager subtyping accepts `tuple[T] <: tuple[int]` when `T` is inferable:
                    // its implicit lower bound is `Never`. A fixed unbounded `T` can also
                    // specialize to types that are not subtypes of `int`.
                    assert_eq!(actual.is_trivially_always_satisfied(), inferable);
                    assert_eq!(actual.is_trivially_never_satisfied(), !inferable);
                    assert!(
                        effects
                            .events
                            .borrow()
                            .contains(&Event::Subtyping(other, negative))
                    );

                    let expected = intersection
                        .positive(&db)
                        .iter()
                        .when_any(&db, checker.constraints, |&positive| {
                            checker.check_type_pair(&db, positive, other)
                        })
                        .or(&db, checker.constraints, || {
                            intersection.negative(&db).iter().when_any(
                                &db,
                                checker.constraints,
                                |&negative| {
                                    checker
                                        .as_relation_checker(TypeRelation::Subtyping)
                                        .check_type_pair(&db, other, negative)
                                },
                            )
                        });
                    assert!(actual.ownership_probe_same_set(expected));
                    assert!(
                        checker
                            .check_type_pair(&db, left, right)
                            .ownership_probe_same_set(expected)
                    );
                    Ok(())
                })?;
            }
        }
    }
    Ok(())
}
