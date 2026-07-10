//! Cycle detection for recursive types.
//!
//! The visitors here ([`TypeTransformer`] and [`PairVisitor`]) are used in methods that
//! recursively visit types to transform them (e.g. [`Type::apply_type_mapping`]) or to
//! decide a relation between a pair of types (e.g. [`Type::has_relation_to`]).
//!
//! The typical pattern is that the "entry" method (e.g. [`Type::apply_type_mapping`]) will create
//! a visitor and pass it to the recursive method (e.g. [`Type::apply_type_mapping_impl`]).
//! Rust types that form part of a complex type (e.g. tuples, protocols, nominal instances, etc)
//! should usually just implement the recursive method, and all recursive calls should call the
//! recursive method and pass along the visitor.
//!
//! Not all recursive calls need to actually call `.visit` on the visitor; only when visiting types
//! that can create a recursive relationship (this includes, for example, type aliases and
//! protocols).
//!
//! There is a risk of double-visiting, for example if [`Type::apply_type_mapping_impl`] calls
//! `visitor.visit` when visiting a protocol type, and then internal `apply_type_mapping_impl`
//! methods of the Rust types implementing protocols also call `visitor.visit`. The best way to
//! avoid this is to prefer always calling `visitor.visit` only in the main recursive method on
//! `Type`.

use std::cell::RefCell;
use std::cmp::Eq;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::Type;
use crate::types::function::FunctionLiteral;
use crate::types::type_alias::TypeAliasApplication;

/// The type identity used for recursive checks/transformations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum TypeIdentity<'db> {
    FunctionLiteral(FunctionLiteral<'db>),
    NewTypeInstance(Definition<'db>),
    TypeAlias(Definition<'db>),
    Other(Type<'db>),
}

impl<'db> Type<'db> {
    pub(crate) fn to_type_identity(self, db: &'db dyn Db) -> TypeIdentity<'db> {
        self.recursive_identity(db)
            .unwrap_or(TypeIdentity::Other(self))
    }

    pub(crate) fn recursive_identity(self, db: &'db dyn Db) -> Option<TypeIdentity<'db>> {
        match self {
            // We can create a self-referential function type: e.g. `def f(x: "TypeOf[f]"): reveal_type(x)`
            // To avoid the difficulty of equality checking for function types containing this, we simply use `literal` for equality checking.
            Type::FunctionLiteral(function) => {
                Some(TypeIdentity::FunctionLiteral(function.literal(db)))
            }
            // Similarly, we can create a self-referential NewType: e.g. `T = NewType("T", list["T"])`
            Type::NewTypeInstance(newtype) => {
                Some(TypeIdentity::NewTypeInstance(newtype.definition(db)))
            }
            // Type aliases can be self-referential: e.g. `type RecursiveT = int | tuple[RecursiveT, ...]`
            Type::TypeAlias(alias) => Some(TypeIdentity::TypeAlias(alias.definition(db))),
            _ => None,
        }
    }
}

pub trait HasIdentity<'db> {
    type Id: Sized + PartialEq;

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id;
}

impl<'db> HasIdentity<'db> for Type<'db> {
    type Id = TypeIdentity<'db>;

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        self.to_type_identity(db)
    }
}

pub(crate) type PairVisitor<'db, C> =
    CycleDetector<'db, TypePairCyclePolicy, (Type<'db>, Type<'db>), C, 1>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>) {
    type Id = (TypeIdentity<'db>, TypeIdentity<'db>);

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (self.0.to_identity(db), self.1.to_identity(db))
    }
}

impl<'db, Context> HasIdentity<'db> for (Type<'db>, Context, Type<'db>)
where
    Context: Copy + PartialEq,
{
    type Id = (TypeIdentity<'db>, Context, TypeIdentity<'db>);

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (self.0.to_identity(db), self.1, self.2.to_identity(db))
    }
}

/// An item whose recursive identity is determined by a pair of types.
pub(crate) trait TypePairItem<'db>: HasIdentity<'db> {
    fn type_pair(&self) -> (Type<'db>, Type<'db>);
}

impl<'db> TypePairItem<'db> for (Type<'db>, Type<'db>) {
    fn type_pair(&self) -> (Type<'db>, Type<'db>) {
        *self
    }
}

impl<'db, Context> TypePairItem<'db> for (Type<'db>, Context, Type<'db>)
where
    Context: Copy + PartialEq,
{
    fn type_pair(&self) -> (Type<'db>, Type<'db>) {
        (self.0, self.2)
    }
}

/// Decides when an abstract-identity reentry requires the recursive fallback.
pub trait CyclePolicy<'db, T: HasIdentity<'db>> {
    fn reentry_index(db: &'db dyn Db, current: &T, active: &[T]) -> Option<usize>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AliasCycleThreshold {
    Immediate,
    AfterSecondUnfold,
}

#[derive(Debug, Clone, Copy)]
enum ActiveRecursiveIdentityRelation<'db> {
    None,
    DirectCycle,
    Alias(ActiveAliasRecursiveIdentityRelation<'db>),
}

#[derive(Debug, Clone, Copy)]
struct ActiveAliasRecursiveIdentityRelation<'db> {
    application: TypeAliasApplication<'db>,
    nested_application: bool,
}

/// The active type stack used to classify recursive-identity reentries.
pub(crate) struct RecursiveTypeStack<'a, 'db> {
    db: &'db dyn Db,
    active: &'a [Type<'db>],
}

impl<'a, 'db> RecursiveTypeStack<'a, 'db> {
    pub(crate) const fn new(db: &'db dyn Db, active: &'a [Type<'db>]) -> Self {
        Self { db, active }
    }

    pub(crate) fn contains_immediate_reentry(self, current: Type<'db>) -> bool {
        self.reentry_index(current, AliasCycleThreshold::Immediate)
            .is_some()
    }

    fn reentry_index(self, current: Type<'db>, threshold: AliasCycleThreshold) -> Option<usize> {
        let mut first_alias_index = None;
        let mut active_alias_count = 0;
        let mut application = None;

        for (index, active) in self.active.iter().copied().enumerate() {
            match current.recursive_identity_relation_to(self.db, active) {
                ActiveRecursiveIdentityRelation::None => {}
                ActiveRecursiveIdentityRelation::DirectCycle => return Some(index),
                ActiveRecursiveIdentityRelation::Alias(relation) => {
                    if relation.nested_application {
                        return None;
                    }
                    active_alias_count += 1;
                    first_alias_index.get_or_insert(index);
                    application.get_or_insert(relation.application);
                }
            }
        }

        let (Some(first_alias_index), Some(application)) = (first_alias_index, application) else {
            return None;
        };

        if application.contains_recursive_identity_from(self.db, self.active) {
            return Some(first_alias_index);
        }

        match threshold {
            AliasCycleThreshold::Immediate => Some(first_alias_index),
            AliasCycleThreshold::AfterSecondUnfold => {
                (active_alias_count > 1).then_some(first_alias_index)
            }
        }
    }

    fn has_immediate_reentry_to(db: &'db dyn Db, current: Type<'db>, active: Type<'db>) -> bool {
        match current.recursive_identity_relation_to(db, active) {
            ActiveRecursiveIdentityRelation::None => false,
            ActiveRecursiveIdentityRelation::DirectCycle => true,
            ActiveRecursiveIdentityRelation::Alias(relation) => !relation.nested_application,
        }
    }
}

impl<'db> Type<'db> {
    fn recursive_identity_relation_to(
        self,
        db: &'db dyn Db,
        active: Type<'db>,
    ) -> ActiveRecursiveIdentityRelation<'db> {
        let Some(identity) = self.recursive_identity(db) else {
            return ActiveRecursiveIdentityRelation::None;
        };

        if active.recursive_identity(db) != Some(identity) {
            return ActiveRecursiveIdentityRelation::None;
        }

        let Type::TypeAlias(alias) = self else {
            return ActiveRecursiveIdentityRelation::DirectCycle;
        };

        let Some(application) = alias.application(db) else {
            return ActiveRecursiveIdentityRelation::DirectCycle;
        };

        ActiveRecursiveIdentityRelation::Alias(ActiveAliasRecursiveIdentityRelation {
            application,
            nested_application: application.is_nested_within(db, active, self),
        })
    }
}

/// Uses the first repeated abstract identity as the recursive fallback boundary.
pub(crate) struct IdentityCyclePolicy;

impl<'db, T: HasIdentity<'db>> CyclePolicy<'db, T> for IdentityCyclePolicy {
    fn reentry_index(db: &'db dyn Db, current: &T, active: &[T]) -> Option<usize> {
        let identity = current.to_identity(db);
        active
            .iter()
            .position(|active| active.to_identity(db) == identity)
    }
}

/// Applies recursive-type semantics to a single type stack.
pub(crate) struct TypeCyclePolicy;

impl<'db> CyclePolicy<'db, Type<'db>> for TypeCyclePolicy {
    fn reentry_index(db: &'db dyn Db, current: &Type<'db>, active: &[Type<'db>]) -> Option<usize> {
        RecursiveTypeStack::new(db, active).reentry_index(*current, AliasCycleThreshold::Immediate)
    }
}

/// Applies recursive-type semantics to an item containing a pair of types.
pub(crate) struct TypePairCyclePolicy;

impl<'db, T> CyclePolicy<'db, T> for TypePairCyclePolicy
where
    T: TypePairItem<'db>,
{
    fn reentry_index(db: &'db dyn Db, current: &T, active: &[T]) -> Option<usize> {
        let identity = current.to_identity(db);
        let (current_left, current_right) = current.type_pair();

        active.iter().position(|active| {
            if active.to_identity(db) != identity {
                return false;
            }
            let (active_left, active_right) = active.type_pair();
            RecursiveTypeStack::has_immediate_reentry_to(db, current_left, active_left)
                || RecursiveTypeStack::has_immediate_reentry_to(db, current_right, active_right)
        })
    }
}

/// `CycleDetector` is temporary, so callers should choose the capacity that keeps observed cycle
/// paths inline even when that makes `seen` slightly larger than an `FxIndexSet<T>`.
#[derive(Debug)]
pub struct CycleDetector<'db, Policy, T: HasIdentity<'db>, R, const INLINE_CAPACITY: usize> {
    /// The active recursion stack. Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[T; INLINE_CAPACITY]>>,

    /// Memoized results from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<T, R>>,

    fallback: R,

    _policy: PhantomData<fn(&'db ()) -> Policy>,
}

impl<'db, Policy, T, R, const INLINE_CAPACITY: usize>
    CycleDetector<'db, Policy, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
    Policy: CyclePolicy<'db, T>,
{
    pub fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(SmallVec::new()),
            cache: RefCell::new(CycleDetectorCache::new()),
            fallback,
            _policy: PhantomData,
        }
    }
}

impl<'db, Policy, T, R: Clone, const INLINE_CAPACITY: usize>
    CycleDetector<'db, Policy, T, R, INLINE_CAPACITY>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
    Policy: CyclePolicy<'db, T>,
{
    #[inline]
    pub fn visit(&self, db: &'db dyn Db, item: T, compute: impl FnOnce() -> R) -> R {
        match self.begin_visit(db, item) {
            CycleDetectorVisit::Ready(result) => result,
            CycleDetectorVisit::Cycle(_) => self.fallback.clone(),
            CycleDetectorVisit::Pending(item) => {
                let result = compute();
                self.finish_visit(item, result)
            }
        }
    }

    /// Start visiting an item, exposing recursive cycles to callers that need an item-specific
    /// fallback.
    pub(crate) fn begin_visit(&self, db: &'db dyn Db, item: T) -> CycleDetectorVisit<T, R> {
        if let Some(result) = self.cache.borrow().get(&item) {
            return CycleDetectorVisit::Ready(result.clone());
        }

        let seen = self.seen.borrow();
        if seen.contains(&item) {
            return CycleDetectorVisit::Ready(self.fallback.clone());
        }

        if let Some(active_index) = Policy::reentry_index(db, &item, &seen) {
            return CycleDetectorVisit::Cycle(CycleDetectorReentry {
                current: item,
                active: seen[active_index].clone(),
            });
        }
        drop(seen);

        self.seen.borrow_mut().push(item.clone());
        CycleDetectorVisit::Pending(item)
    }

    /// Finish a [`CycleDetectorVisit::Pending`] visit and cache its result.
    pub(crate) fn finish_visit(&self, item: T, result: R) -> R {
        let active = self.seen.borrow_mut().pop();
        debug_assert!(active.as_ref().is_some_and(|active| active == &item));
        self.cache
            .borrow_mut()
            .insert_completed(item, result.clone());
        result
    }
}

/// Result of starting a cycle-detector visit.
pub(crate) enum CycleDetectorVisit<T, R> {
    /// The item already has a completed result or hit an exact recursive edge.
    Ready(R),
    /// A different item with the same abstract identity is already pending.
    Cycle(CycleDetectorReentry<T>),
    /// The caller should compute the result and pass it to [`CycleDetector::finish_visit`].
    Pending(T),
}

/// The current item that re-entered an active abstract identity.
pub(crate) struct CycleDetectorReentry<T> {
    pub(crate) current: T,
    pub(crate) active: T,
}

/// Guards recursive type transformations.
///
/// Unlike [`CycleDetector`], type transformation has only one recursive fallback: preserve the
/// current type. It also cannot stop at every same-identity alias application, because some nested
/// alias applications stabilize after another transform step.
pub(crate) struct TypeTransformer<'db, Tag> {
    /// A type already present in `seen` forms a recursive cycle and is returned unchanged.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[Type<'db>; 3]>>,

    /// Memoized transformations from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<Type<'db>, Type<'db>>>,

    _tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Default for TypeTransformer<'_, Tag> {
    fn default() -> Self {
        Self {
            seen: RefCell::default(),
            cache: RefCell::default(),
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag> TypeTransformer<'db, Tag> {
    #[inline]
    pub(crate) fn visit_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        match self.begin_visit(db, ty) {
            TypeTransformerVisit::Return(result) => result,
            TypeTransformerVisit::Pending(ty) => {
                let result = compute();
                self.finish_visit(ty, result)
            }
        }
    }

    fn begin_visit(&self, db: &'db dyn Db, ty: Type<'db>) -> TypeTransformerVisit<'db> {
        if let Some(result) = self.cache.borrow().get(&ty) {
            return TypeTransformerVisit::Return(*result);
        }

        let seen = self.seen.borrow();
        if seen.contains(&ty)
            || RecursiveTypeStack::new(db, &seen)
                .reentry_index(ty, AliasCycleThreshold::AfterSecondUnfold)
                .is_some()
        {
            return TypeTransformerVisit::Return(ty);
        }
        drop(seen);

        self.seen.borrow_mut().push(ty);
        TypeTransformerVisit::Pending(ty)
    }

    fn finish_visit(&self, ty: Type<'db>, result: Type<'db>) -> Type<'db> {
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert_completed(ty, result);
        result
    }
}

enum TypeTransformerVisit<'db> {
    Return(Type<'db>),
    Pending(Type<'db>),
}

impl<'db, Policy, T, R: Default, const INLINE_CAPACITY: usize> Default
    for CycleDetector<'db, Policy, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
    Policy: CyclePolicy<'db, T>,
{
    fn default() -> Self {
        CycleDetector::new(R::default())
    }
}

/// The memoized results for a [`CycleDetector`].
///
/// Most populated cycle-detector caches contain at most two results. Keep those results inline,
/// but spill on the third distinct result so lookups in wider caches remain hashed.
#[derive(Debug, Default)]
enum CycleDetectorCache<T, R> {
    #[default]
    Empty,
    One((T, R)),
    Two([(T, R); 2]),
    Spilled(FxHashMap<T, R>),
}

impl<T, R> CycleDetectorCache<T, R> {
    const fn new() -> Self {
        Self::Empty
    }

    fn get(&self, item: &T) -> Option<&R>
    where
        T: Hash + Eq,
    {
        match self {
            Self::Empty => None,
            Self::One((cached_item, result)) => (cached_item == item).then_some(result),
            Self::Two(entries) => entries
                .iter()
                .find_map(|(cached_item, result)| (cached_item == item).then_some(result)),
            Self::Spilled(cache) => cache.get(item),
        }
    }

    /// Inserts a completed item after the caller has checked that `item` is not already cached.
    fn insert_completed(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        debug_assert!(self.get(&item).is_none());
        self.insert_new(item, result);
    }

    fn insert_new(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        let entry = (item, result);
        *self = match mem::replace(self, Self::Empty) {
            Self::Empty => Self::One(entry),
            Self::One(first) => Self::Two([first, entry]),
            Self::Two(entries) => Self::spill(entries, entry),
            Self::Spilled(mut cache) => {
                cache.insert(entry.0, entry.1);
                Self::Spilled(cache)
            }
        };
    }

    #[cold]
    fn spill(entries: [(T, R); 2], third: (T, R)) -> Self
    where
        T: Hash + Eq,
    {
        Self::Spilled(entries.into_iter().chain([third]).collect())
    }

    #[cfg(test)]
    const fn is_spilled(&self) -> bool {
        matches!(self, Self::Spilled(_))
    }
}

/// Recursion detection without memoization.
///
/// This is useful when a recursive relation needs a coinductive-style "we're already proving this
/// goal, assume it for now" step, but completed results are not safe to reuse for future visits to
/// the same abstract key.
#[derive(Debug)]
pub(crate) struct ActiveRecursionDetector<T> {
    seen: RefCell<FxHashSet<T>>,
}

impl<T> Default for ActiveRecursionDetector<T> {
    fn default() -> Self {
        Self {
            seen: RefCell::new(FxHashSet::default()),
        }
    }
}

impl<T: Hash + Eq + Clone> ActiveRecursionDetector<T> {
    pub(crate) fn visit<R>(
        &self,
        item: &T,
        on_cycle: impl FnOnce() -> R,
        func: impl FnOnce() -> R,
    ) -> R {
        if !self.seen.borrow_mut().insert(item.clone()) {
            return on_cycle();
        }

        // Keep the active-recursion state scoped even if `func` unwinds. In some cases, we catch
        // panics and continue handling later work on the same thread.
        let _guard = ActiveRecursionGuard {
            seen: &self.seen,
            item,
        };

        func()
    }
}

struct ActiveRecursionGuard<'a, T: Hash + Eq> {
    seen: &'a RefCell<FxHashSet<T>>,
    item: &'a T,
}

impl<T: Hash + Eq> Drop for ActiveRecursionGuard<'_, T> {
    fn drop(&mut self) {
        self.seen.borrow_mut().remove(self.item);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CycleDetector, CycleDetectorVisit, CyclePolicy, Db, HasIdentity, IdentityCyclePolicy,
    };
    use crate::db::tests::setup_db;

    type Detector<'db> = CycleDetector<'db, IdentityCyclePolicy, u8, u8, 1>;
    type IdentityDetector<'db> = CycleDetector<'db, IdentityCyclePolicy, TestItem, u8, 1>;

    impl<'db> HasIdentity<'db> for u8 {
        type Id = u8;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            *self
        }
    }

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    struct TestItem {
        value: u8,
        identity: u8,
    }

    impl<'db> HasIdentity<'db> for TestItem {
        type Id = u8;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            self.identity
        }
    }

    struct ConditionalIdentityCyclePolicy;

    type ExactIdentityDetector<'db> =
        CycleDetector<'db, ConditionalIdentityCyclePolicy, ExactIdentityItem, u8, 1>;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    struct ExactIdentityItem {
        value: u8,
        identity: u8,
        recursive_identity: bool,
    }

    impl<'db> HasIdentity<'db> for ExactIdentityItem {
        type Id = u8;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            self.identity
        }
    }

    impl<'db> CyclePolicy<'db, ExactIdentityItem> for ConditionalIdentityCyclePolicy {
        fn reentry_index(
            db: &'db dyn Db,
            current: &ExactIdentityItem,
            active: &[ExactIdentityItem],
        ) -> Option<usize> {
            if !current.recursive_identity {
                return None;
            }
            let identity = current.to_identity(db);
            active
                .iter()
                .position(|active| active.recursive_identity && active.to_identity(db) == identity)
        }
    }

    #[test]
    fn caches_results_and_spills_after_two_entries() {
        let db = setup_db();
        let detector = Detector::new(0);

        assert_eq!(detector.visit(&db, 1, || 10), 10);
        assert_eq!(detector.visit(&db, 1, || 40), 10);
        assert_eq!(detector.visit(&db, 2, || 20), 20);
        assert!(!detector.cache.borrow().is_spilled());
        assert_eq!(detector.visit(&db, 3, || 30), 30);
        assert!(detector.cache.borrow().is_spilled());

        assert_eq!(detector.visit(&db, 2, || 40), 20);
        assert_eq!(detector.visit(&db, 3, || 40), 30);
    }

    #[test]
    fn nested_visit_short_circuits_on_cycle() {
        let db = setup_db();
        let detector = Detector::new(0);

        assert_eq!(
            detector.visit(&db, 1, || detector.visit(&db, 1, || 20) + 10),
            10
        );
    }

    #[test]
    fn nested_visit_short_circuits_on_identity_cycle_without_caching_it() {
        let db = setup_db();
        let detector = IdentityDetector::new(0);
        let first = TestItem {
            value: 1,
            identity: 1,
        };
        let second = TestItem {
            value: 2,
            identity: 1,
        };

        assert_eq!(
            detector.visit(&db, first, || detector.visit(&db, second, || 20) + 10),
            10
        );
        assert_eq!(detector.visit(&db, second, || 30), 30);
    }

    #[test]
    fn exact_identity_items_ignore_shared_abstract_identity() {
        let db = setup_db();
        let detector = ExactIdentityDetector::new(0);
        let first = ExactIdentityItem {
            value: 1,
            identity: 1,
            recursive_identity: false,
        };
        let second = ExactIdentityItem {
            value: 2,
            identity: 1,
            recursive_identity: false,
        };

        assert_eq!(
            detector.visit(&db, first, || detector.visit(&db, second, || 20) + 10),
            30
        );
    }

    #[test]
    fn active_items_that_skip_recursive_identity_do_not_trigger_identity_cycles() {
        let db = setup_db();
        let detector = ExactIdentityDetector::new(0);
        let first = ExactIdentityItem {
            value: 1,
            identity: 1,
            recursive_identity: false,
        };
        let second = ExactIdentityItem {
            value: 2,
            identity: 1,
            recursive_identity: true,
        };

        assert_eq!(
            detector.visit(&db, first, || detector.visit(&db, second, || 20) + 10),
            30
        );
    }

    #[test]
    fn identity_cycle_reports_current_item() {
        let db = setup_db();
        let detector = IdentityDetector::new(0);
        let first = TestItem {
            value: 1,
            identity: 1,
        };
        let second = TestItem {
            value: 2,
            identity: 1,
        };

        let CycleDetectorVisit::Pending(active_item) = detector.begin_visit(&db, first) else {
            panic!("first visit should be pending");
        };

        let CycleDetectorVisit::Cycle(reentry) = detector.begin_visit(&db, second) else {
            panic!("second visit should detect an identity cycle");
        };
        assert_eq!(reentry.current, second);
        assert_eq!(reentry.active, first);

        detector.finish_visit(active_item, 10);
    }
}
