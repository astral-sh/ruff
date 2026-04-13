#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use std::hash::BuildHasherDefault;

use crate::lint::{LintRegistry, LintRegistryBuilder};
use crate::suppression::{
    IGNORE_COMMENT_UNKNOWN_RULE, INVALID_IGNORE_COMMENT, UNUSED_TYPE_IGNORE_COMMENT,
};
pub use db::Db;
pub use diagnostic::{
    add_inferred_python_version_hint_to_diagnostic, inferred_python_version_source_annotation,
};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use rustc_hash::FxHasher;
pub use semantic_model::{
    Completion, HasDefinition, HasOptionalDefinition, HasType, MemberDefinition, NameKind,
    SemanticModel,
};
pub use suppression::{
    UNUSED_IGNORE_COMMENT, is_unused_ignore_comment_lint, suppress_all, suppress_single,
};
use ty_module_resolver::ModuleGlobSet;
use ty_python_core::platform::PythonPlatform;
use ty_python_core::program::Program;
use ty_python_core::scope::ScopeId;
use ty_python_core::{
    BindingWithConstraintsIterator, DeclarationsIterator, FileScopeId, attribute_scopes,
    semantic_index,
};
pub use ty_site_packages::{
    PythonEnvironment, PythonVersionFileSource, PythonVersionSource, PythonVersionWithSource,
    SitePackagesPaths, SysPrefixPathOrigin,
};
pub use types::ide_support::{
    ImportAliasResolution, ResolvedDefinition, TypeHierarchyClass, definitions_for_attribute,
    definitions_for_bin_op, definitions_for_imported_symbol, definitions_for_name,
    definitions_for_unary_op, map_stub_definition, type_hierarchy_prepare, type_hierarchy_subtypes,
    type_hierarchy_supertypes,
};
pub use types::{DisplaySettings, TypeQualifiers};

mod db;
mod dunder_all;
pub mod lint;
pub(crate) mod place;
mod reachability;
mod semantic_model;
mod subscript;
mod suppression;
pub mod types;

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
    registry.register_lint(&UNUSED_TYPE_IGNORE_COMMENT);
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

    pub allowed_unresolved_imports: ModuleGlobSet,

    pub replace_imports_with_any: ModuleGlobSet,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            respect_type_ignore_comments: true,
            allowed_unresolved_imports: ModuleGlobSet::empty(),
            replace_imports_with_any: ModuleGlobSet::empty(),
        }
    }
}

/// Returns all attribute assignments (and their method scope IDs) with a symbol name matching
/// the one given for a specific class body scope.
///
/// Only call this when doing type inference on the same file as `class_body_scope`, otherwise it
/// introduces a direct dependency on that file's AST.
pub(crate) fn attribute_assignments<'db, 's>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
    name: &'s str,
) -> impl Iterator<Item = (BindingWithConstraintsIterator<'db, 'db>, FileScopeId)> + use<'s, 'db> {
    let file = class_body_scope.file(db);
    let index = semantic_index(db, file);

    attribute_scopes(db, class_body_scope).filter_map(|function_scope_id| {
        let place_table = index.place_table(function_scope_id);
        let member = place_table.member_id_by_instance_attribute_name(name)?;
        let use_def = index.use_def_map(function_scope_id);
        Some((use_def.reachable_member_bindings(member), function_scope_id))
    })
}

/// Returns all attribute declarations (and their method scope IDs) with a symbol name matching
/// the one given for a specific class body scope.
///
/// Only call this when doing type inference on the same file as `class_body_scope`, otherwise it
/// introduces a direct dependency on that file's AST.
pub(crate) fn attribute_declarations<'db, 's>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
    name: &'s str,
) -> impl Iterator<Item = (DeclarationsIterator<'db, 'db>, FileScopeId)> + use<'s, 'db> {
    let file = class_body_scope.file(db);
    let index = semantic_index(db, file);

    attribute_scopes(db, class_body_scope).filter_map(|function_scope_id| {
        let place_table = index.place_table(function_scope_id);
        let member = place_table.member_id_by_instance_attribute_name(name)?;
        let use_def = index.use_def_map(function_scope_id);
        Some((
            use_def.reachable_member_declarations(member),
            function_scope_id,
        ))
    })
}

/// Get the module-level docstring for the given file.
pub(crate) fn module_docstring(db: &dyn Db, file: File) -> Option<String> {
    let module = parsed_module(db, file).load(db);
    let stmt = module.suite().first()?;
    let ast::Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return None;
    };
    let docstring_expr = value.as_string_literal_expr()?;
    Some(docstring_expr.value.to_str().to_owned())
}
