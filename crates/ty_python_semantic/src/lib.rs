use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

use crate::lint::{LintRegistry, LintRegistryBuilder};
use crate::suppression::{INVALID_IGNORE_COMMENT, UNKNOWN_RULE, UNUSED_IGNORE_COMMENT};
pub use db::Db;
pub use module_name::ModuleName;
pub use module_resolver::{
    KnownModule, Module, SearchPathValidationError, SearchPaths, resolve_module,
    resolve_real_module, system_module_search_paths,
};
pub use program::{
    Program, ProgramSettings, PythonVersionFileSource, PythonVersionSource,
    PythonVersionWithSource, SearchPathSettings,
};
pub use python_platform::PythonPlatform;
pub use semantic_model::{Completion, CompletionKind, HasType, NameKind, SemanticModel};
pub use site_packages::{PythonEnvironment, SitePackagesPaths, SysPrefixPathOrigin};
pub use types::ide_support::{
    ResolvedDefinition, definitions_for_attribute, definitions_for_imported_symbol,
    definitions_for_name, map_stub_definition,
};
pub use util::diagnostics::add_inferred_python_version_hint_to_diagnostic;

pub mod ast_node_ref;
mod db;
mod dunder_all;
pub mod lint;
pub(crate) mod list;
mod module_name;
mod module_resolver;
mod node_key;
pub(crate) mod place;
mod program;
mod python_platform;
mod rank;
pub mod semantic_index;
mod semantic_model;
pub(crate) mod site_packages;
mod suppression;
pub mod types;
mod unpack;
mod util;

#[cfg(feature = "testing")]
pub mod pull_types;

type FxOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<FxHasher>>;
type FxIndexMap<K, V> = indexmap::IndexMap<K, V, BuildHasherDefault<FxHasher>>;
type FxIndexSet<V> = indexmap::IndexSet<V, BuildHasherDefault<FxHasher>>;

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
