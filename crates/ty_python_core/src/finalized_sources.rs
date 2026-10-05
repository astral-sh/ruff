//! Typed access to the canonical scope-map ingredients for finalized source reads.

use salsa::plumbing::function::IngredientImpl;

use crate::{Db, place_table, use_def_map};

/// Returns the place-table ingredient without preparing or certifying any memo.
pub fn place_table_ingredient(db: &dyn Db) -> &IngredientImpl<crate::PlaceTableConfiguration> {
    place_table::fn_ingredient_(db, db.zalsa())
}

/// Returns the use-def-map ingredient without preparing or certifying any memo.
pub fn use_def_map_ingredient(db: &dyn Db) -> &IngredientImpl<crate::UseDefMapConfiguration> {
    use_def_map::fn_ingredient_(db, db.zalsa())
}
