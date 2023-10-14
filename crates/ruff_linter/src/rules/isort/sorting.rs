/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use natord;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use ruff_python_stdlib::str;

use super::settings::{RelativeImportsOrder, Settings};
use super::types::EitherImport::{Import, ImportFrom};
use super::types::{AliasData, EitherImport, ImportFromData, Importable};

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Copy, Clone)]
pub(crate) enum MemberType {
    Constant,
    Class,
    Variable,
}

fn member_type(name: &str, settings: &Settings) -> MemberType {
    if settings.constants.contains(name) {
        // Ex) `CONSTANT`
        MemberType::Constant
    } else if settings.classes.contains(name) {
        // Ex) `CLASS`
        MemberType::Class
    } else if settings.variables.contains(name) {
        // Ex) `variable`
        MemberType::Variable
    } else if name.len() > 1 && str::is_cased_uppercase(name) {
        // Ex) `CONSTANT`
        MemberType::Constant
    } else if name.chars().next().is_some_and(char::is_uppercase) {
        // Ex) `Class`
        MemberType::Class
    } else {
        // Ex) `variable`
        MemberType::Variable
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

#[derive(Eq, PartialEq, Debug)]
pub(crate) struct NatOrdString(String);

impl std::fmt::Display for NatOrdString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Ord for NatOrdString {
    fn cmp(&self, other: &Self) -> Ordering {
        natord::compare(&self.0, &other.0)
    }
}

impl PartialOrd for NatOrdString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

type ModuleKey = (
    Option<i64>,
    Option<bool>,
    Option<NatOrdString>,
    Option<NatOrdString>,
    Option<NatOrdString>,
    Option<MemberKey>,
);

pub(crate) fn module_key(
    name: Option<&str>,
    asname: Option<&str>,
    level: Option<u32>,
    first_alias: Option<&AliasData>,
    settings: &Settings,
) -> ModuleKey {
    let level = level
        .map(i64::from)
        .map(|l| match settings.relative_imports_order {
            RelativeImportsOrder::ClosestToFurthest => l,
            RelativeImportsOrder::FurthestToClosest => -l,
        });
    let force_to_top = name.map(|name| !settings.force_to_top.contains(name));
    let maybe_lower_case_name = name.and_then(|name| {
        (!settings.case_sensitive)
            .then_some(name.to_lowercase())
            .map(NatOrdString)
    });
    let module_name = name.map(String::from).map(NatOrdString);
    let asname = asname.map(String::from).map(NatOrdString);
    let first_alias = first_alias.map(|alias| member_key(alias.name, alias.asname, settings));

    (
        level,
        force_to_top,
        maybe_lower_case_name,
        module_name,
        asname,
        first_alias,
    )
}

type MemberKey = (
    bool,
    Option<MemberType>,
    Option<NatOrdString>,
    NatOrdString,
    Option<NatOrdString>,
);

pub(crate) fn member_key(name: &str, asname: Option<&str>, settings: &Settings) -> MemberKey {
    let is_star = name != "*";
    let member_type = settings
        .order_by_type
        .then_some(member_type(name, settings));
    let maybe_lower_case_name = (!settings.case_sensitive)
        .then_some(name.to_lowercase())
        .map(NatOrdString);
    let module_name = NatOrdString(name.to_string());
    let asname = asname.map(String::from).map(NatOrdString);

    (
        is_star,
        member_type,
        maybe_lower_case_name,
        module_name,
        asname,
    )
}
