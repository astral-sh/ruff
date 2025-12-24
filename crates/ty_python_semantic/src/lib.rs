#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use std::hash::BuildHasherDefault;

use crate::lint::{LintRegistry, LintRegistryBuilder};
use crate::suppression::{
    IGNORE_COMMENT_UNKNOWN_RULE, INVALID_IGNORE_COMMENT, UNUSED_IGNORE_COMMENT,
};
pub use db::Db;
pub use diagnostic::add_inferred_python_version_hint_to_diagnostic;
pub use program::{
    Program, ProgramSettings, PythonVersionFileSource, PythonVersionSource, PythonVersionWithSource,
};
pub use python_platform::PythonPlatform;
use rustc_hash::FxHasher;
pub use semantic_model::{
    Completion, HasDefinition, HasType, MemberDefinition, NameKind, SemanticModel,
};
pub use site_packages::{PythonEnvironment, SitePackagesPaths, SysPrefixPathOrigin};
pub use suppression::create_suppression_fix;
pub use ty_module_resolver::MisconfigurationMode;
pub use types::DisplaySettings;
pub use types::ide_support::{
    ImportAliasResolution, ResolvedDefinition, definitions_for_attribute, definitions_for_bin_op,
    definitions_for_imported_symbol, definitions_for_name, definitions_for_unary_op,
    map_stub_definition,
};

pub mod ast_node_ref;
mod db;
mod dunder_all;
pub mod lint;
pub(crate) mod list;
mod node_key;
pub(crate) mod place;
mod program;
mod python_platform;
mod rank;
pub mod semantic_index;
mod semantic_model;
pub(crate) mod site_packages;
mod subscript;
mod suppression;
pub mod types;
mod unpack;

mod diagnostic;
#[cfg(feature = "testing")]
pub mod pull_types;

type FxOrderMap<K, V> = ordermap::map::OrderMap<K, V, BuildHasherDefault<FxHasher>>;
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
    registry.register_lint(&IGNORE_COMMENT_UNKNOWN_RULE);
    registry.register_lint(&INVALID_IGNORE_COMMENT);
}

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
pub struct AnalysisSettings {
    /// Whether errors can be suppressed with `type: ignore` comments.
    ///
    /// If set to false, ty won't:
    ///
    /// * allow suppressing errors with `type: ignore` comments
    /// * report unused `type: ignore` comments
    /// * report invalid `type: ignore` comments
    pub respect_type_ignore_comments: bool,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            respect_type_ignore_comments: true,
        }
    }
}
