//! Computes source edits for Python module file renames.
//!
//! [`will_rename_files`] maps filesystem renames to module names, then rewrites imports and uses in
//! the candidate files supplied by the caller. It does not move files, discover package contents,
//! validate the filesystem operation, or normalize the returned edits.
//!
//! A `.py` or `.pyi` module can move when it keeps its extension and is not a package initializer.
//! Runtime files and stubs share one module identity. When both exist, the runtime file determines
//! the rename; moving only its stub produces no edits. Moving both produces one set of edits.
//! Directory renames are unsupported.
//!
//! Import syntax determines whether a local name changes. An explicit `as` alias remains fixed;
//! an unaliased import changes its binding and uses that resolve to that binding. Unaliased re-exports
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
use ruff_db::files::{File, FileRange};
use ruff_db::source::source_text;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};
use ty_module_resolver::{
    ImportingFile, Module, ModuleName, ModuleResolveMode, ResolverEnvironment, ResolverFile,
    file_to_module, resolve_module_confident, resolve_real_module_confident, search_paths,
};
use ty_project::{Db, parallel::ParallelIteratorExt};
use ty_python_core::ProgramFile;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_semantic::types::Type;
use ty_python_semantic::{
    DefinitionResolution, HasType, InferredNameLoads, NameLoadInference, SemanticModel,
};

/// Computes source edits for a batch of Python file renames.
///
/// Call this before moving the files. `files` must include every source the caller wants analyzed,
/// including moved sources. The database must have place-load recording enabled.
/// `in_scope` restricts both the rename sources and the files that can be edited.
///
/// Edits refer to the original files and source ranges. Unsupported or ambiguous occurrences are
/// omitted, so a nonempty result does not imply that every reference was updated. The caller must
/// sort the edits and handle duplicates or overlaps before applying them.
pub fn will_rename_files(
    db: &dyn Db,
    renames: &[FileRename],
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

/// One Python file rename in a batch.
pub struct FileRename {
    /// The source file before the rename.
    pub file: File,
    /// The destination path, which need not exist yet.
    pub new_path: SystemPathBuf,
}

/// A replacement and the file range containing it.
pub type FileRenameEdit = RangedValue<String>;

struct RenamePlan {
    rules: Vec<RenameRule>,
    names: FxHashSet<String>,
}

impl RenamePlan {
    fn new(db: &dyn Db, renames: &[FileRename], in_scope: &impl Fn(File) -> bool) -> Self {
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

    fn replacement(&self, name: &ModuleName) -> Option<&ModuleName> {
        let index = self
            .rules
            .binary_search_by(|rule| rule.old_name.cmp(name))
            .ok()?;
        Some(&self.rules[index].new_name)
    }

    fn affects(&self, name: &ModuleName) -> bool {
        name.ancestors()
            .any(|name| self.replacement(&name).is_some())
    }
}

struct RenameRule {
    old_name: ModuleName,
    new_name: ModuleName,
}

impl RenameRule {
    fn new(db: &dyn Db, rename: &FileRename, in_scope: &impl Fn(File) -> bool) -> Option<Self> {
        let resolver_environment = resolver_environment(db);
        let file = rename.file;
        let old = file.path(db).as_system_path()?;
        let new = SystemPath::absolute(&rename.new_path, db.system().current_directory());
        let extension = old.extension()?;
        (matches!(extension, "py" | "pyi")
            && new.extension() == Some(extension)
            && !matches!(old.file_stem(), Some("__init__"))
            && !matches!(new.file_stem(), Some("__init__")))
        .then_some(())?;
        in_scope(file).then_some(())?;
        let old_name = file_to_module(db, ResolverFile::new(db, file, resolver_environment))?
            .name(db)
            .clone();
        // A runtime module and its stub share an import name. Renaming only the stub
        // must not redirect imports while the runtime module remains at its old path.
        (resolved_source(db, &old_name)? == file).then_some(())?;
        Some(Self {
            old_name,
            new_name: prospective_module(db, &new)?,
        })
    }
}

/// Derives a destination module name without requiring the destination to exist yet.
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

fn edits_for_file(db: &dyn Db, file: File, plan: &RenamePlan) -> Vec<FileRenameEdit> {
    let program_file = db.program_file(file);
    let source = source_text(db, file);
    if source.read_error().is_some() {
        return Vec::new();
    }
    // Non-ASCII identifiers can normalize to a different spelling in the AST. A text search
    // can rule out a candidate only when both the source and the searched names are ASCII.
    if source.as_str().is_ascii()
        && plan.names.iter().all(|name| name.is_ascii())
        && plan
            .names
            .iter()
            .all(|name| !source.as_str().contains(name))
    {
        return Vec::new();
    }
    let module = ruff_db::parsed::parsed_module(db, program_file.python_file(db)).load(db);
    let root = AnyNodeRef::from(module.syntax());
    let model = SemanticModel::new(db, program_file);
    // Determine which imports can be rewritten and which local bindings those edits rename.
    // Later name edits depend on the binding rewrites recorded here.
    let mut imports = ImportPass {
        analyzer: ImportAnalyzer::new(db, &model, plan),
        output: ImportAnalysis::default(),
    };
    root.visit_source_order(&mut imports);
    let ImportAnalysis {
        mut edits,
        definition_rewrites,
    } = imports.output;
    // Collect names from the file and valid string annotations before inferring their scopes.
    let mut name_load_inference = model.name_load_inference();
    root.visit_source_order(&mut NameLoadCollector {
        model: &model,
        plan,
        inference: &mut name_load_inference,
    });
    let name_loads = name_load_inference.finish();
    // Use the recorded definitions to distinguish references from unrelated names with the same
    // spelling, then combine those edits with the import edits.
    let mut semantics = SemanticPass {
        db,
        model: &model,
        plan,
        definition_rewrites: &definition_rewrites,
        name_loads: &name_loads,
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

type DefinitionRewrites<'db> = FxHashMap<Definition<'db>, String>;

#[derive(Default)]
struct ImportAnalysis<'db> {
    edits: Vec<(TextRange, String)>,
    definition_rewrites: DefinitionRewrites<'db>,
}

impl<'db> ImportAnalysis<'db> {
    fn extend(&mut self, other: Self) {
        self.edits.extend(other.edits);
        self.definition_rewrites.extend(other.definition_rewrites);
    }

    fn add_alias(&mut self, alias: ImportAliasAnalysis<'db>) {
        if let Some(edit) = alias.edit {
            self.edits.push(edit);
        }
        if let Some((definition, replacement)) = alias.definition_rewrite {
            self.definition_rewrites.insert(definition, replacement);
        }
    }
}

struct ImportAliasAnalysis<'db> {
    parent: ModuleName,
    edit: Option<(TextRange, String)>,
    definition_rewrite: Option<(Definition<'db>, String)>,
}

struct ImportPass<'a, 'db> {
    analyzer: ImportAnalyzer<'a, 'db>,
    output: ImportAnalysis<'db>,
}

impl<'a> SourceOrderVisitor<'a> for ImportPass<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        let output = match node {
            AnyNodeRef::StmtImport(import) => self.analyzer.import(import),
            AnyNodeRef::StmtImportFrom(import) => self
                .analyzer
                .import_from(import, &mut ExportAnalyzer::default()),
            _ => return TraversalSignal::Traverse,
        };
        if let Some(output) = output {
            self.output.extend(output);
        }
        TraversalSignal::Skip
    }
}

/// Plans one complete import statement and records the local bindings its edits change.
struct ImportAnalyzer<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    plan: &'a RenamePlan,
    moves_across_parent: bool,
}

impl<'a, 'db> ImportAnalyzer<'a, 'db> {
    fn new(db: &'db dyn Db, model: &'a SemanticModel<'db>, plan: &'a RenamePlan) -> Self {
        let moves_across_parent = file_to_module(db, model.program_file().resolver_file(db))
            .is_some_and(|module| {
                plan.replacement(module.name(db))
                    .is_some_and(|new| module.name(db).parent() != new.parent())
            });
        Self {
            db,
            model,
            plan,
            moves_across_parent,
        }
    }

    fn definition_rewrite(
        &self,
        alias: &ast::Alias,
        old: &str,
        new: &str,
    ) -> Option<(Definition<'db>, String)> {
        if alias.asname.is_some() || old == new {
            return None;
        }
        let definition = ty_python_core::semantic_index(self.db, self.model.program_file())
            .expect_single_definition(alias);
        Some((definition, new.to_string()))
    }

    fn import(&self, import: &ast::StmtImport) -> Option<ImportAnalysis<'db>> {
        let mut output = ImportAnalysis::default();
        for alias in &import.names {
            let written = ModuleName::new(alias.name.as_str())?;
            let Some(module) = self.model.resolve_module(Some(alias.name.as_str()), 0) else {
                if self.plan.affects(&written) {
                    return None;
                }
                continue;
            };
            let Some(new) = self.plan.replacement(module.name(self.db)) else {
                continue;
            };
            let old = module.name(self.db);
            if alias.asname.is_none()
                && old.parent() != new.parent()
                && old.first_component() != new.first_component()
            {
                return None;
            }
            let new_binding = implicit_import_value_path(old, new);
            if let Some((definition, replacement)) =
                self.definition_rewrite(alias, old.first_component(), new_binding)
            {
                output.definition_rewrites.insert(definition, replacement);
            }
            if alias.name.as_str() != new.as_str() {
                output
                    .edits
                    .push((alias.name.range, new.as_str().to_string()));
            }
        }
        Some(output)
    }

    fn import_from(
        &self,
        import: &ast::StmtImportFrom,
        exports: &mut ExportAnalyzer<'db>,
    ) -> Option<ImportAnalysis<'db>> {
        let mut output = ImportAnalysis::default();
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
        if resolved_parent.is_none() && self.plan.affects(&old_parent) {
            return None;
        }
        let rewritten_parent = resolved_parent
            .and_then(|module| self.plan.replacement(module.name(self.db)))
            .unwrap_or(&old_parent);
        // A `from` statement has one module path shared by all aliases. If their destinations
        // disagree, leave the whole statement unchanged instead of partially rewriting it.
        let mut desired_parent = None;
        for alias in &import.names {
            let analysis = self.import_from_alias(
                alias,
                &old_parent,
                resolved_parent,
                rewritten_parent,
                exports,
            )?;
            if desired_parent.get_or_insert_with(|| analysis.parent.clone()) != &analysis.parent {
                return None;
            }
            output.add_alias(analysis);
        }
        let desired_parent = desired_parent.as_ref().unwrap_or(rewritten_parent);
        if desired_parent != &old_parent {
            let module = import.module.as_ref()?;
            let replacement = if import.level == 0 {
                desired_parent.as_str().to_string()
            } else {
                relative_replacement(module.as_str(), &old_parent, desired_parent)?
            };
            if replacement == module.as_str() {
                return None;
            }
            output.edits.push((module.range, replacement));
        }
        Some(output)
    }

    fn import_from_alias(
        &self,
        alias: &ast::Alias,
        old_parent: &ModuleName,
        resolved_parent: Option<Module<'db>>,
        rewritten_parent: &ModuleName,
        exports: &mut ExportAnalyzer<'db>,
    ) -> Option<ImportAliasAnalysis<'db>> {
        let imported = module_from_type(self.model, alias);
        let direct = resolved_parent.is_some_and(|parent| {
            imported.is_some_and(|module| {
                let name = module.name(self.db);
                alias.name.as_str() == name.last_component()
                    && name.parent().as_ref() == Some(old_parent)
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
        let mut edit = None;
        let mut definition_rewrite = None;
        let parent = if direct {
            let module = imported?;
            let old = module.name(self.db);
            if let Some(new) = self.plan.replacement(old) {
                if alias.name.as_str() != old.last_component()
                    || old.parent().as_ref() != Some(old_parent)
                {
                    return None;
                }
                definition_rewrite =
                    self.definition_rewrite(alias, old.last_component(), new.last_component());
                if alias.name.as_str() != new.last_component() {
                    edit = Some((alias.name.range, new.last_component().to_string()));
                }
                new.parent()?
            } else if old.parent().as_ref() == Some(old_parent) {
                old_parent.to_owned()
            } else {
                rewritten_parent.to_owned()
            }
        } else {
            if self.plan.names.contains(alias.name.as_str()) {
                let parent = resolved_parent?;
                match exports.module(self.db, self.model, self.plan, parent, alias.name.as_str()) {
                    RewriteDecision::Preserve => {}
                    RewriteDecision::Replace(new) => {
                        let changes_value_path = new.contains('.');
                        if changes_value_path && alias.asname.is_some() {
                            return None;
                        }
                        definition_rewrite =
                            self.definition_rewrite(alias, alias.name.as_str(), &new);
                        if !changes_value_path {
                            edit = Some((alias.name.range, new));
                        }
                    }
                    RewriteDecision::Omit => return None,
                }
            }
            rewritten_parent.to_owned()
        };
        Some(ImportAliasAnalysis {
            parent,
            edit,
            definition_rewrite,
        })
    }
}

/// Shares complete import plans while following re-exports, rejecting cycles.
#[derive(Default)]
struct ExportAnalyzer<'db> {
    // `None` marks an import currently being visited.
    imports: FxHashMap<(ProgramFile<'db>, TextRange), Option<DefinitionRewrites<'db>>>,
    has_cycle: bool,
}

impl<'db> ExportAnalyzer<'db> {
    fn module(
        &mut self,
        db: &'db dyn Db,
        model: &SemanticModel<'db>,
        plan: &RenamePlan,
        module: Module<'db>,
        name: &str,
    ) -> RewriteDecision {
        let mut analyze = |module| {
            model.definitions_for_module_global(module, name).map_or(
                RewriteDecision::Omit,
                |resolution| {
                    rewrite_for_resolution(&resolution, |definition| {
                        self.definition(db, plan, definition)
                    })
                },
            )
        };
        let decision = analyze(module);
        if decision == RewriteDecision::Omit {
            return decision;
        }
        // A stub can expose a different binding. Propagate only changes both facets agree on.
        if let Some(runtime) =
            resolve_real_module_confident(db, resolver_environment(db), module.name(db))
            && runtime.file(db) != module.file(db)
            && analyze(runtime) != decision
        {
            return RewriteDecision::Omit;
        }
        decision
    }

    fn definition(
        &mut self,
        db: &'db dyn Db,
        plan: &RenamePlan,
        definition: Definition<'db>,
    ) -> RewriteDecision {
        let parsed = ruff_db::parsed::parsed_module(db, definition.python_file(db)).load(db);
        let model = SemanticModel::new(db, definition.program_file(db));
        match definition.kind(db) {
            DefinitionKind::Import(import) => {
                let import = import.import(&parsed);
                self.import(db, definition, import.range(), |_| {
                    ImportAnalyzer::new(db, &model, plan).import(import)
                })
            }
            DefinitionKind::ImportFrom(import) => {
                let import = import.import(&parsed);
                self.import(db, definition, import.range(), |exports| {
                    ImportAnalyzer::new(db, &model, plan).import_from(import, exports)
                })
            }
            DefinitionKind::StarImport(_) | DefinitionKind::ImportFromSubmodule(_) => {
                RewriteDecision::Omit
            }
            _ => RewriteDecision::Preserve,
        }
    }

    fn import(
        &mut self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        range: TextRange,
        analyze: impl FnOnce(&mut Self) -> Option<ImportAnalysis<'db>>,
    ) -> RewriteDecision {
        if self.has_cycle {
            return RewriteDecision::Omit;
        }
        let key = (definition.program_file(db), range);
        if let Some(rewrites) = self.imports.get(&key) {
            return match rewrites {
                Some(rewrites) => rewrites
                    .get(&definition)
                    .map_or(RewriteDecision::Preserve, |new| {
                        RewriteDecision::Replace(new.clone())
                    }),
                None => {
                    self.has_cycle = true;
                    RewriteDecision::Omit
                }
            };
        }
        self.imports.insert(key, None);
        // A rejected statement leaves all its bindings unchanged. Cache the complete binding
        // map so looking up another name from this import does not plan the statement again.
        let rewrites = analyze(self)
            .map(|analysis| analysis.definition_rewrites)
            .unwrap_or_default();
        // Reject the whole traversal: a cycle can prevent an import rewrite, but that does not
        // establish that its exported binding stays unchanged.
        if self.has_cycle {
            return RewriteDecision::Omit;
        }
        let decision = rewrites
            .get(&definition)
            .map_or(RewriteDecision::Preserve, |new| {
                RewriteDecision::Replace(new.clone())
            });
        self.imports.insert(key, Some(rewrites));
        decision
    }
}

/// Rewrites the written suffix of a relative import only if its implicit prefix stays unchanged.
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

struct NameLoadCollector<'a, 'db> {
    model: &'a SemanticModel<'db>,
    plan: &'a RenamePlan,
    inference: &'a mut NameLoadInference<'db>,
}

impl NameLoadCollector<'_, '_> {
    fn string(&mut self, string: &ast::ExprStringLiteral) {
        let Some((ast, model)) = self.model.enter_string_annotation(string) else {
            return;
        };
        let mut collector = NameLoadCollector {
            model: &model,
            plan: self.plan,
            inference: self.inference,
        };
        collector.visit_expr(ast.expr());
    }
}

impl<'a> SourceOrderVisitor<'a> for NameLoadCollector<'_, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        match node {
            AnyNodeRef::ExprName(name)
                if name.ctx.is_load() && self.plan.names.contains(name.id.as_str()) =>
            {
                self.inference.extend(self.model, [name]);
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

struct SemanticPass<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    plan: &'a RenamePlan,
    definition_rewrites: &'a DefinitionRewrites<'db>,
    name_loads: &'a InferredNameLoads<'db>,
    edits: Vec<(TextRange, String)>,
}

impl SemanticPass<'_, '_> {
    fn name(&mut self, name: &ast::ExprName) {
        if !name.ctx.is_load() {
            return;
        }
        let Some(load) = self.name_loads.get(name) else {
            return;
        };
        let resolution = load.resolution();
        let mut decision = rewrite_for_resolution(resolution, |definition| {
            if matches!(definition.kind(self.db), DefinitionKind::StarImport(_)) {
                RewriteDecision::Omit
            } else if let Some(replacement) = self.definition_rewrites.get(&definition) {
                RewriteDecision::Replace(replacement.clone())
            } else {
                RewriteDecision::Preserve
            }
        });
        if resolution.crosses_scope_declaration() && matches!(decision, RewriteDecision::Replace(_))
        {
            decision = RewriteDecision::Omit;
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
        let Some(new) = self.plan.replacement(module.name(self.db)) else {
            return TraversalSignal::Traverse;
        };
        let Some(receiver) = module_from_type(self.model, &*attribute.value) else {
            return TraversalSignal::Traverse;
        };
        let resolution = self
            .model
            .definitions_for_module_global(receiver, attribute.attr.as_str());
        if resolution.is_none() && module.name(self.db).parent() != new.parent() {
            let decision = self.module_expression_decision(attribute, new);
            self.apply(attribute.range, decision);
            return TraversalSignal::Skip;
        }
        let decision = if resolution.is_some() {
            ExportAnalyzer::default().module(
                self.db,
                self.model,
                self.plan,
                receiver,
                attribute.attr.as_str(),
            )
        } else {
            replace(attribute.attr.as_str(), new.last_component())
        };
        self.apply(attribute.attr.range, decision);
        TraversalSignal::Traverse
    }

    fn module_expression_decision(
        &self,
        attribute: &ast::ExprAttribute,
        new: &ModuleName,
    ) -> RewriteDecision {
        let mut root = &*attribute.value;
        while let ast::Expr::Attribute(nested) = root {
            root = &nested.value;
        }
        let ast::Expr::Name(root) = root else {
            return RewriteDecision::Omit;
        };
        let Some(root_module) = module_from_type(self.model, root) else {
            return RewriteDecision::Omit;
        };
        let root_name = self
            .plan
            .replacement(root_module.name(self.db))
            .unwrap_or_else(|| root_module.name(self.db));
        if new == root_name {
            RewriteDecision::Replace(root.id.to_string())
        } else {
            new.relative_to(root_name)
                .map_or(RewriteDecision::Omit, |suffix| {
                    RewriteDecision::Replace(format!("{}.{}", root.id, suffix.as_str()))
                })
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
            definition_rewrites: self.definition_rewrites,
            name_loads: self.name_loads,
            edits: Vec::new(),
        };
        pass.visit_expr(ast.expr());
        self.edits.extend(pass.edits);
    }

    fn apply(&mut self, range: TextRange, decision: RewriteDecision) {
        if let RewriteDecision::Replace(text) = decision {
            self.edits.push((range, text));
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

/// Chooses a rewrite only when all reachable definitions agree on the replacement or preservation.
///
/// Incomplete resolution or a reachable deletion prevents a rewrite. Possible unboundness alone
/// does not: a conditional import can still establish the same replacement wherever it is bound.
fn rewrite_for_resolution<'db>(
    resolution: &DefinitionResolution<'db>,
    mut rewrite_for: impl FnMut(Definition<'db>) -> RewriteDecision,
) -> RewriteDecision {
    if !resolution.is_complete() || resolution.may_be_deleted() {
        return RewriteDecision::Omit;
    }
    let Some((first, definitions)) = resolution.definitions().split_first() else {
        return RewriteDecision::Omit;
    };
    let rewrite = rewrite_for(*first);
    if rewrite == RewriteDecision::Omit {
        return rewrite;
    }
    if definitions
        .iter()
        .copied()
        .any(|definition| rewrite_for(definition) != rewrite)
    {
        return RewriteDecision::Omit;
    }
    rewrite
}

#[derive(Eq, PartialEq)]
enum RewriteDecision {
    /// The definition keeps its current spelling, for example because it has an explicit alias.
    Preserve,
    /// The definition requires this replacement wherever the name refers to it.
    Replace(String),
    /// Resolution cannot establish a rewrite, so this occurrence is left unchanged.
    Omit,
}

fn replace(old: &str, new: &str) -> RewriteDecision {
    if old != new {
        return RewriteDecision::Replace(new.to_string());
    }
    RewriteDecision::Preserve
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
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::PythonVersion;
    use ruff_text_size::Ranged;
    use std::collections::BTreeSet;
    use ty_project::{ProjectMetadata, TestDb};
    use ty_python_semantic::PlaceLoadRecordingMode;

    #[test]
    fn file_rename_contract() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", "class C: ..."),
            (
                "/use.py",
                "import pkg.old
from pkg import old
import pkg.old as stable
value: 'old.C'
runtime = 'old.C'
print(old, stable)
pkg.old.VALUE = 1
pkg.old = pkg.old
",
            ),
        ]);
        assert_success(
            &db,
            &[("/pkg/old.py", "/pkg/new.py")],
            &[(
                "/use.py",
                "import pkg.new
from pkg import new
import pkg.new as stable
value: 'new.C'
runtime = 'old.C'
print(new, stable)
pkg.new.VALUE = 1
pkg.old = pkg.new
",
            )],
        );
    }

    #[test]
    fn unicode_identifier_prefilter() {
        let db = test_db(&[
            ("/K·b.py", ""),
            (
                "/use.py",
                "import \u{212a}·b
print(\u{212a}·b)
",
            ),
        ]);
        assert_success(
            &db,
            &[("/K·b.py", "/new.py")],
            &[(
                "/use.py",
                "import new
print(new)
",
            )],
        );
    }

    #[test]
    fn explicit_reexports_propagate_transitively() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/facade.py", "from pkg import old"),
            ("/bridge.py", "from facade import old"),
            ("/stable.py", "from pkg import old as old"),
            (
                "/use.py",
                "from bridge import old
import bridge, stable
print(old, bridge.old, stable.old)",
            ),
            (
                "/star.py",
                "from bridge import *
old",
            ),
        ]);
        assert_success(
            &db,
            &[("/pkg/old.py", "/pkg/new.py")],
            &[
                ("/facade.py", "from pkg import new"),
                ("/bridge.py", "from facade import new"),
                ("/stable.py", "from pkg import new as old"),
                (
                    "/use.py",
                    "from bridge import new
import bridge, stable
print(new, bridge.new, stable.old)",
                ),
            ],
        );
    }

    #[test]
    fn representable_cross_parent_file_move() {
        let db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/one/__init__.py", ""),
            ("/a/one/old.py", ""),
            (
                "/use.py",
                "import a.one.old
from a.one import old
print(a.one.old, old)",
            ),
        ]);
        assert_success(
            &db,
            &[("/a/one/old.py", "/a/two/new.py")],
            &[(
                "/use.py",
                "import a.two.new
from a.two import new
print(a.two.new, new)",
            )],
        );
    }

    #[test]
    fn root_module_can_move_to_submodule() {
        let db = test_db(&[
            ("/a.py", ""),
            ("/a/placeholder.txt", ""),
            (
                "/use.py",
                "import a
print(a)
",
            ),
            ("/facade.py", "import a\n"),
            (
                "/bare.py",
                "from facade import a
print(a)
",
            ),
            (
                "/qualified.py",
                "import facade
print(facade.a)
",
            ),
        ]);
        let renames = [("/a.py", "/a/new.py")];
        assert_success(
            &db,
            &renames,
            &[
                (
                    "/use.py",
                    "import a.new
print(a.new)
",
                ),
                ("/facade.py", "import a.new\n"),
                (
                    "/bare.py",
                    "from facade import a
print(a.new)
",
                ),
                (
                    "/qualified.py",
                    "import facade
print(facade.a.new)
",
                ),
            ],
        );
    }

    #[test]
    fn module_expression_can_collapse_to_root() {
        let db = test_db(&[
            ("/a/old.py", ""),
            (
                "/use.py",
                "import a.old
print(a.old)
",
            ),
        ]);
        assert_success(
            &db,
            &[("/a/old.py", "/a.py")],
            &[(
                "/use.py",
                "import a
print(a)
",
            )],
        );
    }

    #[test]
    fn runtime_and_stub_module_facets() {
        let db = test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old.py", ""),
            ("/pkg/old.pyi", ""),
            (
                "/use.py",
                "from pkg import old
print(old)
",
            ),
        ]);

        assert_file_no_edits(&db, "/pkg/old.pyi", "/pkg/new.pyi");
        assert_success(
            &db,
            &[("/pkg/old.py", "/pkg/new.py")],
            &[(
                "/use.py",
                "from pkg import new
print(new)
",
            )],
        );
        assert_success(
            &db,
            &[
                ("/pkg/old.py", "/pkg/new.py"),
                ("/pkg/old.pyi", "/pkg/new.pyi"),
            ],
            &[(
                "/use.py",
                "from pkg import new
print(new)
",
            )],
        );
    }

    #[test]
    fn conflicting_runtime_and_stub_aliases_are_omitted() {
        let consumer = "import pkg
print(pkg.old)
";
        let db = test_db(&[
            ("/pkg/__init__.py", "from . import old as old\n"),
            ("/pkg/__init__.pyi", "from . import old\n"),
            ("/pkg/old.py", ""),
            ("/use.py", consumer),
        ]);
        assert_success(
            &db,
            &[("/pkg/old.py", "/pkg/new.py")],
            &[
                ("/pkg/__init__.py", "from . import new as old\n"),
                ("/pkg/__init__.pyi", "from . import new\n"),
            ],
        );
    }

    #[test]
    fn possibly_unbound_place_loads_are_rewritten() {
        let db = test_db(&[
            ("/old.py", ""),
            (
                "/use.py",
                "def f(flag):
    if flag:
        import old
    return old",
            ),
        ]);
        assert_success(
            &db,
            &[("/old.py", "/new.py")],
            &[(
                "/use.py",
                "def f(flag):
    if flag:
        import new
    return new",
            )],
        );
    }

    #[test]
    fn scope_declarations_omit_only_dependent_occurrences() {
        let db = test_db(&[
            ("/old.py", ""),
            (
                "/use.py",
                "import old
def affected():
    global old
    return old
def sibling():
    return old",
            ),
        ]);
        assert_success(
            &db,
            &[("/old.py", "/new.py")],
            &[(
                "/use.py",
                "import new
def affected():
    global old
    return old
def sibling():
    return new",
            )],
        );
    }

    #[test]
    fn unsupported_requests_and_imports_are_omitted() {
        let mut db = test_db(&[
            ("/a/__init__.py", ""),
            ("/a/old.py", ""),
            ("/b/__init__.py", ""),
            ("/use.py", "import a.old\n"),
        ]);
        assert_file_no_edits(&db, "/a/old.py", "/b/new.py");
        db.write_file("/use.py", "import a.old.missing\n").unwrap();
        assert_file_no_edits(&db, "/a/old.py", "/a/new.py");
        assert_file_no_edits(&db, "/a/__init__.py", "/a/new.py");
        assert_no_edits(
            "relative import in a moved source",
            &test_db(&[
                ("/a/__init__.py", ""),
                ("/a/one/__init__.py", ""),
                ("/a/one/old.py", "from .. import x\n"),
                ("/a/x.py", ""),
                ("/b/__init__.py", ""),
            ]),
            &[("/a/one/old.py", "/b/new.py"), ("/a/x.py", "/a/y.py")],
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
                "from a import old, sibling
from a import x
print(old, x)
",
            ),
        ]);
        assert_success(
            &db,
            &[("/a/old.py", "/b/new.py"), ("/a/x.py", "/a/y.py")],
            &[(
                "/use.py",
                "from a import old, sibling
from a import y
print(old, y)
",
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
                "if flag: from a import x
else: from b import x
print(x)
",
            ),
        ]);
        assert_success(
            &db,
            &[("/a/x.py", "/a/y.py"), ("/b/x.py", "/b/z.py")],
            &[(
                "/use.py",
                "if flag: from a import y
else: from b import z
print(x)
",
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
        ]);
        let old = system_path_to_file(&db, "/old.py").unwrap();
        let included = system_path_to_file(&db, "/included.py").unwrap();
        let excluded = system_path_to_file(&db, "/excluded.py").unwrap();
        let rename = file_renames(&db, &[("/old.py", "/new.py")]);

        let edits = will_rename_files(&db, &rename, [included, excluded], |file| file != excluded);
        assert!(edits.iter().all(|edit| edit.range.file() == included));
        assert_eq!(apply_edits(&db, &edits, "/included.py"), "import new\n");

        assert!(will_rename_files(&db, &rename, [included], |file| file != old).is_empty());
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
                ("/unsupported.py", "/unsupported.pyi"),
                ("/old.py", "/new.py"),
            ],
            &[("/use.py", "import new\n")],
        );
    }

    fn file_renames(db: &TestDb, paths: &[(&str, &str)]) -> Vec<FileRename> {
        paths
            .iter()
            .map(|&(old, new)| FileRename {
                file: system_path_to_file(db, old).unwrap(),
                new_path: new.into(),
            })
            .collect()
    }

    #[track_caller]
    fn assert_file_no_edits(db: &TestDb, old: &str, new: &str) {
        assert_no_edits(old, db, &[(old, new)]);
    }

    #[track_caller]
    fn assert_success(db: &TestDb, renames: &[(&str, &str)], expected: &[(&str, &str)]) {
        let renames = file_renames(db, renames);
        let edits = will_rename_files(db, &renames, &db.project().files(db), |_| true);
        let actual: BTreeSet<_> = edits.iter().map(|edit| edit.range.file()).collect();
        let expected_files: BTreeSet<_> = expected
            .iter()
            .map(|(path, _)| system_path_to_file(db, *path).unwrap())
            .collect();
        assert_eq!(actual, expected_files);
        for &(path, contents) in expected {
            assert_eq!(apply_edits(db, &edits, path), contents, "{path}");
        }
    }

    #[track_caller]
    fn assert_no_edits(name: &str, db: &TestDb, renames: &[(&str, &str)]) {
        let renames = file_renames(db, renames);
        let edits = will_rename_files(db, &renames, &db.project().files(db), |_| true);
        assert!(edits.is_empty(), "{name}: {edits:?}");
    }

    fn test_db(files: &[(&str, &str)]) -> TestDb {
        let mut db = TestDb::with_place_load_recording_mode(
            ProjectMetadata::new("test", "/".into()),
            PlaceLoadRecordingMode::Enabled,
        );
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
