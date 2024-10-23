//! Settings for the `isort` plugin.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::display_settings;
use crate::rules::isort::categorize::KnownModules;
use crate::rules::isort::ImportType;
use ruff_macros::CacheKey;
use ruff_python_semantic::NameImport;

use super::categorize::ImportSection;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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

impl Display for RelativeImportsOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClosestToFurthest => write!(f, "closest_to_furthest"),
            Self::FurthestToClosest => write!(f, "furthest_to_closest"),
        }
    }
}

#[derive(Debug, Clone, CacheKey)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub required_imports: BTreeSet<NameImport>,
    pub combine_as_imports: bool,
    pub force_single_line: bool,
    pub force_sort_within_sections: bool,
    pub case_sensitive: bool,
    pub force_wrap_aliases: bool,
    pub force_to_top: FxHashSet<String>,
    pub known_modules: KnownModules,
    pub detect_same_package: bool,
    pub order_by_type: bool,
    pub relative_imports_order: RelativeImportsOrder,
    pub single_line_exclusions: FxHashSet<String>,
    pub split_on_trailing_comma: bool,
    pub classes: FxHashSet<String>,
    pub constants: FxHashSet<String>,
    pub variables: FxHashSet<String>,
    pub no_lines_before: FxHashSet<ImportSection>,
    pub lines_after_imports: isize,
    pub lines_between_types: usize,
    pub forced_separate: Vec<String>,
    pub section_order: Vec<ImportSection>,
    pub default_section: ImportSection,
    pub no_sections: bool,
    pub from_first: bool,
    pub length_sort: bool,
    pub length_sort_straight: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            required_imports: BTreeSet::default(),
            combine_as_imports: false,
            force_single_line: false,
            force_sort_within_sections: false,
            detect_same_package: true,
            case_sensitive: false,
            force_wrap_aliases: false,
            force_to_top: FxHashSet::default(),
            known_modules: KnownModules::default(),
            order_by_type: true,
            relative_imports_order: RelativeImportsOrder::default(),
            single_line_exclusions: FxHashSet::default(),
            split_on_trailing_comma: true,
            classes: FxHashSet::default(),
            constants: FxHashSet::default(),
            variables: FxHashSet::default(),
            no_lines_before: FxHashSet::default(),
            lines_after_imports: -1,
            lines_between_types: 0,
            forced_separate: Vec::new(),
            section_order: ImportType::iter().map(ImportSection::Known).collect(),
            default_section: ImportSection::Known(ImportType::ThirdParty),
            no_sections: false,
            from_first: false,
            length_sort: false,
            length_sort_straight: false,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.isort",
            fields = [
                self.required_imports | set,
                self.combine_as_imports,
                self.force_single_line,
                self.force_sort_within_sections,
                self.detect_same_package,
                self.case_sensitive,
                self.force_wrap_aliases,
                self.force_to_top | set,
                self.known_modules,
                self.order_by_type,
                self.relative_imports_order,
                self.single_line_exclusions | set,
                self.split_on_trailing_comma,
                self.classes | set,
                self.constants | set,
                self.variables | set,
                self.no_lines_before | set,
                self.lines_after_imports,
                self.lines_between_types,
                self.forced_separate | array,
                self.section_order | array,
                self.default_section,
                self.no_sections,
                self.from_first,
                self.length_sort,
                self.length_sort_straight
            ]
        }
        Ok(())
    }
}

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidKnownFirstParty(glob::PatternError),
    InvalidKnownThirdParty(glob::PatternError),
    InvalidKnownLocalFolder(glob::PatternError),
    InvalidExtraStandardLibrary(glob::PatternError),
    InvalidUserDefinedSection(glob::PatternError),
}

impl Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::InvalidKnownThirdParty(err) => {
                write!(f, "invalid known third-party pattern: {err}")
            }
            SettingsError::InvalidKnownFirstParty(err) => {
                write!(f, "invalid known first-party pattern: {err}")
            }
            SettingsError::InvalidKnownLocalFolder(err) => {
                write!(f, "invalid known local folder pattern: {err}")
            }
            SettingsError::InvalidExtraStandardLibrary(err) => {
                write!(f, "invalid extra standard library pattern: {err}")
            }
            SettingsError::InvalidUserDefinedSection(err) => {
                write!(f, "invalid user-defined section pattern: {err}")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidKnownThirdParty(err) => Some(err),
            SettingsError::InvalidKnownFirstParty(err) => Some(err),
            SettingsError::InvalidKnownLocalFolder(err) => Some(err),
            SettingsError::InvalidExtraStandardLibrary(err) => Some(err),
            SettingsError::InvalidUserDefinedSection(err) => Some(err),
        }
    }
}
