use std::fmt::{Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub enum OptionEntry {
    Field(OptionField),
    Group(OptionGroup),
}

impl Display for OptionEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionEntry::Field(field) => field.fmt(f),
            OptionEntry::Group(group) => group.fmt(f),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct OptionGroup(&'static [(&'static str, OptionEntry)]);

impl OptionGroup {
    pub const fn new(options: &'static [(&'static str, OptionEntry)]) -> Self {
        Self(options)
    }

    pub fn iter(&self) -> std::slice::Iter<(&str, OptionEntry)> {
        self.into_iter()
    }

    /// Get an option entry by its fully-qualified name
    /// (e.g. `foo.bar` refers to the `bar` option in the `foo` group).
    ///
    /// ## Examples
    ///
    /// ### Find a direct child
    ///
    /// ```rust
    /// # use ruff_workspace::options_base::{OptionGroup, OptionEntry, OptionField};
    ///
    /// const options: [(&'static str, OptionEntry); 2] = [
    ///     ("ignore_names", OptionEntry::Field(OptionField {
    ///         doc: "ignore_doc",
    ///         default: "ignore_default",
    ///         value_type: "value_type",
    ///         example: "ignore code"
    ///     })),
    ///
    ///     ("global_names", OptionEntry::Field(OptionField {
    ///         doc: "global_doc",
    ///         default: "global_default",
    ///         value_type: "value_type",
    ///         example: "global code"
    ///     }))
    /// ];
    ///
    /// let group = OptionGroup::new(&options);
    ///
    /// let ignore_names = group.get("ignore_names");
    ///
    /// match ignore_names {
    ///     None => panic!("Expect option 'ignore_names' to be Some"),
    ///     Some(OptionEntry::Group(group)) => panic!("Expected 'ignore_names' to be a field but found group {}", group),
    ///     Some(OptionEntry::Field(field)) => {
    ///         assert_eq!("ignore_doc", field.doc);
    ///     }
    /// }
    ///
    /// assert_eq!(None, group.get("not_existing_option"));
    /// ```
    ///
    /// ### Find a nested options
    ///
    /// ```rust
    /// # use ruff_workspace::options_base::{OptionGroup, OptionEntry, OptionField};
    ///
    /// const ignore_options: [(&'static str, OptionEntry); 2] = [
    ///     ("names", OptionEntry::Field(OptionField {
    ///         doc: "ignore_name_doc",
    ///         default: "ignore_name_default",
    ///         value_type: "value_type",
    ///         example: "ignore name code"
    ///     })),
    ///
    ///     ("extensions", OptionEntry::Field(OptionField {
    ///         doc: "ignore_extensions_doc",
    ///         default: "ignore_extensions_default",
    ///         value_type: "value_type",
    ///         example: "ignore extensions code"
    ///     }))
    /// ];
    ///
    /// const options: [(&'static str, OptionEntry); 2] = [
    ///     ("ignore", OptionEntry::Group(OptionGroup::new(&ignore_options))),
    ///
    ///     ("global_names", OptionEntry::Field(OptionField {
    ///         doc: "global_doc",
    ///         default: "global_default",
    ///         value_type: "value_type",
    ///         example: "global code"
    ///     }))
    /// ];
    ///
    /// let group = OptionGroup::new(&options);
    ///
    /// let ignore_names = group.get("ignore.names");
    ///
    /// match ignore_names {
    ///     None => panic!("Expect option 'ignore.names' to be Some"),
    ///     Some(OptionEntry::Group(group)) => panic!("Expected 'ignore_names' to be a field but found group {}", group),
    ///     Some(OptionEntry::Field(field)) => {
    ///         assert_eq!("ignore_name_doc", field.doc);
    ///     }
    /// }
    /// ```
    pub fn get(&self, name: &str) -> Option<&OptionEntry> {
        let mut parts = name.split('.').peekable();

        let mut options = self.iter();

        loop {
            let part = parts.next()?;

            let (_, field) = options.find(|(name, _)| *name == part)?;

            match (parts.peek(), field) {
                (None, field) => return Some(field),
                (Some(..), OptionEntry::Field(..)) => return None,
                (Some(..), OptionEntry::Group(group)) => {
                    options = group.iter();
                }
            }
        }
    }
}

impl<'a> IntoIterator for &'a OptionGroup {
    type IntoIter = std::slice::Iter<'a, (&'a str, OptionEntry)>;
    type Item = &'a (&'a str, OptionEntry);

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for OptionGroup {
    type IntoIter = std::slice::Iter<'static, (&'static str, OptionEntry)>;
    type Item = &'static (&'static str, OptionEntry);

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Display for OptionGroup {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (name, _) in self {
            writeln!(f, "{name}")?;
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct OptionField {
    pub doc: &'static str,
    pub default: &'static str,
    pub value_type: &'static str,
    pub example: &'static str,
}

impl Display for OptionField {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.doc)?;
        writeln!(f)?;
        writeln!(f, "Default value: {}", self.default)?;
        writeln!(f, "Type: {}", self.value_type)?;
        writeln!(f, "Example usage:\n```toml\n{}\n```", self.example)
    }
}
