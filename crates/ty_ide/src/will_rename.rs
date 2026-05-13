use crate::goto::GotoTarget;
use crate::references::{ReferencesMode, references};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::parsed::parsed_module;
use ruff_db::system::SystemPath;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::{Module, ModuleName, file_to_module};
use ty_project::Db;

/// A text edit to apply when renaming a file.
#[derive(Debug, Clone)]
pub struct FileRenameEdit {
    pub file: File,
    pub range: TextRange,
    pub new_text: String,
}

/// Compute the edits needed when renaming a Python file.
///
/// Uses the find-references infrastructure for identifier-level references
/// (e.g., `import old_module`, `old_module.foo()`, `from pkg import old_sub`)
/// and a lightweight import-path scanner for module paths in import statements
/// (e.g., `from old_module import x`, `import pkg.old_sub`).
pub fn will_rename_file(
    db: &dyn Db,
    old_path: &SystemPath,
    new_path: &SystemPath,
) -> Vec<FileRenameEdit> {
    let Ok(old_file) = system_path_to_file(db, old_path) else {
        return vec![];
    };

    let Some(old_module) = file_to_module(db, old_file) else {
        return vec![];
    };

    let old_module_name = old_module.name(db).clone();
    let Some(new_module_name) = infer_new_module_name(db, old_path, new_path, &old_module) else {
        return vec![];
    };

    let old_name_str = old_module_name.as_str();
    let new_name_str = new_module_name.as_str();
    let new_last = new_name_str
        .rsplit('.')
        .next()
        .unwrap_or(new_name_str)
        .to_string();

    let mut edits = Vec::new();

    // Use the find-references infrastructure to locate all identifier-level
    // references to the old module across the project. This covers:
    //   - `import old_module` (alias declaration)
    //   - `old_module.foo()` (name expression usage)
    //   - `from pkg import old_sub` (alias declaration)
    //   - attribute access on the module
    let goto_target = GotoTarget::ImportModuleComponent {
        module_name: old_name_str.to_string(),
        level: 0,
        component_index: old_name_str.matches('.').count(),
        component_range: TextRange::default(),
    };

    if let Some(refs) = references(db, old_file, &goto_target, ReferencesMode::RenameMultiFile) {
        edits.extend(refs.into_iter().map(|r| FileRenameEdit {
            file: r.file(),
            range: r.range(),
            new_text: new_last.clone(),
        }));
    }

    // Handle import module-path references not covered by find-references.
    // The references infrastructure doesn't visit the module-path portion of
    // `from <module> import ...` or dotted names in `import pkg.old_sub`.
    let project = db.project();
    for file in project.files(db).iter() {
        if *file == old_file {
            continue;
        }
        collect_import_path_edits(db, *file, &old_module_name, &new_module_name, &mut edits);
    }

    edits
}

/// Infer the new module name from old/new file paths.
///
/// Uses the old module's known name and the file name change to derive
/// the new module name. For example, renaming `foo/bar.py` to `foo/baz.py`
/// when the old module is `foo.bar` yields `foo.baz`.
fn infer_new_module_name(
    db: &dyn Db,
    old_path: &SystemPath,
    new_path: &SystemPath,
    old_module: &Module<'_>,
) -> Option<ModuleName> {
    let old_stem = old_path.file_stem()?;
    let new_stem = new_path.file_stem()?;

    let old_name_str = old_module.name(db).as_str().to_owned();

    if old_stem == "__init__" || new_stem == "__init__" {
        return None;
    }

    let new_name_str = if let Some((prefix, _)) = old_name_str.rsplit_once('.') {
        format!("{prefix}.{new_stem}")
    } else {
        new_stem.to_string()
    };

    ModuleName::new(&new_name_str)
}

/// Scan a file for import module-path references to the old module.
///
/// This handles cases where the module name appears as a path in an import
/// statement rather than as an identifier reference:
/// - `from old_module import x` (the `old_module` is a module path, not an identifier)
/// - `from .old_module import x` (relative import module path)
/// - `import pkg.old_sub` (dotted import name)
fn collect_import_path_edits(
    db: &dyn Db,
    file: File,
    old_module_name: &ModuleName,
    new_module_name: &ModuleName,
    edits: &mut Vec<FileRenameEdit>,
) {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut scanner = ImportPathScanner {
        db,
        file,
        old_module_name,
        new_module_name,
        edits,
    };
    scanner.visit_body(module.suite());
}

struct ImportPathScanner<'a> {
    db: &'a dyn Db,
    file: File,
    old_module_name: &'a ModuleName,
    new_module_name: &'a ModuleName,
    edits: &'a mut Vec<FileRenameEdit>,
}

impl<'a> StatementVisitor<'a> for ImportPathScanner<'_> {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::ImportFrom(import_from) => {
                self.handle_import_from(import_from);
            }
            ast::Stmt::Import(import) => {
                self.handle_dotted_import(import);
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

impl ImportPathScanner<'_> {
    /// Handle `from <module> import ...` where the module path matches the old module.
    fn handle_import_from(&mut self, import_from: &ast::StmtImportFrom) {
        let Ok(resolved_name) =
            ModuleName::from_import_statement(self.db, self.file, import_from)
        else {
            return;
        };

        if &resolved_name != self.old_module_name {
            return;
        }

        let Some(module_id) = &import_from.module else {
            return;
        };

        let Some(new_from) = self.compute_new_from_module(import_from) else {
            return;
        };

        self.edits.push(FileRenameEdit {
            file: self.file,
            range: module_id.range(),
            new_text: new_from,
        });
    }

    fn compute_new_from_module(&self, import_from: &ast::StmtImportFrom) -> Option<String> {
        if import_from.level > 0 {
            let old_suffix = import_from
                .module
                .as_ref()
                .map(ast::Identifier::as_str)
                .unwrap_or("");
            let old_full = self.old_module_name.as_str();
            let new_full = self.new_module_name.as_str();

            old_full
                .strip_suffix(old_suffix)
                .and_then(|prefix| new_full.strip_prefix(prefix))
                .map(str::to_string)
        } else {
            Some(self.new_module_name.to_string())
        }
    }

    /// Handle `import pkg.old_sub` where the dotted name matches the old module.
    ///
    /// Single-component imports (`import old_module`) are already handled by
    /// the find-references infrastructure, so we only process dotted names.
    fn handle_dotted_import(&mut self, import: &ast::StmtImport) {
        for alias in &import.names {
            let name = alias.name.as_str();
            if !name.contains('.') {
                continue;
            }

            let Some(imported_name) = ModuleName::new(name) else {
                continue;
            };

            if &imported_name == self.old_module_name {
                self.edits.push(FileRenameEdit {
                    file: self.file,
                    range: alias.name.range(),
                    new_text: self.new_module_name.to_string(),
                });
            }
        }
    }
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
        sorted_edits.sort_by(|a, b| b.range.start().cmp(&a.range.start()));

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
            (
                "pkg/dictionary_port.py",
                "class DictionaryPort: ...\n",
            ),
            (
                "pkg/embed_model_port.py",
                "class EmbedModelPort: ...\n",
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
            ("consumer.py", "import pkg.old_sub\n\nprint(pkg.old_sub.x)\n"),
        ]);

        let edits = will_rename_file(
            &db,
            SystemPath::new("pkg/old_sub.py"),
            SystemPath::new("pkg/new_sub.py"),
        );

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(
            result,
            "import pkg.new_sub\n\nprint(pkg.old_sub.x)\n"
        );
    }
}
