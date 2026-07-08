//! Computes source edits that should accompany Python module and package renames.
//!
//! [`will_rename_paths`] accepts a batch of filesystem renames and a set of candidate Python
//! sources. It does not rename anything itself. The filesystem operation cannot be cancelled, so
//! the function returns every independently coherent edit that it can determine without guessing.
//!
//! # Supported renames
//!
//! Python files may be renamed when they retain their `.py` or `.pyi` extension, are not package
//! initializers, and resolve to the module being moved. File moves may cross package boundaries
//! when every affected import can still be represented without splitting a statement or rebasing
//! a relative import. An unaliased `import` must retain its implicit root binding; an existing
//! explicit `as` alias permits a root-changing path rewrite.
//!
//! Directory renames support resolver-visible regular packages that remain under the same logical
//! parent. Co-located runtime and stub initializers may move together. Namespace packages, split
//! or merged packages, and cross-parent package moves are unsupported. Relative imports within a
//! moved source remain unchanged when coordinated moves preserve their written relationship.
//!
//! Import rewrites follow the runtime source selected by the resolver. Renaming only a shadowing
//! stub does not redirect runtime imports. A package-directory rename may not add or remove the
//! top-level `-stubs` suffix.
//!
//! # Binding policy
//!
//! Import syntax determines which local spelling changes. An explicit `as` alias always remains
//! fixed, even when it repeats the renamed component. Other occurrences are rewritten only when
//! their inferred module is affected and every live binding supports one spelling
//! policy. Bindings introduced by assignments and other non-import targets remain stable.
//!
//! For example, renaming `pkg/old.py` to `pkg/new.py` produces these edits:
//!
//! ```text
//! # Before
//! import pkg.old
//! from pkg import old as stable
//! print(pkg.old.C, stable.C)
//!
//! # After
//! import pkg.new
//! from pkg import new as stable
//! print(pkg.new.C, stable.C)
//! ```
//!
//! # Conservative fallback
//!
//! Unsupported affected syntax is omitted together with any edits that depend on it. This includes
//! imports that require splitting or relative rebasing, ambiguous binding policies, affected
//! module paths used directly as write or delete targets, bare names read or reassigned through
//! `global` or `nonlocal`, and references reached only through a star-import export chain. A renamed
//! qualifier inside a larger attribute target remains eligible. The path of a direct star import
//! may still be rewritten, and independent imports and occurrences remain eligible for edits.
//! Deferred references, class-scope fallbacks, and conditional bindings are rewritten when all
//! reachable providers agree on the replacement.
//!
//! Coincidental text is not a reference. The feature does not use `__all__`, dynamic lookup, or
//! runtime strings to discover affected modules. Valid single-literal forward annotations follow
//! ordinary semantic rules; implicitly concatenated or malformed annotations and legacy type
//! comments are ignored. A source that cannot be read or whose generated edits conflict is omitted
//! without suppressing edits for other sources.

use crate::RangedValue;
use rayon::prelude::*;
use ruff_db::PythonFile;
use ruff_db::files::{File, FileRange, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_trivia::is_identifier_continuation;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, ModuleResolveMode, ResolverEnvironment, ResolverFile,
    file_to_module, is_legacy_namespace_package, resolve_module_confident,
    resolve_real_module_confident, search_paths,
};
use ty_project::{Db, parallel::ParallelIteratorExt};
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState};
use ty_python_core::{
    BindingWithConstraintsIterator, BoundnessAnalysis, ProgramFile, global_scope, place_table,
    use_def_map,
};
use ty_python_semantic::types::Type;
use ty_python_semantic::{
    HasType, ImplicitPlaceLoad, ImportAliasResolution, PlaceLoad, PlaceLoadFallbacks,
    PlaceLoadSource, ResolvedDefinition, SemanticModel, binding_type,
    definitions_for_imported_symbol, reachable_bindings, user_visible_definitions,
};
use unicode_normalization::UnicodeNormalization;

/// Computes normalized source edits for a batch of filesystem renames.
///
/// `files` is the candidate set discovered by the caller. Files outside `in_scope` are skipped.
/// Directly renamed files are added automatically when they are in scope; directory contents are
/// not discovered here and must be included in `files` by the caller.
///
/// The supported contract covers `.py` and `.pyi` module moves and same-parent regular-package
/// renames. It rewrites representable imports and semantic references when every reachable binding
/// agrees on the new spelling. Dynamic references, cross-parent package moves, bare names governed
/// by `global` or `nonlocal`, and renamed module paths used directly as write or delete targets are
/// intentionally omitted. Renamed qualifiers within larger attribute targets remain eligible.
///
/// Returned edits are sorted, deduplicated, and guaranteed not to overlap. Unsupported rename
/// rules, source files, import statements, and semantic occurrences are omitted independently.
pub fn will_rename_paths(
    db: &dyn Db,
    renames: &[PathRename],
    files: impl IntoIterator<Item = File>,
    in_scope: impl Fn(File) -> bool,
) -> WillRenameResult {
    let plan = RenamePlan::new(db, renames, &in_scope);
    let mut known_omissions = plan.known_omissions;
    let mut files: Vec<_> = files.into_iter().filter(|file| in_scope(*file)).collect();
    for file in plan.rules.iter().filter_map(RenameRule::file) {
        files.push(file);
    }
    files.sort_unstable_by_key(|file| file.path(db).as_ref());
    files.dedup();
    let analyses = files
        .into_par_iter()
        .map_with_db(db, |db, file| edits_for_file(db, file, &plan))
        .collect::<Vec<_>>();
    let mut edits = Vec::new();
    for analysis in analyses {
        edits.extend(analysis.edits);
        known_omissions |= analysis.known_omissions;
    }
    WillRenameResult {
        edits,
        known_omissions,
    }
}

/// Edits for a filesystem rename and whether relevant work was knowingly omitted.
pub struct WillRenameResult {
    edits: Vec<FileRenameEdit>,
    known_omissions: bool,
}

impl WillRenameResult {
    /// Returns the normalized, non-overlapping source edits.
    pub fn into_edits(self) -> Vec<FileRenameEdit> {
        self.edits
    }

    /// Returns `true` when analysis knowingly omitted a relevant rename or occurrence.
    /// A `false` result does not account for dynamic or otherwise out-of-policy references.
    pub fn has_known_omissions(&self) -> bool {
        self.known_omissions
    }
}

/// One filesystem path rename in a batch.
pub struct PathRename {
    old_path: SystemPathBuf,
    new_path: SystemPathBuf,
    kind: RenameKind,
}

impl PathRename {
    /// Creates a Python file rename.
    pub fn file(old_path: SystemPathBuf, new_path: SystemPathBuf) -> Self {
        Self::new(old_path, new_path, RenameKind::File)
    }

    /// Creates a package-directory rename.
    pub fn directory(old_path: SystemPathBuf, new_path: SystemPathBuf) -> Self {
        Self::new(old_path, new_path, RenameKind::Directory)
    }

    fn new(old_path: SystemPathBuf, new_path: SystemPathBuf, kind: RenameKind) -> Self {
        Self {
            old_path,
            new_path,
            kind,
        }
    }
}

/// A replacement and the file range containing it.
pub type FileRenameEdit = RangedValue<String>;

const UNSUPPORTED_RENAME: &str = "unsupported rename request";
const CONFLICTING_RENAMES: &str = "conflicting rename rules";
const OUT_OF_SCOPE: &str = "renamed source is outside the workspace";
const UNREADABLE_SOURCE: &str = "candidate source cannot be read";
const UNREPRESENTABLE_IMPORT: &str = "affected import cannot be represented";
const UNSUPPORTED_SEMANTIC: &str = "semantic occurrence is unsupported";
const CONFLICTING_EDITS: &str = "generated edits overlap";

fn omit(reason: &'static str) {
    tracing::debug!(reason, "Omitting part of `workspace/willRenameFiles`");
}

#[derive(Clone, Copy)]
enum RenameKind {
    File,
    Directory,
}

struct RenamePlan {
    rules: Vec<RenameRule>,
    names: FxHashMap<String, bool>,
    known_omissions: bool,
}

impl RenamePlan {
    fn new(db: &dyn Db, renames: &[PathRename], in_scope: &impl Fn(File) -> bool) -> Self {
        let cwd = db.system().current_directory();
        let mut rejected = vec![false; renames.len()];
        let mut known_omissions = false;
        let mut file_facets = FxHashMap::default();
        for (index, rename) in renames.iter().enumerate() {
            let old = SystemPath::absolute(&rename.old_path, cwd);
            if matches!(rename.kind, RenameKind::File)
                && matches!(old.extension(), Some("py" | "pyi"))
                && let Some(previous) = file_facets.insert(old.with_extension(""), index)
            {
                omit(UNSUPPORTED_RENAME);
                known_omissions = true;
                rejected[index] = true;
                rejected[previous] = true;
            }
        }

        let mut rules: Vec<_> = renames
            .iter()
            .enumerate()
            .filter_map(|(index, rename)| {
                if rejected[index] {
                    return None;
                }
                let Some(rule) = RenameRule::new(db, rename) else {
                    omit(UNSUPPORTED_RENAME);
                    known_omissions = true;
                    return None;
                };
                if rule.old_name == rule.new_name {
                    return None;
                }
                if rule.file().is_some_and(|file| !in_scope(file)) {
                    omit(OUT_OF_SCOPE);
                    known_omissions = true;
                    return None;
                }
                Some((index, rule))
            })
            .collect();

        let mut paths: Vec<_> = rules
            .iter()
            .flat_map(|(index, _)| {
                let rename = &renames[*index];
                [&rename.old_path, &rename.new_path]
                    .map(|path| (SystemPath::absolute(path, cwd), *index))
            })
            .collect();
        paths.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut ancestors: Vec<(SystemPathBuf, usize)> = Vec::new();
        for (path, index) in paths {
            while ancestors
                .last()
                .is_some_and(|(ancestor, _)| !path.starts_with(ancestor))
            {
                ancestors.pop();
            }
            if let Some((_, ancestor)) = ancestors.last() {
                rejected[index] = true;
                rejected[*ancestor] = true;
            }
            ancestors.push((path, index));
        }

        rules.sort_unstable_by(|left, right| left.1.old_name.cmp(&right.1.old_name));
        let mut ancestors: Vec<usize> = Vec::new();
        for index in 0..rules.len() {
            while ancestors.last().is_some_and(|ancestor| {
                !rules[index]
                    .1
                    .old_name
                    .starts_with(&rules[*ancestor].1.old_name)
            }) {
                ancestors.pop();
            }
            if let Some(ancestor) = ancestors.last()
                && rules[*ancestor]
                    .1
                    .rewrites_name(&rules[index].1.old_name)
                    .is_some()
            {
                rejected[rules[index].0] = true;
                rejected[rules[*ancestor].0] = true;
            }
            ancestors.push(index);
        }

        let mut destinations: Vec<_> = (0..rules.len()).collect();
        destinations
            .sort_unstable_by(|left, right| rules[*left].1.new_name.cmp(&rules[*right].1.new_name));
        let mut ancestors: Vec<usize> = Vec::new();
        for index in destinations {
            while ancestors.last().is_some_and(|ancestor| {
                !rules[index]
                    .1
                    .new_name
                    .starts_with(&rules[*ancestor].1.new_name)
            }) {
                ancestors.pop();
            }
            if let Some(ancestor) = ancestors.last() {
                rejected[rules[index].0] = true;
                rejected[rules[*ancestor].0] = true;
            }
            ancestors.push(index);
        }
        rules.retain(|(index, _)| {
            if rejected[*index] {
                omit(CONFLICTING_RENAMES);
                known_omissions = true;
                false
            } else {
                true
            }
        });

        let rules: Vec<_> = rules.into_iter().map(|(_, rule)| rule).collect();
        let mut names = FxHashMap::default();
        for rule in &rules {
            let name = &rule.old_name;
            names
                .entry(name.first_component().to_owned())
                .or_insert(false);
            names.insert(name.last_component().to_owned(), true);
        }
        Self {
            rules,
            names,
            known_omissions,
        }
    }

    fn rewrite(&self, db: &dyn Db, module: Module<'_>) -> Option<(&RenameRule, ModuleName)> {
        let rule = self.rule(module.name(db))?;
        rule.rewrite(db, module).map(|name| (rule, name))
    }

    fn rule(&self, name: &ModuleName) -> Option<&RenameRule> {
        // Descendants stay relevant even when resolution fails and a file rule cannot rewrite them.
        name.ancestors().find_map(|ancestor| {
            let index = self
                .rules
                .binary_search_by(|r| r.old_name.cmp(&ancestor))
                .ok()?;
            Some(&self.rules[index])
        })
    }

    fn mentions_text(&self, text: &str) -> bool {
        text.split(|c: char| !is_identifier_continuation(c))
            .any(|name| {
                let normalized = (!name.is_ascii()).then(|| name.nfkc().collect::<String>());
                self.names
                    .contains_key(normalized.as_deref().unwrap_or(name))
            })
    }

    fn terminal(&self, name: &str) -> bool {
        self.names.get(name).copied().unwrap_or(false)
    }
}

struct RenameRule {
    old_name: ModuleName,
    new_name: ModuleName,
    scope: RenameScope,
}

impl RenameRule {
    fn new(db: &dyn Db, rename: &PathRename) -> Option<Self> {
        let resolver_environment = resolver_environment(db);
        let python_version = resolver_environment.python_version(db);
        let old = SystemPath::absolute(&rename.old_path, db.system().current_directory());
        let new = SystemPath::absolute(&rename.new_path, db.system().current_directory());
        let (old_name, scope) = match rename.kind {
            RenameKind::File => {
                let extension = old.extension()?;
                (matches!(extension, "py" | "pyi")
                    && new.extension() == Some(extension)
                    && !matches!(old.file_stem(), Some("__init__"))
                    && !matches!(new.file_stem(), Some("__init__")))
                .then_some(())?;
                let file = system_path_to_file(db, &old).ok()?;
                let name = file_to_module(db, ResolverFile::new(db, file, resolver_environment))?
                    .name(db)
                    .clone();
                (resolved_source(db, &name)? == file).then_some((name, RenameScope::File(file)))?
            }
            RenameKind::Directory => {
                if !db.system().is_directory(&old) || new.starts_with(&old) {
                    return None;
                }
                let inits: Vec<_> = [old.join("__init__.py"), old.join("__init__.pyi")]
                    .into_iter()
                    .filter(|path| db.system().is_file(path))
                    .filter_map(|path| system_path_to_file(db, &path).ok())
                    .collect();
                (!inits.is_empty()
                    && !inits.iter().any(|file| {
                        is_legacy_namespace_package(db, PythonFile::new(db, *file, python_version))
                    }))
                .then_some(())?;
                let name = inits
                    .iter()
                    .find_map(|file| {
                        file_to_module(db, ResolverFile::new(db, *file, resolver_environment))
                    })?
                    .name(db)
                    .clone();
                if [
                    resolve_module_confident(db, resolver_environment, &name)
                        .and_then(|module| module.file(db)),
                    resolve_real_module_confident(db, resolver_environment, &name)
                        .and_then(|module| module.file(db)),
                ]
                .into_iter()
                .flatten()
                .any(|file| !file_within(db, file, &old))
                    || resolved_source(db, &name).is_none_or(|file| !file_within(db, file, &old))
                {
                    return None;
                }
                (name, RenameScope::Package(old.clone()))
            }
        };
        let (new_name, destination_priority) = prospective_module(db, &new)?;
        if destination_is_shadowed(db, &new_name, destination_priority)
            || destination_has_non_package_ancestor(db, &new_name, &scope)
        {
            return None;
        }
        if matches!(rename.kind, RenameKind::Directory) {
            let destination = new.file_name()?;
            (old_name.parent() == new_name.parent()
                && old.file_name()?.ends_with("-stubs") == destination.ends_with("-stubs")
                && destination.strip_suffix("-stubs").unwrap_or(destination)
                    == new_name.last_component())
            .then_some(())?;
        }
        Some(Self {
            old_name,
            new_name,
            scope,
        })
    }

    fn rewrite(&self, db: &dyn Db, module: Module<'_>) -> Option<ModuleName> {
        let source = resolved_source(db, module.name(db)).or_else(|| module.file(db));
        let applies = match (&self.scope, source) {
            (RenameScope::File(expected), Some(actual)) => *expected == actual,
            (RenameScope::Package(root), Some(file)) => file_within(db, file, root),
            (RenameScope::Package(_), None) => self.rewrites_name(module.name(db)).is_some(),
            (RenameScope::File(_), None) => false,
        };
        applies.then(|| self.rewrites_name(module.name(db)))?
    }

    fn rewrites_name(&self, name: &ModuleName) -> Option<ModuleName> {
        if name == &self.old_name {
            return Some(self.new_name.clone());
        }
        matches!(self.scope, RenameScope::Package(_)).then_some(())?;
        let mut rewritten = self.new_name.clone();
        rewritten.extend(&name.relative_to(&self.old_name)?);
        Some(rewritten)
    }

    fn file(&self) -> Option<File> {
        let RenameScope::File(file) = self.scope else {
            return None;
        };
        Some(file)
    }
}

enum RenameScope {
    File(File),
    Package(SystemPathBuf),
}

fn prospective_module(db: &dyn Db, path: &SystemPath) -> Option<(ModuleName, usize)> {
    search_paths(db, resolver_environment(db), ModuleResolveMode::Typing)
        .enumerate()
        .filter(|(_, search_path)| !search_path.is_standard_library())
        .find_map(|(priority, search_path)| {
            Some((search_path.module_name_for_system_path(path)?, priority))
        })
}

fn destination_is_shadowed(db: &dyn Db, name: &ModuleName, destination_priority: usize) -> bool {
    let resolver_environment = resolver_environment(db);
    [
        resolve_module_confident(db, resolver_environment, name),
        resolve_real_module_confident(db, resolver_environment, name),
    ]
    .into_iter()
    .flatten()
    .filter_map(|module| module.search_path(db))
    .any(|resolved_path| {
        search_paths(db, resolver_environment, ModuleResolveMode::Typing)
            .position(|search_path| search_path == resolved_path)
            .is_some_and(|resolved_priority| resolved_priority <= destination_priority)
    })
}

fn destination_has_non_package_ancestor(
    db: &dyn Db,
    name: &ModuleName,
    scope: &RenameScope,
) -> bool {
    let resolver_environment = resolver_environment(db);
    name.ancestors().skip(1).any(|ancestor| {
        [
            resolve_module_confident(db, resolver_environment, &ancestor),
            resolve_real_module_confident(db, resolver_environment, &ancestor),
        ]
        .into_iter()
        .flatten()
        .any(|module| {
            module.kind(db).is_module()
                && module.file(db).is_none_or(|file| match scope {
                    RenameScope::File(moved) => file != *moved,
                    RenameScope::Package(root) => !file_within(db, file, root),
                })
        })
    })
}

fn resolved_source(db: &dyn Db, name: &ModuleName) -> Option<File> {
    let resolver_environment = resolver_environment(db);
    resolve_real_module_confident(db, resolver_environment, name)
        .or_else(|| resolve_module_confident(db, resolver_environment, name))?
        .file(db)
}

fn resolver_environment(db: &dyn Db) -> ResolverEnvironment<'_> {
    db.project().program(db).resolver_environment(db)
}

fn file_within(db: &dyn Db, file: File, root: &SystemPath) -> bool {
    matches!(file.path(db).as_system_path(), Some(path) if path.starts_with(root))
}

fn edits_for_file(db: &dyn Db, file: File, plan: &RenamePlan) -> WillRenameResult {
    let program_file = db.program_file(file);
    let moved_source = file_to_module(db, program_file.resolver_file(db))
        .and_then(|module| plan.rewrite(db, module))
        .map(|(rule, new_name)| SourceMove::new(db, file, rule, &new_name));
    let source = source_text(db, file);
    if source.read_error().is_some() {
        omit(UNREADABLE_SOURCE);
        return WillRenameResult {
            edits: Vec::new(),
            known_omissions: moved_source.is_some(),
        };
    }
    if moved_source.is_none() && !plan.mentions_text(source.as_str()) {
        return WillRenameResult {
            edits: Vec::new(),
            known_omissions: false,
        };
    }
    let module = ruff_db::parsed::parsed_module(db, program_file.python_file(db)).load(db);
    let root = AnyNodeRef::from(module.syntax());
    let model = SemanticModel::new(db, program_file);
    let mut imports = ImportPass {
        db,
        model: &model,
        plan,
        moved_source: moved_source.as_ref(),
        output: ImportEdits::default(),
        known_omissions: false,
    };
    root.visit_source_order(&mut imports);
    let (mut edits, changes, mut known_omissions) = imports.finish();
    let mut semantics = SemanticPass {
        db,
        model: &model,
        plan,
        changes: &changes,
        edits: Vec::new(),
        augmented: None,
        known_omissions: false,
    };
    root.visit_source_order(&mut semantics);
    known_omissions |= semantics.known_omissions;
    edits.extend(semantics.edits);
    let edits = edits
        .into_iter()
        .map(|(range, value)| RangedValue {
            range: FileRange::new(file, range),
            value,
        })
        .collect();
    match normalize(edits) {
        Some(edits) => WillRenameResult {
            edits,
            known_omissions,
        },
        None => {
            omit(CONFLICTING_EDITS);
            WillRenameResult {
                edits: Vec::new(),
                known_omissions: true,
            }
        }
    }
}

struct SourceMove {
    packages: Option<(ModuleName, ModuleName)>,
    cross_parent: bool,
}

impl SourceMove {
    fn new(db: &dyn Db, file: File, rule: &RenameRule, new_name: &ModuleName) -> Self {
        let old_package =
            ModuleName::package_for_file(db, ImportingFile::File(file, resolver_environment(db)))
                .ok();
        let new_package = match rule.scope {
            RenameScope::File(_) => new_name.parent(),
            RenameScope::Package(_) => old_package.as_ref().and_then(|old| rule.rewrites_name(old)),
        };
        Self {
            cross_parent: matches!(rule.scope, RenameScope::File(_)) && old_package != new_package,
            packages: old_package.zip(new_package),
        }
    }

    fn relative_parent(&self, level: u32, current: &ModuleName) -> Option<ModuleName> {
        let ancestor = level.checked_sub(1)? as usize;
        let (old_package, new_package) = self.packages.as_ref()?;
        let old_base = old_package.ancestors().nth(ancestor)?;
        let mut new_base = new_package.ancestors().nth(ancestor)?;
        if current != &old_base {
            new_base.extend(&current.relative_to(&old_base)?);
        }
        Some(new_base)
    }
}

struct BindingChange {
    new: String,
}

type Changes<'db> = FxHashMap<Definition<'db>, BindingChange>;

#[derive(Default)]
struct ImportEdits<'db> {
    edits: Vec<(TextRange, String)>,
    changes: Changes<'db>,
}

impl ImportEdits<'_> {
    fn extend(&mut self, other: Self) {
        self.edits.extend(other.edits);
        self.changes.extend(other.changes);
    }
}

struct ImportPass<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    plan: &'a RenamePlan,
    moved_source: Option<&'a SourceMove>,
    output: ImportEdits<'db>,
    known_omissions: bool,
}

impl<'db> ImportPass<'_, 'db> {
    fn finish(self) -> (Vec<(TextRange, String)>, Changes<'db>, bool) {
        (self.output.edits, self.output.changes, self.known_omissions)
    }

    fn record(&self, output: &mut ImportEdits<'db>, alias: &ast::Alias, old: &str, new: &str) {
        if alias.asname.is_some() || old == new {
            return;
        }
        let definition = ty_python_core::semantic_index(self.db, self.model.program_file())
            .expect_single_definition(alias);
        output.changes.insert(
            definition,
            BindingChange {
                new: new.to_string(),
            },
        );
    }

    fn import(&self, import: &ast::StmtImport) -> Option<ImportEdits<'db>> {
        let mut output = ImportEdits::default();
        for alias in &import.names {
            let written = ModuleName::new(alias.name.as_str())?;
            let Some(module) = self.model.resolve_module(Some(alias.name.as_str()), 0) else {
                if self.plan.rule(&written).is_some() {
                    return None;
                }
                continue;
            };
            let Some((rule, new)) = self.plan.rewrite(self.db, module) else {
                continue;
            };
            let old = module.name(self.db);
            if alias.asname.is_none()
                && matches!(rule.scope, RenameScope::File(_))
                && old.parent() != new.parent()
                && old.first_component() != new.first_component()
            {
                return None;
            }
            let new_binding = if old.parent().is_none()
                && new.parent().is_some()
                && old.first_component() == new.first_component()
            {
                new.as_str()
            } else {
                new.first_component()
            };
            self.record(&mut output, alias, old.first_component(), new_binding);
            if alias.name.as_str() != new.as_str() {
                output
                    .edits
                    .push((alias.name.range, new.as_str().to_string()));
            }
        }
        Some(output)
    }

    fn import_from(&self, import: &ast::StmtImportFrom) -> Option<ImportEdits<'db>> {
        let mut output = ImportEdits::default();
        let Ok(old_parent) = ModuleName::from_import_statement(
            self.db,
            ImportingFile::ResolverFile(self.model.program_file().resolver_file(self.db)),
            import,
        ) else {
            return (!self
                .moved_source
                .is_some_and(|source| import.level > 0 && source.cross_parent))
            .then_some(output);
        };
        let resolved_parent = self.model.resolve_module(
            import.module.as_ref().map(ast::Identifier::as_str),
            import.level,
        );
        if resolved_parent.is_none() && self.plan.rule(&old_parent).is_some() {
            return None;
        }
        let rewritten_parent = resolved_parent
            .and_then(|module| self.plan.rewrite(self.db, module))
            .map_or_else(|| old_parent.clone(), |(_, name)| name);
        let written_parent = match self.moved_source.filter(|_| import.level > 0) {
            Some(source) => source.relative_parent(import.level, &old_parent)?,
            None => old_parent.clone(),
        };
        let mut desired_parent = None;
        let mut unresolved = false;
        for alias in &import.names {
            let (module, resolved) =
                imported_symbol(self.db, self.model, import, alias, self.plan).ok()?;
            unresolved |= !resolved;
            let parent = if let Some(module) = module {
                let old = module.name(self.db);
                if let Some((_, new)) = self.plan.rewrite(self.db, module) {
                    if alias.name.as_str() != old.last_component()
                        || old.parent().as_ref() != Some(&old_parent)
                    {
                        return None;
                    }
                    self.record(
                        &mut output,
                        alias,
                        old.last_component(),
                        new.last_component(),
                    );
                    if alias.name.as_str() != new.last_component() {
                        output
                            .edits
                            .push((alias.name.range, new.last_component().to_string()));
                    }
                    new.parent()?
                } else if old.parent().as_ref() == Some(&old_parent) {
                    old_parent.clone()
                } else {
                    rewritten_parent.clone()
                }
            } else {
                rewritten_parent.clone()
            };
            if desired_parent.get_or_insert_with(|| parent.clone()) != &parent {
                return None;
            }
        }
        let desired_parent = desired_parent.unwrap_or_else(|| rewritten_parent.clone());
        if unresolved
            && (written_parent != old_parent
                || desired_parent != old_parent
                || desired_parent != rewritten_parent)
        {
            return None;
        }
        if desired_parent != written_parent {
            let module = import.module.as_ref()?;
            let replacement = if import.level == 0 {
                desired_parent.as_str().to_string()
            } else {
                relative_replacement(module.as_str(), &written_parent, &desired_parent)?
            };
            if replacement == module.as_str() {
                return None;
            }
            output.edits.push((module.range, replacement));
        }
        Some(output)
    }
}

impl<'a> SourceOrderVisitor<'a> for ImportPass<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        let output = match node {
            AnyNodeRef::StmtImport(import) => self.import(import),
            AnyNodeRef::StmtImportFrom(import) => self.import_from(import),
            _ => return TraversalSignal::Traverse,
        };
        if let Some(output) = output {
            self.output.extend(output);
        } else {
            omit(UNREPRESENTABLE_IMPORT);
            self.known_omissions = true;
        }
        TraversalSignal::Skip
    }
}

fn imported_symbol<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    import: &ast::StmtImportFrom,
    alias: &ast::Alias,
    plan: &RenamePlan,
) -> Result<(Option<Module<'db>>, bool), ()> {
    let definitions = definitions_for_imported_symbol(
        model,
        import,
        alias.name.as_str(),
        ImportAliasResolution::ResolveAliases,
    );
    let Some(module) = module_from_type(model, alias) else {
        return Ok((None, alias.name.as_str() == "*" || !definitions.is_empty()));
    };
    let matches = !definitions.is_empty()
        && definitions.iter().all(|definition| {
            matches!(definition, ResolvedDefinition::Module(file) if file_to_module(db, file.resolver_file(db)).is_some_and(|resolved| resolved.name(db) == module.name(db)))
        });
    if !matches && plan.rewrite(db, module).is_some() {
        return Err(());
    }
    Ok((
        matches.then_some(module),
        matches || !definitions.is_empty(),
    ))
}

fn relative_replacement(text: &str, old: &ModuleName, new: &ModuleName) -> Option<String> {
    let suffix = text.split('.').count();
    let old: Vec<_> = old.components().collect();
    let new: Vec<_> = new.components().collect();
    let prefix = old.len().checked_sub(suffix)?;
    (old.len() == new.len() && old[..prefix] == new[..prefix]).then(|| new[prefix..].join("."))
}

struct SemanticPass<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    plan: &'a RenamePlan,
    changes: &'a Changes<'db>,
    edits: Vec<(TextRange, String)>,
    augmented: Option<TextRange>,
    known_omissions: bool,
}

impl<'db> SemanticPass<'_, 'db> {
    fn name(&mut self, name: &ast::ExprName) {
        if name.ctx.is_del() {
            if self.plan.terminal(name.id.as_str())
                && self.model.name_load(name).is_some_and(|load| {
                    self.summarize_load(&load, |_| Ok(None))
                        .definitions
                        .iter()
                        .any(|definition| self.changes.contains_key(definition))
                })
            {
                omit(UNSUPPORTED_SEMANTIC);
                self.known_omissions = true;
            }
            return;
        }
        if name.ctx.is_store() && self.augmented != Some(name.range) {
            return;
        }
        let affected = module_from_type(self.model, name)
            .is_some_and(|module| self.plan.rewrite(self.db, module).is_some());
        let decision = self.model.name_load(name).map_or_else(
            || {
                if affected {
                    Decision::Unsupported
                } else {
                    Decision::Keep
                }
            },
            |load| {
                let decision = self
                    .summarize_load(&load, |definition| {
                        if affected
                            && matches!(definition.kind(self.db), DefinitionKind::StarImport(_))
                        {
                            return Err(());
                        }
                        Ok(self
                            .changes
                            .get(&definition)
                            .map(|change| change.new.clone()))
                    })
                    .decision(affected);
                if !load.scope_declarations().is_empty() && matches!(decision, Decision::Replace(_))
                {
                    Decision::Unsupported
                } else {
                    decision
                }
            },
        );
        self.apply(name.range, decision, name.ctx.is_store());
    }

    fn summarize_load(
        &self,
        load: &PlaceLoad<'db>,
        replacement_for: impl Fn(Definition<'db>) -> Result<Option<String>, ()>,
    ) -> ProviderSummary<'db> {
        let (lexical, post_lexical, conditional) = match load.fallbacks() {
            PlaceLoadFallbacks::Unconditional {
                lexical,
                post_lexical,
            } => (lexical, post_lexical, false),
            PlaceLoadFallbacks::IfNoPlaceExprPrefixIsBound {
                lexical,
                post_lexical,
                ..
            } => (lexical, post_lexical, true),
        };

        let mut summary = ProviderSummary::default();
        let mut can_fall_through = true;
        for source in load.local_sources().iter().chain(lexical) {
            if !can_fall_through {
                break;
            }
            let source = self.summarize_source(source, &replacement_for);
            can_fall_through = source.can_fall_through;
            summary.merge(source);
        }
        if can_fall_through && (post_lexical.is_some() || conditional) {
            summary.issue = ProviderIssue::Unknown;
        }
        summary.can_fall_through = can_fall_through && post_lexical.is_none();
        summary
    }

    fn summarize_source(
        &self,
        source: &PlaceLoadSource<'db>,
        replacement_for: &impl Fn(Definition<'db>) -> Result<Option<String>, ()>,
    ) -> ProviderSummary<'db> {
        let Some(bindings) = source.reachable_bindings(self.db) else {
            let mut summary = ProviderSummary::default();
            match source.implicit() {
                Some(ImplicitPlaceLoad::DunderClass(definition)) => {
                    summary.record_definition(self.db, *definition, replacement_for);
                }
                Some(ImplicitPlaceLoad::ClassBodySymbol(_)) => {
                    summary.can_fall_through = true;
                }
                Some(ImplicitPlaceLoad::ExplicitGlobalSymbol { .. }) | None => {
                    summary.can_fall_through = true;
                }
            }
            return summary;
        };
        let mut summary = self.summarize_bindings(bindings, replacement_for);
        if source.is_class_body_global_fallback() && summary.has_provider() {
            summary.can_fall_through = false;
        }
        summary
    }

    fn summarize_bindings(
        &self,
        bindings: ty_python_semantic::ReachableBindings<'db>,
        replacement_for: &impl Fn(Definition<'db>) -> Result<Option<String>, ()>,
    ) -> ProviderSummary<'db> {
        let boundness = bindings.boundness_analysis();
        let mut summary = ProviderSummary::default();
        let mut may_be_absent = false;
        for binding in bindings {
            match binding.state() {
                DefinitionState::Defined(definition) => {
                    summary.record_definition(self.db, definition, replacement_for);
                }
                DefinitionState::Deleted => may_be_absent |= binding.reachability().may_be_true(),
                DefinitionState::Undefined
                    if boundness == BoundnessAnalysis::BasedOnUnboundVisibility =>
                {
                    may_be_absent |= binding.reachability().may_be_true();
                }
                DefinitionState::Undefined => {}
            }
        }
        summary.can_fall_through = may_be_absent || !summary.has_provider();
        summary
    }

    fn attribute(&mut self, attribute: &ast::ExprAttribute) -> TraversalSignal {
        let attribute_module = module_from_type(self.model, attribute);
        let rewrite = attribute_module.and_then(|module| self.plan.rewrite(self.db, module));
        let receiver_module = module_from_type(self.model, &*attribute.value);
        let bindings = module_attribute_bindings(self.db, self.model, attribute);
        let has_bindings = bindings.is_some();
        let mut root = &*attribute.value;
        while let ast::Expr::Attribute(attribute) = root {
            root = &attribute.value;
        }
        let decision = match bindings {
            Some(bindings) => {
                let summary =
                    self.summarize_bindings(reachable_bindings(self.db, bindings), &|definition| {
                        if rewrite.is_some()
                            && matches!(definition.kind(self.db), DefinitionKind::StarImport(_))
                        {
                            return Err(());
                        }
                        let Type::ModuleLiteral(module) = binding_type(self.db, definition) else {
                            return Err(());
                        };
                        Ok(self.plan.rewrite(self.db, module.module(self.db)).and_then(
                            |(_, new)| {
                                implicit_import_name(self.db, definition, &new).map(str::to_string)
                            },
                        ))
                    });
                // A package stub can describe an alias differently from the runtime initializer.
                // Do not use the stub's binding to decide how runtime source should be rewritten.
                if rewrite.is_some()
                    && receiver_module
                        .and_then(|module| resolved_source(self.db, module.name(self.db)))
                        .is_some_and(|runtime_file| {
                            summary
                                .definitions
                                .iter()
                                .any(|definition| definition.file(self.db) != runtime_file)
                        })
                {
                    Decision::Unsupported
                } else {
                    summary.decision(rewrite.is_some())
                }
            }
            None if receiver_module.is_some() => {
                rewrite.as_ref().map_or(Decision::Keep, |(_, name)| {
                    if attribute_module
                        .is_some_and(|module| self.is_implicitly_imported_submodule(root, module))
                    {
                        replace(attribute.attr.as_str(), name.last_component())
                    } else {
                        Decision::Unsupported
                    }
                })
            }
            None => Decision::Keep,
        };
        if matches!(&decision, Decision::Replace(_))
            && let Some(decision) = self.module_expression_decision(attribute)
        {
            self.apply(attribute.range, decision, !attribute.ctx.is_load());
            return TraversalSignal::Skip;
        }
        let unresolved_file = !has_bindings
            && receiver_module
                .and_then(|module| {
                    ModuleName::new(&format!("{}.{}", module.name(self.db), attribute.attr))
                })
                .is_some_and(|name| {
                    self.plan.rule(&name).is_some_and(|rule| {
                        matches!(rule.scope, RenameScope::File(_)) && rule.old_name == name
                    })
                });
        self.apply(
            attribute.attr.range,
            if unresolved_file && matches!(decision, Decision::Keep) {
                Decision::Unsupported
            } else {
                decision
            },
            !attribute.ctx.is_load(),
        );
        TraversalSignal::Traverse
    }

    fn is_implicitly_imported_submodule(&self, root: &ast::Expr, module: Module<'_>) -> bool {
        let ast::Expr::Name(name) = root else {
            return false;
        };
        let Some(load) = self.model.name_load(name) else {
            return false;
        };
        let summary = self.summarize_load(&load, |_| Ok(None));

        !summary.can_fall_through
            && summary.issue == ProviderIssue::None
            && !summary.definitions.is_empty()
            && summary.definitions.iter().all(|definition| {
                let DefinitionKind::Import(import) = definition.kind(self.db) else {
                    return false;
                };
                let parsed =
                    ruff_db::parsed::parsed_module(self.db, definition.python_file(self.db))
                        .load(self.db);
                let alias = import.alias(&parsed);

                alias.asname.is_none()
                    && ModuleName::new(alias.name.as_str())
                        .is_some_and(|imported| imported.starts_with(module.name(self.db)))
            })
    }

    fn module_expression_decision(&self, attribute: &ast::ExprAttribute) -> Option<Decision> {
        let module = module_from_type(self.model, attribute)?;
        let (rule, new) = self.plan.rewrite(self.db, module)?;
        if !matches!(rule.scope, RenameScope::File(_))
            || rule.old_name.parent() == rule.new_name.parent()
        {
            return None;
        }
        let mut root = &*attribute.value;
        while let ast::Expr::Attribute(nested) = root {
            root = &nested.value;
        }
        let ast::Expr::Name(root) = root else {
            return Some(Decision::Unsupported);
        };
        let root_module = module_from_type(self.model, root)?;
        let root_name = self
            .plan
            .rewrite(self.db, root_module)
            .map_or_else(|| root_module.name(self.db).clone(), |(_, name)| name);
        Some(if new == root_name {
            Decision::Replace(root.id.to_string())
        } else {
            new.relative_to(&root_name)
                .map_or(Decision::Unsupported, |suffix| {
                    Decision::Replace(format!("{}.{}", root.id, suffix.as_str()))
                })
        })
    }

    fn string(&mut self, string: &ast::ExprStringLiteral) {
        let Some((ast, model)) = self.model.enter_string_annotation(string) else {
            return;
        };
        let mut pass = SemanticPass {
            db: self.db,
            model: &model,
            plan: self.plan,
            changes: self.changes,
            edits: Vec::new(),
            augmented: None,
            known_omissions: false,
        };
        pass.visit_expr(ast.expr());
        self.known_omissions |= pass.known_omissions;
        self.edits.extend(pass.edits);
    }

    fn apply(&mut self, range: TextRange, decision: Decision, reject_change: bool) {
        match decision {
            Decision::Keep => {}
            Decision::Replace(_) if reject_change => {
                omit(UNSUPPORTED_SEMANTIC);
                self.known_omissions = true;
            }
            Decision::Replace(text) => self.edits.push((range, text)),
            Decision::Unsupported => {
                omit(UNSUPPORTED_SEMANTIC);
                self.known_omissions = true;
            }
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for SemanticPass<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        match node {
            AnyNodeRef::StmtAugAssign(assign) => self.augmented = Some(assign.target.range()),
            AnyNodeRef::ExprName(name) if self.plan.names.contains_key(name.id.as_str()) => {
                self.name(name);
            }
            AnyNodeRef::ExprAttribute(attribute)
                if self.plan.names.contains_key(attribute.attr.as_str()) =>
            {
                return self.attribute(attribute);
            }
            AnyNodeRef::ExprStringLiteral(string) => {
                self.string(string);
                return TraversalSignal::Skip;
            }
            _ => {}
        }
        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        if matches!(node, AnyNodeRef::StmtAugAssign(_)) {
            self.augmented = None;
        }
    }
}

#[derive(Default)]
struct ProviderSummary<'db> {
    replacement: Option<String>,
    definitions: Vec<Definition<'db>>,
    stable: bool,
    issue: ProviderIssue,
    can_fall_through: bool,
}

impl<'db> ProviderSummary<'db> {
    fn record_definition(
        &mut self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        replacement_for: &impl Fn(Definition<'db>) -> Result<Option<String>, ()>,
    ) {
        let definitions = user_visible_definitions(db, [definition]);
        if definitions.is_empty() {
            self.issue = self.issue.max(ProviderIssue::Unknown);
            return;
        }
        for definition in definitions {
            if !self.definitions.contains(&definition) {
                self.definitions.push(definition);
            }
            match replacement_for(definition) {
                Err(()) => self.issue = ProviderIssue::Unsupported,
                Ok(None) => self.stable = true,
                Ok(Some(replacement)) => match &self.replacement {
                    Some(known) if known != &replacement => {
                        self.issue = ProviderIssue::Unsupported;
                    }
                    Some(_) => {}
                    None => self.replacement = Some(replacement),
                },
            }
        }
    }

    fn merge(&mut self, other: Self) {
        self.stable |= other.stable;
        self.issue = self.issue.max(other.issue);
        for definition in other.definitions {
            if !self.definitions.contains(&definition) {
                self.definitions.push(definition);
            }
        }
        if let Some(replacement) = other.replacement {
            match &self.replacement {
                Some(known) if known != &replacement => {
                    self.issue = ProviderIssue::Unsupported;
                }
                Some(_) => {}
                None => self.replacement = Some(replacement),
            }
        }
    }

    fn has_provider(&self) -> bool {
        self.stable || self.issue == ProviderIssue::Unknown || self.replacement.is_some()
    }

    fn decision(self, affected: bool) -> Decision {
        if self.issue == ProviderIssue::Unsupported
            || (self.issue == ProviderIssue::Unknown && (affected || self.replacement.is_some()))
            || (self.stable && self.replacement.is_some())
        {
            return Decision::Unsupported;
        }
        match self.replacement {
            Some(replacement) if affected => Decision::Replace(replacement),
            Some(_) => Decision::Unsupported,
            None if affected && !self.stable => Decision::Unsupported,
            None => Decision::Keep,
        }
    }
}

#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
enum ProviderIssue {
    #[default]
    None,
    Unknown,
    Unsupported,
}

enum Decision {
    Keep,
    Replace(String),
    Unsupported,
}

fn replace(old: &str, new: &str) -> Decision {
    if old != new {
        return Decision::Replace(new.to_string());
    }
    Decision::Keep
}

fn module_from_type<'db, T: HasType>(
    model: &SemanticModel<'db>,
    expression: &T,
) -> Option<Module<'db>> {
    let Type::ModuleLiteral(literal) = expression.inferred_type(model)? else {
        return None;
    };
    Some(literal.module(model.db()))
}

fn module_attribute_bindings<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    attribute: &ast::ExprAttribute,
) -> Option<BindingWithConstraintsIterator<'db, 'db>> {
    let Type::ModuleLiteral(module) = attribute.value.inferred_type(model)? else {
        return None;
    };
    let file = module.module(db).file(db)?;
    let file = ProgramFile::new(db, file, model.program_file().program(db));
    let scope = global_scope(db, file);
    let symbol = place_table(db, scope).symbol_id(attribute.attr.as_str())?;
    Some(use_def_map(db, scope).end_of_scope_symbol_bindings(symbol))
}

fn implicit_import_name<'a>(
    db: &dyn Db,
    definition: Definition<'_>,
    new: &'a ModuleName,
) -> Option<&'a str> {
    let module = ruff_db::parsed::parsed_module(db, definition.python_file(db)).load(db);
    match definition.kind(db) {
        DefinitionKind::Import(import) if import.alias(&module).asname.is_none() => {
            Some(new.first_component())
        }
        DefinitionKind::ImportFrom(import) if import.alias(&module).asname.is_none() => {
            Some(new.last_component())
        }
        _ => None,
    }
}

fn normalize(mut edits: Vec<FileRenameEdit>) -> Option<Vec<FileRenameEdit>> {
    edits.sort_unstable_by_key(|edit| (edit.range.file(), edit.range.start(), edit.range.end()));
    edits.dedup();
    (!edits.windows(2).any(|edits| {
        edits[0].range.file() == edits[1].range.file()
            && edits[1].range.start() < edits[0].range.end()
    }))
    .then_some(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::Db as _;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::PythonVersion;
    use std::collections::BTreeSet;
    use ty_module_resolver::{FallibleStrategy, SearchPathSettings};
    use ty_project::{ProjectMetadata, TestDb};
    use ty_python_core::platform::PythonPlatform;
    use ty_python_core::program::ProgramSettings;
    use ty_python_semantic::PythonVersionWithSource;

    #[test]
    fn file_rename_contract() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            ("/old.py", "class C: ...\n"),
            (
                "/direct.py",
                "import pkg.old\nvalue: 'pkg.old.C'\nprint(pkg.old.C)\n",
            ),
            (
                "/from.py",
                "from typing import Literal\nfrom pkg import old\nvalue: 'old.C'\nbad: \"old.C[\"\nliteral: Literal[\"old.C[\"]\nother: 'threshold['\nroot: 'pkg.Unrelated['\ndef f(): return old.C\nclass C: value = old.C\ntotal = 0\ntotal += old.C.value\ndef assigned(flag):\n if flag: old = 1\n return old\ndef comp(): return [old for old in ()]\n",
            ),
            ("/alias.py", "from pkg import old as old\nprint(old.C)\n"),
            ("/facade.py", "from pkg import old\n"),
            (
                "/loop.py",
                "import pkg.old\nfor old in [pkg.old.C]: print(old)\n",
            ),
            (
                "/e.py",
                "old\nif x: from pkg import old\nelse: import old\n",
            ),
            (
                "/targets.py",
                "from pkg import old\n(old := old)\nfor old in [1]: pass\nvalues = [0 for old in [1]]\n",
            ),
            (
                "/types.py",
                "import pkg.old\ntype Alias[T: pkg.old.C] = list[pkg.old.C]\n",
            ),
            (
                "/comment.py",
                "from pkg import old\nx = None  # type: old.C\n",
            ),
            (
                "/invalid.py",
                "from typing import Callable, Concatenate\nfrom pkg import old\nx: \"old.C[\"\ny: list[\"old.C[\"]\nz: Callable[\"old.C[\", int]\nw: Callable[Concatenate[int, \"old.C[\"], int]\n",
            ),
            (
                "/exports.py",
                "from pkg import old\n__all__ = ['old']\ncomputed = old.__name__\n",
            ),
            (
                "/class_fallback.py",
                "def f():\n from pkg import old\n class C:\n  print(old.C)\n  old = 1\n old = 2\n",
            ),
        ]);
        assert_success(
            &db,
            &[
                file("/pkg/old.py", "/pkg/new.py"),
                file("/old.py", "/other.py"),
            ],
            &[
                (
                    "/direct.py",
                    "import pkg.new\nvalue: 'pkg.new.C'\nprint(pkg.new.C)\n",
                ),
                (
                    "/from.py",
                    "from typing import Literal\nfrom pkg import new\nvalue: 'new.C'\nbad: \"old.C[\"\nliteral: Literal[\"old.C[\"]\nother: 'threshold['\nroot: 'pkg.Unrelated['\ndef f(): return new.C\nclass C: value = new.C\ntotal = 0\ntotal += new.C.value\ndef assigned(flag):\n if flag: old = 1\n return old\ndef comp(): return [old for old in ()]\n",
                ),
                ("/alias.py", "from pkg import new as old\nprint(old.C)\n"),
                ("/facade.py", "from pkg import new\n"),
                (
                    "/loop.py",
                    "import pkg.new\nfor old in [pkg.new.C]: print(old)\n",
                ),
                (
                    "/e.py",
                    "old\nif x: from pkg import new\nelse: import other\n",
                ),
                (
                    "/targets.py",
                    "from pkg import new\n(old := new)\nfor old in [1]: pass\nvalues = [0 for old in [1]]\n",
                ),
                (
                    "/types.py",
                    "import pkg.new\ntype Alias[T: pkg.new.C] = list[pkg.new.C]\n",
                ),
                (
                    "/comment.py",
                    "from pkg import new\nx = None  # type: old.C\n",
                ),
                (
                    "/invalid.py",
                    "from typing import Callable, Concatenate\nfrom pkg import new\nx: \"old.C[\"\ny: list[\"old.C[\"]\nz: Callable[\"old.C[\", int]\nw: Callable[Concatenate[int, \"old.C[\"], int]\n",
                ),
                (
                    "/exports.py",
                    "from pkg import new\n__all__ = ['old']\ncomputed = new.__name__\n",
                ),
                (
                    "/class_fallback.py",
                    "def f():\n from pkg import new\n class C:\n  print(old.C)\n  old = 1\n old = 2\n",
                ),
            ],
        );
    }

    #[test]
    fn unicode_identifier_prefilter() {
        let db = test_db(&[
            ("/K·b.py", "class C: ...\n"),
            ("/use.py", "import \u{212a}·b\nprint(\u{212a}·b.C)\n"),
        ]);
        assert_success(
            &db,
            &[file("/K·b.py", "/new.py")],
            &[("/use.py", "import new\nprint(new.C)\n")],
        );
    }

    #[test]
    fn aliases_and_declarations_remain_stable() {
        let mut db = test_db(&[
            ("/pkg/__init__.py", "from . import old as old\n"),
            ("/pkg/old.py", "class C: ...\n"),
            (
                "/use.py",
                "import pkg, decl\nprint(pkg.old.C, decl.old.C)\n",
            ),
            (
                "/decl.py",
                "import pkg.old as source\nold = source\ndef outer():\n from pkg import old\n print(old.C)\n",
            ),
        ]);
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[
                ("/pkg/__init__.py", "from . import new as old\n"),
                (
                    "/decl.py",
                    "import pkg.new as source\nold = source\ndef outer():\n from pkg import new\n print(new.C)\n",
                ),
            ],
        );
        let mixed = "if input(): from . import old as old\nelse: from . import old\n";
        db.write_file("/pkg/__init__.py", mixed).unwrap();
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[
                (
                    "/pkg/__init__.py",
                    "if input(): from . import new as old\nelse: from . import new\n",
                ),
                (
                    "/decl.py",
                    "import pkg.new as source\nold = source\ndef outer():\n from pkg import new\n print(new.C)\n",
                ),
            ],
        );
    }

    #[test]
    fn representable_cross_parent_file_move() {
        let db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/x.py", ""),
            ("/a/one/__init__.py", ""),
            ("/a/one/old.py", "from .. import one\nclass C: ...\n"),
            (
                "/use.py",
                "import a.one.old\nprint(a.one.old.C)\nfrom a.one import old\nprint(old.C)\nfrom a import x\nfor x in [old.C]: print(x)\n",
            ),
            (
                "/aliased.py",
                "import a.one.old as stable\nprint(stable.C)\n",
            ),
        ]);
        assert_success(
            &db,
            &[
                file("/a/one/old.py", "/a/two/new.py"),
                file("/a/x.py", "/a/y.py"),
            ],
            &[
                (
                    "/use.py",
                    "import a.two.new\nprint(a.two.new.C)\nfrom a.two import new\nprint(new.C)\nfrom a import y\nfor x in [new.C]: print(x)\n",
                ),
                (
                    "/aliased.py",
                    "import a.two.new as stable\nprint(stable.C)\n",
                ),
            ],
        );
    }

    #[test]
    fn root_module_can_move_to_submodule() {
        let db = test_db(&[
            ("/a.py", "class C: ...\n"),
            ("/a/placeholder.txt", ""),
            ("/use.py", "import a\nprint(a.C, a)\n"),
        ]);
        assert_success(
            &db,
            &[file("/a.py", "/a/new.py")],
            &[("/use.py", "import a.new\nprint(a.new.C, a.new)\n")],
        );
    }

    #[test]
    fn module_expression_can_collapse_to_root() {
        let db = test_db(&[
            ("/a/old.py", "class C: ...\n"),
            ("/use.py", "import a.old\nprint(a.old.C)\n"),
        ]);
        assert_success(
            &db,
            &[file("/a/old.py", "/a.py")],
            &[("/use.py", "import a\nprint(a.C)\n")],
        );
    }

    #[test]
    fn shadowed_destination_is_omitted() {
        let mut db = TestDb::new(ProjectMetadata::new("test", "/src".into()));
        db.write_files([
            ("/src/old.py", "class C: ...\n"),
            ("/src/use.py", "import old\nprint(old.C)\n"),
            ("/extra/new.py", "class Other: ...\n"),
        ])
        .unwrap();
        let search_paths = SearchPathSettings {
            extra_paths: vec!["/extra".into()],
            ..SearchPathSettings::new(vec!["/src".into()])
        }
        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
        .unwrap();
        db.project().update_program(
            &mut db,
            ProgramSettings {
                python_version: PythonVersionWithSource::default(),
                python_platform: PythonPlatform::default(),
                search_paths,
            },
        );

        let result = will_rename_paths(
            &db,
            &[file("/src/old.py", "/src/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(result.has_known_omissions());
        assert!(result.into_edits().is_empty());
    }

    #[test]
    fn blocked_destination_ancestor_is_omitted() {
        let db = test_db(&[
            ("/x.py", "class C: ...\n"),
            ("/a.py", ""),
            ("/a/placeholder.txt", ""),
            ("/use.py", "import x as stable\nprint(stable.C)\n"),
        ]);
        let result = will_rename_paths(
            &db,
            &[file("/x.py", "/a/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(result.has_known_omissions());
        assert!(result.into_edits().is_empty());
    }

    #[test]
    fn conflicting_destination_modules_are_omitted() {
        let db = test_db(&[
            ("/x.py", "class C: ...\n"),
            ("/y.pyi", "class D: ...\n"),
            ("/use.py", "import x\nimport y\nprint(x.C, y.D)\n"),
        ]);
        let result = will_rename_paths(
            &db,
            &[file("/x.py", "/z.py"), file("/y.pyi", "/z.pyi")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(result.has_known_omissions());
        assert!(result.into_edits().is_empty());
    }

    #[test]
    fn regular_package_and_runtime_provenance() {
        let db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/p/__init__.py", ""),
            ("/a/p/__init__.pyi", ""),
            ("/a/p/mod.py", "from . import h\nfrom ..p import h\n"),
            ("/a/p/h.py", ""),
            ("/use.py", "import a.p.mod\nfrom a.p import *\na.p.mod\n"),
        ]);
        assert_success(
            &db,
            &[PathRename::directory("/a/p".into(), "/a/n".into())],
            &[
                ("/a/p/mod.py", "from . import h\nfrom ..n import h\n"),
                ("/use.py", "import a.n.mod\nfrom a.n import *\na.n.mod\n"),
            ],
        );

        let shadowed = test_db(&[
            ("/p/__init__.py", ""),
            ("/p/__init__.pyi", ""),
            ("/p/old.py", "class C: ...\n"),
            ("/p/old.pyi", "class C: ...\n"),
            ("/use.py", "import p.old\nprint(p.old.C)\n"),
        ]);
        assert_file_no_edits(&shadowed, "/p/old.pyi", "/p/new.pyi");
        assert_success(
            &shadowed,
            &[file("/p/old.py", "/p/new.py")],
            &[("/use.py", "import p.new\nprint(p.new.C)\n")],
        );
    }

    #[test]
    fn unrepresentable_module_attributes_are_omitted() {
        for (name, initializer) in [
            (
                "loop-carried alias",
                "from . import old as old\nwhile flag:\n from . import old as old\n",
            ),
            (
                "nested global alias",
                "from . import old as old\ndef refresh():\n global old\n from . import old as old\n",
            ),
        ] {
            let consumer = "import pkg\nprint(pkg.old.C)\n";
            let db = test_db(&[
                ("/pkg/__init__.py", initializer),
                ("/pkg/old.py", "class C: ...\n"),
                ("/use.py", consumer),
            ]);
            let result = will_rename_paths(
                &db,
                &[file("/pkg/old.py", "/pkg/new.py")],
                &db.project().files(&db),
                |_| true,
            );

            assert!(result.has_known_omissions(), "{name}");
            assert_eq!(
                apply_edits(&db, &result.into_edits(), "/use.py"),
                consumer,
                "{name}"
            );
        }
    }

    #[test]
    fn conflicting_runtime_and_stub_aliases_are_omitted() {
        for (name, runtime, stub, expected_runtime, expected_stub) in [
            (
                "runtime alias is explicit",
                "from . import old as old\n",
                "from . import old\n",
                "from . import new as old\n",
                "from . import new\n",
            ),
            (
                "stub alias is explicit",
                "from . import old\n",
                "from . import old as old\n",
                "from . import new\n",
                "from . import new as old\n",
            ),
        ] {
            let consumer = "import pkg\nprint(pkg.old.C)\n";
            let db = test_db(&[
                ("/pkg/__init__.py", runtime),
                ("/pkg/__init__.pyi", stub),
                ("/pkg/old.py", "class C: ...\n"),
                ("/use.py", consumer),
            ]);
            let result = will_rename_paths(
                &db,
                &[file("/pkg/old.py", "/pkg/new.py")],
                &db.project().files(&db),
                |_| true,
            );

            assert!(result.has_known_omissions(), "{name}");
            let edits = result.into_edits();
            assert_eq!(apply_edits(&db, &edits, "/use.py"), consumer, "{name}");
            assert_eq!(
                apply_edits(&db, &edits, "/pkg/__init__.py"),
                expected_runtime,
                "{name}"
            );
            assert_eq!(
                apply_edits(&db, &edits, "/pkg/__init__.pyi"),
                expected_stub,
                "{name}"
            );
        }
    }

    #[test]
    fn unsupported_semantics_are_omitted() {
        for (name, source) in [
            ("R2-03", "from facade import old\nold.C\n"),
            ("R8-02 qualified store", "import pkg.old\npkg.old = 1\n"),
            ("R8-03 qualified delete", "import pkg.old\ndel pkg.old\n"),
            ("self assignment", "import pkg.old\npkg.old=pkg.old\n"),
            (
                "assignment-backed class attribute",
                "import old as source\nclass C: old = source\nC.old.C\n",
            ),
            ("global", "def f():\n global old\n import old\n old\n"),
            ("changed augmented", "import old\nold += 1\n"),
            ("star propagation", "from facade import *\nprint(old.C)\n"),
            ("deferred import", "def f():\n old.x\n import old\n"),
            (
                "stale package load",
                "from pkg import old\nimport pkg\npkg.old.C\n",
            ),
            (
                "deleted exception target",
                "from pkg import old\ntry: 1 / int(input())\nexcept Exception as old: pass\nprint(old)\n",
            ),
            (
                "conditional stable deletion",
                "from pkg import old\nif flag:\n old = 1\n del old\nprint(old)\n",
            ),
            (
                "stable local deletion",
                "import old\ndef f(old):\n del old\n print(old)\n",
            ),
        ] {
            let db = test_db(&[
                ("/pkg/__init__.py", ""),
                ("/pkg/old.py", ""),
                ("/old.py", ""),
                ("/facade.py", "from pkg import old\n"),
                ("/main.py", source),
            ]);
            let package = source.contains("pkg") || source.contains("facade");
            let (rename, mut expected) = if package {
                (
                    file("/pkg/old.py", "/pkg/new.py"),
                    source
                        .replace("import pkg.old", "import pkg.new")
                        .replace("from pkg import old", "from pkg import new"),
                )
            } else {
                (
                    file("/old.py", "/new.py"),
                    source.replace("import old", "import new"),
                )
            };
            if name == "self assignment" {
                expected = expected.replace("pkg.old=pkg.old", "pkg.old=pkg.new");
            }
            let mut expected_files = Vec::new();
            if source != expected.as_str() {
                expected_files.push(("/main.py", expected.as_str()));
            }
            if package {
                expected_files.push(("/facade.py", "from pkg import new\n"));
            }
            assert_success_named(name, &db, &[rename], &expected_files);
        }
    }

    #[test]
    fn agreeing_place_load_providers_are_rewritten() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            (
                "/use.py",
                "from pkg import old\nclass C:\n print(old.C)\n from pkg import old\ndef outer():\n def inner(): return old.C\n from pkg import old\n return inner\ndef conditional(flag):\n if flag: from pkg import old\n return old.C\n",
            ),
        ]);
        let result = will_rename_paths(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        let has_known_omissions = result.has_known_omissions();
        assert_eq!(
            apply_edits(&db, &result.into_edits(), "/use.py"),
            "from pkg import new\nclass C:\n print(new.C)\n from pkg import new\ndef outer():\n def inner(): return new.C\n from pkg import new\n return inner\ndef conditional(flag):\n if flag: from pkg import new\n return new.C\n"
        );
        assert!(!has_known_omissions);
    }

    #[test]
    fn qualifiers_beneath_attribute_mutations_are_rewritten() {
        let db = test_db(&[
            ("/old.py", ""),
            (
                "/use.py",
                "import old\nold.VALUE += 1\nold.VALUE = 1\ndel old.VALUE\n",
            ),
        ]);
        let result = will_rename_paths(
            &db,
            &[file("/old.py", "/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(!result.has_known_omissions());
        assert_eq!(
            apply_edits(&db, &result.into_edits(), "/use.py"),
            "import new\nnew.VALUE += 1\nnew.VALUE = 1\ndel new.VALUE\n"
        );
    }

    #[test]
    fn postponed_annotations_use_end_of_scope_bindings() {
        let db = test_db(&[
            ("/other.py", "class C: ...\n"),
            ("/pkg/__init__.py", ""),
            ("/pkg/target.py", "class C: ...\n"),
            (
                "/use.pyi",
                "import other as target\nvalue: target.C\nfrom pkg import target\n",
            ),
        ]);
        let result = will_rename_paths(
            &db,
            &[file("/pkg/target.py", "/pkg/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(result.has_known_omissions());
        assert_eq!(
            apply_edits(&db, &result.into_edits(), "/use.pyi"),
            "import other as target\nvalue: target.C\nfrom pkg import new\n"
        );
    }

    #[test]
    fn scope_declarations_omit_only_dependent_occurrences() {
        let db = test_db(&[
            ("/old.py", "class C: ...\n"),
            ("/other.py", "class C: ...\n"),
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            (
                "/use.py",
                "import old\nimport other\nimport pkg.old\nprint(old.C, other.C, pkg.old.C)\ndef global_use():\n global old, pkg\n def nested(): return old.C, other.C, pkg.old.C\n return old.C, other.C, pkg.old.C, nested\ndef outer():\n import old\n import pkg.old\n def nonlocal_use():\n  nonlocal old, pkg\n  def nested(): return old.C, other.C, pkg.old.C\n  return old.C, other.C, pkg.old.C, nested\n return old.C, pkg.old.C, nonlocal_use\ndef sibling(): return old.C, other.C, pkg.old.C\ndef annotated():\n value: \"other.C\"\n",
            ),
        ]);
        assert_success(
            &db,
            &[
                file("/old.py", "/new.py"),
                file("/other.py", "/renamed.py"),
                file("/pkg/old.py", "/pkg/new.py"),
            ],
            &[(
                "/use.py",
                "import new\nimport renamed\nimport pkg.new\nprint(new.C, renamed.C, pkg.new.C)\ndef global_use():\n global old, pkg\n def nested(): return old.C, renamed.C, pkg.new.C\n return old.C, renamed.C, pkg.new.C, nested\ndef outer():\n import new\n import pkg.new\n def nonlocal_use():\n  nonlocal old, pkg\n  def nested(): return old.C, renamed.C, pkg.new.C\n  return old.C, renamed.C, pkg.new.C, nested\n return new.C, pkg.new.C, nonlocal_use\ndef sibling(): return new.C, renamed.C, pkg.new.C\ndef annotated():\n value: \"renamed.C\"\n",
            )],
        );
    }

    #[test]
    fn unsupported_requests_and_imports_are_omitted() {
        let mut db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/old.py", ""),
            ("/b/__init__.py", ""),
            ("/use.py", "from a import old, sibling\n"),
        ]);
        assert_file_no_edits(&db, "/a/old.py", "/b/new.py");
        db.write_file("/use.py", "import a.old\n").unwrap();
        assert_file_no_edits(&db, "/a/old.py", "/b/new.py");
        db.write_file("/use.py", "import a.old.missing\n").unwrap();
        assert_file_no_edits(&db, "/a/old.py", "/a/new.py");
        assert_file_no_edits(&db, "/a/__init__.py", "/a/new.py");
        assert_no_edits(
            "R10-04 unresolved package alias",
            &test_db(&[
                ("/old/__init__.py", ""),
                ("/use.py", "from old import missing\n"),
            ]),
            &[PathRename::directory("/old".into(), "/new".into())],
        );
        assert_no_edits(
            "mixed relative aliases",
            &test_db(&[
                ("/a/__init__.py", ""),
                ("/a/one/__init__.py", ""),
                ("/a/one/old.py", "from . import helper, stable\n"),
                ("/a/one/helper.py", ""),
                ("/a/one/stable.py", ""),
            ]),
            &[
                file("/a/one/old.py", "/a/two/new.py"),
                file("/a/one/helper.py", "/a/two/helper.py"),
            ],
        );
    }

    #[test]
    fn import_statements_are_coherent_units() {
        let db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/old.py", ""),
            ("/a/x.py", ""),
            ("/b/__init__.py", ""),
            (
                "/use.py",
                "from a import old, sibling\nfrom a import x\nprint(old, x)\n",
            ),
        ]);
        assert_success(
            &db,
            &[file("/a/old.py", "/b/new.py"), file("/a/x.py", "/a/y.py")],
            &[(
                "/use.py",
                "from a import old, sibling\nfrom a import y\nprint(old, y)\n",
            )],
        );
    }

    #[test]
    fn reports_known_omissions() {
        let mut db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/old.py", ""),
            ("/b/__init__.py", ""),
            (
                "/use.py",
                "from a import old\nx = None  # type: old.C\ndef discard(old):\n del old\n",
            ),
        ]);
        let complete = will_rename_paths(
            &db,
            &[file("/a/old.py", "/a/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(!complete.has_known_omissions());
        assert_eq!(
            apply_edits(&db, &complete.into_edits(), "/use.py"),
            "from a import new\nx = None  # type: old.C\ndef discard(old):\n del old\n"
        );

        db.write_file("/use.py", "from a import old, sibling\n")
            .unwrap();
        let incomplete = will_rename_paths(
            &db,
            &[file("/a/old.py", "/b/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(incomplete.has_known_omissions());

        db.write_file("/use.py", "import a.old\na.old = None\n")
            .unwrap();
        let unsupported_use = will_rename_paths(
            &db,
            &[file("/a/old.py", "/a/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(unsupported_use.has_known_omissions());

        db.write_file("/use.py", "from a import old\ndel old\n")
            .unwrap();
        let deleted_import = will_rename_paths(
            &db,
            &[file("/a/old.py", "/a/new.py")],
            &db.project().files(&db),
            |_| true,
        );
        assert!(deleted_import.has_known_omissions());
    }

    #[test]
    fn best_effort_request_contract() {
        let mut db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/x.py", ""),
            ("/a/o/__init__.py", ""),
            ("/a/o/old.py", ""),
            ("/a/one/__init__.py", ""),
            ("/a/one/old.py", "from .. import x\n"),
            ("/b/__init__.py", ""),
            ("/b/x.py", ""),
            ("/ns/mod.py", ""),
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/pkg/old.pyi", ""),
            ("/x.py", ""),
            (
                "/q.py",
                "from a.o import old\nfrom a import o\no.old\nif flag: from a import x\nelse: from b import x\n",
            ),
            ("/u.py", "import q\nq.x\n"),
            ("/independent.py", "import x\nx.VALUE\n"),
        ]);
        assert_success(&db, &[file("/pkg/old.py", "/pkg/new.py")], &[]);
        assert_success(
            &db,
            &[file("/a/o/old.py", "/a/new.py")],
            &[(
                "/q.py",
                "from a import new\nfrom a import o\no.old\nif flag: from a import x\nelse: from b import x\n",
            )],
        );
        let conflicts = || vec![file("/a/x.py", "/a/y.py"), file("/b/x.py", "/b/z.py")];
        assert_success(
            &db,
            &conflicts(),
            &[
                ("/a/one/old.py", "from .. import y\n"),
                (
                    "/q.py",
                    "from a.o import old\nfrom a import o\no.old\nif flag: from a import y\nelse: from b import z\n",
                ),
            ],
        );
        db.write_file("/u.py", "if q:from a import x\nelse: from b import x\nx")
            .unwrap();
        assert_no_edits(
            "namespace package",
            &db,
            &[PathRename::directory("/ns".into(), "/newns".into())],
        );
        assert_no_edits(
            "relative rebasing",
            &db,
            &[file("/a/one/old.py", "/b/new.py")],
        );
        assert_success(
            &db,
            &conflicts(),
            &[
                ("/a/one/old.py", "from .. import y\n"),
                (
                    "/q.py",
                    "from a.o import old\nfrom a import o\no.old\nif flag: from a import y\nelse: from b import z\n",
                ),
                ("/u.py", "if q:from a import y\nelse: from b import z\nx"),
            ],
        );
        assert_no_edits("extension change", &db, &[file("/x.py", "/x.pyi")]);
        assert_success(
            &db,
            &[
                file("/pkg/old.py", "/pkg/new.py"),
                file("/pkg/old.pyi", "/pkg/new.pyi"),
                file("/x.py", "/y.py"),
            ],
            &[("/independent.py", "import y\ny.VALUE\n")],
        );
        assert_success(
            &db,
            &[
                PathRename::directory("/pkg".into(), "/newpkg".into()),
                file("/pkg/old.py", "/elsewhere.py"),
                file("/x.py", "/y.py"),
            ],
            &[("/independent.py", "import y\ny.VALUE\n")],
        );
    }

    fn file(old: &str, new: &str) -> PathRename {
        PathRename::file(old.into(), new.into())
    }

    fn assert_file_no_edits(db: &TestDb, old: &str, new: &str) {
        assert_no_edits(old, db, &[file(old, new)]);
    }

    fn assert_success(db: &TestDb, renames: &[PathRename], expected: &[(&str, &str)]) {
        assert_success_named("rename", db, renames, expected);
    }

    fn assert_success_named(
        name: &str,
        db: &TestDb,
        renames: &[PathRename],
        expected: &[(&str, &str)],
    ) {
        let edits = will_rename_paths(db, renames, &db.project().files(db), |_| true).into_edits();
        let actual: BTreeSet<_> = edits.iter().map(|edit| edit.range.file()).collect();
        let expected_files: BTreeSet<_> = expected
            .iter()
            .map(|(path, _)| system_path_to_file(db, *path).unwrap())
            .collect();
        assert_eq!(actual, expected_files, "{name}");
        for &(path, contents) in expected {
            assert_eq!(apply_edits(db, &edits, path), contents, "{name}: {path}");
        }
    }

    fn assert_no_edits(name: &str, db: &TestDb, renames: &[PathRename]) {
        assert!(
            will_rename_paths(db, renames, &db.project().files(db), |_| true)
                .into_edits()
                .is_empty(),
            "{name}"
        );
    }

    fn test_db(files: &[(&str, &str)]) -> TestDb {
        let mut db = TestDb::new(ProjectMetadata::new("test", "/".into()));
        db.set_python_version(PythonVersion::latest_ty());
        db.write_files(files.iter().copied()).unwrap();
        db
    }

    fn apply_edits(db: &TestDb, edits: &[FileRenameEdit], path: &str) -> String {
        let file = system_path_to_file(db, path).unwrap();
        let mut edits: Vec<_> = edits
            .iter()
            .filter(|edit| edit.range.file() == file)
            .collect();
        edits.sort_unstable_by_key(|edit| std::cmp::Reverse(edit.range.start()));
        let mut result = source_text(db, file).as_str().to_owned();
        for edit in edits {
            result.replace_range(edit.range.range().to_std_range(), &edit.value);
        }
        result
    }
}
