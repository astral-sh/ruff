use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use log::debug;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use ruff_macros::CacheKey;
use ruff_python_stdlib::sys::KNOWN_STANDARD_LIBRARY;

use crate::settings::types::PythonVersion;

use super::types::{ImportBlock, Importable};

#[derive(
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    Clone,
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
}

#[allow(clippy::too_many_arguments)]
pub fn categorize(
    module_name: &str,
    level: Option<&usize>,
    src: &[PathBuf],
    package: Option<&Path>,
    known_modules: &KnownModules,
    known_local_folder: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
    target_version: PythonVersion,
) -> ImportType {
    let module_base = module_name.split('.').next().unwrap();
    let (import_type, reason) = {
        if level.map_or(false, |level| *level > 0) {
            (ImportType::LocalFolder, Reason::NonZeroLevel)
        } else if let Some(type_and_reason) = known_modules.get_category(module_name) {
            type_and_reason
        } else if known_local_folder.contains(module_base) {
            (ImportType::LocalFolder, Reason::KnownLocalFolder)
        } else if extra_standard_library.contains(module_base) {
            (ImportType::StandardLibrary, Reason::ExtraStandardLibrary)
        } else if module_base == "__future__" {
            (ImportType::Future, Reason::Future)
        } else if KNOWN_STANDARD_LIBRARY
            .get(&target_version.as_tuple())
            .unwrap()
            .contains(module_base)
        {
            (ImportType::StandardLibrary, Reason::KnownStandardLibrary)
        } else if same_package(package, module_base) {
            (ImportType::FirstParty, Reason::SamePackage)
        } else if let Some(src) = match_sources(src, module_base) {
            (ImportType::FirstParty, Reason::SourceMatch(src))
        } else {
            (ImportType::ThirdParty, Reason::NoMatch)
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
    known_modules: &KnownModules,
    known_local_folder: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
    target_version: PythonVersion,
) -> BTreeMap<ImportType, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<ImportType, ImportBlock> = BTreeMap::default();
    // Categorize `StmtKind::Import`.
    for (alias, comments) in block.import {
        let import_type = categorize(
            &alias.module_name(),
            None,
            src,
            package,
            known_modules,
            known_local_folder,
            extra_standard_library,
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
            known_local_folder,
            extra_standard_library,
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
            known_local_folder,
            extra_standard_library,
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
            known_local_folder,
            extra_standard_library,
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
    pub first_party: BTreeSet<String>,
    pub third_party: BTreeSet<String>,
    has_submodules: bool,
}

impl KnownModules {
    pub fn new<T: IntoIterator<Item = String>>(first_party: T, third_party: T) -> Self {
        let first_party = BTreeSet::from_iter(first_party);
        let third_party = BTreeSet::from_iter(third_party);
        let fp_submodules = first_party.iter().filter(|m| m.contains('.')).count() != 0;
        let tp_submodules = third_party.iter().filter(|m| m.contains('.')).count() != 0;
        Self {
            first_party,
            third_party,
            has_submodules: fp_submodules || tp_submodules,
        }
    }

    fn get_category(&self, module_name: &str) -> Option<(ImportType, Reason)> {
        // Shortcut for everyone that does not use submodules in KnownModules
        if !self.has_submodules {
            let module_base = module_name.split('.').next().unwrap();
            if self.first_party.contains(module_base) {
                return Some((ImportType::FirstParty, Reason::KnownFirstParty));
            }
            if self.third_party.contains(module_base) {
                return Some((ImportType::ThirdParty, Reason::KnownThirdParty));
            }
        }

        // Check all module prefixes from the longest to the shortest. The first one
        // matching a value in either first_party or third_party modules defines
        // the category.
        let parts: Vec<usize> = module_name
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '.')
            .map(|(i, _)| i)
            .chain([module_name.len()])
            .collect();

        for i in parts.iter().rev() {
            let submodule = &module_name[0..*i];
            if self.first_party.contains(submodule) {
                return Some((ImportType::FirstParty, Reason::KnownFirstParty));
            }
            if self.third_party.contains(submodule) {
                return Some((ImportType::ThirdParty, Reason::KnownThirdParty));
            }
        }

        None
    }
}
