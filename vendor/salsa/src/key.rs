use std::fmt;

use crate::Id;
use crate::function::VerifyResult;
use crate::zalsa::{IngredientIndex, Zalsa};

// ANCHOR: DatabaseKeyIndex
/// An integer that uniquely identifies a particular query instance within the
/// database. Used to track input and output dependencies between queries. Fully
/// ordered and equatable but those orderings are arbitrary, and meant to be used
/// only for inserting into maps and the like.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct DatabaseKeyIndex {
    key_index: Id,
    ingredient_index: IngredientIndex,
}
// ANCHOR_END: DatabaseKeyIndex

impl DatabaseKeyIndex {
    #[inline]
    pub(crate) const fn new(ingredient_index: IngredientIndex, key_index: Id) -> Self {
        Self {
            key_index,
            ingredient_index,
        }
    }

    pub const fn ingredient_index(self) -> IngredientIndex {
        self.ingredient_index
    }

    pub const fn key_index(self) -> Id {
        self.key_index
    }

    pub(crate) fn maybe_changed_after(
        &self,
        db: crate::database::RawDatabase<'_>,
        zalsa: &Zalsa,
        last_verified_at: crate::Revision,
    ) -> VerifyResult {
        // SAFETY: The `db` belongs to the ingredient
        unsafe {
            // here, `db` has to be either the correct type already, or a subtype (as far as trait
            // hierarchy is concerned)
            zalsa
                .lookup_ingredient(self.ingredient_index())
                .maybe_changed_after(zalsa, db, self.key_index(), last_verified_at)
        }
    }

    pub(crate) fn remove_stale_output(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex) {
        zalsa
            .lookup_ingredient(self.ingredient_index())
            .remove_stale_output(zalsa, executor, self.key_index())
    }

    pub(crate) fn mark_validated_output(
        &self,
        zalsa: &Zalsa,
        database_key_index: DatabaseKeyIndex,
    ) {
        zalsa
            .lookup_ingredient(self.ingredient_index())
            .mark_validated_output(zalsa, database_key_index, self.key_index())
    }
}

#[cfg(feature = "persistence")]
impl serde::Serialize for DatabaseKeyIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&(self.key_index, self.ingredient_index), serializer)
    }
}

#[cfg(feature = "persistence")]
impl<'de> serde::Deserialize<'de> for DatabaseKeyIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (key_index, ingredient_index) = serde::Deserialize::deserialize(deserializer)?;

        Ok(DatabaseKeyIndex {
            key_index,
            ingredient_index,
        })
    }
}

impl fmt::Debug for DatabaseKeyIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::attach::with_attached_database(|db| {
            let ingredient = db.zalsa().lookup_ingredient(self.ingredient_index());
            ingredient.fmt_index(self.key_index(), f)
        })
        .unwrap_or_else(|| {
            f.debug_tuple("DatabaseKeyIndex")
                .field(&self.ingredient_index())
                .field(&self.key_index())
                .finish()
        })
    }
}
