//! See: <https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13>

use std::{borrow::Cow, cmp::Ordering, cmp::Reverse};

use natord;
use unicode_width::UnicodeWidthChar;

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
pub(crate) struct NatOrdStr<'a> {
    inner: Cow<'a, str>,
    lexicographical: bool,
}

impl Ord for NatOrdStr<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.lexicographical || other.lexicographical {
            self.inner.cmp(&other.inner)
        } else {
            natord::compare(&self.inner, &other.inner)
        }
    }
}

impl PartialOrd for NatOrdStr<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> NatOrdStr<'a> {
    fn new(inner: Cow<'a, str>, lexicographical: bool) -> Self {
        Self {
            inner,
            lexicographical,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) enum Distance {
    Nearest(u32),
    Furthest(Reverse<u32>),
    None,
}

#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) enum ImportStyle {
    // Ex) `import foo`
    Straight,
    // Ex) `from foo import bar`
    From,
}

/// A comparable key to capture the desired sorting order for an imported module (e.g.,
/// `foo` in `from foo import bar`).
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct ModuleKey<'a> {
    force_to_top: bool,
    maybe_length: Option<usize>,
    distance: Distance,
    maybe_lowercase_name: Option<NatOrdStr<'a>>,
    module_name: Option<NatOrdStr<'a>>,
    first_alias: Option<MemberKey<'a>>,
    asname: Option<NatOrdStr<'a>>,
}

impl<'a> ModuleKey<'a> {
    pub(crate) fn from_module(
        name: Option<&'a str>,
        asname: Option<&'a str>,
        level: u32,
        first_alias: Option<(&'a str, Option<&'a str>)>,
        style: ImportStyle,
        settings: &Settings,
    ) -> Self {
        let force_to_top = !name.is_some_and(|name| settings.force_to_top.contains(name)); // `false` < `true` so we get forced to top first

        let maybe_length = (settings.length_sort
            || (settings.length_sort_straight && style == ImportStyle::Straight))
            .then_some(
                name.map(|name| name.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>())
                    .unwrap_or_default()
                    + level as usize,
            );

        let distance = match level {
            0 => Distance::None,
            _ => match settings.relative_imports_order {
                RelativeImportsOrder::ClosestToFurthest => Distance::Nearest(level),
                RelativeImportsOrder::FurthestToClosest => Distance::Furthest(Reverse(level)),
            },
        };

        let maybe_lowercase_name = name.and_then(|name| {
            (!settings.case_sensitive).then_some(NatOrdStr::new(
                maybe_lowercase(name),
                settings.lexicographical,
            ))
        });

        let module_name =
            name.map(|name| NatOrdStr::new(Cow::Borrowed(name), settings.lexicographical));

        let asname =
            asname.map(|name| NatOrdStr::new(Cow::Borrowed(name), settings.lexicographical));

        let first_alias =
            first_alias.map(|(name, asname)| MemberKey::from_member(name, asname, settings));

        Self {
            force_to_top,
            maybe_length,
            distance,
            maybe_lowercase_name,
            module_name,
            first_alias,
            asname,
        }
    }
}

/// A comparable key to capture the desired sorting order for an imported member (e.g., `bar` in
/// `from foo import bar`).
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct MemberKey<'a> {
    not_star_import: bool,
    member_type: Option<MemberType>,
    maybe_length: Option<usize>,
    maybe_lowercase_name: Option<NatOrdStr<'a>>,
    module_name: NatOrdStr<'a>,
    asname: Option<NatOrdStr<'a>>,
}

impl<'a> MemberKey<'a> {
    pub(crate) fn from_member(name: &'a str, asname: Option<&'a str>, settings: &Settings) -> Self {
        let not_star_import = name != "*"; // `false` < `true` so we get star imports first
        let member_type = settings
            .order_by_type
            .then_some(member_type(name, settings));
        let maybe_length = settings
            .length_sort
            .then(|| name.chars().map(|c| c.width().unwrap_or(0)).sum());
        let maybe_lowercase_name = (!settings.case_sensitive).then_some(NatOrdStr::new(
            maybe_lowercase(name),
            settings.lexicographical,
        ));
        let module_name = NatOrdStr::new(Cow::Borrowed(name), settings.lexicographical);
        let asname =
            asname.map(|name| NatOrdStr::new(Cow::Borrowed(name), settings.lexicographical));

        Self {
            not_star_import,
            member_type,
            maybe_length,
            maybe_lowercase_name,
            module_name,
            asname,
        }
    }
}

/// Lowercase the given string, if it contains any uppercase characters.
fn maybe_lowercase(name: &str) -> Cow<'_, str> {
    if name.chars().all(char::is_lowercase) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.to_lowercase())
    }
}
