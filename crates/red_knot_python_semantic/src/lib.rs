use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

pub use db::Db;
pub use module_name::ModuleName;
pub use module_resolver::{resolve_module, system_module_search_paths, Module};
pub use program::{Program, ProgramSettings, SearchPathSettings, SitePackages};
pub use python_version::PythonVersion;
pub use semantic_model::{HasTy, SemanticModel};

pub mod ast_node_ref;
mod db;
mod module_name;
mod module_resolver;
mod node_key;
mod program;
mod python_version;
pub mod semantic_index;
mod semantic_model;
pub(crate) mod site_packages;
mod stdlib;
pub mod types;

type FxOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<FxHasher>>;
