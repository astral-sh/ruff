use std::collections::{BTreeMap, BTreeSet};

// STOPSHIP(charlie): Turn these into structs.
pub type FromData<'a> = (&'a Option<String>, &'a Option<usize>);
pub type AliasData<'a> = (&'a str, &'a Option<String>);

#[derive(Debug, Default)]
pub struct ImportBlock<'a> {
    // Map from (module, level) to `AliasData`.
    pub import_from: BTreeMap<FromData<'a>, BTreeSet<AliasData<'a>>>,
    // Set of (name, asname).
    pub import: BTreeSet<AliasData<'a>>,
}
