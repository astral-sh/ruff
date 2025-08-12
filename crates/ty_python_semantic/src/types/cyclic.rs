use rustc_hash::FxHashMap;

use crate::FxIndexSet;
use crate::types::Type;
use std::cell::RefCell;
use std::cmp::Eq;
use std::hash::Hash;

pub(crate) type TypeTransformer<'db> = CycleDetector<Type<'db>, Type<'db>>;

impl Default for TypeTransformer<'_> {
    fn default() -> Self {
        // TODO: proper recursive type handling

        // This must be Any, not e.g. a todo type, because Any is the normalized form of the
        // dynamic type (that is, todo types are normalized to Any).
        CycleDetector::new(Type::any())
    }
}

pub(crate) type PairVisitor<'db> = CycleDetector<(Type<'db>, Type<'db>), bool>;

#[derive(Debug)]
pub(crate) struct CycleDetector<T, R> {
    /// If the type we're visiting is present in `seen`,
    /// it indicates that we've hit a cycle (due to a recursive type);
    /// we need to immediately short circuit the whole operation and return the fallback value.
    /// That's why we pop items off the end of `seen` after we've visited them.
    seen: RefCell<FxIndexSet<T>>,

    /// Unlike `seen`, this field is a pure performance optimisation (and an essential one).
    /// If the type we're trying to normalize is present in `cache`, it doesn't necessarily mean we've hit a cycle:
    /// it just means that we've already visited this inner type as part of a bigger call chain we're currently in.
    /// Since this cache is just a performance optimisation, it doesn't make sense to pop items off the end of the
    /// cache after they've been visited (it would sort-of defeat the point of a cache if we did!)
    cache: RefCell<FxHashMap<T, R>>,

    fallback: R,
}

impl<T: Hash + Eq + Copy, R: Copy> CycleDetector<T, R> {
    pub(crate) fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(FxIndexSet::default()),
            cache: RefCell::new(FxHashMap::default()),
            fallback,
        }
    }

    pub(crate) fn visit(&self, item: T, func: impl FnOnce() -> R) -> R {
        if let Some(ty) = self.cache.borrow().get(&item) {
            return *ty;
        }

        // We hit a cycle
        if !self.seen.borrow_mut().insert(item) {
            return self.fallback;
        }

        let ret = func();
        self.seen.borrow_mut().pop();
        self.cache.borrow_mut().insert(item, ret);

        ret
    }
}
