use anyhow::Result;

use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_parser::{ParseOptions, parse};
use ruff_text_size::TextRange;

use crate::collector::Collector;
pub use crate::db::ImportDb;
use crate::resolver::Resolver;
pub use ty_module_resolver::ModuleName;

mod collector;
mod db;
mod resolver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyzeOptions {
    pub string_imports: StringImports,
    pub type_checking_imports: bool,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            string_imports: StringImports::default(),
            type_checking_imports: true,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct StringImports {
    pub enabled: bool,
    pub min_dots: usize,
}

impl Default for StringImports {
    fn default() -> Self {
        Self {
            enabled: false,
            min_dots: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Import,
    ImportFrom,
    StringImport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOccurrence {
    pub importer: SystemPathBuf,
    pub kind: ImportKind,
    pub requested: ModuleName,
    pub range: Option<TextRange>,
    pub in_type_checking: bool,
    pub is_relative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedPathKind {
    FirstParty,
    StandardLibrary,
    SitePackages,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    pub occurrence: ImportOccurrence,
    pub resolved_module: Option<ModuleName>,
    pub resolved_path: Option<SystemPathBuf>,
    pub winning_root: Option<usize>,
    pub resolved_path_kind: Option<ResolvedPathKind>,
}

impl ResolvedImport {
    fn unresolved(occurrence: ImportOccurrence) -> Self {
        Self {
            occurrence,
            resolved_module: None,
            resolved_path: None,
            winning_root: None,
            resolved_path_kind: None,
        }
    }
}

pub fn analyze_file(
    db: &ImportDb,
    path: &SystemPath,
    package: Option<&SystemPath>,
    source: &str,
    source_type: PySourceType,
    options: &AnalyzeOptions,
) -> Result<Vec<ResolvedImport>> {
    // Parse the source code.
    let parsed = parse(source, ParseOptions::from(source_type))?;

    let module_path =
        package.and_then(|package| to_module_path(package.as_std_path(), path.as_std_path()));

    // Collect the imports.
    let imports = Collector::new(
        module_path.as_deref(),
        options.string_imports,
        options.type_checking_imports,
    )
    .collect(parsed.syntax());

    // Resolve the imports.
    let resolver = Resolver::new(db, path);
    Ok(imports
        .into_iter()
        .map(|import| {
            let mut resolved = resolver.resolve(import);
            resolved.occurrence.importer = path.to_path_buf();
            resolved
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use super::{
        AnalyzeOptions, ImportDb, ImportKind, ResolvedPathKind, StringImports, analyze_file,
    };
    use ruff_db::system::{OsSystem, SystemPathBuf};
    use ruff_python_ast::PythonVersion;

    #[test]
    fn analyze_file_preserves_occurrence_level_details() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_root = temp_dir.path();
        let src_root = project_root.join("src");
        let site_packages_root = project_root.join("site-packages");
        std::fs::create_dir_all(src_root.join("pkg"))?;
        std::fs::create_dir_all(src_root.join("bar"))?;
        std::fs::create_dir_all(src_root.join("alpha"))?;
        std::fs::create_dir_all(site_packages_root.join("dep"))?;

        std::fs::write(src_root.join("pkg/__init__.py"), "")?;
        std::fs::write(src_root.join("pkg/sibling.py"), "")?;
        std::fs::write(src_root.join("foo.py"), "")?;
        std::fs::write(src_root.join("bar/__init__.py"), "")?;
        std::fs::write(src_root.join("alpha/beta.py"), "")?;
        std::fs::write(site_packages_root.join("dep/__init__.py"), "")?;

        let source = r#"
import foo
from bar import baz
from . import sibling
import missing
import dep
if TYPE_CHECKING:
    import alpha.beta
value = "alpha.beta.Gamma"
"#;
        let path = src_root.join("pkg/mod.py");
        std::fs::write(&path, source)?;

        let src_root = SystemPathBuf::from_path_buf(src_root).expect("valid UTF-8 path");
        let site_packages_root =
            SystemPathBuf::from_path_buf(site_packages_root).expect("valid UTF-8 path");
        let path = SystemPathBuf::from_path_buf(path).expect("valid UTF-8 path");
        let package =
            SystemPathBuf::from_path_buf(project_root.join("src/pkg")).expect("valid UTF-8 path");

        let db = ImportDb::from_roots(
            OsSystem::default(),
            vec![src_root],
            vec![site_packages_root],
            PythonVersion::PY312,
        )?;

        let results = analyze_file(
            &db,
            &path,
            Some(&package),
            source,
            ruff_python_ast::PySourceType::Python,
            &AnalyzeOptions {
                string_imports: StringImports {
                    enabled: true,
                    min_dots: 1,
                },
                type_checking_imports: true,
            },
        )?;

        assert_eq!(results.len(), 7);

        assert_eq!(results[0].occurrence.kind, ImportKind::Import);
        assert_eq!(results[0].occurrence.requested.as_str(), "foo");
        assert_eq!(results[0].resolved_module.as_ref().unwrap().as_str(), "foo");
        assert_eq!(results[0].winning_root, Some(0));
        assert_eq!(
            results[0].resolved_path_kind,
            Some(ResolvedPathKind::FirstParty)
        );
        assert!(!results[0].occurrence.in_type_checking);
        assert_eq!(results[0].occurrence.importer, path);
        assert!(results[0].occurrence.range.is_some());

        assert_eq!(results[1].occurrence.kind, ImportKind::ImportFrom);
        assert_eq!(results[1].occurrence.requested.as_str(), "bar.baz");
        assert_eq!(results[1].resolved_module.as_ref().unwrap().as_str(), "bar");
        assert_eq!(results[1].winning_root, Some(0));

        assert_eq!(results[2].occurrence.requested.as_str(), "pkg.sibling");
        assert!(results[2].occurrence.is_relative);
        assert_eq!(
            results[2].resolved_module.as_ref().unwrap().as_str(),
            "pkg.sibling"
        );

        assert_eq!(results[3].occurrence.requested.as_str(), "missing");
        assert!(results[3].resolved_module.is_none());
        assert!(results[3].resolved_path.is_none());
        assert!(results[3].winning_root.is_none());

        assert_eq!(results[4].occurrence.requested.as_str(), "dep");
        assert_eq!(results[4].resolved_module.as_ref().unwrap().as_str(), "dep");
        assert_eq!(results[4].winning_root, Some(1));
        assert_eq!(
            results[4].resolved_path_kind,
            Some(ResolvedPathKind::SitePackages)
        );

        assert_eq!(results[5].occurrence.requested.as_str(), "alpha.beta");
        assert!(results[5].occurrence.in_type_checking);
        assert_eq!(
            results[5].resolved_module.as_ref().unwrap().as_str(),
            "alpha.beta"
        );

        assert_eq!(results[6].occurrence.kind, ImportKind::StringImport);
        assert_eq!(results[6].occurrence.requested.as_str(), "alpha.beta.Gamma");
        assert_eq!(
            results[6].resolved_module.as_ref().unwrap().as_str(),
            "alpha.beta"
        );

        Ok(())
    }

    #[test]
    fn analyze_file_skips_type_checking_imports_when_disabled() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let src_root = temp_dir.path().join("src");
        std::fs::create_dir_all(src_root.join("pkg"))?;
        std::fs::write(src_root.join("pkg/__init__.py"), "")?;
        std::fs::write(
            src_root.join("pkg/mod.py"),
            "if TYPE_CHECKING:\n    import foo\n",
        )?;

        let src_root = SystemPathBuf::from_path_buf(src_root).expect("valid UTF-8 path");
        let path = SystemPathBuf::from_path_buf(temp_dir.path().join("src/pkg/mod.py"))
            .expect("valid UTF-8 path");
        let package = SystemPathBuf::from_path_buf(temp_dir.path().join("src/pkg"))
            .expect("valid UTF-8 path");

        let db = ImportDb::from_roots(
            OsSystem::default(),
            vec![src_root],
            Vec::new(),
            PythonVersion::PY312,
        )?;

        let results = analyze_file(
            &db,
            &path,
            Some(&package),
            "if TYPE_CHECKING:\n    import foo\n",
            ruff_python_ast::PySourceType::Python,
            &AnalyzeOptions {
                string_imports: StringImports::default(),
                type_checking_imports: false,
            },
        )?;

        assert!(results.is_empty());

        Ok(())
    }
}
