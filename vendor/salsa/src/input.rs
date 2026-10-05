use std::any::{Any, TypeId};
use std::fmt;
use std::ops::IndexMut;

pub mod input_field;
pub mod setter;
pub mod singleton;

use input_field::FieldIngredientImpl;

use crate::function::VerifyResult;
use crate::function::execute::execution_run::explicit_reads::{
    OrdinaryAccess, assert_ordinary_execution_allowed,
};
use crate::hash::{FxHashSet, FxIndexSet};
use crate::id::{AsId, FromId, FromIdWithDb};
use crate::ingredient::Ingredient;
use crate::input::singleton::{Singleton, SingletonChoice};
use crate::key::DatabaseKeyIndex;
use crate::plumbing::{self, Jar, ZalsaLocal};
use crate::sync::Arc;
use crate::table::memo::{MemoTable, MemoTableTypes};
use crate::table::{Slot, Table};
use crate::zalsa::{IngredientIndex, JarKind, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::{Durability, Id, Revision, Runtime};

pub trait Configuration: Any {
    const DEBUG_NAME: &'static str;
    const FIELD_DEBUG_NAMES: &'static [&'static str];
    const LOCATION: crate::ingredient::Location;

    /// Whether this struct should be persisted with the database.
    const PERSIST: bool;

    /// The singleton state for this input if any.
    type Singleton: SingletonChoice + Send + Sync;

    /// The input struct (which wraps an `Id`)
    type Struct: FromId + AsId + 'static + Send + Sync;

    /// A (possibly empty) tuple of the fields for this struct.
    type Fields: 'static + Send + Sync;

    /// A array of [`Revision`], one per each of the value fields.
    #[cfg(feature = "persistence")]
    type Revisions: Send
        + Sync
        + fmt::Debug
        + IndexMut<usize, Output = Revision>
        + plumbing::serde::Serialize
        + for<'de> plumbing::serde::Deserialize<'de>;

    #[cfg(not(feature = "persistence"))]
    type Revisions: Send + Sync + fmt::Debug + IndexMut<usize, Output = Revision>;

    /// A array of [`Durability`], one per each of the value fields.
    #[cfg(feature = "persistence")]
    type Durabilities: Send
        + Sync
        + fmt::Debug
        + IndexMut<usize, Output = Durability>
        + plumbing::serde::Serialize
        + for<'de> plumbing::serde::Deserialize<'de>;

    #[cfg(not(feature = "persistence"))]
    type Durabilities: Send + Sync + fmt::Debug + IndexMut<usize, Output = Durability>;

    /// Returns the size of any heap allocations in the output value, in bytes.
    fn heap_size(_value: &Self::Fields) -> Option<usize> {
        None
    }

    /// Serialize the fields using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn serialize<S>(value: &Self::Fields, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: plumbing::serde::Serializer;

    /// Deserialize the fields using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn deserialize<'de, D>(deserializer: D) -> Result<Self::Fields, D::Error>
    where
        D: plumbing::serde::Deserializer<'de>;
}

pub struct JarImpl<C: Configuration> {
    _phantom: std::marker::PhantomData<C>,
}

impl<C: Configuration> Default for JarImpl<C> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<C: Configuration> Jar for JarImpl<C> {
    fn create_ingredients(
        _zalsa: &mut Zalsa,
        struct_index: crate::zalsa::IngredientIndex,
    ) -> Vec<Box<dyn Ingredient>> {
        let struct_ingredient: IngredientImpl<C> = IngredientImpl::new(struct_index);

        std::iter::once(Box::new(struct_ingredient) as _)
            .chain((0..C::FIELD_DEBUG_NAMES.len()).map(|field_index| {
                Box::new(<FieldIngredientImpl<C>>::new(struct_index, field_index)) as _
            }))
            .collect()
    }

    fn id_struct_type_id() -> TypeId {
        TypeId::of::<C::Struct>()
    }
}

pub struct IngredientImpl<C: Configuration> {
    ingredient_index: IngredientIndex,
    singleton: C::Singleton,
    memo_table_types: Arc<MemoTableTypes>,
    _phantom: std::marker::PhantomData<C::Struct>,
}

impl<C: Configuration> IngredientImpl<C> {
    pub fn new(index: IngredientIndex) -> Self {
        Self {
            ingredient_index: index,
            singleton: Default::default(),
            memo_table_types: Arc::new(MemoTableTypes::default()),
            _phantom: std::marker::PhantomData,
        }
    }

    fn data(zalsa: &Zalsa, id: Id) -> &Value<C> {
        zalsa.table().get(id)
    }

    fn data_raw(table: &Table, id: Id) -> *mut Value<C> {
        table.get_raw(id)
    }

    pub fn database_key_index(&self, id: Id) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.ingredient_index, id)
    }

    pub fn new_input(
        &self,
        zalsa: &Zalsa,
        zalsa_local: &ZalsaLocal,
        fields: C::Fields,
        revisions: C::Revisions,
        durabilities: C::Durabilities,
    ) -> C::Struct {
        assert_ordinary_execution_allowed((zalsa, zalsa_local).into(), "input constructor");
        let id = self.singleton.with_scope(|| {
            zalsa_local
                .allocate(zalsa, self.ingredient_index, |_| Value::<C> {
                    fields,
                    revisions,
                    durabilities,
                    // SAFETY: We only ever access the memos of a value that we allocated through
                    // our `MemoTableTypes`.
                    memos: unsafe { MemoTable::new(self.memo_table_types()) },
                })
                .0
        });

        FromIdWithDb::from_id(id, zalsa)
    }

    /// Change the value of the field `field_index` to a new value.
    ///
    /// # Parameters
    ///
    /// * `runtime`, the salsa runtiem
    /// * `id`, id of the input struct
    /// * `field_index`, index of the field that will be changed
    /// * `durability`, durability of the new value. If omitted, uses the durability of the previous value.
    /// * `setter`, function that modifies the fields tuple; should only modify the element for `field_index`
    ///
    /// # Panics
    ///
    /// Panics if the field's current durability is [`Durability::NEVER_CHANGE`].
    pub fn set_field<R>(
        &mut self,
        runtime: &mut Runtime,
        id: C::Struct,
        field_index: usize,
        durability: Option<Durability>,
        setter: impl FnOnce(&mut C::Fields) -> R,
    ) -> R {
        assert_ordinary_execution_allowed(OrdinaryAccess::Mutation, "input setter");
        let id: Id = id.as_id();

        let data_raw = Self::data_raw(runtime.table(), id);

        // SAFETY: We hold `&mut` on the runtime so no `&`-references can be active.
        // Also, we don't access any other data from the table while `r` is active.
        let data = unsafe { &mut *data_raw };

        assert_ne!(
            data.durabilities[field_index],
            Durability::NEVER_CHANGE,
            "never-changing inputs cannot be mutated"
        );

        data.revisions[field_index] = runtime.current_revision();

        let field_durability = &mut data.durabilities[field_index];
        if *field_durability != Durability::MIN {
            runtime.report_tracked_write(*field_durability);
        }
        *field_durability = durability.unwrap_or(*field_durability);

        setter(&mut data.fields)
    }

    /// Get the singleton input previously created (if any).
    #[doc(hidden)]
    pub fn get_singleton_input(&self, zalsa: &Zalsa) -> Option<C::Struct>
    where
        C: Configuration<Singleton = Singleton>,
    {
        assert_ordinary_execution_allowed(OrdinaryAccess::Storage(zalsa), "singleton input");
        self.singleton
            .index()
            .map(|id| FromIdWithDb::from_id(id, zalsa))
    }

    /// Access field of an input.
    /// Note that this function returns the entire tuple of value fields.
    /// The caller is responsible for selecting the appropriate element.
    pub fn field<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        id: C::Struct,
        field_index: usize,
    ) -> &'db C::Fields {
        assert_ordinary_execution_allowed((zalsa, zalsa_local).into(), "input field");
        let (fields, key, durability, changed_at) = self.select_field(zalsa, id, field_index);
        zalsa_local.report_tracked_read_simple(key, durability, changed_at);
        fields
    }

    pub(crate) fn select_field<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        id: C::Struct,
        field_index: usize,
    ) -> (&'db C::Fields, DatabaseKeyIndex, Durability, Revision) {
        let field_ingredient_index = self.ingredient_index.successor(field_index);
        let id = id.as_id();
        let value = Self::data(zalsa, id);
        let durability = value.durabilities[field_index];
        let revision = value.revisions[field_index];
        (
            &value.fields,
            DatabaseKeyIndex::new(field_ingredient_index, id),
            durability,
            revision,
        )
    }

    /// Returns all data corresponding to the input struct.
    pub fn entries<'db>(&'db self, zalsa: &'db Zalsa) -> impl Iterator<Item = StructEntry<'db, C>> {
        zalsa
            .table()
            .slots_of::<Value<C>>()
            .map(|(id, value)| StructEntry {
                value,
                key: self.database_key_index(id),
            })
    }

    /// Peek at the field values without recording any read dependency.
    /// Used for debug printouts.
    pub fn leak_fields<'db>(&'db self, zalsa: &'db Zalsa, id: C::Struct) -> &'db C::Fields {
        assert_ordinary_execution_allowed(OrdinaryAccess::Storage(zalsa), "input debug fields");
        let id = id.as_id();
        let value = Self::data(zalsa, id);
        &value.fields
    }
}

/// An input struct entry.
pub struct StructEntry<'db, C>
where
    C: Configuration,
{
    #[cfg_attr(not(feature = "salsa_unstable"), allow(dead_code))]
    value: &'db Value<C>,
    key: DatabaseKeyIndex,
}

impl<'db, C> StructEntry<'db, C>
where
    C: Configuration,
{
    /// Returns the `DatabaseKeyIndex` for this entry.
    pub fn key(&self) -> DatabaseKeyIndex {
        self.key
    }

    /// Returns the input struct.
    pub fn as_struct(&self) -> C::Struct {
        FromId::from_id(self.key.key_index())
    }

    #[cfg(feature = "salsa_unstable")]
    pub fn value(&self) -> &'db Value<C> {
        self.value
    }
}

impl<C: Configuration> Ingredient for IngredientImpl<C> {
    fn location(&self) -> &'static crate::ingredient::Location {
        &C::LOCATION
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.ingredient_index
    }

    unsafe fn maybe_changed_after(
        &self,
        _zalsa: &crate::zalsa::Zalsa,
        _db: crate::database::RawDatabase<'_>,
        _input: Id,
        _revision: Revision,
    ) -> VerifyResult {
        // Input ingredients are just a counter, they store no data, they are immortal.
        // Their *fields* are stored in function ingredients elsewhere.
        panic!("nothing should ever depend on an input struct directly")
    }

    fn collect_minimum_serialized_edges(
        &self,
        _zalsa: &Zalsa,
        _edge: QueryEdge,
        _serialized_edges: &mut FxIndexSet<QueryEdge>,
        _visited_edges: &mut FxHashSet<QueryEdge>,
    ) {
        panic!("nothing should ever depend on an input struct directly")
    }

    fn flatten_cycle_head_dependencies(
        &self,
        _zalsa: &Zalsa,
        _id: Id,
        _flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        _seen: &mut FxHashSet<DatabaseKeyIndex>,
    ) {
        panic!("nothing should ever depend on an input struct directly")
    }

    fn debug_name(&self) -> &'static str {
        C::DEBUG_NAME
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::Struct
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        &self.memo_table_types
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        &mut self.memo_table_types
    }

    /// Returns memory usage information about any inputs.
    #[cfg(feature = "salsa_unstable")]
    fn memory_usage(&self, db: &dyn crate::Database) -> Option<Vec<crate::database::SlotInfo>> {
        let memory_usage = self
            .entries(db.zalsa())
            // SAFETY: The memo table belongs to a value that we allocated, so it
            // has the correct type.
            .map(|entry| unsafe { entry.value.memory_usage(&self.memo_table_types) })
            .collect();

        Some(memory_usage)
    }

    fn is_persistable(&self) -> bool {
        C::PERSIST
    }

    fn should_serialize(&self, zalsa: &Zalsa) -> bool {
        C::PERSIST && self.entries(zalsa).next().is_some()
    }

    #[cfg(feature = "persistence")]
    unsafe fn serialize<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        f: &mut dyn FnMut(&dyn erased_serde::Serialize),
    ) {
        f(&persistence::SerializeIngredient {
            zalsa,
            _ingredient: self,
        })
    }

    #[cfg(feature = "persistence")]
    fn deserialize(
        &mut self,
        zalsa: &mut Zalsa,
        deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<(), erased_serde::Error> {
        let deserialize = persistence::DeserializeIngredient {
            zalsa,
            ingredient: self,
        };

        serde::de::DeserializeSeed::deserialize(deserialize, deserializer)
    }
}

impl<C: Configuration> std::fmt::Debug for IngredientImpl<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("index", &self.ingredient_index)
            .finish()
    }
}

#[derive(Debug)]
pub struct Value<C>
where
    C: Configuration,
{
    /// Fields of this input struct.
    ///
    /// They can change across revisions, but they do not change within
    /// a particular revision.
    fields: C::Fields,

    /// Revisions of the fields.
    revisions: C::Revisions,

    /// Durabilities of the fields.
    durabilities: C::Durabilities,

    /// Memos
    memos: MemoTable,
}

impl<C> Value<C>
where
    C: Configuration,
{
    /// Fields of this tracked struct.
    ///
    /// They can change across revisions, but they do not change within
    /// a particular revision.
    #[cfg(feature = "salsa_unstable")]
    pub fn fields(&self) -> &C::Fields {
        &self.fields
    }

    /// Returns memory usage information about the input.
    ///
    /// # Safety
    ///
    /// The `MemoTable` must belong to a `Value` of the correct type.
    #[cfg(feature = "salsa_unstable")]
    unsafe fn memory_usage(&self, memo_table_types: &MemoTableTypes) -> crate::database::SlotInfo {
        let heap_size = C::heap_size(&self.fields);
        // SAFETY: The caller guarantees this is the correct types table.
        let memos = unsafe { memo_table_types.attach_memos(&self.memos) };

        crate::database::SlotInfo {
            debug_name: C::DEBUG_NAME,
            size_of_metadata: std::mem::size_of::<Self>() - std::mem::size_of::<C::Fields>(),
            size_of_fields: std::mem::size_of::<C::Fields>(),
            heap_size_of_fields: heap_size,
            memos: memos.memory_usage(),
        }
    }
}

pub trait HasBuilder {
    type Builder;
}

// SAFETY: `Value<C>` is our private type branded over the unique configuration `C`.
unsafe impl<C> Slot for Value<C>
where
    C: Configuration,
{
    #[inline(always)]
    unsafe fn memos(
        this: *const Self,
        _current_revision: Revision,
    ) -> *const crate::table::memo::MemoTable {
        // SAFETY: Caller obligation demands this pointer to be valid.
        unsafe { &raw const (*this).memos }
    }

    #[inline(always)]
    fn memos_mut(&mut self) -> &mut crate::table::memo::MemoTable {
        &mut self.memos
    }
}

#[cfg(feature = "persistence")]
mod persistence {
    use std::fmt;

    use serde::ser::{SerializeMap, SerializeStruct};
    use serde::{Deserialize, de};

    use super::{Configuration, IngredientImpl, Value};
    use crate::Id;
    use crate::input::singleton::SingletonChoice;
    use crate::plumbing::Ingredient;
    use crate::table::memo::MemoTable;
    use crate::zalsa::Zalsa;

    pub struct SerializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db Zalsa,
        pub _ingredient: &'db IngredientImpl<C>,
    }

    impl<C> serde::Serialize for SerializeIngredient<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let Self { zalsa, .. } = self;

            let count = zalsa.table().slots_of::<Value<C>>().count();
            let mut map = serializer.serialize_map(Some(count))?;

            for (id, value) in zalsa.table().slots_of::<Value<C>>() {
                map.serialize_entry(&id.as_bits(), value)?;
            }

            map.end()
        }
    }

    impl<C> serde::Serialize for Value<C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let mut value = serializer.serialize_struct("Value", 3)?;

            let Value {
                fields,
                revisions,
                durabilities,
                memos: _,
            } = self;

            value.serialize_field("durabilities", &durabilities)?;
            value.serialize_field("revisions", &revisions)?;
            value.serialize_field("fields", &SerializeFields::<C>(fields))?;

            value.end()
        }
    }

    struct SerializeFields<'db, C: Configuration>(&'db C::Fields);

    impl<C> serde::Serialize for SerializeFields<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            C::serialize(self.0, serializer)
        }
    }

    pub struct DeserializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db mut Zalsa,
        pub ingredient: &'db mut IngredientImpl<C>,
    }

    impl<'de, C> de::DeserializeSeed<'de> for DeserializeIngredient<'_, C>
    where
        C: Configuration,
    {
        type Value = ();

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_map(self)
        }
    }

    impl<'de, C> de::Visitor<'de> for DeserializeIngredient<'_, C>
    where
        C: Configuration,
    {
        type Value = ();

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let DeserializeIngredient { zalsa, ingredient } = self;

            while let Some((id, value)) = access.next_entry::<u64, DeserializeValue<C>>()? {
                let id = Id::from_bits(id);
                let (page_idx, _) = crate::table::split_id(id);

                let value = Value::<C> {
                    fields: value.fields.0,
                    revisions: value.revisions,
                    durabilities: value.durabilities,
                    // SAFETY: We only ever access the memos of a value that we allocated through
                    // our `MemoTableTypes`.
                    memos: unsafe { MemoTable::new(ingredient.memo_table_types()) },
                };

                // Force initialize the relevant page.
                zalsa.table_mut().force_page::<Value<C>>(
                    page_idx,
                    ingredient.ingredient_index(),
                    ingredient.memo_table_types(),
                );

                // Initialize the slot.
                //
                // SAFETY: We have a mutable reference to the database.
                let allocated_id = ingredient.singleton.with_scope(|| unsafe {
                    zalsa
                        .table()
                        .page(page_idx)
                        .allocate(page_idx, |_| value)
                        .unwrap_or_else(|_| panic!("serialized an invalid `Id`: {id:?}"))
                        .0
                });

                assert_eq!(
                    allocated_id, id,
                    "values are serialized in allocation order"
                );
            }

            Ok(())
        }
    }

    #[derive(Deserialize)]
    #[serde(rename = "Value")]
    pub struct DeserializeValue<C: Configuration> {
        durabilities: C::Durabilities,
        revisions: C::Revisions,
        #[serde(bound = "C: Configuration")]
        fields: DeserializeFields<C>,
    }

    struct DeserializeFields<C: Configuration>(C::Fields);

    impl<'de, C> serde::Deserialize<'de> for DeserializeFields<C>
    where
        C: Configuration,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            C::deserialize(deserializer)
                .map(DeserializeFields)
                .map_err(de::Error::custom)
        }
    }
}
