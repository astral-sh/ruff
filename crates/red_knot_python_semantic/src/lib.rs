use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;

use crate::lint::{LintRegistry, LintRegistryBuilder};
use crate::suppression::{INVALID_IGNORE_COMMENT, UNKNOWN_RULE, UNUSED_IGNORE_COMMENT};
pub use db::Db;
pub use module_name::ModuleName;
pub use module_resolver::{resolve_module, system_module_search_paths, KnownModule, Module};
pub use program::{Program, ProgramSettings, SearchPathSettings, SitePackages};
pub use python_platform::PythonPlatform;
pub use semantic_model::{HasType, SemanticModel};

pub mod ast_node_ref;
mod db;
pub mod lint;
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
pub mod syntax;
pub mod types;
mod unpack;
mod util;
mod visibility_constraints;

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

// TODO(brent) remove this. It should just be `Program::get(db).python_version(db)`, but for some
// reason `tests::check_file_skips_type_checking_when_file_cant_be_read` fails when I use `get`, so
// I factored this out instead of inlining everywhere
pub fn python_version(db: &dyn Db) -> ruff_python_ast::PythonVersion {
    Program::try_get(db)
        .map(|program| program.python_version(db))
        .unwrap_or_default()
}
