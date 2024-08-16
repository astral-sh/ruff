use std::fmt;
use std::ops::Deref;

use compact_str::{CompactString, ToCompactString};

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
        Self::is_valid_name(name).then(|| Self(CompactString::from(name)))
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
    /// use red_knot_python_semantic::ModuleName;
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
        Self::is_valid_name(name).then(|| Self(CompactString::const_new(name)))
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
    /// use red_knot_python_semantic::ModuleName;
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
    /// use red_knot_python_semantic::ModuleName;
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
    /// use red_knot_python_semantic::ModuleName;
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

    /// Construct a [`ModuleName`] from a sequence of parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert_eq!(&*ModuleName::from_components(["a"]).unwrap(), "a");
    /// assert_eq!(&*ModuleName::from_components(["a", "b"]).unwrap(), "a.b");
    /// assert_eq!(&*ModuleName::from_components(["a", "b", "c"]).unwrap(), "a.b.c");
    ///
    /// assert_eq!(ModuleName::from_components(["a-b"]), None);
    /// assert_eq!(ModuleName::from_components(["a", "a-b"]), None);
    /// assert_eq!(ModuleName::from_components(["a", "b", "a-b-c"]), None);
    /// ```
    #[must_use]
    pub fn from_components<'a>(components: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut components = components.into_iter();
        let first_part = components.next()?;
        if !is_identifier(first_part) {
            return None;
        }
        let name = if let Some(second_part) = components.next() {
            if !is_identifier(second_part) {
                return None;
            }
            let mut name = format!("{first_part}.{second_part}");
            for part in components {
                if !is_identifier(part) {
                    return None;
                }
                name.push('.');
                name.push_str(part);
            }
            CompactString::from(&name)
        } else {
            CompactString::from(first_part)
        };
        Some(Self(name))
    }

    /// Extend `self` with the components of `other`
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// let mut module_name = ModuleName::new_static("foo").unwrap();
    /// module_name.extend(&ModuleName::new_static("bar").unwrap());
    /// assert_eq!(&module_name, "foo.bar");
    /// module_name.extend(&ModuleName::new_static("baz.eggs.ham").unwrap());
    /// assert_eq!(&module_name, "foo.bar.baz.eggs.ham");
    /// ```
    pub fn extend(&mut self, other: &ModuleName) {
        self.0.push('.');
        self.0.push_str(other);
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
