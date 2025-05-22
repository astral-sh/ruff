use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

use crate::lint::{LintRegistry, LintRegistryBuilder};
use crate::suppression::{INVALID_IGNORE_COMMENT, UNKNOWN_RULE, UNUSED_IGNORE_COMMENT};
pub use db::Db;
pub use module_name::ModuleName;
pub use module_resolver::{KnownModule, Module, resolve_module, system_module_search_paths};
pub use program::{
    Program, ProgramSettings, PythonPath, PythonVersionSource, PythonVersionWithSource,
    SearchPathSettings,
};
pub use python_platform::PythonPlatform;
pub use semantic_model::{HasType, SemanticModel};
pub use site_packages::SysPrefixPathOrigin;

pub mod ast_node_ref;
mod db;
mod dunder_all;
pub mod lint;
pub(crate) mod list;
mod module_name;
mod module_resolver;
mod node_key;
mod program;
mod python_platform;
pub mod semantic_index;
mod semantic_model;
pub(crate) mod site_packages;
mod suppression;
pub(crate) mod symbol;
pub mod types;
mod unpack;
mod util;

type FxOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<FxHasher>>;

/// Returns the default registry with all known semantic lints.
pub fn default_lint_registry() -> &'static LintRegistry {
    static REGISTRY: std::sync::LazyLock<LintRegistry> = std::sync::LazyLock::new(|| {
        let mut registry = LintRegistryBuilder::default();
        register_lints(&mut registry);
        registry.build()
    });

    &REGISTRY
}

/// Register all known semantic lints.
pub fn register_lints(registry: &mut LintRegistryBuilder) {
    types::register_lints(registry);
    registry.register_lint(&UNUSED_IGNORE_COMMENT);
    registry.register_lint(&UNKNOWN_RULE);
    registry.register_lint(&INVALID_IGNORE_COMMENT);
}
