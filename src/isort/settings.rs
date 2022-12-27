//! Settings for the `isort` plugin.

use std::collections::BTreeSet;

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "IsortOptions"
)]
pub struct Options {
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-wrap-aliases = true
            combine-as-imports = true
        "#
    )]
    /// Force `import from` statements with multiple members and at least one
    /// alias (e.g., `import A as B`) to wrap such that every line contains
    /// exactly one member. For example, this formatting would be retained,
    /// rather than condensing to a single line:
    ///
    /// ```py
    /// from .utils import (
    ///     test_directory as test_directory,
    ///     test_id as test_id
    /// )
    /// ```
    ///
    /// Note that this setting is only effective when combined with
    /// `combine-as-imports = true`. When `combine-as-imports` isn't
    /// enabled, every aliased `import from` will be given its own line, in
    /// which case, wrapping is not necessary.
    pub force_wrap_aliases: Option<bool>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"force-single-line = true"#
    )]
    /// Forces all from imports to appear on their own line.
    pub force_single_line: Option<bool>,
    #[option(
        default = r#"[]"#,
        value_type = "Vec<String>",
        example = r#"
            single-line-exclusions = ["os", "json"]
        "#
    )]
    /// One or more modules to exclude from the single line rule.
    pub single_line_exclusions: Option<Vec<String>>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            combine-as-imports = true
        "#
    )]
    /// Combines as imports on the same line. See isort's [`combine-as-imports`](https://pycqa.github.io/isort/docs/configuration/options.html#combine-as-imports)
    /// option.
    pub combine_as_imports: Option<bool>,
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            split-on-trailing-comma = false
        "#
    )]
    /// If a comma is placed after the last member in a multi-line import, then
    /// the imports will never be folded into one line.
    ///
    /// See isort's [`split-on-trailing-comma`](https://pycqa.github.io/isort/docs/configuration/options.html#split-on-trailing-comma) option.
    pub split_on_trailing_comma: Option<bool>,
    #[option(
        default = r#"[]"#,
        value_type = "Vec<String>",
        example = r#"
            known-first-party = ["src"]
        "#
    )]
    /// A list of modules to consider first-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    pub known_first_party: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "Vec<String>",
        example = r#"
            known-third-party = ["src"]
        "#
    )]
    /// A list of modules to consider third-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    pub known_third_party: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "Vec<String>",
        example = r#"
            extra-standard-library = ["path"]
        "#
    )]
    /// A list of modules to consider standard-library, in addition to those
    /// known to Ruff in advance.
    pub extra_standard_library: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub combine_as_imports: bool,
    pub force_wrap_aliases: bool,
    pub split_on_trailing_comma: bool,
    pub force_single_line: bool,
    pub single_line_exclusions: BTreeSet<String>,
    pub known_first_party: BTreeSet<String>,
    pub known_third_party: BTreeSet<String>,
    pub extra_standard_library: BTreeSet<String>,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            combine_as_imports: options.combine_as_imports.unwrap_or(false),
            force_wrap_aliases: options.force_wrap_aliases.unwrap_or(false),
            split_on_trailing_comma: options.split_on_trailing_comma.unwrap_or(true),
            force_single_line: options.force_single_line.unwrap_or(false),
            single_line_exclusions: BTreeSet::from_iter(
                options.single_line_exclusions.unwrap_or_default(),
            ),
            known_first_party: BTreeSet::from_iter(options.known_first_party.unwrap_or_default()),
            known_third_party: BTreeSet::from_iter(options.known_third_party.unwrap_or_default()),
            extra_standard_library: BTreeSet::from_iter(
                options.extra_standard_library.unwrap_or_default(),
            ),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            combine_as_imports: false,
            force_wrap_aliases: false,
            split_on_trailing_comma: true,
            force_single_line: false,
            single_line_exclusions: BTreeSet::new(),
            known_first_party: BTreeSet::new(),
            known_third_party: BTreeSet::new(),
            extra_standard_library: BTreeSet::new(),
        }
    }
}
