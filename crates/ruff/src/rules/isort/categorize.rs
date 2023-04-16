use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, iter};

use log::debug;
use rustc_hash::FxHashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use ruff_macros::CacheKey;
use ruff_python_stdlib::sys::KNOWN_STANDARD_LIBRARY;

use crate::settings::types::PythonVersion;
use crate::warn_user_once;

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
    JsonSchema,
    CacheKey,
    EnumIter,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum ImportType {
    Future,
    StandardLibrary,
    ThirdParty,
    FirstParty,
    LocalFolder,
}

#[derive(
    Debug, PartialOrd, Ord, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, JsonSchema, CacheKey,
)]
#[serde(untagged)]
pub enum ImportSection {
    Known(ImportType),
    UserDefined(String),
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
    SourceMatch(&'a Path),
    NoMatch,
    UserDefinedSection,
}

#[allow(clippy::too_many_arguments)]
pub fn categorize<'a>(
    module_name: &str,
    level: Option<usize>,
    src: &[PathBuf],
    package: Option<&Path>,
    known_modules: &'a KnownModules,
    target_version: PythonVersion,
) -> &'a ImportSection {
    let module_base = module_name.split('.').next().unwrap();
    let (import_type, reason) = {
        if level.map_or(false, |level| level > 0) {
            (
                &ImportSection::Known(ImportType::LocalFolder),
                Reason::NonZeroLevel,
            )
        } else if module_base == "__future__" {
            (&ImportSection::Known(ImportType::Future), Reason::Future)
        } else if let Some((import_type, reason)) = known_modules.categorize(module_name) {
            (import_type, reason)
        } else if KNOWN_STANDARD_LIBRARY
            .get(&target_version.as_tuple())
            .unwrap()
            .contains(module_base)
        {
            (
                &ImportSection::Known(ImportType::StandardLibrary),
                Reason::KnownStandardLibrary,
            )
        } else if same_package(package, module_base) {
            (
                &ImportSection::Known(ImportType::FirstParty),
                Reason::SamePackage,
            )
        } else if let Some(src) = match_sources(src, module_base) {
            (
                &ImportSection::Known(ImportType::FirstParty),
                Reason::SourceMatch(src),
            )
        } else {
            (
                &ImportSection::Known(ImportType::ThirdParty),
                Reason::NoMatch,
            )
        }
    };
    debug!(
        "Categorized '{}' as {:?} ({:?})",
        module_name, import_type, reason
    );
    import_type
}

fn same_package(package: Option<&Path>, module_base: &str) -> bool {
    package.map_or(false, |package| package.ends_with(module_base))
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
pub fn categorize_imports<'a>(
    block: ImportBlock<'a>,
    src: &[PathBuf],
    package: Option<&Path>,
    known_modules: &'a KnownModules,
    target_version: PythonVersion,
) -> BTreeMap<&'a ImportSection, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<&ImportSection, ImportBlock> = BTreeMap::default();
    // Categorize `StmtKind::Import`.
    for (alias, comments) in block.import {
        let import_type = categorize(
            &alias.module_name(),
            None,
            src,
            package,
            known_modules,
            target_version,
        );
        block_by_type
            .entry(import_type)
            .or_default()
            .import
            .insert(alias, comments);
    }
    // Categorize `StmtKind::ImportFrom` (without re-export).
    for (import_from, aliases) in block.import_from {
        let classification = categorize(
            &import_from.module_name(),
            import_from.level,
            src,
            package,
            known_modules,
            target_version,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from
            .insert(import_from, aliases);
    }
    // Categorize `StmtKind::ImportFrom` (with re-export).
    for ((import_from, alias), aliases) in block.import_from_as {
        let classification = categorize(
            &import_from.module_name(),
            import_from.level,
            src,
            package,
            known_modules,
            target_version,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_as
            .insert((import_from, alias), aliases);
    }
    // Categorize `StmtKind::ImportFrom` (with star).
    for (import_from, comments) in block.import_from_star {
        let classification = categorize(
            &import_from.module_name(),
            import_from.level,
            src,
            package,
            known_modules,
            target_version,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_star
            .insert(import_from, comments);
    }
    block_by_type
}

#[derive(Debug, Default, CacheKey)]
pub struct KnownModules {
    /// A map of known modules to their section.
    known: FxHashMap<String, ImportSection>,
    /// Whether any of the known modules are submodules (e.g., `foo.bar`, as opposed to `foo`).
    has_submodules: bool,
}

impl KnownModules {
    pub fn new(
        first_party: Vec<String>,
        third_party: Vec<String>,
        local_folder: Vec<String>,
        standard_library: Vec<String>,
        user_defined: FxHashMap<String, Vec<String>>,
    ) -> Self {
        let modules = user_defined
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
            );

        let mut known = FxHashMap::with_capacity_and_hasher(
            modules.size_hint().0,
            std::hash::BuildHasherDefault::default(),
        );
        modules.for_each(|(module, section)| {
            if known.insert(module, section).is_some() {
                warn_user_once!("One or more modules are part of multiple import sections.");
            }
        });

        let has_submodules = known.keys().any(|module| module.contains('.'));

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
        if let Some(section) = self.known.get(submodule) {
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
        } else {
            None
        }
    }

    /// Return the list of modules that are known to be of a given type.
    pub fn modules_for_known_type(&self, import_type: ImportType) -> Vec<String> {
        self.known
            .iter()
            .filter_map(|(module, known_section)| {
                if let ImportSection::Known(section) = known_section {
                    if *section == import_type {
                        Some(module.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Return the list of user-defined modules, indexed by section.
    pub fn user_defined(&self) -> FxHashMap<String, Vec<String>> {
        let mut user_defined: FxHashMap<String, Vec<String>> = FxHashMap::default();
        for (module, section) in &self.known {
            if let ImportSection::UserDefined(section_name) = section {
                user_defined
                    .entry(section_name.clone())
                    .or_default()
                    .push(module.clone());
            }
        }
        user_defined
    }
}
