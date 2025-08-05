use crate::Db;

pub(crate) trait Constraints<'db>: Sized {
    fn never(db: &'db dyn Db) -> Self;
    fn always(db: &'db dyn Db) -> Self;
    fn union(&mut self, db: &'db dyn Db, other: Self);
    fn intersect(&mut self, db: &'db dyn Db, other: Self);

    fn distributed_union(db: &'db dyn Db, iter: impl IntoIterator<Item = Self>) -> Self {
        let mut result = Self::never(db);
        for child in iter {
            result.union(db, child);
        }
        result
    }

    fn distributed_intersection(db: &'db dyn Db, iter: impl IntoIterator<Item = Self>) -> Self {
        let mut result = Self::always(db);
        for child in iter {
            result.intersect(db, child);
        }
        result
    }
}

impl<'db> Constraints<'db> for bool {
    fn never(_db: &'db dyn Db) -> Self {
        false
    }

    fn always(_db: &'db dyn Db) -> Self {
        true
    }

    fn union(&mut self, _db: &'db dyn Db, other: Self) {
        *self = *self || other;
    }

    fn intersect(&mut self, _db: &'db dyn Db, other: Self) {
        *self = *self && other;
    }
}
