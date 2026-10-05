use crate::sync::Arc;
use crate::table::memo::{MemoEntryType, MemoTableTypes};
use crate::zalsa::{MemoIngredientIndex, Zalsa};
use crate::{Id, IngredientIndex};

/// An ingredient has an [ingredient index][IngredientIndex]. However, Salsa also supports
/// enums of salsa structs (and other salsa enums), and those don't have a constant ingredient index,
/// because they are not ingredients by themselves but rather composed of them. However, an enum can
/// be viewed as a *set* of [`IngredientIndex`], where each instance of the enum can belong
/// to one, potentially different, index. This is what this type represents: a set of
/// `IngredientIndex`.
#[derive(Clone)]
pub struct IngredientIndices {
    indices: Box<[IngredientIndex]>,
}

impl From<IngredientIndex> for IngredientIndices {
    #[inline]
    fn from(value: IngredientIndex) -> Self {
        Self {
            indices: Box::new([value]),
        }
    }
}

impl IngredientIndices {
    #[inline]
    pub fn empty() -> Self {
        Self {
            indices: Box::default(),
        }
    }

    pub fn merge(iter: impl IntoIterator<Item = Self>) -> Self {
        let mut indices = Vec::new();
        for index in iter {
            indices.extend(index.indices);
        }
        indices.sort_unstable();
        indices.dedup();
        Self {
            indices: indices.into_boxed_slice(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = IngredientIndex> + '_ {
        self.indices.iter().copied()
    }
}

pub trait NewMemoIngredientIndices {
    /// # Safety
    ///
    /// The memo types must be correct.
    unsafe fn create(
        zalsa: &mut Zalsa,
        struct_indices: IngredientIndices,
        ingredient: IngredientIndex,
        memo_type: MemoEntryType,
        intern_ingredient_memo_types: Option<&mut Arc<MemoTableTypes>>,
    ) -> Self;
}

impl NewMemoIngredientIndices for MemoIngredientIndices {
    /// # Safety
    ///
    /// The memo types must be correct.
    unsafe fn create(
        zalsa: &mut Zalsa,
        struct_indices: IngredientIndices,
        ingredient: IngredientIndex,
        memo_type: MemoEntryType,
        _intern_ingredient_memo_types: Option<&mut Arc<MemoTableTypes>>,
    ) -> Self {
        debug_assert!(
            _intern_ingredient_memo_types.is_none(),
            "intern ingredient can only have a singleton memo ingredient"
        );

        let Some(&last) = struct_indices.indices.last() else {
            unreachable!("Attempting to construct struct memo mapping for non tracked function?")
        };

        let mut indices = Vec::new();
        indices.resize(
            (last.as_u32() as usize) + 1,
            MemoIngredientIndex::from_usize(Self::UNMAPPED),
        );

        for &struct_ingredient in &struct_indices.indices {
            let memo_ingredient_index =
                zalsa.next_memo_ingredient_index(struct_ingredient, ingredient);
            indices[struct_ingredient.as_u32() as usize] = memo_ingredient_index;

            let (struct_ingredient, _) = zalsa.lookup_ingredient_mut(struct_ingredient);
            let memo_types = Arc::get_mut(struct_ingredient.memo_table_types_mut())
                .expect("memo tables are not shared until database initialization is complete");

            memo_types.set(memo_ingredient_index, memo_type);
        }

        MemoIngredientIndices {
            indices: indices.into_boxed_slice(),
        }
    }
}

/// This type is to [`MemoIngredientIndex`] what [`IngredientIndices`] is to [`IngredientIndex`]:
/// since enums can contain different ingredient indices, they can also have different memo indices,
/// so we need to keep track of them.
///
/// This acts a map from [`IngredientIndex`] to [`MemoIngredientIndex`] but implemented
/// via a slice for fast lookups, trading memory for speed. With these changes, lookups are `O(1)`
/// instead of `O(n)`.
///
/// A database tends to have few ingredients (i), less function ingredients and even less
/// function ingredients targeting `#[derive(Supertype)]` enums (e).
/// While this is bounded as `O(i * e)` memory usage, the average case is significantly smaller: a
/// function ingredient targeting enums only stores a slice whose length corresponds to the largest
/// ingredient index's _value_. For example, if we have the ingredient indices `[2, 6, 17]`, then we
/// will allocate a slice whose length is `17 + 1`.
///
/// Assuming a heavy example scenario of 1000 ingredients (500 of which are function ingredients, 100
/// of which are enum targeting functions) this would come out to a maximum possibly memory usage of
/// 4bytes * 1000 * 100 ~= 0.38MB which is negligible.
pub struct MemoIngredientIndices {
    indices: Box<[MemoIngredientIndex]>,
}

impl MemoIngredientIndices {
    const UNMAPPED: usize = (u32::MAX - 1) as usize;
}

impl MemoIngredientMap for MemoIngredientIndices {
    #[inline(always)]
    fn get_zalsa_id(&self, zalsa: &Zalsa, id: Id) -> MemoIngredientIndex {
        self.get(zalsa.ingredient_index(id))
    }
    #[inline(always)]
    fn get(&self, index: IngredientIndex) -> MemoIngredientIndex {
        self.indices[index.as_u32() as usize]
    }

    fn get_checked(&self, index: IngredientIndex) -> Option<MemoIngredientIndex> {
        self.indices
            .get(index.as_u32() as usize)
            .copied()
            .filter(|index| index.as_usize() != Self::UNMAPPED)
    }
}

#[derive(Debug)]
pub struct MemoIngredientSingletonIndex(MemoIngredientIndex);

impl MemoIngredientMap for MemoIngredientSingletonIndex {
    #[inline(always)]
    fn get_zalsa_id(&self, _: &Zalsa, _: Id) -> MemoIngredientIndex {
        self.0
    }
    #[inline(always)]
    fn get(&self, _: IngredientIndex) -> MemoIngredientIndex {
        self.0
    }

    fn get_checked(&self, _: IngredientIndex) -> Option<MemoIngredientIndex> {
        Some(self.0)
    }
}

impl NewMemoIngredientIndices for MemoIngredientSingletonIndex {
    #[inline]
    unsafe fn create(
        zalsa: &mut Zalsa,
        indices: IngredientIndices,
        ingredient: IngredientIndex,
        memo_type: MemoEntryType,
        intern_ingredient_memo_types: Option<&mut Arc<MemoTableTypes>>,
    ) -> Self {
        let &[struct_ingredient] = &*indices.indices else {
            unreachable!("Attempting to construct struct memo mapping from enum?")
        };

        let memo_ingredient_index = zalsa.next_memo_ingredient_index(struct_ingredient, ingredient);
        let memo_types = intern_ingredient_memo_types.unwrap_or_else(|| {
            let (struct_ingredient, _) = zalsa.lookup_ingredient_mut(struct_ingredient);
            struct_ingredient.memo_table_types_mut()
        });

        Arc::get_mut(memo_types)
            .expect("memo tables are not shared until database initialization is complete")
            .set(memo_ingredient_index, memo_type);

        Self(memo_ingredient_index)
    }
}

pub trait MemoIngredientMap: Send + Sync {
    fn get_zalsa_id(&self, zalsa: &Zalsa, id: Id) -> MemoIngredientIndex;
    fn get(&self, index: IngredientIndex) -> MemoIngredientIndex;

    /// Returns a candidate slot without indexing outside a supertype's owner map.
    /// Singleton maps still require validation against the owner's actual memo types.
    fn get_checked(&self, index: IngredientIndex) -> Option<MemoIngredientIndex>;
}
