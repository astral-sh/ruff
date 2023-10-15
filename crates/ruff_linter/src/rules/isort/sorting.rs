/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use natord;
use std::cmp::Ordering;

use ruff_python_stdlib::str;

use super::settings::{RelativeImportsOrder, Settings};
use super::types::AliasData;

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
    i64,
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
        })
        .unwrap_or_default();
    let force_to_top = name.map(|name| !settings.force_to_top.contains(name)); // `false` < `true` so we get forced to top first
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
    let not_star_import = name != "*"; // `false` < `true` so we get star imports first
    let member_type = settings
        .order_by_type
        .then_some(member_type(name, settings));
    let maybe_lower_case_name = (!settings.case_sensitive)
        .then_some(name.to_lowercase())
        .map(NatOrdString);
    let module_name = NatOrdString(name.to_string());
    let asname = asname.map(String::from).map(NatOrdString);

    (
        not_star_import,
        member_type,
        maybe_lower_case_name,
        module_name,
        asname,
    )
}
