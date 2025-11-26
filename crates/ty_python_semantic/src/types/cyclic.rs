//! Cycle detection for recursive types.
//!
//! The visitors here (`TypeTransformer` and `PairVisitor`) are used in methods that recursively
//! visit types to transform them (e.g. `Type::normalize`) or to decide a relation between a pair
//! of types (e.g. `Type::has_relation_to`).
//!
//! The typical pattern is that the "entry" method (e.g. `Type::has_relation_to`) will create a
//! visitor and pass it to the recursive method (e.g. `Type::has_relation_to_impl`). Rust types
//! that form part of a complex type (e.g. tuples, protocols, nominal instances, etc) should
//! usually just implement the recursive method, and all recursive calls should call the recursive
//! method and pass along the visitor.
//!
//! Not all recursive calls need to actually call `.visit` on the visitor; only when visiting types
//! that can create a recursive relationship (this includes, for example, type aliases and
//! protocols).
//!
//! There is a risk of double-visiting, for example if `Type::has_relation_to_impl` calls
//! `visitor.visit` when visiting a protocol type, and then internal `has_relation_to_impl` methods
//! of the Rust types implementing protocols also call `visitor.visit`. The best way to avoid this
//! is to prefer always calling `visitor.visit` only in the main recursive method on `Type`.

use std::cell::RefCell;
use std::cmp::Eq;
use std::hash::Hash;
use std::marker::PhantomData;

use rustc_hash::FxHashMap;

use crate::FxIndexSet;
use crate::types::Type;

pub(crate) type TypeTransformer<'db, Tag> = CycleDetector<Tag, Type<'db>, Type<'db>>;

impl<Tag> Default for TypeTransformer<'_, Tag> {
    fn default() -> Self {
        // TODO: proper recursive type handling

        // This must be Any, not e.g. a todo type, because Any is the normalized form of the
        // dynamic type (that is, todo types are normalized to Any).
        CycleDetector::new(Type::any())
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

    fallback: R,

    _tag: PhantomData<Tag>,
}

impl<Tag, T: Hash + Eq + Clone, R: Clone> CycleDetector<Tag, T, R> {
    pub fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(FxIndexSet::default()),
            cache: RefCell::new(FxHashMap::default()),
            fallback,
            _tag: PhantomData,
        }
    }

    pub fn visit(&self, item: T, func: impl FnOnce() -> R) -> R {
        if let Some(val) = self.cache.borrow().get(&item) {
            return val.clone();
        }

        // We hit a cycle
        if !self.seen.borrow_mut().insert(item.clone()) {
            return self.fallback.clone();
        }

        let ret = func();
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert(item, ret.clone());

        ret
    }

    pub fn try_visit(&self, item: T, func: impl FnOnce() -> Option<R>) -> Option<R> {
        if let Some(val) = self.cache.borrow().get(&item) {
            return Some(val.clone());
        }

        // We hit a cycle
        if !self.seen.borrow_mut().insert(item.clone()) {
            return Some(self.fallback.clone());
        }

        let ret = func()?;
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert(item, ret.clone());

        Some(ret)
    }
}

impl<Tag, T: Hash + Eq + Clone, R: Default + Clone> Default for CycleDetector<Tag, T, R> {
    fn default() -> Self {
        CycleDetector::new(R::default())
    }
}
