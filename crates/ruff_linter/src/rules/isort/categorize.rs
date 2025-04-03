use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::{fs, iter};

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
    #[allow(dead_code)]
    SourceMatch(&'a Path),
    NoMatch,
    UserDefinedSection,
    NoSections,
    #[allow(dead_code)]
    DisabledSection(&'a ImportSection),
}

#[allow(clippy::too_many_arguments)]
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
        } else if let Some(src) = match_sources(src, module_base) {
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

fn match_sources<'a>(paths: &'a [PathBuf], base: &str) -> Option<&'a Path> {
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

#[allow(clippy::too_many_arguments)]
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
