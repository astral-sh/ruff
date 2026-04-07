use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ruff_db::system::{OsSystem, System, SystemPathBuf};
use ruff_python_ast::PySourceType;

use crate::db::{ImportDb, deduplicated_root_paths};
use crate::{AnalyzeOptions, ImportKind, RawImportOccurrence, RawResolvedImport};

/// A caller-defined search root for import resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRoot {
    /// Stable identifier returned as [`ResolvedImport::winning_root_id`].
    pub id: usize,
    /// Path to the root on disk.
    pub path: PathBuf,
    /// How this root participates in resolution.
    pub kind: SearchRootKind,
}

/// The role a search root plays in import resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRootKind {
    /// A first-party or workspace source root.
    Source,
    /// A site-packages root.
    SitePackages,
}

/// Settings for constructing an [`ImportAnalyzer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerSettings {
    /// Search roots used for import resolution.
    pub roots: Vec<SearchRoot>,
    /// Python version to assume during parsing and resolution.
    pub python_version: (u8, u8),
}

/// A plain-data import occurrence returned by the facade API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOccurrence {
    pub importer: PathBuf,
    pub kind: ImportKind,
    pub requested: String,
    /// Byte offsets within the source file.
    pub range: Range<u32>,
    pub in_type_checking: bool,
    pub is_relative: bool,
}

/// A plain-data resolved import returned by the facade API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    pub occurrence: ImportOccurrence,
    pub resolved_module: Option<String>,
    pub resolved_path: Option<PathBuf>,
    /// The caller-supplied [`SearchRoot::id`] that won resolution.
    ///
    /// This is `None` if the import is unresolved or if it resolved outside the configured
    /// roots, such as to the standard library.
    pub winning_root_id: Option<usize>,
}

/// High-level facade for Python import collection and resolution.
#[derive(Clone)]
pub struct ImportAnalyzer {
    db: ImportDb,
    root_ids: Box<[usize]>,
}

impl ImportAnalyzer {
    /// Construct an analyzer from caller-defined search roots.
    ///
    /// Roots are canonicalized and deduplicated internally. If multiple configured roots resolve
    /// to the same canonical path, the first root's ID wins.
    pub fn new(settings: AnalyzerSettings) -> Result<Self> {
        let system = OsSystem::default();

        let mut source_roots = Vec::new();
        let mut site_packages_roots = Vec::new();
        let mut configured_roots = Vec::new();

        for root in settings.roots {
            let path = system_path_buf(&root.path)?;
            match root.kind {
                SearchRootKind::Source => source_roots.push(path.clone()),
                SearchRootKind::SitePackages => site_packages_roots.push(path.clone()),
            }
            configured_roots.push((root.id, path));
        }

        let root_paths =
            deduplicated_root_paths(&system, source_roots.clone(), site_packages_roots.clone());
        let root_ids = root_paths
            .iter()
            .filter_map(|deduplicated_path| {
                configured_roots.iter().find_map(|(id, configured_path)| {
                    let configured_path = system
                        .canonicalize_path(configured_path)
                        .unwrap_or_else(|_| configured_path.clone());
                    (configured_path == *deduplicated_path).then_some(*id)
                })
            })
            .collect::<Vec<_>>();

        debug_assert_eq!(root_paths.len(), root_ids.len());

        let db = ImportDb::from_roots(
            system,
            source_roots,
            site_packages_roots,
            settings.python_version.into(),
        )?;

        Ok(Self {
            db,
            root_ids: root_ids.into_boxed_slice(),
        })
    }

    /// Analyze a Python source string at `path`.
    pub fn analyze_source(
        &self,
        path: &Path,
        package_root: Option<&Path>,
        source: &str,
        options: &AnalyzeOptions,
    ) -> Result<Vec<ResolvedImport>> {
        let path = system_path_buf(path)?;
        let package_root = package_root.map(system_path_buf).transpose()?;
        let source_type = PySourceType::from(path.as_std_path());

        let imports = crate::analyze_file(
            &self.db,
            path.as_path(),
            package_root.as_ref().map(SystemPathBuf::as_path),
            source,
            source_type,
            options,
        )?;

        Ok(imports
            .into_iter()
            .map(|import| convert_import(import, &self.root_ids))
            .collect())
    }

    /// Read and analyze a Python source file from disk.
    pub fn analyze_path(
        &self,
        path: &Path,
        package_root: Option<&Path>,
        options: &AnalyzeOptions,
    ) -> Result<Vec<ResolvedImport>> {
        let source = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        self.analyze_source(path, package_root, &source, options)
    }
}

fn convert_import(import: RawResolvedImport, root_ids: &[usize]) -> ResolvedImport {
    let RawResolvedImport {
        occurrence,
        resolved_module,
        resolved_path,
        winning_root,
    } = import;
    let RawImportOccurrence {
        importer,
        kind,
        requested,
        range,
        in_type_checking,
        is_relative,
    } = occurrence;

    ResolvedImport {
        occurrence: ImportOccurrence {
            importer: importer.into_std_path_buf(),
            kind,
            requested: requested.as_str().to_string(),
            range: range.start().to_u32()..range.end().to_u32(),
            in_type_checking,
            is_relative,
        },
        resolved_module: resolved_module.map(|module| module.as_str().to_string()),
        resolved_path: resolved_path.map(SystemPathBuf::into_std_path_buf),
        winning_root_id: winning_root.map(|index| root_ids[index]),
    }
}

fn system_path_buf(path: &Path) -> Result<SystemPathBuf> {
    SystemPathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| anyhow::anyhow!("Expected UTF-8 path, got {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{AnalyzerSettings, ImportAnalyzer, ResolvedImport, SearchRoot, SearchRootKind};
    use crate::{AnalyzeOptions, StringImports};
    use anyhow::Result;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn analyzer(roots: Vec<SearchRoot>) -> Result<ImportAnalyzer> {
        ImportAnalyzer::new(AnalyzerSettings {
            roots,
            python_version: (3, 12),
        })
    }

    fn default_options() -> AnalyzeOptions {
        AnalyzeOptions {
            string_imports: StringImports::default(),
            type_checking_imports: true,
        }
    }

    #[test]
    fn winning_root_ids_are_reported_for_source_and_site_packages() -> Result<()> {
        let tempdir = TempDir::new()?;
        let root = tempdir.path();

        let src = root.join("src");
        let site_packages = root.join("site-packages");
        write_file(&src.join("myapp.py"), "");
        write_file(&site_packages.join("dep/__init__.py"), "");

        let importer_path = src.join("main.py");
        write_file(&importer_path, "import myapp\nimport dep\n");

        let analyzer = analyzer(vec![
            SearchRoot {
                id: 10,
                path: src,
                kind: SearchRootKind::Source,
            },
            SearchRoot {
                id: 20,
                path: site_packages,
                kind: SearchRootKind::SitePackages,
            },
        ])?;

        let imports = analyzer.analyze_path(&importer_path, None, &default_options())?;

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].occurrence.requested, "myapp");
        assert_eq!(imports[0].winning_root_id, Some(10));
        assert_eq!(imports[1].occurrence.requested, "dep");
        assert_eq!(imports[1].winning_root_id, Some(20));

        Ok(())
    }

    #[test]
    fn duplicate_roots_preserve_the_first_root_id() -> Result<()> {
        let tempdir = TempDir::new()?;
        let root = tempdir.path();
        let first = root.join("first");

        write_file(&first.join("foo.py"), "");

        let importer_path = first.join("main.py");
        write_file(&importer_path, "import foo\n");

        let analyzer = analyzer(vec![
            SearchRoot {
                id: 10,
                path: first,
                kind: SearchRootKind::Source,
            },
            SearchRoot {
                id: 20,
                path: root.join("first/./../first"),
                kind: SearchRootKind::Source,
            },
        ])?;

        let imports = analyzer.analyze_path(&importer_path, None, &default_options())?;

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].occurrence.requested, "foo");
        assert_eq!(imports[0].winning_root_id, Some(10));

        Ok(())
    }

    #[test]
    fn analyze_path_reads_source_and_preserves_plain_data() -> Result<()> {
        let tempdir = TempDir::new()?;
        let root = tempdir.path();

        write_file(&root.join("foo.py"), "");

        let importer_path = root.join("main.py");
        write_file(&importer_path, "import foo\n");

        let analyzer = analyzer(vec![SearchRoot {
            id: 10,
            path: root.to_path_buf(),
            kind: SearchRootKind::Source,
        }])?;

        let imports = analyzer.analyze_path(&importer_path, None, &default_options())?;

        assert_eq!(
            imports,
            vec![ResolvedImport {
                occurrence: super::ImportOccurrence {
                    importer: importer_path,
                    kind: crate::ImportKind::Import,
                    requested: "foo".to_string(),
                    range: 7..10,
                    in_type_checking: false,
                    is_relative: false,
                },
                resolved_module: Some("foo".to_string()),
                resolved_path: Some(root.join("foo.py")),
                winning_root_id: Some(10),
            }]
        );

        Ok(())
    }
}
