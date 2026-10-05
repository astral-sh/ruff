use std::any::{Any, TypeId};
use std::fmt;

use crate::database::RawDatabase;
use crate::function::{FunctionIngredientRef, VerifyResult};
use crate::hash::{FxHashSet, FxIndexSet};
use crate::sync::Arc;
use crate::table::Table;
use crate::table::memo::MemoTableTypes;
use crate::zalsa::{IngredientIndex, JarKind, Zalsa, transmute_data_mut_ptr, transmute_data_ptr};
use crate::zalsa_local::QueryEdge;
use crate::{DatabaseKeyIndex, Id, Revision};

/// A "jar" is a group of ingredients that are added atomically.
///
/// Each type implementing jar can be added to the database at most once.
pub trait Jar: Any {
    /// Create the ingredients given the index of the first one.
    ///
    /// All subsequent ingredients will be assigned contiguous indices.
    fn create_ingredients(
        zalsa: &mut Zalsa,
        first_index: IngredientIndex,
    ) -> Vec<Box<dyn Ingredient>>;

    /// This returns the [`TypeId`] of the ID struct, that is, the struct that wraps `salsa::Id`
    /// and carry the name of the jar.
    fn id_struct_type_id() -> TypeId;
}

pub struct Location {
    pub file: &'static str,
    pub line: u32,
}

pub trait Ingredient: Any + fmt::Debug + Send + Sync {
    fn debug_name(&self) -> &'static str;
    fn location(&self) -> &'static Location;
    fn jar_kind(&self) -> JarKind;

    /// Has the value for `input` in this ingredient changed after `revision`?
    ///
    /// # Safety
    ///
    /// The passed in database needs to be the same one that the ingredient was created with.
    unsafe fn maybe_changed_after(
        &self,
        zalsa: &Zalsa,
        db: RawDatabase<'_>,
        input: Id,
        revision: Revision,
    ) -> VerifyResult;

    /// Validates a leaf dependency without executing or recursively validating a query.
    /// An unsupported ingredient returns `None`, which is distinct from an unchanged value.
    ///
    /// # Safety
    ///
    /// The database must be the one this ingredient was created with.
    unsafe fn maybe_changed_after_leaf(
        &self,
        _zalsa: &Zalsa,
        _db: RawDatabase<'_>,
        _input: Id,
        _revision: Revision,
    ) -> Option<VerifyResult> {
        None
    }

    /// Collects the minimum edges necessary to serialize a given dependency edge on this ingredient,
    /// without necessarily serializing the dependency edge itself.
    ///
    /// This generally only returns any transitive input dependencies, i.e. the leaves of the dependency
    /// tree, as most other fine-grained dependencies are covered by the inputs.
    ///
    /// Note that any ingredients returned by this function must be persistable.
    fn collect_minimum_serialized_edges(
        &self,
        zalsa: &Zalsa,
        edge: QueryEdge,
        serialized_edges: &mut FxIndexSet<QueryEdge>,
        visited_edges: &mut FxHashSet<QueryEdge>,
    );

    /// Returns the function-specific interface for this ingredient, if any.
    #[doc(hidden)]
    fn as_function(&self) -> Option<FunctionIngredientRef<'_>> {
        None
    }

    /// Invoked when the value `output_key` should be marked as valid in the current revision.
    /// This occurs because the value for `executor`, which generated it, was marked as valid
    /// in the current revision.
    fn mark_validated_output(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex, output_key: Id) {
        let _ = (zalsa, executor, output_key);
        unreachable!("only tracked struct and function ingredients can have validatable outputs")
    }

    /// Invoked when the value `stale_output` was output by `executor` in a previous
    /// revision, but was NOT output in the current revision.
    ///
    /// This hook is used to clear out the stale value so others cannot read it.
    fn remove_stale_output(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex, stale_output_key: Id) {
        let _ = (zalsa, executor, stale_output_key);
        unreachable!("only tracked struct ingredients can have stale outputs")
    }

    /// Returns the [`IngredientIndex`] of this ingredient.
    fn ingredient_index(&self) -> IngredientIndex;

    /// Returns true if `reset_for_new_revision` should be called when new revisions start.
    /// Invoked once when ingredient is added and not after that.
    fn requires_reset_for_new_revision(&self) -> bool {
        false
    }

    /// Invoked when a new revision is about to start.
    /// This moment is important because it means that we have an `&mut`-reference to the
    /// database, and hence any pre-existing `&`-references must have expired.
    /// Many ingredients, given an `&'db`-reference to the database,
    /// use unsafe code to return `&'db`-references to internal values.
    /// The backing memory for those values can only be freed once an `&mut`-reference to the
    /// database is created.
    ///
    /// **Important:** to actually receive resets, the ingredient must set
    /// [`IngredientRequiresReset::RESET_ON_NEW_REVISION`] to true.
    fn reset_for_new_revision(&mut self, table: &mut Table) {
        _ = table;
        panic!(
            "Ingredient `{}` set `Ingredient::requires_reset_for_new_revision` to true but does \
            not overwrite `Ingredient::reset_for_new_revision`",
            self.debug_name()
        );
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes>;

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes>;

    fn fmt_index(&self, index: Id, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_index(self.debug_name(), index, fmt)
    }
    /// Flattens the dependencies of a query with cycle handling that participates in a cycle.
    ///
    /// This query recursively walks the dependency graph of `id` and flattens input dependencies
    /// on provisional queries into `flattened_input_outputs`. Outputs aren't flattened because
    /// outputs are owned by the creating query and not the cycle head.
    ///
    /// Flattening the dependencies is necessary because the memo's dependency graph only captures
    /// the dependencies from the last iteration. It also ensures that the dependency graph doesn't
    /// contain any cycles.
    ///
    /// See `cycle_dependency_order_different_entry_queries` and `cycle_left_recursive_query`.
    fn flatten_cycle_head_dependencies(
        &self,
        zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        seen: &mut FxHashSet<DatabaseKeyIndex>,
    );

    /// What values were accumulated during the creation of the value at `key_index`
    /// (if any).
    ///
    /// # Safety
    ///
    /// The passed in database needs to be the same one that the ingredient was created with.
    #[cfg(feature = "accumulator")]
    unsafe fn accumulated<'db>(
        &'db self,
        db: RawDatabase<'db>,
        key_index: Id,
    ) -> (
        Option<&'db crate::accumulator::accumulated_map::AccumulatedMap>,
        crate::accumulator::accumulated_map::InputAccumulatedValues,
    ) {
        let _ = (db, key_index);
        (
            None,
            crate::accumulator::accumulated_map::InputAccumulatedValues::Empty,
        )
    }

    /// Returns memory usage information about any instances of the ingredient,
    /// if applicable.
    #[cfg(feature = "salsa_unstable")]
    fn memory_usage(&self, _db: &dyn crate::Database) -> Option<Vec<crate::database::SlotInfo>> {
        None
    }

    /// Whether this ingredient will be persisted with the database.
    fn is_persistable(&self) -> bool {
        false
    }

    /// Whether there is data to serialize for this ingredient.
    ///
    /// If this returns `false`, the ingredient will not be serialized, even if `is_persistable`
    /// returns `true`.
    fn should_serialize(&self, _zalsa: &Zalsa) -> bool {
        false
    }

    /// Serialize the ingredient.
    ///
    /// This function should invoke the provided callback with a reference to an object implementing [`erased_serde::Serialize`].
    ///
    /// # Safety
    ///
    /// While this method takes an immutable reference to the database, it can only be called when a
    /// the serializer has exclusive access to the database.
    // See <https://github.com/dtolnay/erased-serde/issues/113> for why this callback signature is necessary, instead
    // of providing an `erased_serde::Serializer` directly.
    #[cfg(feature = "persistence")]
    unsafe fn serialize<'db>(
        &'db self,
        _zalsa: &'db Zalsa,
        _f: &mut dyn FnMut(&dyn erased_serde::Serialize),
    ) {
        unimplemented!("called `serialize` on ingredient where `should_serialize` returns `false`")
    }

    /// Deserialize the ingredient.
    #[cfg(feature = "persistence")]
    fn deserialize(
        &mut self,
        _zalsa: &mut Zalsa,
        _deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<(), erased_serde::Error> {
        unimplemented!(
            "called `deserialize` on ingredient where `should_serialize` returns `false`"
        )
    }
}

impl dyn Ingredient {
    /// Equivalent to the `downcast` method on `Any`.
    ///
    /// Because we do not have dyn-downcasting support, we need this workaround.
    pub fn assert_type<T: Any>(&self) -> &T {
        assert_eq!(
            self.type_id(),
            TypeId::of::<T>(),
            "ingredient `{self:?}` is not of type `{}`",
            std::any::type_name::<T>()
        );

        // SAFETY: We know that the underlying data pointer
        // refers to a value of type T because of the `TypeId` check above.
        unsafe { transmute_data_ptr(self) }
    }

    /// Equivalent to the `downcast` methods on `Any`.
    ///
    /// Because we do not have dyn-downcasting support, we need this workaround.
    ///
    /// # Safety
    ///
    /// The contained value must be of type `T`.
    pub unsafe fn assert_type_unchecked<T: Any>(&self) -> &T {
        debug_assert_eq!(
            self.type_id(),
            TypeId::of::<T>(),
            "ingredient `{self:?}` is not of type `{}`",
            std::any::type_name::<T>()
        );

        // SAFETY: Guaranteed by caller.
        unsafe { transmute_data_ptr(self) }
    }

    /// Equivalent to the `downcast` method on `Any`.
    ///
    /// Because we do not have dyn-downcasting support, we need this workaround.
    pub fn assert_type_mut<T: Any>(&mut self) -> &mut T {
        assert_eq!(
            Any::type_id(self),
            TypeId::of::<T>(),
            "ingredient `{self:?}` is not of type `{}`",
            std::any::type_name::<T>()
        );

        // SAFETY: We know that the underlying data pointer
        // refers to a value of type T because of the `TypeId` check above.
        unsafe { transmute_data_mut_ptr(self) }
    }
}

/// A helper function to show human readable fmt.
pub(crate) fn fmt_index(debug_name: &str, id: Id, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(fmt, "{debug_name}({id:?})")
}
