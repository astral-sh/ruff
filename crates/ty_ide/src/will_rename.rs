use crate::references::contains_identifier;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_db::system::SystemPath;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashMap;
use ty_module_resolver::{Module, ModuleName, file_to_module, resolve_module_confident};
use ty_project::Db;
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

/// A text edit to apply when renaming a file.
#[derive(Debug, Clone)]
pub struct FileRenameEdit {
    pub file: File,
    pub range: TextRange,
    pub new_text: String,
}

/// A mapping from each renamed module's old name to its new name.
///
/// A single entry covers the renamed module *and* all of its submodules: a
/// reference to `old.sub` is remapped by matching the longest renamed ancestor
/// (see [`remap_module_name`]).
type RenameMap = FxHashMap<ModuleName, ModuleName>;

/// Compute the edits needed when renaming a single Python file.
pub fn will_rename_file(
    db: &dyn Db,
    old_path: &SystemPath,
    new_path: &SystemPath,
) -> Vec<FileRenameEdit> {
    will_rename(db, &[(old_path, new_path)])
}

/// Compute the edits needed when renaming a directory (package).
pub fn will_rename_directory(
    db: &dyn Db,
    old_dir: &SystemPath,
    new_dir: &SystemPath,
) -> Vec<FileRenameEdit> {
    will_rename(db, &[(old_dir, new_dir)])
}

/// Compute the edits needed for a batch of file/directory renames.
///
/// Resolves every rename to a module-name mapping, then walks each project file
/// exactly once, building edits during an AST walk. Working from the AST (rather
/// than find-references) lets us inspect the surrounding import/attribute syntax
/// and use the semantic model to confirm that a name actually refers to a module
/// being moved, which is what makes cross-package renames and aliases correct.
pub fn will_rename(db: &dyn Db, renames: &[(&SystemPath, &SystemPath)]) -> Vec<FileRenameEdit> {
    let map = build_rename_map(db, renames);
    if map.is_empty() {
        return Vec::new();
    }

    let mut edits = Vec::new();
    let files = db.project().files(db);
    for file in &files {
        collect_file_edits(db, file, &map, &mut edits);
    }
    edits
}

/// Resolve each `(old, new)` path pair to an `old -> new` module-name entry.
fn build_rename_map(db: &dyn Db, renames: &[(&SystemPath, &SystemPath)]) -> RenameMap {
    let mut map = RenameMap::default();
    for (old, new) in renames {
        if let Some((old_name, new_name)) = resolve_rename(db, old, new) {
            map.insert(old_name, new_name);
        }
    }
    map
}

/// Resolve a single rename to its `(old_name, new_name)` module names.
///
/// A path that resolves to a file is treated as a module rename; otherwise it is
/// treated as a directory (package) rename.
fn resolve_rename(
    db: &dyn Db,
    old: &SystemPath,
    new: &SystemPath,
) -> Option<(ModuleName, ModuleName)> {
    if let Ok(old_file) = system_path_to_file(db, old) {
        let old_module = file_to_module(db, old_file)?;
        let new_name = infer_new_module_name(db, old, new, &old_module)?;
        return Some((old_module.name(db).clone(), new_name));
    }
    resolve_directory_rename(db, old, new)
}

/// Resolve a directory rename via its `__init__` file. Namespace packages
/// (directories without an `__init__`) are skipped.
fn resolve_directory_rename(
    db: &dyn Db,
    old_dir: &SystemPath,
    new_dir: &SystemPath,
) -> Option<(ModuleName, ModuleName)> {
    let init_file = system_path_to_file(db, old_dir.join("__init__.py"))
        .or_else(|_| system_path_to_file(db, old_dir.join("__init__.pyi")))
        .ok()?;
    let old_module = file_to_module(db, init_file)?;
    let new_name = infer_directory_module_name(db, new_dir, &old_module)?;
    Some((old_module.name(db).clone(), new_name))
}

/// Remap an absolute module name through the rename map.
///
/// Matches the longest ancestor of `name` that is a renamed prefix and rewrites
/// that prefix, so a single `old -> new` entry also remaps every submodule.
fn remap_module_name(name: &ModuleName, renames: &RenameMap) -> Option<ModuleName> {
    for ancestor in name.ancestors() {
        let Some(new_prefix) = renames.get(&ancestor) else {
            continue;
        };
        if &ancestor == name {
            return Some(new_prefix.clone());
        }
        let relative = name.relative_to(&ancestor)?;
        let mut result = new_prefix.clone();
        result.extend(&relative);
        return Some(result);
    }
    None
}

/// Parse `file` and collect any rename edits it requires.
///
/// A cheap textual prefilter skips files that cannot mention any renamed module.
fn collect_file_edits(
    db: &dyn Db,
    file: File,
    renames: &RenameMap,
    edits: &mut Vec<FileRenameEdit>,
) {
    let source = source_text(db, file);
    if !source_mentions_any(source.as_str(), renames) {
        return;
    }

    let parsed = parsed_module(db, file);
    let module = parsed.load(db);
    let model = SemanticModel::new(db, file);
    let mut visitor = ModuleRenameVisitor {
        db,
        model: &model,
        file,
        renames,
        edits,
        ancestors: Vec::new(),
    };
    AnyNodeRef::from(module.syntax()).visit_source_order(&mut visitor);
}

/// Returns whether the source could possibly reference any renamed module.
///
/// Checks both the first and last component of each renamed name so that
/// absolute imports (which spell the first component) and relative imports
/// (which spell only the final component) are both caught.
fn source_mentions_any(source: &str, renames: &RenameMap) -> bool {
    renames.keys().any(|name| {
        contains_identifier(source, name.first_component())
            || contains_identifier(source, name.last_component())
    })
}

/// Walks a file's AST, emitting [`FileRenameEdit`]s for every import statement
/// and module-typed expression that refers to a renamed module.
struct ModuleRenameVisitor<'a, 'db> {
    db: &'db dyn Db,
    model: &'a SemanticModel<'db>,
    file: File,
    renames: &'a RenameMap,
    edits: &'a mut Vec<FileRenameEdit>,
    ancestors: Vec<AnyNodeRef<'a>>,
}

impl<'a> SourceOrderVisitor<'a> for ModuleRenameVisitor<'a, '_> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.ancestors.push(node);

        match node {
            AnyNodeRef::StmtImport(import) => self.handle_import(import),
            AnyNodeRef::StmtImportFrom(import_from) => self.handle_import_from(import_from),
            AnyNodeRef::ExprName(name) => self.handle_name_expr(name),
            AnyNodeRef::ExprAttribute(attr) => self.handle_attr_expr(attr),
            _ => {}
        }

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        debug_assert_eq!(self.ancestors.last(), Some(&node));
        self.ancestors.pop();
    }
}

impl ModuleRenameVisitor<'_, '_> {
    /// Handle `import a.b.c [as x]`: rewrite each dotted module path that maps.
    fn handle_import(&mut self, import: &ast::StmtImport) {
        for alias in &import.names {
            let Some(old) = ModuleName::new(alias.name.as_str()) else {
                continue;
            };
            let Some(new) = remap_module_name(&old, self.renames) else {
                continue;
            };
            if new != old {
                self.push_if_changed(alias.name.range(), new.as_str().to_string());
            }
        }
    }

    /// Handle `from <module> import <names>`.
    fn handle_import_from(&mut self, stmt: &ast::StmtImportFrom) {
        let Ok(from_abs) = ModuleName::from_import_statement(self.db, self.file, stmt) else {
            return;
        };

        // The `from` module (or one of its ancestors) is itself renamed.
        if let Some(new_from) = remap_module_name(&from_abs, self.renames) {
            if new_from != from_abs {
                self.rewrite_from_module(stmt, &new_from);
            }
            self.rewrite_submodule_names(stmt, &from_abs);
            return;
        }

        // The `from` module is unchanged, but an imported submodule may move.
        self.handle_moved_submodules(stmt, &from_abs);
    }

    /// Rewrite imported names that are themselves renamed submodules of `from_abs`.
    fn rewrite_submodule_names(&mut self, stmt: &ast::StmtImportFrom, from_abs: &ModuleName) {
        for alias in &stmt.names {
            let Some(new_full) = self.renamed_submodule(from_abs, &alias.name) else {
                continue;
            };
            let new_last = new_full.last_component();
            if new_last != alias.name.as_str() {
                self.push_if_changed(alias.name.range(), new_last.to_string());
            }
        }
    }

    /// Handle `from X import Y` where `X` is unchanged but `Y` is a moved submodule.
    fn handle_moved_submodules(&mut self, stmt: &ast::StmtImportFrom, from_abs: &ModuleName) {
        let renamed: Vec<(&ast::Alias, ModuleName)> = stmt
            .names
            .iter()
            .filter_map(|alias| Some((alias, self.renamed_submodule(from_abs, &alias.name)?)))
            .collect();
        if renamed.is_empty() {
            return;
        }

        let new_parent = renamed[0].1.parent();
        if !renamed.iter().all(|(_, full)| full.parent() == new_parent) {
            return;
        }

        match &new_parent {
            // Submodule renamed within the same package: only the name changes.
            Some(parent) if parent == from_abs => {}
            // Submodule moved to another package: the `from` part must change too,
            // which is only safe when no other name in this statement would break.
            Some(parent) => {
                if stmt.names.len() != renamed.len() {
                    return;
                }
                self.rewrite_from_module(stmt, parent);
            }
            // Submodule moved to the top level: cannot keep a `from X import` form.
            None => return,
        }

        for (alias, full) in renamed {
            let new_last = full.last_component();
            if new_last != alias.name.as_str() {
                self.push_if_changed(alias.name.range(), new_last.to_string());
            }
        }
    }

    /// If `name` is a renamed submodule of `from_abs`, return its new full name.
    ///
    /// Resolves the candidate to confirm it is actually a module (not a value
    /// re-exported with the same name).
    fn renamed_submodule(
        &self,
        from_abs: &ModuleName,
        name: &ast::Identifier,
    ) -> Option<ModuleName> {
        let part = ModuleName::new(name.as_str())?;
        let mut candidate = from_abs.clone();
        candidate.extend(&part);
        resolve_module_confident(self.db, &candidate)?;
        let new_full = remap_module_name(&candidate, self.renames)?;
        (new_full != candidate).then_some(new_full)
    }

    /// Rewrite the module portion of a `from <module> import ...` statement to
    /// the absolute name `new_abs`, preserving relative form where possible.
    fn rewrite_from_module(&mut self, stmt: &ast::StmtImportFrom, new_abs: &ModuleName) {
        if stmt.level == 0 {
            if let Some(module) = &stmt.module {
                self.push_if_changed(module.range(), new_abs.as_str().to_string());
            }
            return;
        }

        let source = source_text(self.db, self.file);
        let Some(range) = relative_part_range(source.as_str(), stmt) else {
            return;
        };
        let Some(new_text) = self.express_relative_or_absolute(new_abs) else {
            return;
        };
        self.push_if_changed(range, new_text);
    }

    /// Express `new_abs` as it should be written in the current file's import:
    /// relatively (e.g. `..pkg.mod`) when possible, otherwise absolutely.
    ///
    /// The relative base is computed from the importing file's *new* location, so
    /// a relative import that moves together with its target stays unchanged.
    fn express_relative_or_absolute(&self, new_abs: &ModuleName) -> Option<String> {
        let importing = file_to_module(self.db, self.file)?;
        let is_package = importing.kind(self.db).is_package();
        let importing_old = importing.name(self.db).clone();
        let importing_new =
            remap_module_name(&importing_old, self.renames).unwrap_or(importing_old);
        let anchor = if is_package {
            importing_new
        } else {
            importing_new.parent()?
        };

        for (depth, base) in anchor.ancestors().enumerate() {
            let dots = ".".repeat(depth + 1);
            if new_abs == &base {
                return Some(dots);
            }
            if let Some(tail) = new_abs.relative_to(&base) {
                return Some(format!("{dots}{tail}"));
            }
        }
        Some(new_abs.as_str().to_string())
    }

    /// Handle a bare name expression that refers to a module (e.g. `old_module`).
    fn handle_name_expr(&mut self, name: &ast::ExprName) {
        if self.parent_is_module_attr() {
            return;
        }
        let Some(module) = self.module_name_of(name) else {
            return;
        };
        let Some(new) = remap_module_name(&module, self.renames) else {
            return;
        };
        // A name written as the full absolute module path (`import a.b.c`) is
        // replaced wholesale; a name bound by `from pkg import sub` is replaced
        // with the new final component. An explicit alias is left untouched.
        let new_text = if name.id.as_str() == module.as_str() {
            new.as_str().to_string()
        } else if name.id.as_str() == module.last_component() {
            new.last_component().to_string()
        } else {
            return;
        };
        self.push_if_changed(name.range(), new_text);
    }

    /// Handle an attribute chain that refers to a module (e.g. `pkg.old_sub`).
    fn handle_attr_expr(&mut self, attr: &ast::ExprAttribute) {
        if self.parent_is_module_attr() {
            return;
        }
        let Some(module) = self.module_name_of(attr) else {
            return;
        };
        let Some(new) = remap_module_name(&module, self.renames) else {
            return;
        };
        if let Some(new_text) = self.rewritten_attr_path(attr, &new) {
            self.push_if_changed(attr.range(), new_text);
        }
    }

    /// Compute the replacement text for a module-typed attribute chain.
    ///
    /// A literal root (`a.b.c`) is replaced wholesale with the new absolute name.
    /// An aliased root (`import pkg as p; p.sub`) keeps the alias and only updates
    /// the suffix after it.
    fn rewritten_attr_path(&self, attr: &ast::ExprAttribute, new: &ModuleName) -> Option<String> {
        let root = root_name_of(&attr.value)?;
        let root_module = self.module_name_of(root)?;
        // Literal absolute root (`a.b.c`): replace the whole path.
        if root.id.as_str() == root_module.as_str() {
            return Some(new.as_str().to_string());
        }

        let root_new =
            remap_module_name(&root_module, self.renames).unwrap_or_else(|| root_module.clone());
        let suffix = new.relative_to(&root_new)?;
        // A `from pkg import sub` binding moves with the module's final component;
        // an explicit alias keeps its text.
        let root_text = if root.id.as_str() == root_module.last_component() {
            root_new.last_component()
        } else {
            root.id.as_str()
        };
        Some(format!("{root_text}.{suffix}"))
    }

    /// Returns whether the parent node is itself a module-typed attribute chain,
    /// in which case the outer node already rewrites this one's path.
    fn parent_is_module_attr(&self) -> bool {
        let len = self.ancestors.len();
        if len < 2 {
            return false;
        }
        matches!(
            self.ancestors[len - 2],
            AnyNodeRef::ExprAttribute(parent) if self.module_name_of(parent).is_some()
        )
    }

    /// The module name an expression refers to, if its type is a module literal.
    fn module_name_of<T: HasType>(&self, expr: &T) -> Option<ModuleName> {
        match expr.inferred_type(self.model)? {
            Type::ModuleLiteral(literal) => Some(literal.module(self.db).name(self.db).clone()),
            _ => None,
        }
    }

    /// Push an edit unless the replacement matches the current source text.
    fn push_if_changed(&mut self, range: TextRange, new_text: String) {
        let source = source_text(self.db, self.file);
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        if source.as_str().get(start..end) == Some(new_text.as_str()) {
            return;
        }
        self.edits.push(FileRenameEdit {
            file: self.file,
            range,
            new_text,
        });
    }
}

/// Walk an expression chain to its root name (e.g. `a.b.c` -> `a`).
fn root_name_of(expr: &ast::Expr) -> Option<&ast::ExprName> {
    match expr {
        ast::Expr::Name(name) => Some(name),
        ast::Expr::Attribute(attr) => root_name_of(&attr.value),
        _ => None,
    }
}

/// The source range of the relative portion of a `from` import, covering the
/// leading dots and the module path (e.g. `..pkg.mod` or `.`).
fn relative_part_range(source: &str, stmt: &ast::StmtImportFrom) -> Option<TextRange> {
    let bytes = source.as_bytes();
    let mut start = usize::from(stmt.range().start()) + "from".len();
    while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    let end = match &stmt.module {
        Some(module) => usize::from(module.range().end()),
        None => start + stmt.level as usize,
    };
    Some(TextRange::new(
        TextSize::try_from(start).ok()?,
        TextSize::try_from(end).ok()?,
    ))
}

/// Infer the new module name for a renamed directory from its search path.
fn infer_directory_module_name(
    db: &dyn Db,
    new_dir: &SystemPath,
    old_module: &Module<'_>,
) -> Option<ModuleName> {
    let search_path = old_module.search_path(db)?.as_system_path()?;
    let new_relative = new_dir.strip_prefix(search_path).ok()?;
    let components: Vec<&str> = new_relative.components().map(|c| c.as_str()).collect();
    if components.is_empty() {
        return None;
    }
    ModuleName::from_components(components)
}

/// Infer the new module name from old/new file paths.
///
/// For same-directory renames, replaces the last component of the old module
/// name using [`ModuleName::parent()`]. For cross-directory moves, derives the
/// new module name from the module's search path.
fn infer_new_module_name(
    db: &dyn Db,
    old_path: &SystemPath,
    new_path: &SystemPath,
    old_module: &Module<'_>,
) -> Option<ModuleName> {
    let new_stem = new_path.file_stem()?;
    let old_stem = old_path.file_stem()?;
    if new_stem == "__init__" || old_stem == "__init__" {
        return None;
    }

    let old_module_name = old_module.name(db);

    if old_path.parent() == new_path.parent() {
        return if let Some(parent) = old_module_name.parent() {
            let components: Vec<&str> = parent
                .components()
                .chain(std::iter::once(new_stem))
                .collect();
            ModuleName::from_components(components)
        } else {
            ModuleName::new(new_stem)
        };
    }

    // Cross-directory move: derive the full module name from the search path.
    let search_path = old_module.search_path(db)?.as_system_path()?;
    let new_relative = new_path.strip_prefix(search_path).ok()?;
    let parent = new_relative.parent()?;
    let components: Vec<&str> = if parent.as_str().is_empty() {
        vec![new_stem]
    } else {
        parent
            .components()
            .map(|c| c.as_str())
            .chain(std::iter::once(new_stem))
            .collect()
    };
    ModuleName::from_components(components)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ruff_python_ast::PythonVersion;
    use ty_project::{ProjectMetadata, TestDb};

    fn create_test_db(files: &[(&str, &str)]) -> TestDb {
        let mut db = TestDb::new(ProjectMetadata::new(
            "test".into(),
            SystemPathBuf::from("/"),
        ));

        db.init_program_with_python_version(PythonVersion::latest_ty())
            .unwrap();

        for &(path, contents) in files {
            db.write_file(path, contents)
                .expect("write to memory file system to be successful");
        }

        db
    }

    fn apply_edits(db: &dyn Db, edits: &[FileRenameEdit], file: File) -> String {
        let source = source_text(db, file);
        let text = source.as_str().to_owned();

        let mut sorted_edits: Vec<_> = edits.iter().filter(|e| e.file == file).collect();
        sorted_edits.sort_by_key(|b| std::cmp::Reverse(b.range.start()));

        let mut result = text;
        for edit in sorted_edits {
            let start = usize::from(edit.range.start());
            let end = usize::from(edit.range.end());
            result.replace_range(start..end, &edit.new_text);
        }
        result
    }

    #[test]
    fn rename_simple_import() {
        let db = create_test_db(&[
            ("old_module.py", "x = 1\n"),
            ("consumer.py", "import old_module\n\nprint(old_module.x)\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("old_module.py"),
            SystemPath::new("new_module.py"),
        );

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import new_module\n\nprint(new_module.x)\n");
    }

    #[test]
    fn rename_from_import() {
        let db = create_test_db(&[
            ("old_module.py", "x = 1\n"),
            ("consumer.py", "from old_module import x\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("old_module.py"),
            SystemPath::new("new_module.py"),
        );

        assert_eq!(edits.len(), 1);

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from new_module import x\n");
    }

    #[test]
    fn rename_no_edits_for_unrelated_files() {
        let db = create_test_db(&[
            ("old_module.py", "x = 1\n"),
            ("other.py", "y = 2\n"),
            ("consumer.py", "import other\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("old_module.py"),
            SystemPath::new("new_module.py"),
        );

        assert!(edits.is_empty());
    }

    #[test]
    fn rename_multiple_consumers() {
        let db = create_test_db(&[
            ("old_module.py", "x = 1\n"),
            ("consumer1.py", "import old_module\n"),
            ("consumer2.py", "from old_module import x\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("old_module.py"),
            SystemPath::new("new_module.py"),
        );

        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn rename_nonexistent_file() {
        let db = create_test_db(&[("consumer.py", "import something\n")]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("nonexistent.py"),
            SystemPath::new("new_name.py"),
        );

        assert!(edits.is_empty());
    }

    #[test]
    fn rename_package_submodule() {
        let db = create_test_db(&[
            ("pkg/__init__.py", ""),
            ("pkg/old_sub.py", "x = 1\n"),
            ("consumer.py", "from pkg.old_sub import x\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/old_sub.py"),
            SystemPath::new("pkg/new_sub.py"),
        );

        assert_eq!(edits.len(), 1);

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from pkg.new_sub import x\n");
    }

    #[test]
    fn rename_relative_import() {
        let db = create_test_db(&[
            (
                "pkg/__init__.py",
                "from .ner_model_port import NERModelPort as NERModelPort\n",
            ),
            ("pkg/ner_model_port.py", "class NERModelPort: ...\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/ner_model_port.py"),
            SystemPath::new("pkg/ner_model.py"),
        );

        assert_eq!(edits.len(), 1);

        let init = system_path_to_file(&db, "pkg/__init__.py").unwrap();
        let result = apply_edits(&db, &edits, init);
        assert_eq!(
            result,
            "from .ner_model import NERModelPort as NERModelPort\n"
        );
    }

    #[test]
    fn rename_relative_import_deep_package() {
        let db = create_test_db(&[
            ("qu/__init__.py", ""),
            ("qu/domain/__init__.py", ""),
            (
                "qu/domain/port/__init__.py",
                concat!(
                    "from .ai_model_port import AIModelPort as AIModelPort\n",
                    "from .ai_model_port import AIModelResult as AIModelResult\n",
                    "from .cache_port import CachePort as CachePort\n",
                    "from .dictionary_port import DictionaryPort as DictionaryPort\n",
                    "from .embed_model_port import EmbedModelPort as EmbedModelPort\n",
                    "from .ner_model_port import NERModelPort as NERModelPort\n",
                ),
            ),
            (
                "qu/domain/port/ai_model_port.py",
                "class AIModelPort: ...\nclass AIModelResult: ...\n",
            ),
            ("qu/domain/port/cache_port.py", "class CachePort: ...\n"),
            (
                "qu/domain/port/dictionary_port.py",
                "class DictionaryPort: ...\n",
            ),
            (
                "qu/domain/port/embed_model_port.py",
                "class EmbedModelPort: ...\n",
            ),
            (
                "qu/domain/port/ner_model_port.py",
                "class NERModelPort: ...\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("qu/domain/port/ner_model_port.py"),
            SystemPath::new("qu/domain/port/ner_model.py"),
        );

        assert_eq!(edits.len(), 1);

        let init = system_path_to_file(&db, "qu/domain/port/__init__.py").unwrap();
        let result = apply_edits(&db, &edits, init);
        let expected = concat!(
            "from .ai_model_port import AIModelPort as AIModelPort\n",
            "from .ai_model_port import AIModelResult as AIModelResult\n",
            "from .cache_port import CachePort as CachePort\n",
            "from .dictionary_port import DictionaryPort as DictionaryPort\n",
            "from .embed_model_port import EmbedModelPort as EmbedModelPort\n",
            "from .ner_model import NERModelPort as NERModelPort\n",
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn rename_relative_import_multiple_lines() {
        let db = create_test_db(&[
            (
                "pkg/__init__.py",
                concat!(
                    "from .ai_model_port import AIModelPort as AIModelPort\n",
                    "from .ai_model_port import AIModelResult as AIModelResult\n",
                    "from .cache_port import CachePort as CachePort\n",
                    "from .dictionary_port import DictionaryPort as DictionaryPort\n",
                    "from .embed_model_port import EmbedModelPort as EmbedModelPort\n",
                    "from .ner_model_port import NERModelPort as NERModelPort\n",
                ),
            ),
            (
                "pkg/ai_model_port.py",
                "class AIModelPort: ...\nclass AIModelResult: ...\n",
            ),
            ("pkg/cache_port.py", "class CachePort: ...\n"),
            ("pkg/dictionary_port.py", "class DictionaryPort: ...\n"),
            ("pkg/embed_model_port.py", "class EmbedModelPort: ...\n"),
            ("pkg/ner_model_port.py", "class NERModelPort: ...\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/ner_model_port.py"),
            SystemPath::new("pkg/ner_model.py"),
        );

        assert_eq!(edits.len(), 1);

        let init = system_path_to_file(&db, "pkg/__init__.py").unwrap();
        let result = apply_edits(&db, &edits, init);
        let expected = concat!(
            "from .ai_model_port import AIModelPort as AIModelPort\n",
            "from .ai_model_port import AIModelResult as AIModelResult\n",
            "from .cache_port import CachePort as CachePort\n",
            "from .dictionary_port import DictionaryPort as DictionaryPort\n",
            "from .embed_model_port import EmbedModelPort as EmbedModelPort\n",
            "from .ner_model import NERModelPort as NERModelPort\n",
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn rename_from_parent_import_submodule() {
        let db = create_test_db(&[
            ("pkg/__init__.py", ""),
            ("pkg/old_sub.py", "x = 1\n"),
            ("consumer.py", "from pkg import old_sub\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/old_sub.py"),
            SystemPath::new("pkg/new_sub.py"),
        );

        assert_eq!(edits.len(), 1);

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from pkg import new_sub\n");
    }

    #[test]
    fn rename_module_usage_sites() {
        let db = create_test_db(&[
            ("old_module.py", "x = 1\ndef hello(): ...\n"),
            (
                "consumer.py",
                "import old_module\n\nprint(old_module.x)\nold_module.hello()\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("old_module.py"),
            SystemPath::new("new_module.py"),
        );

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(
            result,
            "import new_module\n\nprint(new_module.x)\nnew_module.hello()\n"
        );
    }

    #[test]
    fn rename_dotted_import() {
        let db = create_test_db(&[
            ("pkg/__init__.py", ""),
            ("pkg/old_sub.py", "x = 1\n"),
            (
                "consumer.py",
                "import pkg.old_sub\n\nprint(pkg.old_sub.x)\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/old_sub.py"),
            SystemPath::new("pkg/new_sub.py"),
        );

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import pkg.new_sub\n\nprint(pkg.new_sub.x)\n");
    }

    #[test]
    fn rename_cross_directory() {
        let db = create_test_db(&[
            ("/old_package/__init__.py", ""),
            ("/old_package/old_module.py", "x = 1\n"),
            ("/new_package/__init__.py", ""),
            (
                "/consumer.py",
                "import old_package.old_module\n\nprint(old_package.old_module.x)\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/old_package/old_module.py"),
            SystemPath::new("/new_package/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(
            result,
            "import new_package.new_module\n\nprint(new_package.new_module.x)\n"
        );
    }

    #[test]
    fn rename_cross_directory_from_import() {
        let db = create_test_db(&[
            ("/old_package/__init__.py", ""),
            ("/old_package/old_module.py", "x = 1\n"),
            ("/new_package/__init__.py", ""),
            ("/consumer.py", "from old_package.old_module import x\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/old_package/old_module.py"),
            SystemPath::new("/new_package/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from new_package.new_module import x\n");
    }

    #[test]
    fn rename_cross_directory_standalone_import() {
        let db = create_test_db(&[
            ("/old_package/__init__.py", ""),
            ("/old_package/old_module.py", "x = 1\n"),
            ("/new_package/__init__.py", ""),
            ("/consumer.py", "from old_package import old_module\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/old_package/old_module.py"),
            SystemPath::new("/new_package/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        // Both the package and module names change: the module moved packages.
        assert_eq!(result, "from new_package import new_module\n");
    }

    #[test]
    fn rename_init_file_returns_no_edits() {
        let db = create_test_db(&[
            ("pkg/__init__.py", "x = 1\n"),
            ("consumer.py", "import pkg\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/__init__.py"),
            SystemPath::new("pkg/new.py"),
        );

        assert!(edits.is_empty());
    }

    #[test]
    fn rename_shadowed_module_not_rewritten() {
        let db = create_test_db(&[
            ("pkg/__init__.py", ""),
            ("pkg/foo.py", "x = 1\n"),
            (
                "consumer.py",
                "from pkg import foo\n\ndef f(pkg):\n    return pkg.foo\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/foo.py"),
            SystemPath::new("pkg/bar.py"),
        );

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        // `from pkg import foo` → `from pkg import bar`, but `pkg.foo` inside
        // `f(pkg)` should NOT be rewritten because `pkg` is a parameter.
        assert_eq!(
            result,
            "from pkg import bar\n\ndef f(pkg):\n    return pkg.foo\n"
        );
    }

    #[test]
    fn rename_pyi_file() {
        let db = create_test_db(&[
            ("old_module.pyi", "x: int\n"),
            ("consumer.py", "import old_module\n\nprint(old_module.x)\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("old_module.pyi"),
            SystemPath::new("new_module.pyi"),
        );

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import new_module\n\nprint(new_module.x)\n");
    }

    #[test]
    fn rename_directory_simple() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", ""),
            ("/old_pkg/sub.py", "x = 1\n"),
            ("/consumer.py", "import old_pkg\n\nprint(old_pkg.sub)\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import new_pkg\n\nprint(new_pkg.sub)\n");
    }

    #[test]
    fn rename_directory_from_import() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", "x = 1\n"),
            ("/consumer.py", "from old_pkg import x\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from new_pkg import x\n");
    }

    #[test]
    fn rename_directory_dotted_import() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", ""),
            ("/old_pkg/sub.py", "x = 1\n"),
            (
                "/consumer.py",
                "import old_pkg.sub\nfrom old_pkg.sub import x\n",
            ),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import new_pkg.sub\nfrom new_pkg.sub import x\n");
    }

    #[test]
    fn rename_directory_no_init_skipped() {
        let db = create_test_db(&[
            ("/ns_pkg/sub.py", "x = 1\n"),
            ("/consumer.py", "from ns_pkg import sub\n"),
        ]);

        let edits =
            will_rename_directory(&db, SystemPath::new("/ns_pkg"), SystemPath::new("/new_ns"));

        assert!(edits.is_empty());
    }

    #[test]
    fn rename_directory_nested_package() {
        let db = create_test_db(&[
            ("/parent/__init__.py", ""),
            ("/parent/old_child/__init__.py", "x = 1\n"),
            ("/parent/old_child/mod.py", "y = 2\n"),
            (
                "/consumer.py",
                "from parent.old_child import x\nimport parent.old_child.mod\n",
            ),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/parent/old_child"),
            SystemPath::new("/parent/new_child"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(
            result,
            "from parent.new_child import x\nimport parent.new_child.mod\n"
        );
    }

    #[test]
    fn rename_directory_relative_imports_unchanged() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", ""),
            ("/old_pkg/a.py", "from . import b\n"),
            ("/old_pkg/b.py", "x = 1\n"),
            ("/consumer.py", "from old_pkg import a\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        // Only consumer.py should be modified; files inside the package must not
        // appear in edits (relative imports are unaffected by package rename).
        for edit in &edits {
            let path = edit.file.path(&db);
            assert!(
                !path.as_str().contains("old_pkg"),
                "unexpected edit in package-internal file: {path}"
            );
        }

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from new_pkg import a\n");
    }

    #[test]
    fn rename_directory_invalid_identifier_returns_no_edits() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", "x = 1\n"),
            ("/consumer.py", "from old_pkg import x\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/123-bad"),
        );

        assert!(edits.is_empty());
    }

    #[test]
    fn rename_cross_directory_from_import_with_usage() {
        let db = create_test_db(&[
            ("/old_package/__init__.py", ""),
            ("/old_package/old_module.py", "x = 1\n"),
            ("/new_package/__init__.py", ""),
            (
                "/consumer.py",
                "from old_package import old_module\n\nprint(old_module.x)\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/old_package/old_module.py"),
            SystemPath::new("/new_package/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        // The package and module both change, and the `old_module` binding used
        // below must follow the new final component.
        assert_eq!(
            result,
            "from new_package import new_module\n\nprint(new_module.x)\n"
        );
    }

    #[test]
    fn rename_relative_import_becomes_absolute_after_move() {
        let db = create_test_db(&[
            ("/a/__init__.py", ""),
            ("/a/b/__init__.py", ""),
            ("/a/b/old_module.py", "x = 1\n"),
            ("/a/b/c/__init__.py", ""),
            ("/a/b/c/consumer.py", "from ..old_module import x\n"),
        ]);

        // Move to a completely different package; the import can no longer be
        // expressed relatively and must become absolute.
        let edits = will_rename_file(
            &db,
            SystemPath::new("/a/b/old_module.py"),
            SystemPath::new("/x/y/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/a/b/c/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from x.y.new_module import x\n");
    }

    #[test]
    fn rename_relative_import_level_changes_after_move() {
        let db = create_test_db(&[
            ("/a/__init__.py", ""),
            ("/a/b/__init__.py", ""),
            ("/a/b/old_module.py", "x = 1\n"),
            ("/a/b/c/__init__.py", ""),
            ("/a/b/c/consumer.py", "from ..old_module import x\n"),
        ]);

        // Move one level up; the relative import needs an extra dot.
        let edits = will_rename_file(
            &db,
            SystemPath::new("/a/b/old_module.py"),
            SystemPath::new("/a/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/a/b/c/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from ...new_module import x\n");
    }

    #[test]
    fn rename_import_alias_with_dotted_path() {
        let db = create_test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old_module.py", "x = 1\n"),
            (
                "/consumer.py",
                "import pkg.old_module as pom\n\nprint(pom.x)\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/pkg/old_module.py"),
            SystemPath::new("/pkg/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        // The dotted import path is updated; the alias and its usages are not.
        assert_eq!(result, "import pkg.new_module as pom\n\nprint(pom.x)\n");
    }

    #[test]
    fn rename_from_import_with_symbol_alias() {
        let db = create_test_db(&[
            ("/pkg/__init__.py", ""),
            ("/pkg/old_module.py", "class MyClass: ...\n"),
            (
                "/consumer.py",
                "from pkg.old_module import MyClass as MC\n\nobj = MC()\n",
            ),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/pkg/old_module.py"),
            SystemPath::new("/pkg/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(
            result,
            "from pkg.new_module import MyClass as MC\n\nobj = MC()\n"
        );
    }

    #[test]
    fn rename_directory_package_attribute_access() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", "from .sub import helper\n"),
            ("/old_pkg/sub.py", "def helper(): ...\n"),
            ("/consumer.py", "import old_pkg\n\nold_pkg.helper()\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import new_pkg\n\nnew_pkg.helper()\n");
    }

    #[test]
    fn rename_submodule_accessed_via_package_attribute() {
        let db = create_test_db(&[
            ("/pkg/__init__.py", "from . import old_sub\n"),
            ("/pkg/old_sub.py", "x = 1\n"),
            ("/consumer.py", "import pkg\n\nprint(pkg.old_sub.x)\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/pkg/old_sub.py"),
            SystemPath::new("/pkg/new_sub.py"),
        );

        let init = system_path_to_file(&db, "/pkg/__init__.py").unwrap();
        let init_result = apply_edits(&db, &edits, init);
        assert_eq!(init_result, "from . import new_sub\n");

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import pkg\n\nprint(pkg.new_sub.x)\n");
    }

    #[test]
    fn rename_directory_init_with_absolute_import() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", "from old_pkg.sub import helper\n"),
            ("/old_pkg/sub.py", "def helper(): ...\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        // The package's own `__init__.py` is the anchor file but must still be
        // rewritten when it references the package by absolute path.
        let init = system_path_to_file(&db, "/old_pkg/__init__.py").unwrap();
        let result = apply_edits(&db, &edits, init);
        assert_eq!(result, "from new_pkg.sub import helper\n");
    }

    #[test]
    fn rename_directory_init_with_dotted_import() {
        let db = create_test_db(&[
            ("/old_pkg/__init__.py", "import old_pkg.sub\n"),
            ("/old_pkg/sub.py", "x = 1\n"),
        ]);

        let edits = will_rename_directory(
            &db,
            SystemPath::new("/old_pkg"),
            SystemPath::new("/new_pkg"),
        );

        let init = system_path_to_file(&db, "/old_pkg/__init__.py").unwrap();
        let result = apply_edits(&db, &edits, init);
        assert_eq!(result, "import new_pkg.sub\n");
    }

    #[test]
    fn rename_multiline_from_import_range() {
        let db = create_test_db(&[
            ("/a/__init__.py", ""),
            ("/a/b/__init__.py", ""),
            ("/a/b/old_module.py", "x = 1\n"),
            ("/consumer.py", "from a.b.old_module import (\n    x,\n)\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("/a/b/old_module.py"),
            SystemPath::new("/a/b/new_module.py"),
        );

        let consumer = system_path_to_file(&db, "/consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "from a.b.new_module import (\n    x,\n)\n");
    }
}
