use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use ruff_python_imports::{AnalyzeOptions, analyze_file};

pub use crate::settings::{AnalyzeSettings, Direction};
pub use ruff_python_imports::ImportDb as ModuleDb;
pub use ruff_python_imports::StringImports;

mod settings;

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ModuleImports(BTreeSet<SystemPathBuf>);

impl ModuleImports {
    /// Detect the [`ModuleImports`] for a given Python file.
    pub fn detect(
        db: &ModuleDb,
        source: &str,
        source_type: PySourceType,
        path: &SystemPath,
        package: Option<&SystemPath>,
        string_imports: StringImports,
        type_checking_imports: bool,
    ) -> Result<Self> {
        let mut resolved_imports = ModuleImports::default();
        for resolved in analyze_file(
            db,
            path,
            package,
            source,
            source_type,
            &AnalyzeOptions {
                string_imports,
                type_checking_imports,
            },
        )? {
            if let Some(path) = resolved.resolved_path {
                resolved_imports.insert(path);
            }
        }

        Ok(resolved_imports)
    }

    /// Insert a file path into the module imports.
    pub fn insert(&mut self, path: SystemPathBuf) {
        self.0.insert(path);
    }

    /// Extend the module imports with additional file paths.
    pub fn extend(&mut self, paths: impl IntoIterator<Item = SystemPathBuf>) {
        self.0.extend(paths);
    }

    /// Returns `true` if the module imports are empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of module imports.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Convert the file paths to be relative to a given path.
    #[must_use]
    pub fn relative_to(self, path: &SystemPath) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|import| {
                    import
                        .strip_prefix(path)
                        .map(SystemPath::to_path_buf)
                        .unwrap_or(import)
                })
                .collect(),
        )
    }
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImportMap(BTreeMap<SystemPathBuf, ModuleImports>);

impl ImportMap {
    /// Create an [`ImportMap`] of file to its dependencies.
    ///
    /// Assumes that the input is a collection of unique file paths and their imports.
    pub fn dependencies(imports: impl IntoIterator<Item = (SystemPathBuf, ModuleImports)>) -> Self {
        let mut map = ImportMap::default();
        for (path, imports) in imports {
            map.0.insert(path, imports);
        }
        map
    }

    /// Create an [`ImportMap`] of file to its dependents.
    ///
    /// Assumes that the input is a collection of unique file paths and their imports.
    pub fn dependents(imports: impl IntoIterator<Item = (SystemPathBuf, ModuleImports)>) -> Self {
        let mut reverse = ImportMap::default();
        for (path, imports) in imports {
            for import in imports.0 {
                reverse.0.entry(import).or_default().insert(path.clone());
            }
            reverse.0.entry(path).or_default();
        }
        reverse
    }
}
