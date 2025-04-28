use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::iter;
use std::path::{Path, PathBuf};

use log::debug;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::package::PackageRoot;
use crate::warn_user_once;
use ruff_macros::CacheKey;
use ruff_python_ast::PythonVersion;
use ruff_python_stdlib::sys::is_known_standard_library;

use super::types::{ImportBlock, Importable};

#[derive(
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Copy,
    Clone,
    Hash,
    Serialize,
    Deserialize,
    CacheKey,
    EnumIter,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ImportType {
    Future,
    StandardLibrary,
    ThirdParty,
    FirstParty,
    LocalFolder,
}

impl fmt::Display for ImportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Future => write!(f, "future"),
            Self::StandardLibrary => write!(f, "standard_library"),
            Self::ThirdParty => write!(f, "third_party"),
            Self::FirstParty => write!(f, "first_party"),
            Self::LocalFolder => write!(f, "local_folder"),
        }
    }
}

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, CacheKey)]
#[serde(untagged)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ImportSection {
    Known(ImportType),
    UserDefined(String),
}

impl fmt::Display for ImportSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(import_type) => write!(f, "known {{ type = {import_type} }}",),
            Self::UserDefined(string) => fmt::Debug::fmt(string, f),
        }
    }
}

#[derive(Debug)]
enum Reason<'a> {
    NonZeroLevel,
    KnownFirstParty,
    KnownThirdParty,
    KnownLocalFolder,
    ExtraStandardLibrary,
    Future,
    KnownStandardLibrary,
    SamePackage,
    #[expect(dead_code)]
    SourceMatch(&'a Path),
    NoMatch,
    UserDefinedSection,
    NoSections,
    #[expect(dead_code)]
    DisabledSection(&'a ImportSection),
}

#[expect(clippy::too_many_arguments)]
pub(crate) fn categorize<'a>(
    module_name: &str,
    is_relative: bool,
    src: &[PathBuf],
    package: Option<PackageRoot<'_>>,
    detect_same_package: bool,
    known_modules: &'a KnownModules,
    target_version: PythonVersion,
    no_sections: bool,
    section_order: &'a [ImportSection],
    default_section: &'a ImportSection,
    match_source_strategy: MatchSourceStrategy,
) -> &'a ImportSection {
    let module_base = module_name.split('.').next().unwrap();
    let (mut import_type, mut reason) = {
        if !is_relative && module_base == "__future__" {
            (&ImportSection::Known(ImportType::Future), Reason::Future)
        } else if no_sections {
            (
                &ImportSection::Known(ImportType::FirstParty),
                Reason::NoSections,
            )
        } else if is_relative {
            (
                &ImportSection::Known(ImportType::LocalFolder),
                Reason::NonZeroLevel,
            )
        } else if let Some((import_type, reason)) = known_modules.categorize(module_name) {
            (import_type, reason)
        } else if is_known_standard_library(target_version.minor, module_base) {
            (
                &ImportSection::Known(ImportType::StandardLibrary),
                Reason::KnownStandardLibrary,
            )
        } else if detect_same_package && same_package(package, module_base) {
            (
                &ImportSection::Known(ImportType::FirstParty),
                Reason::SamePackage,
            )
        } else if let Some(src) = match_sources(src, module_name, match_source_strategy) {
            (
                &ImportSection::Known(ImportType::FirstParty),
                Reason::SourceMatch(src),
            )
        } else if !is_relative && module_name == "__main__" {
            (
                &ImportSection::Known(ImportType::FirstParty),
                Reason::KnownFirstParty,
            )
        } else {
            (default_section, Reason::NoMatch)
        }
    };
    // If a value is not in `section_order` then map it to `default_section`.
    if !section_order.contains(import_type) {
        reason = Reason::DisabledSection(import_type);
        import_type = default_section;
    }
    debug!("Categorized '{module_name}' as {import_type:?} ({reason:?})");
    import_type
}

fn same_package(package: Option<PackageRoot<'_>>, module_base: &str) -> bool {
    package
        .map(PackageRoot::path)
        .is_some_and(|package| package.ends_with(module_base))
}

/// Returns the source path with respect to which the module `name`
/// should be considered first party, or `None` if no path is found.
///
/// The [`MatchSourceStrategy`] is the criterion used to decide whether
/// the module path matches a given source directory.
///
/// # Examples
///
/// - The module named `foo` will match `[SRC]` if `[SRC]/foo` is a directory,
///   no matter the strategy.
///
/// - With `match_source_strategy == MatchSourceStrategy::Root`, the module
///   named `foo.baz` will match `[SRC]` if `[SRC]/foo` is a
///   directory or `[SRC]/foo.py` exists.
///
/// - With `match_source_stratgy == MatchSourceStrategy::FullPath`, the module
///   named `foo.baz` will match `[SRC]` only if `[SRC]/foo/baz` is a directory,
///   or `[SRC]/foo/baz.py` exists or `[SRC]/foo/baz.pyi` exists.
fn match_sources<'a>(
    paths: &'a [PathBuf],
    name: &str,
    match_source_strategy: MatchSourceStrategy,
) -> Option<&'a Path> {
    match match_source_strategy {
        MatchSourceStrategy::Root => {
            let base = name.split('.').next()?;
            for path in paths {
                if let Ok(metadata) = fs::metadata(path.join(base)) {
                    if metadata.is_dir() {
                        return Some(path);
                    }
                }
                if let Ok(metadata) = fs::metadata(path.join(format!("{base}.py"))) {
                    if metadata.is_file() {
                        return Some(path);
                    }
                }
            }
            None
        }
        MatchSourceStrategy::FullPath => {
            let relative_path: PathBuf = name.split('.').collect();
            relative_path.components().next()?;
            for root in paths {
                let candidate = root.join(&relative_path);
                if candidate.is_dir() {
                    return Some(root);
                }
                if ["py", "pyi"]
                    .into_iter()
                    .any(|extension| candidate.with_extension(extension).is_file())
                {
                    return Some(root);
                }
            }
            None
        }
    }
}

#[expect(clippy::too_many_arguments)]
pub(crate) fn categorize_imports<'a>(
    block: ImportBlock<'a>,
    src: &[PathBuf],
    package: Option<PackageRoot<'_>>,
    detect_same_package: bool,
    known_modules: &'a KnownModules,
    target_version: PythonVersion,
    no_sections: bool,
    section_order: &'a [ImportSection],
    default_section: &'a ImportSection,
    match_source_strategy: MatchSourceStrategy,
) -> BTreeMap<&'a ImportSection, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<&ImportSection, ImportBlock> = BTreeMap::default();
    // Categorize `Stmt::Import`.
    for (alias, comments) in block.import {
        let import_type = categorize(
            &alias.module_name(),
            false,
            src,
            package,
            detect_same_package,
            known_modules,
            target_version,
            no_sections,
            section_order,
            default_section,
            match_source_strategy,
        );
        block_by_type
            .entry(import_type)
            .or_default()
            .import
            .insert(alias, comments);
    }
    // Categorize `Stmt::ImportFrom` (without re-export).
    for (import_from, aliases) in block.import_from {
        let classification = categorize(
            &import_from.module_name(),
            import_from.level > 0,
            src,
            package,
            detect_same_package,
            known_modules,
            target_version,
            no_sections,
            section_order,
            default_section,
            match_source_strategy,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from
            .insert(import_from, aliases);
    }
    // Categorize `Stmt::ImportFrom` (with re-export).
    for ((import_from, alias), aliases) in block.import_from_as {
        let classification = categorize(
            &import_from.module_name(),
            import_from.level > 0,
            src,
            package,
            detect_same_package,
            known_modules,
            target_version,
            no_sections,
            section_order,
            default_section,
            match_source_strategy,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_as
            .insert((import_from, alias), aliases);
    }
    // Categorize `Stmt::ImportFrom` (with star).
    for (import_from, comments) in block.import_from_star {
        let classification = categorize(
            &import_from.module_name(),
            import_from.level > 0,
            src,
            package,
            detect_same_package,
            known_modules,
            target_version,
            no_sections,
            section_order,
            default_section,
            match_source_strategy,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_star
            .insert(import_from, comments);
    }
    block_by_type
}

#[derive(Debug, Clone, Default, CacheKey)]
pub struct KnownModules {
    /// A map of known modules to their section.
    known: Vec<(glob::Pattern, ImportSection)>,
    /// Whether any of the known modules are submodules (e.g., `foo.bar`, as opposed to `foo`).
    has_submodules: bool,
}

impl KnownModules {
    pub fn new(
        first_party: Vec<glob::Pattern>,
        third_party: Vec<glob::Pattern>,
        local_folder: Vec<glob::Pattern>,
        standard_library: Vec<glob::Pattern>,
        user_defined: FxHashMap<String, Vec<glob::Pattern>>,
    ) -> Self {
        let known: Vec<(glob::Pattern, ImportSection)> = user_defined
            .into_iter()
            .flat_map(|(section, modules)| {
                modules
                    .into_iter()
                    .map(move |module| (module, ImportSection::UserDefined(section.clone())))
            })
            .chain(
                first_party
                    .into_iter()
                    .map(|module| (module, ImportSection::Known(ImportType::FirstParty))),
            )
            .chain(
                third_party
                    .into_iter()
                    .map(|module| (module, ImportSection::Known(ImportType::ThirdParty))),
            )
            .chain(
                local_folder
                    .into_iter()
                    .map(|module| (module, ImportSection::Known(ImportType::LocalFolder))),
            )
            .chain(
                standard_library
                    .into_iter()
                    .map(|module| (module, ImportSection::Known(ImportType::StandardLibrary))),
            )
            .collect();

        // Warn in the case of duplicate modules.
        let mut seen = FxHashSet::with_capacity_and_hasher(known.len(), FxBuildHasher);
        for (module, _) in &known {
            if !seen.insert(module) {
                warn_user_once!("One or more modules are part of multiple import sections, including: `{module}`");
                break;
            }
        }

        // Check if any of the known modules are submodules.
        let has_submodules = known
            .iter()
            .any(|(module, _)| module.as_str().contains('.'));

        Self {
            known,
            has_submodules,
        }
    }

    /// Return the [`ImportSection`] for a given module, if it's been categorized as a known module
    /// by the user.
    fn categorize(&self, module_name: &str) -> Option<(&ImportSection, Reason)> {
        if self.has_submodules {
            // Check all module prefixes from the longest to the shortest (e.g., given
            // `foo.bar.baz`, check `foo.bar.baz`, then `foo.bar`, then `foo`, taking the first,
            // most precise match).
            for i in module_name
                .match_indices('.')
                .map(|(i, _)| i)
                .chain(iter::once(module_name.len()))
                .rev()
            {
                let submodule = &module_name[0..i];
                if let Some(result) = self.categorize_submodule(submodule) {
                    return Some(result);
                }
            }
            None
        } else {
            // Happy path: no submodules, so we can check the module base and be done.
            let module_base = module_name.split('.').next().unwrap();
            self.categorize_submodule(module_base)
        }
    }

    fn categorize_submodule(&self, submodule: &str) -> Option<(&ImportSection, Reason)> {
        let section = self.known.iter().find_map(|(pattern, section)| {
            if pattern.matches(submodule) {
                Some(section)
            } else {
                None
            }
        })?;
        let reason = match section {
            ImportSection::UserDefined(_) => Reason::UserDefinedSection,
            ImportSection::Known(ImportType::FirstParty) => Reason::KnownFirstParty,
            ImportSection::Known(ImportType::ThirdParty) => Reason::KnownThirdParty,
            ImportSection::Known(ImportType::LocalFolder) => Reason::KnownLocalFolder,
            ImportSection::Known(ImportType::StandardLibrary) => Reason::ExtraStandardLibrary,
            ImportSection::Known(ImportType::Future) => {
                unreachable!("__future__ imports are not known")
            }
        };
        Some((section, reason))
    }

    /// Return the list of user-defined modules, indexed by section.
    pub fn user_defined(&self) -> FxHashMap<&str, Vec<&glob::Pattern>> {
        let mut user_defined: FxHashMap<&str, Vec<&glob::Pattern>> = FxHashMap::default();
        for (module, section) in &self.known {
            if let ImportSection::UserDefined(section_name) = section {
                user_defined
                    .entry(section_name.as_str())
                    .or_default()
                    .push(module);
            }
        }
        user_defined
    }
}

impl fmt::Display for KnownModules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.known.is_empty() {
            write!(f, "{{}}")?;
        } else {
            writeln!(f, "{{")?;
            for (pattern, import_section) in &self.known {
                writeln!(f, "\t{pattern} => {import_section:?},")?;
            }
            write!(f, "}}")?;
        }
        Ok(())
    }
}

/// Rule to determine whether a module path matches
/// a relative path from a source directory.
#[derive(Debug, Clone, Copy)]
pub(crate) enum MatchSourceStrategy {
    /// Matches if first term in module path is found in file system
    ///
    /// # Example
    /// Module is `foo.bar.baz` and `[SRC]/foo` exists
    Root,
    /// Matches only if full module path is reflected in file system
    ///
    /// # Example
    /// Module is `foo.bar.baz` and `[SRC]/foo/bar/baz` exists
    FullPath,
}

#[cfg(test)]
mod tests {
    use crate::rules::isort::categorize::{match_sources, MatchSourceStrategy};

    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    /// Helper function to create a file with parent directories
    fn create_file<P: AsRef<Path>>(path: P) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "").unwrap();
    }

    /// Helper function to create a directory and all parent directories
    fn create_dir<P: AsRef<Path>>(path: P) {
        fs::create_dir_all(path).unwrap();
    }

    /// Tests a traditional Python package layout:
    /// ```
    /// project/
    /// └── mypackage/
    ///     ├── __init__.py
    ///     ├── module1.py
    ///     └── module2.py
    /// ```
    #[test]
    fn test_traditional_layout() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");

        // Create traditional layout
        create_dir(project_dir.join("mypackage"));
        create_file(project_dir.join("mypackage/__init__.py"));
        create_file(project_dir.join("mypackage/module1.py"));
        create_file(project_dir.join("mypackage/module2.py"));

        let paths = vec![project_dir.clone()];

        // Test with Root strategy

        assert_eq!(
            match_sources(&paths, "mypackage", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "mypackage.module1", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "mypackage.nonexistent", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "nonexistent", MatchSourceStrategy::Root),
            None
        );

        // Test with FullPath strategy

        assert_eq!(
            match_sources(&paths, "mypackage", MatchSourceStrategy::FullPath),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "mypackage.module1", MatchSourceStrategy::FullPath),
            Some(project_dir.as_path())
        );

        // Differs in behavior from [`MatchSourceStrategy::Root`]
        assert_eq!(
            match_sources(
                &paths,
                "mypackage.nonexistent",
                MatchSourceStrategy::FullPath
            ),
            None
        );
    }

    /// Tests a src-based Python package layout:
    /// ```
    /// project/
    /// └── src/
    ///     └── mypackage/
    ///         ├── __init__.py
    ///         └── module1.py
    /// ```
    #[test]
    fn test_src_layout() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");
        let src_dir = project_dir.join("src");

        // Create src layout
        create_dir(src_dir.join("mypackage"));
        create_file(src_dir.join("mypackage/__init__.py"));
        create_file(src_dir.join("mypackage/module1.py"));

        let paths = vec![src_dir.clone()];

        // Test with Root strategy

        assert_eq!(
            match_sources(&paths, "mypackage", MatchSourceStrategy::Root),
            Some(src_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "mypackage.module1", MatchSourceStrategy::Root),
            Some(src_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "mypackage.nonexistent", MatchSourceStrategy::Root),
            Some(src_dir.as_path())
        );

        // Test with FullPath strategy

        assert_eq!(
            match_sources(&paths, "mypackage.module1", MatchSourceStrategy::FullPath),
            Some(src_dir.as_path())
        );

        // Differs in behavior from [`MatchSourceStrategy::Root`]
        assert_eq!(
            match_sources(
                &paths,
                "mypackage.nonexistent",
                MatchSourceStrategy::FullPath
            ),
            None
        );
    }

    /// Tests a nested package layout:
    /// ```
    /// project/
    /// └── mypackage/
    ///     ├── __init__.py
    ///     ├── module1.py
    ///     └── subpackage/
    ///         ├── __init__.py
    ///         └── module2.py
    /// ```
    #[test]
    fn test_nested_packages() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");

        // Create nested package layout
        create_dir(project_dir.join("mypackage/subpackage"));
        create_file(project_dir.join("mypackage/__init__.py"));
        create_file(project_dir.join("mypackage/module1.py"));
        create_file(project_dir.join("mypackage/subpackage/__init__.py"));
        create_file(project_dir.join("mypackage/subpackage/module2.py"));

        let paths = vec![project_dir.clone()];

        // Test with Root strategy
        assert_eq!(
            match_sources(&paths, "mypackage", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "mypackage.subpackage", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        // Test with FullPath strategy

        assert_eq!(
            match_sources(
                &paths,
                "mypackage.subpackage.module2",
                MatchSourceStrategy::FullPath
            ),
            Some(project_dir.as_path())
        );

        // Differs in behavior from [`MatchSourceStrategy::Root`]
        assert_eq!(
            match_sources(
                &paths,
                "mypackage.subpackage.nonexistent",
                MatchSourceStrategy::FullPath
            ),
            None
        );
    }

    /// Tests a namespace package layout (PEP 420):
    /// ```
    /// project/
    /// └── namespace/        # No __init__.py (namespace package)
    ///     └── package1/
    ///         ├── __init__.py
    ///         └── module1.py
    /// ```
    #[test]
    fn test_namespace_packages() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");

        // Create namespace package layout
        create_dir(project_dir.join("namespace/package1"));
        create_file(project_dir.join("namespace/package1/__init__.py"));
        create_file(project_dir.join("namespace/package1/module1.py"));

        let paths = vec![project_dir.clone()];
        // Test with Root strategy

        assert_eq!(
            match_sources(&paths, "namespace", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(&paths, "namespace.package1", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(
                &paths,
                "namespace.package2.module1",
                MatchSourceStrategy::Root
            ),
            Some(project_dir.as_path())
        );

        // Test with FullPath strategy

        assert_eq!(
            match_sources(&paths, "namespace.package1", MatchSourceStrategy::FullPath),
            Some(project_dir.as_path())
        );

        assert_eq!(
            match_sources(
                &paths,
                "namespace.package1.module1",
                MatchSourceStrategy::FullPath
            ),
            Some(project_dir.as_path())
        );

        // Differs in behavior from [`MatchSourceStrategy::Root`]
        assert_eq!(
            match_sources(
                &paths,
                "namespace.package2.module1",
                MatchSourceStrategy::FullPath
            ),
            None
        );
    }

    /// Tests a package with type stubs (.pyi files):
    /// ```
    /// project/
    /// └── mypackage/
    ///     ├── __init__.py
    ///     └── module1.pyi   # Only .pyi file, no .py
    /// ```
    #[test]
    fn test_type_stubs() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");

        // Create package with type stub
        create_dir(project_dir.join("mypackage"));
        create_file(project_dir.join("mypackage/__init__.py"));
        create_file(project_dir.join("mypackage/module1.pyi")); // Only create .pyi file, not .py

        // Test with FullPath strategy
        let paths = vec![project_dir.clone()];

        // Module "mypackage.module1" should match project_dir using .pyi file
        assert_eq!(
            match_sources(&paths, "mypackage.module1", MatchSourceStrategy::FullPath),
            Some(project_dir.as_path())
        );
    }

    /// Tests a package with both a module and a directory having the same name:
    /// ```
    /// project/
    /// └── mypackage/
    ///     ├── __init__.py
    ///     ├── feature.py      # Module with same name as directory
    ///     └── feature/        # Directory with same name as module
    ///         ├── __init__.py
    ///         └── submodule.py
    /// ```
    #[test]
    fn test_same_name_module_and_directory() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");

        // Create package with module and directory of the same name
        create_dir(project_dir.join("mypackage/feature"));
        create_file(project_dir.join("mypackage/__init__.py"));
        create_file(project_dir.join("mypackage/feature.py")); // Module with same name as directory
        create_file(project_dir.join("mypackage/feature/__init__.py"));
        create_file(project_dir.join("mypackage/feature/submodule.py"));

        // Test with Root strategy
        let paths = vec![project_dir.clone()];

        // Module "mypackage.feature" should match project_dir (matches the file first)
        assert_eq!(
            match_sources(&paths, "mypackage.feature", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );

        // Test with FullPath strategy

        // Module "mypackage.feature" should match project_dir
        assert_eq!(
            match_sources(&paths, "mypackage.feature", MatchSourceStrategy::FullPath),
            Some(project_dir.as_path())
        );

        // Module "mypackage.feature.submodule" should match project_dir
        assert_eq!(
            match_sources(
                &paths,
                "mypackage.feature.submodule",
                MatchSourceStrategy::FullPath
            ),
            Some(project_dir.as_path())
        );
    }

    /// Tests multiple source directories with different packages:
    /// ```
    /// project1/
    /// └── package1/
    ///     ├── __init__.py
    ///     └── module1.py
    ///
    /// project2/
    /// └── package2/
    ///     ├── __init__.py
    ///     └── module2.py
    /// ```
    #[test]
    fn test_multiple_source_paths() {
        let temp_dir = tempdir().unwrap();
        let project1_dir = temp_dir.path().join("project1");
        let project2_dir = temp_dir.path().join("project2");

        // Create files in project1
        create_dir(project1_dir.join("package1"));
        create_file(project1_dir.join("package1/__init__.py"));
        create_file(project1_dir.join("package1/module1.py"));

        // Create files in project2
        create_dir(project2_dir.join("package2"));
        create_file(project2_dir.join("package2/__init__.py"));
        create_file(project2_dir.join("package2/module2.py"));

        // Test with multiple paths in search order
        let paths = vec![project1_dir.clone(), project2_dir.clone()];

        // Module "package1" should match project1_dir
        assert_eq!(
            match_sources(&paths, "package1", MatchSourceStrategy::Root),
            Some(project1_dir.as_path())
        );

        // Module "package2" should match project2_dir
        assert_eq!(
            match_sources(&paths, "package2", MatchSourceStrategy::Root),
            Some(project2_dir.as_path())
        );

        // Try with reversed order to check search order
        let paths_reversed = vec![project2_dir, project1_dir.clone()];

        // Module "package1" should still match project1_dir
        assert_eq!(
            match_sources(&paths_reversed, "package1", MatchSourceStrategy::Root),
            Some(project1_dir.as_path())
        );
    }

    /// Tests behavior with an empty module name
    /// ```
    /// project/
    /// └── mypackage/
    /// ```
    ///
    /// In theory this should never happen since we expect
    /// module names to have been normalized by the time we
    /// call `match_sources`. But it is worth noting that the
    /// behavior is different depending on the [`MatchSourceStrategy`]
    #[test]
    fn test_empty_module_name() {
        let temp_dir = tempdir().unwrap();
        let project_dir = temp_dir.path().join("project");

        create_dir(project_dir.join("mypackage"));

        let paths = vec![project_dir.clone()];

        assert_eq!(
            match_sources(&paths, "", MatchSourceStrategy::Root),
            Some(project_dir.as_path())
        );
        assert_eq!(
            match_sources(&paths, "", MatchSourceStrategy::FullPath),
            None
        );
    }

    /// Tests behavior with an empty list of source paths
    #[test]
    fn test_empty_paths() {
        let paths: Vec<PathBuf> = vec![];

        // Empty paths should return None
        assert_eq!(
            match_sources(&paths, "mypackage", MatchSourceStrategy::Root),
            None
        );
        assert_eq!(
            match_sources(&paths, "mypackage", MatchSourceStrategy::FullPath),
            None
        );
    }
}
