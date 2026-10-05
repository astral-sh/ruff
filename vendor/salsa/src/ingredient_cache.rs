pub use imp::IngredientCache;

#[cfg(feature = "inventory")]
mod imp {
    use crate::IngredientIndex;
    use crate::plumbing::{Ingredient, Jar};
    use crate::sync::atomic::{self, AtomicU32, Ordering};
    use crate::zalsa::Zalsa;

    use std::any::{TypeId, type_name};
    use std::marker::PhantomData;

    /// Caches an ingredient index.
    ///
    /// Note that all ingredients are statically registered with `inventory`, so their
    /// indices should be stable across any databases.
    pub struct IngredientCache<I>
    where
        I: Ingredient,
    {
        ingredient_index: AtomicU32,
        phantom: PhantomData<fn() -> I>,
    }

    impl<I> Default for IngredientCache<I>
    where
        I: Ingredient,
    {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<I> IngredientCache<I>
    where
        I: Ingredient,
    {
        const UNINITIALIZED: u32 = u32::MAX;

        /// Create a new cache
        pub const fn new() -> Self {
            Self {
                ingredient_index: atomic::AtomicU32::new(Self::UNINITIALIZED),
                phantom: PhantomData,
            }
        }

        /// Get a reference to the ingredient in the database.
        ///
        /// If the ingredient index is not already in the cache, it will be loaded and cached.
        ///
        /// # Safety
        ///
        /// Ingredient `OFFSET` in `J` must have type `I`.
        pub unsafe fn get_or_create<'db, J: Jar, const OFFSET: usize>(
            &self,
            zalsa: &'db Zalsa,
        ) -> &'db I {
            let mut ingredient_index = self.ingredient_index.load(Ordering::Acquire);
            if ingredient_index == Self::UNINITIALIZED {
                ingredient_index = get_or_create_index_slow(
                    &self.ingredient_index,
                    zalsa,
                    TypeId::of::<J>(),
                    type_name::<J>(),
                    OFFSET,
                )
                .as_u32();
            };

            // SAFETY: `ingredient_index` is initialized from a valid `IngredientIndex`.
            let ingredient_index = unsafe { IngredientIndex::new_unchecked(ingredient_index) };

            // SAFETY: There are a two cases here:
            // - The jar was looked up because the data was uncached. In this case, the caller
            //   guarantees the index is in-bounds and has the correct type.
            // - The index was cached. While the current database might not be the same database
            //   the ingredient was initially loaded from, the `inventory` feature is enabled, so
            //   ingredient indices are stable across databases. Thus the index is still in-bounds
            //   and has the correct type.
            unsafe {
                zalsa
                    .lookup_ingredient_unchecked(ingredient_index)
                    .assert_type_unchecked()
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn get_or_create_index_slow(
        cached_index: &AtomicU32,
        zalsa: &Zalsa,
        jar_type_id: TypeId,
        jar_type_name: &'static str,
        ingredient_offset: usize,
    ) -> IngredientIndex {
        let ingredient_index = zalsa
            .lookup_jar_by_type_id(jar_type_id, jar_type_name)
            .at_offset(ingredient_offset);

        // It doesn't matter if we overwrite any stores, as the jar lookup should
        // always return the same index when the `inventory` feature is enabled.
        cached_index.store(ingredient_index.as_u32(), Ordering::Release);

        ingredient_index
    }
}

#[cfg(not(feature = "inventory"))]
mod imp {
    use crate::IngredientIndex;
    use crate::nonce::Nonce;
    use crate::plumbing::{Ingredient, Jar};
    use crate::sync::atomic::{AtomicU64, Ordering};
    use crate::zalsa::{StorageNonce, Zalsa};

    use std::any::{TypeId, type_name};
    use std::marker::PhantomData;
    use std::mem;

    const UNINITIALIZED: u64 = 0;

    /// Caches an ingredient index.
    ///
    /// With manual registration, ingredient indices can vary across databases,
    /// but we can retain most of the benefit by optimizing for the the case of
    /// a single database.
    pub struct IngredientCache<I>
    where
        I: Ingredient,
    {
        // A packed representation of `Option<(Nonce<StorageNonce>, IngredientIndex)>`.
        //
        // This allows us to replace a lock in favor of an atomic load. This works thanks to `Nonce`
        // having a niche, which means the entire type can fit into an `AtomicU64`.
        cached_data: AtomicU64,
        phantom: PhantomData<fn() -> I>,
    }

    impl<I> Default for IngredientCache<I>
    where
        I: Ingredient,
    {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<I> IngredientCache<I>
    where
        I: Ingredient,
    {
        /// Create a new cache
        pub const fn new() -> Self {
            Self {
                cached_data: AtomicU64::new(UNINITIALIZED),
                phantom: PhantomData,
            }
        }

        /// Get a reference to the ingredient in the database.
        ///
        /// If the ingredient is not already in the cache, it will be created.
        ///
        /// # Safety
        ///
        /// Ingredient `OFFSET` in `J` must have type `I`.
        #[inline(always)]
        pub unsafe fn get_or_create<'db, J: Jar, const OFFSET: usize>(
            &self,
            zalsa: &'db Zalsa,
        ) -> &'db I {
            let index = self.get_or_create_index::<J, OFFSET>(zalsa);

            // SAFETY: There are a two cases here:
            // - The jar was looked up because the data was uncached for the provided database.
            //   In this case, the caller guarantees the index is in-bounds and has the correct
            //   type.
            // - We verified the index was cached for the same database, by the nonce check.
            //   Thus the initial safety argument still applies.
            unsafe {
                zalsa
                    .lookup_ingredient_unchecked(index)
                    .assert_type_unchecked::<I>()
            }
        }

        fn get_or_create_index<J: Jar, const OFFSET: usize>(
            &self,
            zalsa: &Zalsa,
        ) -> IngredientIndex {
            const _: () = assert!(
                mem::size_of::<(Nonce<StorageNonce>, IngredientIndex)>() == mem::size_of::<u64>()
            );

            let cached_data = self.cached_data.load(Ordering::Acquire);
            if cached_data == UNINITIALIZED {
                return get_or_create_index_slow(
                    &self.cached_data,
                    zalsa,
                    TypeId::of::<J>(),
                    type_name::<J>(),
                    OFFSET,
                );
            };

            // Unpack our `u64` into the nonce and index.
            //
            // SAFETY: The lower bits of `cached_data` are initialized from a valid `IngredientIndex`.
            let index = unsafe { IngredientIndex::new_unchecked(cached_data as u32) };

            // SAFETY: We've checked against `UNINITIALIZED` (0) above and so the upper bits must be non-zero.
            let nonce = crate::nonce::Nonce::<StorageNonce>::from_u32(unsafe {
                std::num::NonZeroU32::new_unchecked((cached_data >> u32::BITS) as u32)
            });

            // The data was cached for a different database, we have to ensure the ingredient was
            // created in ours. Keep this call statically dispatched because this is the hot path
            // for programs that use multiple databases.
            if zalsa.nonce() != nonce {
                return zalsa.lookup_jar_by_type::<J>().at_offset(OFFSET);
            }

            index
        }
    }

    #[cold]
    #[inline(never)]
    fn get_or_create_index_slow(
        cache: &AtomicU64,
        zalsa: &Zalsa,
        jar_type_id: TypeId,
        jar_type_name: &'static str,
        ingredient_offset: usize,
    ) -> IngredientIndex {
        let index = zalsa
            .lookup_jar_by_type_id(jar_type_id, jar_type_name)
            .at_offset(ingredient_offset);
        let nonce = zalsa.nonce().into_u32().get() as u64;
        let packed = (nonce << u32::BITS) | (index.as_u32() as u64);
        debug_assert_ne!(packed, UNINITIALIZED);

        // Discard the result, whether we won over the cache or not doesn't matter.
        _ = cache.compare_exchange(UNINITIALIZED, packed, Ordering::Release, Ordering::Relaxed);

        // Use our locally computed index regardless of which one was cached.
        index
    }
}
