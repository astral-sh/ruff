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

use rustc_hash::{FxHashMap, FxHashSet};

use crate::Db;
use crate::FxIndexSet;
use crate::types::Type;

pub(crate) type TypeTransformer<'db, Tag> = CycleDetector<Tag, Type<'db>, Type<'db>>;

impl<Tag> Default for TypeTransformer<'_, Tag> {
    fn default() -> Self {
        CycleDetector {
            seen: RefCell::new(FxIndexSet::default()),
            cache: RefCell::new(FxHashMap::default()),
            fallback: None,
            _tag: PhantomData,
        }
    }
}

pub(crate) type PairVisitor<'db, Tag, C> = CycleDetector<Tag, (Type<'db>, Type<'db>), C>;

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

    fallback: Option<R>,

    _tag: PhantomData<Tag>,
}

impl<Tag, T, R> CycleDetector<Tag, T, R> {
    pub fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(FxIndexSet::default()),
            cache: RefCell::new(FxHashMap::default()),
            fallback: Some(fallback),
            _tag: PhantomData,
        }
    }
}

impl<Tag, T: Hash + Eq + Clone, R: Clone> CycleDetector<Tag, T, R> {
    /// Some recursive types cannot be evaluated for equality using simple hash values.
    /// `is_cycle` provides a manual equality check.
    /// `on_cycle` returns the type to be used as a fallback during the cycle.
    fn visit_or_else(
        &self,
        item: T,
        is_cycle: impl FnOnce(&FxIndexSet<T>, &T) -> bool,
        on_cycle: impl FnOnce(T) -> R,
        func: impl FnOnce() -> R,
    ) -> R {
        if let Some(val) = self.cache.borrow().get(&item) {
            return val.clone();
        }

        // We hit a cycle
        if is_cycle(&self.seen.borrow(), &item) || !self.seen.borrow_mut().insert(item.clone()) {
            return on_cycle(item);
        }

        let ret = func();

        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert(item, ret.clone());

        ret
    }

    /// For `TypeTransformer`, use `visit_type` instead.
    pub fn visit(&self, item: T, func: impl FnOnce() -> R) -> R {
        debug_assert!(self.fallback.is_some());
        self.visit_or_else(
            item,
            FxIndexSet::contains,
            |_| self.fallback.clone().unwrap(),
            func,
        )
    }
}

impl<'db, Tag> TypeTransformer<'db, Tag> {
    fn same_type_identity(db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> bool {
        if left == right {
            return true;
        }

        match (left, right) {
            // We can create a self-referential function type: e.g. `def f(x: "TypeOf[f]"): reveal_type(x)`
            // To avoid the difficulty of equality checking for function types containing this, we simply use `literal` for equality checking.
            (Type::FunctionLiteral(left), Type::FunctionLiteral(right)) => {
                left.literal(db) == right.literal(db)
            }
            // Similarly, we can create a self-referential NewType: e.g. `T = NewType("T", list["T"])`
            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                left.definition(db) == right.definition(db)
            }
            _ => false,
        }
    }

    pub fn visit_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        func: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        self.visit_or_else(
            ty,
            |seen, ty| {
                seen.contains(ty)
                    || seen
                        .iter()
                        .any(|seen_type| Self::same_type_identity(db, *seen_type, *ty))
            },
            // When a cycle is encountered, the type being visited is returned as a fallback (typically a recursive type alias).
            |item| item,
            func,
        )
    }
}

impl<Tag, T, R: Default> Default for CycleDetector<Tag, T, R> {
    fn default() -> Self {
        CycleDetector::new(R::default())
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

        let ret = func();

        self.seen.borrow_mut().remove(item);

        ret
    }
}
