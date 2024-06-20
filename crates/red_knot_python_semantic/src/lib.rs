pub mod ast_node_ref;
mod db;
pub mod module;
pub mod name;
mod node_key;
pub mod semantic_index;
pub mod types;

type FxIndexSet<V> = indexmap::set::IndexSet<V, BuildHasherDefault<FxHasher>>;

pub use db::{Db, Jar};
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
