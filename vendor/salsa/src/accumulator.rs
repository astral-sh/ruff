//! Basic test of accumulator functionality.

use std::any::{Any, TypeId};
use std::fmt;
use std::marker::PhantomData;
use std::panic::UnwindSafe;

use accumulated::{Accumulated, AnyAccumulated};

use crate::function::VerifyResult;
use crate::hash::{FxHashSet, FxIndexSet};
use crate::ingredient::{Ingredient, Jar};
use crate::plumbing::ZalsaLocal;
use crate::sync::Arc;
use crate::table::memo::MemoTableTypes;
use crate::zalsa::{IngredientIndex, JarKind, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::{Database, DatabaseKeyIndex, Id, Revision};

mod accumulated;
pub(crate) mod accumulated_map;

/// Trait implemented on the struct that user annotated with `#[salsa::accumulator]`.
/// The `Self` type is therefore the types to be accumulated.
pub trait Accumulator: Send + Sync + Any + Sized + UnwindSafe {
    const DEBUG_NAME: &'static str;

    /// Accumulate an instance of this in the database for later retrieval.
    fn accumulate<Db>(self, db: &Db)
    where
        Db: ?Sized + Database;
}

pub struct JarImpl<A: Accumulator> {
    phantom: PhantomData<A>,
}

impl<A: Accumulator> Default for JarImpl<A> {
    fn default() -> Self {
        Self {
            phantom: Default::default(),
        }
    }
}

impl<A: Accumulator> Jar for JarImpl<A> {
    fn create_ingredients(
        _zalsa: &mut Zalsa,
        first_index: IngredientIndex,
    ) -> Vec<Box<dyn Ingredient>> {
        vec![Box::new(<IngredientImpl<A>>::new(first_index))]
    }

    fn id_struct_type_id() -> TypeId {
        TypeId::of::<A>()
    }
}

pub struct IngredientImpl<A: Accumulator> {
    index: IngredientIndex,
    phantom: PhantomData<Accumulated<A>>,
}

impl<A: Accumulator> IngredientImpl<A> {
    /// Find the accumulator ingredient for `A` in the database, if any.
    pub fn from_zalsa(zalsa: &Zalsa) -> Option<&Self> {
        let index = zalsa.lookup_jar_by_type::<JarImpl<A>>();
        let ingredient = zalsa.lookup_ingredient(index).assert_type::<Self>();
        Some(ingredient)
    }

    pub fn new(index: IngredientIndex) -> Self {
        Self {
            index,
            phantom: PhantomData,
        }
    }

    pub fn push(&self, zalsa_local: &ZalsaLocal, value: A) {
        if let Err(()) = zalsa_local.accumulate(self.index, value) {
            panic!("cannot accumulate values outside of an active tracked function");
        }
    }

    pub fn index(&self) -> IngredientIndex {
        self.index
    }
}

impl<A: Accumulator> Ingredient for IngredientImpl<A> {
    fn location(&self) -> &'static crate::ingredient::Location {
        &const {
            crate::ingredient::Location {
                file: file!(),
                line: line!(),
            }
        }
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.index
    }

    unsafe fn maybe_changed_after(
        &self,
        _zalsa: &crate::zalsa::Zalsa,
        _db: crate::database::RawDatabase<'_>,
        _input: Id,
        _revision: Revision,
    ) -> VerifyResult {
        panic!("nothing should ever depend on an accumulator directly")
    }

    fn collect_minimum_serialized_edges(
        &self,
        _zalsa: &Zalsa,
        _edge: QueryEdge,
        _serialized_edges: &mut FxIndexSet<QueryEdge>,
        _visited_edges: &mut FxHashSet<QueryEdge>,
    ) {
        panic!("nothing should ever depend on an accumulator directly")
    }

    fn debug_name(&self) -> &'static str {
        A::DEBUG_NAME
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::Struct
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        unreachable!("accumulator does not allocate pages")
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        unreachable!("accumulator does not allocate pages")
    }

    fn flatten_cycle_head_dependencies(
        &self,
        _zalsa: &Zalsa,
        _id: Id,
        _flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        _seen: &mut FxHashSet<DatabaseKeyIndex>,
    ) {
        panic!("nothing should ever depend on an accumulator directly")
    }
}

impl<A> std::fmt::Debug for IngredientImpl<A>
where
    A: Accumulator,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("index", &self.index)
            .finish()
    }
}
