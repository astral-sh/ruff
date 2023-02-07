use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use log::debug;
use ruff_python::sys::KNOWN_STANDARD_LIBRARY;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{ImportBlock, Importable};
use crate::settings::types::PythonVersion;

#[derive(
    Debug, PartialOrd, Ord, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema, Hash,
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
    ExtraStandardLibrary,
    Future,
    KnownStandardLibrary,
    SamePackage,
    SourceMatch(&'a Path),
    NoMatch,
}

#[allow(clippy::too_many_arguments)]
pub fn categorize(
    module_base: &str,
    level: Option<&usize>,
    src: &[PathBuf],
    package: Option<&Path>,
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
    target_version: PythonVersion,
) -> ImportType {
    let (import_type, reason) = {
        if level.map_or(false, |level| *level > 0) {
            (ImportType::LocalFolder, Reason::NonZeroLevel)
        } else if known_first_party.contains(module_base) {
            (ImportType::FirstParty, Reason::KnownFirstParty)
        } else if known_third_party.contains(module_base) {
            (ImportType::ThirdParty, Reason::KnownThirdParty)
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
        module_base, import_type, reason
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

pub fn categorize_imports<'a>(
    block: ImportBlock<'a>,
    src: &[PathBuf],
    package: Option<&Path>,
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
    target_version: PythonVersion,
) -> BTreeMap<ImportType, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<ImportType, ImportBlock> = BTreeMap::default();
    // Categorize `StmtKind::Import`.
    for (alias, comments) in block.import {
        let import_type = categorize(
            &alias.module_base(),
            None,
            src,
            package,
            known_first_party,
            known_third_party,
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
            &import_from.module_base(),
            import_from.level,
            src,
            package,
            known_first_party,
            known_third_party,
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
    for ((import_from, alias), comments) in block.import_from_as {
        let classification = categorize(
            &import_from.module_base(),
            import_from.level,
            src,
            package,
            known_first_party,
            known_third_party,
            extra_standard_library,
            target_version,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_as
            .insert((import_from, alias), comments);
    }
    // Categorize `StmtKind::ImportFrom` (with star).
    for (import_from, comments) in block.import_from_star {
        let classification = categorize(
            &import_from.module_base(),
            import_from.level,
            src,
            package,
            known_first_party,
            known_third_party,
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
