use std::fmt::Formatter;
use std::ops::Deref;

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleName(smol_str::SmolStr);

impl ModuleName {
    #[inline]
    pub fn new(name: &str) -> Self {
        assert!(!name.is_empty());
        assert!(!name.starts_with('.'), "module name must be absolute");
        assert!(!name.ends_with('.'), "module cannot end with a '.'");

        Self(smol_str::SmolStr::new(name))
    }

    /// An iterator over the components of the module name:
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_python_semantic::module::ModuleName;
    ///
    /// assert_eq!(ModuleName::new("foo.bar.baz").components().collect::<Vec<_>>(), vec!["foo", "bar", "baz"]);
    /// ```
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// The name of this module's immediate parent, if it has a parent.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_python_semantic::module::ModuleName;
    ///
    /// assert_eq!(ModuleName::new("foo.bar").parent(), Some(ModuleName::new("foo")));
    /// assert_eq!(ModuleName::new("foo.bar.baz").parent(), Some(ModuleName::new("foo.bar")));
    /// assert_eq!(ModuleName::new("root").parent(), None);
    /// ```
    pub fn parent(&self) -> Option<ModuleName> {
        let (parent, _) = self.0.rsplit_once('.')?;

        Some(Self(smol_str::SmolStr::new(parent)))
    }

    /// Returns `true` if the name starts with `other`.
    ///
    /// This is equivalent to checking if `self` is a sub-module of `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ruff_python_semantic::module::ModuleName;
    ///
    /// assert!(ModuleName::new("foo.bar").starts_with(&ModuleName::new("foo")));
    ///
    /// assert!(!ModuleName::new("foo.bar").starts_with(&ModuleName::new("bar")));
    /// assert!(!ModuleName::new("foo_bar").starts_with(&ModuleName::new("foo")));
    /// ```
    pub fn starts_with(&self, other: &ModuleName) -> bool {
        let mut self_components = self.components();
        let mut other_components = other.components();

        while let Some(other_component) = other_components.next() {
            if self_components.next() != Some(other_component) {
                return false;
            }
        }

        true
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for ModuleName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
