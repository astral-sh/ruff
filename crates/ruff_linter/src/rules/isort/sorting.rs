/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use std::cmp::Ordering;
use std::collections::BTreeSet;

use ruff_python_stdlib::str;

use super::settings::{RelativeImportsOrder, Settings};
use super::types::EitherImport::{Import, ImportFrom};
use super::types::{AliasData, EitherImport, ImportFromData, Importable};

#[derive(PartialOrd, Ord, PartialEq, Eq, Copy, Clone)]
pub(crate) enum Prefix {
    Constants,
    Classes,
    Variables,
}

fn prefix(name: &str, settings: &Settings) -> Prefix {
    if settings.constants.contains(name) {
        // Ex) `CONSTANT`
        Prefix::Constants
    } else if settings.classes.contains(name) {
        // Ex) `CLASS`
        Prefix::Classes
    } else if settings.variables.contains(name) {
        // Ex) `variable`
        Prefix::Variables
    } else if name.len() > 1 && str::is_cased_uppercase(name) {
        // Ex) `CONSTANT`
        Prefix::Constants
    } else if name.chars().next().is_some_and(char::is_uppercase) {
        // Ex) `Class`
        Prefix::Classes
    } else {
        // Ex) `variable`
        Prefix::Variables
    }
}

/// Compare two module names' by their `force-to-top`ness.
fn cmp_force_to_top(name1: &str, name2: &str, force_to_top: &BTreeSet<String>) -> Ordering {
    let force_to_top1 = force_to_top.contains(name1);
    let force_to_top2 = force_to_top.contains(name2);
    force_to_top1.cmp(&force_to_top2).reverse()
}

/// Compare two top-level modules.
pub(crate) fn cmp_modules(alias1: &AliasData, alias2: &AliasData, settings: &Settings) -> Ordering {
    cmp_force_to_top(alias1.name, alias2.name, &settings.force_to_top)
        .then_with(|| {
            if settings.case_sensitive {
                natord::compare(alias1.name, alias2.name)
            } else {
                natord::compare_ignore_case(alias1.name, alias2.name)
                    .then_with(|| natord::compare(alias1.name, alias2.name))
            }
        })
        .then_with(|| match (alias1.asname, alias2.asname) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(asname1), Some(asname2)) => natord::compare(asname1, asname2),
        })
}

/// Compare two member imports within `Stmt::ImportFrom` blocks.
pub(crate) fn cmp_members(alias1: &AliasData, alias2: &AliasData, settings: &Settings) -> Ordering {
    match (alias1.name == "*", alias2.name == "*") {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => {
            if settings.order_by_type {
                prefix(alias1.name, settings)
                    .cmp(&prefix(alias2.name, settings))
                    .then_with(|| cmp_modules(alias1, alias2, settings))
            } else {
                cmp_modules(alias1, alias2, settings)
            }
        }
    }
}

/// Compare two relative import levels.
pub(crate) fn cmp_levels(
    level1: Option<u32>,
    level2: Option<u32>,
    relative_imports_order: RelativeImportsOrder,
) -> Ordering {
    match (level1, level2) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(level1), Some(level2)) => match relative_imports_order {
            RelativeImportsOrder::ClosestToFurthest => level1.cmp(&level2),
            RelativeImportsOrder::FurthestToClosest => level2.cmp(&level1),
        },
    }
}

/// Compare two `Stmt::ImportFrom` blocks.
pub(crate) fn cmp_import_from(
    import_from1: &ImportFromData,
    import_from2: &ImportFromData,
    settings: &Settings,
) -> Ordering {
    cmp_levels(
        import_from1.level,
        import_from2.level,
        settings.relative_imports_order,
    )
    .then_with(|| {
        cmp_force_to_top(
            &import_from1.module_name(),
            &import_from2.module_name(),
            &settings.force_to_top,
        )
    })
    .then_with(|| match (&import_from1.module, import_from2.module) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(module1), Some(module2)) => {
            if settings.case_sensitive {
                natord::compare(module1, module2)
            } else {
                natord::compare_ignore_case(module1, module2)
            }
        }
    })
}

/// Compare an import to an import-from.
fn cmp_import_import_from(
    import: &AliasData,
    import_from: &ImportFromData,
    settings: &Settings,
) -> Ordering {
    cmp_force_to_top(
        import.name,
        &import_from.module_name(),
        &settings.force_to_top,
    )
    .then_with(|| {
        if settings.case_sensitive {
            natord::compare(import.name, import_from.module.unwrap_or_default())
        } else {
            natord::compare_ignore_case(import.name, import_from.module.unwrap_or_default())
        }
    })
}

/// Compare two [`EitherImport`] enums which may be [`Import`] or [`ImportFrom`]
/// structs.
pub(crate) fn cmp_either_import(
    a: &EitherImport,
    b: &EitherImport,
    settings: &Settings,
) -> Ordering {
    match (a, b) {
        (Import((alias1, _)), Import((alias2, _))) => cmp_modules(alias1, alias2, settings),
        (ImportFrom((import_from, ..)), Import((alias, _))) => {
            cmp_import_import_from(alias, import_from, settings).reverse()
        }
        (Import((alias, _)), ImportFrom((import_from, ..))) => {
            cmp_import_import_from(alias, import_from, settings)
        }
        (ImportFrom((import_from1, ..)), ImportFrom((import_from2, ..))) => {
            cmp_import_from(import_from1, import_from2, settings)
        }
    }
}
