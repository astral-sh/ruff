use crate::collector::Collector;
pub use crate::db::ModuleDb;
use crate::resolver::Resolver;
pub use crate::settings::{AnalyzeSettings, Direction};
use anyhow::Result;
use red_knot_python_semantic::SemanticModel;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::helpers::to_module_path;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod collector;
mod db;
mod resolver;
mod settings;

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ModuleImports(BTreeSet<SystemPathBuf>);

impl ModuleImports {
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImportMap(BTreeMap<SystemPathBuf, ModuleImports>);

impl ImportMap {
    /// Insert a module's imports into the map.
    pub fn insert(&mut self, path: SystemPathBuf, imports: ModuleImports) {
        self.0.insert(path, imports);
    }

    /// Reverse the [`ImportMap`], e.g., to convert from dependencies to dependents.
    #[must_use]
    pub fn reverse(imports: impl IntoIterator<Item = (SystemPathBuf, ModuleImports)>) -> Self {
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

impl FromIterator<(SystemPathBuf, ModuleImports)> for ImportMap {
    fn from_iter<I: IntoIterator<Item = (SystemPathBuf, ModuleImports)>>(iter: I) -> Self {
        let mut map = ImportMap::default();
        for (path, imports) in iter {
            map.0.entry(path).or_default().0.extend(imports.0);
        }
        map
    }
}

/// Generate the module imports for a given Python file.
pub fn generate(
    path: &SystemPath,
    package: Option<&SystemPath>,
    string_imports: bool,
    db: &ModuleDb,
) -> Result<ModuleImports> {
    // Read and parse the source code.
    let file = system_path_to_file(db, path)?;
    let parsed = parsed_module(db, file);
    let module_path =
        package.and_then(|package| to_module_path(package.as_std_path(), path.as_std_path()));
    let model = SemanticModel::new(db, file);

    // Collect the imports.
    let imports = Collector::new(module_path.as_deref(), string_imports).collect(parsed.syntax());

    // Resolve the imports.
    let mut resolved_imports = ModuleImports::default();
    for import in imports {
        let Some(resolved) = Resolver::new(&model).resolve(import) else {
            continue;
        };
        let Some(path) = resolved.as_system_path() else {
            continue;
        };
        resolved_imports.insert(path.to_path_buf());
    }

    Ok(resolved_imports)
}
