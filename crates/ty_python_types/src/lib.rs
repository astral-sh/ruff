//! Python type system for ty.
//!
//! This crate contains the type definitions, type inference, and type checking logic.
//! It builds on top of `ty_python_semantic` which provides the semantic index
//! (scopes, symbols, definitions, use-def chains).

#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]

use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;
use ty_python_semantic::lint::{LintRegistry, LintRegistryBuilder};

pub mod db;
pub mod dunder_all;
pub mod place;
#[cfg(feature = "testing")]
pub mod pull_types;
pub mod reachability;
pub mod semantic_model;
pub mod types;

// Re-export from ty_python_semantic for convenience
pub use ty_python_semantic::Db;
pub use ty_python_semantic::Program;
pub use ty_python_semantic::Truthiness;

// Re-export extension traits from reachability module
pub use reachability::{
    ConstraintsIteratorExt, ReachabilityConstraintsExt, SemanticIndexExt, UseDefMapExt,
};

// Re-export the declare_lint macro (it's #[macro_export] so at crate root)
pub use ty_python_semantic::declare_lint;

// Re-export types that were previously exported from ty_python_semantic::types
pub use types::DisplaySettings;

// Re-export from semantic_model
pub use semantic_model::{
    Completion, HasDefinition, HasType, MemberDefinition, NameKind, SemanticModel,
};
pub use types::ide_support::{
    ImportAliasResolution, ResolvedDefinition, definitions_for_attribute, definitions_for_bin_op,
    definitions_for_imported_symbol, definitions_for_name, definitions_for_unary_op,
    map_stub_definition,
};

pub(crate) type FxOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<FxHasher>>;

/// Returns the default registry with all known lints.
pub fn default_lint_registry() -> &'static LintRegistry {
    static REGISTRY: std::sync::LazyLock<LintRegistry> = std::sync::LazyLock::new(|| {
        let mut registry = LintRegistryBuilder::default();
        register_lints(&mut registry);
        registry.build()
    });

    &REGISTRY
}

/// Register all known lints.
pub fn register_lints(registry: &mut LintRegistryBuilder) {
    types::register_lints(registry);
    ty_python_semantic::register_suppression_lints(registry);
}
