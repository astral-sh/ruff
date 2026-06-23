use crate::goto::GotoTarget;
use crate::references::{ReferencesMode, references};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::SystemPath;
use ruff_text_size::{TextRange, TextSize};
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
/// Uses the find-references infrastructure to locate all references to the old
/// module name across the project, including:
///   - `import old_module` (alias declaration)
///   - `old_module.foo()` (name expression usage)
///   - `from pkg import old_sub` (alias declaration)
///   - `from old_module import x` (module path in from-import)
///   - `import pkg.old_sub` (dotted import name component)
///   - `pkg.old_sub.x` (attribute access on the module)
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
    let old_components: Vec<&str> = old_name_str.split('.').collect();

    let goto_target = GotoTarget::ImportModuleComponent {
        module_name: old_name_str.to_string(),
        level: 0,
        component_index: old_name_str.matches('.').count(),
        component_range: TextRange::default(),
    };

    let Some(refs) = references(db, old_file, &goto_target, ReferencesMode::RenameMultiFile) else {
        return vec![];
    };

    refs.into_iter()
        .filter(|r| r.file() != old_file)
        .map(|r| {
            let (range, new_text) = expand_dotted_range(
                db,
                r.file(),
                r.range(),
                &old_components,
                new_name_str,
                &new_last,
            );
            FileRenameEdit {
                file: r.file(),
                range,
                new_text,
            }
        })
        .collect()
}

/// Expand a reference range to cover the full dotted module path when the
/// reference is part of a larger path (e.g. `pkg.old_sub` in `import pkg.old_sub`).
///
/// For standalone references (e.g. `old_sub` in `from pkg import old_sub`),
/// returns the original range with just the last component as replacement.
fn expand_dotted_range(
    db: &dyn Db,
    file: File,
    component_range: TextRange,
    old_components: &[&str],
    new_module_name: &str,
    new_last: &str,
) -> (TextRange, String) {
    let source = source_text(db, file);
    let text = source.as_str();
    let ref_start = usize::from(component_range.start());

    // Walk backwards through parent components of the old module path,
    // checking that each ".<component>" prefix matches.
    let mut expanded_start = ref_start;
    for i in (0..old_components.len().saturating_sub(1)).rev() {
        let component = old_components[i];
        let Some(dot_pos) = expanded_start.checked_sub(1) else {
            break;
        };
        if text.as_bytes().get(dot_pos) != Some(&b'.') {
            break;
        }
        let Some(comp_start) = dot_pos.checked_sub(component.len()) else {
            break;
        };
        if text.get(comp_start..dot_pos) != Some(component) {
            break;
        }
        expanded_start = comp_start;
    }

    if expanded_start < ref_start {
        let expanded_range = TextRange::new(
            TextSize::try_from(expanded_start).unwrap(),
            component_range.end(),
        );
        (expanded_range, new_module_name.to_string())
    } else {
        (component_range, new_last.to_string())
    }
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
        assert_eq!(result, "from old_package import new_module\n");
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
}
