use std::fmt;

use anyhow::Result;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_parser::{ParseOptions, parse};
use ruff_text_size::TextRange;
use ty_module_resolver::ModuleName;

use crate::collector::Collector;
use crate::resolver::Resolver;

pub use crate::db::ImportDb;
pub use crate::facade::{
    AnalyzerSettings, ImportAnalyzer, ImportOccurrence, ResolvedImport, SearchRoot, SearchRootKind,
};

mod collector;
mod db;
mod facade;
mod resolver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, CacheKey)]
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

impl fmt::Display for StringImports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.enabled {
            write!(f, "enabled (min_dots: {})", self.min_dots)
        } else {
            write!(f, "disabled")
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    Import,
    ImportFrom,
    StringImport { min_dots: usize },
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImportOccurrence {
    pub importer: SystemPathBuf,
    pub kind: ImportKind,
    pub requested: ModuleName,
    pub range: TextRange,
    pub in_type_checking: bool,
    pub is_relative: bool,
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResolvedImport {
    pub occurrence: RawImportOccurrence,
    pub resolved_module: Option<ModuleName>,
    pub resolved_path: Option<SystemPathBuf>,
    /// Index of the configured root that won resolution for this import.
    pub winning_root: Option<usize>,
}

#[doc(hidden)]
pub fn analyze_file(
    db: &ImportDb,
    path: &SystemPath,
    package: Option<&SystemPath>,
    source: &str,
    source_type: PySourceType,
    options: &AnalyzeOptions,
) -> Result<Vec<RawResolvedImport>> {
    let parsed = parse(source, ParseOptions::from(source_type))?;

    let module_path =
        package.and_then(|package| to_module_path(package.as_std_path(), path.as_std_path()));

    let imports = Collector::new(
        path,
        module_path.as_deref(),
        options.string_imports,
        options.type_checking_imports,
    )
    .collect(parsed.syntax());

    Ok(Resolver::new(db, path).resolve_all(imports))
}

#[cfg(test)]
mod tests;
