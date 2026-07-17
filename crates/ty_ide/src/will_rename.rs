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
//! qualified writes or deletes, `global`- or `nonlocal`-dependent occurrences, and references
//! reached only through a star-import export chain. The path of a direct star import may still be
//! rewritten, and independent imports and occurrences remain eligible for edits.
//!
//! Coincidental text is not a reference. The feature does not use `__all__`, dynamic lookup, or
//! runtime strings to discover affected modules. Valid single-literal forward annotations follow
//! ordinary semantic rules; implicitly concatenated or malformed annotations and legacy type
//! comments are ignored. A source that cannot be read or whose generated edits conflict is omitted
//! without suppressing edits for other sources.

use crate::RangedValue;
use rayon::prelude::*;
use ruff_db::files::{File, FileRange, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_python_trivia::is_identifier_continuation;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;
use ty_module_resolver::{
    Module, ModuleName, ModuleResolveMode, file_to_module, is_legacy_namespace_package,
    resolve_module_confident, resolve_real_module_confident, search_paths,
};
use ty_project::{Db, parallel::ParallelIteratorExt};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_semantic::types::Type;
use ty_python_semantic::{
    HasType, ImportAliasResolution, LiveBindings, ResolvedDefinition, SemanticModel,
    definitions_for_imported_symbol,
};
use unicode_normalization::UnicodeNormalization;

/// Computes normalized source edits for a batch of filesystem renames.
///
/// `files` is the candidate set discovered by the caller. Files outside `in_scope` are skipped.
/// Directly renamed files are added automatically when they are in scope; directory contents are
/// not discovered here and must be included in `files` by the caller.
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
                let name = file_to_module(db, file)?.name(db).clone();
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
                    && !inits
                        .iter()
                        .any(|file| is_legacy_namespace_package(db, *file)))
                .then_some(())?;
                let name = inits
                    .iter()
                    .find_map(|file| file_to_module(db, *file))?
                    .name(db)
                    .clone();
                if [
                    resolve_module_confident(db, &name).and_then(|module| module.file(db)),
                    resolve_real_module_confident(db, &name).and_then(|module| module.file(db)),
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
        let new_name = prospective_name(db, &new)?;
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

fn prospective_name(db: &dyn Db, path: &SystemPath) -> Option<ModuleName> {
    search_paths(db, ModuleResolveMode::Typing)
        .filter(|search_path| !search_path.is_standard_library())
        .find_map(|search_path| search_path.module_name_for_system_path(path))
}

fn resolved_source(db: &dyn Db, name: &ModuleName) -> Option<File> {
    resolve_real_module_confident(db, name)
        .or_else(|| resolve_module_confident(db, name))?
        .file(db)
}

fn file_within(db: &dyn Db, file: File, root: &SystemPath) -> bool {
    matches!(file.path(db).as_system_path(), Some(path) if path.starts_with(root))
}

fn edits_for_file(db: &dyn Db, file: File, plan: &RenamePlan) -> WillRenameResult {
    let moved_source = file_to_module(db, file)
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
    let module = ruff_db::parsed::parsed_module(db, file).load(db);
    let root = AnyNodeRef::from(module.syntax());
    let model = SemanticModel::new(db, file);
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
        mutation_target_depth: 0,
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
        let old_package = ModuleName::package_for_file(db, file).ok();
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
    range: TextRange,
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
        let definition = ty_python_core::semantic_index(self.db, self.model.file())
            .expect_single_definition(alias);
        output.changes.insert(
            definition,
            BindingChange {
                new: new.to_string(),
                range: alias.range,
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
            self.record(
                &mut output,
                alias,
                old.first_component(),
                new.first_component(),
            );
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
        let Ok(old_parent) = ModuleName::from_import_statement(self.db, self.model.file(), import)
        else {
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
            matches!(definition, ResolvedDefinition::Module(file) if file_to_module(db, *file).is_some_and(|resolved| resolved.name(db) == module.name(db)))
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
    mutation_target_depth: usize,
    known_omissions: bool,
}

impl SemanticPass<'_, '_> {
    fn name(&mut self, name: &ast::ExprName) {
        if name.ctx.is_del() {
            if self.plan.terminal(name.id.as_str()) {
                omit(UNSUPPORTED_SEMANTIC);
                self.known_omissions = true;
            }
            return;
        }
        if name.ctx.is_store() && self.augmented != Some(name.range) {
            return;
        }
        let bindings = self.model.name_use_bindings(name);
        let affected = module_from_type(self.model, name)
            .is_some_and(|module| self.plan.rewrite(self.db, module).is_some());
        let module_scope = self
            .model
            .scope(name.into())
            .is_some_and(ty_python_core::FileScopeId::is_global);
        let decision = match bindings.as_ref() {
            Some(bindings) => self.binding_decision(bindings, affected, |definition| {
                let Some(change) = self.changes.get(&definition) else {
                    return Ok(None);
                };
                if change.range.start() > name.range.start() && !module_scope {
                    return Err(());
                }
                Ok(Some(change.new.clone()))
            }),
            None if affected => Decision::Unsupported,
            None => Decision::Keep,
        };
        self.apply(name.range, decision, name.ctx.is_store());
    }

    fn binding_decision(
        &self,
        bindings: &LiveBindings<'_>,
        affected: bool,
        replacement_for: impl Fn(Definition<'_>) -> Result<Option<String>, ()>,
    ) -> Decision {
        if affected && bindings.depends_on_global_or_nonlocal {
            return Decision::Unsupported;
        }
        let mut replacement = None;
        let mut stable = false;
        for definition in &bindings.definitions {
            if affected && matches!(definition.kind(self.db), DefinitionKind::StarImport(_)) {
                return Decision::Unsupported;
            }
            let Ok(change) = replacement_for(*definition) else {
                return Decision::Unsupported;
            };
            let Some(change) = change else {
                stable = true;
                continue;
            };
            if replacement.as_ref().is_some_and(|known| known != &change) {
                return Decision::Unsupported;
            }
            replacement = Some(change);
        }
        let unavailable = bindings.definitions.is_empty() || bindings.may_be_deleted;
        match replacement {
            Some(_) if stable || bindings.may_be_deleted => Decision::Unsupported,
            Some(_) if !affected => Decision::Unsupported,
            Some(replacement) => Decision::Replace(replacement),
            None if affected && unavailable => Decision::Unsupported,
            None => Decision::Keep,
        }
    }

    fn attribute(&mut self, attribute: &ast::ExprAttribute) -> TraversalSignal {
        let rewrite = module_from_type(self.model, attribute)
            .and_then(|module| self.plan.rewrite(self.db, module));
        let receiver_module = module_from_type(self.model, &*attribute.value);
        let bindings = self.model.module_attribute_bindings(attribute);
        let mut root = &*attribute.value;
        while let ast::Expr::Attribute(attribute) = root {
            root = &attribute.value;
        }
        let declaration_dependent = rewrite.is_some()
            && match root {
                ast::Expr::Name(name) => self
                    .model
                    .name_use_bindings(name)
                    .is_some_and(|bindings| bindings.depends_on_global_or_nonlocal),
                _ => false,
            };
        let decision = match bindings.as_ref() {
            _ if declaration_dependent => Decision::Unsupported,
            Some(bindings) => self.binding_decision(bindings, rewrite.is_some(), |definition| {
                let Type::ModuleLiteral(module) = self.model.binding_type(definition) else {
                    return Err(());
                };
                Ok(self
                    .plan
                    .rewrite(self.db, module.module(self.db))
                    .and_then(|(_, new)| {
                        implicit_import_name(self.db, definition, &new).map(str::to_string)
                    }))
            }),
            None if receiver_module.is_some() => {
                rewrite.as_ref().map_or(Decision::Keep, |(_, name)| {
                    replace(attribute.attr.as_str(), name.last_component())
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
        let unresolved_file = bindings.is_none()
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
        Some(
            new.relative_to(&root_name)
                .map_or(Decision::Unsupported, |suffix| {
                    Decision::Replace(format!("{}.{}", root.id, suffix.as_str()))
                }),
        )
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
            mutation_target_depth: 0,
            known_omissions: false,
        };
        pass.visit_expr(ast.expr());
        self.known_omissions |= pass.known_omissions;
        self.edits.extend(pass.edits);
    }

    fn apply(&mut self, range: TextRange, decision: Decision, reject_change: bool) {
        match decision {
            Decision::Keep => {}
            Decision::Replace(_) if reject_change || self.mutation_target_depth > 0 => {
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
        // Attribute mutations are unsupported, including a changed qualifier nested in the target.
        if matches!(node, AnyNodeRef::ExprAttribute(attribute) if !attribute.ctx.is_load()) {
            self.mutation_target_depth += 1;
        }
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
        match node {
            AnyNodeRef::StmtAugAssign(_) => self.augmented = None,
            AnyNodeRef::ExprAttribute(attribute) if !attribute.ctx.is_load() => {
                self.mutation_target_depth -= 1;
            }
            _ => {}
        }
    }
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

fn implicit_import_name<'a>(
    db: &dyn Db,
    definition: Definition<'_>,
    new: &'a ModuleName,
) -> Option<&'a str> {
    let module = ruff_db::parsed::parsed_module(db, definition.file(db)).load(db);
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
mod tests;
