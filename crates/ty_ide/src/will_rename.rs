use ruff_db::files::{File, system_path_to_file};
use ruff_db::parsed::parsed_module;
use ruff_db::system::SystemPath;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::{self as ast, AnyNodeRef};
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

/// Compute the import edits needed when renaming a Python file.
///
/// Returns a list of edits across all project files that update import statements
/// to reflect the new module path.
pub fn will_rename_file(db: &dyn Db, old_path: &SystemPath, new_path: &SystemPath) -> Vec<FileRenameEdit> {
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

    let project = db.project();
    let mut edits = Vec::new();

    for file in project.files(db).iter() {
        if *file == old_file {
            continue;
        }

        collect_import_edits(db, *file, &old_module_name, &new_module_name, &mut edits);
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

/// Scan a file's import statements and collect edits for those that reference the old module.
fn collect_import_edits(
    db: &dyn Db,
    file: File,
    old_module_name: &ModuleName,
    new_module_name: &ModuleName,
    edits: &mut Vec<FileRenameEdit>,
) {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = ImportEditVisitor {
        db,
        file,
        old_module_name,
        new_module_name,
        edits,
    };

    visitor.visit_body(module.suite());
}

struct ImportEditVisitor<'a> {
    db: &'a dyn Db,
    file: File,
    old_module_name: &'a ModuleName,
    new_module_name: &'a ModuleName,
    edits: &'a mut Vec<FileRenameEdit>,
}

impl<'a> SourceOrderVisitor<'a> for ImportEditVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::Import(import) => {
                self.handle_import(import);
            }
            ast::Stmt::ImportFrom(import_from) => {
                self.handle_import_from(import_from);
            }
            _ => {
                ruff_python_ast::visitor::source_order::walk_stmt(self, stmt);
            }
        }
    }

    fn enter_node(&mut self, _node: AnyNodeRef<'a>) -> ruff_python_ast::visitor::source_order::TraversalSignal {
        ruff_python_ast::visitor::source_order::TraversalSignal::Traverse
    }
}

impl ImportEditVisitor<'_> {
    /// Handle `import foo.bar` statements.
    fn handle_import(&mut self, import: &ast::StmtImport) {
        for alias in &import.names {
            let Some(imported_name) = ModuleName::new(&alias.name) else {
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

    /// Handle `from foo.bar import baz` statements.
    fn handle_import_from(&mut self, import_from: &ast::StmtImportFrom) {
        let Ok(resolved_name) =
            ModuleName::from_import_statement(self.db, self.file, import_from)
        else {
            return;
        };

        if &resolved_name == self.old_module_name {
            if let Some(module_id) = &import_from.module {
                if let Some(new_from) = self.compute_new_from_module(import_from) {
                    self.edits.push(FileRenameEdit {
                        file: self.file,
                        range: module_id.range(),
                        new_text: new_from,
                    });
                }
            }
        } else if self.old_module_name.starts_with(&resolved_name) {
            let old_prefix_len = resolved_name.as_str().len();
            let suffix = &self.old_module_name.as_str()[old_prefix_len + 1..];
            if !suffix.contains('.') {
                for alias in &import_from.names {
                    if alias.name.as_str() == suffix {
                        let new_suffix =
                            &self.new_module_name.as_str()[old_prefix_len + 1..];
                        self.edits.push(FileRenameEdit {
                            file: self.file,
                            range: alias.name.range(),
                            new_text: new_suffix.to_string(),
                        });
                    }
                }
            }
        }
    }

    fn compute_new_from_module(&self, import_from: &ast::StmtImportFrom) -> Option<String> {
        let level = import_from.level;

        if level > 0 {
            let old_suffix = import_from.module.as_ref().map(ast::Identifier::as_str).unwrap_or("");
            let old_full = self.old_module_name.as_str();
            let new_full = self.new_module_name.as_str();

            if let Some(prefix) = old_full.strip_suffix(old_suffix) {
                if let Some(new_suffix) = new_full.strip_prefix(prefix) {
                    return Some(new_suffix.to_string());
                }
            }

            None
        } else {
            Some(self.new_module_name.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem, SystemPathBuf};
    use ty_project::{ProjectMetadata, TestDb};
    use ruff_python_ast::PythonVersion;

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

        assert_eq!(edits.len(), 1);

        let consumer = system_path_to_file(&db, "consumer.py").unwrap();
        let result = apply_edits(&db, &edits, consumer);
        assert_eq!(result, "import new_module\n\nprint(old_module.x)\n");
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
        let db = create_test_db(&[
            ("consumer.py", "import something\n"),
        ]);

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
}
