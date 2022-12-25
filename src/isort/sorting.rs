/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use std::cmp::Ordering;

use crate::isort::types::{AliasData, ImportFromData};
use crate::python::string;

#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub enum Prefix {
    Constants,
    Classes,
    Variables,
}

fn prefix(name: &str) -> Prefix {
    if name.len() > 1 && string::is_upper(name) {
        // Ex) `CONSTANT`
        Prefix::Constants
    } else if name.chars().next().map_or(false, char::is_uppercase) {
        // Ex) `Class`
        Prefix::Classes
    } else {
        // Ex) `variable`
        Prefix::Variables
    }
}

/// Compare two top-level modules.
pub fn cmp_modules(alias1: &AliasData, alias2: &AliasData) -> Ordering {
    natord::compare_ignore_case(alias1.name, alias2.name)
        .then_with(|| natord::compare(alias1.name, alias2.name))
        .then_with(|| match (alias1.asname, alias2.asname) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(asname1), Some(asname2)) => natord::compare(asname1, asname2),
        })
}

/// Compare two member imports within `StmtKind::ImportFrom` blocks.
pub fn cmp_members(alias1: &AliasData, alias2: &AliasData) -> Ordering {
    prefix(alias1.name)
        .cmp(&prefix(alias2.name))
        .then_with(|| cmp_modules(alias1, alias2))
}

/// Compare two relative import levels.
pub fn cmp_levels(level1: Option<&usize>, level2: Option<&usize>) -> Ordering {
    match (level1, level2) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(level1), Some(level2)) => level2.cmp(level1),
    }
}

/// Compare two `StmtKind::ImportFrom` blocks.
pub fn cmp_import_froms(import_from1: &ImportFromData, import_from2: &ImportFromData) -> Ordering {
    cmp_levels(import_from1.level, import_from2.level).then_with(|| {
        match (&import_from1.module, import_from2.module) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(module1), Some(module2)) => natord::compare_ignore_case(module1, module2)
                .then_with(|| natord::compare(module1, module2)),
        }
    })
}
