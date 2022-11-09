use std::collections::BTreeMap;

use once_cell::sync::Lazy;

use crate::python::sys::STDLIB_MODULE_NAMES;

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

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone)]
pub enum ImportType {
    Future,
    StandardLibrary,
    ThirdParty,
    FirstParty,
}

pub fn categorize(module_base: &str) -> ImportType {
    if let Some(import_type) = STATIC_CLASSIFICATIONS.get(module_base) {
        import_type.clone()
    } else if STDLIB_MODULE_NAMES.contains(module_base) {
        ImportType::StandardLibrary
    } else {
        // STOPSHIP(charlie): Implement first-party classification.
        ImportType::ThirdParty
    }
}
