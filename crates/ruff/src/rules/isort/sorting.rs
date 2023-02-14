/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use std::cmp::Ordering;
use std::collections::BTreeSet;

use ruff_python::string;

use super::settings::RelativeImportsOrder;
use super::types::EitherImport::{Import, ImportFrom};
use super::types::{AliasData, EitherImport, ImportFromData};

#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub enum Prefix {
    Constants,
    Classes,
    Variables,
}

fn prefix(
    name: &str,
    classes: &BTreeSet<String>,
    constants: &BTreeSet<String>,
    variables: &BTreeSet<String>,
) -> Prefix {
    if constants.contains(name) {
        // Ex) `CONSTANT`
        Prefix::Constants
    } else if classes.contains(name) {
        // Ex) `CLASS`
        Prefix::Classes
    } else if variables.contains(name) {
        // Ex) `variable`
        Prefix::Variables
    } else if name.len() > 1 && string::is_upper(name) {
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
pub fn cmp_members(
    alias1: &AliasData,
    alias2: &AliasData,
    order_by_type: bool,
    classes: &BTreeSet<String>,
    constants: &BTreeSet<String>,
    variables: &BTreeSet<String>,
) -> Ordering {
    match (alias1.name == "*", alias2.name == "*") {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => {
            if order_by_type {
                prefix(alias1.name, classes, constants, variables)
                    .cmp(&prefix(alias2.name, classes, constants, variables))
                    .then_with(|| cmp_modules(alias1, alias2))
            } else {
                cmp_modules(alias1, alias2)
            }
        }
    }
}

/// Compare two relative import levels.
pub fn cmp_levels(
    level1: Option<&usize>,
    level2: Option<&usize>,
    relative_imports_order: RelativeImportsOrder,
) -> Ordering {
    match (level1, level2) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(level1), Some(level2)) => match relative_imports_order {
            RelativeImportsOrder::ClosestToFurthest => level1.cmp(level2),
            RelativeImportsOrder::FurthestToClosest => level2.cmp(level1),
        },
    }
}

/// Compare two `StmtKind::ImportFrom` blocks.
pub fn cmp_import_from(
    import_from1: &ImportFromData,
    import_from2: &ImportFromData,
    relative_imports_order: RelativeImportsOrder,
) -> Ordering {
    cmp_levels(
        import_from1.level,
        import_from2.level,
        relative_imports_order,
    )
    .then_with(|| match (&import_from1.module, import_from2.module) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(module1), Some(module2)) => natord::compare_ignore_case(module1, module2)
            .then_with(|| natord::compare(module1, module2)),
    })
}

/// Compare two [`EitherImport`] enums which may be [`Import`] or [`ImportFrom`]
/// structs.
pub fn cmp_either_import(
    a: &EitherImport,
    b: &EitherImport,
    relative_imports_order: RelativeImportsOrder,
) -> Ordering {
    match (a, b) {
        (Import((alias1, _)), Import((alias2, _))) => cmp_modules(alias1, alias2),
        (ImportFrom((import_from, ..)), Import((alias, _))) => {
            natord::compare_ignore_case(import_from.module.unwrap_or_default(), alias.name)
        }
        (Import((alias, _)), ImportFrom((import_from, ..))) => {
            natord::compare_ignore_case(alias.name, import_from.module.unwrap_or_default())
        }
        (ImportFrom((import_from1, ..)), ImportFrom((import_from2, ..))) => {
            cmp_import_from(import_from1, import_from2, relative_imports_order)
        }
    }
}
