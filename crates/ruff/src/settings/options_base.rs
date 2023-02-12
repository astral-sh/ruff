pub trait ConfigurationOptions {
    fn get_available_options() -> Vec<(&'static str, OptionEntry)>;

    /// Get an option entry by its fully-qualified name
    /// (e.g. `foo.bar` refers to the `bar` option in the `foo` group).
    fn get(name: Option<&str>) -> Option<OptionEntry> {
        let mut entries = Self::get_available_options();

        let mut parts_iter = name.into_iter().flat_map(|s| s.split('.'));

        while let Some(part) = parts_iter.next() {
            let (_, field) = entries.into_iter().find(|(name, _)| *name == part)?;
            match field {
                OptionEntry::Field(..) => {
                    if parts_iter.next().is_some() {
                        return None;
                    }

                    return Some(field);
                }
                OptionEntry::Group(fields) => {
                    entries = fields;
                }
            }
        }
        Some(OptionEntry::Group(entries))
    }
}

#[derive(Debug)]
pub enum OptionEntry {
    Field(OptionField),
    Group(Vec<(&'static str, OptionEntry)>),
}

#[derive(Debug)]
pub struct OptionField {
    pub doc: &'static str,
    pub default: &'static str,
    pub value_type: &'static str,
    pub example: &'static str,
}
