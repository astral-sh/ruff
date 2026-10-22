use crate::Db;
use crate::types::{ClassBase, KnownClass, StaticClassLiteral};

/// Returns whether `class` is a subclass of Django's `Model`.
#[expect(dead_code, reason = "Will be used by Django attribute synthesis")]
pub(in crate::types) fn is_model<'db>(db: &'db dyn Db, class: StaticClassLiteral<'db>) -> bool {
    class
        .iter_mro(db, None)
        .skip(1)
        .filter_map(ClassBase::into_class)
        .any(|base| base.is_known(db, KnownClass::DjangoModel))
}
