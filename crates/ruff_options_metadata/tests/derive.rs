use ruff_options_metadata::{OptionEntry, OptionsMetadata};

#[expect(dead_code)]
#[derive(Default, ruff_macros::OptionsMetadata, serde::Deserialize)]
struct NestedOptions {
    /// A nested option.
    #[option(default = "false", value_type = "bool", example = "nested = true")]
    nested: Option<bool>,
}

#[expect(dead_code)]
#[derive(ruff_macros::OptionsMetadata, serde::Deserialize)]
struct FlattenedOptions {
    #[serde(default)]
    #[serde(flatten)]
    nested: NestedOptions,
}

#[expect(dead_code)]
#[derive(ruff_macros::OptionsMetadata)]
struct BareDeprecatedOptions {
    /// A deprecated option.
    #[deprecated]
    #[option(default = "false", value_type = "bool", example = "value = true")]
    value: Option<bool>,
}

#[test]
fn finds_flatten_in_repeated_serde_attributes() {
    let metadata = FlattenedOptions::metadata();
    assert!(metadata.has("nested"));
}

#[test]
fn accepts_bare_deprecated_attribute() {
    let metadata = BareDeprecatedOptions::metadata();
    let deprecated = metadata
        .find("value")
        .and_then(OptionEntry::into_field)
        .and_then(|field| field.deprecated)
        .expect("field should be deprecated");

    assert_eq!(deprecated.since, None);
    assert_eq!(deprecated.message, None);
}
