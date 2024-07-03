use std::fmt;
use std::ops::Deref;

use compact_str::ToCompactString;

use ruff_python_stdlib::identifiers::is_identifier;

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ModuleName(compact_str::CompactString);

impl ModuleName {
    /// Creates a new module name for `name`. Returns `Some` if `name` is a valid, absolute
    /// module name and `None` otherwise.
    ///
    /// The module name is invalid if:
    ///
    /// * The name is empty
    /// * The name is relative
    /// * The name ends with a `.`
    /// * The name contains a sequence of multiple dots
    /// * A component of a name (the part between two dots) isn't a valid python identifier.
    #[inline]
    #[must_use]
    pub fn new(name: &str) -> Option<Self> {
        Self::is_valid_name(name).then(|| Self(compact_str::CompactString::from(name)))
    }

    /// Creates a new module name for `name` where `name` is a static string.
    /// Returns `Some` if `name` is a valid, absolute module name and `None` otherwise.
    ///
    /// The module name is invalid if:
    ///
    /// * The name is empty
    /// * The name is relative
    /// * The name ends with a `.`
    /// * The name contains a sequence of multiple dots
    /// * A component of a name (the part between two dots) isn't a valid python identifier.
    ///
    /// ## Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar").as_deref(), Some("foo.bar"));
    /// assert_eq!(ModuleName::new_static(""), None);
    /// assert_eq!(ModuleName::new_static("..foo"), None);
    /// assert_eq!(ModuleName::new_static(".foo"), None);
    /// assert_eq!(ModuleName::new_static("foo."), None);
    /// assert_eq!(ModuleName::new_static("foo..bar"), None);
    /// assert_eq!(ModuleName::new_static("2000"), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn new_static(name: &'static str) -> Option<Self> {
        // TODO(Micha): Use CompactString::const_new once we upgrade to 0.8 https://github.com/ParkMyCar/compact_str/pull/336
        Self::is_valid_name(name).then(|| Self(compact_str::CompactString::from(name)))
    }

    #[must_use]
    fn is_valid_name(name: &str) -> bool {
        !name.is_empty() && name.split('.').all(is_identifier)
    }

    /// An iterator over the components of the module name:
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar.baz").unwrap().components().collect::<Vec<_>>(), vec!["foo", "bar", "baz"]);
    /// ```
    #[must_use]
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// The name of this module's immediate parent, if it has a parent.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar").unwrap().parent(), Some(ModuleName::new_static("foo").unwrap()));
    /// assert_eq!(ModuleName::new_static("foo.bar.baz").unwrap().parent(), Some(ModuleName::new_static("foo.bar").unwrap()));
    /// assert_eq!(ModuleName::new_static("root").unwrap().parent(), None);
    /// ```
    #[must_use]
    pub fn parent(&self) -> Option<ModuleName> {
        let (parent, _) = self.0.rsplit_once('.')?;
        Some(Self(parent.to_compact_string()))
    }

    /// Returns `true` if the name starts with `other`.
    ///
    /// This is equivalent to checking if `self` is a sub-module of `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert!(ModuleName::new_static("foo.bar").unwrap().starts_with(&ModuleName::new_static("foo").unwrap()));
    ///
    /// assert!(!ModuleName::new_static("foo.bar").unwrap().starts_with(&ModuleName::new_static("bar").unwrap()));
    /// assert!(!ModuleName::new_static("foo_bar").unwrap().starts_with(&ModuleName::new_static("foo").unwrap()));
    /// ```
    #[must_use]
    pub fn starts_with(&self, other: &ModuleName) -> bool {
        let mut self_components = self.components();
        let other_components = other.components();

        for other_component in other_components {
            if self_components.next() != Some(other_component) {
                return false;
            }
        }

        true
    }

    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn from_components<'a>(mut components: impl Iterator<Item = &'a str>) -> Option<Self> {
        let first_part = components.next()?;
        if let Some(second_part) = components.next() {
            let mut name = format!("{first_part}.{second_part}");
            for part in components {
                name.push('.');
                name.push_str(part);
            }
            ModuleName::new(&name)
        } else {
            ModuleName::new(first_part)
        }
    }
}

impl Deref for ModuleName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<str> for ModuleName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<ModuleName> for str {
    fn eq(&self, other: &ModuleName) -> bool {
        self == other.as_str()
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
