//! Computes source edits for Python module and regular-package renames.
//!
//! [`will_rename_paths`] maps filesystem renames to module names, then rewrites imports and uses in
//! the candidate files supplied by the caller. It does not move files, discover package contents,
//! validate the filesystem operation, or normalize the returned edits.
//!
//! A `.py` or `.pyi` module can move when it keeps its extension and is not a package initializer.
//! Runtime files and stubs share one module identity, so a shadowed stub rename is a no-op while a
//! coordinated runtime/stub move is handled once. Directory renames are limited to regular
//! packages that keep the same logical parent; namespace and cross-parent package moves are
//! ignored.
//!
//! Import syntax determines whether a local name changes. An explicit `as` alias remains fixed;
//! an unaliased import changes its binding and semantically established uses. Unaliased re-exports
//! propagate that spelling change when all reachable definitions agree. Inference also lets valid
//! string annotations participate without treating arbitrary strings as references.
//!
//! For example, renaming `pkg/old.py` to `pkg/new.py` updates both a re-export and its consumer:
//!
//! ```text
//! # facade.py, before
//! from pkg import old
//!
//! # use.py, before
//! from facade import old
//! print(old.C)
//!
//! # facade.py, after
//! from pkg import new
//!
//! # use.py, after
//! from facade import new
//! print(new.C)
//! ```
//!
//! Unsupported or ambiguous occurrences are left unchanged without suppressing independent edits.
//! This includes relative-import rebasing, write targets, scope declarations, star imports,
//! `__all__`, and dynamic references.

use crate::RangedValue;
use rayon::prelude::*;
use ruff_db::PythonFile;
use ruff_db::files::{File, FileRange, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, ModuleResolveMode, ResolverEnvironment, ResolverFile,
    file_to_module, is_legacy_namespace_package, resolve_module_confident,
    resolve_real_module_confident, search_paths,
};
use ty_project::{Db, parallel::ParallelIteratorExt};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel, binding_type};

/// Computes source edits for a batch of filesystem renames.
///
/// `files` must include every source the caller wants analyzed, including moved sources. Files
/// outside `in_scope` and edits that cannot be established safely are omitted.
pub fn will_rename_paths(
    db: &dyn Db,
    renames: &[PathRename],
    files: impl IntoIterator<Item = File>,
    in_scope: impl Fn(File) -> bool,
) -> Vec<FileRenameEdit> {
    let plan = RenamePlan::new(db, renames, &in_scope);
    let mut files: Vec<_> = files.into_iter().filter(|file| in_scope(*file)).collect();
    files.sort_unstable_by_key(|file| file.path(db).as_ref());
    files.dedup();
    files
        .into_par_iter()
        .map_with_db(db, |db, file| edits_for_file(db, file, &plan))
        .flatten()
        .collect()
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

#[derive(Clone, Copy)]
enum RenameKind {
    File,
    Directory,
}

struct RenamePlan {
    rules: Vec<RenameRule>,
    names: FxHashSet<String>,
}

impl RenamePlan {
    fn new(db: &dyn Db, renames: &[PathRename], in_scope: &impl Fn(File) -> bool) -> Self {
        let mut rules: Vec<RenameRule> = Vec::new();
        for rename in renames {
            if let Some(rule) = RenameRule::new(db, rename, in_scope) {
                rules.push(rule);
            }
        }
        rules.retain(|rule| rule.old_name != rule.new_name);
        rules.sort_unstable_by(|left, right| left.old_name.cmp(&right.old_name));
        rules.dedup_by(|left, right| {
            left.old_name == right.old_name && left.new_name == right.new_name
        });

        let mut names = FxHashSet::default();
        for rule in &rules {
            names.insert(rule.old_name.last_component().to_owned());
        }
        Self { rules, names }
    }

    fn rewrite(&self, db: &dyn Db, module: Module<'_>) -> Option<(&RenameRule, ModuleName)> {
        let rule = self.rule(module.name(db))?;
        rule.rewrites_name(module.name(db)).map(|name| (rule, name))
    }

    fn rule(&self, name: &ModuleName) -> Option<&RenameRule> {
        name.ancestors().find_map(|ancestor| {
            let index = self
                .rules
                .binary_search_by(|r| r.old_name.cmp(&ancestor))
                .ok()?;
            Some(&self.rules[index])
        })
    }
}

struct RenameRule {
    old_name: ModuleName,
    new_name: ModuleName,
    package: bool,
}

impl RenameRule {
    fn new(db: &dyn Db, rename: &PathRename, in_scope: &impl Fn(File) -> bool) -> Option<Self> {
        let resolver_environment = resolver_environment(db);
        let python_version = resolver_environment.python_version(db);
        let old = SystemPath::absolute(&rename.old_path, db.system().current_directory());
        let new = SystemPath::absolute(&rename.new_path, db.system().current_directory());
        let (old_name, package) = match rename.kind {
            RenameKind::File => {
                let extension = old.extension()?;
                (matches!(extension, "py" | "pyi")
                    && new.extension() == Some(extension)
                    && !matches!(old.file_stem(), Some("__init__"))
                    && !matches!(new.file_stem(), Some("__init__")))
                .then_some(())?;
                let file = system_path_to_file(db, &old).ok()?;
                in_scope(file).then_some(())?;
                let name = file_to_module(db, ResolverFile::new(db, file, resolver_environment))?
                    .name(db)
                    .clone();
                (resolved_source(db, &name)? == file).then_some(())?;
                (name, false)
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
                    && inits.iter().copied().all(in_scope)
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
                (name, true)
            }
        };
        let new_name = prospective_module(db, &new)?;
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
            package,
        })
    }

    fn rewrites_name(&self, name: &ModuleName) -> Option<ModuleName> {
        if name == &self.old_name {
            return Some(self.new_name.clone());
        }
        self.package.then_some(())?;
        let mut rewritten = self.new_name.clone();
        rewritten.extend(&name.relative_to(&self.old_name)?);
        Some(rewritten)
    }
}

fn prospective_module(db: &dyn Db, path: &SystemPath) -> Option<ModuleName> {
    search_paths(db, resolver_environment(db), ModuleResolveMode::Typing)
        .filter(|search_path| !search_path.is_standard_library())
        .find_map(|search_path| search_path.module_name_for_system_path(path))
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

fn edits_for_file(db: &dyn Db, file: File, plan: &RenamePlan) -> Vec<FileRenameEdit> {
    let program_file = db.program_file(file);
    let source = source_text(db, file);
    if source.read_error().is_some() {
        return Vec::new();
    }
    if source.as_str().is_ascii()
        && plan.names.iter().all(|name| name.is_ascii())
        && plan
            .names
            .iter()
            .all(|name| !source.as_str().contains(name))
    {
        return Vec::new();
    }
    let moves_across_parent =
        file_to_module(db, program_file.resolver_file(db)).is_some_and(|module| {
            plan.rewrite(db, module).is_some_and(|(rule, new)| {
                !rule.package && module.name(db).parent() != new.parent()
            })
        });
    let module = ruff_db::parsed::parsed_module(db, program_file.python_file(db)).load(db);
    let root = AnyNodeRef::from(module.syntax());
    let model = SemanticModel::new(db, program_file);
    let mut imports = ImportPass {
        db,
        model: &model,
        plan,
        moves_across_parent,
        output: ImportEdits::default(),
    };
    root.visit_source_order(&mut imports);
    let (mut edits, changes) = imports.finish();
    let mut semantics = SemanticPass {
        db,
        model: &model,
        plan,
        changes: &changes,
        edits: Vec::new(),
    };
    root.visit_source_order(&mut semantics);
    edits.extend(semantics.edits);
    edits
        .into_iter()
        .map(|(range, value)| RangedValue {
            range: FileRange::new(file, range),
            value,
        })
        .collect()
}

type Changes<'db> = FxHashMap<Definition<'db>, String>;

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
    moves_across_parent: bool,
    output: ImportEdits<'db>,
}

impl<'db> ImportPass<'_, 'db> {
    fn finish(self) -> (Vec<(TextRange, String)>, Changes<'db>) {
        (self.output.edits, self.output.changes)
    }

    fn record(&self, output: &mut ImportEdits<'db>, alias: &ast::Alias, old: &str, new: &str) {
        if alias.asname.is_some() || old == new {
            return;
        }
        let definition = ty_python_core::semantic_index(self.db, self.model.program_file())
            .expect_single_definition(alias);
        output.changes.insert(definition, new.to_string());
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
                && !rule.package
                && old.parent() != new.parent()
                && old.first_component() != new.first_component()
            {
                return None;
            }
            let new_binding = implicit_import_value_path(old, &new);
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
        if import.level > 0 && self.moves_across_parent {
            return None;
        }
        let Ok(old_parent) = ModuleName::from_import_statement(
            self.db,
            ImportingFile::ResolverFile(self.model.program_file().resolver_file(self.db)),
            import,
        ) else {
            return Some(output);
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
        let mut desired_parent = None;
        for alias in &import.names {
            let imported = module_from_type(self.model, alias);
            let direct = resolved_parent.is_some_and(|parent| {
                imported.is_some_and(|module| {
                    let name = module.name(self.db);
                    alias.name.as_str() == name.last_component()
                        && name.parent().as_ref() == Some(&old_parent)
                        && (self
                            .model
                            .definitions_for_module_global(parent, alias.name.as_str())
                            .is_none()
                            || file_to_module(
                                self.db,
                                self.model.program_file().resolver_file(self.db),
                            )
                            .is_some_and(|module| module.name(self.db) == parent.name(self.db)))
                })
            });
            if rewritten_parent != old_parent
                && alias
                    .inferred_type(self.model)
                    .is_none_or(|ty| ty.is_unknown())
            {
                return None;
            }
            let parent = if direct {
                let module = imported?;
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
                if self.plan.names.contains(alias.name.as_str()) {
                    let parent = resolved_parent?;
                    match module_export_decision(
                        self.db,
                        self.model,
                        self.plan,
                        parent,
                        alias.name.as_str(),
                        &mut Vec::new(),
                    ) {
                        Ok(None) => {}
                        Ok(Some(new)) => {
                            let changes_value_path = new.contains('.');
                            if changes_value_path && alias.asname.is_some() {
                                return None;
                            }
                            self.record(&mut output, alias, alias.name.as_str(), &new);
                            if !changes_value_path {
                                output.edits.push((alias.name.range, new));
                            }
                        }
                        Err(()) => return None,
                    }
                }
                rewritten_parent.clone()
            };
            if desired_parent.get_or_insert_with(|| parent.clone()) != &parent {
                return None;
            }
        }
        let desired_parent = desired_parent.unwrap_or_else(|| rewritten_parent.clone());
        if desired_parent != old_parent {
            let module = import.module.as_ref()?;
            let replacement = if import.level == 0 {
                desired_parent.as_str().to_string()
            } else {
                relative_replacement(module.as_str(), &old_parent, &desired_parent)?
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
        }
        TraversalSignal::Skip
    }
}

fn module_export_decision<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    plan: &RenamePlan,
    module: Module<'db>,
    name: &str,
    stack: &mut Vec<(Module<'db>, String)>,
) -> Decision {
    // Explicit re-exports form a graph. Follow direct definitions until their binding policy is
    // known, but reject a cycle rather than guessing which spelling it preserves.
    if stack
        .iter()
        .any(|(seen_module, seen_name)| *seen_module == module && seen_name == name)
    {
        return Err(());
    }
    stack.push((module, name.to_string()));
    let decision =
        model
            .definitions_for_module_global(module, name)
            .map_or(Err(()), |resolution| {
                definition_resolution_decision(
                    resolution.definitions(),
                    resolution.is_complete(),
                    resolution.may_be_deleted(),
                    |definition| module_export_replacement(db, plan, definition, stack),
                )
            });
    stack.pop();
    decision
}

fn module_export_replacement<'db>(
    db: &'db dyn Db,
    plan: &RenamePlan,
    definition: Definition<'db>,
    stack: &mut Vec<(Module<'db>, String)>,
) -> Result<Option<String>, ()> {
    let parsed = ruff_db::parsed::parsed_module(db, definition.python_file(db)).load(db);
    match definition.kind(db) {
        DefinitionKind::Import(import) => {
            let alias = import.alias(&parsed);
            if alias.asname.is_some() {
                return Ok(None);
            }
            let Type::ModuleLiteral(module) = binding_type(db, definition) else {
                return Ok(None);
            };
            let old = module.module(db).name(db);
            Ok(plan.rewrite(db, module.module(db)).and_then(|(_, new)| {
                let new = implicit_import_value_path(old, &new);
                (old.first_component() != new).then(|| new.to_string())
            }))
        }
        DefinitionKind::ImportFrom(import_definition) => {
            let import = import_definition.import(&parsed);
            let alias = import_definition.alias(&parsed);
            if alias.asname.is_some() {
                return Ok(None);
            }
            let definition_model = SemanticModel::new(db, definition.program_file(db));
            let parent = definition_model
                .resolve_module(
                    import.module.as_ref().map(ast::Identifier::as_str),
                    import.level,
                )
                .ok_or(())?;
            if definition_model
                .definitions_for_module_global(parent, alias.name.as_str())
                .is_some()
            {
                return module_export_decision(
                    db,
                    &definition_model,
                    plan,
                    parent,
                    alias.name.as_str(),
                    stack,
                );
            }
            let Type::ModuleLiteral(imported) = binding_type(db, definition) else {
                return Ok(None);
            };
            let old = imported.module(db).name(db);
            let new = plan.rewrite(db, imported.module(db)).map(|(_, new)| new);
            Ok(new.and_then(|new| {
                (old.last_component() != new.last_component())
                    .then(|| new.last_component().to_string())
            }))
        }
        DefinitionKind::StarImport(_) | DefinitionKind::ImportFromSubmodule(_) => Err(()),
        _ => Ok(None),
    }
}

fn relative_replacement(text: &str, old: &ModuleName, new: &ModuleName) -> Option<String> {
    let suffix = text.split('.').count();
    let old: Vec<_> = old.components().collect();
    let new: Vec<_> = new.components().collect();
    let prefix = old.len().checked_sub(suffix)?;
    (old.len() == new.len() && old[..prefix] == new[..prefix]).then(|| new[prefix..].join("."))
}

fn implicit_import_value_path<'a>(old: &ModuleName, new: &'a ModuleName) -> &'a str {
    // `import a.new` still binds `a`. If the old module was `a` itself, its references must now
    // follow the import to `a.new`; otherwise the implicit root binding remains unchanged.
    if old.parent().is_none()
        && new.parent().is_some()
        && old.first_component() == new.first_component()
    {
        new.as_str()
    } else {
        new.first_component()
    }
}

struct SemanticPass<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    plan: &'a RenamePlan,
    changes: &'a Changes<'db>,
    edits: Vec<(TextRange, String)>,
}

impl SemanticPass<'_, '_> {
    fn name(&mut self, name: &ast::ExprName) {
        if !name.ctx.is_load() {
            return;
        }
        if module_from_type(self.model, name)
            .is_none_or(|module| self.plan.rewrite(self.db, module).is_none())
        {
            return;
        }
        let Some(load) = self.model.name_load(name) else {
            return;
        };
        let mut decision = definition_resolution_decision(
            load.definitions(),
            load.is_complete(),
            load.may_be_deleted(),
            |definition| {
                if matches!(definition.kind(self.db), DefinitionKind::StarImport(_)) {
                    Err(())
                } else {
                    Ok(self.changes.get(&definition).cloned())
                }
            },
        );
        if load.crosses_scope_declaration() && matches!(decision, Ok(Some(_))) {
            decision = Err(());
        }
        self.apply(name.range, decision);
    }

    fn attribute(&mut self, attribute: &ast::ExprAttribute) -> TraversalSignal {
        if !attribute.ctx.is_load() {
            return TraversalSignal::Traverse;
        }
        let Some(module) = module_from_type(self.model, attribute) else {
            return TraversalSignal::Traverse;
        };
        let Some((rule, new)) = self.plan.rewrite(self.db, module) else {
            return TraversalSignal::Traverse;
        };
        let Some(receiver) = module_from_type(self.model, &*attribute.value) else {
            return TraversalSignal::Traverse;
        };
        let resolution = self
            .model
            .definitions_for_module_global(receiver, attribute.attr.as_str());
        if resolution.is_none() && !rule.package && rule.old_name.parent() != rule.new_name.parent()
        {
            let decision = self.module_expression_decision(attribute, &new);
            self.apply(attribute.range, decision);
            return TraversalSignal::Skip;
        }
        let decision = if let Some(resolution) = resolution {
            if resolved_source(self.db, receiver.name(self.db)).is_some_and(|runtime| {
                resolution
                    .definitions()
                    .iter()
                    .copied()
                    .any(|definition| definition.file(self.db) != runtime)
            }) {
                Err(())
            } else {
                module_export_decision(
                    self.db,
                    self.model,
                    self.plan,
                    receiver,
                    attribute.attr.as_str(),
                    &mut Vec::new(),
                )
            }
        } else {
            Ok(replace(attribute.attr.as_str(), new.last_component()))
        };
        self.apply(attribute.attr.range, decision);
        TraversalSignal::Traverse
    }

    fn module_expression_decision(
        &self,
        attribute: &ast::ExprAttribute,
        new: &ModuleName,
    ) -> Decision {
        let mut root = &*attribute.value;
        while let ast::Expr::Attribute(nested) = root {
            root = &nested.value;
        }
        let ast::Expr::Name(root) = root else {
            return Err(());
        };
        let Some(root_module) = module_from_type(self.model, root) else {
            return Err(());
        };
        let root_name = self
            .plan
            .rewrite(self.db, root_module)
            .map_or_else(|| root_module.name(self.db).clone(), |(_, name)| name);
        if new == &root_name {
            Ok(Some(root.id.to_string()))
        } else {
            new.relative_to(&root_name)
                .map(|suffix| Some(format!("{}.{}", root.id, suffix.as_str())))
                .ok_or(())
        }
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
        };
        pass.visit_expr(ast.expr());
        self.edits.extend(pass.edits);
    }

    fn apply(&mut self, range: TextRange, decision: Decision) {
        match decision {
            Ok(None) => {}
            Ok(Some(text)) => self.edits.push((range, text)),
            Err(()) => {}
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for SemanticPass<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        match node {
            AnyNodeRef::ExprName(name) if self.plan.names.contains(name.id.as_str()) => {
                self.name(name);
            }
            AnyNodeRef::ExprAttribute(attribute)
                if self.plan.names.contains(attribute.attr.as_str()) =>
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
}

fn definition_resolution_decision<'db>(
    definitions: &[Definition<'db>],
    is_complete: bool,
    may_be_deleted: bool,
    mut replacement_for: impl FnMut(Definition<'db>) -> Result<Option<String>, ()>,
) -> Decision {
    if !is_complete || may_be_deleted {
        return Err(());
    }
    let Some((first, definitions)) = definitions.split_first() else {
        return Err(());
    };
    let Ok(replacement) = replacement_for(*first) else {
        return Err(());
    };
    if definitions
        .iter()
        .copied()
        .any(|definition| replacement_for(definition) != Ok(replacement.clone()))
    {
        return Err(());
    }
    match replacement {
        Some(replacement) => Ok(Some(replacement)),
        None => Ok(None),
    }
}

type Decision = Result<Option<String>, ()>;

fn replace(old: &str, new: &str) -> Option<String> {
    if old != new {
        return Some(new.to_string());
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::PythonVersion;
    use ruff_text_size::Ranged;
    use std::collections::BTreeSet;
    use ty_project::{ProjectMetadata, TestDb};

    #[test]
    fn file_rename_contract() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            (
                "/use.py",
                "from typing import Literal\nimport pkg.old\nfrom pkg import old\nimport pkg.old as stable\nvalue: 'old.C'\nruntime = 'old.C'\nliteral: Literal['old.C']\ninvalid: 'old.C['\nprint(pkg.old.C, old.C, stable.C)\n",
            ),
        ]);
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[(
                "/use.py",
                "from typing import Literal\nimport pkg.new\nfrom pkg import new\nimport pkg.new as stable\nvalue: 'new.C'\nruntime = 'old.C'\nliteral: Literal['old.C']\ninvalid: 'old.C['\nprint(pkg.new.C, new.C, stable.C)\n",
            )],
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
                    "import pkg.new as source\nold = source\ndef outer():\n from pkg import old\n print(old.C)\n",
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
                    "import pkg.new as source\nold = source\ndef outer():\n from pkg import old\n print(old.C)\n",
                ),
            ],
        );
    }

    #[test]
    fn explicit_reexports_propagate_transitively() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            ("/facade.py", "from pkg import old\n"),
            ("/bridge.py", "from facade import old\n"),
            (
                "/use.py",
                "from bridge import old\nimport bridge\nprint(old.C, bridge.old.C)\n",
            ),
            ("/stable_facade.py", "from pkg import old as old\n"),
            ("/stable_bridge.py", "from stable_facade import old\n"),
            (
                "/stable_use.py",
                "import stable_bridge\nprint(stable_bridge.old.C)\n",
            ),
        ]);
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[
                ("/facade.py", "from pkg import new\n"),
                ("/bridge.py", "from facade import new\n"),
                (
                    "/use.py",
                    "from bridge import new\nimport bridge\nprint(new.C, bridge.new.C)\n",
                ),
                ("/stable_facade.py", "from pkg import new as old\n"),
            ],
        );
    }

    #[test]
    fn exports_are_out_of_scope() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/exports.py", "from pkg import old\n__all__ = ['old']\n"),
        ]);
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[("/exports.py", "from pkg import new\n__all__ = ['old']\n")],
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
            ("/facade.py", "import a\n"),
            ("/bare.py", "from facade import a\nprint(a.C)\n"),
            ("/qualified.py", "import facade\nprint(facade.a.C)\n"),
        ]);
        let renames = [file("/a.py", "/a/new.py")];
        assert_success(
            &db,
            &renames,
            &[
                ("/use.py", "import a.new\nprint(a.new.C, a.new)\n"),
                ("/facade.py", "import a.new\n"),
                ("/bare.py", "from facade import a\nprint(a.new.C)\n"),
                ("/qualified.py", "import facade\nprint(facade.a.new.C)\n"),
            ],
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
    fn regular_package_rename() {
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
    }

    #[test]
    fn runtime_and_stub_module_facets() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            ("/pkg/old.pyi", "class C: ...\n"),
            ("/use.py", "from pkg import old\nprint(old.C)\n"),
        ]);

        assert_file_no_edits(&db, "/pkg/old.pyi", "/pkg/new.pyi");
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[("/use.py", "from pkg import new\nprint(new.C)\n")],
        );
        assert_success(
            &db,
            &[
                file("/pkg/old.py", "/pkg/new.py"),
                file("/pkg/old.pyi", "/pkg/new.pyi"),
            ],
            &[("/use.py", "from pkg import new\nprint(new.C)\n")],
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

            assert_eq!(apply_edits(&db, &result, "/use.py"), consumer, "{name}");
        }
    }

    #[test]
    fn conflicting_runtime_and_stub_aliases_are_omitted() {
        let consumer = "import pkg\nprint(pkg.old.C)\n";
        let db = test_db(&[
            ("/pkg/__init__.py", "from . import old as old\n"),
            ("/pkg/__init__.pyi", "from . import old\n"),
            ("/pkg/old.py", "class C: ...\n"),
            ("/use.py", consumer),
        ]);
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[
                ("/pkg/__init__.py", "from . import new as old\n"),
                ("/pkg/__init__.pyi", "from . import new\n"),
            ],
        );
    }

    #[test]
    fn unsupported_semantics_are_omitted() {
        for (name, package, source, expected) in [
            (
                "qualified store",
                true,
                "import pkg.old\npkg.old = 1\n",
                "import pkg.new\npkg.old = 1\n",
            ),
            (
                "self assignment",
                true,
                "import pkg.old\npkg.old=pkg.old\n",
                "import pkg.new\npkg.old=pkg.new\n",
            ),
            (
                "assignment-backed class attribute",
                false,
                "import old as source\nclass C: old = source\nC.old.C\n",
                "import new as source\nclass C: old = source\nC.old.C\n",
            ),
            (
                "changed augmented",
                false,
                "import old\nold += 1\n",
                "import new\nold += 1\n",
            ),
            (
                "star propagation",
                true,
                "from facade import *\nprint(old.C)\n",
                "from facade import *\nprint(old.C)\n",
            ),
            (
                "deferred import",
                false,
                "def f():\n old.x\n import old\n",
                "def f():\n old.x\n import new\n",
            ),
            (
                "stale package load",
                true,
                "from pkg import old\nimport pkg\npkg.old.C\n",
                "from pkg import new\nimport pkg\npkg.old.C\n",
            ),
            (
                "conditional stable deletion",
                true,
                "from pkg import old\nif flag:\n old = 1\n del old\nprint(old)\n",
                "from pkg import new\nif flag:\n old = 1\n del old\nprint(old)\n",
            ),
        ] {
            assert_semantic_rewrite(name, package, source, expected);
        }
    }

    #[test]
    fn agreeing_place_load_definitions_are_rewritten() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ...\n"),
            (
                "/use.py",
                "from pkg import old\nclass C:\n print(old.C)\n from pkg import old\ndef outer():\n def inner(): return old.C\n from pkg import old\n return inner\ndef conditional(flag):\n if flag: from pkg import old\n return old.C\n",
            ),
        ]);
        assert_success(
            &db,
            &[file("/pkg/old.py", "/pkg/new.py")],
            &[(
                "/use.py",
                "from pkg import new\nclass C:\n print(new.C)\n from pkg import new\ndef outer():\n def inner(): return new.C\n from pkg import new\n return inner\ndef conditional(flag):\n if flag: from pkg import new\n return new.C\n",
            )],
        );
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
        assert_success(
            &db,
            &[file("/old.py", "/new.py")],
            &[(
                "/use.py",
                "import new\nnew.VALUE += 1\nnew.VALUE = 1\ndel new.VALUE\n",
            )],
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
        assert_success(
            &db,
            &[file("/pkg/target.py", "/pkg/new.py")],
            &[(
                "/use.pyi",
                "import other as target\nvalue: target.C\nfrom pkg import new\n",
            )],
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
                "import old\nimport other\nimport pkg.old\ndef affected():\n global old, pkg\n def nested(): return old.C, other.C, pkg.old.C\n return nested\ndef sibling(): return old.C, other.C, pkg.old.C\n",
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
                "import new\nimport renamed\nimport pkg.new\ndef affected():\n global old, pkg\n def nested(): return old.C, renamed.C, pkg.new.C\n return nested\ndef sibling(): return new.C, renamed.C, pkg.new.C\n",
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
            "relative import in a moved source",
            &test_db(&[
                ("/a/__init__.py", ""),
                ("/a/one/__init__.py", ""),
                ("/a/one/old.py", "from .. import x\n"),
                ("/a/x.py", ""),
                ("/b/__init__.py", ""),
            ]),
            &[
                file("/a/one/old.py", "/b/new.py"),
                file("/a/x.py", "/a/y.py"),
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
    fn conflicting_place_load_definitions_are_omitted() {
        let db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/x.py", ""),
            ("/b/__init__.py", ""),
            ("/b/x.py", ""),
            (
                "/use.py",
                "if flag: from a import x\nelse: from b import x\nprint(x)\n",
            ),
        ]);
        assert_success(
            &db,
            &[file("/a/x.py", "/a/y.py"), file("/b/x.py", "/b/z.py")],
            &[(
                "/use.py",
                "if flag: from a import y\nelse: from b import z\nprint(x)\n",
            )],
        );
    }

    #[test]
    fn candidates_and_scope_contract() {
        let db = test_db(&[
            ("/old.py", ""),
            ("/included.py", "import old\n"),
            ("/excluded.py", "import old\n"),
            ("/not_a_candidate.py", "import old\n"),
            ("/pkg/__init__.py", ""),
            ("/pkg/mod.py", ""),
            ("/package_use.py", "import pkg.mod\n"),
        ]);
        let old = system_path_to_file(&db, "/old.py").unwrap();
        let included = system_path_to_file(&db, "/included.py").unwrap();
        let excluded = system_path_to_file(&db, "/excluded.py").unwrap();
        let package_init = system_path_to_file(&db, "/pkg/__init__.py").unwrap();
        let package_use = system_path_to_file(&db, "/package_use.py").unwrap();
        let rename = [file("/old.py", "/new.py")];

        let edits = will_rename_paths(&db, &rename, [included, excluded], |file| file != excluded);
        assert!(edits.iter().all(|edit| edit.range.file() == included));
        assert_eq!(apply_edits(&db, &edits, "/included.py"), "import new\n");

        assert!(will_rename_paths(&db, &rename, [included], |file| file != old).is_empty());
        assert!(
            will_rename_paths(
                &db,
                &[PathRename::directory("/pkg".into(), "/new_pkg".into())],
                [package_use],
                |file| file != package_init,
            )
            .is_empty()
        );
    }

    #[test]
    fn unsupported_package_renames_are_omitted() {
        let db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/pkg/__init__.py", ""),
            ("/b/__init__.py", ""),
            ("/cross_parent.py", "import a.pkg\n"),
            ("/ns/mod.py", ""),
            ("/implicit.py", "import ns.mod\n"),
            (
                "/legacy/__init__.py",
                "__import__('pkg_resources').declare_namespace(__name__)\n",
            ),
            ("/legacy/mod.py", ""),
            ("/legacy_use.py", "import legacy.mod\n"),
        ]);
        assert_no_edits(
            "cross-parent regular package",
            &db,
            &[PathRename::directory("/a/pkg".into(), "/b/pkg".into())],
        );
        assert_no_edits(
            "implicit namespace package",
            &db,
            &[PathRename::directory("/ns".into(), "/new_ns".into())],
        );
        assert_no_edits(
            "legacy namespace package",
            &db,
            &[PathRename::directory(
                "/legacy".into(),
                "/new_legacy".into(),
            )],
        );
    }

    #[test]
    fn unsupported_rules_do_not_suppress_independent_edits() {
        let db = test_db(&[
            ("/unsupported.py", ""),
            ("/old.py", ""),
            ("/use.py", "import old\n"),
        ]);
        assert_success(
            &db,
            &[
                file("/unsupported.py", "/unsupported.pyi"),
                file("/old.py", "/new.py"),
            ],
            &[("/use.py", "import new\n")],
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
        let edits = will_rename_paths(db, renames, &db.project().files(db), |_| true);
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
            will_rename_paths(db, renames, &db.project().files(db), |_| true).is_empty(),
            "{name}"
        );
    }

    fn assert_semantic_rewrite(name: &str, package: bool, source: &str, expected: &str) {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/old.py", ""),
            ("/facade.py", "from pkg import old\n"),
            ("/main.py", source),
        ]);
        let rename = if package {
            file("/pkg/old.py", "/pkg/new.py")
        } else {
            file("/old.py", "/new.py")
        };
        let mut expected_files = Vec::new();
        if source != expected {
            expected_files.push(("/main.py", expected));
        }
        if package {
            expected_files.push(("/facade.py", "from pkg import new\n"));
        }
        assert_success_named(name, &db, &[rename], &expected_files);
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
