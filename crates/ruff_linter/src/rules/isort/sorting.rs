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

impl From<String> for NatOrdStr<'_> {
    fn from(s: String) -> Self {
        NatOrdStr(Cow::Owned(s))
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

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
enum SortKey<T> {
    Forward(T),
    Reverse(Reverse<T>),
}

impl<T> SortKey<T> {
    fn new(value: T, reverse: bool) -> Self {
        if reverse {
            Self::Reverse(Reverse(value))
        } else {
            Self::Forward(value)
        }
    }
}

/// A comparable key to capture the desired sorting order for an imported module (e.g.,
/// `foo` in `from foo import bar`).
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct ModuleKey<'a>(SortKey<ModuleKeyInner<'a>>);

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
struct ModuleKeyInner<'a> {
    force_to_top: bool,
    maybe_length: Option<usize>,
    distance: Distance,
    maybe_lowercase_name: Option<NatOrdStr<'a>>,
    module_name: Option<NatOrdStr<'a>>,
    first_alias: Option<MemberKeyInner<'a>>,
    asname: Option<NatOrdStr<'a>>,
}

impl<'a> ModuleKey<'a> {
    pub(crate) fn from_module(
        name: Option<&'a str>,
        asname: Option<&'a str>,
        level: u32,
        first_alias: Option<(&'a str, Option<&'a str>)>,
        style: ImportStyle,
        statement_width: usize,
        settings: &Settings,
    ) -> Self {
        let force_to_top = !name.is_some_and(|name| settings.force_to_top.contains(name)); // `false` < `true` so we get forced to top first

        let maybe_length = (settings.length_sort
            || (settings.length_sort_straight && style == ImportStyle::Straight))
            .then(|| {
                if settings.reverse_sort {
                    statement_width
                } else {
                    name.map(|name| name.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>())
                        .unwrap_or_default()
                        + level as usize
                }
            });

        let distance = match level {
            0 => Distance::None,
            _ => match settings.relative_imports_order {
                RelativeImportsOrder::ClosestToFurthest => Distance::Nearest(level),
                RelativeImportsOrder::FurthestToClosest => Distance::Furthest(Reverse(level)),
            },
        };

        let maybe_lowercase_name = name.and_then(|name| {
            (!settings.case_sensitive).then_some(NatOrdStr(maybe_lowercase(name)))
        });

        let module_name = name.map(NatOrdStr::from);

        let asname = asname.map(NatOrdStr::from);

        let first_alias =
            first_alias.map(|(name, asname)| MemberKeyInner::from_member(name, asname, settings));

        Self(SortKey::new(
            ModuleKeyInner {
                force_to_top,
                maybe_length,
                distance,
                maybe_lowercase_name,
                module_name,
                first_alias,
                asname,
            },
            settings.reverse_sort,
        ))
    }
}

/// A comparable key to capture the desired sorting order for an imported member (e.g., `bar` in
/// `from foo import bar`).
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct MemberKey<'a>(SortKey<MemberKeyInner<'a>>);

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
struct MemberKeyInner<'a> {
    not_star_import: bool,
    member_type: Option<MemberType>,
    maybe_length: Option<usize>,
    maybe_lowercase_name: Option<NatOrdStr<'a>>,
    module_name: NatOrdStr<'a>,
    asname: Option<NatOrdStr<'a>>,
}

impl<'a> MemberKey<'a> {
    pub(crate) fn from_member(name: &'a str, asname: Option<&'a str>, settings: &Settings) -> Self {
        Self(SortKey::new(
            MemberKeyInner::from_member(name, asname, settings),
            settings.reverse_sort,
        ))
    }
}

impl<'a> MemberKeyInner<'a> {
    fn from_member(name: &'a str, asname: Option<&'a str>, settings: &Settings) -> Self {
        let not_star_import = name != "*"; // `false` < `true` so we get star imports first
        let member_type = settings
            .order_by_type
            .then_some(member_type(name, settings));
        let maybe_length = settings
            .length_sort
            .then(|| name.chars().map(|c| c.width().unwrap_or(0)).sum());
        let maybe_lowercase_name =
            (!settings.case_sensitive).then_some(NatOrdStr(maybe_lowercase(name)));
        let module_name = NatOrdStr::from(name);
        let asname = asname.map(NatOrdStr::from);

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

#[cfg(test)]
mod tests {
    use super::{ImportStyle, MemberKey, ModuleKey};
    use crate::rules::isort::settings::Settings;

    fn module_key<'a>(name: &'a str, settings: &Settings) -> ModuleKey<'a> {
        ModuleKey::from_module(
            Some(name),
            None,
            0,
            None,
            ImportStyle::Straight,
            0,
            settings,
        )
    }

    fn member_key<'a>(name: &'a str, settings: &Settings) -> MemberKey<'a> {
        MemberKey::from_member(name, None, settings)
    }

    #[test]
    fn reverses_module_and_member_order() {
        let settings = Settings::default();
        assert!(module_key("alpha", &settings) < module_key("beta", &settings));
        assert!(member_key("alpha", &settings) < member_key("beta", &settings));

        let settings = Settings {
            reverse_sort: true,
            ..Settings::default()
        };
        assert!(module_key("beta", &settings) < module_key("alpha", &settings));
        assert!(member_key("beta", &settings) < member_key("alpha", &settings));
        assert!(
            ModuleKey::from_module(
                Some("module"),
                None,
                0,
                Some(("beta", None)),
                ImportStyle::From,
                0,
                &settings,
            ) < ModuleKey::from_module(
                Some("module"),
                None,
                0,
                Some(("alpha", None)),
                ImportStyle::From,
                0,
                &settings,
            )
        );
    }

    #[test]
    fn sorts_longest_statement_first_when_length_sort_is_reversed() {
        let settings = Settings {
            length_sort: true,
            reverse_sort: true,
            ..Settings::default()
        };

        assert!(
            ModuleKey::from_module(
                Some("short"),
                None,
                0,
                None,
                ImportStyle::From,
                30,
                &settings,
            ) < ModuleKey::from_module(
                Some("long_module"),
                None,
                0,
                None,
                ImportStyle::From,
                20,
                &settings,
            )
        );
        assert!(member_key("long_member", &settings) < member_key("short", &settings));
    }
}
