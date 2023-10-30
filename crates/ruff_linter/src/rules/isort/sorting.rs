/// See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>
use natord;
use std::{borrow::Cow, cmp::Ordering};

use ruff_python_stdlib::str;

use super::settings::{RelativeImportsOrder, Settings};

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
pub(crate) struct NatOrdStr<'a>(Cow<'a, str>);

impl Ord for NatOrdStr<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        natord::compare(&self.0, &other.0)
    }
}

impl PartialOrd for NatOrdStr<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> From<&'a str> for NatOrdStr<'a> {
    fn from(s: &'a str) -> Self {
        NatOrdStr(Cow::Borrowed(s))
    }
}

impl<'a> From<String> for NatOrdStr<'a> {
    fn from(s: String) -> Self {
        NatOrdStr(Cow::Owned(s))
    }
}

type ModuleKey<'a> = (
    i64,
    Option<bool>,
    Option<NatOrdStr<'a>>,
    Option<NatOrdStr<'a>>,
    Option<NatOrdStr<'a>>,
    Option<MemberKey<'a>>,
);

/// Returns a comparable key to capture the desired sorting order for an imported module (e.g.,
/// `foo` in `from foo import bar`).
pub(crate) fn module_key<'a>(
    name: Option<&'a str>,
    asname: Option<&'a str>,
    level: Option<u32>,
    first_alias: Option<(&'a str, Option<&'a str>)>,
    settings: &Settings,
) -> ModuleKey<'a> {
    let level = level
        .map(i64::from)
        .map(|l| match settings.relative_imports_order {
            RelativeImportsOrder::ClosestToFurthest => l,
            RelativeImportsOrder::FurthestToClosest => -l,
        })
        .unwrap_or_default();
    let force_to_top = name.map(|name| !settings.force_to_top.contains(name)); // `false` < `true` so we get forced to top first
    let maybe_lowercase_name = name
        .and_then(|name| (!settings.case_sensitive).then_some(NatOrdStr(maybe_lowercase(name))));
    let module_name = name.map(NatOrdStr::from);
    let asname = asname.map(NatOrdStr::from);
    let first_alias = first_alias.map(|(name, asname)| member_key(name, asname, settings));

    (
        level,
        force_to_top,
        maybe_lowercase_name,
        module_name,
        asname,
        first_alias,
    )
}

type MemberKey<'a> = (
    bool,
    Option<MemberType>,
    Option<NatOrdStr<'a>>,
    NatOrdStr<'a>,
    Option<NatOrdStr<'a>>,
);

/// Returns a comparable key to capture the desired sorting order for an imported member (e.g.,
/// `bar` in `from foo import bar`).
pub(crate) fn member_key<'a>(
    name: &'a str,
    asname: Option<&'a str>,
    settings: &Settings,
) -> MemberKey<'a> {
    let not_star_import = name != "*"; // `false` < `true` so we get star imports first
    let member_type = settings
        .order_by_type
        .then_some(member_type(name, settings));
    let maybe_lowercase_name =
        (!settings.case_sensitive).then_some(NatOrdStr(maybe_lowercase(name)));
    let module_name = NatOrdStr::from(name);
    let asname = asname.map(NatOrdStr::from);

    (
        not_star_import,
        member_type,
        maybe_lowercase_name,
        module_name,
        asname,
    )
}

/// Lowercase the given string, if it contains any uppercase characters.
fn maybe_lowercase(name: &str) -> Cow<'_, str> {
    if name.chars().all(char::is_lowercase) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.to_lowercase())
    }
}
