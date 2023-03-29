use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::{fs, iter};

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
    Copy,
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
    target_version: PythonVersion,
) -> ImportType {
    let module_base = module_name.split('.').next().unwrap();
    let (import_type, reason) = {
        if level.map_or(false, |level| *level > 0) {
            (ImportType::LocalFolder, Reason::NonZeroLevel)
        } else if module_base == "__future__" {
            (ImportType::Future, Reason::Future)
        } else if let Some((import_type, reason)) = known_modules.categorize(module_name) {
            (import_type, reason)
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
    /// A set of user-provided first-party modules.
    pub first_party: BTreeSet<String>,
    /// A set of user-provided third-party modules.
    pub third_party: BTreeSet<String>,
    /// A set of user-provided local folder modules.
    pub local_folder: BTreeSet<String>,
    /// A set of user-provided standard library modules.
    pub standard_library: BTreeSet<String>,
    /// Whether any of the known modules are submodules (e.g., `foo.bar`, as opposed to `foo`).
    has_submodules: bool,
}

impl KnownModules {
    pub fn new(
        first_party: Vec<String>,
        third_party: Vec<String>,
        local_folder: Vec<String>,
        standard_library: Vec<String>,
    ) -> Self {
        let first_party = BTreeSet::from_iter(first_party);
        let third_party = BTreeSet::from_iter(third_party);
        let local_folder = BTreeSet::from_iter(local_folder);
        let standard_library = BTreeSet::from_iter(standard_library);
        let has_submodules = first_party
            .iter()
            .chain(third_party.iter())
            .chain(local_folder.iter())
            .chain(standard_library.iter())
            .any(|m| m.contains('.'));
        Self {
            first_party,
            third_party,
            local_folder,
            standard_library,
            has_submodules,
        }
    }

    /// Return the [`ImportType`] for a given module, if it's been categorized as a known module
    /// by the user.
    fn categorize(&self, module_name: &str) -> Option<(ImportType, Reason)> {
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
                if self.first_party.contains(submodule) {
                    return Some((ImportType::FirstParty, Reason::KnownFirstParty));
                }
                if self.third_party.contains(submodule) {
                    return Some((ImportType::ThirdParty, Reason::KnownThirdParty));
                }
                if self.local_folder.contains(submodule) {
                    return Some((ImportType::LocalFolder, Reason::KnownLocalFolder));
                }
                if self.standard_library.contains(submodule) {
                    return Some((ImportType::StandardLibrary, Reason::ExtraStandardLibrary));
                }
            }
            None
        } else {
            // Happy path: no submodules, so we can check the module base and be done.
            let module_base = module_name.split('.').next().unwrap();
            if self.first_party.contains(module_base) {
                Some((ImportType::FirstParty, Reason::KnownFirstParty))
            } else if self.third_party.contains(module_base) {
                Some((ImportType::ThirdParty, Reason::KnownThirdParty))
            } else if self.local_folder.contains(module_base) {
                Some((ImportType::LocalFolder, Reason::KnownLocalFolder))
            } else if self.standard_library.contains(module_base) {
                Some((ImportType::StandardLibrary, Reason::ExtraStandardLibrary))
            } else {
                None
            }
        }
    }
}
