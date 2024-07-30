use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

pub use db::Db;
pub use semantic_model::{HasTy, SemanticModel};

pub mod ast_node_ref;
mod builtins;
mod db;
mod node_key;
pub mod semantic_index;
mod semantic_model;
pub mod types;

type FxOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<FxHasher>>;
