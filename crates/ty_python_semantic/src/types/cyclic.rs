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

use std::cell::{OnceCell, RefCell};
use std::cmp::Eq;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::Db;
use crate::types::function::FunctionLiteral;
use crate::types::{ProtocolInstanceType, Type};

const MAX_RECURSIVE_TYPE_EXPANSIONS: usize = 10;

/// The type identity used for recursive checks/transformations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum TypeIdentity<'db> {
    FunctionLiteral(FunctionLiteral<'db>),
    NewTypeInstance(Definition<'db>),
    Protocol(Definition<'db>),
    TypeAlias(Definition<'db>),
    TypedDict(Definition<'db>),
    Other(Type<'db>),
}

impl<'db> Type<'db> {
    pub(crate) fn to_type_identity(self, db: &'db dyn Db) -> TypeIdentity<'db> {
        self.recursive_identity(db)
            .unwrap_or(TypeIdentity::Other(self))
    }

    /// Returns `false` if `self` and `other` cannot have the same [`TypeIdentity`].
    ///
    /// A `true` result is only a candidate match and must be confirmed with
    /// [`Type::to_type_identity`].
    pub(crate) fn may_share_type_identity(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Type::FunctionLiteral(a), Type::FunctionLiteral(b)) => a.literal(db) == b.literal(db),
            (Type::NewTypeInstance(a), Type::NewTypeInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::ProtocolInstance(a), Type::ProtocolInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::TypeAlias(a), Type::TypeAlias(b)) => a.definition(db) == b.definition(db),
            (Type::TypedDict(a), Type::TypedDict(b)) => a.definition(db) == b.definition(db),
            _ => false,
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn recursive_identity(self, db: &'db dyn Db) -> Option<TypeIdentity<'db>> {
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
            Type::ProtocolInstance(protocol) => {
                Some(TypeIdentity::Protocol(protocol.definition(db)?))
            }
            Type::TypedDict(typed_dict) => {
                let definition = typed_dict.definition(db)?;
                Some(TypeIdentity::TypedDict(definition))
            }
            _ => None,
        }
    }
}

impl<'db> ProtocolInstanceType<'db> {
    fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        let (origin, _) = self.class_origin(db)?.static_class_literal(db)?;
        Some(origin.definition(db))
    }
}

/// An item that provides the identity used to detect active recursive cycles.
pub trait HasIdentity<'db> {
    type Id: PartialEq;

    /// Returns `false` if `self` and `other` cannot have the same identity.
    ///
    /// Implementations can use this to avoid constructing an expensive identity. Returning
    /// `true` does not imply that the identities match; [`HasIdentity::to_identity`] confirms it.
    fn may_share_identity(&self, _db: &'db dyn Db, _other: &Self) -> bool {
        true
    }

    /// Returns an identity that remains stable while this item is active in a [`CycleDetector`].
    fn to_identity(&self, db: &'db dyn Db) -> Self::Id;
}

impl<'db> HasIdentity<'db> for Type<'db> {
    type Id = TypeIdentity<'db>;

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.may_share_type_identity(db, *other)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        Type::to_type_identity(*self, db)
    }
}

pub(crate) type PairVisitor<'db, Tag, C> = CycleDetector<'db, Tag, (Type<'db>, Type<'db>), C, 1>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>) {
    type Id = (TypeIdentity<'db>, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0) && self.1.may_share_type_identity(db, other.1)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (self.0.to_type_identity(db), self.1.to_type_identity(db))
    }
}

impl<'db, Context> HasIdentity<'db> for (Type<'db>, Context, Type<'db>)
where
    Context: Copy + PartialEq,
{
    type Id = (TypeIdentity<'db>, Context, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0)
            && self.1 == other.1
            && self.2.may_share_type_identity(db, other.2)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (
            self.0.to_type_identity(db),
            self.1,
            self.2.to_type_identity(db),
        )
    }
}

/// `CycleDetector` is temporary, so callers should choose the capacity that keeps observed cycle
/// paths inline even when that makes `seen` slightly larger than an `FxIndexSet<T>`.
#[derive(Debug)]
pub struct CycleDetector<'db, Tag, T: HasIdentity<'db>, R, const INLINE_CAPACITY: usize> {
    /// The active recursion stack and the lazily-computed identity of each item.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveCycleDetectorVisit<'db, T>; INLINE_CAPACITY]>>,

    /// Memoized results from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<T, R>>,

    fallback: R,

    _tag: PhantomData<fn() -> &'db Tag>,
}

impl<'db, Tag, T, R, const INLINE_CAPACITY: usize> CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    pub(crate) fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(SmallVec::new()),
            cache: RefCell::new(CycleDetectorCache::new()),
            fallback,
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag, T, R: Clone, const INLINE_CAPACITY: usize>
    CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
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

    /// Visits `item`, returning it in `Err` if another active item has the same identity.
    ///
    /// The caller must convert `Err(item)` into an operation-specific conservative result. An
    /// exact recursive reentry uses the detector's configured fallback and is returned as `Ok`.
    #[inline]
    pub(super) fn try_visit(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce() -> R,
    ) -> Result<R, T> {
        match self.begin_visit(db, item) {
            CycleDetectorVisit::Ready(result) => Ok(result),
            CycleDetectorVisit::Cycle(item) => Err(item),
            CycleDetectorVisit::Pending(item) => {
                let result = compute();
                Ok(self.finish_visit(item, result))
            }
        }
    }

    /// Starts a visit while allowing `MAX_RECURSIVE_TYPE_EXPANSIONS` different items
    /// with the same identity after the first active item.
    /// Exact item recurrence is handled before this limit and returns the configured fallback at any depth.
    ///
    /// This is necessary because there are recursive aliases that require several expansions to reach a "stable point", such as:
    ///
    /// ```python
    /// type Left[A, B, C] = tuple[A, Left[B, C, None]]
    /// type Right[A, B, C] = tuple[A, Right[B, C, None]]
    ///
    /// # Left[int, int, int]
    /// # = tuple[int, Left[int, int, None]]
    /// # = tuple[int, tuple[int, Left[int, None, None]]]
    /// # = tuple[int, tuple[int, tuple[int, Left[None, None, None]]]]
    /// # Left[None, None, None] (= tuple[None, Left[None, None, None]]) is stable, so it can be completely determined
    /// static_assert(is_subtype_of(Left[int, int, int], Right[int, int, int]))
    /// ```
    ///
    /// A growing specialization chain may never reach such an exact recurrence. The finite limit
    /// guarantees that it eventually produces [`CycleDetectorVisit::Cycle`], allowing the caller
    /// to return a conservative result.
    fn begin_visit(&self, db: &'db dyn Db, item: T) -> CycleDetectorVisit<T, R> {
        if let Some(result) = self.cache.borrow().get(&item) {
            return CycleDetectorVisit::Ready(result.clone());
        }

        let seen = self.seen.borrow();
        if seen.iter().any(|active| active.item == item) {
            return CycleDetectorVisit::Ready(self.fallback.clone());
        }

        let mut candidates = seen
            .iter()
            .filter(|active| item.may_share_identity(db, &active.item))
            .peekable();
        let identity = if candidates.peek().is_none() {
            OnceCell::new()
        } else {
            // Deriving an identity can require a structural definition walk. Defer it until a
            // cheap candidate match shows that another active item could form a cycle.
            let identity = item.to_identity(db);
            if candidates
                .filter(|active| {
                    active.identity.get_or_init(|| active.item.to_identity(db)) == &identity
                })
                .count()
                > MAX_RECURSIVE_TYPE_EXPANSIONS
            {
                return CycleDetectorVisit::Cycle(item);
            }
            OnceCell::from(identity)
        };
        drop(seen);

        self.seen.borrow_mut().push(ActiveCycleDetectorVisit {
            item: item.clone(),
            identity,
        });
        CycleDetectorVisit::Pending(item)
    }

    /// Finish a [`CycleDetectorVisit::Pending`] visit and cache its result.
    fn finish_visit(&self, item: T, result: R) -> R {
        let active = self.seen.borrow_mut().pop();
        debug_assert!(active.as_ref().is_some_and(|active| active.item == item));
        self.cache
            .borrow_mut()
            .insert_completed(item, result.clone());
        result
    }
}

struct ActiveCycleDetectorVisit<'db, T: HasIdentity<'db>> {
    item: T,
    identity: OnceCell<T::Id>,
}

impl<'db, T: fmt::Debug + HasIdentity<'db>> fmt::Debug for ActiveCycleDetectorVisit<'db, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.item.fmt(f)
    }
}

/// Result of starting a cycle-detector visit.
pub(super) enum CycleDetectorVisit<T, R> {
    /// The item already has a completed result or hit an exact recursive edge.
    Ready(R),
    /// A different item with the same abstract identity is already pending.
    Cycle(T),
    /// The caller should compute the result and finish the pending visit.
    Pending(T),
}

/// Guards recursive type transformations.
pub(crate) struct TypeTransformer<'db, Tag> {
    /// The active transformation stack and its recursive identities.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveTypeTransformation<'db>; 3]>>,

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
            TypeTransformerVisit::Ready(result) => result,
            TypeTransformerVisit::Pending(ty) => {
                let result = compute();
                self.finish_visit(ty, result)
            }
        }
    }

    fn begin_visit(&self, db: &'db dyn Db, ty: Type<'db>) -> TypeTransformerVisit<'db> {
        if let Some(result) = self.cache.borrow().get(&ty) {
            return TypeTransformerVisit::Ready(*result);
        }

        let identity = ty.to_type_identity(db);
        let seen = self.seen.borrow();
        if seen
            .iter()
            .any(|active| active.ty == ty || active.identity == identity)
        {
            return TypeTransformerVisit::Ready(ty);
        }
        drop(seen);

        self.seen
            .borrow_mut()
            .push(ActiveTypeTransformation { ty, identity });
        TypeTransformerVisit::Pending(ty)
    }

    fn finish_visit(&self, ty: Type<'db>, result: Type<'db>) -> Type<'db> {
        let active = self.seen.borrow_mut().pop();
        debug_assert_eq!(active.map(|active| active.ty), Some(ty));
        self.cache.borrow_mut().insert_completed(ty, result);
        result
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveTypeTransformation<'db> {
    ty: Type<'db>,
    identity: TypeIdentity<'db>,
}

enum TypeTransformerVisit<'db> {
    Ready(Type<'db>),
    Pending(Type<'db>),
}

impl<'db, Tag, T, R: Default, const INLINE_CAPACITY: usize> Default
    for CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
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
    use super::MAX_RECURSIVE_TYPE_EXPANSIONS;
    use super::{CycleDetector, CycleDetectorVisit, Db, HasIdentity};
    use crate::db::tests::setup_db;
    use std::cell::Cell;
    use std::hash::{Hash, Hasher};

    struct TestVisit;

    type Detector<'db> = CycleDetector<'db, TestVisit, u8, u8, 1>;

    impl<'db> HasIdentity<'db> for u8 {
        type Id = Self;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            *self
        }
    }

    #[derive(Clone)]
    struct CountingIdentityItem<'a> {
        value: u8,
        identity_calls: &'a Cell<usize>,
    }

    impl<'a> CountingIdentityItem<'a> {
        const fn new(value: u8, identity_calls: &'a Cell<usize>) -> Self {
            Self {
                value,
                identity_calls,
            }
        }
    }

    impl PartialEq for CountingIdentityItem<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }

    impl Eq for CountingIdentityItem<'_> {}

    impl Hash for CountingIdentityItem<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    impl<'db> HasIdentity<'db> for CountingIdentityItem<'_> {
        type Id = u8;

        fn may_share_identity(&self, _db: &'db dyn Db, other: &Self) -> bool {
            self.value % 2 == other.value % 2
        }

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            self.identity_calls.set(self.identity_calls.get() + 1);
            self.value
        }
    }

    #[derive(Clone, Eq, Hash, PartialEq)]
    struct ConstantIdentityItem(u8);

    impl<'db> HasIdentity<'db> for ConstantIdentityItem {
        type Id = ();

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {}
    }

    #[test]
    fn caches_results_and_spills_after_two_entries() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(detector.visit(db, 1, || 10), 10);
        assert_eq!(detector.visit(db, 1, || 40), 10);
        assert_eq!(detector.visit(db, 2, || 20), 20);
        assert!(!detector.cache.borrow().is_spilled());
        assert_eq!(detector.visit(db, 3, || 30), 30);
        assert!(detector.cache.borrow().is_spilled());

        assert_eq!(detector.visit(db, 2, || 40), 20);
        assert_eq!(detector.visit(db, 3, || 40), 30);
    }

    #[test]
    fn nested_visit_short_circuits_on_cycle() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(
            detector.visit(db, 1, || detector.visit(db, 1, || 20) + 10),
            10
        );
    }

    #[test]
    fn computes_each_active_identity_once() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || {
                detector.visit(db, CountingIdentityItem::new(3, &identity_calls), || 1)
            }),
            1
        );
        assert_eq!(identity_calls.get(), 2);
    }

    #[test]
    fn skips_identity_for_distinct_candidates() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || {
                detector.visit(db, CountingIdentityItem::new(2, &identity_calls), || 1)
            }),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn skips_identity_without_a_distinct_active_item() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || 1),
            1
        );
        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || 2),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn different_items_with_same_identity_hit_expansion_limit() {
        let db = setup_db();
        let db = &db;
        let detector = CycleDetector::<TestVisit, ConstantIdentityItem, u8, 1>::new(0);

        let mut pending = Vec::new();
        #[allow(clippy::cast_possible_truncation)]
        for value in 1..=(MAX_RECURSIVE_TYPE_EXPANSIONS as u8 + 1) {
            let CycleDetectorVisit::Pending(item) =
                detector.begin_visit(db, ConstantIdentityItem(value))
            else {
                panic!("items within the recursive expansion limit should be pending");
            };
            pending.push(item);
        }

        let CycleDetectorVisit::Cycle(item) = detector.begin_visit(db, ConstantIdentityItem(12))
        else {
            panic!("the first item beyond the recursive expansion limit should form a cycle");
        };
        assert_eq!(item.0, 12);

        for item in pending.into_iter().rev() {
            let result = item.0;
            detector.finish_visit(item, result);
        }

        let CycleDetectorVisit::Ready(seen) = detector.begin_visit(db, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 1);
        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, ConstantIdentityItem(12))
        else {
            panic!("the item beyond the limit should be pending after the active visits finish");
        };
        detector.finish_visit(pending, 12);
        let CycleDetectorVisit::Ready(seen) = detector.begin_visit(db, ConstantIdentityItem(12))
        else {
            panic!("the item should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 12);
    }
}
