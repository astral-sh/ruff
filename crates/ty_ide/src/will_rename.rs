//! Computes source edits for Python file renames.
//!
//! ## Overview
//!
//! [`will_rename_files`] maps filesystem renames to module names, then computes edits for import
//! statements and references in indexed project files. It does not move files, discover package
//! contents, validate the filesystem operation, or normalize the returned edits.
//!
//! EXAMPLE:
//!
//! When `pkg/old.py` is renamed to `pkg/new.py`, the following code:
//!
//! ```python
//! from pkg import old
//! print(old.C)
//! ```
//!
//! is updated like so:
//!
//! ```python
//! from pkg import new
//! print(new.C)
//! ```
//!
//! [`will_rename_files`] computes edits over all files in parallel using [`edits_for_file`]. That
//! in turn computes edits for a single file in two distinct AST passes: [`ImportStatementVisitor`]
//! visits import statements and computes edits for them, then [`ModuleReferenceVisitor`] visits
//! all references to imported modules and computes edits for those references. Those two sets of
//! edits are combined to produce the final result.
//!
//! ## Terminology
//!
//! Below is the anatomy of the two types of import statement.
//!
//! A "plain" import statement:
//!
//! ```text
//!              Alias
//!        +---------------+
//! import pkg.old as stable
//!        |          |
//!        |          explicit alias
//!        imported name
//! ```
//!
//! and an "import-from" statement:
//!
//! ```text
//!                       Alias
//!                   +-----------+
//! from ..pkg import old as stable
//!      | |            |      |
//!      | module name  |      explicit alias
//!      |              imported name
//!      |
//!      import level
//! ```
//!
//! From there, we can layer on a few additional terms.
//!
//! | Term | Meaning | Example |
//! | --- | --- | --- |
//! | Bound name | A name introduced by an import. | `stable` in both diagrams. |
//! | Module reference | The use of a module in an expression. | `old` in `old.C`; `pkg.old` in `pkg.old.C`. |
//! | Re-export | A name that a module imports from elsewhere and makes available through its own namespace. | `facade.py`: `from pkg import old`; elsewhere: `from facade import old`. |
//! | Imported name edit | Replacement of an imported name. | `import old` → `import new`, `import pkg.old` → `import pkg.new` |
//! | Module name edit | Replacement of the module name in an import-from statement. | `from old import C` → `from new import C`. |
//! | Bound name change | Change to a bound name caused by an imported name edit. | `old` → `new`; no bound name change for an explicit alias like `as stable`. |
//! | Reference edit | Replacement at a module reference. | `old.C` → `new.C`. |
//! | Source edit | Source range and replacement text. | Range of `old`, replacement `new`. |
//!
//! The above terms are used consistently throughout this module.

use crate::RangedValue;
use rayon::prelude::*;
use ruff_db::files::{File, FileRange};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_trivia::NameMatcher;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{
    Module, ModuleName, ModuleResolveMode, ResolverEnvironment, ResolverFile, file_to_module,
    resolve_module_confident, resolve_real_module_confident, search_paths,
};
use ty_project::{Db, parallel::ParallelIteratorExt};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_semantic::types::{ReachingDefinitionsByExpression, Type};
use ty_python_semantic::{DefinitionResolution, HasType, SemanticModel};

/// Computes source edits for a batch of Python file renames.
///
/// The following types of rename request are not yet supported:
///
/// - A request that renames a directory
/// - A request that changes a module parent (e.g., renaming `a.old` to `b.new` or `new`)
/// - A request that renames an `__init__.py` or `__init__.pyi` file
/// - A request that changes the extension of a module (e.g., from `.py` to `.pyi`)
///
/// When a rename batch contains both supported and unsupported requests, the unsupported requests
/// are skipped while the supported ones produce edits as normal.
///
/// The returned edits refer to the original files and source ranges. The caller must sort the edits
/// and handle duplicates and overlaps before applying them.
///
/// ## Unsupported edits
///
/// Even for supported renames, some occurrences are not updated:
///
/// - Name references supplied by star imports, such as `from pkg import *`.
/// - Names in `global` and `nonlocal` declarations, and references whose resolution crosses those
///   declarations.
/// - Names and attributes being assigned to or deleted. Receiver expressions can still be updated.
/// - Names in ordinary strings, including `__all__` entries and dynamic imports (although module
///   references in string annotations are supported).
///
/// Other edits may still be returned, so an import can be updated while a reference to its old
/// name remains unchanged.
///
/// Additionally, an ambiguous imported name is left unchanged even when other names in the same
/// import-from statement are updated. For example, suppose we rename `old.py` to `new.py` and
/// `other.py` to `another.py`:
///
/// ```python
/// # facade.py
/// import old
/// import other
/// if flag:
///     old = 0
///
/// # use.py
/// from facade import other, old
/// ```
///
/// In the import from `facade`, `old` may or may not refer to the renamed module, so it is left
/// unchanged. The import becomes `from facade import another, old`.
fn will_rename_files(db: &dyn Db, renames: &[FileRename]) -> Vec<FileRenameEdit> {
    let files: Vec<_> = db.project().files(db).iter().collect();
    let module_name_changes = ModuleNameChanges::new(db, renames);
    if module_name_changes.is_empty() {
        return Vec::new();
    }

    files
        .into_par_iter()
        .map_with_db(db, |db, file| {
            edits_for_file(db, file, &module_name_changes)
        })
        .flatten()
        .collect()
}

/// One Python file rename in a batch.
struct FileRename {
    /// The source file before the rename.
    file: File,
    /// The destination path, which need not exist yet.
    new_path: SystemPathBuf,
}

/// A replacement and the file range containing it.
type FileRenameEdit = RangedValue<String>;

/// Maps old module names to new module names for supported file renames.
struct ModuleNameChanges<'db> {
    // A map from old module name to new module name for supported rename operations.
    module_renames: FxHashMap<&'db ModuleName, ModuleName>,
    // The module basename for each key in `module_renames` (e.g., `old` in `pkg.old`).
    old_module_basenames: FxHashSet<&'db str>,
    // Used to check whether a source file may contain an old module basename.
    old_module_basename_matchers: Vec<NameMatcher<'db>>,
}

impl<'db> ModuleNameChanges<'db> {
    fn new(db: &'db dyn Db, renames: &[FileRename]) -> Self {
        let module_renames: FxHashMap<_, _> = renames
            .iter()
            .filter_map(|rename| module_rename(db, rename))
            .filter(|(old_module_name, new_module_name)| *old_module_name != new_module_name)
            .collect();
        let old_module_basenames: FxHashSet<&'db str> = module_renames
            .keys()
            .map(|&module_name| module_name.last_component())
            .collect();
        let old_module_basename_matchers = old_module_basenames
            .iter()
            .map(|&basename| NameMatcher::new(basename))
            .collect();
        Self {
            module_renames,
            old_module_basenames,
            old_module_basename_matchers,
        }
    }

    fn is_empty(&self) -> bool {
        self.module_renames.is_empty()
    }

    fn new_module_name(&self, old_module_name: &ModuleName) -> Option<&ModuleName> {
        self.module_renames.get(old_module_name)
    }

    fn is_old_module_basename(&self, name: &str) -> bool {
        self.old_module_basenames.contains(name)
    }

    /// Returns whether the source may need edits for these module name changes.
    fn may_affect_source(&self, source: &str) -> bool {
        self.old_module_basename_matchers
            .iter()
            .any(|matcher| matcher.may_match(source))
    }
}

/// Computes edits for one candidate file in two AST passes.
///
/// 1. [`ImportStatementVisitor`] traverses import statements in the AST to compute edits to those
///    statements specifically, and to record bound name changes.
/// 2. [`ModuleReferenceVisitor`] traverses many different AST nodes to computes edits to each
///    reference to an imported module. Reaching definitions determine which module references need
///    updating after a bound name change.
///
/// Computing import statement edits first lets us base reference edits on the bound name changes,
/// accounting for explicit aliases and re-exports. A rejected imported name edit leaves its
/// bound name unchanged.
fn edits_for_file(
    db: &dyn Db,
    file: File,
    module_name_changes: &ModuleNameChanges<'_>,
) -> Vec<FileRenameEdit> {
    let program_file = db.program_file(file);
    let source = source_text(db, file);
    if !module_name_changes.may_affect_source(&source) {
        return Vec::new();
    }

    let parsed_module = ruff_db::parsed::parsed_module(db, program_file.python_file(db)).load(db);
    let root = AnyNodeRef::from(parsed_module.syntax());
    let model = SemanticModel::new(db, program_file);
    let mut bound_name_change_cache = BoundNameChangeCache::new(module_name_changes);

    let import_statement_changes = {
        let mut import_statement_visitor =
            ImportStatementVisitor::new(db, &model, &mut bound_name_change_cache);
        root.visit_source_order(&mut import_statement_visitor);
        import_statement_visitor.changes
    };

    let mut reaching_definitions = FxHashMap::default();
    let mut module_reference_visitor = ModuleReferenceVisitor::new(
        db,
        &model,
        &mut reaching_definitions,
        &import_statement_changes.bound_name_changes,
        &mut bound_name_change_cache,
    );
    root.visit_source_order(&mut module_reference_visitor);

    let mut edits = import_statement_changes.edits;
    edits.extend(module_reference_visitor.edits);
    edits
}

/// Computes imported name and module name edits, and records bound name changes.
///
/// For example, renaming `pkg.old` to `pkg.new` changes
///
/// ```python
/// import pkg.old
/// ```
///
/// to
///
/// ```python
/// import pkg.new
/// ```
///
/// and
///
/// ```python
/// from pkg.old import C
/// ```
///
/// to
///
/// ```python
/// from pkg.new import C
/// ```
///
/// For the moment, the computed edits never change the form of an import statement. For instance,
/// we never introduce an explicit alias where there wasn't one before, or change the import level in
/// a relative import.
///
/// The visitor uses a [`ModuleMemberNameAnalyzer`] to follow bound name changes through re-exports,
/// sharing a [`BoundNameChangeCache`] across import statements.
struct ImportStatementVisitor<'a, 'db> {
    db: &'db dyn Db,
    editor: ImportStatementEditor<'a, 'db>,
    bound_name_change_cache: &'a mut BoundNameChangeCache<'db>,
    changes: ImportStatementChanges<'db>,
}

impl<'a, 'db> ImportStatementVisitor<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        model: &'a SemanticModel<'db>,
        bound_name_change_cache: &'a mut BoundNameChangeCache<'db>,
    ) -> Self {
        Self {
            db,
            editor: ImportStatementEditor::new(model, bound_name_change_cache.module_name_changes),
            bound_name_change_cache,
            changes: ImportStatementChanges::default(),
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for ImportStatementVisitor<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        let changes = match node {
            AnyNodeRef::StmtImport(statement) => self.editor.edit_plain_import(statement),
            AnyNodeRef::StmtImportFrom(statement) => self.editor.edit_import_from(
                statement,
                &mut ModuleMemberNameAnalyzer::new(self.db, self.bound_name_change_cache),
            ),
            _ => return TraversalSignal::Traverse,
        };

        if let Some(changes) = changes {
            self.changes.extend(changes);
        }

        // The import statement has been handled, so its descendants
        // (e.g., aliases) do not need a separate visit.
        TraversalSignal::Skip
    }

    // Plain import statements and import-from statements cannot occur inside expressions.
    fn visit_expr(&mut self, _expr: &'a ast::Expr) {}
}

/// Computes edits and bound name changes for one import statement.
struct ImportStatementEditor<'a, 'db> {
    model: &'a SemanticModel<'db>,
    module_name_changes: &'a ModuleNameChanges<'db>,
}

impl<'a, 'db> ImportStatementEditor<'a, 'db> {
    fn new(model: &'a SemanticModel<'db>, module_name_changes: &'a ModuleNameChanges<'db>) -> Self {
        Self {
            model,
            module_name_changes,
        }
    }

    /// Computes imported name edits for a plain import statement, such as `import old` to `import new`.
    ///
    /// Returns `None` if no source edits or bound name changes are produced.
    fn edit_plain_import(
        &self,
        statement: &ast::StmtImport,
    ) -> Option<ImportStatementChanges<'db>> {
        let mut changes = ImportStatementChanges::default();
        for alias in &statement.names {
            match self.should_edit_imported_name_for_plain_import(alias) {
                EditDecision::Yes(new_imported_name) => {
                    self.record_imported_name_edit(&mut changes, alias, new_imported_name);
                }
                EditDecision::No => continue,
                EditDecision::Omit => return None,
            }
        }

        (!changes.is_empty()).then_some(changes)
    }

    /// Computes imported name and module name edits for an import-from statement.
    ///
    /// For example, `from pkg import old` becomes `from pkg import new`, and
    /// `from pkg.old import C` becomes `from pkg.new import C`.
    ///
    /// Returns `None` when the module cannot be resolved or the rename produces no source edits
    /// or bound name changes.
    fn edit_import_from(
        &self,
        statement: &ast::StmtImportFrom,
        module_member_name_analyzer: &mut ModuleMemberNameAnalyzer<'_, 'db>,
    ) -> Option<ImportStatementChanges<'db>> {
        let resolved_module = self.model.resolve_module(
            statement.module.as_ref().map(ast::Identifier::as_str),
            statement.level,
        )?;
        let import_from_module_edit = if let Some(new_module_name) = self
            .module_name_changes
            .new_module_name(resolved_module.name(self.model.db()))
        {
            let module = statement.module.as_ref()?;
            Some(self.import_from_module_edit(module, new_module_name))
        } else {
            None
        };

        let mut changes = ImportStatementChanges::default();
        for alias in &statement.names {
            match self.should_edit_imported_name_for_import_from(
                alias,
                resolved_module,
                module_member_name_analyzer,
            ) {
                EditDecision::Yes(new_imported_name) => {
                    self.record_imported_name_edit(&mut changes, alias, new_imported_name);
                }
                EditDecision::No | EditDecision::Omit => continue,
            }
        }

        // An import-from statement may still have a module name edit when no imported names change.
        if let Some(import_from_module_edit) = import_from_module_edit {
            changes.edits.push(import_from_module_edit);
        }

        (!changes.is_empty()).then_some(changes)
    }

    /// Chooses an imported name edit for a plain import statement (e.g., `pkg.old` to `pkg.new`
    /// in the statement `import pkg.old`).
    fn should_edit_imported_name_for_plain_import(&self, alias: &ast::Alias) -> EditDecision {
        let Some(resolved_module) = self.model.resolve_module(Some(alias.name.as_str()), 0) else {
            return EditDecision::No;
        };

        let Some(new_module_name) = self
            .module_name_changes
            .new_module_name(resolved_module.name(self.model.db()))
        else {
            return EditDecision::No;
        };

        EditDecision::replace_if_changed(alias.name.as_str(), new_module_name.as_str())
    }

    /// Chooses an imported name edit for an import-from statement, including re-exports.
    ///
    /// For example, renaming `pkg.old` can change the imported name `old` in both
    /// `from pkg import old` and `from facade import old` when `facade` re-exports `old`.
    fn should_edit_imported_name_for_import_from(
        &self,
        alias: &ast::Alias,
        resolved_module: Module<'db>,
        module_member_name_analyzer: &mut ModuleMemberNameAnalyzer<'_, 'db>,
    ) -> EditDecision {
        if !self
            .module_name_changes
            .is_old_module_basename(alias.name.as_str())
        {
            return EditDecision::No;
        }

        if let Some(imported_module) = self.directly_imported_submodule(alias, resolved_module) {
            let Some(new_module_name) = self
                .module_name_changes
                .new_module_name(imported_module.name(self.model.db()))
            else {
                return EditDecision::No;
            };

            return EditDecision::replace_if_changed(
                alias.name.as_str(),
                new_module_name.last_component(),
            );
        }

        module_member_name_analyzer.should_edit_module_member(
            self.model,
            resolved_module,
            alias.name.as_str(),
        )
    }

    /// Constructs a module name edit, preserving its prefix and the import level.
    ///
    /// The module parent is unchanged, so only the final component needs replacing.
    /// For example, `from ..pkg.old import C` becomes `from ..pkg.new import C`.
    fn import_from_module_edit(
        &self,
        module: &ast::Identifier,
        new_module_name: &ModuleName,
    ) -> FileRenameEdit {
        let replacement = if let Some((prefix, _)) = module.as_str().rsplit_once('.') {
            format!("{prefix}.{}", new_module_name.last_component())
        } else {
            new_module_name.last_component().to_string()
        };
        self.source_edit(module.range, replacement)
    }

    /// Identifies an imported name that resolves to a direct submodule.
    ///
    /// For example, `old` in `from pkg import old` when it resolves to `pkg/old.py`.
    fn directly_imported_submodule(
        &self,
        alias: &ast::Alias,
        resolved_module: Module<'db>,
    ) -> Option<Module<'db>> {
        let db = self.model.db();
        let module_name = resolved_module.name(db);
        let imported_module = resolved_module_from_type(self.model, alias)?;
        let imported_module_name = imported_module.name(db);

        if alias.name.as_str() != imported_module_name.last_component()
            || imported_module_name.parent().as_ref() != Some(module_name)
        {
            return None;
        }

        // Without a package definition for `old`, `from pkg import old` imports `pkg.old`
        // directly (for example, when `pkg/__init__.py` is empty).
        if self
            .model
            .definitions_for_module_global(resolved_module, alias.name.as_str())
            .is_none()
        {
            return Some(imported_module);
        }

        // In `pkg/__init__.py`, `from . import old` imports `pkg.old` directly and defines
        // the package's `old` bound name. Resolving that name as a re-export would analyze
        // the same import again.
        if file_to_module(db, self.model.program_file().resolver_file(db))
            .is_some_and(|importing_module| importing_module.name(db) == module_name)
        {
            return Some(imported_module);
        }

        None
    }

    /// Records an imported name edit and any resulting bound name change.
    ///
    /// For example, renaming `pkg/old.py` to `pkg/new.py` changes the imported name `pkg.old` in
    /// `import pkg.old` to `pkg.new`, but keeps the bound name `pkg`. Conversely, the same rename
    /// changes the imported name `old` in `from pkg import old` to `new` and also changes the bound
    /// name.
    fn record_imported_name_edit(
        &self,
        changes: &mut ImportStatementChanges<'db>,
        alias: &ast::Alias,
        new_imported_name: String,
    ) {
        if let Some(new_bound_name) = Self::bound_name_replacement(alias, &new_imported_name) {
            let definition =
                ty_python_core::semantic_index(self.model.db(), self.model.program_file())
                    .expect_single_definition(alias);
            changes
                .bound_name_changes
                .insert(definition, new_bound_name.to_owned());
        }

        changes
            .edits
            .push(self.source_edit(alias.name.range, new_imported_name));
    }

    /// Constructs a source edit, such as replacing the imported name `old` with `new`.
    fn source_edit(&self, range: TextRange, replacement: String) -> FileRenameEdit {
        RangedValue {
            range: FileRange::new(self.model.file(), range),
            value: replacement,
        }
    }

    /// Returns the new bound name if changing the imported name also changes its binding.
    fn bound_name_replacement<'name>(
        alias: &ast::Alias,
        new_imported_name: &'name str,
    ) -> Option<&'name str> {
        if alias.asname.is_some() {
            return None;
        }

        // `import pkg.old` binds `pkg`, while `from pkg import old` binds `old`.
        // The first component of the imported name gives the bound name in both cases.
        let old_bound_name = Self::first_component(alias.name.as_str());
        let new_bound_name = Self::first_component(new_imported_name);
        (old_bound_name != new_bound_name).then_some(new_bound_name)
    }

    /// Retrieves the first component of an imported name: `pkg` for `pkg.old`, or `old` for `old`.
    fn first_component(alias_name: &str) -> &str {
        alias_name
            .split_once('.')
            .map(|(first, _)| first)
            .unwrap_or(alias_name)
    }
}

type BoundNameChanges<'db> = FxHashMap<Definition<'db>, String>;

#[derive(Default)]
struct ImportStatementChanges<'db> {
    /// Source edits to imported names and module names.
    edits: Vec<FileRenameEdit>,
    /// Maps import definitions to new bound names for reference edits.
    bound_name_changes: BoundNameChanges<'db>,
}

impl ImportStatementChanges<'_> {
    fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.bound_name_changes.is_empty()
    }

    fn extend(&mut self, other: Self) {
        self.edits.extend(other.edits);
        self.bound_name_changes.extend(other.bound_name_changes);
    }
}

/// Caches bound name decisions by import definition while computing edits for one file.
struct BoundNameChangeCache<'db> {
    module_name_changes: &'db ModuleNameChanges<'db>,
    bound_name_decisions_by_import: FxHashMap<Definition<'db>, EditDecision>,
}

impl<'db> BoundNameChangeCache<'db> {
    fn new(module_name_changes: &'db ModuleNameChanges<'db>) -> Self {
        Self {
            module_name_changes,
            bound_name_decisions_by_import: FxHashMap::default(),
        }
    }
}

/// Computes an edit decision for a module member name after a file rename.
struct ModuleMemberNameAnalyzer<'cache, 'db> {
    db: &'db dyn Db,
    cache: &'cache mut BoundNameChangeCache<'db>,
    import_definitions_in_progress: FxHashSet<Definition<'db>>,
    import_cycle_detected: bool,
}

impl<'cache, 'db> ModuleMemberNameAnalyzer<'cache, 'db> {
    fn new(db: &'db dyn Db, cache: &'cache mut BoundNameChangeCache<'db>) -> Self {
        Self {
            db,
            cache,
            import_definitions_in_progress: FxHashSet::default(),
            import_cycle_detected: false,
        }
    }

    /// Decides whether to edit `bound_name` when it is imported from `resolved_module`
    /// (e.g., `old` in `from facade import old`) or accessed as an attribute
    /// (e.g., `old` in `facade.old`).
    ///
    /// This follows import definitions through re-exports. An explicit alias preserves the bound
    /// name even when the imported name changes.
    ///
    /// A `Yes` decision requires every possible definition to produce the same replacement; `No`
    /// requires every definition to preserve the name. Otherwise, the method returns `Omit`.
    /// For example:
    ///
    /// ```python
    /// # facade.py
    /// if flag:
    ///     from pkg import old
    /// else:
    ///     old = 1
    /// ```
    ///
    /// Renaming `pkg.old` to `pkg.new` changes the imported name to `new`, but the assignment
    /// keeps `old`. Neither name works in both branches, so the method returns `Omit` for `old`.
    ///
    /// Missing definitions and disagreements between a stub and its runtime module also lead to
    /// `Omit`. If a cycle is detected, this method returns `Omit` without caching the affected
    /// bound name changes.
    fn should_edit_module_member(
        &mut self,
        model: &SemanticModel<'db>,
        resolved_module: Module<'db>,
        bound_name: &str,
    ) -> EditDecision {
        // A cycle in one name must not affect the next name in the same import statement.
        if self.import_definitions_in_progress.is_empty() {
            self.import_cycle_detected = false;
        }

        let db = self.db;
        let decision = self.should_edit_member_in_module(model, resolved_module, bound_name);
        if decision == EditDecision::Omit {
            return decision;
        }

        // A stub can expose a different bound name;
        // propagate only those changes that both facets agree on.
        if let Some(runtime_module) =
            resolve_real_module_confident(db, resolver_environment(db), resolved_module.name(db))
            && runtime_module.file(db) != resolved_module.file(db)
            && self.should_edit_member_in_module(model, runtime_module, bound_name) != decision
        {
            return EditDecision::Omit;
        }

        decision
    }

    fn should_edit_member_in_module(
        &mut self,
        model: &SemanticModel<'db>,
        module: Module<'db>,
        bound_name: &str,
    ) -> EditDecision {
        let Some(resolution) = model.definitions_for_module_global(module, bound_name) else {
            return EditDecision::Omit;
        };
        let mut builder = EditDecisionBuilder::new(&resolution);

        for &definition in resolution.definitions() {
            if builder.edit_is_omitted() {
                break;
            }
            builder.add(self.should_edit_arbitrary_definition(definition));
        }

        builder.finish()
    }

    fn should_edit_arbitrary_definition(&mut self, definition: Definition<'db>) -> EditDecision {
        let db = self.db;
        // An import in a non-indexed file keeps its bound name.
        if !db.project().files(db).contains(definition.file(db)) {
            return EditDecision::No;
        }

        let parsed_module = ruff_db::parsed::parsed_module(db, definition.python_file(db)).load(db);
        let model = SemanticModel::new(db, definition.program_file(db));
        let module_name_changes = self.cache.module_name_changes;

        match definition.kind(db) {
            DefinitionKind::Import(import) => {
                let alias = import.alias(&parsed_module);
                self.should_edit_import_definition(definition, alias, |_| {
                    ImportStatementEditor::new(&model, module_name_changes)
                        .should_edit_imported_name_for_plain_import(alias)
                })
            }
            DefinitionKind::ImportFrom(import) => {
                let statement = import.import(&parsed_module);
                let alias = import.alias(&parsed_module);
                self.should_edit_import_definition(
                    definition,
                    alias,
                    |module_member_name_analyzer| {
                        let Some(resolved_module) = model.resolve_module(
                            statement.module.as_ref().map(ast::Identifier::as_str),
                            statement.level,
                        ) else {
                            return EditDecision::Omit;
                        };

                        ImportStatementEditor::new(&model, module_name_changes)
                            .should_edit_imported_name_for_import_from(
                                alias,
                                resolved_module,
                                module_member_name_analyzer,
                            )
                    },
                )
            }
            DefinitionKind::StarImport(_) | DefinitionKind::ImportFromSubmodule(_) => {
                EditDecision::Omit
            }
            _ => EditDecision::No,
        }
    }

    /// Determines and caches whether the bound name introduced by an import definition changes.
    ///
    /// Revisiting a definition while its analysis is in progress detects a cycle. Cyclic results
    /// are not cached because their outcome can depend on where the analysis started.
    fn should_edit_import_definition(
        &mut self,
        import_definition: Definition<'db>,
        alias: &ast::Alias,
        should_edit_imported_name: impl FnOnce(&mut Self) -> EditDecision,
    ) -> EditDecision {
        if self.import_cycle_detected {
            return EditDecision::Omit;
        }

        if let Some(decision) = self
            .cache
            .bound_name_decisions_by_import
            .get(&import_definition)
        {
            return decision.clone();
        }

        if !self
            .import_definitions_in_progress
            .insert(import_definition)
        {
            self.import_cycle_detected = true;
            return EditDecision::Omit;
        }

        let imported_name_decision = should_edit_imported_name(self);
        self.import_definitions_in_progress
            .remove(&import_definition);
        if self.import_cycle_detected {
            return EditDecision::Omit;
        }

        let decision = match imported_name_decision {
            EditDecision::Yes(new_imported_name) => {
                ImportStatementEditor::bound_name_replacement(alias, &new_imported_name)
                    .map(|name| EditDecision::Yes(name.to_owned()))
                    .unwrap_or(EditDecision::No)
            }
            // An omitted imported name edit leaves the binding unchanged.
            EditDecision::No | EditDecision::Omit => EditDecision::No,
        };
        self.cache
            .bound_name_decisions_by_import
            .insert(import_definition, decision.clone());
        decision
    }
}

/// Computes reference edits for renamed modules.
///
/// For example, when `from pkg import old` becomes `from pkg import new`,
/// this visitor computes an edit to change a module reference such as
///
/// ```python
/// old.C
/// ```
///
/// to
///
/// ```python
/// new.C
/// ```
///
/// It also rewrites module references through attributes: renaming `pkg.old`
/// to `pkg.new` can change `pkg.old.C` to `pkg.new.C`.
///
/// For module references through names, reaching definitions connect each
/// reference to the bound name changes recorded by [`ImportStatementVisitor`].
/// For module references through attributes, the visitor uses module types and, when the receiver
/// defines the attribute, [`ModuleMemberNameAnalyzer`] to decide whether and how to edit it.
struct ModuleReferenceVisitor<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    reaching_definitions: &'a mut FxHashMap<ScopeId<'db>, ReachingDefinitionsByExpression<'db>>,
    module_name_changes: &'a ModuleNameChanges<'db>,
    bound_name_changes: &'a BoundNameChanges<'db>,
    bound_name_change_cache: &'a mut BoundNameChangeCache<'db>,
    edits: Vec<FileRenameEdit>,
}

impl<'a, 'db> ModuleReferenceVisitor<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        model: &'a SemanticModel<'db>,
        reaching_definitions: &'a mut FxHashMap<ScopeId<'db>, ReachingDefinitionsByExpression<'db>>,
        bound_name_changes: &'a BoundNameChanges<'db>,
        bound_name_change_cache: &'a mut BoundNameChangeCache<'db>,
    ) -> Self {
        Self {
            db,
            model,
            reaching_definitions,
            module_name_changes: bound_name_change_cache.module_name_changes,
            bound_name_changes,
            bound_name_change_cache,
            edits: Vec::new(),
        }
    }

    /// Chooses a reference edit for a name by considering the definitions that connect that name
    /// to a bound name change from an import statement.
    fn edit_name_reference(&mut self, name: &ast::ExprName) {
        let Some(resolution) = self
            .model
            .reaching_definitions(name, Some(self.reaching_definitions))
        else {
            return;
        };

        if resolution.crosses_scope_declaration() {
            // Renaming a name declared `global` or `nonlocal` also requires updating its
            // declaration and writes; this is not yet supported.
            return;
        }

        let mut builder = EditDecisionBuilder::new(&resolution);

        for &definition in resolution.definitions() {
            if builder.edit_is_omitted() {
                break;
            }
            let decision = if matches!(definition.kind(self.db), DefinitionKind::StarImport(_)) {
                // Module references introduced by star imports such as `from pkg import *`
                // are unsupported.
                EditDecision::Omit
            } else if let Some(replacement) = self.bound_name_changes.get(&definition) {
                EditDecision::Yes(replacement.clone())
            } else {
                EditDecision::No
            };
            builder.add(decision);
        }

        self.record_reference_edit(name.range(), builder.finish());
    }

    /// Chooses a reference edit for an attribute such as `pkg.old`.
    ///
    /// Both the attribute and its receiver must resolve to modules.
    /// For attributes defined in the receiver's module, [`ModuleMemberNameAnalyzer`] determines
    /// whether their names change.
    fn edit_attribute_reference(&mut self, attribute: &ast::ExprAttribute) {
        if !attribute.ctx.is_load() {
            // Do not rewrite an attribute name that is assigned to or deleted.
            // The visitor still traverses the receiver, so renaming `pkg.old` to `pkg.new`
            // can change `pkg.old.VALUE = 1` to `pkg.new.VALUE = 1`.
            return;
        }

        let Some(attribute_module) = resolved_module_from_type(self.model, attribute) else {
            return;
        };
        let old_module_name = attribute_module.name(self.db);
        let Some(new_module_name) = self.module_name_changes.new_module_name(old_module_name)
        else {
            return;
        };
        let Some(receiver_module) = resolved_module_from_type(self.model, &*attribute.value) else {
            return;
        };

        let attribute_name = attribute.attr.as_str();

        // Computes a necessary condition for rewriting a direct submodule reference.
        //
        // Consider this example (when we're renaming `pkg/old.py` to `pkg/new.py`):
        //
        // ```python
        // holder.old = pkg.old # A
        // print(holder.old)    # B
        // ```
        //
        // For A, we want to rewrite `pkg.old` to `pkg.new`. There, `pkg.old`
        // (the old module name) relative to `pkg` (the receiver) is `old`
        // (the attribute name), so the attribute does indeed match the
        // old module name and we should rewrite it.
        //
        // For B, we do not want to rewrite `holder.old` to `holder.new`, even though
        // `holder.old` refers to the `pkg.old` module. There, `pkg.old` relative to
        // `holder` is `None`, so the attribute does
        // not match the old module name and we do not rewrite it.
        let attribute_matches_old_module_name = old_module_name
            .relative_to(receiver_module.name(self.db))
            .is_some_and(|relative_name| relative_name.as_str() == attribute_name);

        let decision = if self
            .model
            .definitions_for_module_global(receiver_module, attribute.attr.as_str())
            .is_some()
        {
            ModuleMemberNameAnalyzer::new(self.db, self.bound_name_change_cache)
                .should_edit_module_member(self.model, receiver_module, attribute.attr.as_str())
        } else if attribute_matches_old_module_name {
            EditDecision::replace_if_changed(
                attribute.attr.as_str(),
                new_module_name.last_component(),
            )
        } else {
            EditDecision::No
        };

        self.record_reference_edit(attribute.attr.range, decision);
    }

    /// Computes reference edits in string annotations, such as `value: 'old.C'`.
    ///
    /// Ordinary strings in `__all__ = ["old"]` or `importlib.import_module("pkg.old")`
    /// are left unchanged.
    fn edit_string_annotation_references(&mut self, string: &ast::ExprStringLiteral) {
        let Some((ast, model)) = self.model.enter_string_annotation(string) else {
            return;
        };
        let mut visitor = ModuleReferenceVisitor::new(
            self.db,
            &model,
            self.reaching_definitions,
            self.bound_name_changes,
            self.bound_name_change_cache,
        );
        visitor.visit_expr(ast.expr());
        self.edits.extend(visitor.edits);
    }

    /// Records a module reference edit.
    fn record_reference_edit(&mut self, range: TextRange, decision: EditDecision) {
        let EditDecision::Yes(replacement) = decision else {
            return;
        };
        self.edits.push(RangedValue {
            range: FileRange::new(self.model.file(), range),
            value: replacement,
        });
    }
}

impl<'a> SourceOrderVisitor<'a> for ModuleReferenceVisitor<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        match node {
            AnyNodeRef::ExprName(name)
                if name.ctx.is_load()
                    && self
                        .module_name_changes
                        .is_old_module_basename(name.id.as_str()) =>
            {
                self.edit_name_reference(name);
            }
            AnyNodeRef::ExprAttribute(attribute)
                if self
                    .module_name_changes
                    .is_old_module_basename(attribute.attr.as_str()) =>
            {
                self.edit_attribute_reference(attribute);
            }
            AnyNodeRef::ExprStringLiteral(string) => {
                self.edit_string_annotation_references(string);
                return TraversalSignal::Skip;
            }
            _ => {}
        }
        TraversalSignal::Traverse
    }
}

/// Combines edit decisions for a name's reachable definitions.
///
/// The edit is omitted when the definitions disagree or their resolution is incomplete. For
/// example, if branches execute `from a import x` and `from b import x`, renaming `a.x` to `a.y`
/// and `b.x` to `b.z` rewrites both imported names but leaves the module reference in a subsequent
/// `print(x)` unchanged: neither `y` nor `z` works for both branches. Such partial results can
/// require manual fixes.
///
/// Possible unboundness alone does not prevent edits: a conditional import can still establish
/// the same replacement wherever the name is bound.
struct EditDecisionBuilder {
    decision: Option<EditDecision>,
}

impl EditDecisionBuilder {
    fn new(resolution: &DefinitionResolution<'_>) -> Self {
        Self {
            decision: if resolution.is_complete() {
                None
            } else {
                Some(EditDecision::Omit)
            },
        }
    }

    fn edit_is_omitted(&self) -> bool {
        self.decision == Some(EditDecision::Omit)
    }

    fn add(&mut self, decision: EditDecision) {
        match &self.decision {
            None => self.decision = Some(decision),
            Some(previous) if previous != &decision => self.decision = Some(EditDecision::Omit),
            Some(_) => {}
        }
    }

    fn finish(self) -> EditDecision {
        self.decision.unwrap_or(EditDecision::Omit)
    }
}

#[derive(Clone, Eq, PartialEq)]
enum EditDecision {
    /// Preserve the imported name or module reference.
    No,
    /// Replace the imported name or module reference with this value.
    Yes(String),
    /// We cannot determine whether or how to change the name, so omit its edit.
    Omit,
}

impl EditDecision {
    /// Returns `Yes` if the names differ, or `No` if they match.
    fn replace_if_changed(old: &str, new: &str) -> Self {
        if old != new {
            return Self::Yes(new.to_string());
        }
        Self::No
    }
}

/// Maps a supported file rename to its old and new module names.
fn module_rename<'db>(
    db: &'db dyn Db,
    rename: &FileRename,
) -> Option<(&'db ModuleName, ModuleName)> {
    let resolver_environment = resolver_environment(db);
    let file = rename.file;
    let old_path = file.path(db).as_system_path()?;
    let new_path = SystemPath::absolute(&rename.new_path, db.system().current_directory());
    let extension = old_path.extension()?;

    if !matches!(extension, "py" | "pyi")
        || new_path.extension() != Some(extension)
        || old_path.file_stem() == Some("__init__")
        || new_path.file_stem() == Some("__init__")
    {
        return None;
    }

    let old_module_name =
        file_to_module(db, ResolverFile::new(db, file, resolver_environment))?.name(db);

    // A runtime module and its stub share a module name. Renaming only the stub
    // must not redirect imports while the runtime module remains at its old path.
    if resolve_module_file(db, old_module_name)? != file {
        return None;
    }

    let new_module_name = destination_module_name(db, &new_path)?;
    if old_module_name.parent() != new_module_name.parent() {
        return None;
    }

    Some((old_module_name, new_module_name))
}

/// Derives a destination module name without requiring the destination to exist yet.
fn destination_module_name(db: &dyn Db, path: &SystemPath) -> Option<ModuleName> {
    search_paths(db, resolver_environment(db), ModuleResolveMode::Typing)
        .filter(|search_path| !search_path.is_standard_library())
        .find_map(|search_path| search_path.module_name_for_system_path(path))
}

fn resolve_module_file(db: &dyn Db, module_name: &ModuleName) -> Option<File> {
    let resolver_environment = resolver_environment(db);
    resolve_real_module_confident(db, resolver_environment, module_name)
        .or_else(|| resolve_module_confident(db, resolver_environment, module_name))?
        .file(db)
}

fn resolver_environment(db: &dyn Db) -> ResolverEnvironment<'_> {
    db.project().program(db).resolver_environment(db)
}

fn resolved_module_from_type<'db, T: HasType>(
    model: &SemanticModel<'db>,
    node: &T,
) -> Option<Module<'db>> {
    let Type::ModuleLiteral(literal) = node.inferred_type(model)? else {
        return None;
    };
    Some(literal.module(model.db()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use ruff_db::Db as _;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, LintName, Severity,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem, SystemVirtualPath};
    use ruff_python_ast::PythonVersion;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::Ranged;
    use ty_project::{ProjectMetadata, TestDb};

    #[test]
    fn rewrites_alias_names_and_preserves_asnames_in_a_plain_import() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            (
                "/use.py",
                "
                import pkg.old
                import pkg.old as stable
                print(stable)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:8
          |
        2 | import pkg.old
          |        ------- pkg.new
        3 | import pkg.old as stable
          |        ------- pkg.new
        4 | print(stable)
          |
        ");
    }

    #[test]
    fn rewrites_module_fields_and_alias_names_in_an_import_from() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ..."),
            (
                "/use.py",
                "
                from pkg.old import C
                from pkg import old
                print(C, old)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (3)
         --> use.py:2:6
          |
        2 | from pkg.old import C
          |      ------- pkg.new
        3 | from pkg import old
          |                 --- new
        4 | print(C, old)
          |          --- new
        ");
    }

    #[test]
    fn preserves_import_levels_in_relative_import_from_statements() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ..."),
            (
                "/pkg/use.py",
                "
                from .old import C
                from . import old
                print(C, old.C)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (3)
         --> pkg/use.py:2:7
          |
        2 | from .old import C
          |       --- new
        3 | from . import old
          |               --- new
        4 | print(C, old.C)
          |          --- new
        ");
    }

    #[test]
    fn rewrites_module_references_in_string_annotations() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ..."),
            (
                "/use.py",
                "
                from pkg import old
                value: 'old.C'
                runtime = 'old.C'
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:17
          |
        2 | from pkg import old
          |                 --- new
        3 | value: 'old.C'
          |         --- new
        4 | runtime = 'old.C'
          |
        ");
    }

    #[test]
    fn rewrites_imports_for_multiple_module_renames() {
        let sources = &[
            ("/old.py", ""),
            ("/other.py", ""),
            ("/first.py", "import old"),
            ("/second.py", "import other"),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py"), ("/other.py", "/another.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> first.py:1:8
          |
        1 | import old
          |        --- new
          |
         ::: second.py:1:8
          |
        1 | import other
          |        ----- another
        ");
    }

    #[test]
    fn edits_only_indexed_project_files() {
        let mut db = test_db(&[
            ("/old.py", ""),
            (
                "/included.py",
                "
                from not_a_candidate import old
                import old as independent
                print(old, independent)
                ",
            ),
            ("/not_a_candidate.py", "import old\n"),
        ]);
        let not_indexed = system_path_to_file(&db, "/not_a_candidate.py").unwrap();
        let virtual_path = SystemVirtualPath::new("untitled:consumer.py");
        db.write_virtual_file(virtual_path, "import old");
        let virtual_file = db.files().virtual_file(&db, virtual_path).file();

        db.project()
            .set_included_paths(&mut db, vec!["/old.py".into(), "/included.py".into()]);
        db.project().open_file(&mut db, not_indexed);
        db.project().open_file(&mut db, virtual_file);

        let rename = file_renames(&db, &[("/old.py", "/new.py")]);
        let edits = will_rename_files(&db, &rename);
        assert_snapshot!(render_edits(&db, edits), @"
        info[file-rename]: Rename edits (1)
         --> included.py:3:8
          |
        2 | from not_a_candidate import old
        3 | import old as independent
          |        --- new
        4 | print(old, independent)
          |
        ");
    }

    #[test]
    fn possibly_unbound_module_references_are_rewritten() {
        let sources = &[
            ("/old.py", ""),
            (
                "/use.py",
                "
                def f(flag):
                    if flag:
                        import old
                    return old
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:4:16
          |
        2 | def f(flag):
        3 |     if flag:
        4 |         import old
          |                --- new
        5 |     return old
          |            --- new
        ");
    }

    #[test]
    fn rewrites_references_only_when_import_definitions_agree() {
        let sources = &[
            ("/a/__init__.py", ""),
            ("/a/x.py", ""),
            ("/b/__init__.py", ""),
            ("/b/x.py", ""),
            (
                "/use.py",
                "
                if flag: from a import x
                else: from b import x
                print(x)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/a/x.py", "/a/y.py"), ("/b/x.py", "/b/y.py")], sources), @"
        info[file-rename]: Rename edits (3)
         --> use.py:2:24
          |
        2 | if flag: from a import x
          |                        - y
        3 | else: from b import x
          |                     - y
        4 | print(x)
          |       - y
        ");
        assert_snapshot!(rename_edits(&[("/a/x.py", "/a/y.py"), ("/b/x.py", "/b/z.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:24
          |
        2 | if flag: from a import x
          |                        - y
        3 | else: from b import x
          |                     - z
        4 | print(x)
          |
        ");
    }

    #[test]
    fn propagates_bound_name_changes_transitively_through_explicit_reexports() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/facade.py", "from pkg import old"),
            ("/bridge.py", "from facade import old"),
            ("/stable.py", "from pkg import old as old"),
            (
                "/use.py",
                "
                from bridge import old
                import bridge, stable
                print(old, bridge.old, stable.old)
                ",
            ),
            (
                "/star.py",
                "
                from bridge import *
                old
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (6)
         --> bridge.py:1:20
          |
        1 | from facade import old
          |                    --- new
          |
         ::: facade.py:1:17
          |
        1 | from pkg import old
          |                 --- new
          |
         ::: stable.py:1:17
          |
        1 | from pkg import old as old
          |                 --- new
          |
         ::: use.py:2:20
          |
        2 | from bridge import old
          |                    --- new
        3 | import bridge, stable
        4 | print(old, bridge.old, stable.old)
          |       --- new     --- new
        ");
    }

    #[test]
    fn propagates_multiple_names_from_the_same_reexport_statement() {
        let sources = &[
            ("/old.py", ""),
            ("/other.py", ""),
            ("/facade.py", "import old, other"),
            ("/use.py", "from facade import old, other"),
        ];

        assert_snapshot!(rename_edits(&[
            ("/old.py", "/new.py"),
            ("/other.py", "/another.py"),
        ], sources), @"
        info[file-rename]: Rename edits (4)
         --> facade.py:1:8
          |
        1 | import old, other
          |        ---  ----- another
          |        |
          |        new
          |
         ::: use.py:1:20
          |
        1 | from facade import old, other
          |                    ---  ----- another
          |                    |
          |                    new
        ");
    }

    #[test]
    fn reexport_cycles_do_not_suppress_independent_edits() {
        let sources = &[
            ("/old.py", ""),
            ("/other.py", ""),
            (
                "/a.py",
                "
                import old
                from b import old, other
                from safe import old as independent
                print(old, other, independent)
                ",
            ),
            (
                "/b.py",
                "
                from a import old
                import other
                ",
            ),
            ("/safe.py", "import old"),
            ("/use.py", "from a import old, other"),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py"), ("/other.py", "/another.py")], sources), @"
        info[file-rename]: Rename edits (7)
         --> a.py:2:8
          |
        2 | import old
          |        --- new
        3 | from b import old, other
          |                    ----- another
        4 | from safe import old as independent
          |                  --- new
        5 | print(old, other, independent)
          |            ----- another
          |
         ::: b.py:3:8
          |
        2 | from a import old
        3 | import other
          |        ----- another
          |
         ::: safe.py:1:8
          |
        1 | import old
          |        --- new
          |
         ::: use.py:1:20
          |
        1 | from a import old, other
          |                    ----- another
        ");
    }

    #[test]
    fn ambiguous_imported_names_do_not_suppress_independent_edits() {
        let sources = &[
            ("/old.py", ""),
            ("/other.py", ""),
            (
                "/facade.py",
                "
                import other
                if flag: import old
                else: import other as old
                ",
            ),
            (
                "/use.py",
                "
                import facade
                from bridge import other as indirect
                from facade import other, old
                print(other, old, indirect)
                ",
            ),
            ("/bridge.py", "from facade import old, other"),
        ];

        assert_snapshot!(rename_edits(&[
            ("/old.py", "/new.py"),
            ("/other.py", "/another.py"),
            ("/facade.py", "/renamed.py"),
        ], sources), @"
        info[file-rename]: Rename edits (10)
         --> bridge.py:1:6
          |
        1 | from facade import old, other
          |      ------ renamed     ----- another
          |
         ::: facade.py:2:8
          |
        2 | import other
          |        ----- another
        3 | if flag: import old
          |                 --- new
        4 | else: import other as old
          |              ----- another
          |
         ::: use.py:2:8
          |
        2 | import facade
          |        ------ renamed
        3 | from bridge import other as indirect
          |                    ----- another
        4 | from facade import other, old
          |      ------        ----- another
          |      |
          |      renamed
        5 | print(other, old, indirect)
          |       ----- another
        ");
    }

    #[test]
    fn preserves_assigned_attribute_names() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/holder.py", ""),
            (
                "/use.py",
                "
                import pkg.old
                import holder
                pkg.new = 42

                # This reads pkg.old to find the object whose VALUE attribute is assigned.
                pkg.old.VALUE = 1

                # This assigns the old attribute; only the right side reads pkg.old.
                pkg.old = pkg.old

                holder.old = pkg.old
                print(holder.old)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (4)
          --> use.py:2:8
           |
         2 | import pkg.old
           |        ------- pkg.new
         3 | import holder
         4 | pkg.new = 42
         5 |
         6 | # This reads pkg.old to find the object whose VALUE attribute is assigned.
         7 | pkg.old.VALUE = 1
           |     --- new
         8 |
         9 | # This assigns the old attribute; only the right side reads pkg.old.
        10 | pkg.old = pkg.old
           |               --- new
        11 |
        12 | holder.old = pkg.old
           |                  --- new
        13 | print(holder.old)
           |
        ");
    }

    #[test]
    fn reassignments_prevent_dependent_reference_rewrites() {
        let sources = &[
            ("/old.py", ""),
            (
                "/use.py",
                "
                import old
                print(old)
                if flag:
                    old = 0
                print(old)
                del old
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:8
          |
        2 | import old
          |        --- new
        3 | print(old)
          |       --- new
        4 | if flag:
        5 |     old = 0
        6 | print(old)
        7 | del old
          |
        ");
    }

    #[test]
    fn ignores_deletion_targets() {
        let sources = &[
            ("/old.py", ""),
            ("/list.py", ""),
            (
                "/use.py",
                "
                import old
                import list
                if flag:
                    del old
                    del list
                print(old)
                del list
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py"), ("/list.py", "/renamed.py")], sources), @"
        info[file-rename]: Rename edits (3)
         --> use.py:2:8
          |
        2 | import old
          |        --- new
        3 | import list
          |        ---- renamed
        4 | if flag:
        5 |     del old
        6 |     del list
        7 | print(old)
          |       --- new
        8 | del list
          |
        ");
    }

    #[test]
    fn ignores_references_crossing_scope_declarations() {
        let sources = &[
            ("/old.py", ""),
            (
                "/use.py",
                "
                import old

                def affected():
                    global old
                    print(old)

                def sibling():
                    return old

                def outer():
                    import old

                    def inner():
                        nonlocal old
                        import old

                        class Eager:
                            value = old

                        print(old)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (4)
          --> use.py:2:8
           |
         2 | import old
           |        --- new
         3 |
         4 | def affected():
         5 |     global old
         6 |     print(old)
         7 |
         8 | def sibling():
         9 |     return old
           |            --- new
        10 |
        11 | def outer():
        12 |     import old
           |            --- new
        13 |
        14 |     def inner():
        15 |         nonlocal old
        16 |         import old
           |                --- new
        17 |
        18 |         class Eager:
        19 |             value = old
        20 |
        21 |         print(old)
           |
        ");
    }

    #[test]
    fn runtime_file_rename_controls_edits_when_a_stub_exists() {
        let sources = &[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/pkg/old.pyi", ""),
            (
                "/use.py",
                "
                from pkg import old
                print(old)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.pyi", "/pkg/new.pyi")], sources), @"No edits");
        let runtime_edits = rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources);
        assert_snapshot!(runtime_edits, @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:17
          |
        2 | from pkg import old
          |                 --- new
        3 | print(old)
          |       --- new
        ");
        assert_eq!(
            rename_edits(
                &[
                    ("/pkg/old.py", "/pkg/new.py"),
                    ("/pkg/old.pyi", "/pkg/new.pyi"),
                ],
                sources,
            ),
            runtime_edits,
        );
    }

    #[test]
    fn conflicting_runtime_and_stub_bound_name_changes_prevent_reference_rewrites() {
        let sources = &[
            ("/pkg/__init__.py", "from . import old as old\n"),
            ("/pkg/__init__.pyi", "from . import old\n"),
            ("/pkg/old.py", ""),
            (
                "/use.py",
                "
                import pkg
                print(pkg.old)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/pkg/old.py", "/pkg/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> pkg/__init__.py:1:15
          |
        1 | from . import old as old
          |               --- new
          |
         ::: pkg/__init__.pyi:1:15
          |
        1 | from . import old
          |               --- new
        ");
    }

    #[test]
    #[expect(
        clippy::unicode_not_nfc,
        reason = "The snapshot preserves the non-normalized identifier spelling."
    )]
    fn identifier_prefilter_preserves_unicode_identifier_matches() {
        let sources = &[
            ("/K·b.py", ""),
            (
                "/use.py",
                "
                import \u{212a}·b
                print(\u{212a}·b)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/K·b.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:8
          |
        2 | import K·b
          |        --- new
        3 | print(K·b)
          |       --- new
        ");
    }

    #[test]
    fn omits_renames_that_change_the_module_parent() {
        let sources = &[
            ("/a/__init__.py", ""),
            ("/a/old.py", ""),
            ("/use.py", "from a import old"),
        ];

        assert_no_edits(&[("/a/old.py", "/b/new.py")], sources);
    }

    #[test]
    fn omits_renames_of_package_initializers() {
        let sources = &[("/old/__init__.py", ""), ("/use.py", "import old")];

        assert_no_edits(&[("/old/__init__.py", "/new.py")], sources);
    }

    #[test]
    fn preserves_unresolved_alias_names_in_a_plain_import() {
        let sources = &[
            ("/a/__init__.py", ""),
            ("/a/old.py", ""),
            ("/use.py", "import a.old.missing"),
        ];

        assert_no_edits(&[("/a/old.py", "/a/new.py")], sources);
    }

    #[test]
    fn unsupported_file_renames_do_not_suppress_independent_edits() {
        let sources = &[
            ("/unsupported.py", ""),
            ("/old.py", ""),
            ("/use.py", "import old, unsupported"),
        ];

        assert_snapshot!(rename_edits(&[
                ("/unsupported.py", "/renamed.pyi"),
                ("/old.py", "/new.py"),
            ], sources), @"
        info[file-rename]: Rename edits (1)
         --> use.py:1:8
          |
        1 | import old, unsupported
          |        --- new
        ");
    }

    /// Computes rename edits and shows their ranges and replacements in the original sources.
    fn rename_edits(renames: &[(&str, &str)], sources: &[(&str, &str)]) -> String {
        let db = test_db(sources);
        let renames = file_renames(&db, renames);
        let edits = will_rename_files(&db, &renames);
        render_edits(&db, edits)
    }

    fn render_edits(db: &TestDb, mut edits: Vec<FileRenameEdit>) -> String {
        if edits.is_empty() {
            return "No edits".to_string();
        }
        edits.sort_by(|left, right| {
            left.range
                .file()
                .path(db)
                .as_ref()
                .cmp(right.range.file().path(db).as_ref())
                .then_with(|| left.range.start().cmp(&right.range.start()))
                .then_with(|| left.range.end().cmp(&right.range.end()))
                .then_with(|| left.value.cmp(&right.value))
        });
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::Lint(LintName::of("file-rename")),
            Severity::Info,
            format!("Rename edits ({})", edits.len()),
        );
        let mut context = 0;
        for edit in edits {
            // Keep unchanged lines visible so omissions can be reviewed alongside replacements.
            context = context.max(source_text(db, edit.range.file()).as_str().lines().count());
            diagnostic.annotate(Annotation::secondary(edit.range.into()).message(edit.value));
        }
        diagnostic
            .display(db, &DisplayDiagnosticConfig::new("ty").context(context))
            .to_string()
            .replace('\\', "/")
    }

    fn file_renames(db: &TestDb, paths: &[(&str, &str)]) -> Vec<FileRename> {
        paths
            .iter()
            .map(|&(old_path, new_path)| FileRename {
                file: system_path_to_file(db, old_path).unwrap(),
                new_path: new_path.into(),
            })
            .collect()
    }

    /// Asserts that the rename batch produces no source edits.
    #[track_caller]
    fn assert_no_edits(renames: &[(&str, &str)], sources: &[(&str, &str)]) {
        assert_eq!(rename_edits(renames, sources), "No edits");
    }

    fn test_db(files: &[(&str, &str)]) -> TestDb {
        let mut db = TestDb::new(ProjectMetadata::new("test", "/".into()));
        db.set_python_version(PythonVersion::latest_ty());
        db.write_files(files.iter().map(|(path, source)| (path, dedent(source))))
            .unwrap();
        db
    }
}
