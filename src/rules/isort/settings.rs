//! Settings for the `isort` plugin.

use std::collections::BTreeSet;

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::categorize::ImportType;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum RelativeImportsOrder {
    /// Place "closer" imports (fewer `.` characters, most local) before
    /// "further" imports (more `.` characters, least local).
    ClosestToFurthest,
    /// Place "further" imports (more `.` characters, least local) imports
    /// before "closer" imports (fewer `.` characters, most local).
    FurthestToClosest,
}

impl Default for RelativeImportsOrder {
    fn default() -> Self {
        Self::FurthestToClosest
    }
}

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
        value_type = "list[str]",
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
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            order-by-type = true
        "#
    )]
    /// Order imports by type, which is determined by case, in addition to
    /// alphabetically.
    pub order_by_type: Option<bool>,
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            force-sort-within-sections = true
        "#
    )]
    /// Don't sort straight-style imports (like `import sys`) before from-style
    /// imports (like `from itertools import groupby`). Instead, sort the
    /// imports by module, independent of import style.
    pub force_sort_within_sections: Option<bool>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-first-party = ["src"]
        "#
    )]
    /// A list of modules to consider first-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    pub known_first_party: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            known-third-party = ["src"]
        "#
    )]
    /// A list of modules to consider third-party, regardless of whether they
    /// can be identified as such via introspection of the local filesystem.
    pub known_third_party: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            extra-standard-library = ["path"]
        "#
    )]
    /// A list of modules to consider standard-library, in addition to those
    /// known to Ruff in advance.
    pub extra_standard_library: Option<Vec<String>>,
    #[option(
        default = r#"furthest-to-closest"#,
        value_type = r#""furthest-to-closest" | "closest-to-furthest""#,
        example = r#"
            relative-imports-order = "closest-to-furthest"
        "#
    )]
    /// Whether to place "closer" imports (fewer `.` characters, most local)
    /// before "further" imports (more `.` characters, least local), or vice
    /// versa.
    ///
    /// The default ("furthest-to-closest") is equivalent to isort's
    /// `reverse-relative` default (`reverse-relative = false`); setting
    /// this to "closest-to-furthest" is equivalent to isort's `reverse-relative
    /// = true`.
    pub relative_imports_order: Option<RelativeImportsOrder>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            required-imports = ["from __future__ import annotations"]
        "#
    )]
    /// Add the specified import line to all files.
    pub required_imports: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            classes = ["SVC"]
        "#
    )]
    /// An override list of tokens to always recognize as a Class for
    /// `order-by-type` regardless of casing.
    pub classes: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            constants = ["constant"]
        "#
    )]
    /// An override list of tokens to always recognize as a CONSTANT
    /// for `order-by-type` regardless of casing.
    pub constants: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            variables = ["VAR"]
        "#
    )]
    /// An override list of tokens to always recognize as a var
    /// for `order-by-type` regardless of casing.
    pub variables: Option<Vec<String>>,
    #[option(
        default = r#"[]"#,
        value_type = r#"list["future" | "standard-library" | "third-party" | "first-party" | "local-folder"]"#,
        example = r#"
            no-lines-before = ["future", "standard-library"]
        "#
    )]
    /// A list of sections that should _not_ be delineated from the previous
    /// section via empty lines.
    pub no_lines_before: Option<Vec<ImportType>>,
    #[option(
        default = r#"-1"#,
        value_type = "int",
        example = r#"
            # Use a single line after each import block.
            lines-after-imports = 1
        "#
    )]
    /// The number of blank lines to place after imports.
    /// -1 for automatic determination.
    pub lines_after_imports: Option<isize>,
    #[option(
        default = r#"[]"#,
        value_type = "Vec<String>",
        example = r#"
            forced-separate = ["tests"]
        "#
    )]
    /// A list of modules to separate into auxiliary block(s) of imports,
    /// in the order specified.
    pub forced_separate: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub required_imports: BTreeSet<String>,
    pub combine_as_imports: bool,
    pub extra_standard_library: BTreeSet<String>,
    pub force_single_line: bool,
    pub force_sort_within_sections: bool,
    pub force_wrap_aliases: bool,
    pub known_first_party: BTreeSet<String>,
    pub known_third_party: BTreeSet<String>,
    pub order_by_type: bool,
    pub relative_imports_order: RelativeImportsOrder,
    pub single_line_exclusions: BTreeSet<String>,
    pub split_on_trailing_comma: bool,
    pub classes: BTreeSet<String>,
    pub constants: BTreeSet<String>,
    pub variables: BTreeSet<String>,
    pub no_lines_before: BTreeSet<ImportType>,
    pub lines_after_imports: isize,
    pub forced_separate: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            required_imports: BTreeSet::new(),
            combine_as_imports: false,
            extra_standard_library: BTreeSet::new(),
            force_single_line: false,
            force_sort_within_sections: false,
            force_wrap_aliases: false,
            known_first_party: BTreeSet::new(),
            known_third_party: BTreeSet::new(),
            order_by_type: true,
            relative_imports_order: RelativeImportsOrder::default(),
            single_line_exclusions: BTreeSet::new(),
            split_on_trailing_comma: true,
            classes: BTreeSet::new(),
            constants: BTreeSet::new(),
            variables: BTreeSet::new(),
            no_lines_before: BTreeSet::new(),
            lines_after_imports: -1,
            forced_separate: Vec::new(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            required_imports: BTreeSet::from_iter(options.required_imports.unwrap_or_default()),
            combine_as_imports: options.combine_as_imports.unwrap_or(false),
            extra_standard_library: BTreeSet::from_iter(
                options.extra_standard_library.unwrap_or_default(),
            ),
            force_single_line: options.force_single_line.unwrap_or(false),
            force_sort_within_sections: options.force_sort_within_sections.unwrap_or(false),
            force_wrap_aliases: options.force_wrap_aliases.unwrap_or(false),
            known_first_party: BTreeSet::from_iter(options.known_first_party.unwrap_or_default()),
            known_third_party: BTreeSet::from_iter(options.known_third_party.unwrap_or_default()),
            order_by_type: options.order_by_type.unwrap_or(true),
            relative_imports_order: options.relative_imports_order.unwrap_or_default(),
            single_line_exclusions: BTreeSet::from_iter(
                options.single_line_exclusions.unwrap_or_default(),
            ),
            split_on_trailing_comma: options.split_on_trailing_comma.unwrap_or(true),
            classes: BTreeSet::from_iter(options.classes.unwrap_or_default()),
            constants: BTreeSet::from_iter(options.constants.unwrap_or_default()),
            variables: BTreeSet::from_iter(options.variables.unwrap_or_default()),
            no_lines_before: BTreeSet::from_iter(options.no_lines_before.unwrap_or_default()),
            lines_after_imports: options.lines_after_imports.unwrap_or(-1),
            forced_separate: Vec::from_iter(options.forced_separate.unwrap_or_default()),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            required_imports: Some(settings.required_imports.into_iter().collect()),
            combine_as_imports: Some(settings.combine_as_imports),
            extra_standard_library: Some(settings.extra_standard_library.into_iter().collect()),
            force_single_line: Some(settings.force_single_line),
            force_sort_within_sections: Some(settings.force_sort_within_sections),
            force_wrap_aliases: Some(settings.force_wrap_aliases),
            known_first_party: Some(settings.known_first_party.into_iter().collect()),
            known_third_party: Some(settings.known_third_party.into_iter().collect()),
            order_by_type: Some(settings.order_by_type),
            relative_imports_order: Some(settings.relative_imports_order),
            single_line_exclusions: Some(settings.single_line_exclusions.into_iter().collect()),
            split_on_trailing_comma: Some(settings.split_on_trailing_comma),
            classes: Some(settings.classes.into_iter().collect()),
            constants: Some(settings.constants.into_iter().collect()),
            variables: Some(settings.variables.into_iter().collect()),
            no_lines_before: Some(settings.no_lines_before.into_iter().collect()),
            lines_after_imports: Some(settings.lines_after_imports),
            forced_separate: Some(settings.forced_separate.into_iter().collect()),
        }
    }
}
