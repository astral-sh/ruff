use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::python::sys::KNOWN_STANDARD_LIBRARY;

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
pub enum ImportType {
    Future,
    StandardLibrary,
    ThirdParty,
    FirstParty,
    LocalFolder,
}

pub fn categorize(
    module_base: &str,
    level: Option<&usize>,
    src: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> ImportType {
    if level.map(|level| *level > 0).unwrap_or(false) {
        ImportType::LocalFolder
    } else if known_first_party.contains(module_base) {
        ImportType::FirstParty
    } else if known_third_party.contains(module_base) {
        ImportType::ThirdParty
    } else if extra_standard_library.contains(module_base) {
        ImportType::StandardLibrary
    } else if module_base == "__future__" {
        ImportType::Future
    } else if KNOWN_STANDARD_LIBRARY.contains(module_base) {
        ImportType::StandardLibrary
    } else if find_local(src, module_base) {
        ImportType::FirstParty
    } else {
        ImportType::ThirdParty
    }
}

fn find_local(paths: &[PathBuf], base: &str) -> bool {
    for path in paths {
        if let Ok(metadata) = fs::metadata(path.join(base)) {
            if metadata.is_dir() {
                return true;
            }
        }
        if let Ok(metadata) = fs::metadata(path.join(format!("{base}.py"))) {
            if metadata.is_file() {
                return true;
            }
        }
    }
    false
}
