use crate::FxIndexSet;
use crate::types::Type;
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
pub(crate) struct CycleDetector<T: Hash + Eq, R: Copy> {
    seen: FxIndexSet<T>,
    fallback: R,
}

impl<T: Hash + Eq, R: Copy> CycleDetector<T, R> {
    pub(crate) fn new(fallback: R) -> Self {
        CycleDetector {
            seen: FxIndexSet::default(),
            fallback,
        }
    }

    pub(crate) fn visit(&mut self, item: T, func: impl FnOnce(&mut Self) -> R) -> R {
        if !self.seen.insert(item) {
            return self.fallback;
        }
        let ret = func(self);
        self.seen.pop();
        ret
    }
}
