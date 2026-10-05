use std::borrow::Cow;
use std::ptr::NonNull;

use crate::function::execute::execution_run::explicit_reads::assert_ordinary_execution_allowed;
use crate::views::DatabaseDownCaster;
use crate::zalsa::{IngredientIndex, ZalsaDatabase};
use crate::zalsa_local::CancellationToken;
use crate::{Durability, Revision};

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct RawDatabase<'db> {
    pub(crate) ptr: NonNull<()>,
    _marker: std::marker::PhantomData<&'db dyn Database>,
}

impl<'db, Db: Database + ?Sized> From<&'db Db> for RawDatabase<'db> {
    #[inline]
    fn from(db: &'db Db) -> Self {
        RawDatabase {
            ptr: NonNull::from(db).cast(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'db, Db: Database + ?Sized> From<&'db mut Db> for RawDatabase<'db> {
    #[inline]
    fn from(db: &'db mut Db) -> Self {
        RawDatabase {
            ptr: NonNull::from(db).cast(),
            _marker: std::marker::PhantomData,
        }
    }
}

/// The trait implemented by all Salsa databases.
/// You can create your own subtraits of this trait using the `#[salsa::db]`(`crate::db`) procedural macro.
pub trait Database: Send + ZalsaDatabase + AsDynDatabase {
    /// Enforces current LRU limits, evicting entries if necessary.
    ///
    /// **WARNING:** Just like an ordinary write, this method triggers
    /// cancellation. If you invoke it while a snapshot exists, it
    /// will block until that snapshot is dropped -- if that snapshot
    /// is owned by the current thread, this could trigger deadlock.
    fn trigger_lru_eviction(&mut self) {
        let zalsa_mut = self.zalsa_mut();
        zalsa_mut.evict_lru();
    }

    /// A "synthetic write" causes the system to act *as though* some
    /// input of durability `durability` has changed, triggering a new revision.
    /// This is mostly useful for profiling scenarios.
    ///
    /// **WARNING:** Just like an ordinary write, this method triggers
    /// cancellation. If you invoke it while a snapshot exists, it
    /// will block until that snapshot is dropped -- if that snapshot
    /// is owned by the current thread, this could trigger deadlock.
    ///
    /// # Panics
    ///
    /// Panics if `durability` is [`Durability::NEVER_CHANGE`].
    fn synthetic_write(&mut self, durability: Durability) {
        let zalsa_mut = self.zalsa_mut();
        zalsa_mut.new_revision();
        zalsa_mut.runtime_mut().report_tracked_write(durability);
    }

    /// This method cancels all outstanding computations.
    /// If you invoke it while a snapshot exists, it
    /// will block until that snapshot is dropped -- if that snapshot
    /// is owned by the current thread, this could trigger deadlock.
    fn trigger_cancellation(&mut self) {
        let _ = self.zalsa_mut();
    }

    /// Retrieves a [`CancellationToken`] for the current database handle.
    fn cancellation_token(&self) -> CancellationToken {
        self.zalsa_local().cancellation_token()
    }

    /// Reports that the query depends on some state unknown to salsa.
    ///
    /// Queries which report untracked reads will be re-executed in the next
    /// revision.
    fn report_untracked_read(&self) {
        assert_ordinary_execution_allowed(self.zalsas().into(), "untracked observation");
        let (zalsa, zalsa_local) = self.zalsas();
        zalsa_local.report_untracked_read(zalsa.current_revision())
    }

    /// Return the "debug name" (i.e., the struct name, etc) for an "ingredient",
    /// which are the fine-grained components we use to track data. This is intended
    /// for debugging and the contents of the returned string are not semver-guaranteed.
    ///
    /// Ingredient indices can be extracted from [`DatabaseKeyIndex`](`crate::DatabaseKeyIndex`) values.
    fn ingredient_debug_name(&self, ingredient_index: IngredientIndex) -> Cow<'_, str> {
        Cow::Borrowed(
            self.zalsa()
                .lookup_ingredient(ingredient_index)
                .debug_name(),
        )
    }

    /// Starts unwinding the stack if the current revision is cancelled.
    ///
    /// This method can be called by query implementations that perform
    /// potentially expensive computations, in order to speed up propagation of
    /// cancellation.
    ///
    /// Cancellation will automatically be triggered by salsa on any query
    /// invocation.
    ///
    /// This method should not be overridden by `Database` implementors. A
    /// `salsa_event` is emitted when this method is called, so that should be
    /// used instead.
    fn unwind_if_revision_cancelled(&self) {
        let (zalsa, zalsa_local) = self.zalsas();
        zalsa.unwind_if_revision_cancelled(zalsa_local);
    }

    /// Execute `op` with the database in thread-local storage for debug print-outs.
    #[inline(always)]
    fn attach<R>(&self, op: impl FnOnce(&Self) -> R) -> R
    where
        Self: Sized,
    {
        crate::attach::attach(self, || op(self))
    }

    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    fn zalsa_register_downcaster(&self) -> &DatabaseDownCaster<dyn Database> {
        self.zalsa().views().downcaster_for::<dyn Database>()
        // The no-op downcaster is special cased in view caster construction.
    }

    #[doc(hidden)]
    #[inline(always)]
    fn downcast(&self) -> &dyn Database
    where
        Self: Sized,
    {
        // No-op
        self
    }
}

/// Upcast to a `dyn Database`.
///
/// Only required because upcasting does not work for unsized generic parameters.
pub trait AsDynDatabase {
    fn as_dyn_database(&self) -> &dyn Database;
}

impl<T: Database> AsDynDatabase for T {
    #[inline(always)]
    fn as_dyn_database(&self) -> &dyn Database {
        self
    }
}

pub fn current_revision<Db: ?Sized + Database>(db: &Db) -> Revision {
    db.zalsa().current_revision()
}

#[cfg(feature = "persistence")]
mod persistence {
    use crate::plumbing::Ingredient;
    use crate::zalsa::Zalsa;
    use crate::{Database, IngredientIndex, Runtime};

    use std::fmt;

    use serde::de::{self, DeserializeSeed, SeqAccess};
    use serde::ser::SerializeMap;

    impl dyn Database {
        /// Returns a type implementing [`serde::Serialize`], that can be used to serialize the
        /// current state of the database.
        pub fn as_serialize(&mut self) -> impl serde::Serialize + '_ {
            SerializeDatabase {
                runtime: self.zalsa().runtime(),
                ingredients: SerializeIngredients(self.zalsa()),
            }
        }

        /// Deserialize the database using a [`serde::Deserializer`].
        ///
        /// This method will modify the database in-place based on the serialized data.
        pub fn deserialize<'db, D>(&mut self, deserializer: D) -> Result<(), D::Error>
        where
            D: serde::Deserializer<'db>,
        {
            DeserializeDatabase(self.zalsa_mut()).deserialize(deserializer)
        }
    }

    #[derive(serde::Serialize)]
    #[serde(rename = "Database")]
    pub struct SerializeDatabase<'db> {
        pub runtime: &'db Runtime,
        pub ingredients: SerializeIngredients<'db>,
    }

    pub struct SerializeIngredients<'db>(pub &'db Zalsa);

    impl serde::Serialize for SerializeIngredients<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let SerializeIngredients(zalsa) = self;

            let mut ingredients = zalsa
                .ingredients()
                .filter(|ingredient| ingredient.should_serialize(zalsa))
                .collect::<Vec<_>>();

            // Ensure structs are serialized before tracked functions, as deserializing a
            // memo requires its input struct to have been deserialized.
            ingredients.sort_by_key(|ingredient| ingredient.jar_kind());

            let mut map = serializer.serialize_map(Some(ingredients.len()))?;
            for ingredient in ingredients {
                map.serialize_entry(
                    &ingredient.ingredient_index().as_u32(),
                    &SerializeIngredient(ingredient, zalsa),
                )?;
            }

            map.end()
        }
    }

    struct SerializeIngredient<'db>(&'db dyn Ingredient, &'db Zalsa);

    impl serde::Serialize for SerializeIngredient<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let mut result = None;
            let mut serializer = Some(serializer);

            // SAFETY: `<dyn Database>::as_serialize` take `&mut self`.
            unsafe {
                self.0.serialize(self.1, &mut |serialize| {
                    let serializer = serializer.take().expect(
                        "`Ingredient::serialize` must invoke the serialization callback only once",
                    );

                    result = Some(erased_serde::serialize(&serialize, serializer))
                })
            };

            result.expect("`Ingredient::serialize` must invoke the serialization callback")
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(field_identifier, rename_all = "lowercase")]
    enum DatabaseField {
        Runtime,
        Ingredients,
    }

    pub struct DeserializeDatabase<'db>(pub &'db mut Zalsa);

    impl<'de> de::DeserializeSeed<'de> for DeserializeDatabase<'_> {
        type Value = ();

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            // Note that we have to deserialize using a manual visitor here because the
            // `Deserialize` derive does not support fields that use `DeserializeSeed`.
            deserializer.deserialize_struct("Database", &["runtime", "ingredients"], self)
        }
    }

    impl<'de> serde::de::Visitor<'de> for DeserializeDatabase<'_> {
        type Value = ();

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("struct Database")
        }

        fn visit_seq<V>(self, mut seq: V) -> Result<(), V::Error>
        where
            V: SeqAccess<'de>,
        {
            let mut runtime = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(0, &self))?;
            let () = seq
                .next_element_seed(DeserializeIngredients(self.0))?
                .ok_or_else(|| de::Error::invalid_length(1, &self))?;

            self.0.runtime_mut().deserialize_from(&mut runtime);
            Ok(())
        }

        fn visit_map<V>(self, mut map: V) -> Result<(), V::Error>
        where
            V: serde::de::MapAccess<'de>,
        {
            let mut runtime = None;
            let mut ingredients = None;

            while let Some(key) = map.next_key()? {
                match key {
                    DatabaseField::Runtime => {
                        if runtime.is_some() {
                            return Err(serde::de::Error::duplicate_field("runtime"));
                        }

                        runtime = Some(map.next_value()?);
                    }
                    DatabaseField::Ingredients => {
                        if ingredients.is_some() {
                            return Err(serde::de::Error::duplicate_field("ingredients"));
                        }

                        ingredients = Some(map.next_value_seed(DeserializeIngredients(self.0))?);
                    }
                }
            }

            let mut runtime = runtime.ok_or_else(|| serde::de::Error::missing_field("runtime"))?;
            let () = ingredients.ok_or_else(|| serde::de::Error::missing_field("ingredients"))?;

            self.0.runtime_mut().deserialize_from(&mut runtime);

            Ok(())
        }
    }

    struct DeserializeIngredients<'db>(&'db mut Zalsa);

    impl<'de> serde::de::Visitor<'de> for DeserializeIngredients<'_> {
        type Value = ();

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let DeserializeIngredients(zalsa) = self;

            while let Some(index) = access.next_key::<u32>()? {
                let index = IngredientIndex::new(index);

                // Remove the ingredient temporarily, to avoid holding an overlapping mutable borrow
                // to the ingredient as well as the database.
                let mut ingredient = zalsa.take_ingredient(index);

                // Deserialize the ingredient.
                access.next_value_seed(DeserializeIngredient(&mut *ingredient, zalsa))?;

                zalsa.replace_ingredient(index, ingredient);
            }

            Ok(())
        }
    }

    impl<'de> serde::de::DeserializeSeed<'de> for DeserializeIngredients<'_> {
        type Value = ();

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_map(self)
        }
    }

    struct DeserializeIngredient<'db>(&'db mut dyn Ingredient, &'db mut Zalsa);

    impl<'de> serde::de::DeserializeSeed<'de> for DeserializeIngredient<'_> {
        type Value = ();

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let deserializer = &mut <dyn erased_serde::Deserializer>::erase(deserializer);

            self.0
                .deserialize(self.1, deserializer)
                .map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(feature = "salsa_unstable")]
pub use memory_usage::{IngredientInfo, PageInfo};

#[cfg(feature = "salsa_unstable")]
pub(crate) use memory_usage::{MemoInfo, SlotInfo};

#[cfg(feature = "salsa_unstable")]
mod memory_usage {
    use hashbrown::HashMap;

    use crate::Database;

    impl dyn Database {
        /// Returns memory usage information about ingredients in the database.
        pub fn memory_usage(&self) -> DatabaseInfo {
            let mut queries = HashMap::new();
            let mut structs = Vec::new();
            let mut page_infos = self.zalsa().table().page_infos();
            let page_capacity = self.zalsa().table().page_capacity();

            for input_ingredient in self.zalsa().ingredients() {
                let Some(input_info) = input_ingredient.memory_usage(self) else {
                    continue;
                };

                let mut size_of_fields = 0;
                let mut size_of_metadata = 0;
                let mut count = 0;
                let mut heap_size_of_fields = None;

                for input_slot in input_info {
                    count += 1;
                    size_of_fields += input_slot.size_of_fields;
                    size_of_metadata += input_slot.size_of_metadata;

                    if let Some(slot_heap_size) = input_slot.heap_size_of_fields {
                        heap_size_of_fields =
                            Some(heap_size_of_fields.unwrap_or_default() + slot_heap_size);
                    }

                    for memo in input_slot.memos {
                        let info = queries.entry(memo.debug_name).or_insert(IngredientInfo {
                            debug_name: memo.output.debug_name,
                            ..Default::default()
                        });

                        info.count += 1;
                        info.size_of_fields += memo.output.size_of_fields;
                        info.size_of_metadata += memo.output.size_of_metadata;

                        if let Some(memo_heap_size) = memo.output.heap_size_of_fields {
                            info.heap_size_of_fields =
                                Some(info.heap_size_of_fields.unwrap_or_default() + memo_heap_size);
                        }
                    }
                }

                structs.push(IngredientInfo {
                    count,
                    size_of_fields,
                    size_of_metadata,
                    heap_size_of_fields,
                    debug_name: input_ingredient.debug_name(),
                    page_info: Some(
                        page_infos
                            .remove(&input_ingredient.ingredient_index())
                            .unwrap_or_else(|| PageInfo::empty(page_capacity)),
                    ),
                });
            }

            DatabaseInfo { structs, queries }
        }
    }

    /// Memory usage information about ingredients in the Salsa database.
    pub struct DatabaseInfo {
        /// Information about any Salsa structs.
        pub structs: Vec<IngredientInfo>,

        /// Memory usage information for memoized values of a given query, keyed
        /// by the query function name.
        pub queries: HashMap<&'static str, IngredientInfo>,
    }

    /// Information about instances of a particular Salsa ingredient.
    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct IngredientInfo {
        debug_name: &'static str,
        count: usize,
        size_of_metadata: usize,
        size_of_fields: usize,
        heap_size_of_fields: Option<usize>,
        page_info: Option<PageInfo>,
    }

    impl IngredientInfo {
        /// Returns the debug name of the ingredient.
        pub fn debug_name(&self) -> &'static str {
            self.debug_name
        }

        /// Returns the total stack size of the fields of any instances of this ingredient, in bytes.
        pub fn size_of_fields(&self) -> usize {
            self.size_of_fields
        }

        /// Returns the total heap size of the fields of any instances of this ingredient, in bytes.
        ///
        /// Returns `None` if the ingredient doesn't specify a `heap_size` function.
        pub fn heap_size_of_fields(&self) -> Option<usize> {
            self.heap_size_of_fields
        }

        /// Returns the total size of Salsa metadata of any instances of this ingredient, in bytes.
        pub fn size_of_metadata(&self) -> usize {
            self.size_of_metadata
        }

        /// Returns the number of instances of this ingredient.
        pub fn count(&self) -> usize {
            self.count
        }

        /// Returns page occupancy information for this ingredient.
        ///
        /// Returns `None` for query summaries. Struct ingredients without any non-empty pages
        /// return page information with zero counts.
        pub fn page_info(&self) -> Option<&PageInfo> {
            self.page_info.as_ref()
        }
    }

    /// Page occupancy information for a Salsa struct ingredient.
    ///
    /// Empty pages are excluded. Page fill is the number of slots that have been initialized, so
    /// slots remain included after their values are deleted or made available for reuse.
    /// Percentiles use the nearest-rank method across the ingredient's non-empty pages.
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct PageInfo {
        page_count: usize,
        page_capacity: usize,
        excess_capacity: usize,
        p25_fill: usize,
        p50_fill: usize,
        p75_fill: usize,
        p90_fill: usize,
        p99_fill: usize,
    }

    impl PageInfo {
        pub(crate) fn from_page_fills(
            page_capacity: usize,
            mut page_fills: Vec<usize>,
        ) -> Option<Self> {
            if page_fills.is_empty() {
                return None;
            }

            debug_assert!(
                page_fills
                    .iter()
                    .all(|&fill| fill > 0 && fill <= page_capacity)
            );
            page_fills.sort_unstable();

            let percentile = |percentile: usize| {
                let rank = (page_fills.len() * percentile).div_ceil(100);
                page_fills[rank - 1]
            };

            Some(Self {
                page_count: page_fills.len(),
                page_capacity,
                excess_capacity: page_fills.iter().map(|&fill| page_capacity - fill).sum(),
                p25_fill: percentile(25),
                p50_fill: percentile(50),
                p75_fill: percentile(75),
                p90_fill: percentile(90),
                p99_fill: percentile(99),
            })
        }

        fn empty(page_capacity: usize) -> Self {
            Self {
                page_count: 0,
                page_capacity,
                excess_capacity: 0,
                p25_fill: 0,
                p50_fill: 0,
                p75_fill: 0,
                p90_fill: 0,
                p99_fill: 0,
            }
        }

        /// Returns the number of non-empty pages allocated for this ingredient.
        pub fn page_count(&self) -> usize {
            self.page_count
        }

        /// Returns the number of slots that can be stored in each page.
        pub fn page_capacity(&self) -> usize {
            self.page_capacity
        }

        /// Returns the number of unused slots across all non-empty pages.
        pub fn excess_capacity(&self) -> usize {
            self.excess_capacity
        }

        /// Returns the 25th percentile of initialized slots per non-empty page.
        pub fn p25_fill(&self) -> usize {
            self.p25_fill
        }

        /// Returns the 50th percentile of initialized slots per non-empty page.
        pub fn p50_fill(&self) -> usize {
            self.p50_fill
        }

        /// Returns the 75th percentile of initialized slots per non-empty page.
        pub fn p75_fill(&self) -> usize {
            self.p75_fill
        }

        /// Returns the 90th percentile of initialized slots per non-empty page.
        pub fn p90_fill(&self) -> usize {
            self.p90_fill
        }

        /// Returns the 99th percentile of initialized slots per non-empty page.
        pub fn p99_fill(&self) -> usize {
            self.p99_fill
        }
    }

    /// Memory usage information about a particular instance of struct, input or output.
    pub struct SlotInfo {
        pub(crate) debug_name: &'static str,
        pub(crate) size_of_metadata: usize,
        pub(crate) size_of_fields: usize,
        pub(crate) heap_size_of_fields: Option<usize>,
        pub(crate) memos: Vec<MemoInfo>,
    }

    /// Memory usage information about a particular memo.
    pub struct MemoInfo {
        pub(crate) debug_name: &'static str,
        pub(crate) output: SlotInfo,
    }

    #[cfg(test)]
    mod tests {
        use super::PageInfo;

        #[test]
        fn page_info_uses_nearest_rank_percentiles() {
            let page_info = PageInfo::from_page_fills(128, (1..=100).collect()).unwrap();

            assert_eq!(page_info.page_count(), 100);
            assert_eq!(page_info.page_capacity(), 128);
            assert_eq!(page_info.excess_capacity(), 7_750);
            assert_eq!(page_info.p25_fill(), 25);
            assert_eq!(page_info.p50_fill(), 50);
            assert_eq!(page_info.p75_fill(), 75);
            assert_eq!(page_info.p90_fill(), 90);
            assert_eq!(page_info.p99_fill(), 99);
        }

        #[test]
        fn page_info_is_none_without_page_fills() {
            assert_eq!(PageInfo::from_page_fills(128, Vec::new()), None);
        }
    }
}
