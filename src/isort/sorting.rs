/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use std::cmp::Ordering;

use crate::isort::types::{AliasData, ImportFromData, OrderedImportBlock};
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
pub fn cmp_members(alias1: &AliasData, alias2: &AliasData, order_by_type: bool) -> Ordering {
    if order_by_type {
        prefix(alias1.name)
            .cmp(&prefix(alias2.name))
            .then_with(|| cmp_modules(alias1, alias2))
    } else {
        cmp_modules(alias1, alias2)
    }
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
pub fn cmp_import_from(import_from1: &ImportFromData, import_from2: &ImportFromData) -> Ordering {
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

pub fn merge_imports(
    import_block: &OrderedImportBlock,
    force_sort_within_sections: bool,
) -> Vec<(i32, usize)> {
    let mut idx_import = 0;
    let len_import = import_block.import.len();

    let mut idx_import_from = 0;
    let len_import_from = import_block.import_from.len();

    let mut merged: Vec<(i32, usize)> = Vec::new();

    while idx_import < len_import {
        if !force_sort_within_sections || idx_import_from >= len_import_from {
            merged.push((0, idx_import));
            idx_import += 1;
            continue;
        }

        let import = &import_block.import[idx_import];
        let (alias, _) = &import;

        let import_from = &import_block.import_from[idx_import_from];
        let (import_from_data, ..) = &import_from;

        let cmp = alias
            .name
            .to_lowercase()
            .cmp(&import_from_data.module.unwrap().to_lowercase());
        match cmp {
            Ordering::Equal | Ordering::Less => {
                merged.push((0, idx_import));
                idx_import += 1;
            }
            Ordering::Greater => {
                merged.push((1, idx_import_from));
                idx_import_from += 1;
            }
        }
    }
    while idx_import_from < len_import_from {
        merged.push((1, idx_import_from));
        idx_import_from += 1;
    }
    merged
}
