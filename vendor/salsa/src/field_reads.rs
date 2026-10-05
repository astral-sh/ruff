use crate::Database;
use crate::function::execute::execution_run::explicit_reads::{
    OrdinaryAccess, assert_ordinary_execution_allowed,
};
use crate::ingredient_cache::IngredientCache;
use crate::interned::{Configuration, IngredientImpl, JarImpl};
use crate::zalsa::Zalsa;

/// Access to existing interned fields without database query or constructor access.
///
/// Pass this capability to a generated field view enabled with
/// `#[salsa::interned(field_view = read_fields)]`. The view returns references to the
/// stored field types, independently of each ordinary getter's return mode.
#[derive(Clone, Copy)]
pub struct FieldReads<'db> {
    zalsa: &'db Zalsa,
}

impl<'db> FieldReads<'db> {
    /// Borrow the database storage without reading a field or looking up an ingredient.
    #[inline]
    pub fn new(db: &'db dyn Database) -> Self {
        Self { zalsa: db.zalsa() }
    }
}

/// Read the canonical interned tuple using the generated configuration and ingredient cache.
#[doc(hidden)]
#[inline]
pub fn interned_fields<'db, C: Configuration>(
    fields: FieldReads<'db>,
    value: C::Struct<'db>,
    cache: &IngredientCache<IngredientImpl<C>>,
) -> &'db C::Fields<'db> {
    assert_ordinary_execution_allowed(OrdinaryAccess::Storage(fields.zalsa), "interned field view");
    // SAFETY: JarImpl<C> places IngredientImpl<C> at offset zero. The typed cache
    // and canonical fields accessor preserve its database and reusable-value checks.
    let ingredient = unsafe { cache.get_or_create::<JarImpl<C>, 0>(fields.zalsa) };
    ingredient.select_fields(fields.zalsa, value)
}
