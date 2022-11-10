use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::macos::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use once_cell::sync::Lazy;

use crate::python::sys::KNOWN_STANDARD_LIBRARY;

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
pub enum ImportType {
    Future,
    StandardLibrary,
    ThirdParty,
    FirstParty,
}

pub fn categorize(
    module_base: &str,
    src_paths: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> Result<ImportType> {
    if known_first_party.contains(module_base) {
        Ok(ImportType::FirstParty)
    } else if known_third_party.contains(module_base) {
        Ok(ImportType::ThirdParty)
    } else if extra_standard_library.contains(module_base) {
        Ok(ImportType::StandardLibrary)
    } else if let Some(import_type) = STATIC_CLASSIFICATIONS.get(module_base) {
        Ok(import_type.clone())
    } else if KNOWN_STANDARD_LIBRARY.contains(module_base) {
        Ok(ImportType::StandardLibrary)
    } else {
        // STOPSHIP(charlie): Do this once.
        let app_dirs = get_app(
            src_paths
                .iter()
                .map(|src_path| Path::new(src_path).to_path_buf()),
        )?;
        println!("app_dirs = {:?}", app_dirs);
        if find_local(&app_dirs, module_base) {
            Ok(ImportType::FirstParty)
        } else {
            Ok(ImportType::ThirdParty)
        }
    }
}

static STATIC_CLASSIFICATIONS: Lazy<BTreeMap<&'static str, ImportType>> = Lazy::new(|| {
    BTreeMap::from([
        ("__future__", ImportType::Future),
        ("__main__", ImportType::FirstParty),
        // Force `disutils` to be considered third-party.
        ("disutils", ImportType::ThirdParty),
        // Relative imports (e.g., `from . import module`).
        ("", ImportType::FirstParty),
    ])
});

fn path_key(path: &PathBuf) -> Result<(u64, u64)> {
    let metadata = fs::metadata(path)?;
    Ok((metadata.st_ino(), metadata.st_dev()))
}

fn get_app(app_dirs: impl Iterator<Item = PathBuf>) -> Result<Vec<PathBuf>> {
    let mut paths = vec![];
    let mut seen: BTreeSet<(u64, u64)> = Default::default();
    for app_dir in app_dirs {
        if seen.insert(path_key(&app_dir)?) {
            paths.push(app_dir);
        }
    }
    Ok(paths)
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
