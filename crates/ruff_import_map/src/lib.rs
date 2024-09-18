use crate::collector::Collector;
pub use crate::db::ModuleDb;
use crate::resolver::Resolver;
pub use crate::settings::{Direction, ImportMapSettings};
use anyhow::Result;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::PySourceType;
use ruff_python_parser::{parse, AsMode};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tracing_subscriber::Layer;

mod collector;
mod db;
mod resolver;
mod settings;

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ModuleImports(BTreeSet<PathBuf>);

impl ModuleImports {
    pub fn insert(&mut self, path: PathBuf) {
        self.0.insert(path);
    }
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImportMap(BTreeMap<PathBuf, ModuleImports>);

impl ImportMap {
    pub fn insert(&mut self, path: PathBuf, imports: ModuleImports) {
        self.0.insert(path, imports);
    }

    #[must_use]
    pub fn reverse(imports: impl IntoIterator<Item = (PathBuf, ModuleImports)>) -> Self {
        let mut reverse = ImportMap::default();
        for (path, imports) in imports {
            for import in imports.0 {
                reverse.0.entry(import).or_default().insert(path.clone());
            }
        }
        reverse
    }
}

impl FromIterator<(PathBuf, ModuleImports)> for ImportMap {
    fn from_iter<I: IntoIterator<Item = (PathBuf, ModuleImports)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

pub fn generate(
    path: &Path,
    package: Option<&Path>,
    source_type: PySourceType,
    settings: &ImportMapSettings,
    // db: &ModuleDb,
) -> Result<ModuleImports> {
    // Initialize the module database.
    let db = ModuleDb::from_settings(&settings)?;

    // Read and parse the source code.
    let source = std::fs::read_to_string(path)?;
    let parsed = parse(&source, source_type.as_mode())?;
    let module_path = package.and_then(|package| to_module_path(package, path));

    // Collect the imports.
    let imports = Collector::default().collect(parsed.syntax());

    // Resolve the imports.
    let mut resolved_imports = ModuleImports::default();
    for import in imports {
        let Some(resolved) = Resolver::new(module_path.as_deref(), &db).resolve(&import) else {
            continue;
        };
        let Some(path) = resolved.as_system_path() else {
            continue;
        };
        resolved_imports.insert(path.as_std_path().to_path_buf());
    }

    Ok(resolved_imports)
}
