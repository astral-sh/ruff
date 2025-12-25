#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use std::hash::BuildHasherDefault;

use crate::lint::LintRegistryBuilder;
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
pub use site_packages::{PythonEnvironment, SitePackagesPaths, SysPrefixPathOrigin};
pub use suppression::{FileSuppressionId, TypeCheckDiagnostics, create_suppression_fix};
pub use ty_module_resolver::MisconfigurationMode;

pub mod ast_node_ref;
pub mod db;
pub(crate) mod has_tracked_scope;
pub mod lint;
pub mod list;
pub mod node_key;
pub mod place_qualifiers;
mod program;
mod python_platform;
pub mod rank;
pub mod semantic_index;
pub(crate) mod site_packages;
pub mod subscript;
pub mod suppression;
mod truthiness;
pub mod unpack;

pub use truthiness::Truthiness;

pub mod diagnostic;

pub type FxOrderMap<K, V> = ordermap::map::OrderMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FxOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<FxHasher>>;
pub type FxIndexMap<K, V> = indexmap::IndexMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FxIndexSet<V> = indexmap::IndexSet<V, BuildHasherDefault<FxHasher>>;

/// Register suppression-related lints (used by `ty_python_types` to build the full registry).
pub fn register_suppression_lints(registry: &mut LintRegistryBuilder) {
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
