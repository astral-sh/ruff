use std::assert_matches;

use ruff_options_metadata::{OptionEntry, OptionField, OptionsMetadata, Visit};

const RESPECT_GITIGNORE: OptionField = OptionField {
    doc: "Respect the gitignore file.",
    default: "true",
    value_type: "bool",
    example: "",
    scope: None,
    deprecated: None,
};
const HARD_TABS: OptionField = OptionField {
    doc: "Use hard tabs for indentation.",
    default: "false",
    ..RESPECT_GITIGNORE
};

struct Root;
struct Format;

impl OptionsMetadata for Root {
    fn record(visit: &mut dyn Visit) {
        visit.record_field("respect-gitignore", RESPECT_GITIGNORE);
        visit.record_set("format", Format::metadata());
    }
}

impl OptionsMetadata for Format {
    fn record(visit: &mut dyn Visit) {
        visit.record_field("hard-tabs", HARD_TABS);
    }
}

#[test]
fn has_options() {
    let options = Root::metadata();
    assert!(options.has("respect-gitignore"));
    assert!(options.has("format.hard-tabs"));
    assert!(!options.has("does-not-exist"));
    assert!(!options.has("format.spaces"));
    assert!(!options.has("lint.hard-tabs"));
}

#[test]
fn find_options() {
    let options = Root::metadata();
    assert_eq!(
        options
            .find("respect-gitignore")
            .and_then(OptionEntry::into_field),
        Some(RESPECT_GITIGNORE),
    );
    assert_eq!(
        options
            .find("format.hard-tabs")
            .and_then(OptionEntry::into_field),
        Some(HARD_TABS),
    );
    assert_matches!(options.find("format"), Some(OptionEntry::Set(_)));
    assert!(options.find("does-not-exist").is_none());
    assert!(options.find("format.spaces").is_none());
    assert!(options.find("lint.hard-tabs").is_none());
}
