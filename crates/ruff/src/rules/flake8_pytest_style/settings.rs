//! Settings for the `flake8-pytest-style` plugin.
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::settings::types::IdentifierPattern;
use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

use super::types;

fn default_broad_exceptions() -> Vec<IdentifierPattern> {
    [
        "BaseException",
        "Exception",
        "ValueError",
        "OSError",
        "IOError",
        "EnvironmentError",
        "socket.error",
    ]
    .map(|pattern| IdentifierPattern::new(pattern).expect("invalid default exception pattern"))
    .to_vec()
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8PytestStyleOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = "true",
        value_type = "bool",
        example = "fixture-parentheses = true"
    )]
    /// Boolean flag specifying whether `@pytest.fixture()` without parameters
    /// should have parentheses. If the option is set to `true` (the
    /// default), `@pytest.fixture()` is valid and `@pytest.fixture` is
    /// invalid. If set to `false`, `@pytest.fixture` is valid and
    /// `@pytest.fixture()` is invalid.
    pub fixture_parentheses: Option<bool>,
    #[option(
        default = "tuple",
        value_type = r#""csv" | "tuple" | "list""#,
        example = "parametrize-names-type = \"list\""
    )]
    /// Expected type for multiple argument names in `@pytest.mark.parametrize`.
    /// The following values are supported:
    ///
    /// - `csv` — a comma-separated list, e.g.
    ///   `@pytest.mark.parametrize('name1,name2', ...)`
    /// - `tuple` (default) — e.g.
    ///   `@pytest.mark.parametrize(('name1', 'name2'), ...)`
    /// - `list` — e.g. `@pytest.mark.parametrize(['name1', 'name2'], ...)`
    pub parametrize_names_type: Option<types::ParametrizeNameType>,
    #[option(
        default = "list",
        value_type = r#""tuple" | "list""#,
        example = "parametrize-values-type = \"tuple\""
    )]
    /// Expected type for the list of values rows in `@pytest.mark.parametrize`.
    /// The following values are supported:
    ///
    /// - `tuple` — e.g. `@pytest.mark.parametrize('name', (1, 2, 3))`
    /// - `list` (default) — e.g. `@pytest.mark.parametrize('name', [1, 2, 3])`
    pub parametrize_values_type: Option<types::ParametrizeValuesType>,
    #[option(
        default = "tuple",
        value_type = r#""tuple" | "list""#,
        example = "parametrize-values-row-type = \"list\""
    )]
    /// Expected type for each row of values in `@pytest.mark.parametrize` in
    /// case of multiple parameters. The following values are supported:
    ///
    /// - `tuple` (default) — e.g.
    ///   `@pytest.mark.parametrize(('name1', 'name2'), [(1, 2), (3, 4)])`
    /// - `list` — e.g.
    ///   `@pytest.mark.parametrize(('name1', 'name2'), [[1, 2], [3, 4]])`
    pub parametrize_values_row_type: Option<types::ParametrizeValuesRowType>,
    #[option(
        default = r#"["BaseException", "Exception", "ValueError", "OSError", "IOError", "EnvironmentError", "socket.error"]"#,
        value_type = "list[str]",
        example = "raises-require-match-for = [\"requests.RequestException\"]"
    )]
    /// List of exception names that require a match= parameter in a
    /// `pytest.raises()` call.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub raises_require_match_for: Option<Vec<String>>,
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = "raises-extend-require-match-for = [\"requests.RequestException\"]"
    )]
    /// List of additional exception names that require a match= parameter in a
    /// `pytest.raises()` call. This extends the default list of exceptions
    /// that require a match= parameter.
    /// This option is useful if you want to extend the default list of
    /// exceptions that require a match= parameter without having to specify
    /// the entire list.
    /// Note that this option does not remove any exceptions from the default
    /// list.
    ///
    /// Supports glob patterns. For more information on the glob syntax, refer
    /// to the [`globset` documentation](https://docs.rs/globset/latest/globset/#syntax).
    pub raises_extend_require_match_for: Option<Vec<String>>,
    #[option(
        default = "true",
        value_type = "bool",
        example = "mark-parentheses = true"
    )]
    /// Boolean flag specifying whether `@pytest.mark.foo()` without parameters
    /// should have parentheses. If the option is set to `true` (the
    /// default), `@pytest.mark.foo()` is valid and `@pytest.mark.foo` is
    /// invalid. If set to `false`, `@pytest.fixture` is valid and
    /// `@pytest.mark.foo()` is invalid.
    pub mark_parentheses: Option<bool>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub fixture_parentheses: bool,
    pub parametrize_names_type: types::ParametrizeNameType,
    pub parametrize_values_type: types::ParametrizeValuesType,
    pub parametrize_values_row_type: types::ParametrizeValuesRowType,
    pub raises_require_match_for: Vec<IdentifierPattern>,
    pub raises_extend_require_match_for: Vec<IdentifierPattern>,
    pub mark_parentheses: bool,
}

impl TryFrom<Options> for Settings {
    type Error = SettingsError;

    fn try_from(options: Options) -> Result<Self, Self::Error> {
        Ok(Self {
            fixture_parentheses: options.fixture_parentheses.unwrap_or(true),
            parametrize_names_type: options.parametrize_names_type.unwrap_or_default(),
            parametrize_values_type: options.parametrize_values_type.unwrap_or_default(),
            parametrize_values_row_type: options.parametrize_values_row_type.unwrap_or_default(),
            raises_require_match_for: options
                .raises_require_match_for
                .map(|patterns| {
                    patterns
                        .into_iter()
                        .map(|pattern| IdentifierPattern::new(&pattern))
                        .collect()
                })
                .transpose()
                .map_err(SettingsError::InvalidRaisesRequireMatchFor)?
                .unwrap_or_else(default_broad_exceptions),
            raises_extend_require_match_for: options
                .raises_extend_require_match_for
                .map(|patterns| {
                    patterns
                        .into_iter()
                        .map(|pattern| IdentifierPattern::new(&pattern))
                        .collect()
                })
                .transpose()
                .map_err(SettingsError::InvalidRaisesExtendRequireMatchFor)?
                .unwrap_or_default(),
            mark_parentheses: options.mark_parentheses.unwrap_or(true),
        })
    }
}
impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            fixture_parentheses: Some(settings.fixture_parentheses),
            parametrize_names_type: Some(settings.parametrize_names_type),
            parametrize_values_type: Some(settings.parametrize_values_type),
            parametrize_values_row_type: Some(settings.parametrize_values_row_type),
            raises_require_match_for: Some(
                settings
                    .raises_require_match_for
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
            raises_extend_require_match_for: Some(
                settings
                    .raises_extend_require_match_for
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
            mark_parentheses: Some(settings.mark_parentheses),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fixture_parentheses: true,
            parametrize_names_type: types::ParametrizeNameType::default(),
            parametrize_values_type: types::ParametrizeValuesType::default(),
            parametrize_values_row_type: types::ParametrizeValuesRowType::default(),
            raises_require_match_for: default_broad_exceptions(),
            raises_extend_require_match_for: vec![],
            mark_parentheses: true,
        }
    }
}

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidRaisesRequireMatchFor(glob::PatternError),
    InvalidRaisesExtendRequireMatchFor(glob::PatternError),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::InvalidRaisesRequireMatchFor(err) => {
                write!(f, "invalid raises-require-match-for pattern: {err}")
            }
            SettingsError::InvalidRaisesExtendRequireMatchFor(err) => {
                write!(f, "invalid raises-extend-require-match-for pattern: {err}")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidRaisesRequireMatchFor(err) => Some(err),
            SettingsError::InvalidRaisesExtendRequireMatchFor(err) => Some(err),
        }
    }
}
