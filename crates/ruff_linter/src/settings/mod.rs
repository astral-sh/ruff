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
    RuleSelector::rule(Rule::CancelScopeNoCheckpoint), // ASYNC100
    RuleSelector::rule(Rule::TrioSyncCall), // ASYNC105
    RuleSelector::rule(Rule::AsyncZeroSleep), // ASYNC115
    RuleSelector::rule(Rule::LongSleepNotForever), // ASYNC116
    RuleSelector::rule(Rule::BlockingHttpCallInAsyncFunction), // ASYNC210
    RuleSelector::rule(Rule::CreateSubprocessInAsyncFunction), // ASYNC220
    RuleSelector::rule(Rule::RunProcessInAsyncFunction), // ASYNC221
    RuleSelector::rule(Rule::WaitForProcessInAsyncFunction), // ASYNC222
    RuleSelector::rule(Rule::BlockingOpenCallInAsyncFunction), // ASYNC230
    RuleSelector::rule(Rule::BlockingSleepInAsyncFunction), // ASYNC251
    RuleSelector::rule(Rule::UnaryPrefixIncrementDecrement), // B002
    RuleSelector::rule(Rule::AssignmentToOsEnviron), // B003
    RuleSelector::rule(Rule::UnreliableCallableCheck), // B004
    RuleSelector::rule(Rule::StripWithMultiCharacters), // B005
    RuleSelector::rule(Rule::MutableArgumentDefault), // B006
    RuleSelector::rule(Rule::FunctionCallInDefaultArgument), // B008
    RuleSelector::rule(Rule::GetAttrWithConstant), // B009
    RuleSelector::rule(Rule::SetAttrWithConstant), // B010
    RuleSelector::rule(Rule::JumpStatementInFinally), // B012
    RuleSelector::rule(Rule::RedundantTupleInExceptionHandler), // B013
    RuleSelector::rule(Rule::DuplicateHandlerException), // B014
    RuleSelector::rule(Rule::UselessComparison), // B015
    RuleSelector::rule(Rule::RaiseLiteral), // B016
    RuleSelector::rule(Rule::AssertRaisesException), // B017
    RuleSelector::rule(Rule::UselessExpression), // B018
    RuleSelector::rule(Rule::CachedInstanceMethod), // B019
    RuleSelector::rule(Rule::LoopVariableOverridesIterator), // B020
    RuleSelector::rule(Rule::FStringDocstring), // B021
    RuleSelector::rule(Rule::UselessContextlibSuppress), // B022
    RuleSelector::rule(Rule::FunctionUsesLoopVariable), // B023
    RuleSelector::rule(Rule::DuplicateTryBlockException), // B025
    RuleSelector::rule(Rule::StarArgUnpackingAfterKeywordArg), // B026
    RuleSelector::rule(Rule::ExceptWithEmptyTuple), // B029
    RuleSelector::rule(Rule::ExceptWithNonExceptionClasses), // B030
    RuleSelector::rule(Rule::ReuseOfGroupbyGenerator), // B031
    RuleSelector::rule(Rule::UnintentionalTypeAnnotation), // B032
    RuleSelector::rule(Rule::DuplicateValue), // B033
    RuleSelector::rule(Rule::StaticKeyDictComprehension), // B035
    RuleSelector::rule(Rule::MutableContextvarDefault), // B039
    RuleSelector::rule(Rule::BlindExcept), // BLE001
    RuleSelector::rule(Rule::UnnecessaryGeneratorList), // C400
    RuleSelector::rule(Rule::UnnecessaryGeneratorSet), // C401
    RuleSelector::rule(Rule::UnnecessaryGeneratorDict), // C402
    RuleSelector::rule(Rule::UnnecessaryListComprehensionSet), // C403
    RuleSelector::rule(Rule::UnnecessaryListComprehensionDict), // C404
    RuleSelector::rule(Rule::UnnecessaryLiteralSet), // C405
    RuleSelector::rule(Rule::UnnecessaryLiteralDict), // C406
    RuleSelector::rule(Rule::UnnecessaryCollectionCall), // C408
    RuleSelector::rule(Rule::UnnecessaryLiteralWithinTupleCall), // C409
    RuleSelector::rule(Rule::UnnecessaryLiteralWithinListCall), // C410
    RuleSelector::rule(Rule::UnnecessaryListCall), // C411
    RuleSelector::rule(Rule::UnnecessaryCallAroundSorted), // C413
    RuleSelector::rule(Rule::UnnecessaryDoubleCastOrProcess), // C414
    RuleSelector::rule(Rule::UnnecessarySubscriptReversal), // C415
    RuleSelector::rule(Rule::UnnecessaryMap), // C417
    RuleSelector::rule(Rule::UnnecessaryLiteralWithinDictCall), // C418
    RuleSelector::rule(Rule::UnnecessaryComprehensionInCall), // C419
    RuleSelector::rule(Rule::EmptyDocstring), // D419
    RuleSelector::rule(Rule::CallDatetimeWithoutTzinfo), // DTZ001
    RuleSelector::rule(Rule::CallDatetimeToday), // DTZ002
    RuleSelector::rule(Rule::CallDatetimeUtcnow), // DTZ003
    RuleSelector::rule(Rule::CallDatetimeUtcfromtimestamp), // DTZ004
    RuleSelector::rule(Rule::CallDatetimeNowWithoutTzinfo), // DTZ005
    RuleSelector::rule(Rule::CallDatetimeFromtimestamp), // DTZ006
    RuleSelector::rule(Rule::CallDatetimeStrptimeWithoutZone), // DTZ007
    RuleSelector::rule(Rule::CallDateToday), // DTZ011
    RuleSelector::rule(Rule::CallDateFromtimestamp), // DTZ012
    RuleSelector::rule(Rule::DatetimeMinMax), // DTZ901
    RuleSelector::rule(Rule::BareExcept), // E722
    RuleSelector::rule(Rule::IOError), // E902
    RuleSelector::rule(Rule::ShebangNotExecutable), // EXE001
    RuleSelector::rule(Rule::ShebangMissingExecutableFile), // EXE002
    RuleSelector::rule(Rule::ShebangLeadingWhitespace), // EXE004
    RuleSelector::rule(Rule::ShebangNotFirstLine), // EXE005
    RuleSelector::rule(Rule::UnusedImport), // F401
    RuleSelector::rule(Rule::ImportShadowedByLoopVar), // F402
    RuleSelector::rule(Rule::LateFutureImport), // F404
    RuleSelector::rule(Rule::FutureFeatureNotDefined), // F407
    RuleSelector::rule(Rule::PercentFormatInvalidFormat), // F501
    RuleSelector::rule(Rule::PercentFormatExpectedMapping), // F502
    RuleSelector::rule(Rule::PercentFormatExpectedSequence), // F503
    RuleSelector::rule(Rule::PercentFormatExtraNamedArguments), // F504
    RuleSelector::rule(Rule::PercentFormatMissingArgument), // F505
    RuleSelector::rule(Rule::PercentFormatMixedPositionalAndNamed), // F506
    RuleSelector::rule(Rule::PercentFormatPositionalCountMismatch), // F507
    RuleSelector::rule(Rule::PercentFormatStarRequiresSequence), // F508
    RuleSelector::rule(Rule::PercentFormatUnsupportedFormatCharacter), // F509
    RuleSelector::rule(Rule::StringDotFormatInvalidFormat), // F521
    RuleSelector::rule(Rule::StringDotFormatExtraNamedArguments), // F522
    RuleSelector::rule(Rule::StringDotFormatExtraPositionalArguments), // F523
    RuleSelector::rule(Rule::StringDotFormatMissingArguments), // F524
    RuleSelector::rule(Rule::StringDotFormatMixingAutomatic), // F525
    RuleSelector::rule(Rule::FStringMissingPlaceholders), // F541
    RuleSelector::rule(Rule::MultiValueRepeatedKeyLiteral), // F601
    RuleSelector::rule(Rule::MultiValueRepeatedKeyVariable), // F602
    RuleSelector::rule(Rule::ExpressionsInStarAssignment), // F621
    RuleSelector::rule(Rule::MultipleStarredExpressions), // F622
    RuleSelector::rule(Rule::AssertTuple), // F631
    RuleSelector::rule(Rule::IsLiteral), // F632
    RuleSelector::rule(Rule::InvalidPrintSyntax), // F633
    RuleSelector::rule(Rule::IfTuple), // F634
    RuleSelector::rule(Rule::BreakOutsideLoop), // F701
    RuleSelector::rule(Rule::ContinueOutsideLoop), // F702
    RuleSelector::rule(Rule::YieldOutsideFunction), // F704
    RuleSelector::rule(Rule::ReturnOutsideFunction), // F706
    RuleSelector::rule(Rule::DefaultExceptNotLast), // F707
    RuleSelector::rule(Rule::RedefinedWhileUnused), // F811
    RuleSelector::rule(Rule::UndefinedName), // F821
    RuleSelector::rule(Rule::UndefinedExport), // F822
    RuleSelector::rule(Rule::UndefinedLocal), // F823
    RuleSelector::rule(Rule::UnusedVariable), // F841
    RuleSelector::rule(Rule::UnusedAnnotation), // F842
    RuleSelector::rule(Rule::RaiseNotImplemented), // F901
    RuleSelector::rule(Rule::FutureRewritableTypeAnnotation), // FA100
    RuleSelector::rule(Rule::FutureRequiredTypeAnnotation), // FA102
    RuleSelector::rule(Rule::StaticJoinToFString), // FLY002
    RuleSelector::rule(Rule::PrintEmptyString), // FURB105
    RuleSelector::rule(Rule::ForLoopWrites), // FURB122
    RuleSelector::rule(Rule::ReadlinesInFor), // FURB129
    RuleSelector::rule(Rule::CheckAndRemoveFromSet), // FURB132
    RuleSelector::rule(Rule::IfExprMinMax), // FURB136
    RuleSelector::rule(Rule::VerboseDecimalConstructor), // FURB157
    RuleSelector::rule(Rule::BitCount), // FURB161
    RuleSelector::rule(Rule::FromisoformatReplaceZ), // FURB162
    RuleSelector::rule(Rule::RedundantLogBase), // FURB163
    RuleSelector::rule(Rule::IntOnSlicedStr), // FURB166
    RuleSelector::rule(Rule::RegexFlagAlias), // FURB167
    RuleSelector::rule(Rule::IsinstanceTypeNone), // FURB168
    RuleSelector::rule(Rule::TypeNoneComparison), // FURB169
    RuleSelector::rule(Rule::ImplicitCwd), // FURB177
    RuleSelector::rule(Rule::HashlibDigestHex), // FURB181
    RuleSelector::rule(Rule::SliceToRemovePrefixOrSuffix), // FURB188
    RuleSelector::rule(Rule::LoggingWarn), // G010
    RuleSelector::rule(Rule::LoggingExtraAttrClash), // G101
    RuleSelector::rule(Rule::LoggingExcInfo), // G201
    RuleSelector::rule(Rule::LoggingRedundantExcInfo), // G202
    RuleSelector::rule(Rule::UnsortedImports), // I001
    RuleSelector::rule(Rule::FStringInGetTextFuncCall), // INT001
    RuleSelector::rule(Rule::FormatInGetTextFuncCall), // INT002
    RuleSelector::rule(Rule::PrintfInGetTextFuncCall), // INT003
    RuleSelector::rule(Rule::DirectLoggerInstantiation), // LOG001
    RuleSelector::rule(Rule::InvalidGetLoggerArgument), // LOG002
    RuleSelector::rule(Rule::UndocumentedWarn), // LOG009
    RuleSelector::rule(Rule::ExcInfoOutsideExceptHandler), // LOG014
    RuleSelector::rule(Rule::RootLoggerCall), // LOG015
    RuleSelector::rule(Rule::InvalidModuleName), // N999
    RuleSelector::rule(Rule::UnnecessaryListCast), // PERF101
    RuleSelector::rule(Rule::IncorrectDictIterator), // PERF102
    RuleSelector::rule(Rule::ManualListCopy), // PERF402
    RuleSelector::rule(Rule::InvalidMockAccess), // PGH005
    RuleSelector::rule(Rule::UnnecessaryPlaceholder), // PIE790
    RuleSelector::rule(Rule::DuplicateClassFieldDefinition), // PIE794
    RuleSelector::rule(Rule::NonUniqueEnums), // PIE796
    RuleSelector::rule(Rule::UnnecessarySpread), // PIE800
    RuleSelector::rule(Rule::UnnecessaryDictKwargs), // PIE804
    RuleSelector::rule(Rule::ReimplementedContainerBuiltin), // PIE807
    RuleSelector::rule(Rule::UnnecessaryRangeStart), // PIE808
    RuleSelector::rule(Rule::MultipleStartsEndsWith), // PIE810
    RuleSelector::rule(Rule::TypeNameIncorrectVariance), // PLC0105
    RuleSelector::rule(Rule::TypeBivariance), // PLC0131
    RuleSelector::rule(Rule::TypeParamNameMismatch), // PLC0132
    RuleSelector::rule(Rule::SingleStringSlots), // PLC0205
    RuleSelector::rule(Rule::DictIndexMissingItems), // PLC0206
    RuleSelector::rule(Rule::IterationOverSet), // PLC0208
    RuleSelector::rule(Rule::UselessImportAlias), // PLC0414
    RuleSelector::rule(Rule::UnnecessaryDirectLambdaCall), // PLC3002
    RuleSelector::rule(Rule::YieldInInit), // PLE0100
    RuleSelector::rule(Rule::ReturnInInit), // PLE0101
    RuleSelector::rule(Rule::NonlocalAndGlobal), // PLE0115
    RuleSelector::rule(Rule::ContinueInFinally), // PLE0116
    RuleSelector::rule(Rule::NonlocalWithoutBinding), // PLE0117
    RuleSelector::rule(Rule::LoadBeforeGlobalDeclaration), // PLE0118
    RuleSelector::rule(Rule::InvalidLengthReturnType), // PLE0303
    RuleSelector::rule(Rule::InvalidIndexReturnType), // PLE0305
    RuleSelector::rule(Rule::InvalidStrReturnType), // PLE0307
    RuleSelector::rule(Rule::InvalidBytesReturnType), // PLE0308
    RuleSelector::rule(Rule::InvalidHashReturnType), // PLE0309
    RuleSelector::rule(Rule::InvalidAllObject), // PLE0604
    RuleSelector::rule(Rule::InvalidAllFormat), // PLE0605
    RuleSelector::rule(Rule::PotentialIndexError), // PLE0643
    RuleSelector::rule(Rule::MisplacedBareRaise), // PLE0704
    RuleSelector::rule(Rule::RepeatedKeywordArgument), // PLE1132
    RuleSelector::rule(Rule::AwaitOutsideAsync), // PLE1142
    RuleSelector::rule(Rule::LoggingTooManyArgs), // PLE1205
    RuleSelector::rule(Rule::LoggingTooFewArgs), // PLE1206
    RuleSelector::rule(Rule::BadStringFormatCharacter), // PLE1300
    RuleSelector::rule(Rule::BadStringFormatType), // PLE1307
    RuleSelector::rule(Rule::BadStrStripCall), // PLE1310
    RuleSelector::rule(Rule::InvalidEnvvarValue), // PLE1507
    RuleSelector::rule(Rule::SingledispatchMethod), // PLE1519
    RuleSelector::rule(Rule::SingledispatchmethodFunction), // PLE1520
    RuleSelector::rule(Rule::YieldFromInAsyncFunction), // PLE1700
    RuleSelector::rule(Rule::BidirectionalUnicode), // PLE2502
    RuleSelector::rule(Rule::InvalidCharacterBackspace), // PLE2510
    RuleSelector::rule(Rule::InvalidCharacterSub), // PLE2512
    RuleSelector::rule(Rule::InvalidCharacterEsc), // PLE2513
    RuleSelector::rule(Rule::InvalidCharacterNul), // PLE2514
    RuleSelector::rule(Rule::InvalidCharacterZeroWidthSpace), // PLE2515
    RuleSelector::rule(Rule::ComparisonWithItself), // PLR0124
    RuleSelector::rule(Rule::ComparisonOfConstant), // PLR0133
    RuleSelector::rule(Rule::PropertyWithParameters), // PLR0206
    RuleSelector::rule(Rule::ManualFromImport), // PLR0402
    RuleSelector::rule(Rule::RedefinedArgumentFromLocal), // PLR1704
    RuleSelector::rule(Rule::UselessReturn), // PLR1711
    RuleSelector::rule(Rule::BooleanChainedComparison), // PLR1716
    RuleSelector::rule(Rule::SysExitAlias), // PLR1722
    RuleSelector::rule(Rule::IfStmtMinMax), // PLR1730
    RuleSelector::rule(Rule::UnnecessaryDictIndexLookup), // PLR1733
    RuleSelector::rule(Rule::UnnecessaryListIndexLookup), // PLR1736
    RuleSelector::rule(Rule::EmptyComment), // PLR2044
    RuleSelector::rule(Rule::UselessElseOnLoop), // PLW0120
    RuleSelector::rule(Rule::SelfAssigningVariable), // PLW0127
    RuleSelector::rule(Rule::RedeclaredAssignedName), // PLW0128
    RuleSelector::rule(Rule::AssertOnStringLiteral), // PLW0129
    RuleSelector::rule(Rule::NamedExprWithoutContext), // PLW0131
    RuleSelector::rule(Rule::UselessExceptionStatement), // PLW0133
    RuleSelector::rule(Rule::NanComparison), // PLW0177
    RuleSelector::rule(Rule::BadStaticmethodArgument), // PLW0211
    RuleSelector::rule(Rule::SuperWithoutBrackets), // PLW0245
    RuleSelector::rule(Rule::ImportSelf), // PLW0406
    RuleSelector::rule(Rule::GlobalVariableNotAssigned), // PLW0602
    RuleSelector::rule(Rule::GlobalAtModuleLevel), // PLW0604
    RuleSelector::rule(Rule::SelfOrClsAssignment), // PLW0642
    RuleSelector::rule(Rule::BinaryOpException), // PLW0711
    RuleSelector::rule(Rule::BadOpenMode), // PLW1501
    RuleSelector::rule(Rule::ShallowCopyEnviron), // PLW1507
    RuleSelector::rule(Rule::InvalidEnvvarDefault), // PLW1508
    RuleSelector::rule(Rule::SubprocessPopenPreexecFn), // PLW1509
    RuleSelector::rule(Rule::SubprocessRunWithoutCheck), // PLW1510
    RuleSelector::rule(Rule::UselessWithLock), // PLW2101
    RuleSelector::rule(Rule::PytestRaisesWithoutException), // PT010
    RuleSelector::rule(Rule::PytestDuplicateParametrizeTestCases), // PT014
    RuleSelector::rule(Rule::PytestDeprecatedYieldFixture), // PT020
    RuleSelector::rule(Rule::PytestErroneousUseFixturesOnFixture), // PT025
    RuleSelector::rule(Rule::PytestUseFixturesWithoutParameters), // PT026
    RuleSelector::rule(Rule::PytestWarnsWithMultipleStatements), // PT031
    RuleSelector::rule(Rule::PyPath), // PTH124
    RuleSelector::rule(Rule::InvalidPathlibWithSuffix), // PTH210
    RuleSelector::rule(Rule::UnprefixedTypeParam), // PYI001
    RuleSelector::rule(Rule::ComplexIfStatementInStub), // PYI002
    RuleSelector::rule(Rule::UnrecognizedVersionInfoCheck), // PYI003
    RuleSelector::rule(Rule::PatchVersionComparison), // PYI004
    RuleSelector::rule(Rule::WrongTupleLengthVersionComparison), // PYI005
    RuleSelector::rule(Rule::BadVersionInfoComparison), // PYI006
    RuleSelector::rule(Rule::UnrecognizedPlatformCheck), // PYI007
    RuleSelector::rule(Rule::UnrecognizedPlatformName), // PYI008
    RuleSelector::rule(Rule::PassStatementStubBody), // PYI009
    RuleSelector::rule(Rule::NonEmptyStubBody), // PYI010
    RuleSelector::rule(Rule::PassInClassBody), // PYI012
    RuleSelector::rule(Rule::EllipsisInNonEmptyClassBody), // PYI013
    RuleSelector::rule(Rule::AssignmentDefaultInStub), // PYI015
    RuleSelector::rule(Rule::DuplicateUnionMember), // PYI016
    RuleSelector::rule(Rule::ComplexAssignmentInStub), // PYI017
    RuleSelector::rule(Rule::UnusedPrivateTypeVar), // PYI018
    RuleSelector::rule(Rule::CustomTypeVarForSelf), // PYI019
    RuleSelector::rule(Rule::QuotedAnnotationInStub), // PYI020
    RuleSelector::rule(Rule::UnaliasedCollectionsAbcSetImport), // PYI025
    RuleSelector::rule(Rule::TypeAliasWithoutAnnotation), // PYI026
    RuleSelector::rule(Rule::StrOrReprDefinedInStub), // PYI029
    RuleSelector::rule(Rule::UnnecessaryLiteralUnion), // PYI030
    RuleSelector::rule(Rule::AnyEqNeAnnotation), // PYI032
    RuleSelector::rule(Rule::LegacyTypeComment), // PYI033
    RuleSelector::rule(Rule::NonSelfReturnType), // PYI034
    RuleSelector::rule(Rule::UnassignedSpecialVariableInStub), // PYI035
    RuleSelector::rule(Rule::BadExitAnnotation), // PYI036
    RuleSelector::rule(Rule::RedundantNumericUnion), // PYI041
    RuleSelector::rule(Rule::SnakeCaseTypeAlias), // PYI042
    RuleSelector::rule(Rule::TSuffixedTypeAlias), // PYI043
    RuleSelector::rule(Rule::FutureAnnotationsInStub), // PYI044
    RuleSelector::rule(Rule::IterMethodReturnIterable), // PYI045
    RuleSelector::rule(Rule::UnusedPrivateProtocol), // PYI046
    RuleSelector::rule(Rule::UnusedPrivateTypeAlias), // PYI047
    RuleSelector::rule(Rule::StubBodyMultipleStatements), // PYI048
    RuleSelector::rule(Rule::UnusedPrivateTypedDict), // PYI049
    RuleSelector::rule(Rule::NoReturnArgumentAnnotationInStub), // PYI050
    RuleSelector::rule(Rule::UnannotatedAssignmentInStub), // PYI052
    RuleSelector::rule(Rule::UnnecessaryTypeUnion), // PYI055
    RuleSelector::rule(Rule::ByteStringUsage), // PYI057
    RuleSelector::rule(Rule::GeneratorReturnFromIterMethod), // PYI058
    RuleSelector::rule(Rule::GenericNotLastBaseClass), // PYI059
    RuleSelector::rule(Rule::RedundantNoneLiteral), // PYI061
    RuleSelector::rule(Rule::DuplicateLiteralMember), // PYI062
    RuleSelector::rule(Rule::Pep484StylePositionalOnlyParameter), // PYI063
    RuleSelector::rule(Rule::RedundantFinalLiteral), // PYI064
    RuleSelector::rule(Rule::BadVersionInfoOrder), // PYI066
    RuleSelector::rule(Rule::UnnecessaryReturnNone), // RET501
    RuleSelector::rule(Rule::ZipInsteadOfPairwise), // RUF007
    RuleSelector::rule(Rule::MutableDataclassDefault), // RUF008
    RuleSelector::rule(Rule::FunctionCallInDataclassDefaultArgument), // RUF009
    RuleSelector::rule(Rule::ExplicitFStringTypeConversion), // RUF010
    RuleSelector::rule(Rule::MutableClassDefault), // RUF012
    RuleSelector::rule(Rule::ImplicitOptional), // RUF013
    RuleSelector::rule(Rule::UnnecessaryIterableAllocationForFirstElement), // RUF015
    RuleSelector::rule(Rule::InvalidIndexType), // RUF016
    RuleSelector::rule(Rule::QuadraticListSummation), // RUF017
    RuleSelector::rule(Rule::AssignmentInAssert), // RUF018
    RuleSelector::rule(Rule::UnnecessaryKeyCheck), // RUF019
    RuleSelector::rule(Rule::NeverUnion), // RUF020
    RuleSelector::rule(Rule::UnsortedDunderAll), // RUF022
    RuleSelector::rule(Rule::UnsortedDunderSlots), // RUF023
    RuleSelector::rule(Rule::MutableFromkeysValue), // RUF024
    RuleSelector::rule(Rule::DefaultFactoryKwarg), // RUF026
    RuleSelector::rule(Rule::InvalidFormatterSuppressionComment), // RUF028
    RuleSelector::rule(Rule::AssertWithPrintMessage), // RUF030
    RuleSelector::rule(Rule::DecimalFromFloatLiteral), // RUF032
    RuleSelector::rule(Rule::PostInitDefault), // RUF033
    RuleSelector::rule(Rule::UselessIfElse), // RUF034
    RuleSelector::rule(Rule::InvalidAssertMessageLiteralArgument), // RUF040
    RuleSelector::rule(Rule::UnnecessaryNestedLiteral), // RUF041
    RuleSelector::rule(Rule::UnnecessaryCastToInt), // RUF046
    RuleSelector::rule(Rule::MapIntVersionParsing), // RUF048
    RuleSelector::rule(Rule::DataclassEnum), // RUF049
    RuleSelector::rule(Rule::IfKeyInDictDel), // RUF051
    RuleSelector::rule(Rule::ClassWithMixedTypeVars), // RUF053
    RuleSelector::rule(Rule::UnnecessaryRound), // RUF057
    RuleSelector::rule(Rule::StarmapZip), // RUF058
    RuleSelector::rule(Rule::UnusedUnpackedVariable), // RUF059
    RuleSelector::rule(Rule::UnusedNOQA), // RUF100
    RuleSelector::rule(Rule::RedirectedNOQA), // RUF101
    RuleSelector::rule(Rule::InvalidPyprojectToml), // RUF200
    RuleSelector::rule(Rule::ExecBuiltin), // S102
    RuleSelector::rule(Rule::TryExceptPass), // S110
    RuleSelector::rule(Rule::TryExceptContinue), // S112
    RuleSelector::rule(Rule::DuplicateIsinstanceCall), // SIM101
    RuleSelector::rule(Rule::CollapsibleIf), // SIM102
    RuleSelector::rule(Rule::NeedlessBool), // SIM103
    RuleSelector::rule(Rule::ReturnInTryExceptFinally), // SIM107
    RuleSelector::rule(Rule::EnumerateForLoop), // SIM113
    RuleSelector::rule(Rule::IfWithSameArms), // SIM114
    RuleSelector::rule(Rule::OpenFileWithContextHandler), // SIM115
    RuleSelector::rule(Rule::MultipleWithStatements), // SIM117
    RuleSelector::rule(Rule::InDictKeys), // SIM118
    RuleSelector::rule(Rule::NegateEqualOp), // SIM201
    RuleSelector::rule(Rule::NegateNotEqualOp), // SIM202
    RuleSelector::rule(Rule::DoubleNegation), // SIM208
    RuleSelector::rule(Rule::IfExprWithTrueFalse), // SIM210
    RuleSelector::rule(Rule::IfExprWithFalseTrue), // SIM211
    RuleSelector::rule(Rule::ExprAndNotExpr), // SIM220
    RuleSelector::rule(Rule::ExprOrNotExpr), // SIM221
    RuleSelector::rule(Rule::ExprOrTrue), // SIM222
    RuleSelector::rule(Rule::ExprAndFalse), // SIM223
    RuleSelector::rule(Rule::IfElseBlockInsteadOfDictGet), // SIM401
    RuleSelector::rule(Rule::SplitStaticString), // SIM905
    RuleSelector::rule(Rule::ZipDictKeysAndValues), // SIM911
    RuleSelector::rule(Rule::Debugger), // T100
    RuleSelector::rule(Rule::RuntimeImportInTypeCheckingBlock), // TC004
    RuleSelector::rule(Rule::EmptyTypeCheckingBlock), // TC005
    RuleSelector::rule(Rule::UnquotedTypeAlias), // TC007
    RuleSelector::rule(Rule::RuntimeStringUnion), // TC010
    RuleSelector::rule(Rule::RaiseVanillaClass), // TRY002
    RuleSelector::rule(Rule::VerboseRaise), // TRY201
    RuleSelector::rule(Rule::UselessTryExcept), // TRY203
    RuleSelector::rule(Rule::VerboseLogMessage), // TRY401
    RuleSelector::rule(Rule::UselessMetaclassType), // UP001
    RuleSelector::rule(Rule::TypeOfPrimitive), // UP003
    RuleSelector::rule(Rule::UselessObjectInheritance), // UP004
    RuleSelector::rule(Rule::DeprecatedUnittestAlias), // UP005
    RuleSelector::rule(Rule::NonPEP585Annotation), // UP006
    RuleSelector::rule(Rule::NonPEP604AnnotationUnion), // UP007
    RuleSelector::rule(Rule::SuperCallWithParameters), // UP008
    RuleSelector::rule(Rule::UTF8EncodingDeclaration), // UP009
    RuleSelector::rule(Rule::UnnecessaryFutureImport), // UP010
    RuleSelector::rule(Rule::LRUCacheWithoutParameters), // UP011
    RuleSelector::rule(Rule::UnnecessaryEncodeUTF8), // UP012
    RuleSelector::rule(Rule::ConvertNamedTupleFunctionalToClass), // UP014
    RuleSelector::rule(Rule::DatetimeTimezoneUTC), // UP017
    RuleSelector::rule(Rule::NativeLiterals), // UP018
    RuleSelector::rule(Rule::TypingTextStrAlias), // UP019
    RuleSelector::rule(Rule::OpenAlias), // UP020
    RuleSelector::rule(Rule::ReplaceUniversalNewlines), // UP021
    RuleSelector::rule(Rule::ReplaceStdoutStderr), // UP022
    RuleSelector::rule(Rule::DeprecatedCElementTree), // UP023
    RuleSelector::rule(Rule::OSErrorAlias), // UP024
    RuleSelector::rule(Rule::UnicodeKindPrefix), // UP025
    RuleSelector::rule(Rule::DeprecatedMockImport), // UP026
    RuleSelector::rule(Rule::YieldInForLoop), // UP028
    RuleSelector::rule(Rule::UnnecessaryBuiltinImport), // UP029
    RuleSelector::rule(Rule::FormatLiterals), // UP030
    RuleSelector::rule(Rule::PrintfStringFormatting), // UP031
    RuleSelector::rule(Rule::FString), // UP032
    RuleSelector::rule(Rule::LRUCacheWithMaxsizeNone), // UP033
    RuleSelector::rule(Rule::ExtraneousParentheses), // UP034
    RuleSelector::rule(Rule::DeprecatedImport), // UP035
    RuleSelector::rule(Rule::OutdatedVersionBlock), // UP036
    RuleSelector::rule(Rule::QuotedAnnotation), // UP037
    RuleSelector::rule(Rule::UnnecessaryClassParentheses), // UP039
    RuleSelector::rule(Rule::NonPEP695TypeAlias), // UP040
    RuleSelector::rule(Rule::TimeoutErrorAlias), // UP041
    RuleSelector::rule(Rule::UnnecessaryDefaultTypeArgs), // UP043
    RuleSelector::rule(Rule::NonPEP646Unpack), // UP044
    RuleSelector::rule(Rule::NonPEP604AnnotationOptional), // UP045
    RuleSelector::rule(Rule::NonPEP695GenericClass), // UP046
    RuleSelector::rule(Rule::NonPEP695GenericFunction), // UP047
    RuleSelector::rule(Rule::PrivateTypeParameter), // UP049
    RuleSelector::rule(Rule::UselessClassMetaclassType), // UP050
    RuleSelector::rule(Rule::InvalidEscapeSequence), // W605
    RuleSelector::rule(Rule::SysVersionSlice3), // YTT101
    RuleSelector::rule(Rule::SysVersion2), // YTT102
    RuleSelector::rule(Rule::SysVersionCmpStr3), // YTT103
    RuleSelector::rule(Rule::SysVersionInfo0Eq3), // YTT201
    RuleSelector::rule(Rule::SixPY3), // YTT202
    RuleSelector::rule(Rule::SysVersionInfo1CmpInt), // YTT203
    RuleSelector::rule(Rule::SysVersionInfoMinorCmpInt), // YTT204
    RuleSelector::rule(Rule::SysVersion0), // YTT301
    RuleSelector::rule(Rule::SysVersionCmpStr10), // YTT302
    RuleSelector::rule(Rule::SysVersionSlice1), // YTT303
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
