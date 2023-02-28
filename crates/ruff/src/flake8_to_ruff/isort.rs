//! Extract isort configuration settings from a pyproject.toml.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::rules::isort::{settings::RelativeImportsOrder, ImportType};

fn default_lines_after_imports() -> i64 {
    -1
}

fn default_order_by_type() -> bool {
    true
}

fn default_relative_imports_order() -> RelativeImportsOrder {
    RelativeImportsOrder::FurthestToClosest
}

/// The [isort configuration](https://pycqa.github.io/isort/docs/configuration/config_files.html).
// See https://github.com/PyCQA/isort/blob/3a72e069635a865a92b8a0273aa829f630cbcd6f/isort/settings.py
// See https://beta.ruff.rs/docs/settings/#isort
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Isort {
    #[serde(alias = "src-paths", alias = "src_paths")]
    pub src_paths: Option<Vec<String>>,

    // Only options in https://beta.ruff.rs/docs/settings/#isort
    #[serde(default)]
    pub classes: HashSet<String>,
    #[serde(default, alias = "combine_as_imports", alias = "combine-as-imports")]
    pub combine_as_imports: bool,
    #[serde(default)]
    pub constants: HashSet<String>,
    #[serde(
        default,
        alias = "extra_standard_library",
        alias = "extra-standard-library"
    )]
    pub extra_standard_library: HashSet<String>,
    #[serde(default, alias = "force_single_line", alias = "force-single-line")]
    pub force_single_line: bool,
    #[serde(
        default,
        alias = "force_sort_within_sections",
        alias = "force-sort-within-sections"
    )]
    pub force_sort_within_sections: bool,
    #[serde(default, alias = "force_to_top", alias = "force-to-top")]
    pub force_to_top: HashSet<String>,
    #[serde(default, alias = "force_wrap_aliases", alias = "force-wrap-aliases")]
    pub force_wrap_aliases: bool,
    #[serde(default, alias = "force_separate", alias = "force-separate")]
    pub forced_separate: Box<[String]>,
    #[serde(default, alias = "known_first_party", alias = "known-first-party")]
    pub known_first_party: HashSet<String>,
    #[serde(default, alias = "known_local_folder", alias = "known-local-folder")]
    pub known_local_folder: HashSet<String>,
    #[serde(default, alias = "known_third_party", alias = "known-third-party")]
    pub known_third_party: HashSet<String>,
    #[serde(
        default = "default_lines_after_imports",
        alias = "lines_after_imports",
        alias = "lines-after-imports"
    )]
    pub lines_after_imports: i64,
    #[serde(default, alias = "lines_between_types", alias = "lines-between-types")]
    pub lines_between_types: i64,
    #[serde(default, alias = "no_lines_before", alias = "no-lines-before")]
    pub no_lines_before: HashSet<ImportType>,
    #[serde(
        default = "default_order_by_type",
        alias = "order_by_type",
        alias = "order-by-type"
    )]
    pub order_by_type: bool,
    #[serde(
        default = "default_relative_imports_order",
        alias = "relative_imports_order",
        alias = "relative-imports-order"
    )]
    pub relative_imports_order: RelativeImportsOrder,
    #[serde(default, alias = "required_imports", alias = "required-imports")]
    pub required_imports: Box<[String]>,
    #[serde(
        default,
        alias = "single_line_exclusions",
        alias = "single-line-exclusions"
    )]
    pub single_line_exclusions: Box<[String]>,
    #[serde(
        default,
        alias = "split_on_trailing_comma",
        alias = "split-on-trailing-comma"
    )]
    pub split_on_trailing_comma: bool,
    #[serde(default)]
    pub variables: HashSet<String>,
}
