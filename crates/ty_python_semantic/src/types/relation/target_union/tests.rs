use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem as _;
use ruff_python_ast::name::Name;
use ty_python_core::ProgramFile;

use super::*;
use crate::FxOrderSet;
use crate::db::tests::{TestDb, setup_db};
use crate::place::global_symbol;
use crate::types::constraints::{ConstraintSetBuilder, IteratorConstraintsExtension};
use crate::types::newtype::NewTypeBase;
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeVarEvaluation,
};
use crate::types::set_theoretic::{NegativeIntersectionElements, RecursivelyDefined};
use crate::types::signatures::SignatureRelationVisitor;
use crate::types::typevar::TypeVarSet;
use crate::types::{ApplyTypeMappingVisitor, KnownClass, KnownInstanceType, TypeVarVariance};

fn union<'db>(db: &'db TestDb, elements: &[Type<'db>]) -> UnionType<'db> {
    UnionType::new(
        db,
        elements.to_vec().into_boxed_slice(),
        RecursivelyDefined::No,
    )
}

fn intersection<'db>(db: &'db TestDb) -> Type<'db> {
    Type::Intersection(IntersectionType::new(
        db,
        FxOrderSet::default(),
        NegativeIntersectionElements::Empty,
    ))
}

fn newtype<'db>(db: &'db TestDb) -> anyhow::Result<Type<'db>> {
    let env = db.program_environment();
    let Some(class) = KnownClass::Int.try_to_class_literal(db, &env) else {
        anyhow::bail!("missing int class");
    };
    Ok(Type::NewTypeInstance(NewType::new(
        db,
        Name::new_static("Wrapped"),
        class.definition(db),
        Some(NewTypeBase::ClassType(class.identity_specialization(db))),
    )))
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
    Elements,
    Count,
    Next,
    IsAliasLike(Type<'db>),
    FoldStart,
    FoldPush,
    FoldFinish,
    IsTriviallyAlways,
    Never,
    Disjoin,
    IsInferable,
    Bounds,
    CheckBounds(Type<'db>),
    ShouldExpand,
    Expand,
    NewtypeBase,
    ContextStart,
    Capture,
    HasCollected,
    IsNever,
    Report(usize),
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
    inline: InlineTargetUnionEffects<'db, 'check, 'a, 'c>,
    events: RefCell<Vec<Event<'db>>>,
    pair_results: Option<Vec<ConstraintSet<'db, 'c>>>,
    pair_contexts: Vec<Option<ErrorContext<'db>>>,
    pairs: Cell<usize>,
    finite_result: Option<Option<Type<'db>>>,
    inferable: Option<bool>,
    bounds: Option<Option<TypeVarBoundOrConstraints<'db>>>,
    bounds_result: Option<ConstraintSet<'db, 'c>>,
    expansion_eligible: Option<bool>,
    expanded: Option<Type<'db>>,
    newtype_base: Option<Type<'db>>,
    live_folds: Cell<usize>,
    live_contexts: Cell<usize>,
    refuse_at: Option<usize>,
    pause_before: Cell<Option<Type<'db>>>,
}

impl<'db, 'check, 'a, 'c> Recording<'db, 'check, 'a, 'c> {
    fn new(db: &'db TestDb, checker: &'check TypeRelationChecker<'a, 'c, 'db>) -> Self {
        Self {
            inline: InlineTargetUnionEffects::new(db, checker),
            events: RefCell::default(),
            pair_results: None,
            pair_contexts: vec![],
            pairs: Cell::new(0),
            finite_result: None,
            inferable: None,
            bounds: None,
            bounds_result: None,
            expansion_eligible: None,
            expanded: None,
            newtype_base: None,
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
        source: Type<'db>,
        target: UnionType<'db>,
        asynchronous: bool,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if asynchronous {
            ready(check_target_union_with(source, target, self))
        } else {
            check_target_union_sync(source, target, self)
        }
    }

    fn evaluate_aliases(
        &self,
        target: UnionType<'db>,
        asynchronous: bool,
    ) -> Result<bool, Refused> {
        if asynchronous {
            ready(union_has_aliases_with(target, self))
        } else {
            union_has_aliases_sync(target, self)
        }
    }

    fn assert_released(&self) {
        assert_eq!(self.live_folds.get(), 0);
        assert_eq!(self.live_contexts.get(), 0);
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

impl<'c, 'db: 'c> SynchronousTargetUnionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = slice::Iter<'db, Type<'db>>
    where
        Self: 'state;
    type Fold<'state>
        = Retained<'state, ConstraintFold<'db, 'c>>
    where
        Self: 'state;
    type Context<'state>
        = Retained<'state, TargetUnionContext<'state, 'db>>
    where
        Self: 'state;

    recording_methods! {
        fn elements(union: UnionType<'db>) -> Self::Elements<'_> => Event::Elements;
        fn element_is_alias_like(element: Type<'db>) -> bool => Event::IsAliasLike(element);
        fn is_trivially_always_satisfied(value: ConstraintSet<'db, 'c>) -> bool => Event::IsTriviallyAlways;
        fn never() -> ConstraintSet<'db, 'c> => Event::Never;
        fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c> => Event::Disjoin;
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

    fn element_count<'state>(
        &'state self,
        elements: &Self::Elements<'state>,
    ) -> Result<usize, Refused> {
        self.record(Event::Count)?;
        Ok(infallible(self.inline.element_count(elements)))
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

    fn typevar_is_inferable(&self, typevar: BoundTypeVarInstance<'db>) -> Result<bool, Refused> {
        self.record(Event::IsInferable)?;
        Ok(self
            .inferable
            .unwrap_or_else(|| infallible(self.inline.typevar_is_inferable(typevar))))
    }

    fn typevar_bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, Refused> {
        self.record(Event::Bounds)?;
        Ok(self
            .bounds
            .unwrap_or_else(|| infallible(self.inline.typevar_bound_or_constraints(typevar))))
    }

    fn source_typevar_bounds(
        &self,
        bounds: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        self.record(Event::CheckBounds(target))?;
        Ok(self
            .bounds_result
            .unwrap_or_else(|| infallible(self.inline.source_typevar_bounds(bounds, target))))
    }

    fn should_expand(&self, intersection: IntersectionType<'db>) -> Result<bool, Refused> {
        self.record(Event::ShouldExpand)?;
        Ok(self
            .expansion_eligible
            .unwrap_or_else(|| infallible(self.inline.should_expand(intersection))))
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

    fn newtype_concrete_base(&self, newtype: NewType<'db>) -> Result<Type<'db>, Refused> {
        self.record(Event::NewtypeBase)?;
        Ok(self
            .newtype_base
            .unwrap_or_else(|| infallible(self.inline.newtype_concrete_base(newtype))))
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
        source: Type<'db>,
        union: UnionType<'db>,
        element_count: usize,
    ) -> Result<(), Refused> {
        self.record(Event::Report(element_count))?;
        Ok(infallible(self.inline.report_context(
            &mut context.inner,
            source,
            union,
            element_count,
        )))
    }
}

macro_rules! asynchronous_methods {
    ($(fn $name:ident($($argument:ident: $argument_ty:ty),* $(,)?) -> $result:ty;)*) => {
        $(async fn $name(&self, $($argument: $argument_ty),*) -> Result<$result, Refused> {
            SynchronousTargetUnionEffects::$name(self, $($argument),*)
        })*
    };
}

impl<'c, 'db: 'c> TargetUnionEffects<'c, 'db> for Recording<'db, '_, '_, 'c> {
    type Error = Refused;
    type Elements<'state>
        = <Self as SynchronousTargetUnionEffects<'c, 'db>>::Elements<'state>
    where
        Self: 'state;
    type Fold<'state>
        = <Self as SynchronousTargetUnionEffects<'c, 'db>>::Fold<'state>
    where
        Self: 'state;
    type Context<'state>
        = <Self as SynchronousTargetUnionEffects<'c, 'db>>::Context<'state>
    where
        Self: 'state;

    asynchronous_methods! {
        fn finite_alternatives(intersection: IntersectionType<'db>) -> Option<Type<'db>>;
        fn elements(union: UnionType<'db>) -> Self::Elements<'_>;
        fn element_is_alias_like(element: Type<'db>) -> bool;
        fn fold_start() -> Self::Fold<'_>;
        fn is_trivially_always_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
        fn never() -> ConstraintSet<'db, 'c>;
        fn disjoin(left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> ConstraintSet<'db, 'c>;
        fn typevar_is_inferable(typevar: BoundTypeVarInstance<'db>) -> bool;
        fn typevar_bound_or_constraints(typevar: BoundTypeVarInstance<'db>) -> Option<TypeVarBoundOrConstraints<'db>>;
        fn source_typevar_bounds(bounds: TypeVarBoundOrConstraints<'db>, target: Type<'db>) -> ConstraintSet<'db, 'c>;
        fn should_expand(intersection: IntersectionType<'db>) -> bool;
        fn expand_intersection(intersection: IntersectionType<'db>) -> Type<'db>;
        fn newtype_concrete_base(newtype: NewType<'db>) -> Type<'db>;
        fn context_start() -> Self::Context<'_>;
        fn is_never_satisfied(value: ConstraintSet<'db, 'c>) -> bool;
    }

    async fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        if self.pause_before.get() == Some(target) {
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
        SynchronousTargetUnionEffects::pair(self, source, target)
    }

    async fn element_count<'state>(
        &'state self,
        elements: &Self::Elements<'state>,
    ) -> Result<usize, Refused> {
        SynchronousTargetUnionEffects::element_count(self, elements)
    }

    async fn next_element<'state>(
        &'state self,
        elements: &mut Self::Elements<'state>,
    ) -> Result<Option<Type<'db>>, Refused> {
        SynchronousTargetUnionEffects::next_element(self, elements)
    }

    async fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Refused> {
        SynchronousTargetUnionEffects::fold_push(self, fold, next)
    }

    async fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> Result<ConstraintSet<'db, 'c>, Refused> {
        SynchronousTargetUnionEffects::fold_finish(self, fold)
    }

    async fn capture_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
    ) -> Result<(), Refused> {
        SynchronousTargetUnionEffects::capture_context(self, context)
    }

    async fn has_collected_context<'state>(
        &'state self,
        context: &Self::Context<'state>,
    ) -> Result<bool, Refused> {
        SynchronousTargetUnionEffects::has_collected_context(self, context)
    }

    async fn report_context<'state>(
        &'state self,
        context: &mut Self::Context<'state>,
        source: Type<'db>,
        union: UnionType<'db>,
        element_count: usize,
    ) -> Result<(), Refused> {
        SynchronousTargetUnionEffects::report_context(self, context, source, union, element_count)
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
fn alias_scan_keeps_element_order_and_stops_before_the_tail() -> anyhow::Result<()> {
    let mut db = setup_db();
    db.write_dedented("/src/alias.py", "type Alias = int")?;
    let file = system_path_to_file(&db, "/src/alias.py")?;
    let file = ProgramFile::new(&db, file, db.program_environment().program(&db));
    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
        global_symbol(&db, file, "Alias").place.expect_type()
    else {
        anyhow::bail!("expected Alias to be a type alias");
    };
    let alias = Type::TypeAlias(alias);
    with_checker(&db, |checker| {
        for (members, expected, visited) in [
            (vec![Type::AlwaysTruthy, Type::AlwaysFalsy], false, 2),
            (vec![alias, Type::AlwaysTruthy], true, 1),
            (vec![Type::AlwaysFalsy, alias, Type::AlwaysTruthy], true, 2),
        ] {
            let target = union(&db, &members);
            assert_eq!(target.has_aliases(&db), expected);
            let mut events = vec![Event::Elements];
            for &element in &members[..visited] {
                events.extend([Event::Next, Event::IsAliasLike(element)]);
            }
            if !expected {
                events.push(Event::Next);
            }
            for asynchronous in [false, true] {
                let effects = Recording::new(&db, checker);
                assert_eq!(
                    completed(effects.evaluate_aliases(target, asynchronous))?,
                    expected
                );
                assert_eq!(*effects.events.borrow(), events);
                effects.assert_released();
                for index in 0..events.len() {
                    let mut refused = Recording::new(&db, checker);
                    refused.refuse_at = Some(index);
                    assert_eq!(
                        refused.evaluate_aliases(target, asynchronous),
                        Err(Refused::Effect(index))
                    );
                    assert_eq!(*refused.events.borrow(), events[..=index]);
                    refused.assert_released();
                    let retry = Recording::new(&db, checker);
                    assert_eq!(
                        completed(retry.evaluate_aliases(target, asynchronous))?,
                        expected
                    );
                    assert_eq!(*retry.events.borrow(), events);
                    retry.assert_released();
                }
            }
        }
        Ok(())
    })
}

#[test]
fn finite_alternatives_return_before_member_or_context_storage() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(&db);
    let target = union(&db, &[Type::AlwaysTruthy, Type::AlwaysFalsy]);
    let alternatives = Type::bool_literal(true);
    with_checker(&db, |checker| {
        let [expected, ..] = constraints(&db, checker.constraints);
        for asynchronous in [false, true] {
            let mut effects = Recording::new(&db, checker);
            effects.finite_result = Some(Some(alternatives));
            effects.pair_results = Some(vec![expected]);
            let actual = completed(effects.evaluate(source, target, asynchronous))?;
            assert!(actual.ownership_probe_same_set(expected));
            assert_eq!(
                *effects.events.borrow(),
                [
                    Event::Finite,
                    Event::Pair(alternatives, Type::Union(target))
                ]
            );
            effects.assert_released();
        }
        Ok(())
    })
}

#[test]
fn members_keep_order_and_expansion_stays_outside_the_fold() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(&db);
    let members = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let target = union(&db, &members);
    let expanded = Type::int_literal(7);
    with_checker(&db, |checker| {
        let values = constraints(&db, checker.constraints);
        let expected =
            fold(&db, checker.constraints, &values[..3]).or(&db, checker.constraints, || values[3]);
        let reversed = [values[2], values[1], values[0], values[3]];
        let reversed_expected =
            fold(&db, checker.constraints, &reversed[..3])
                .or(&db, checker.constraints, || reversed[3]);
        // This probe compares builder identity, the decision node, and the source-order identifier.
        // Reordering or regrouping these equivalent disjunctions changes the source-order identifier.
        assert!(!expected.ownership_probe_same_set(fold(&db, checker.constraints, &values)));
        assert!(!expected.ownership_probe_same_set(reversed_expected));
        for (values, expected) in [(values, expected), (reversed, reversed_expected)] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.finite_result = Some(None);
                effects.pair_results = Some(values.to_vec());
                effects.expansion_eligible = Some(true);
                effects.expanded = Some(expanded);
                let actual = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(expected));
                let mut events = vec![
                    Event::Finite,
                    Event::ContextStart,
                    Event::Elements,
                    Event::Count,
                    Event::FoldStart,
                ];
                for member in members {
                    events.extend([
                        Event::Next,
                        Event::Pair(source, member),
                        Event::Capture,
                        Event::FoldPush,
                    ]);
                }
                events.extend([
                    Event::Next,
                    Event::FoldFinish,
                    Event::IsTriviallyAlways,
                    Event::ShouldExpand,
                    Event::Expand,
                    Event::Pair(expanded, Type::Union(target)),
                    Event::Disjoin,
                    Event::HasCollected,
                ]);
                assert_eq!(*effects.events.borrow(), events);
                effects.assert_released();
            }
        }
        Ok(())
    })
}

#[test]
fn saturation_captures_the_last_member_context_and_skips_expansion() -> anyhow::Result<()> {
    let db = setup_db();
    let source = intersection(&db);
    let members = [
        Type::AlwaysTruthy,
        Type::AlwaysFalsy,
        Type::literal_string(),
    ];
    let target = union(&db, &members);
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
                effects.finite_result = Some(None);
                effects.pair_results = Some(values.clone());
                effects.pair_contexts = (1..=values.len())
                    .map(|index| Some(child_context(index)))
                    .collect();
                assert!(
                    completed(effects.evaluate(source, target, asynchronous))?
                        .ownership_probe_same_set(expected)
                );
                let mut events = vec![
                    Event::Finite,
                    Event::ContextStart,
                    Event::Elements,
                    Event::Count,
                    Event::FoldStart,
                ];
                for &member in &members[..values.len()] {
                    events.extend([
                        Event::Next,
                        Event::Pair(source, member),
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
                effects.assert_released();
            }
        }
        Ok(())
    })
}

#[test]
fn typevar_fallback_reads_bounds_only_for_noninferable_sources() -> anyhow::Result<()> {
    let db = setup_db();
    let target = union(&db, &[Type::AlwaysTruthy, Type::AlwaysFalsy]);
    with_checker(&db, |checker| {
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            checker.env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let source = Type::TypeVar(variable);
        let bounds = TypeVarBoundOrConstraints::UpperBound(Type::object());
        let [bounded, ..] = constraints(&db, checker.constraints);
        for (inferable, bounds, tail, expected) in [
            (
                true,
                Some(bounds),
                vec![Event::IsInferable, Event::Never],
                checker.never(),
            ),
            (
                false,
                None,
                vec![Event::IsInferable, Event::Bounds, Event::Never],
                checker.never(),
            ),
            (
                false,
                Some(bounds),
                vec![
                    Event::IsInferable,
                    Event::Bounds,
                    Event::CheckBounds(Type::Union(target)),
                ],
                bounded,
            ),
        ] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.pair_results = Some(vec![checker.never(); 2]);
                effects.inferable = Some(inferable);
                effects.bounds = Some(bounds);
                effects.bounds_result = Some(bounded);
                let actual = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(expected));
                let events = effects.events.borrow();
                let mut expected_tail = vec![Event::IsTriviallyAlways];
                expected_tail.extend_from_slice(&tail);
                expected_tail.extend([Event::Disjoin, Event::HasCollected]);
                assert!(events.ends_with(&expected_tail));
                assert_eq!(effects.pairs.get(), 2);
                effects.assert_released();
            }
        }
        Ok(())
    })
}

#[test]
fn fallback_compares_only_eligible_intersections_and_union_newtype_bases() -> anyhow::Result<()> {
    let db = setup_db();
    let target = union(&db, &[Type::AlwaysTruthy, Type::AlwaysFalsy]);
    let wrapped = newtype(&db)?;
    with_checker(&db, |checker| {
        let [expanded_result, ..] = constraints(&db, checker.constraints);
        for (source, expand, base, tail, expected) in [
            (
                intersection(&db),
                false,
                Type::object(),
                vec![Event::ShouldExpand, Event::Never],
                checker.never(),
            ),
            (
                intersection(&db),
                true,
                Type::object(),
                vec![
                    Event::ShouldExpand,
                    Event::Expand,
                    Event::Pair(Type::int_literal(7), Type::Union(target)),
                ],
                expanded_result,
            ),
            (
                wrapped,
                false,
                Type::object(),
                vec![Event::NewtypeBase, Event::Never],
                checker.never(),
            ),
            (
                wrapped,
                false,
                Type::Union(target),
                vec![
                    Event::NewtypeBase,
                    Event::Pair(Type::Union(target), Type::Union(target)),
                ],
                expanded_result,
            ),
        ] {
            for asynchronous in [false, true] {
                let mut effects = Recording::new(&db, checker);
                effects.finite_result = Some(None);
                effects.pair_results =
                    Some(vec![checker.never(), checker.never(), expanded_result]);
                effects.expansion_eligible = Some(expand);
                effects.expanded = Some(Type::int_literal(7));
                effects.newtype_base = Some(base);
                let actual = completed(effects.evaluate(source, target, asynchronous))?;
                assert!(actual.ownership_probe_same_set(expected));
                let mut expected_tail = vec![Event::IsTriviallyAlways];
                expected_tail.extend_from_slice(&tail);
                expected_tail.extend([Event::Disjoin, Event::HasCollected]);
                assert!(effects.events.borrow().ends_with(&expected_tail));
                assert_eq!(
                    effects.pairs.get(),
                    if expand || base.is_union() { 3 } else { 2 }
                );
                effects.assert_released();
            }
        }
        Ok(())
    })
}

#[test]
fn full_unsatisfiability_groups_member_contexts_and_counts_missing_contexts() -> anyhow::Result<()>
{
    let db = setup_db();
    let source = intersection(&db);
    let target = union(
        &db,
        &[
            Type::AlwaysTruthy,
            Type::AlwaysFalsy,
            Type::literal_string(),
        ],
    );
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
            for contextual_members in [0, 1, 3] {
                for asynchronous in [false, true] {
                    let mut checker = checker.clone();
                    checker.context_tree = enabled.map(|enabled| {
                        let tree = ErrorContextTree::new(checker.relation);
                        tree.set_enabled(enabled);
                        tree
                    });
                    let mut effects = Recording::new(&db, &checker);
                    effects.finite_result = Some(None);
                    effects.pair_results = Some(vec![
                        impossible,
                        checker.never(),
                        checker.never(),
                        checker.never(),
                    ]);
                    effects.pair_contexts = (0..3)
                        .map(|index| {
                            (index < contextual_members).then(|| child_context(10 + index))
                        })
                        .chain([Some(child_context(99))])
                        .collect();
                    effects.expansion_eligible = Some(true);
                    effects.expanded = Some(Type::int_literal(7));
                    assert!(
                        completed(effects.evaluate(source, target, asynchronous))?
                            .ownership_probe_same_set(impossible)
                    );
                    let reported = enabled == Some(true) && contextual_members > 0;
                    assert_eq!(effects.events.borrow().contains(&Event::IsNever), reported);
                    assert_eq!(
                        effects.events.borrow().contains(&Event::Report(3)),
                        reported
                    );
                    if let Some(actual) = checker.report_context() {
                        let expected = if contextual_members > 0 {
                            let mut children: Vec<_> = (0..contextual_members)
                                .map(|index| {
                                    ErrorContextTree::from_context(
                                        child_context(10 + index),
                                        checker.relation,
                                    )
                                })
                                .collect();
                            if contextual_members < 3 {
                                children.push(ErrorContextTree::from_context(
                                    child_context(3 - contextual_members),
                                    checker.relation,
                                ));
                            }
                            let expected = ErrorContextTree::new(checker.relation);
                            expected.set(
                                ErrorContext::NotAssignableToAnyUnionElement {
                                    source,
                                    union: Type::Union(target),
                                },
                                children,
                            );
                            expected
                        } else {
                            ErrorContextTree::from_context(child_context(99), checker.relation)
                        };
                        assert_eq!(*actual, expected);
                    }
                    effects.assert_released();
                }
            }
        }
        Ok(())
    })
}

#[test]
fn ordinary_lazy_members_keep_inference_constraints_and_the_original_builder() -> anyhow::Result<()>
{
    let db = setup_db();
    with_checker(&db, |checker| {
        let variables = ["T", "U"].map(|name| {
            BoundTypeVarInstance::synthetic(
                &db,
                checker.env,
                Name::new_static(name),
                TypeVarVariance::Invariant,
            )
        });
        checker.inferable = TypeVarSet::from_typevars(&db, variables);
        checker.typevar_evaluation = TypeVarEvaluation::Lazy;
        let source = KnownClass::Int.to_instance(&db, checker.env);
        let target = union(&db, &variables.map(Type::TypeVar));
        let expected = target
            .elements(&db)
            .iter()
            .when_any(&db, checker.constraints, |&member| {
                checker.check_type_pair(&db, source, member)
            })
            .or(&db, checker.constraints, || checker.never());
        assert!(!expected.is_trivially_always_satisfied());
        assert!(!expected.is_trivially_never_satisfied());
        for variable in variables {
            assert!(expected.mentions_typevar(&db, variable));
        }
        for asynchronous in [false, true] {
            let effects = Recording::new(&db, checker);
            assert!(
                completed(effects.evaluate(source, target, asynchronous))?
                    .ownership_probe_same_set(expected)
            );
            effects.assert_released();
        }
        Ok(())
    })
}

#[test]
fn every_reached_effect_refuses_without_advancing_and_releases_retained_state() -> anyhow::Result<()>
{
    let db = setup_db();
    let target = union(&db, &[Type::AlwaysTruthy, Type::AlwaysFalsy]);
    let wrapped = newtype(&db)?;
    with_checker(&db, |checker| {
        checker.context_tree = Some(ErrorContextTree::new(checker.relation));
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            checker.env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        for source in [
            intersection(&db),
            Type::TypeVar(variable),
            wrapped,
            Type::object(),
        ] {
            let recording = || {
                let mut effects = Recording::new(&db, checker);
                effects.finite_result = Some(None);
                effects.pair_results = Some(vec![checker.never(); 3]);
                effects.pair_contexts = (1..=3).map(|index| Some(child_context(index))).collect();
                effects.inferable = Some(false);
                effects.bounds = Some(Some(TypeVarBoundOrConstraints::UpperBound(Type::object())));
                effects.bounds_result = Some(checker.never());
                effects.expansion_eligible = Some(true);
                effects.expanded = Some(Type::int_literal(7));
                effects.newtype_base = Some(Type::Union(target));
                effects
            };
            let successful = recording();
            let expected = completed(successful.evaluate(source, target, false))?;
            let events = successful.events.into_inner();
            assert_eq!(events.last(), Some(&Event::Report(2)));
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
                    effects.assert_released();
                    if let Some(context) = checker.report_context() {
                        context.take();
                    }
                    let retry = recording();
                    assert!(
                        completed(retry.evaluate(source, target, asynchronous))?
                            .ownership_probe_same_set(expected)
                    );
                    assert_eq!(*retry.events.borrow(), events);
                    retry.assert_released();
                }
            }
            if let Some(context) = checker.report_context() {
                context.take();
            }
        }
        Ok(())
    })
}

#[test]
fn suspended_member_retains_state_without_borrowing_storage_and_drop_allows_retry()
-> anyhow::Result<()> {
    let db = setup_db();
    let source = Type::object();
    let members = [Type::AlwaysTruthy, Type::AlwaysFalsy];
    let target = union(&db, &members);
    with_checker(&db, |checker| {
        checker.context_tree = Some(ErrorContextTree::new(checker.relation));
        let [a, b, ..] = constraints(&db, checker.constraints);
        let mut effects = Recording::new(&db, checker);
        effects.pair_results = Some(vec![a, b]);
        effects.pair_contexts = vec![Some(child_context(1)), Some(child_context(2))];
        effects.pause_before.set(Some(members[1]));
        {
            let mut future = pin!(check_target_union_with(source, target, &effects));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(effects.pairs.get(), 1);
            assert_eq!(effects.live_folds.get(), 1);
            assert_eq!(effects.live_contexts.get(), 1);
            // This combination needs mutable access to the builder's constraint storage,
            // which must remain available while the child future is suspended.
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
        effects.assert_released();
        let mut retry = Recording::new(&db, checker);
        retry.pair_results = Some(vec![a, b]);
        let expected = fold(&db, checker.constraints, &[a, b])
            .or(&db, checker.constraints, || checker.never());
        assert!(
            completed(retry.evaluate(source, target, true))?.ownership_probe_same_set(expected)
        );
        retry.assert_released();
        Ok(())
    })
}
