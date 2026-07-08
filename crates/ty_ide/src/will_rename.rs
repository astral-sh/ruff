//! Computes source edits for Python file renames.\
//!
//! ## Overview
//!
//! [`will_rename_files`] maps filesystem renames to module names, then plans alias name, module
//! field, and reference rewrites in the candidate files supplied by the caller (for definitions of
//! those terms, please see the "Terminology" section below). It does not move files, discover
//! package contents, validate the filesystem operation, or normalize the returned edits.
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
//! in turn computes edits for a single file in two distinct AST passes: [`ImportEditPlanner`]
//! computes alias name and module field rewrites, then [`ReferenceEditPlanner`] computes reference
//! rewrites. Those two sets of edits are combined to produce the final result.
//!
//! ## Terminology
//!
//! Below is the anatomy of the two types of import statement.
//!
//! A "plain" import statement:
//!
//! ```text
//!              alias
//!        +---------------+
//! import pkg.old as stable
//!        |          |
//!        |          alias asname
//!        alias name
//! ```
//!
//! and an "import from" statement:
//!
//! ```text
//!                       alias
//!                   +-----------+
//! from ..pkg import old as stable
//!      | |            |      |
//!      | module field |      alias asname
//!      |              alias name
//!      |
//!      import level
//! ```
//!
//! From there, we can layer on a few additional terms.
//!
//! | Term | Meaning | Example |
//! | --- | --- | --- |
//! | Bound name | Name introduced by an alias. | `stable` in both diagrams. |
//! | Module reference | Use of a module in an expression. | `old` in `old.C`; `pkg.old` in `pkg.old.C`. |
//! | Re-export | Imported bound name exposed to another module. | `facade.py`: `import old`; elsewhere: `from facade import old`. |
//! | Alias name rewrite | Planned replacement of an alias name. | `import old` → `import new`. |
//! | Module field rewrite | Planned replacement of a module field. | `from old import C` → `from new import C`. |
//! | Bound name change | Change to a bound name caused by an alias name rewrite. | `old` → `new`; `as stable` keeps `stable`. |
//! | Reference rewrite | Planned replacement at a module reference. | `old.C` → `new.C`. |
//! | Source edit | Source range and replacement text implementing a rewrite. | Range of `old`, replacement `new`. |
//!
//! The above terms are used consistently throughout this module.

use std::collections::hash_map::Entry;

use crate::RangedValue;
use rayon::prelude::*;
use ruff_db::files::{File, FileRange};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{
    Module, ModuleName, ModuleResolveMode, ResolverEnvironment, ResolverFile, file_to_module,
    resolve_module_confident, resolve_real_module_confident, search_paths,
};
use ty_project::{Db, parallel::ParallelIteratorExt};
use ty_python_core::ProgramFile;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::place::PlaceExpr;
use ty_python_semantic::types::Type;
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
/// The `files` argument must include every source the caller wants analyzed and the `db` argument
/// must have place load recording enabled.
///
/// The returned edits refer to the original files and source ranges. Unsupported or ambiguous
/// occurrences are omitted, so a non-empty result does not imply that every reference was updated.
/// The caller must sort the edits and handle duplicates and overlaps before applying them.
pub fn will_rename_files(
    db: &dyn Db,
    renames: &[FileRename],
    files: impl IntoIterator<Item = File>,
) -> Vec<FileRenameEdit> {
    let mut files: Vec<_> = files.into_iter().collect();
    let module_name_changes = ModuleNameChanges::new(db, renames, &files);
    if module_name_changes.module_renames.is_empty() {
        return Vec::new();
    }

    files.sort_unstable_by_key(|file| file.path(db).as_ref());
    files.dedup();

    files
        .into_par_iter()
        .map_with_db(db, |db, file| {
            edits_for_file(db, file, &module_name_changes)
        })
        .flatten()
        .collect()
}

/// One Python file rename in a batch.
pub struct FileRename {
    /// The source file before the rename.
    pub file: File,
    /// The destination path, which need not exist yet.
    pub new_path: SystemPathBuf,
}

/// A replacement and the file range containing it.
pub type FileRenameEdit = RangedValue<String>;

/// Maps old module names to new module names for supported file renames.
struct ModuleNameChanges {
    // A map from old module name to new module name for supported rename operations.
    module_renames: FxHashMap<ModuleName, ModuleName>,
    // The module basename for each key in `module_renames`. For example, if `acme.tools`
    // appears as a key in `module_renames`, then this set will contain `tools`.
    // This is used to filter out irrelevant files and identifiers to avoid unnecessary semantic analysis.
    old_module_basenames: FxHashSet<String>,
    // Files whose imports can contribute bound name changes.
    candidate_files: FxHashSet<File>,
}

impl ModuleNameChanges {
    fn new(db: &dyn Db, renames: &[FileRename], files: &[File]) -> Self {
        let module_renames: FxHashMap<_, _> = renames
            .iter()
            .filter_map(|rename| module_rename(db, rename))
            .filter(|(old_module_name, new_module_name)| old_module_name != new_module_name)
            .collect();
        let old_module_basenames = module_renames
            .keys()
            .map(|module_name| module_name.last_component().to_owned())
            .collect();
        Self {
            module_renames,
            old_module_basenames,
            candidate_files: files.iter().copied().collect(),
        }
    }

    fn new_module_name(&self, old_module_name: &ModuleName) -> Option<&ModuleName> {
        self.module_renames.get(old_module_name)
    }

    fn is_old_module_basename(&self, name: &str) -> bool {
        self.old_module_basenames.contains(name)
    }

    /// Returns whether the source may need edits for these module name changes.
    ///
    /// Source text that doesn't contain any of the old module basenames can
    /// safely be skipped.
    ///
    /// We only apply this check when both the source and the basenames are
    /// ASCII, because non-ASCII identifiers can normalize to a different
    /// spelling in the AST.
    fn may_affect_source(&self, source: &str) -> bool {
        if !source.is_ascii()
            || self
                .old_module_basenames
                .iter()
                .any(|basename| !basename.is_ascii())
        {
            return true;
        }

        self.old_module_basenames
            .iter()
            .any(|basename| source.contains(basename))
    }
}

/// Plans edits for one candidate file in two AST passes.
///
/// 1. [`ImportEditPlanner`] plans alias name and module field rewrites and records bound name changes.
/// 2. [`ReferenceEditPlanner`] plans reference rewrites, including those in string annotations.
///    Reaching definitions determine which module references need updating after a bound name change.
///
/// Planning alias name rewrites first lets us base reference rewrites on the bound name changes,
/// accounting for alias asnames and re-exports. A rejected alias name rewrite leaves its bound name
/// unchanged.
fn edits_for_file(
    db: &dyn Db,
    file: File,
    module_name_changes: &ModuleNameChanges,
) -> Vec<FileRenameEdit> {
    let program_file = db.program_file(file);
    let source = source_text(db, file);
    if source.read_error().is_some() {
        return Vec::new();
    }

    if !module_name_changes.may_affect_source(source.as_str()) {
        return Vec::new();
    }

    let parsed_module = ruff_db::parsed::parsed_module(db, program_file.python_file(db)).load(db);
    let root = AnyNodeRef::from(parsed_module.syntax());
    let model = SemanticModel::new(db, program_file);

    let mut imports = ImportEditPlanner::new(db, &model, module_name_changes);
    root.visit_source_order(&mut imports);

    let mut references = ReferenceEditPlanner::new(
        db,
        &model,
        module_name_changes,
        &imports.output.bound_name_changes,
    );
    root.visit_source_order(&mut references);

    let mut edits = imports.output.edits;
    edits.extend(references.edits);
    edits
}

/// Computes alias name and module field rewrites, and records bound name changes.
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
/// we never introduce an alias asname where there wasn't one before, or change the import level in
/// a relative import.
///
/// The planner uses [`ReexportAnalyzer`] to follow bound name changes through re-exports.
struct ImportEditPlanner<'a, 'db> {
    analyzer: ImportAnalyzer<'a, 'db>,
    output: ImportAnalysis<'db>,
}

impl<'a, 'db> ImportEditPlanner<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        model: &'a SemanticModel<'db>,
        module_name_changes: &'a ModuleNameChanges,
    ) -> Self {
        Self {
            analyzer: ImportAnalyzer::new(db, model, module_name_changes),
            output: ImportAnalysis::default(),
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for ImportEditPlanner<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        let output = match node {
            AnyNodeRef::StmtImport(statement) => self.analyzer.analyze_plain_import(statement),
            AnyNodeRef::StmtImportFrom(statement) => self
                .analyzer
                .analyze_import_from(statement, &mut ReexportAnalyzer::default()),
            _ => return TraversalSignal::Traverse,
        };

        if let Some(output) = output {
            self.output.extend(output);
        }

        TraversalSignal::Skip
    }

    // Plain import statements and import from statements cannot occur inside expressions.
    fn visit_expr(&mut self, _expr: &'a ast::Expr) {}
}

/// Analyzes one plain import statement or import from statement for [`ImportEditPlanner`].
struct ImportAnalyzer<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    module_name_changes: &'a ModuleNameChanges,
}

impl<'a, 'db> ImportAnalyzer<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        model: &'a SemanticModel<'db>,
        module_name_changes: &'a ModuleNameChanges,
    ) -> Self {
        Self {
            db,
            model,
            module_name_changes,
        }
    }

    /// Plans alias name rewrites for a plain import statement, such as `import old` to `import new`.
    ///
    /// Returns `None` if no source edits or bound name changes are produced.
    fn analyze_plain_import(&self, statement: &ast::StmtImport) -> Option<ImportAnalysis<'db>> {
        let output = self.alias_name_rewrites(&statement.names, |alias| {
            self.rewrite_plain_import_alias_name(alias)
        })?;

        (!output.is_empty()).then_some(output)
    }

    /// Plans alias name and module field rewrites for an import from statement.
    ///
    /// For example, `from pkg import old` becomes `from pkg import new`, and
    /// `from pkg.old import C` becomes `from pkg.new import C`.
    ///
    /// Returns `None` when we decline to edit the import statement, or the rename produces no
    /// source edits or bound name changes.
    fn analyze_import_from(
        &self,
        statement: &ast::StmtImportFrom,
        reexports: &mut ReexportAnalyzer<'db>,
    ) -> Option<ImportAnalysis<'db>> {
        let resolved_module = self.model.resolve_module(
            statement.module.as_ref().map(ast::Identifier::as_str),
            statement.level,
        )?;
        let module_field_rewrite = if let Some(new_module_name) = self
            .module_name_changes
            .new_module_name(resolved_module.name(self.db))
        {
            let module = statement.module.as_ref()?;
            Some(self.module_field_rewrite(module, new_module_name))
        } else {
            None
        };

        let Some(mut output) = self.alias_name_rewrites(&statement.names, |alias| {
            self.rewrite_import_from_alias_name(alias, resolved_module, reexports)
        }) else {
            // Rejecting an alias name rewrite also discards any module field rewrite.
            return None;
        };

        if let Some(module_field_rewrite) = module_field_rewrite {
            output.edits.push(module_field_rewrite);
        }

        (!output.is_empty()).then_some(output)
    }

    /// Constructs alias name rewrites and records their bound name changes.
    ///
    /// For example, renaming `pkg/old.py` to `pkg/new.py` changes the alias name `pkg.old` in
    /// `import pkg.old` to `pkg.new`, but keeps the bound name `pkg`. Conversely, the same rename
    /// changes the alias name `old` in `from pkg import old` to `new` and also changes the bound
    /// name.
    ///
    /// Returns `None` when we decline to rewrite the import statement because we can't rewrite one
    /// of the alias names (e.g., `from facade import old` when `facade` conditionally re-exports
    /// `pkg.old` or `other.old`, but only `pkg.old` is renamed).
    fn alias_name_rewrites(
        &self,
        aliases: &[ast::Alias],
        mut rewrite_alias_name: impl FnMut(&ast::Alias) -> RewriteDecision,
    ) -> Option<ImportAnalysis<'db>> {
        let mut output = ImportAnalysis::default();

        for alias in aliases {
            let new_alias_name = match rewrite_alias_name(alias) {
                RewriteDecision::Preserve => continue,
                RewriteDecision::Replace(new_alias_name) => new_alias_name,
                RewriteDecision::Omit => return None,
            };

            if alias.asname.is_none() {
                // Without an alias asname, a plain import statement such as `import pkg.old`
                // binds `pkg`, while an import from statement such as `from pkg import old`
                // binds `old`. Therefore, we can compute the old bound name in both cases
                // by retrieving just the first component.
                let old_bound_name = Self::first_component(alias.name.as_str());

                // The replacement alias name obeys the same rule: `pkg.new` binds `pkg`,
                // while `new` binds `new`. Retrieve its first component for the new bound name.
                let new_bound_name = Self::first_component(&new_alias_name);

                if old_bound_name != new_bound_name {
                    let definition =
                        ty_python_core::semantic_index(self.db, self.model.program_file())
                            .expect_single_definition(alias);
                    output
                        .bound_name_changes
                        .insert(definition, new_bound_name.to_owned());
                }
            }

            output
                .edits
                .push(self.source_edit(alias.name.range, new_alias_name));
        }

        // An empty alias analysis is valid: an import from statement may still have a
        // module field rewrite. The caller checks the combined analysis for emptiness.
        Some(output)
    }

    /// Chooses an alias name rewrite for a plain import statement (e.g., `pkg.old` to `pkg.new`
    /// in the statement `import pkg.old`).
    fn rewrite_plain_import_alias_name(&self, alias: &ast::Alias) -> RewriteDecision {
        let Some(resolved_module) = self.model.resolve_module(Some(alias.name.as_str()), 0) else {
            return RewriteDecision::Preserve;
        };

        let Some(new_module_name) = self
            .module_name_changes
            .new_module_name(resolved_module.name(self.db))
        else {
            return RewriteDecision::Preserve;
        };

        RewriteDecision::replace_if_changed(alias.name.as_str(), new_module_name.as_str())
    }

    /// Chooses an alias name rewrite for an import from statement, including re-exports.
    ///
    /// For example, renaming `pkg.old` can change the alias name `old` in both
    /// `from pkg import old` and `from facade import old` when `facade` re-exports `old`.
    fn rewrite_import_from_alias_name(
        &self,
        alias: &ast::Alias,
        resolved_module: Module<'db>,
        reexports: &mut ReexportAnalyzer<'db>,
    ) -> RewriteDecision {
        if !self
            .module_name_changes
            .is_old_module_basename(alias.name.as_str())
        {
            return RewriteDecision::Preserve;
        }

        if let Some(imported_module) = self.directly_imported_submodule(alias, resolved_module) {
            let Some(new_module_name) = self
                .module_name_changes
                .new_module_name(imported_module.name(self.db))
            else {
                return RewriteDecision::Preserve;
            };

            return RewriteDecision::replace_if_changed(
                alias.name.as_str(),
                new_module_name.last_component(),
            );
        }

        reexports.rewrite_reexported_bound_name(
            self.db,
            self.model,
            self.module_name_changes,
            resolved_module,
            alias.name.as_str(),
        )
    }

    /// Constructs a module field rewrite, preserving its prefix and the import level.
    ///
    /// The module parent is unchanged, so only the final component needs replacing.
    /// For example, `from ..pkg.old import C` becomes `from ..pkg.new import C`.
    fn module_field_rewrite(
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

    /// Identifies an alias name that resolves to a direct submodule, as in `from pkg import old`.
    fn directly_imported_submodule(
        &self,
        alias: &ast::Alias,
        resolved_module: Module<'db>,
    ) -> Option<Module<'db>> {
        let module_name = resolved_module.name(self.db);
        let imported_module = resolved_module_from_type(self.model, alias)?;
        let imported_module_name = imported_module.name(self.db);

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
        if file_to_module(self.db, self.model.program_file().resolver_file(self.db))
            .is_some_and(|importing_module| importing_module.name(self.db) == module_name)
        {
            return Some(imported_module);
        }

        None
    }

    /// Constructs a source edit, such as replacing the alias name `old` with `new`.
    fn source_edit(&self, range: TextRange, replacement: String) -> FileRenameEdit {
        RangedValue {
            range: FileRange::new(self.model.file(), range),
            value: replacement,
        }
    }

    /// Retrieves the first component of an alias name: `pkg` for `pkg.old`, or `old` for `old`.
    fn first_component(alias_name: &str) -> &str {
        alias_name
            .split_once('.')
            .map(|(first, _)| first)
            .unwrap_or(alias_name)
    }
}

type BoundNameChanges<'db> = FxHashMap<Definition<'db>, String>;

#[derive(Default)]
struct ImportAnalysis<'db> {
    /// Source edits for alias name and module field rewrites.
    edits: Vec<FileRenameEdit>,
    /// Maps import definitions to new bound names for reference rewrites.
    bound_name_changes: BoundNameChanges<'db>,
}

impl ImportAnalysis<'_> {
    fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.bound_name_changes.is_empty()
    }

    fn extend(&mut self, other: Self) {
        self.edits.extend(other.edits);
        self.bound_name_changes.extend(other.bound_name_changes);
    }
}

/// Caches bound name changes while resolving re-exports, rejecting cycles.
///
/// For example, if `facade.py` contains `from pkg import old`, renaming `pkg.old` to `pkg.new`
/// also changes `from facade import old` to `from facade import new`.
#[derive(Default)]
struct ReexportAnalyzer<'db> {
    analysis_by_statement: FxHashMap<(ProgramFile<'db>, TextRange), ReexportAnalysisState<'db>>,
    has_cycle: bool,
}

impl<'db> ReexportAnalyzer<'db> {
    fn rewrite_reexported_bound_name(
        &mut self,
        db: &'db dyn Db,
        model: &SemanticModel<'db>,
        module_name_changes: &ModuleNameChanges,
        resolved_module: Module<'db>,
        bound_name: &str,
    ) -> RewriteDecision {
        let mut analyze = |resolved_module| {
            model
                .definitions_for_module_global(resolved_module, bound_name)
                .map(|resolution| {
                    rewrite_for_definition_resolution(&resolution, |definition| {
                        self.rewrite_for_definition(db, module_name_changes, definition)
                    })
                })
                .unwrap_or(RewriteDecision::Omit)
        };

        let decision = analyze(resolved_module);
        if decision == RewriteDecision::Omit {
            return decision;
        }

        // A stub can expose a different bound name;
        // propagate only those changes that both facets agree on.
        if let Some(runtime_module) =
            resolve_real_module_confident(db, resolver_environment(db), resolved_module.name(db))
            && runtime_module.file(db) != resolved_module.file(db)
            && analyze(runtime_module) != decision
        {
            return RewriteDecision::Omit;
        }

        decision
    }

    fn rewrite_for_definition(
        &mut self,
        db: &'db dyn Db,
        module_name_changes: &ModuleNameChanges,
        definition: Definition<'db>,
    ) -> RewriteDecision {
        // An import outside the candidate files keeps its bound name.
        if !module_name_changes
            .candidate_files
            .contains(&definition.file(db))
        {
            return RewriteDecision::Preserve;
        }

        let parsed_module = ruff_db::parsed::parsed_module(db, definition.python_file(db)).load(db);
        let model = SemanticModel::new(db, definition.program_file(db));

        match definition.kind(db) {
            DefinitionKind::Import(import) => {
                let statement = import.import(&parsed_module);
                self.rewrite_for_import_definition(db, definition, statement.range(), |_| {
                    ImportAnalyzer::new(db, &model, module_name_changes)
                        .analyze_plain_import(statement)
                })
            }
            DefinitionKind::ImportFrom(import) => {
                let statement = import.import(&parsed_module);
                self.rewrite_for_import_definition(db, definition, statement.range(), |reexports| {
                    ImportAnalyzer::new(db, &model, module_name_changes)
                        .analyze_import_from(statement, reexports)
                })
            }
            DefinitionKind::StarImport(_) | DefinitionKind::ImportFromSubmodule(_) => {
                RewriteDecision::Omit
            }
            _ => RewriteDecision::Preserve,
        }
    }

    /// Determines whether the bound name introduced by an import definition
    /// should change.
    ///
    /// This method caches the analysis of an entire import statement.
    /// For example, consider this import from statement (assuming it re-exports
    /// `old` and `other`):
    ///
    /// ```python
    /// from pkg import old, other
    /// ```
    ///
    /// Renaming `pkg.old` to `pkg.new` changes only the bound name `old`, hence
    /// analyzing the import definition will return `Replace("new")` and cache
    /// all of the bound name changes for entire import statement. A later lookup
    /// of the import definition for `other` will reuse that cached analysis and
    /// return `Preserve`.
    ///
    /// Revisiting an import statement while its analysis is in progress detects
    /// a cycle and returns `Omit`.
    fn rewrite_for_import_definition(
        &mut self,
        db: &'db dyn Db,
        import_definition: Definition<'db>,
        statement_range: TextRange,
        analyze_statement: impl FnOnce(&mut Self) -> Option<ImportAnalysis<'db>>,
    ) -> RewriteDecision {
        if self.has_cycle {
            return RewriteDecision::Omit;
        }

        let statement_key = (import_definition.program_file(db), statement_range);
        let cached_analysis = match self.analysis_by_statement.entry(statement_key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                entry.insert(ReexportAnalysisState::InProgress);

                let analysis = analyze_statement(self);

                if self.has_cycle {
                    return RewriteDecision::Omit;
                }

                self.analysis_by_statement
                    .entry(statement_key)
                    .insert_entry(ReexportAnalysisState::Complete(
                        analysis
                            .map(|analysis| analysis.bound_name_changes)
                            .unwrap_or_default(),
                    ))
                    .into_mut()
            }
        };

        match cached_analysis {
            ReexportAnalysisState::Complete(bound_name_changes) => bound_name_changes
                .get(&import_definition)
                .map(|new_bound_name| RewriteDecision::Replace(new_bound_name.clone()))
                .unwrap_or(RewriteDecision::Preserve),
            ReexportAnalysisState::InProgress => {
                self.has_cycle = true;
                RewriteDecision::Omit
            }
        }
    }
}

/// Tracks analysis of a statement's bound name changes for reuse and cycle detection.
enum ReexportAnalysisState<'db> {
    /// Analysis is in progress; revisiting this statement indicates a cycle.
    InProgress,
    /// Analysis is complete; definitions absent from the map keep their bound names.
    Complete(BoundNameChanges<'db>),
}

/// Plans reference rewrites for renamed modules.
///
/// For example, when `from pkg import old` becomes `from pkg import new`,
/// this planner changes a module reference such as
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
/// reference to the bound name changes recorded by [`ImportEditPlanner`].
/// For module references through attributes, the planner uses module types
/// and [`ReexportAnalyzer`] to determine the replacement.
struct ReferenceEditPlanner<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    module_name_changes: &'a ModuleNameChanges,
    bound_name_changes: &'a BoundNameChanges<'db>,
    edits: Vec<FileRenameEdit>,
}

impl<'a, 'db> ReferenceEditPlanner<'a, 'db> {
    fn new(
        db: &'db dyn Db,
        model: &'a SemanticModel<'db>,
        module_name_changes: &'a ModuleNameChanges,
        bound_name_changes: &'a BoundNameChanges<'db>,
    ) -> Self {
        Self {
            db,
            model,
            module_name_changes,
            bound_name_changes,
            edits: Vec::new(),
        }
    }

    /// Chooses a reference rewrite for a name by considering the definitions that reach a bound
    /// name change from an import statement.
    fn rewrite_name_reference(&mut self, name: &ast::ExprName) {
        let Some(resolution) = self.model.reaching_definitions(name) else {
            return;
        };

        if resolution.crosses_scope_declaration() {
            // A bound name change across `global` or `nonlocal` requires updating the
            // declaration and the binding's reads, writes, and deletions together.
            // We don't support those coordinated rewrites yet.
            return;
        }

        let decision = rewrite_for_definition_resolution(&resolution, |definition| {
            if matches!(definition.kind(self.db), DefinitionKind::StarImport(_)) {
                // Module references introduced by star imports such as `from pkg import *`
                // are unsupported.
                RewriteDecision::Omit
            } else if let Some(replacement) = self.bound_name_changes.get(&definition) {
                RewriteDecision::Replace(replacement.clone())
            } else {
                RewriteDecision::Preserve
            }
        });

        self.record_reference_rewrite(name.into(), name.range(), decision);
    }

    /// Chooses a reference rewrite for an attribute such as `pkg.old`.
    ///
    /// Both the attribute and its receiver must resolve to modules.
    /// Re-exported attributes use [`ReexportAnalyzer`] to determine whether their bound names change.
    fn rewrite_attribute_reference(&mut self, attribute: &ast::ExprAttribute) {
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
        // For B, we do NOT want to rewrite `holder.old` to `holder.new`. There,
        // `pkg.old` relative to `holder` is `None`, so the attribute does
        // not match the old module name and we do not rewrite it.
        let attribute_matches_old_module_name = old_module_name
            .relative_to(receiver_module.name(self.db))
            .is_some_and(|relative_name| relative_name.as_str() == attribute_name);

        let decision = if self
            .model
            .definitions_for_module_global(receiver_module, attribute.attr.as_str())
            .is_some()
        {
            ReexportAnalyzer::default().rewrite_reexported_bound_name(
                self.db,
                self.model,
                self.module_name_changes,
                receiver_module,
                attribute.attr.as_str(),
            )
        } else if attribute_matches_old_module_name {
            RewriteDecision::replace_if_changed(
                attribute.attr.as_str(),
                new_module_name.last_component(),
            )
        } else {
            RewriteDecision::Preserve
        };

        self.record_reference_rewrite(attribute.into(), attribute.attr.range, decision);
    }

    /// Computes reference rewrites in string annotations, such as `value: 'old.C'`.
    ///
    /// Ordinary strings in `__all__ = ["old"]` or `importlib.import_module("pkg.old")`
    /// are left unchanged.
    fn rewrite_string_annotation_references(&mut self, string: &ast::ExprStringLiteral) {
        let Some((ast, model)) = self.model.enter_string_annotation(string) else {
            return;
        };
        let mut planner = ReferenceEditPlanner::new(
            self.db,
            &model,
            self.module_name_changes,
            self.bound_name_changes,
        );
        planner.visit_expr(ast.expr());
        self.edits.extend(planner.edits);
    }

    /// Records a module reference rewrite unless an existing binding could capture its replacement.
    fn record_reference_rewrite(
        &mut self,
        reference: ast::ExprRef<'_>,
        range: TextRange,
        decision: RewriteDecision,
    ) {
        let RewriteDecision::Replace(replacement) = decision else {
            return;
        };
        if self.replacement_has_conflicting_binding(reference, &replacement) {
            return;
        }

        self.edits.push(RangedValue {
            range: FileRange::new(self.model.file(), range),
            value: replacement,
        });
    }

    /// Checks whether an existing binding or declaration could capture the rewritten module reference.
    ///
    /// For `old` -> `new`, a parameter named `new` conflicts.
    /// For `pkg.old` -> `pkg.new`, an assignment such as `pkg.new = 42` conflicts.
    fn replacement_has_conflicting_binding(
        &self,
        reference: ast::ExprRef<'_>,
        replacement: &str,
    ) -> bool {
        let Some(scope) = self.model.scope(reference.into()) else {
            return false;
        };

        match reference {
            ast::ExprRef::Name(_) => self
                .model
                .has_visible_name_binding_or_declaration(scope, replacement),
            ast::ExprRef::Attribute(attribute) => {
                let mut replacement_attribute = attribute.clone();
                replacement_attribute.attr.id = replacement.into();
                PlaceExpr::try_from_expr(&replacement_attribute).is_some_and(|place| {
                    self.model.has_visible_binding_or_declaration(scope, &place)
                })
            }
            _ => false,
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for ReferenceEditPlanner<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        match node {
            AnyNodeRef::ExprName(name)
                // Rewrite names only when they are read. Assignment and deletion targets
                // such as `old = 0` and `del old` keep their names.
                if name.ctx.is_load()
                    && self
                        .module_name_changes
                        .is_old_module_basename(name.id.as_str()) =>
            {
                self.rewrite_name_reference(name);
            }
            AnyNodeRef::ExprAttribute(attribute)
                if self
                    .module_name_changes
                    .is_old_module_basename(attribute.attr.as_str()) =>
            {
                self.rewrite_attribute_reference(attribute);
            }
            AnyNodeRef::ExprStringLiteral(string) => {
                self.rewrite_string_annotation_references(string);
                return TraversalSignal::Skip;
            }
            _ => {}
        }
        TraversalSignal::Traverse
    }
}

/// Chooses a rewrite only when all reachable definitions agree on the replacement or preservation.
///
/// Incomplete definition resolution and reachable deletions prevent rewrites, but possible
/// unboundness alone does not: a conditional import can still establish the same replacement
/// wherever the name is bound.
///
/// If branches execute `from a import x` and `from b import x`, renaming `a.x` to `a.y` and
/// `b.x` to `b.z` rewrites both alias names but leaves the module reference in a subsequent
/// `print(x)` unchanged: neither `y` nor `z` works for both branches. Such partial results
/// can require manual fixes.
fn rewrite_for_definition_resolution<'db>(
    resolution: &DefinitionResolution<'db>,
    mut rewrite_for_definition: impl FnMut(Definition<'db>) -> RewriteDecision,
) -> RewriteDecision {
    if !resolution.is_complete() || resolution.may_be_deleted() {
        return RewriteDecision::Omit;
    }
    let Some((first, definitions)) = resolution.definitions().split_first() else {
        return RewriteDecision::Omit;
    };

    let decision = rewrite_for_definition(*first);
    if decision == RewriteDecision::Omit {
        return decision;
    }

    if definitions
        .iter()
        .copied()
        .any(|definition| rewrite_for_definition(definition) != decision)
    {
        return RewriteDecision::Omit;
    }

    decision
}

#[derive(Eq, PartialEq)]
enum RewriteDecision {
    /// Preserve the alias name or module reference.
    Preserve,
    /// Rewrite the alias name or module reference using this replacement.
    Replace(String),
    /// Resolution cannot establish a rewrite; omit the reference rewrite
    /// or reject the import from statement.
    Omit,
}

impl RewriteDecision {
    /// Returns `Replace` if the names differ, or `Preserve` if they match.
    fn replace_if_changed(old: &str, new: &str) -> Self {
        if old != new {
            return Self::Replace(new.to_string());
        }
        Self::Preserve
    }
}

/// Maps a supported file rename to its old and new module names.
fn module_rename(db: &dyn Db, rename: &FileRename) -> Option<(ModuleName, ModuleName)> {
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

    let old_module_name = file_to_module(db, ResolverFile::new(db, file, resolver_environment))?
        .name(db)
        .clone();

    // A runtime module and its stub share a module name. Renaming only the stub
    // must not redirect imports while the runtime module remains at its old path.
    if resolve_module_file(db, &old_module_name)? != file {
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
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, LintName, Severity,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::PythonVersion;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::Ranged;
    use ty_project::{ProjectMetadata, TestDb};
    use ty_python_semantic::ReachingDefinitionsRecordingMode;

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
         4 |
         5 | # This reads pkg.old to find the object whose VALUE attribute is assigned.
         6 | pkg.old.VALUE = 1
           |     --- new
         7 |
         8 | # This assigns the old attribute; only the right side reads pkg.old.
         9 | pkg.old = pkg.old
           |               --- new
        10 |
        11 | holder.old = pkg.old
           |                  --- new
        12 | print(holder.old)
           |
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
    fn reexport_cycles_do_not_suppress_independent_edits() {
        let sources = &[
            ("/old.py", ""),
            (
                "/a.py",
                "
                import old
                from b import old
                import old as independent
                print(old, independent)
                ",
            ),
            ("/b.py", "from a import old"),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> a.py:2:8
          |
        2 | import old
          |        --- new
        3 | from b import old
        4 | import old as independent
          |        --- new
        5 | print(old, independent)
          |
        ");
    }

    #[test]
    fn ambiguous_reexports_prevent_all_edits_to_an_import_from() {
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
                from facade import other, old
                print(other, old)
                ",
            ),
        ];

        // Reject the whole import from statement, including the preceding alias name rewrite.
        assert_snapshot!(rename_edits(&[
            ("/old.py", "/new.py"),
            ("/other.py", "/another.py"),
            ("/facade.py", "/renamed.py"),
        ], sources), @"
        info[file-rename]: Rename edits (4)
         --> facade.py:2:8
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
        3 | from facade import other, old
        4 | print(other, old)
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
          |
        ");
    }

    #[test]
    fn reachable_deletions_prevent_reference_rewrites() {
        let sources = &[
            ("/old.py", ""),
            (
                "/use.py",
                "
                import old
                if flag:
                    del old
                print(old)
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (1)
         --> use.py:2:8
          |
        2 | import old
          |        --- new
        3 | if flag:
        4 |     del old
        5 | print(old)
          |
        ");
    }

    #[test]
    fn scope_declarations_prevent_only_dependent_reference_rewrites() {
        // Both module references resolve to the same import definition. The global declaration
        // and its dependent reference require manual edits; the sibling's reference can be rewritten.
        let sources = &[
            ("/old.py", ""),
            (
                "/use.py",
                "
                import old
                def affected():
                    global old
                    return old
                def sibling():
                    return old
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[("/old.py", "/new.py")], sources), @"
        info[file-rename]: Rename edits (2)
         --> use.py:2:8
          |
        2 | import old
          |        --- new
        3 | def affected():
        4 |     global old
        5 |     return old
        6 | def sibling():
        7 |     return old
          |            --- new
        ");
    }

    #[test]
    fn name_capture_prevents_only_conflicting_reference_rewrites() {
        let sources = &[
            ("/old.py", "VALUE = 1"),
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "VALUE = 1"),
            ("/other/__init__.py", ""),
            ("/other/old.py", "VALUE = 1"),
            (
                "/use.py",
                "
                import old
                import pkg.old
                import other.old

                pkg.new = 42
                print(pkg.old.VALUE, other.old.VALUE)

                def captured(new):
                    return old.VALUE
                def sibling():
                    return old.VALUE
                ",
            ),
        ];

        assert_snapshot!(rename_edits(&[
            ("/old.py", "/new.py"),
            ("/pkg/old.py", "/pkg/new.py"),
            ("/other/old.py", "/other/new.py"),
        ], sources), @"
        info[file-rename]: Rename edits (5)
          --> use.py:2:8
           |
         2 | import old
           |        --- new
         3 | import pkg.old
           |        ------- pkg.new
         4 | import other.old
           |        --------- other.new
         5 |
         6 | pkg.new = 42
         7 | print(pkg.old.VALUE, other.old.VALUE)
           |                            --- new
         8 |
         9 | def captured(new):
        10 |     return old.VALUE
        11 | def sibling():
        12 |     return old.VALUE
           |            --- new
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
    fn edits_only_candidate_files() {
        let db = test_db(&[
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
        let included = system_path_to_file(&db, "/included.py").unwrap();
        let rename = file_renames(&db, &[("/old.py", "/new.py")]);

        let edits = will_rename_files(&db, &rename, [included]);
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
        let edits = will_rename_files(&db, &renames, &db.project().files(&db));
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
        let mut db = TestDb::with_reaching_definitions_recording_mode(
            ProjectMetadata::new("test", "/".into()),
            ReachingDefinitionsRecordingMode::Enabled,
        );
        db.set_python_version(PythonVersion::latest_ty());
        db.write_files(files.iter().map(|(path, source)| (path, dedent(source))))
            .unwrap();
        db
    }
}
