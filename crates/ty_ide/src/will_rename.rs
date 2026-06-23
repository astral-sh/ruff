//! Import edits for renaming Python modules and packages.
//!
//! See [`will_rename_paths`] for the supported policy.

use std::sync::Mutex;

use crate::references::contains_identifier;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::{
    self as ast, AnyNodeRef,
    visitor::source_order::{SourceOrderVisitor, TraversalSignal},
};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use ty_module_resolver::{Module, ModuleName, ModuleResolveMode, file_to_module, search_paths};
use ty_project::Db;
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

/// Returns the text replacements that should be applied before renaming Python modules.
///
/// Every supported item in `renames` is handled independently. Python files can move within or
/// between import roots, and directories can contain regular packages, namespace-package
/// portions, or both. Imports keep their existing statement shape, meaningful explicit aliases
/// remain unchanged, and direct module usages are rewritten when semantic analysis resolves them
/// to a moved file. A redundant alias that repeats a renamed module component is renamed with its
/// uses. For example, moving `old_pkg/tool.py` to `new_pkg/tool.py` changes:
///
/// ```python
/// from old_pkg import tool
/// ```
///
/// to `from new_pkg import tool`.
///
/// Relative imports in moved files are recomputed at their destination and become absolute when
/// no relative spelling is available. Unmoved portions of a split namespace are not exhaustively
/// searched for implicit relative references to descendants in the moved portion. An unsupported
/// path or module does not suppress edits for other rename items. If one affected file contains an
/// import or reference that cannot be rewritten with non-overlapping text replacements, edits for
/// that file are omitted while other files remain.
///
/// This is a semantic best-effort operation, not a proof that the destination workspace is valid.
/// It does not preflight destination shadowing, binding collisions, collisions inferred from a
/// moved directory's descendants, or completeness through dynamic references. Namespace modules
/// without a concrete file are left unchanged because a split namespace cannot be attributed to
/// one physical directory safely.
///
/// # Arguments
///
/// * `db` - The semantic database used to resolve modules and analyze references.
/// * `renames` - The filesystem path renames reported by the client.
/// * `files` - The candidate Python files that may contain affected imports or references.
/// * `file_is_in_scope` - Returns whether a candidate or renamed file belongs to the request's
///   scope. Files outside the scope are not inspected or edited.
pub fn will_rename_paths(
    db: &dyn Db,
    renames: &[PathRename],
    files: impl IntoIterator<Item = File>,
    file_is_in_scope: impl Fn(File) -> bool,
) -> Vec<FileRenameEdit> {
    let plan = RenamePlan::new(db, renames);
    if plan.rules.is_empty() {
        return Vec::new();
    }

    let mut files: FxHashSet<_> = files
        .into_iter()
        .filter(|file| file_is_in_scope(*file))
        .collect();

    // A moved file can be excluded from the project index but still contain an import whose
    // meaning changes at the destination.
    files.extend(
        plan.moved_files
            .iter()
            .copied()
            .filter(|file| file_is_in_scope(*file)),
    );

    let db = Db::dyn_clone(db);
    let collected = Mutex::new(Vec::new());

    let plan_ref = &plan;
    let collected_ref = &collected;
    rayon::scope(move |scope| {
        #[expect(
            clippy::iter_over_hash_type,
            reason = "Rayon task order is unspecified and edits are sorted before returning"
        )]
        for file in files {
            let db = Db::dyn_clone(&*db);
            let plan = plan_ref;
            let collected = collected_ref;
            scope.spawn(move |_| {
                if let Some(edits) = rename_edits_for_file(&*db, file, plan) {
                    collected
                        .lock()
                        .expect("rename edit worker should not panic while holding the lock")
                        .extend(edits);
                }
            });
        }
    });

    let mut edits = collected
        .into_inner()
        .expect("rename edit worker should not panic while holding the lock");
    edits.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| left.range.start().cmp(&right.range.start()))
            .then_with(|| left.range.end().cmp(&right.range.end()))
            .then_with(|| left.new_text.cmp(&right.new_text))
    });
    edits
}

/// A Python module or directory rename received from the client.
#[derive(Debug, Clone)]
pub struct PathRename {
    /// The current filesystem path.
    old_path: SystemPathBuf,
    /// The requested filesystem path.
    new_path: SystemPathBuf,
    /// Whether the path names a file or directory.
    kind: PathRenameKind,
}

impl PathRename {
    /// Creates a file rename from `old_path` to `new_path`.
    pub fn file(old_path: SystemPathBuf, new_path: SystemPathBuf) -> Self {
        Self {
            old_path,
            new_path,
            kind: PathRenameKind::File,
        }
    }

    /// Creates a directory rename from `old_path` to `new_path`.
    pub fn directory(old_path: SystemPathBuf, new_path: SystemPathBuf) -> Self {
        Self {
            old_path,
            new_path,
            kind: PathRenameKind::Directory,
        }
    }
}

/// A text replacement to apply before renaming a Python module or package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRenameEdit {
    /// The file to edit before the rename.
    file: File,
    /// The source range to replace.
    range: TextRange,
    /// The replacement text.
    new_text: String,
}

impl FileRenameEdit {
    /// Returns the file, source range, and replacement text that make up this edit.
    pub fn into_parts(self) -> (File, TextRange, String) {
        (self.file, self.range, self.new_text)
    }
}

/// Whether a path identifies a Python file or a directory containing Python modules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathRenameKind {
    File,
    Directory,
}

/// A batch of non-conflicting module rename rules.
struct RenamePlan {
    rules: Vec<RenameRule>,
    moved_files: FxHashSet<File>,
}

impl RenamePlan {
    fn new(db: &dyn Db, renames: &[PathRename]) -> Self {
        let items: Vec<_> = renames
            .iter()
            .filter_map(|rename| ResolvedRename::from_path_rename(db, rename))
            .collect();
        let mut conflicting = FxHashSet::default();

        for left in 0..items.len() {
            for right in left + 1..items.len() {
                if items[left].rule.conflicts_with(db, &items[right].rule) {
                    conflicting.insert(left);
                    conflicting.insert(right);
                }
            }
        }

        let mut plan = Self {
            rules: Vec::new(),
            moved_files: FxHashSet::default(),
        };
        for (index, item) in items.into_iter().enumerate() {
            if conflicting.contains(&index) {
                continue;
            }
            plan.moved_files.extend(item.moved_files);
            plan.insert_rule(item.rule);
        }
        plan
    }

    fn insert_rule(&mut self, rule: RenameRule) {
        let RenameRule {
            old_name,
            new_name,
            source,
        } = rule;
        match source {
            RenameSource::Files { files } => {
                if let Some(existing) = self
                    .rules
                    .iter_mut()
                    .find(|existing| existing.old_name == old_name && existing.new_name == new_name)
                    && let RenameSource::Files {
                        files: existing_files,
                    } = &mut existing.source
                {
                    existing_files.extend(files);
                    return;
                }
                self.rules.push(RenameRule {
                    old_name,
                    new_name,
                    source: RenameSource::Files { files },
                });
            }
            RenameSource::Directory { old_root } => self.rules.push(RenameRule {
                old_name,
                new_name,
                source: RenameSource::Directory { old_root },
            }),
        }
    }

    fn remap_module(&self, db: &dyn Db, module: Module<'_>) -> ModuleRemap {
        let name = module.name(db);
        let file = module.file(db);
        let mut best: Option<(usize, ModuleName)> = None;
        let mut ambiguous = false;

        for rule in &self.rules {
            let Some(new_name) = rule.rewrite_name(name) else {
                continue;
            };
            match rule.applies_to_module(db, file) {
                Some(true) => {
                    let specificity = rule.old_name.components().count();
                    if best
                        .as_ref()
                        .is_none_or(|(best_specificity, _)| specificity > *best_specificity)
                    {
                        best = Some((specificity, new_name));
                    }
                }
                Some(false) => {}
                None => ambiguous = true,
            }
        }

        if let Some((_, new_name)) = best {
            ModuleRemap::Renamed(new_name)
        } else if ambiguous {
            ModuleRemap::Ambiguous
        } else {
            ModuleRemap::Unchanged
        }
    }

    fn remap_file(&self, db: &dyn Db, file: File) -> Option<ModuleName> {
        let old_name = file_to_module(db, file)?.name(db);
        self.rules
            .iter()
            .filter(|rule| rule.contains_file(db, file))
            .filter_map(|rule| {
                Some((
                    rule.old_name.components().count(),
                    rule.rewrite_name(old_name)?,
                ))
            })
            .max_by_key(|(specificity, _)| *specificity)
            .map(|(_, new_name)| new_name)
    }

    fn source_mentions_any(&self, source: &str) -> bool {
        self.rules.iter().any(|rule| {
            contains_identifier(source, rule.old_name.first_component())
                || contains_identifier(source, rule.old_name.last_component())
        })
    }
}

struct ResolvedRename {
    rule: RenameRule,
    moved_files: FxHashSet<File>,
}

impl ResolvedRename {
    fn from_path_rename(db: &dyn Db, rename: &PathRename) -> Option<Self> {
        let old_path = SystemPath::absolute(&rename.old_path, db.system().current_directory());
        let new_path = SystemPath::absolute(&rename.new_path, db.system().current_directory());

        match rename.kind {
            PathRenameKind::File => {
                let extension = old_path.extension()?;
                if !matches!(extension, "py" | "pyi")
                    || new_path.extension() != Some(extension)
                    || old_path.file_stem() == Some("__init__")
                    || new_path.file_stem() == Some("__init__")
                {
                    return None;
                }
                let file = system_path_to_file(db, &old_path).ok()?;
                let old_name = file_to_module(db, file)?.name(db).clone();
                let new_name = unique_module_name_for_path(db, &new_path, PathRenameKind::File)?;
                Some(Self {
                    rule: RenameRule {
                        old_name,
                        new_name,
                        source: RenameSource::Files {
                            files: FxHashSet::from_iter([file]),
                        },
                    },
                    moved_files: FxHashSet::from_iter([file]),
                })
            }
            PathRenameKind::Directory => {
                if !db.system().is_directory(&old_path) || new_path.starts_with(&old_path) {
                    return None;
                }
                let old_name =
                    unique_module_name_for_path(db, &old_path, PathRenameKind::Directory)?;
                let new_name =
                    unique_module_name_for_path(db, &new_path, PathRenameKind::Directory)?;
                let moved_files = python_files_in_directory(db, &old_path);
                if moved_files.is_empty() {
                    return None;
                }
                Some(Self {
                    rule: RenameRule {
                        old_name,
                        new_name,
                        source: RenameSource::Directory { old_root: old_path },
                    },
                    moved_files,
                })
            }
        }
    }
}

struct RenameRule {
    old_name: ModuleName,
    new_name: ModuleName,
    source: RenameSource,
}

impl RenameRule {
    fn rewrite_name(&self, name: &ModuleName) -> Option<ModuleName> {
        if name == &self.old_name {
            return Some(self.new_name.clone());
        }
        if !matches!(self.source, RenameSource::Directory { .. }) {
            return None;
        }
        let suffix = name.relative_to(&self.old_name)?;
        let mut new_name = self.new_name.clone();
        new_name.extend(&suffix);
        Some(new_name)
    }

    /// Returns `None` when the matching module is an aggregate namespace without a concrete file.
    fn applies_to_module(&self, db: &dyn Db, file: Option<File>) -> Option<bool> {
        match &self.source {
            RenameSource::Files { files } => Some(file.is_some_and(|file| files.contains(&file))),
            RenameSource::Directory { old_root } => {
                let file = file?;
                Some(
                    file.path(db)
                        .as_system_path()
                        .is_some_and(|path| path.starts_with(old_root)),
                )
            }
        }
    }

    fn contains_file(&self, db: &dyn Db, file: File) -> bool {
        match &self.source {
            RenameSource::Files { files } => files.contains(&file),
            RenameSource::Directory { old_root } => file
                .path(db)
                .as_system_path()
                .is_some_and(|path| path.starts_with(old_root)),
        }
    }

    fn conflicts_with(&self, db: &dyn Db, other: &Self) -> bool {
        if self.old_name == other.old_name {
            return self.new_name != other.new_name;
        }
        if self.new_name == other.new_name {
            return true;
        }

        self.contains_source_of(db, other)
            .is_some_and(|expected| expected != other.new_name)
            || other
                .contains_source_of(db, self)
                .is_some_and(|expected| expected != self.new_name)
    }

    fn contains_source_of(&self, db: &dyn Db, other: &Self) -> Option<ModuleName> {
        let RenameSource::Directory { old_root } = &self.source else {
            return None;
        };
        let contained = match &other.source {
            RenameSource::Files { files } => files.iter().any(|file| {
                file.path(db)
                    .as_system_path()
                    .is_some_and(|path| path.starts_with(old_root))
            }),
            RenameSource::Directory {
                old_root: other_root,
            } => other_root.starts_with(old_root),
        };
        contained
            .then(|| self.rewrite_name(&other.old_name))
            .flatten()
    }
}

enum RenameSource {
    Files { files: FxHashSet<File> },
    Directory { old_root: SystemPathBuf },
}

enum ModuleRemap {
    Unchanged,
    Renamed(ModuleName),
    Ambiguous,
}

fn unique_module_name_for_path(
    db: &dyn Db,
    path: &SystemPath,
    kind: PathRenameKind,
) -> Option<ModuleName> {
    let path = SystemPath::absolute(path, db.system().current_directory());
    let mut result = None;

    for search_path in search_paths(db, ModuleResolveMode::StubsAllowed) {
        let Some(root) = search_path.as_system_path() else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if search_path.is_standard_library() {
            return None;
        }
        let Some(name) = (match kind {
            PathRenameKind::File => ModuleName::from_components(
                relative
                    .parent()?
                    .components()
                    .map(|component| component.as_str())
                    .chain(std::iter::once(path.file_stem()?)),
            ),
            PathRenameKind::Directory => ModuleName::from_components(
                relative.components().map(|component| component.as_str()),
            ),
        }) else {
            continue;
        };

        match &result {
            None => result = Some(name),
            Some(existing) if existing == &name => {}
            Some(_) => return None,
        }
    }
    result
}

fn python_files_in_directory(db: &dyn Db, root: &SystemPath) -> FxHashSet<File> {
    let mut directories = vec![root.to_path_buf()];
    let mut files = FxHashSet::default();

    while let Some(directory) = directories.pop() {
        let Ok(entries) = db.system().read_directory(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_type = entry.file_type();
            let path = entry.into_path();
            if file_type.is_directory() {
                directories.push(path);
            } else if file_type.is_file()
                && matches!(path.extension(), Some("py" | "pyi"))
                && let Ok(file) = system_path_to_file(db, path)
            {
                files.insert(file);
            }
        }
    }
    files
}

fn rename_edits_for_file(
    db: &dyn Db,
    file: File,
    plan: &RenamePlan,
) -> Option<Vec<FileRenameEdit>> {
    let source = source_text(db, file);
    if source.read_error().is_some() {
        return None;
    }
    if !plan.moved_files.contains(&file) && !plan.source_mentions_any(source.as_str()) {
        return Some(Vec::new());
    }

    let parsed = ruff_db::parsed::parsed_module(db, file);
    let module = parsed.load(db);
    let importing_module = file_to_module(db, file).map(|importing| ImportingModule {
        name_after_rename: plan
            .remap_file(db, file)
            .unwrap_or_else(|| importing.name(db).clone()),
        is_package: importing.kind(db).is_package(),
        renamed: plan.moved_files.contains(&file),
    });
    let model = SemanticModel::new(db, file);
    let mut visitor = ModuleRenameVisitor {
        db,
        model: &model,
        tokens: module.tokens(),
        source: source.as_str(),
        plan,
        importing_module: importing_module.as_ref(),
        edits: Vec::new(),
        supported: true,
    };
    AnyNodeRef::from(module.syntax()).visit_source_order(&mut visitor);
    visitor.finish(file)
}

struct ImportingModule {
    name_after_rename: ModuleName,
    is_package: bool,
    renamed: bool,
}

struct ModuleRenameVisitor<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    tokens: &'a Tokens,
    source: &'a str,
    plan: &'a RenamePlan,
    importing_module: Option<&'a ImportingModule>,
    edits: Vec<(TextRange, String)>,
    supported: bool,
}

impl<'a> SourceOrderVisitor<'a> for ModuleRenameVisitor<'_, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        if !self.supported {
            return TraversalSignal::Skip;
        }
        match node {
            AnyNodeRef::StmtImport(import) => {
                self.handle_import(import);
                TraversalSignal::Skip
            }
            AnyNodeRef::StmtImportFrom(import) => {
                self.handle_import_from(import);
                TraversalSignal::Skip
            }
            AnyNodeRef::ExprName(name) => {
                if self.handle_name(name) {
                    TraversalSignal::Skip
                } else {
                    TraversalSignal::Traverse
                }
            }
            AnyNodeRef::ExprAttribute(attribute) => {
                if self.handle_attribute(attribute) {
                    TraversalSignal::Skip
                } else {
                    TraversalSignal::Traverse
                }
            }
            _ => TraversalSignal::Traverse,
        }
    }
}

impl ModuleRenameVisitor<'_, '_> {
    fn finish(mut self, file: File) -> Option<Vec<FileRenameEdit>> {
        if !self.supported {
            return None;
        }
        self.edits.sort_by(|left, right| {
            left.0
                .start()
                .cmp(&right.0.start())
                .then_with(|| left.0.end().cmp(&right.0.end()))
                .then_with(|| left.1.cmp(&right.1))
        });
        self.edits.dedup();
        if self.edits.windows(2).any(|edits| {
            edits[0].0.start() == edits[1].0.start() || edits[0].0.end() > edits[1].0.start()
        }) {
            return None;
        }
        Some(
            self.edits
                .into_iter()
                .map(|(range, new_text)| FileRenameEdit {
                    file,
                    range,
                    new_text,
                })
                .collect(),
        )
    }

    fn handle_import(&mut self, import: &ast::StmtImport) {
        let mut edits = Vec::new();
        for alias in &import.names {
            let Some(module) = self.model.resolve_module(Some(alias.name.as_str()), 0) else {
                continue;
            };
            match self.plan.remap_module(self.db, module) {
                ModuleRemap::Unchanged => {}
                ModuleRemap::Renamed(new_name) => {
                    if let Some(edit) =
                        redundant_alias_edit(alias, module.name(self.model.db()), &new_name)
                    {
                        edits.push(edit);
                    }
                    edits.push((alias.name.range, new_name.as_str().to_string()));
                }
                ModuleRemap::Ambiguous => {
                    self.supported = false;
                    return;
                }
            }
        }
        for (range, new_text) in edits {
            self.push_if_changed(range, new_text);
        }
    }

    fn handle_import_from(&mut self, import: &ast::StmtImportFrom) {
        let importing_context_changed = import.level > 0
            && self
                .importing_module
                .is_some_and(|importing| importing.renamed);
        let Ok(old_parent) =
            ModuleName::from_import_statement(self.model.db(), self.model.file(), import)
        else {
            if importing_context_changed {
                self.supported = false;
            }
            return;
        };
        let parent_remap = self
            .model
            .resolve_module(
                import.module.as_ref().map(ast::Identifier::as_str),
                import.level,
            )
            .map_or(ModuleRemap::Unchanged, |module| {
                self.plan.remap_module(self.db, module)
            });
        let default_parent = match &parent_remap {
            ModuleRemap::Unchanged => Some(old_parent.clone()),
            ModuleRemap::Renamed(new_parent) => Some(new_parent.clone()),
            ModuleRemap::Ambiguous => None,
        };

        let mut statement_parent = None;
        let mut alias_edits = Vec::new();
        for alias in &import.names {
            // A differently named module value can be a stable re-export rather than a direct
            // submodule of the statement's parent.
            let alias_module = module_from_type(self.model, alias).filter(|module| {
                alias.name.as_str() == module.name(self.model.db()).last_component()
            });
            let alias_parent = if let Some(module) = alias_module {
                match self.plan.remap_module(self.db, module) {
                    ModuleRemap::Renamed(new_name) => {
                        if let Some(edit) =
                            redundant_alias_edit(alias, module.name(self.model.db()), &new_name)
                        {
                            alias_edits.push(edit);
                        }
                        let Some(new_parent) = new_name.parent() else {
                            self.supported = false;
                            return;
                        };
                        if alias.name.as_str() != new_name.last_component() {
                            alias_edits
                                .push((alias.name.range, new_name.last_component().to_string()));
                        }
                        new_parent
                    }
                    ModuleRemap::Unchanged => {
                        default_parent.clone().unwrap_or_else(|| old_parent.clone())
                    }
                    ModuleRemap::Ambiguous => {
                        self.supported = false;
                        return;
                    }
                }
            } else if let Some(default_parent) = &default_parent {
                default_parent.clone()
            } else {
                self.supported = false;
                return;
            };

            if statement_parent
                .as_ref()
                .is_some_and(|parent| parent != &alias_parent)
            {
                self.supported = false;
                return;
            }
            statement_parent.get_or_insert(alias_parent);
        }

        let statement_parent = statement_parent.unwrap_or(old_parent.clone());
        let mut edits = Vec::new();
        if statement_parent != old_parent || importing_context_changed {
            let Some(range) = import_from_module_range(self.tokens, import) else {
                self.supported = false;
                return;
            };
            let Some(new_text) =
                render_import_from_module(import, &statement_parent, self.importing_module)
            else {
                self.supported = false;
                return;
            };
            edits.push((range, new_text));
        }
        edits.extend(alias_edits);
        for (range, new_text) in edits {
            self.push_if_changed(range, new_text);
        }
    }

    fn handle_name(&mut self, name: &ast::ExprName) -> bool {
        let Some(module) = module_from_type(self.model, name) else {
            return false;
        };
        let old_name = module.name(self.model.db());
        match self.plan.remap_module(self.db, module) {
            ModuleRemap::Unchanged => false,
            ModuleRemap::Ambiguous if module.file(self.db).is_none() => true,
            ModuleRemap::Ambiguous => {
                self.supported = false;
                true
            }
            ModuleRemap::Renamed(new_name) => {
                let new_text = if name.id.as_str() == old_name.as_str() {
                    Some(new_name.as_str())
                } else if name.id.as_str() == old_name.last_component() {
                    Some(new_name.last_component())
                } else {
                    None
                };
                if let Some(new_text) = new_text {
                    self.push_if_changed(name.range, new_text.to_string());
                }
                true
            }
        }
    }

    fn handle_attribute(&mut self, attribute: &ast::ExprAttribute) -> bool {
        let Some(module) = module_from_type(self.model, attribute) else {
            return false;
        };
        let old_name = module.name(self.model.db());
        match self.plan.remap_module(self.db, module) {
            // The complete module expression is unchanged. Do not descend into an aggregate
            // namespace root, which cannot be attributed to one physical portion on its own.
            ModuleRemap::Unchanged => module.file(self.db).is_none(),
            ModuleRemap::Ambiguous if module.file(self.db).is_none() => true,
            ModuleRemap::Ambiguous => {
                self.supported = false;
                true
            }
            ModuleRemap::Renamed(new_name) => {
                if let Some(new_text) =
                    rewritten_attribute_path(self.model, attribute, old_name, &new_name)
                {
                    self.push_if_changed(attribute.range, new_text);
                } else {
                    self.supported = false;
                }
                true
            }
        }
    }

    fn push_if_changed(&mut self, range: TextRange, new_text: String) {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        if self.source.get(start..end) != Some(new_text.as_str()) {
            self.edits.push((range, new_text));
        }
    }
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

fn redundant_alias_edit(
    alias: &ast::Alias,
    old_name: &ModuleName,
    new_name: &ModuleName,
) -> Option<(TextRange, String)> {
    let asname = alias.asname.as_ref()?;
    (asname.as_str() == old_name.last_component() && asname.as_str() != new_name.last_component())
        .then(|| (asname.range, new_name.last_component().to_string()))
}

fn rewritten_attribute_path(
    model: &SemanticModel<'_>,
    attribute: &ast::ExprAttribute,
    old_name: &ModuleName,
    new_name: &ModuleName,
) -> Option<String> {
    let root = root_name_of(&attribute.value)?;
    let root_module = module_from_type(model, root)?;
    let root_old = root_module.name(model.db());
    if root.id.as_str() == root_old.as_str() {
        return Some(new_name.as_str().to_string());
    }

    let removed_components = old_name.relative_to(root_old)?.components().count();
    let mut root_new = new_name.clone();
    for _ in 0..removed_components {
        root_new = root_new.parent()?;
    }
    let suffix = new_name.relative_to(&root_new)?;
    let root_text = if root.id.as_str() == root_old.last_component() {
        root_new.last_component()
    } else {
        if &root_new != root_old {
            return None;
        }
        root.id.as_str()
    };
    if suffix.as_str().is_empty() {
        Some(root_text.to_string())
    } else {
        Some(format!("{root_text}.{suffix}"))
    }
}

fn root_name_of(expression: &ast::Expr) -> Option<&ast::ExprName> {
    match expression {
        ast::Expr::Name(name) => Some(name),
        ast::Expr::Attribute(attribute) => root_name_of(&attribute.value),
        _ => None,
    }
}

fn import_from_module_range(tokens: &Tokens, import: &ast::StmtImportFrom) -> Option<TextRange> {
    let mut after_from = false;
    let mut first = None;
    let mut last = None;

    for token in tokens.in_range(import.range) {
        match token.kind() {
            TokenKind::From => after_from = true,
            TokenKind::Import if after_from => break,
            TokenKind::Dot | TokenKind::Ellipsis | TokenKind::Name if after_from => {
                first.get_or_insert(token.start());
                last = Some(token.end());
            }
            _ => {}
        }
    }
    Some(TextRange::new(first?, last?))
}

fn render_import_from_module(
    import: &ast::StmtImportFrom,
    new_name: &ModuleName,
    importing_module: Option<&ImportingModule>,
) -> Option<String> {
    if import.level == 0 {
        return Some(new_name.as_str().to_string());
    }

    let importing_module = importing_module?;
    let anchor = if importing_module.is_package {
        Some(importing_module.name_after_rename.clone())
    } else {
        importing_module.name_after_rename.parent()
    };
    let Some(anchor) = anchor else {
        return Some(new_name.as_str().to_string());
    };

    for (depth, base) in anchor.ancestors().enumerate() {
        let mut rendered = ".".repeat(depth + 1);
        if new_name == &base {
            return Some(rendered);
        }
        if let Some(relative) = new_name.relative_to(&base) {
            rendered.push_str(relative.as_str());
            return Some(rendered);
        }
    }
    Some(new_name.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::Db as _;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ruff_python_ast::PythonVersion;
    use ty_module_resolver::SearchPathSettings;
    use ty_project::{ProjectMetadata, TestDb};
    use ty_python_core::platform::PythonPlatform;
    use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
    use ty_python_semantic::PythonVersionWithSource;

    #[test]
    fn semantic_file_move_preserves_aliases_reexports_and_shadowed_names() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", ""),
            ("/old_pkg/old.py", "import helper as helper\n"),
            ("/new_pkg/__init__.py", ""),
            ("/helper.py", "x = 1\n"),
            ("/facade.py", "import old_pkg.old as stable\n"),
            (
                "/consumer.py",
                "def qualified():\n    import old_pkg.old\n    return old_pkg.old.helper.x\nimport old_pkg.old as direct_stable\nfrom facade import stable\ndef local():\n    from old_pkg import old\n    return old.helper.x, direct_stable.helper.x, stable.helper.x\ndef redundant():\n    import old_pkg.old as old\n    return old.helper.x\ndef redundant_from():\n    from old_pkg import old as old\n    return old.helper.x\ndef shadowed(old_pkg):\n    return old_pkg.old\n",
            ),
        ]);

        let edits = will_rename(
            &db,
            &[PathRename::file(
                "/old_pkg/old.py".into(),
                "/new_pkg/new.py".into(),
            )],
        );
        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let facade = system_path_to_file(&db, "/facade.py").unwrap();
        assert_eq!(
            apply_edits(&db, &edits, consumer),
            "def qualified():\n    import new_pkg.new\n    return new_pkg.new.helper.x\nimport new_pkg.new as direct_stable\nfrom facade import stable\ndef local():\n    from new_pkg import new\n    return new.helper.x, direct_stable.helper.x, stable.helper.x\ndef redundant():\n    import new_pkg.new as new\n    return new.helper.x\ndef redundant_from():\n    from new_pkg import new as new\n    return new.helper.x\ndef shadowed(old_pkg):\n    return old_pkg.old\n"
        );
        assert_eq!(
            apply_edits(&db, &edits, facade),
            "import new_pkg.new as stable\n"
        );
    }

    #[test]
    fn directory_move_supports_regular_and_namespace_packages() {
        for (initializer, old, new) in [
            (Some(("/old_pkg/__init__.py", "")), "/old_pkg", "/new_pkg"),
            (None, "/old_ns", "/new_ns"),
        ] {
            let is_regular = initializer.is_some();
            let mut files = vec![
                ("/consumer.py", "import old_pkg.sub\nprint(old_pkg.sub.x)\n"),
                ("/old_pkg/sub.py", "x = 1\n"),
            ];
            if let Some(initializer) = initializer {
                files.push(initializer);
            } else {
                files = vec![
                    ("/consumer.py", "import old_ns.sub\nprint(old_ns.sub.x)\n"),
                    ("/old_ns/sub.py", "x = 1\n"),
                ];
            }
            let db = create_test_db(&files);
            let edits = will_rename(&db, &[PathRename::directory(old.into(), new.into())]);
            let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
            let expected = if is_regular {
                "import new_pkg.sub\nprint(new_pkg.sub.x)\n"
            } else {
                "import new_ns.sub\nprint(new_ns.sub.x)\n"
            };
            assert_eq!(apply_edits(&db, &edits, consumer), expected);
        }
    }

    #[test]
    fn moved_file_rebases_relative_import_without_being_a_candidate() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", ""),
            ("/old_pkg/helper.py", "value = 1\n"),
            ("/old_pkg/moved.py", "from . import helper\n"),
            ("/new_pkg/__init__.py", ""),
        ]);
        let rename = PathRename::file("/old_pkg/moved.py".into(), "/new_pkg/moved.py".into());
        let edits = will_rename_paths(&db, &[rename], std::iter::empty(), |_| true);
        let moved = system_path_to_file(&db, "/old_pkg/moved.py").unwrap();
        assert_eq!(
            apply_edits(&db, &edits, moved),
            "from old_pkg import helper\n"
        );
    }

    #[test]
    fn batch_coalesces_source_stub_pairs_and_isolates_conflicts() {
        let mut db = create_test_db(&[
            ("/old_a.py", "x = 1\n"),
            ("/old_b.py", "x = 1\n"),
            ("/old_b.pyi", "x: int\n"),
            ("/foo.py", "file_value = 1\n"),
            ("/foo/__init__.py", "package_value = 1\n"),
            ("/site-packages/old.py", "x = 1\n"),
            (
                "/consumer.py",
                "import old_a\nimport old_b\nimport foo\nimport old\nprint(old_a.x, old_b.x, foo.package_value, old.x)\n",
            ),
        ]);
        configure_search_paths(&mut db, vec!["/".into(), "/site-packages".into()]);
        let edits = will_rename(
            &db,
            &[
                PathRename::file("/old_a.py".into(), "/new_a.py".into()),
                PathRename::file("/old_a.py".into(), "/other_a.py".into()),
                PathRename::file("/old_b.py".into(), "/new_b.py".into()),
                PathRename::file("/old_b.pyi".into(), "/new_b.pyi".into()),
                PathRename::file("/foo.py".into(), "/baz.py".into()),
                PathRename::directory("/foo".into(), "/bar".into()),
                PathRename::file(
                    "/site-packages/old.py".into(),
                    "/site-packages/new.py".into(),
                ),
            ],
        );
        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        assert_eq!(
            apply_edits(&db, &edits, consumer),
            "import old_a\nimport new_b\nimport bar\nimport new\nprint(old_a.x, new_b.x, bar.package_value, new.x)\n"
        );
    }

    #[test]
    fn unsupported_statement_discards_only_its_file() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", "other = 1\n"),
            ("/old_pkg/moved.py", "x = 1\n"),
            ("/new_pkg/__init__.py", ""),
            ("/safe.py", "import old_pkg.moved\nprint(old_pkg.moved.x)\n"),
            (
                "/unsupported.py",
                "from old_pkg import moved, other\nprint(moved.x, other)\n",
            ),
            (
                "/unchanged_alias.py",
                "import old_pkg.moved\nimport old_pkg as stable\nfrom old_pkg import moved\nprint(stable.moved.x, moved.x)\n",
            ),
        ]);
        let edits = will_rename(
            &db,
            &[PathRename::file(
                "/old_pkg/moved.py".into(),
                "/new_pkg/new.py".into(),
            )],
        );
        let safe = system_path_to_file(&db, "/safe.py").unwrap();
        let unsupported = system_path_to_file(&db, "/unsupported.py").unwrap();
        let unchanged_alias = system_path_to_file(&db, "/unchanged_alias.py").unwrap();
        assert_eq!(
            apply_edits(&db, &edits, safe),
            "import new_pkg.new\nprint(new_pkg.new.x)\n"
        );
        assert_eq!(
            apply_edits(&db, &edits, unsupported),
            source_text(&db, unsupported).as_str()
        );
        assert_eq!(
            apply_edits(&db, &edits, unchanged_alias),
            source_text(&db, unchanged_alias).as_str()
        );
    }

    #[test]
    fn namespace_move_only_rewrites_the_moved_portion() {
        let mut db = create_test_db(&[
            ("/one/ns/moved.py", "x = 1\n"),
            ("/two/ns/stays.py", "x = 2\n"),
            (
                "/consumer.py",
                "import ns.moved\nimport ns.stays\nprint(ns.moved.x, ns.stays.x)\n",
            ),
        ]);
        configure_search_paths(&mut db, vec!["/one".into(), "/two".into()]);
        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let edits = will_rename_paths(
            &db,
            &[PathRename::directory(
                "/one/ns".into(),
                "/one/new_ns".into(),
            )],
            [consumer],
            |_| true,
        );
        assert_eq!(
            apply_edits(&db, &edits, consumer),
            "import new_ns.moved\nimport ns.stays\nprint(new_ns.moved.x, ns.stays.x)\n"
        );
    }

    #[test]
    fn destination_binding_collision_does_not_cancel_local_edits() {
        let db = create_test_db(&[
            ("/old.py", "x = 1\n"),
            ("/consumer.py", "import old\nnew = 1\nprint(old.x, new)\n"),
        ]);
        assert_file_move(
            &db,
            "/old.py",
            "/new.py",
            "/consumer.py",
            "import new\nnew = 1\nprint(new.x, new)\n",
        );
    }

    fn will_rename(db: &dyn Db, renames: &[PathRename]) -> Vec<FileRenameEdit> {
        let project = db.project();
        let indexed_files = project.files(db);
        let open_files = project.open_files(db);
        will_rename_paths(
            db,
            renames,
            (&indexed_files)
                .into_iter()
                .chain(open_files.iter().copied()),
            |_| true,
        )
    }

    fn assert_file_move(db: &dyn Db, old_path: &str, new_path: &str, target: &str, expected: &str) {
        let edits = will_rename(db, &[PathRename::file(old_path.into(), new_path.into())]);
        let target = system_path_to_file(db, target).unwrap();
        assert_eq!(apply_edits(db, &edits, target), expected);
    }

    fn create_test_db(files: &[(&str, &str)]) -> TestDb {
        let mut db = TestDb::new(ProjectMetadata::new("test".into(), "/".into()));
        db.init_program_with_python_version(PythonVersion::latest_ty())
            .unwrap();
        for &(path, contents) in files {
            db.write_file(path, contents)
                .expect("write to memory file system to be successful");
        }
        db
    }

    fn configure_search_paths(db: &mut TestDb, src_roots: Vec<SystemPathBuf>) {
        let settings = SearchPathSettings::new(src_roots);
        let search_paths = settings
            .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
            .expect("valid search paths");
        Program::init_or_update(
            db,
            ProgramSettings {
                python_version: PythonVersionWithSource::default(),
                python_platform: PythonPlatform::default(),
                search_paths,
            },
        );
    }

    fn apply_edits(db: &dyn Db, edits: &[FileRenameEdit], file: File) -> String {
        let mut sorted_edits: Vec<_> = edits.iter().filter(|edit| edit.file == file).collect();
        sorted_edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start()));

        let mut result = source_text(db, file).as_str().to_owned();
        for edit in sorted_edits {
            let start = usize::from(edit.range.start());
            let end = usize::from(edit.range.end());
            result.replace_range(start..end, &edit.new_text);
        }
        result
    }
}
