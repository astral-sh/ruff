//! Cycle detection for recursive types.
//!
//! The guards here ([`CycleDetector`], and [`ApplyTypeMappingVisitor`]) are used in methods that
//! recursively visit types to transform them (e.g. [`Type::apply_type_mapping`]) or to decide a
//! relation between a pair of types (e.g. [`Type::has_relation_to`]).
//!
//! ## `CycleDetector` (for type relation checking)
//!
//! [`CycleDetector`] is used for methods like `has_relation_to` and `is_disjoint_from`, where the
//! visited item (a type pair) uniquely determines the result. If we encounter the same pair during
//! recursion, it's always a genuine cycle, so adding it to a `seen` set and caching the result is
//! correct.
//!
//! ## `ApplyTypeMappingVisitor` (for `apply_type_mapping`)
//!
//! [`ApplyTypeMappingVisitor`] is a purpose-built recursion guard for [`Type::apply_type_mapping`].
//! Unlike `CycleDetector`, it separates cycle detection from depth limiting, because
//! `apply_type_mapping` has a property that type relation checking does not: the result of mapping
//! a type depends on context (for example, which type aliases are currently being expanded).
//!
//! Cycles in `apply_type_mapping` arise from self-referential type definitions: recursive type
//! aliases (e.g. `type Rec[T] = T | list[Rec[T]]`), self-referencing function literals (via
//! `TypeOf`), and recursive newtypes. Other types like nominal instances merely *contain*
//! recursive types but cannot introduce cycles themselves. Using a generic `CycleDetector` on
//! such types would incorrectly flag them as cycles when they appear at different levels of a
//! recursive expansion.
//!
//! `ApplyTypeMappingVisitor` provides:
//! - [`ApplyTypeMappingVisitor::visit`]: Cycle detection (via a `seen` set) and result caching, used
//!   *only* for types that can introduce cycles (type aliases, function literals, newtypes).
//! - A depth counter incremented at every call to [`Type::apply_type_mapping_impl`] via
//!   [`ApplyTypeMappingVisitor::enter_depth`], providing universal stack overflow protection.

use std::cell::{Cell, RefCell};
use std::cmp::Eq;
use std::hash::Hash;
use std::marker::PhantomData;

use rustc_hash::FxHashMap;

use crate::FxIndexSet;
use crate::types::Type;

/// Maximum recursion depth for cycle detection.
///
/// This is a safety limit to prevent stack overflow when checking recursive generic protocols
/// that create infinitely growing type specializations. For example:
///
/// ```python
/// class C[T](Protocol):
///     a: 'C[set[T]]'
/// ```
///
/// When checking `C[set[int]]` against e.g. `C[Unknown]`, member `a` requires checking
/// `C[set[set[int]]]`, which in turn requires checking `C[set[set[set[int]]]]`, etc. Each level
/// creates a unique cache key, so the standard cycle detection doesn't catch it. The depth limit
/// ensures we bail out before hitting a stack overflow.
const MAX_RECURSION_DEPTH: u32 = 64;

#[derive(Debug)]
pub struct CycleDetector<Tag, T, R> {
    /// If the type we're visiting is present in `seen`, it indicates that we've hit a cycle (due
    /// to a recursive type); we need to immediately short circuit the whole operation and return
    /// the fallback value. That's why we pop items off the end of `seen` after we've visited them.
    seen: RefCell<FxIndexSet<T>>,

    /// Unlike `seen`, this field is a pure performance optimisation (and an essential one). If the
    /// type we're trying to normalize is present in `cache`, it doesn't necessarily mean we've hit
    /// a cycle: it just means that we've already visited this inner type as part of a bigger call
    /// chain we're currently in. Since this cache is just a performance optimisation, it doesn't
    /// make sense to pop items off the end of the cache after they've been visited (it would
    /// sort-of defeat the point of a cache if we did!)
    cache: RefCell<FxHashMap<T, R>>,

    /// Current recursion depth. Used to prevent stack overflow if recursive generic types create
    /// infinitely growing type specializations that don't trigger exact-match cycle detection.
    depth: Cell<u32>,

    fallback: R,

    _tag: PhantomData<Tag>,
}

impl<Tag, T, R> CycleDetector<Tag, T, R> {
    pub fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(FxIndexSet::default()),
            cache: RefCell::new(FxHashMap::default()),
            depth: Cell::new(0),
            fallback,
            _tag: PhantomData,
        }
    }
}

impl<Tag, T: Hash + Eq + Clone, R: Clone> CycleDetector<Tag, T, R> {
    pub fn visit(&self, item: T, func: impl FnOnce() -> R) -> R {
        if let Some(val) = self.cache.borrow().get(&item) {
            return val.clone();
        }

        // We hit a cycle
        if !self.seen.borrow_mut().insert(item.clone()) {
            return self.fallback.clone();
        }

        // Check depth limit to prevent stack overflow from recursive generic types
        // with growing specializations (e.g., C[set[T]] -> C[set[set[T]]] -> ...)
        let current_depth = self.depth.get();
        if current_depth >= MAX_RECURSION_DEPTH {
            self.seen.borrow_mut().pop();
            return self.fallback.clone();
        }
        self.depth.set(current_depth + 1);

        let ret = func();

        self.depth.set(current_depth);
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert(item, ret.clone());

        ret
    }
}

impl<Tag, T, R: Default> Default for CycleDetector<Tag, T, R> {
    fn default() -> Self {
        CycleDetector::new(R::default())
    }
}

/// Recursion guard for [`Type::apply_type_mapping`] operations.
///
/// This guard provides two orthogonal protections:
///
/// 1. Cycle detection for self-referential types, via [`Self::visit`].
///
///    Only type aliases, function literals (via `TypeOf`), and recursive newtypes can introduce
///    cycles during type mapping. The `visit` method tracks which types are being expanded and
///    short-circuits with a fallback (`Any`) when a cycle is detected.
///
/// 2. Depth limiting for stack overflow prevention, via [`Self::enter_depth`].
///
///    This is checked at every call to `Type::apply_type_mapping_impl`, providing universal
///    protection against unbounded recursion from any source (e.g. ever-growing generic
///    specializations).
#[derive(Debug, Default)]
pub(crate) struct ApplyTypeMappingVisitor<'db> {
    /// Types currently being expanded (type aliases, function literals, newtypes). If we
    /// encounter one of these again during expansion, we've hit a cycle and should return
    /// the fallback.
    seen: RefCell<FxIndexSet<Type<'db>>>,

    /// Cache of already-expanded results for cycle-introducing types. This is safe because
    /// a self-referential type always expands to the same approximation: the inner recursive
    /// reference always resolves to the fallback (currently always `Any`).
    cache: RefCell<FxHashMap<Type<'db>, Type<'db>>>,

    /// Global recursion depth, incremented at every call to `Type::apply_type_mapping_impl`.
    depth: Cell<u32>,
}

impl<'db> ApplyTypeMappingVisitor<'db> {
    /// Track expansion of a cycle-introducing type (type alias, function literal, or newtype).
    ///
    /// Returns the cached result if available, detects cycles via the `seen` set, and
    /// caches the result after expansion. Does *not* check depth (that is handled by
    /// [`Self::enter_depth`] at the top of [`Type::apply_type_mapping_impl`]).
    pub(crate) fn visit(&self, ty: Type<'db>, func: impl FnOnce() -> Type<'db>) -> Type<'db> {
        if let Some(val) = self.cache.borrow().get(&ty) {
            return *val;
        }

        // We hit a cycle
        if !self.seen.borrow_mut().insert(ty) {
            // TODO: proper recursive type handling
            return Type::any();
        }

        let ret = func();

        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert(ty, ret);

        ret
    }

    /// Check the recursion depth limit and increment the counter.
    ///
    /// Returns `Some(DepthGuard)` if within limits (the guard decrements on drop).
    /// Returns `None` if the depth limit has been exceeded (caller should return `Type::any()`).
    ///
    /// This should be called at the top of `Type::apply_type_mapping_impl` to provide
    /// universal stack overflow protection for all type variants.
    pub(crate) fn enter_depth(&self) -> Option<DepthGuard<'_>> {
        let previous = self.depth.get();
        if previous >= MAX_RECURSION_DEPTH {
            return None;
        }
        self.depth.set(previous + 1);
        Some(DepthGuard {
            depth: &self.depth,
            previous,
        })
    }
}

/// Guard that restores the recursion depth counter when dropped.
pub(crate) struct DepthGuard<'a> {
    depth: &'a Cell<u32>,
    previous: u32,
}

impl Drop for DepthGuard<'_> {
    fn drop(&mut self) {
        self.depth.set(self.previous);
    }
}
