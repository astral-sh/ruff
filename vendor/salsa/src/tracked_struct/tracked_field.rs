use std::marker::PhantomData;

use crate::function::VerifyResult;
use crate::hash::{FxHashSet, FxIndexSet};
use crate::ingredient::Ingredient;
use crate::sync::Arc;
use crate::table::memo::MemoTableTypes;
use crate::tracked_struct::{Configuration, Value};
use crate::zalsa::{IngredientIndex, JarKind, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::{DatabaseKeyIndex, Id};

/// Created for each tracked struct.
///
/// This ingredient only stores the "id" fields.
/// It is a kind of "dressed up" interner;
/// the active query + values of id fields are hashed to create the tracked struct id.
/// The value fields are stored in [`crate::function::IngredientImpl`] instances keyed by the tracked struct id.
/// Unlike normal interners, tracked struct indices can be deleted and reused aggressively:
/// when a tracked function re-executes,
/// any tracked structs that it created before but did not create this time can be deleted.
pub struct FieldIngredientImpl<C>
where
    C: Configuration,
{
    /// Index of this ingredient in the database (used to construct database-ids, etc).
    ingredient_index: IngredientIndex,

    /// The index of this field on the tracked struct relative to all other tracked fields.
    field_index: usize,
    phantom: PhantomData<fn() -> Value<C>>,
}

impl<C> FieldIngredientImpl<C>
where
    C: Configuration,
{
    pub(super) fn new(field_index: usize, ingredient_index: IngredientIndex) -> Self {
        Self {
            field_index,
            ingredient_index,
            phantom: PhantomData,
        }
    }

    fn database_key_index(&self, id: Id) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.ingredient_index, id)
    }
}

impl<C> Ingredient for FieldIngredientImpl<C>
where
    C: Configuration,
{
    fn location(&self) -> &'static crate::ingredient::Location {
        &C::LOCATION
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.ingredient_index
    }

    unsafe fn maybe_changed_after(
        &self,
        zalsa: &crate::zalsa::Zalsa,
        _db: crate::database::RawDatabase<'_>,
        input: Id,
        revision: crate::Revision,
    ) -> VerifyResult {
        let data = <super::IngredientImpl<C>>::data_raw(zalsa.table(), input);
        let field_changed_at = unsafe { (&(*data).revisions)[self.field_index].load() };
        VerifyResult::changed_if(field_changed_at > revision)
    }

    unsafe fn maybe_changed_after_leaf(
        &self,
        zalsa: &Zalsa,
        db: crate::database::RawDatabase<'_>,
        input: Id,
        revision: crate::Revision,
    ) -> Option<VerifyResult> {
        // SAFETY: The caller supplies the same database required by ordinary validation.
        Some(unsafe { self.maybe_changed_after(zalsa, db, input, revision) })
    }

    fn collect_minimum_serialized_edges(
        &self,
        _zalsa: &Zalsa,
        _edge: QueryEdge,
        _serialized_edges: &mut FxIndexSet<QueryEdge>,
        _visited_edges: &mut FxHashSet<QueryEdge>,
    ) {
        // Tracked fields do not have transitive dependencies, and their dependencies are covered by
        // the base inputs.
    }

    fn flatten_cycle_head_dependencies(
        &self,
        _zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        _seen: &mut FxHashSet<crate::DatabaseKeyIndex>,
    ) {
        flattened_input_outputs.insert(QueryEdge::input(self.database_key_index(id)));
    }

    fn fmt_index(&self, index: crate::Id, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            fmt,
            "{}.{}({:?})",
            C::DEBUG_NAME,
            C::TRACKED_FIELD_NAMES[self.field_index],
            index
        )
    }

    fn debug_name(&self) -> &'static str {
        C::TRACKED_FIELD_NAMES[self.field_index]
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::Struct
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        unreachable!("tracked field does not allocate pages")
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        unreachable!("tracked field does not allocate pages")
    }

    fn is_persistable(&self) -> bool {
        // Tracked field dependencies are valid as long as the tracked struct is persistable.
        C::PERSIST
    }

    fn should_serialize(&self, _zalsa: &Zalsa) -> bool {
        // However, they are never serialized directly.
        false
    }
}

impl<C> std::fmt::Debug for FieldIngredientImpl<C>
where
    C: Configuration,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("ingredient_index", &self.ingredient_index)
            .field("field_index", &self.field_index)
            .finish()
    }
}
