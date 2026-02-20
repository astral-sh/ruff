//! Effective program settings, taking into account pyproject.toml and
//! command-line options. Structure is optimized for internal usage, as opposed
//! to external visibility or parsing.

use regex::Regex;
use rustc_hash::FxHashSet;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use types::CompiledPerFileTargetVersionList;

use crate::codes::RuleCodePrefix;
use ruff_macros::CacheKey;
use ruff_python_ast::PythonVersion;

use crate::line_width::LineLength;
use crate::registry::{Linter, Rule};
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_boolean_trap, flake8_bugbear, flake8_builtins,
    flake8_comprehensions, flake8_copyright, flake8_errmsg, flake8_gettext,
    flake8_implicit_str_concat, flake8_import_conventions, flake8_pytest_style, flake8_quotes,
    flake8_self, flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe,
    pep8_naming, pycodestyle, pydoclint, pydocstyle, pyflakes, pylint, pyupgrade, ruff,
};
use crate::settings::types::{CompiledPerFileIgnoreList, ExtensionMapping, FilePatternSet};
use crate::{RuleSelector, codes, fs};

use super::line_width::IndentWidth;

use self::fix_safety_table::FixSafetyTable;
use self::rule_table::RuleTable;
use self::types::PreviewMode;
use crate::rule_selector::PreviewOptions;

pub mod fix_safety_table;
pub mod flags;
pub mod rule_table;
pub mod types;

/// `display_settings!` is a macro that can display and format struct fields in a readable,
/// namespaced format. It's particularly useful at generating `Display` implementations
/// for types used in settings.
///
/// # Example
/// ```
/// use std::fmt;
/// use ruff_linter::display_settings;
/// #[derive(Default)]
/// struct Settings {
///     option_a: bool,
///     sub_settings: SubSettings,
///     option_b: String,
/// }
///
/// struct SubSettings {
///     name: String
/// }
///
/// impl Default for SubSettings {
///     fn default() -> Self {
///         Self { name: "Default Name".into() }
///     }
///
/// }
///
/// impl fmt::Display for SubSettings {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         display_settings! {
///             formatter = f,
///             namespace = "sub_settings",
///             fields = [
///                 self.name | quoted
///             ]
///         }
///         Ok(())
///     }
///
/// }
///
/// impl fmt::Display for Settings {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         display_settings! {
///             formatter = f,
///             fields = [
///                 self.option_a,
///                 self.sub_settings | nested,
///                 self.option_b | quoted,
///             ]
///         }
///         Ok(())
///     }
///
/// }
///
/// const EXPECTED_OUTPUT: &str = r#"option_a = false
/// sub_settings.name = "Default Name"
/// option_b = ""
/// "#;
///
/// fn main() {
///     let settings = Settings::default();
///     assert_eq!(format!("{settings}"), EXPECTED_OUTPUT);
/// }
/// ```
#[macro_export]
macro_rules! display_settings {
    (formatter = $fmt:ident, namespace = $namespace:literal, fields = [$($settings:ident.$field:ident $(| $modifier:tt)?),* $(,)?]) => {
        {
            const _PREFIX: &str = concat!($namespace, ".");
            $(
                display_settings!(@field $fmt, _PREFIX, $settings.$field $(| $modifier)?);
            )*
        }
    };
    (formatter = $fmt:ident, fields = [$($settings:ident.$field:ident $(| $modifier:tt)?),* $(,)?]) => {
        {
            const _PREFIX: &str = "";
            $(
                display_settings!(@field $fmt, _PREFIX, $settings.$field $(| $modifier)?);
            )*
        }
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | debug) => {
        writeln!($fmt, "{}{} = {:?}", $prefix, stringify!($field), $settings.$field)?;
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | path) => {
        writeln!($fmt, "{}{} = \"{}\"", $prefix, stringify!($field), $settings.$field.display())?;
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | quoted) => {
        writeln!($fmt, "{}{} = \"{}\"", $prefix, stringify!($field), $settings.$field)?;
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | globmatcher) => {
        writeln!($fmt, "{}{} = \"{}\"", $prefix, stringify!($field), $settings.$field.glob())?;
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | nested) => {
        write!($fmt, "{}", $settings.$field)?;
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | optional) => {
        {
            write!($fmt, "{}{} = ", $prefix, stringify!($field))?;
            match &$settings.$field {
                Some(value) => writeln!($fmt, "{}", value)?,
                None        => writeln!($fmt, "none")?
            };
        }
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | array) => {
        {
            write!($fmt, "{}{} = ", $prefix, stringify!($field))?;
            if $settings.$field.is_empty() {
                writeln!($fmt, "[]")?;
            } else {
                writeln!($fmt, "[")?;
                for elem in &$settings.$field {
                    writeln!($fmt, "\t{elem},")?;
                }
                writeln!($fmt, "]")?;
            }
        }
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | map) => {
        {
            use itertools::Itertools;

            write!($fmt, "{}{} = ", $prefix, stringify!($field))?;
            if $settings.$field.is_empty() {
                writeln!($fmt, "{{}}")?;
            } else {
                writeln!($fmt, "{{")?;
                for (key, value) in $settings.$field.iter().sorted_by(|(left, _), (right, _)| left.cmp(right)) {
                    writeln!($fmt, "\t{key} = {value},")?;
                }
                writeln!($fmt, "}}")?;
            }
        }
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | set) => {
        {
            use itertools::Itertools;

            write!($fmt, "{}{} = ", $prefix, stringify!($field))?;
            if $settings.$field.is_empty() {
                writeln!($fmt, "[]")?;
            } else {
                writeln!($fmt, "[")?;
                for elem in $settings.$field.iter().sorted_by(|left, right| left.cmp(right)) {
                    writeln!($fmt, "\t{elem},")?;
                }
                writeln!($fmt, "]")?;
            }
        }
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident | paths) => {
        {
            write!($fmt, "{}{} = ", $prefix, stringify!($field))?;
            if $settings.$field.is_empty() {
                writeln!($fmt, "[]")?;
            } else {
                writeln!($fmt, "[")?;
                for elem in &$settings.$field {
                    writeln!($fmt, "\t\"{}\",", elem.display())?;
                }
                writeln!($fmt, "]")?;
            }
        }
    };
    (@field $fmt:ident, $prefix:ident, $settings:ident.$field:ident) => {
        writeln!($fmt, "{}{} = {}", $prefix, stringify!($field), $settings.$field)?;
    };
}

#[derive(Debug, Clone, CacheKey)]
#[expect(clippy::struct_excessive_bools)]
pub struct LinterSettings {
    pub exclude: FilePatternSet,
    pub extension: ExtensionMapping,
    pub project_root: PathBuf,

    pub rules: RuleTable,
    pub per_file_ignores: CompiledPerFileIgnoreList,
    pub fix_safety: FixSafetyTable,

    /// The non-path-resolved Python version specified by the `target-version` input option.
    ///
    /// If you have a `Checker` available, see its `target_version` method instead.
    ///
    /// Otherwise, see [`LinterSettings::resolve_target_version`] for a way to obtain the Python
    /// version for a given file, while respecting the overrides in `per_file_target_version`.
    pub unresolved_target_version: TargetVersion,
    /// Path-specific overrides to `unresolved_target_version`.
    ///
    /// If you have a `Checker` available, see its `target_version` method instead.
    ///
    /// Otherwise, see [`LinterSettings::resolve_target_version`] for a way to check a given
    /// [`Path`] against these patterns, while falling back to `unresolved_target_version` if none
    /// of them match.
    pub per_file_target_version: CompiledPerFileTargetVersionList,
    pub preview: PreviewMode,
    pub explicit_preview_rules: bool,

    // Rule-specific settings
    pub allowed_confusables: FxHashSet<char>,
    pub builtins: Vec<String>,
    pub dummy_variable_rgx: Regex,
    pub external: Vec<String>,
    pub ignore_init_module_imports: bool,
    pub logger_objects: Vec<String>,
    pub namespace_packages: Vec<PathBuf>,
    pub src: Vec<PathBuf>,
    pub tab_size: IndentWidth,
    pub line_length: LineLength,
    pub task_tags: Vec<String>,
    pub typing_modules: Vec<String>,
    pub typing_extensions: bool,
    pub future_annotations: bool,

    // Plugins
    pub flake8_annotations: flake8_annotations::settings::Settings,
    pub flake8_bandit: flake8_bandit::settings::Settings,
    pub flake8_boolean_trap: flake8_boolean_trap::settings::Settings,
    pub flake8_bugbear: flake8_bugbear::settings::Settings,
    pub flake8_builtins: flake8_builtins::settings::Settings,
    pub flake8_comprehensions: flake8_comprehensions::settings::Settings,
    pub flake8_copyright: flake8_copyright::settings::Settings,
    pub flake8_errmsg: flake8_errmsg::settings::Settings,
    pub flake8_gettext: flake8_gettext::settings::Settings,
    pub flake8_implicit_str_concat: flake8_implicit_str_concat::settings::Settings,
    pub flake8_import_conventions: flake8_import_conventions::settings::Settings,
    pub flake8_pytest_style: flake8_pytest_style::settings::Settings,
    pub flake8_quotes: flake8_quotes::settings::Settings,
    pub flake8_self: flake8_self::settings::Settings,
    pub flake8_tidy_imports: flake8_tidy_imports::settings::Settings,
    pub flake8_type_checking: flake8_type_checking::settings::Settings,
    pub flake8_unused_arguments: flake8_unused_arguments::settings::Settings,
    pub isort: isort::settings::Settings,
    pub mccabe: mccabe::settings::Settings,
    pub pep8_naming: pep8_naming::settings::Settings,
    pub pycodestyle: pycodestyle::settings::Settings,
    pub pydoclint: pydoclint::settings::Settings,
    pub pydocstyle: pydocstyle::settings::Settings,
    pub pyflakes: pyflakes::settings::Settings,
    pub pylint: pylint::settings::Settings,
    pub pyupgrade: pyupgrade::settings::Settings,
    pub ruff: ruff::settings::Settings,
}

impl Display for LinterSettings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n# Linter Settings")?;
        display_settings! {
            formatter = f,
            namespace = "linter",
            fields = [
                self.exclude,
                self.project_root | path,

                self.rules | nested,
                self.per_file_ignores,
                self.fix_safety | nested,

                self.unresolved_target_version,
                self.per_file_target_version,
                self.preview,
                self.explicit_preview_rules,
                self.extension | debug,

                self.allowed_confusables | array,
                self.builtins | array,
                self.dummy_variable_rgx,
                self.external | array,
                self.ignore_init_module_imports,
                self.logger_objects | array,
                self.namespace_packages | debug,
                self.src | paths,
                self.tab_size,
                self.line_length,
                self.task_tags | array,
                self.typing_modules | array,
                self.typing_extensions,
            ]
        }
        writeln!(f, "\n# Linter Plugins")?;
        display_settings! {
            formatter = f,
            namespace = "linter",
            fields = [
                self.flake8_annotations | nested,
                self.flake8_bandit | nested,
                self.flake8_bugbear | nested,
                self.flake8_builtins | nested,
                self.flake8_comprehensions | nested,
                self.flake8_copyright | nested,
                self.flake8_errmsg | nested,
                self.flake8_gettext | nested,
                self.flake8_implicit_str_concat | nested,
                self.flake8_import_conventions | nested,
                self.flake8_pytest_style | nested,
                self.flake8_quotes | nested,
                self.flake8_self | nested,
                self.flake8_tidy_imports | nested,
                self.flake8_type_checking | nested,
                self.flake8_unused_arguments | nested,
                self.isort | nested,
                self.mccabe | nested,
                self.pep8_naming | nested,
                self.pycodestyle | nested,
                self.pyflakes | nested,
                self.pylint | nested,
                self.pyupgrade | nested,
                self.ruff | nested,
            ]
        }
        Ok(())
    }
}

pub const DEFAULT_SELECTORS: &[RuleSelector] = &[
    RuleSelector::Linter(Linter::Pyflakes),
    // Only include pycodestyle rules that do not overlap with the formatter
    RuleSelector::Prefix {
        prefix: RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E4),
        redirected_from: None,
    },
    RuleSelector::Prefix {
        prefix: RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E7),
        redirected_from: None,
    },
    RuleSelector::Prefix {
        prefix: RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E9),
        redirected_from: None,
    },
];

#[rustfmt::skip]
pub const PREVIEW_DEFAULT_SELECTORS: &[RuleSelector] = &[
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_100)), // ASYNC100
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_105)), // ASYNC105
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_115)), // ASYNC115
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_116)), // ASYNC116
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_210)), // ASYNC210
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_220)), // ASYNC220
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_221)), // ASYNC221
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_222)), // ASYNC222
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_230)), // ASYNC230
    RuleSelector::rule(RuleCodePrefix::Flake8Async(codes::Flake8Async::_251)), // ASYNC251
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_002)), // B002
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_003)), // B003
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_004)), // B004
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_005)), // B005
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_006)), // B006
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_008)), // B008
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_009)), // B009
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_010)), // B010
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_012)), // B012
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_013)), // B013
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_014)), // B014
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_015)), // B015
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_016)), // B016
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_017)), // B017
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_018)), // B018
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_019)), // B019
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_020)), // B020
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_021)), // B021
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_022)), // B022
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_023)), // B023
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_025)), // B025
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_026)), // B026
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_029)), // B029
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_030)), // B030
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_031)), // B031
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_032)), // B032
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_033)), // B033
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_035)), // B035
    RuleSelector::rule(RuleCodePrefix::Flake8Bugbear(codes::Flake8Bugbear::_039)), // B039
    RuleSelector::rule(RuleCodePrefix::Flake8BlindExcept(codes::Flake8BlindExcept::_001)), // BLE001
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_00)), // C400
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_01)), // C401
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_02)), // C402
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_03)), // C403
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_04)), // C404
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_05)), // C405
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_06)), // C406
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_08)), // C408
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_09)), // C409
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_10)), // C410
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_11)), // C411
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_13)), // C413
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_14)), // C414
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_15)), // C415
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_17)), // C417
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_18)), // C418
    RuleSelector::rule(RuleCodePrefix::Flake8Comprehensions(codes::Flake8Comprehensions::_19)), // C419
    RuleSelector::rule(RuleCodePrefix::Pydocstyle(codes::Pydocstyle::_419)), // D419
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_001)), // DTZ001
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_002)), // DTZ002
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_003)), // DTZ003
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_004)), // DTZ004
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_005)), // DTZ005
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_006)), // DTZ006
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_007)), // DTZ007
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_011)), // DTZ011
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_012)), // DTZ012
    RuleSelector::rule(RuleCodePrefix::Flake8Datetimez(codes::Flake8Datetimez::_901)), // DTZ901
    RuleSelector::rule(RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E722)), // E722
    RuleSelector::rule(RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E902)), // E902
    RuleSelector::rule(RuleCodePrefix::Flake8Executable(codes::Flake8Executable::_001)), // EXE001
    RuleSelector::rule(RuleCodePrefix::Flake8Executable(codes::Flake8Executable::_002)), // EXE002
    RuleSelector::rule(RuleCodePrefix::Flake8Executable(codes::Flake8Executable::_004)), // EXE004
    RuleSelector::rule(RuleCodePrefix::Flake8Executable(codes::Flake8Executable::_005)), // EXE005
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_401)), // F401
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_402)), // F402
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_404)), // F404
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_407)), // F407
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_501)), // F501
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_502)), // F502
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_503)), // F503
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_504)), // F504
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_505)), // F505
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_506)), // F506
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_507)), // F507
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_508)), // F508
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_509)), // F509
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_521)), // F521
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_522)), // F522
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_523)), // F523
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_524)), // F524
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_525)), // F525
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_541)), // F541
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_601)), // F601
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_602)), // F602
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_621)), // F621
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_622)), // F622
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_631)), // F631
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_632)), // F632
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_633)), // F633
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_634)), // F634
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_701)), // F701
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_702)), // F702
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_704)), // F704
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_706)), // F706
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_707)), // F707
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_811)), // F811
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_821)), // F821
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_822)), // F822
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_823)), // F823
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_841)), // F841
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_842)), // F842
    RuleSelector::rule(RuleCodePrefix::Pyflakes(codes::Pyflakes::_901)), // F901
    RuleSelector::rule(RuleCodePrefix::Flake8FutureAnnotations(codes::Flake8FutureAnnotations::_100)), // FA100
    RuleSelector::rule(RuleCodePrefix::Flake8FutureAnnotations(codes::Flake8FutureAnnotations::_102)), // FA102
    RuleSelector::rule(RuleCodePrefix::Flynt(codes::Flynt::_002)), // FLY002
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_105)), // FURB105
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_122)), // FURB122
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_129)), // FURB129
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_132)), // FURB132
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_136)), // FURB136
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_157)), // FURB157
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_161)), // FURB161
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_162)), // FURB162
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_163)), // FURB163
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_166)), // FURB166
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_167)), // FURB167
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_168)), // FURB168
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_169)), // FURB169
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_177)), // FURB177
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_181)), // FURB181
    RuleSelector::rule(RuleCodePrefix::Refurb(codes::Refurb::_188)), // FURB188
    RuleSelector::rule(RuleCodePrefix::Flake8LoggingFormat(codes::Flake8LoggingFormat::_010)), // G010
    RuleSelector::rule(RuleCodePrefix::Flake8LoggingFormat(codes::Flake8LoggingFormat::_101)), // G101
    RuleSelector::rule(RuleCodePrefix::Flake8LoggingFormat(codes::Flake8LoggingFormat::_201)), // G201
    RuleSelector::rule(RuleCodePrefix::Flake8LoggingFormat(codes::Flake8LoggingFormat::_202)), // G202
    RuleSelector::rule(RuleCodePrefix::Isort(codes::Isort::_001)), // I001
    RuleSelector::rule(RuleCodePrefix::Flake8GetText(codes::Flake8GetText::_001)), // INT001
    RuleSelector::rule(RuleCodePrefix::Flake8GetText(codes::Flake8GetText::_002)), // INT002
    RuleSelector::rule(RuleCodePrefix::Flake8GetText(codes::Flake8GetText::_003)), // INT003
    RuleSelector::rule(RuleCodePrefix::Flake8Logging(codes::Flake8Logging::_001)), // LOG001
    RuleSelector::rule(RuleCodePrefix::Flake8Logging(codes::Flake8Logging::_002)), // LOG002
    RuleSelector::rule(RuleCodePrefix::Flake8Logging(codes::Flake8Logging::_009)), // LOG009
    RuleSelector::rule(RuleCodePrefix::Flake8Logging(codes::Flake8Logging::_014)), // LOG014
    RuleSelector::rule(RuleCodePrefix::Flake8Logging(codes::Flake8Logging::_015)), // LOG015
    RuleSelector::rule(RuleCodePrefix::PEP8Naming(codes::PEP8Naming::_999)), // N999
    RuleSelector::rule(RuleCodePrefix::Perflint(codes::Perflint::_101)), // PERF101
    RuleSelector::rule(RuleCodePrefix::Perflint(codes::Perflint::_102)), // PERF102
    RuleSelector::rule(RuleCodePrefix::Perflint(codes::Perflint::_401)), // PERF401
    RuleSelector::rule(RuleCodePrefix::Perflint(codes::Perflint::_402)), // PERF402
    RuleSelector::rule(RuleCodePrefix::Perflint(codes::Perflint::_403)), // PERF403
    RuleSelector::rule(RuleCodePrefix::PygrepHooks(codes::PygrepHooks::_005)), // PGH005
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_790)), // PIE790
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_794)), // PIE794
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_796)), // PIE796
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_800)), // PIE800
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_804)), // PIE804
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_807)), // PIE807
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_808)), // PIE808
    RuleSelector::rule(RuleCodePrefix::Flake8Pie(codes::Flake8Pie::_810)), // PIE810
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0105)), // PLC0105
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0131)), // PLC0131
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0132)), // PLC0132
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0205)), // PLC0205
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0206)), // PLC0206
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0208)), // PLC0208
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C0414)), // PLC0414
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::C3002)), // PLC3002
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0100)), // PLE0100
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0101)), // PLE0101
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0115)), // PLE0115
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0116)), // PLE0116
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0117)), // PLE0117
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0118)), // PLE0118
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0303)), // PLE0303
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0305)), // PLE0305
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0307)), // PLE0307
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0308)), // PLE0308
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0309)), // PLE0309
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0604)), // PLE0604
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0605)), // PLE0605
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0643)), // PLE0643
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E0704)), // PLE0704
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1132)), // PLE1132
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1142)), // PLE1142
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1205)), // PLE1205
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1206)), // PLE1206
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1300)), // PLE1300
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1307)), // PLE1307
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1310)), // PLE1310
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1507)), // PLE1507
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1519)), // PLE1519
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1520)), // PLE1520
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E1700)), // PLE1700
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E2502)), // PLE2502
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E2510)), // PLE2510
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E2512)), // PLE2512
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E2513)), // PLE2513
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E2514)), // PLE2514
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::E2515)), // PLE2515
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R0124)), // PLR0124
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R0133)), // PLR0133
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R0206)), // PLR0206
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R0402)), // PLR0402
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1704)), // PLR1704
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1711)), // PLR1711
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1714)), // PLR1714
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1716)), // PLR1716
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1722)), // PLR1722
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1730)), // PLR1730
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1733)), // PLR1733
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R1736)), // PLR1736
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::R2044)), // PLR2044
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0120)), // PLW0120
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0127)), // PLW0127
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0128)), // PLW0128
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0129)), // PLW0129
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0131)), // PLW0131
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0133)), // PLW0133
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0177)), // PLW0177
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0211)), // PLW0211
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0245)), // PLW0245
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0406)), // PLW0406
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0602)), // PLW0602
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0604)), // PLW0604
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0642)), // PLW0642
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W0711)), // PLW0711
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W1501)), // PLW1501
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W1507)), // PLW1507
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W1508)), // PLW1508
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W1509)), // PLW1509
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W1510)), // PLW1510
    RuleSelector::rule(RuleCodePrefix::Pylint(codes::Pylint::W2101)), // PLW2101
    RuleSelector::rule(RuleCodePrefix::Flake8PytestStyle(codes::Flake8PytestStyle::_010)), // PT010
    RuleSelector::rule(RuleCodePrefix::Flake8PytestStyle(codes::Flake8PytestStyle::_014)), // PT014
    RuleSelector::rule(RuleCodePrefix::Flake8PytestStyle(codes::Flake8PytestStyle::_020)), // PT020
    RuleSelector::rule(RuleCodePrefix::Flake8PytestStyle(codes::Flake8PytestStyle::_025)), // PT025
    RuleSelector::rule(RuleCodePrefix::Flake8PytestStyle(codes::Flake8PytestStyle::_026)), // PT026
    RuleSelector::rule(RuleCodePrefix::Flake8PytestStyle(codes::Flake8PytestStyle::_031)), // PT031
    RuleSelector::rule(RuleCodePrefix::Flake8UsePathlib(codes::Flake8UsePathlib::_124)), // PTH124
    RuleSelector::rule(RuleCodePrefix::Flake8UsePathlib(codes::Flake8UsePathlib::_210)), // PTH210
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_001)), // PYI001
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_002)), // PYI002
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_003)), // PYI003
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_004)), // PYI004
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_005)), // PYI005
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_006)), // PYI006
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_007)), // PYI007
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_008)), // PYI008
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_009)), // PYI009
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_010)), // PYI010
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_012)), // PYI012
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_013)), // PYI013
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_015)), // PYI015
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_016)), // PYI016
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_017)), // PYI017
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_018)), // PYI018
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_019)), // PYI019
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_020)), // PYI020
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_025)), // PYI025
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_026)), // PYI026
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_029)), // PYI029
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_030)), // PYI030
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_032)), // PYI032
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_033)), // PYI033
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_034)), // PYI034
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_035)), // PYI035
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_036)), // PYI036
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_041)), // PYI041
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_042)), // PYI042
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_043)), // PYI043
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_044)), // PYI044
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_045)), // PYI045
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_046)), // PYI046
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_047)), // PYI047
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_048)), // PYI048
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_049)), // PYI049
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_050)), // PYI050
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_052)), // PYI052
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_055)), // PYI055
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_057)), // PYI057
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_058)), // PYI058
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_059)), // PYI059
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_061)), // PYI061
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_062)), // PYI062
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_063)), // PYI063
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_064)), // PYI064
    RuleSelector::rule(RuleCodePrefix::Flake8Pyi(codes::Flake8Pyi::_066)), // PYI066
    RuleSelector::rule(RuleCodePrefix::Flake8Return(codes::Flake8Return::_501)), // RET501
    RuleSelector::rule(RuleCodePrefix::Flake8Return(codes::Flake8Return::_504)), // RET504
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_007)), // RUF007
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_008)), // RUF008
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_009)), // RUF009
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_010)), // RUF010
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_012)), // RUF012
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_013)), // RUF013
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_015)), // RUF015
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_016)), // RUF016
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_017)), // RUF017
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_018)), // RUF018
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_019)), // RUF019
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_020)), // RUF020
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_022)), // RUF022
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_023)), // RUF023
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_024)), // RUF024
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_026)), // RUF026
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_028)), // RUF028
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_030)), // RUF030
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_032)), // RUF032
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_033)), // RUF033
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_034)), // RUF034
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_040)), // RUF040
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_041)), // RUF041
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_046)), // RUF046
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_048)), // RUF048
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_049)), // RUF049
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_051)), // RUF051
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_053)), // RUF053
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_057)), // RUF057
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_058)), // RUF058
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_059)), // RUF059
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_100)), // RUF100
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_101)), // RUF101
    RuleSelector::rule(RuleCodePrefix::Ruff(codes::Ruff::_200)), // RUF200
    RuleSelector::rule(RuleCodePrefix::Flake8Bandit(codes::Flake8Bandit::_102)), // S102
    RuleSelector::rule(RuleCodePrefix::Flake8Bandit(codes::Flake8Bandit::_110)), // S110
    RuleSelector::rule(RuleCodePrefix::Flake8Bandit(codes::Flake8Bandit::_112)), // S112
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_101)), // SIM101
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_102)), // SIM102
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_103)), // SIM103
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_107)), // SIM107
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_113)), // SIM113
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_114)), // SIM114
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_115)), // SIM115
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_117)), // SIM117
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_118)), // SIM118
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_201)), // SIM201
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_202)), // SIM202
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_208)), // SIM208
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_210)), // SIM210
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_211)), // SIM211
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_220)), // SIM220
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_221)), // SIM221
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_222)), // SIM222
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_223)), // SIM223
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_401)), // SIM401
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_905)), // SIM905
    RuleSelector::rule(RuleCodePrefix::Flake8Simplify(codes::Flake8Simplify::_911)), // SIM911
    RuleSelector::rule(RuleCodePrefix::Flake8Debugger(codes::Flake8Debugger::_0)), // T100
    RuleSelector::rule(RuleCodePrefix::Flake8TypeChecking(codes::Flake8TypeChecking::_004)), // TC004
    RuleSelector::rule(RuleCodePrefix::Flake8TypeChecking(codes::Flake8TypeChecking::_005)), // TC005
    RuleSelector::rule(RuleCodePrefix::Flake8TypeChecking(codes::Flake8TypeChecking::_007)), // TC007
    RuleSelector::rule(RuleCodePrefix::Flake8TypeChecking(codes::Flake8TypeChecking::_010)), // TC010
    RuleSelector::rule(RuleCodePrefix::Tryceratops(codes::Tryceratops::_002)), // TRY002
    RuleSelector::rule(RuleCodePrefix::Tryceratops(codes::Tryceratops::_004)), // TRY004
    RuleSelector::rule(RuleCodePrefix::Tryceratops(codes::Tryceratops::_201)), // TRY201
    RuleSelector::rule(RuleCodePrefix::Tryceratops(codes::Tryceratops::_203)), // TRY203
    RuleSelector::rule(RuleCodePrefix::Tryceratops(codes::Tryceratops::_300)), // TRY300
    RuleSelector::rule(RuleCodePrefix::Tryceratops(codes::Tryceratops::_401)), // TRY401
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_001)), // UP001
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_003)), // UP003
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_004)), // UP004
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_005)), // UP005
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_006)), // UP006
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_007)), // UP007
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_008)), // UP008
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_009)), // UP009
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_010)), // UP010
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_011)), // UP011
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_012)), // UP012
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_014)), // UP014
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_017)), // UP017
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_018)), // UP018
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_019)), // UP019
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_020)), // UP020
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_021)), // UP021
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_022)), // UP022
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_023)), // UP023
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_024)), // UP024
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_025)), // UP025
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_026)), // UP026
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_028)), // UP028
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_029)), // UP029
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_030)), // UP030
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_031)), // UP031
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_032)), // UP032
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_033)), // UP033
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_034)), // UP034
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_035)), // UP035
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_036)), // UP036
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_037)), // UP037
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_039)), // UP039
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_040)), // UP040
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_041)), // UP041
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_043)), // UP043
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_044)), // UP044
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_045)), // UP045
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_046)), // UP046
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_047)), // UP047
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_049)), // UP049
    RuleSelector::rule(RuleCodePrefix::Pyupgrade(codes::Pyupgrade::_050)), // UP050
    RuleSelector::rule(RuleCodePrefix::Pycodestyle(codes::Pycodestyle::W605)), // W605
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_101)), // YTT101
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_102)), // YTT102
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_103)), // YTT103
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_201)), // YTT201
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_202)), // YTT202
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_203)), // YTT203
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_204)), // YTT204
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_301)), // YTT301
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_302)), // YTT302
    RuleSelector::rule(RuleCodePrefix::Flake82020(codes::Flake82020::_303)), // YTT303
];

pub const TASK_TAGS: &[&str] = &["TODO", "FIXME", "XXX"];

pub static DUMMY_VARIABLE_RGX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

impl LinterSettings {
    pub fn for_rule(rule_code: Rule) -> Self {
        Self {
            rules: RuleTable::from_iter([rule_code]),
            unresolved_target_version: PythonVersion::latest().into(),
            ..Self::default()
        }
    }

    pub fn for_rules(rules: impl IntoIterator<Item = Rule>) -> Self {
        Self {
            rules: RuleTable::from_iter(rules),
            unresolved_target_version: PythonVersion::latest().into(),
            ..Self::default()
        }
    }

    pub fn new(project_root: &Path) -> Self {
        Self {
            exclude: FilePatternSet::default(),
            unresolved_target_version: TargetVersion(None),
            per_file_target_version: CompiledPerFileTargetVersionList::default(),
            project_root: project_root.to_path_buf(),
            rules: DEFAULT_SELECTORS
                .iter()
                .flat_map(|selector| selector.rules(&PreviewOptions::default()))
                .collect(),
            allowed_confusables: FxHashSet::from_iter([]),

            // Needs duplicating
            builtins: vec![],
            dummy_variable_rgx: DUMMY_VARIABLE_RGX.clone(),

            external: vec![],
            ignore_init_module_imports: true,
            logger_objects: vec![],
            namespace_packages: vec![],

            per_file_ignores: CompiledPerFileIgnoreList::default(),
            fix_safety: FixSafetyTable::default(),

            src: vec![fs::get_cwd().to_path_buf(), fs::get_cwd().join("src")],
            // Needs duplicating
            tab_size: IndentWidth::default(),
            line_length: LineLength::default(),

            task_tags: TASK_TAGS.iter().map(ToString::to_string).collect(),
            typing_modules: vec![],
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_boolean_trap: flake8_boolean_trap::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_builtins: flake8_builtins::settings::Settings::default(),
            flake8_comprehensions: flake8_comprehensions::settings::Settings::default(),
            flake8_copyright: flake8_copyright::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_gettext: flake8_gettext::settings::Settings::default(),
            flake8_implicit_str_concat: flake8_implicit_str_concat::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_self: flake8_self::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            flake8_type_checking: flake8_type_checking::settings::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydoclint: pydoclint::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pyflakes: pyflakes::settings::Settings::default(),
            pylint: pylint::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
            ruff: ruff::settings::Settings::default(),
            preview: PreviewMode::default(),
            explicit_preview_rules: false,
            extension: ExtensionMapping::default(),
            typing_extensions: true,
            future_annotations: false,
        }
    }

    #[must_use]
    pub fn with_target_version(mut self, target_version: PythonVersion) -> Self {
        self.unresolved_target_version = target_version.into();
        self
    }

    #[must_use]
    pub fn with_preview_mode(mut self) -> Self {
        self.preview = PreviewMode::Enabled;
        self
    }

    #[must_use]
    pub fn with_external_rules(mut self, rules: &[&str]) -> Self {
        self.external
            .extend(rules.iter().map(std::string::ToString::to_string));
        self
    }

    /// Resolve the [`TargetVersion`] to use for linting.
    ///
    /// This method respects the per-file version overrides in
    /// [`LinterSettings::per_file_target_version`] and falls back on
    /// [`LinterSettings::unresolved_target_version`] if none of the override patterns match.
    pub fn resolve_target_version(&self, path: &Path) -> TargetVersion {
        self.per_file_target_version
            .is_match(path)
            .map_or(self.unresolved_target_version, TargetVersion::from)
    }
}

impl Default for LinterSettings {
    fn default() -> Self {
        Self::new(fs::get_cwd())
    }
}

/// A thin wrapper around `Option<PythonVersion>` to clarify the reason for different `unwrap`
/// calls in various places.
///
/// For example, we want to default to `PythonVersion::latest()` for parsing and detecting semantic
/// syntax errors because this will minimize version-related diagnostics when the Python version is
/// unset. In contrast, we want to default to `PythonVersion::default()` for lint rules. These
/// correspond to the [`TargetVersion::parser_version`] and [`TargetVersion::linter_version`]
/// methods, respectively.
#[derive(Debug, Clone, Copy, CacheKey, PartialEq, Eq)]
pub struct TargetVersion(pub Option<PythonVersion>);

impl TargetVersion {
    /// Return the [`PythonVersion`] to use for parsing.
    ///
    /// This will be either the Python version specified by the user or the latest supported
    /// version if unset.
    pub fn parser_version(&self) -> PythonVersion {
        self.0.unwrap_or_else(PythonVersion::latest)
    }

    /// Return the [`PythonVersion`] to use for version-dependent lint rules.
    ///
    /// This will either be the Python version specified by the user or the default Python version
    /// if unset.
    pub fn linter_version(&self) -> PythonVersion {
        self.0.unwrap_or_default()
    }
}

impl From<PythonVersion> for TargetVersion {
    fn from(value: PythonVersion) -> Self {
        Self(Some(value))
    }
}

impl Display for TargetVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // manual inlining of display_settings!
        match self.0 {
            Some(value) => write!(f, "{value}"),
            None => f.write_str("none"),
        }
    }
}
