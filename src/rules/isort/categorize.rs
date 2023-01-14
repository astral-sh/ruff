use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use log::debug;

use crate::python::sys::KNOWN_STANDARD_LIBRARY;

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
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

pub fn categorize(
    module_base: &str,
    level: Option<&usize>,
    src: &[PathBuf],
    package: Option<&Path>,
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
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
        } else if KNOWN_STANDARD_LIBRARY.contains(module_base) {
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
