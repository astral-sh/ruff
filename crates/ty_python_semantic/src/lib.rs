#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use crate::lint::{LintRegistry, LintRegistryBuilder};
use crate::suppression::{
    IGNORE_COMMENT_UNKNOWN_RULE, INVALID_IGNORE_COMMENT, UNUSED_TYPE_IGNORE_COMMENT,
};
use crate::types::check_types;
pub use db::Db;
pub use diagnostic::add_inferred_python_version_hint_to_diagnostic;
pub use fixes::suppress_all_diagnostics;
use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{SourceTextError, source_text};
use rustc_hash::FxHasher;
pub use semantic_model::{
    Completion, HasDefinition, HasOptionalDefinition, HasType, MemberDefinition, NameKind,
    SemanticModel,
};
use std::hash::BuildHasherDefault;
pub use suppression::{
    UNUSED_IGNORE_COMMENT, is_unused_ignore_comment_lint, suppress_all, suppress_single,
};
use ty_module_resolver::ModuleGlobSet;
use ty_python_core::definition::docstring_from_body;
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
mod fixes;
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
    docstring_from_body(module.suite())
        .map(|docstring_expr| docstring_expr.value.to_str().to_owned())
}

pub fn check_file_unwrap(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    check_file(db, file)
        .map(<[ruff_db::diagnostic::Diagnostic]>::into_vec)
        .unwrap_or_else(|error| vec![error])
}

pub fn check_file(db: &dyn Db, file: File) -> Result<Box<[Diagnostic]>, Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Abort checking if there are IO errors.
    let source = source_text(db, file);

    if let Some(read_error) = source.read_error() {
        return Err(IOErrorDiagnostic {
            file,
            error: read_error.clone(),
        }
        .to_diagnostic());
    }

    let parsed = parsed_module(db, file);

    let parsed_ref = parsed.load(db);
    diagnostics.extend(
        parsed_ref
            .errors()
            .iter()
            .map(|error| Diagnostic::invalid_syntax(file, &error.error, error)),
    );

    diagnostics.extend(parsed_ref.unsupported_syntax_errors().iter().map(|error| {
        let mut error = Diagnostic::invalid_syntax(file, error, error);
        add_inferred_python_version_hint_to_diagnostic(db, &mut error, "parsing syntax");
        error
    }));

    diagnostics.extend(check_types(db, file));

    diagnostics.sort_unstable_by(|a, b| {
        let db: &dyn ruff_db::Db = db;
        let resolver: &dyn ruff_db::diagnostic::FileResolver = &db;
        a.rendering_sort_key(resolver)
            .cmp(&b.rendering_sort_key(resolver))
    });

    Ok(diagnostics.into_boxed_slice())
}

#[derive(Debug, Clone, get_size2::GetSize)]
pub struct IOErrorDiagnostic {
    file: File,
    error: SourceTextError,
}

impl IOErrorDiagnostic {
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut diag = Diagnostic::new(DiagnosticId::Io, Severity::Error, &self.error);
        diag.annotate(Annotation::primary(Span::from(self.file)));
        diag
    }
}
