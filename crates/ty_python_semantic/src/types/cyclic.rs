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
        match self {
            // We can create a self-referential function type: e.g. `def f(x: "TypeOf[f]"): reveal_type(x)`
            // To avoid the difficulty of equality checking for function types containing this, we simply use `literal` for equality checking.
            Type::FunctionLiteral(function) => TypeIdentity::FunctionLiteral(function.literal(db)),
            // Similarly, we can create a self-referential NewType: e.g. `T = NewType("T", list["T"])`
            Type::NewTypeInstance(newtype) => TypeIdentity::NewTypeInstance(newtype.definition(db)),
            // Type aliases can be self-referential: e.g. `type RecursiveT = int | tuple[RecursiveT, ...]`
            Type::TypeAlias(alias) => TypeIdentity::TypeAlias(alias.definition(db)),
            _ => TypeIdentity::Other(self),
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

pub(crate) type PairVisitor<'db, Tag, C> = CycleDetector<'db, Tag, (Type<'db>, Type<'db>), C, 1>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>) {
    type Id = (TypeIdentity<'db>, TypeIdentity<'db>);

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (self.0.to_identity(db), self.1.to_identity(db))
    }
}

/// `CycleDetector` is temporary, so callers should choose the capacity that keeps observed cycle
/// paths inline even when that makes `seen` slightly larger than an `FxIndexSet<T>`.
#[derive(Debug)]
pub struct CycleDetector<'db, Tag, T: HasIdentity<'db>, R, const INLINE_CAPACITY: usize> {
    /// If the type we're visiting is present in `seen`, it indicates that we've hit a cycle (due
    /// to a recursive type); we need to immediately short circuit the whole operation and return
    /// the fallback value. That's why we pop items off the end of `seen` after we've visited them.
    /// Actually, what is contained here is not the `Type` itself, but its identity.
    /// `Type` has extra data than the type structure that should be equated,
    /// so it is compared using identity, which removes extra data.
    seen: RefCell<SmallVec<[T::Id; INLINE_CAPACITY]>>,

    /// Unlike `seen`, this field is a pure performance optimisation (and an essential one). If the
    /// type we're trying to normalize is present in `cache`, it doesn't necessarily mean we've hit
    /// a cycle: it just means that we've already visited this inner type as part of a bigger call
    /// chain we're currently in. Since this cache is just a performance optimisation, it doesn't
    /// make sense to pop items off the end of the cache after they've been visited (it would
    /// sort-of defeat the point of a cache if we did!)
    cache: RefCell<CycleDetectorCache<T, R>>,

    fallback: R,

    _tag: PhantomData<fn() -> Tag>,
}

impl<'db, Tag, T, R, const INLINE_CAPACITY: usize> CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    pub fn new(fallback: R) -> Self {
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

    /// Start visiting an item, exposing recursive cycles to callers that need an item-specific
    /// fallback.
    pub(crate) fn begin_visit(&self, db: &'db dyn Db, item: T) -> CycleDetectorVisit<T, R> {
        if let Some(result) = self.cache.borrow().get(&item) {
            return CycleDetectorVisit::Ready(result.clone());
        }

        let identity = item.to_identity(db);
        if self.seen.borrow().contains(&identity) {
            return CycleDetectorVisit::Cycle(item);
        }

        self.seen.borrow_mut().push(identity);
        CycleDetectorVisit::Pending(item)
    }

    /// Finish a [`CycleDetectorVisit::Pending`] visit and cache its result.
    pub(crate) fn finish_visit(&self, item: T, result: R) -> R {
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert_new(item, result.clone());
        result
    }
}

/// Result of starting a cycle-detector visit.
pub(crate) enum CycleDetectorVisit<T, R> {
    /// The item was already completed in this recursive operation.
    Ready(R),
    /// The wrapped value here is the input when recursion is detected.
    /// For complete results, implement recursive-only fallback handling using the wrapped value.
    Cycle(T),
    /// The caller should compute the result and pass it to [`CycleDetector::finish_visit`].
    Pending(T),
}

pub(crate) struct TypeTransformer<'db, Tag> {
    /// A type already present in `seen` forms a recursive cycle and is returned unchanged.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[TypeIdentity<'db>; 3]>>,

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
            CycleDetectorVisit::Ready(result) | CycleDetectorVisit::Cycle(result) => result,
            CycleDetectorVisit::Pending(ty) => {
                let result = compute();
                self.finish_visit(ty, result)
            }
        }
    }

    fn begin_visit(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> CycleDetectorVisit<Type<'db>, Type<'db>> {
        if let Some(result) = self.cache.borrow().get(&ty) {
            return CycleDetectorVisit::Ready(*result);
        }

        let identity = ty.to_identity(db);
        if self.seen.borrow().contains(&identity) {
            // When a cycle is encountered, the type being visited is returned as a fallback
            // (typically a recursive type alias).
            return CycleDetectorVisit::Cycle(ty);
        }

        self.seen.borrow_mut().push(identity);
        CycleDetectorVisit::Pending(ty)
    }

    fn finish_visit(&self, ty: Type<'db>, result: Type<'db>) -> Type<'db> {
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert_new(ty, result);
        result
    }
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

    /// Inserts a result after the caller has checked that `item` is not already cached.
    fn insert_new(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        debug_assert!(self.get(&item).is_none());
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
    use super::{CycleDetector, Db, HasIdentity};
    use crate::db::tests::setup_db;

    struct TestCycleDetector;
    type Detector<'db> = CycleDetector<'db, TestCycleDetector, u8, u8, 1>;

    impl<'db> HasIdentity<'db> for u8 {
        type Id = u8;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            *self
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
}
