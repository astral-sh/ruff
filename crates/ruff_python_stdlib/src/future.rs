/// Returns `true` if `name` is a valid `__future__` feature name, as defined by
/// `__future__.all_feature_names`.
pub fn is_feature_name(name: &str) -> bool {
    matches!(
        name,
        "nested_scopes"
            | "generators"
            | "division"
            | "absolute_import"
            | "with_statement"
            | "print_function"
            | "unicode_literals"
            | "barry_as_FLUFL"
            | "generator_stop"
            | "annotations"
    )
}
