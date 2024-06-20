use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

pub mod ast_node_ref;
mod node_key;
pub mod semantic_index;
pub mod types;

pub(crate) type FxIndexSet<V> = indexmap::set::IndexSet<V, BuildHasherDefault<FxHasher>>;
