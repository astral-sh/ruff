//! Registry of all supported `CheckCode` and `CheckKind` types.

use std::fmt;

use once_cell::sync::Lazy;
use ruff_macros::CheckCodePrefix;
use rustc_hash::FxHashMap;
use rustpython_ast::Cmpop;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, Display, EnumIter, EnumString};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::violation::Violation;
use crate::violations;

macro_rules! define_rule_mapping {
    ($($code:ident => $mod:ident::$name:ident,)+) => {
        #[derive(
            AsRefStr,
            CheckCodePrefix,
            EnumIter,
            EnumString,
            Debug,
            Display,
            PartialEq,
            Eq,
            Clone,
            Serialize,
            Deserialize,
            Hash,
            PartialOrd,
            Ord,
        )]
        pub enum CheckCode {
            $(
                $code,
            )+
        }


        #[derive(AsRefStr, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum CheckKind {
            $(
                $name($mod::$name),
            )+
        }

        impl CheckCode {
            /// A placeholder representation of the `CheckKind` for the check.
            pub fn kind(&self) -> CheckKind {
                match self {
                    $(
                        CheckCode::$code => CheckKind::$name(<$mod::$name as Violation>::placeholder()),
                    )+
                }
            }
        }

        impl CheckKind {
            /// A four-letter shorthand code for the check.
            pub fn code(&self) -> &'static CheckCode {
                match self {
                    $(
                        CheckKind::$name(..) => &CheckCode::$code,
                    )+
                }
            }


            /// The body text for the check.
            pub fn body(&self) -> String {
                match self {
                    $(
                        CheckKind::$name(x) => Violation::message(x),
                    )+
                }
            }


            /// Whether the check kind is (potentially) fixable.
            pub fn fixable(&self) -> bool {
                match self {
                    $(
                        CheckKind::$name(x) => x.autofix_title_formatter().is_some(),
                    )+
                }
            }


            /// The message used to describe the fix action for a given `CheckKind`.
            pub fn commit(&self) -> Option<String> {
                match self {
                    $(
                        CheckKind::$name(x) => x.autofix_title_formatter().map(|f| f(x)),
                    )+
                }
            }
        }

        $(
            impl From<$mod::$name> for CheckKind {
                fn from(x: $mod::$name) -> Self {
                    CheckKind::$name(x)
                }
            }
        )+

    };
}

define_rule_mapping!(
    // pycodestyle errors
    E401 => violations::MultipleImportsOnOneLine,
    E402 => violations::ModuleImportNotAtTopOfFile,
    E501 => violations::LineTooLong,
    E711 => violations::NoneComparison,
    E712 => violations::TrueFalseComparison,
    E713 => violations::NotInTest,
    E714 => violations::NotIsTest,
    E721 => violations::TypeComparison,
    E722 => violations::DoNotUseBareExcept,
    E731 => violations::DoNotAssignLambda,
    E741 => violations::AmbiguousVariableName,
    E742 => violations::AmbiguousClassName,
    E743 => violations::AmbiguousFunctionName,
    E902 => violations::IOError,
    E999 => violations::SyntaxError,
    // pycodestyle warnings
    W292 => violations::NoNewLineAtEndOfFile,
    W605 => violations::InvalidEscapeSequence,
    // pyflakes
    F401 => violations::UnusedImport,
    F402 => violations::ImportShadowedByLoopVar,
    F403 => violations::ImportStarUsed,
    F404 => violations::LateFutureImport,
    F405 => violations::ImportStarUsage,
    F406 => violations::ImportStarNotPermitted,
    F407 => violations::FutureFeatureNotDefined,
    F501 => violations::PercentFormatInvalidFormat,
    F502 => violations::PercentFormatExpectedMapping,
    F503 => violations::PercentFormatExpectedSequence,
    F504 => violations::PercentFormatExtraNamedArguments,
    F505 => violations::PercentFormatMissingArgument,
    F506 => violations::PercentFormatMixedPositionalAndNamed,
    F507 => violations::PercentFormatPositionalCountMismatch,
    F508 => violations::PercentFormatStarRequiresSequence,
    F509 => violations::PercentFormatUnsupportedFormatCharacter,
    F521 => violations::StringDotFormatInvalidFormat,
    F522 => violations::StringDotFormatExtraNamedArguments,
    F523 => violations::StringDotFormatExtraPositionalArguments,
    F524 => violations::StringDotFormatMissingArguments,
    F525 => violations::StringDotFormatMixingAutomatic,
    F541 => violations::FStringMissingPlaceholders,
    F601 => violations::MultiValueRepeatedKeyLiteral,
    F602 => violations::MultiValueRepeatedKeyVariable,
    F621 => violations::ExpressionsInStarAssignment,
    F622 => violations::TwoStarredExpressions,
    F631 => violations::AssertTuple,
    F632 => violations::IsLiteral,
    F633 => violations::InvalidPrintSyntax,
    F634 => violations::IfTuple,
    F701 => violations::BreakOutsideLoop,
    F702 => violations::ContinueOutsideLoop,
    F704 => violations::YieldOutsideFunction,
    F706 => violations::ReturnOutsideFunction,
    F707 => violations::DefaultExceptNotLast,
    F722 => violations::ForwardAnnotationSyntaxError,
    F811 => violations::RedefinedWhileUnused,
    F821 => violations::UndefinedName,
    F822 => violations::UndefinedExport,
    F823 => violations::UndefinedLocal,
    F841 => violations::UnusedVariable,
    F842 => violations::UnusedAnnotation,
    F901 => violations::RaiseNotImplemented,
    // pylint
    PLC0414 => violations::UselessImportAlias,
    PLC2201 => violations::MisplacedComparisonConstant,
    PLC3002 => violations::UnnecessaryDirectLambdaCall,
    PLE0117 => violations::NonlocalWithoutBinding,
    PLE0118 => violations::UsedPriorGlobalDeclaration,
    PLE1142 => violations::AwaitOutsideAsync,
    PLR0206 => violations::PropertyWithParameters,
    PLR0402 => violations::ConsiderUsingFromImport,
    PLR1701 => violations::ConsiderMergingIsinstance,
    PLR1722 => violations::UseSysExit,
    PLW0120 => violations::UselessElseOnLoop,
    PLW0602 => violations::GlobalVariableNotAssigned,
    // flake8-builtins
    A001 => violations::BuiltinVariableShadowing,
    A002 => violations::BuiltinArgumentShadowing,
    A003 => violations::BuiltinAttributeShadowing,
    // flake8-bugbear
    B002 => violations::UnaryPrefixIncrement,
    B003 => violations::AssignmentToOsEnviron,
    B004 => violations::UnreliableCallableCheck,
    B005 => violations::StripWithMultiCharacters,
    B006 => violations::MutableArgumentDefault,
    B007 => violations::UnusedLoopControlVariable,
    B008 => violations::FunctionCallArgumentDefault,
    B009 => violations::GetAttrWithConstant,
    B010 => violations::SetAttrWithConstant,
    B011 => violations::DoNotAssertFalse,
    B012 => violations::JumpStatementInFinally,
    B013 => violations::RedundantTupleInExceptionHandler,
    B014 => violations::DuplicateHandlerException,
    B015 => violations::UselessComparison,
    B016 => violations::CannotRaiseLiteral,
    B017 => violations::NoAssertRaisesException,
    B018 => violations::UselessExpression,
    B019 => violations::CachedInstanceMethod,
    B020 => violations::LoopVariableOverridesIterator,
    B021 => violations::FStringDocstring,
    B022 => violations::UselessContextlibSuppress,
    B023 => violations::FunctionUsesLoopVariable,
    B024 => violations::AbstractBaseClassWithoutAbstractMethod,
    B025 => violations::DuplicateTryBlockException,
    B026 => violations::StarArgUnpackingAfterKeywordArg,
    B027 => violations::EmptyMethodWithoutAbstractDecorator,
    B904 => violations::RaiseWithoutFromInsideExcept,
    B905 => violations::ZipWithoutExplicitStrict,
    // flake8-blind-except
    BLE001 => violations::BlindExcept,
    // flake8-comprehensions
    C400 => violations::UnnecessaryGeneratorList,
    C401 => violations::UnnecessaryGeneratorSet,
    C402 => violations::UnnecessaryGeneratorDict,
    C403 => violations::UnnecessaryListComprehensionSet,
    C404 => violations::UnnecessaryListComprehensionDict,
    C405 => violations::UnnecessaryLiteralSet,
    C406 => violations::UnnecessaryLiteralDict,
    C408 => violations::UnnecessaryCollectionCall,
    C409 => violations::UnnecessaryLiteralWithinTupleCall,
    C410 => violations::UnnecessaryLiteralWithinListCall,
    C411 => violations::UnnecessaryListCall,
    C413 => violations::UnnecessaryCallAroundSorted,
    C414 => violations::UnnecessaryDoubleCastOrProcess,
    C415 => violations::UnnecessarySubscriptReversal,
    C416 => violations::UnnecessaryComprehension,
    C417 => violations::UnnecessaryMap,
    // flake8-debugger
    T100 => violations::Debugger,
    // mccabe
    C901 => violations::FunctionIsTooComplex,
    // flake8-tidy-imports
    TID251 => violations::BannedApi,
    TID252 => violations::BannedRelativeImport,
    // flake8-return
    RET501 => violations::UnnecessaryReturnNone,
    RET502 => violations::ImplicitReturnValue,
    RET503 => violations::ImplicitReturn,
    RET504 => violations::UnnecessaryAssign,
    RET505 => violations::SuperfluousElseReturn,
    RET506 => violations::SuperfluousElseRaise,
    RET507 => violations::SuperfluousElseContinue,
    RET508 => violations::SuperfluousElseBreak,
    // flake8-implicit-str-concat
    ISC001 => violations::SingleLineImplicitStringConcatenation,
    ISC002 => violations::MultiLineImplicitStringConcatenation,
    ISC003 => violations::ExplicitStringConcatenation,
    // flake8-print
    T201 => violations::PrintFound,
    T203 => violations::PPrintFound,
    // flake8-quotes
    Q000 => violations::BadQuotesInlineString,
    Q001 => violations::BadQuotesMultilineString,
    Q002 => violations::BadQuotesDocstring,
    Q003 => violations::AvoidQuoteEscape,
    // flake8-annotations
    ANN001 => violations::MissingTypeFunctionArgument,
    ANN002 => violations::MissingTypeArgs,
    ANN003 => violations::MissingTypeKwargs,
    ANN101 => violations::MissingTypeSelf,
    ANN102 => violations::MissingTypeCls,
    ANN201 => violations::MissingReturnTypePublicFunction,
    ANN202 => violations::MissingReturnTypePrivateFunction,
    ANN204 => violations::MissingReturnTypeSpecialMethod,
    ANN205 => violations::MissingReturnTypeStaticMethod,
    ANN206 => violations::MissingReturnTypeClassMethod,
    ANN401 => violations::DynamicallyTypedExpression,
    // flake8-2020
    YTT101 => violations::SysVersionSlice3Referenced,
    YTT102 => violations::SysVersion2Referenced,
    YTT103 => violations::SysVersionCmpStr3,
    YTT201 => violations::SysVersionInfo0Eq3Referenced,
    YTT202 => violations::SixPY3Referenced,
    YTT203 => violations::SysVersionInfo1CmpInt,
    YTT204 => violations::SysVersionInfoMinorCmpInt,
    YTT301 => violations::SysVersion0Referenced,
    YTT302 => violations::SysVersionCmpStr10,
    YTT303 => violations::SysVersionSlice1Referenced,
    // flake8-simplify
    SIM101 => violations::DuplicateIsinstanceCall,
    SIM102 => violations::NestedIfStatements,
    SIM103 => violations::ReturnBoolConditionDirectly,
    SIM105 => violations::UseContextlibSuppress,
    SIM107 => violations::ReturnInTryExceptFinally,
    SIM108 => violations::UseTernaryOperator,
    SIM109 => violations::CompareWithTuple,
    SIM110 => violations::ConvertLoopToAny,
    SIM111 => violations::ConvertLoopToAll,
    SIM117 => violations::MultipleWithStatements,
    SIM118 => violations::KeyInDict,
    SIM201 => violations::NegateEqualOp,
    SIM202 => violations::NegateNotEqualOp,
    SIM208 => violations::DoubleNegation,
    SIM220 => violations::AAndNotA,
    SIM221 => violations::AOrNotA,
    SIM222 => violations::OrTrue,
    SIM223 => violations::AndFalse,
    SIM300 => violations::YodaConditions,
    // pyupgrade
    UP001 => violations::UselessMetaclassType,
    UP003 => violations::TypeOfPrimitive,
    UP004 => violations::UselessObjectInheritance,
    UP005 => violations::DeprecatedUnittestAlias,
    UP006 => violations::UsePEP585Annotation,
    UP007 => violations::UsePEP604Annotation,
    UP008 => violations::SuperCallWithParameters,
    UP009 => violations::PEP3120UnnecessaryCodingComment,
    UP010 => violations::UnnecessaryFutureImport,
    UP011 => violations::UnnecessaryLRUCacheParams,
    UP012 => violations::UnnecessaryEncodeUTF8,
    UP013 => violations::ConvertTypedDictFunctionalToClass,
    UP014 => violations::ConvertNamedTupleFunctionalToClass,
    UP015 => violations::RedundantOpenModes,
    UP016 => violations::RemoveSixCompat,
    UP017 => violations::DatetimeTimezoneUTC,
    UP018 => violations::NativeLiterals,
    UP019 => violations::TypingTextStrAlias,
    UP020 => violations::OpenAlias,
    UP021 => violations::ReplaceUniversalNewlines,
    UP022 => violations::ReplaceStdoutStderr,
    UP023 => violations::RewriteCElementTree,
    UP024 => violations::OSErrorAlias,
    UP025 => violations::RewriteUnicodeLiteral,
    UP026 => violations::RewriteMockImport,
    UP027 => violations::RewriteListComprehension,
    UP028 => violations::RewriteYieldFrom,
    UP029 => violations::UnnecessaryBuiltinImport,
    // pydocstyle
    D100 => violations::PublicModule,
    D101 => violations::PublicClass,
    D102 => violations::PublicMethod,
    D103 => violations::PublicFunction,
    D104 => violations::PublicPackage,
    D105 => violations::MagicMethod,
    D106 => violations::PublicNestedClass,
    D107 => violations::PublicInit,
    D200 => violations::FitsOnOneLine,
    D201 => violations::NoBlankLineBeforeFunction,
    D202 => violations::NoBlankLineAfterFunction,
    D203 => violations::OneBlankLineBeforeClass,
    D204 => violations::OneBlankLineAfterClass,
    D205 => violations::BlankLineAfterSummary,
    D206 => violations::IndentWithSpaces,
    D207 => violations::NoUnderIndentation,
    D208 => violations::NoOverIndentation,
    D209 => violations::NewLineAfterLastParagraph,
    D210 => violations::NoSurroundingWhitespace,
    D211 => violations::NoBlankLineBeforeClass,
    D212 => violations::MultiLineSummaryFirstLine,
    D213 => violations::MultiLineSummarySecondLine,
    D214 => violations::SectionNotOverIndented,
    D215 => violations::SectionUnderlineNotOverIndented,
    D300 => violations::UsesTripleQuotes,
    D301 => violations::UsesRPrefixForBackslashedContent,
    D400 => violations::EndsInPeriod,
    D402 => violations::NoSignature,
    D403 => violations::FirstLineCapitalized,
    D404 => violations::NoThisPrefix,
    D405 => violations::CapitalizeSectionName,
    D406 => violations::NewLineAfterSectionName,
    D407 => violations::DashedUnderlineAfterSection,
    D408 => violations::SectionUnderlineAfterName,
    D409 => violations::SectionUnderlineMatchesSectionLength,
    D410 => violations::BlankLineAfterSection,
    D411 => violations::BlankLineBeforeSection,
    D412 => violations::NoBlankLinesBetweenHeaderAndContent,
    D413 => violations::BlankLineAfterLastSection,
    D414 => violations::NonEmptySection,
    D415 => violations::EndsInPunctuation,
    D416 => violations::SectionNameEndsInColon,
    D417 => violations::DocumentAllArguments,
    D418 => violations::SkipDocstring,
    D419 => violations::NonEmpty,
    // pep8-naming
    N801 => violations::InvalidClassName,
    N802 => violations::InvalidFunctionName,
    N803 => violations::InvalidArgumentName,
    N804 => violations::InvalidFirstArgumentNameForClassMethod,
    N805 => violations::InvalidFirstArgumentNameForMethod,
    N806 => violations::NonLowercaseVariableInFunction,
    N807 => violations::DunderFunctionName,
    N811 => violations::ConstantImportedAsNonConstant,
    N812 => violations::LowercaseImportedAsNonLowercase,
    N813 => violations::CamelcaseImportedAsLowercase,
    N814 => violations::CamelcaseImportedAsConstant,
    N815 => violations::MixedCaseVariableInClassScope,
    N816 => violations::MixedCaseVariableInGlobalScope,
    N817 => violations::CamelcaseImportedAsAcronym,
    N818 => violations::ErrorSuffixOnExceptionName,
    // isort
    I001 => violations::UnsortedImports,
    // eradicate
    ERA001 => violations::CommentedOutCode,
    // flake8-bandit
    S101 => violations::AssertUsed,
    S102 => violations::ExecUsed,
    S103 => violations::BadFilePermissions,
    S104 => violations::HardcodedBindAllInterfaces,
    S105 => violations::HardcodedPasswordString,
    S106 => violations::HardcodedPasswordFuncArg,
    S107 => violations::HardcodedPasswordDefault,
    S108 => violations::HardcodedTempFile,
    S113 => violations::RequestWithoutTimeout,
    S324 => violations::HashlibInsecureHashFunction,
    S501 => violations::RequestWithNoCertValidation,
    S506 => violations::UnsafeYAMLLoad,
    // flake8-boolean-trap
    FBT001 => violations::BooleanPositionalArgInFunctionDefinition,
    FBT002 => violations::BooleanDefaultValueInFunctionDefinition,
    FBT003 => violations::BooleanPositionalValueInFunctionCall,
    // flake8-unused-arguments
    ARG001 => violations::UnusedFunctionArgument,
    ARG002 => violations::UnusedMethodArgument,
    ARG003 => violations::UnusedClassMethodArgument,
    ARG004 => violations::UnusedStaticMethodArgument,
    ARG005 => violations::UnusedLambdaArgument,
    // flake8-import-conventions
    ICN001 => violations::ImportAliasIsNotConventional,
    // flake8-datetimez
    DTZ001 => violations::CallDatetimeWithoutTzinfo,
    DTZ002 => violations::CallDatetimeToday,
    DTZ003 => violations::CallDatetimeUtcnow,
    DTZ004 => violations::CallDatetimeUtcfromtimestamp,
    DTZ005 => violations::CallDatetimeNowWithoutTzinfo,
    DTZ006 => violations::CallDatetimeFromtimestamp,
    DTZ007 => violations::CallDatetimeStrptimeWithoutZone,
    DTZ011 => violations::CallDateToday,
    DTZ012 => violations::CallDateFromtimestamp,
    // pygrep-hooks
    PGH001 => violations::NoEval,
    PGH002 => violations::DeprecatedLogWarn,
    PGH003 => violations::BlanketTypeIgnore,
    PGH004 => violations::BlanketNOQA,
    // pandas-vet
    PD002 => violations::UseOfInplaceArgument,
    PD003 => violations::UseOfDotIsNull,
    PD004 => violations::UseOfDotNotNull,
    PD007 => violations::UseOfDotIx,
    PD008 => violations::UseOfDotAt,
    PD009 => violations::UseOfDotIat,
    PD010 => violations::UseOfDotPivotOrUnstack,
    PD011 => violations::UseOfDotValues,
    PD012 => violations::UseOfDotReadTable,
    PD013 => violations::UseOfDotStack,
    PD015 => violations::UseOfPdMerge,
    PD901 => violations::DfIsABadVariableName,
    // flake8-errmsg
    EM101 => violations::RawStringInException,
    EM102 => violations::FStringInException,
    EM103 => violations::DotFormatInException,
    // flake8-pytest-style
    PT001 => violations::IncorrectFixtureParenthesesStyle,
    PT002 => violations::FixturePositionalArgs,
    PT003 => violations::ExtraneousScopeFunction,
    PT004 => violations::MissingFixtureNameUnderscore,
    PT005 => violations::IncorrectFixtureNameUnderscore,
    PT006 => violations::ParametrizeNamesWrongType,
    PT007 => violations::ParametrizeValuesWrongType,
    PT008 => violations::PatchWithLambda,
    PT009 => violations::UnittestAssertion,
    PT010 => violations::RaisesWithoutException,
    PT011 => violations::RaisesTooBroad,
    PT012 => violations::RaisesWithMultipleStatements,
    PT013 => violations::IncorrectPytestImport,
    PT015 => violations::AssertAlwaysFalse,
    PT016 => violations::FailWithoutMessage,
    PT017 => violations::AssertInExcept,
    PT018 => violations::CompositeAssertion,
    PT019 => violations::FixtureParamWithoutValue,
    PT020 => violations::DeprecatedYieldFixture,
    PT021 => violations::FixtureFinalizerCallback,
    PT022 => violations::UselessYieldFixture,
    PT023 => violations::IncorrectMarkParenthesesStyle,
    PT024 => violations::UnnecessaryAsyncioMarkOnFixture,
    PT025 => violations::ErroneousUseFixturesOnFixture,
    PT026 => violations::UseFixturesWithoutParameters,
    // flake8-pie
    PIE790 => violations::NoUnnecessaryPass,
    PIE794 => violations::DupeClassFieldDefinitions,
    PIE807 => violations::PreferListBuiltin,
    // Ruff
    RUF001 => violations::AmbiguousUnicodeCharacterString,
    RUF002 => violations::AmbiguousUnicodeCharacterDocstring,
    RUF003 => violations::AmbiguousUnicodeCharacterComment,
    RUF004 => violations::KeywordArgumentBeforeStarArgument,
    RUF100 => violations::UnusedNOQA,
);

#[derive(EnumIter, Debug, PartialEq, Eq)]
pub enum CheckCategory {
    Pyflakes,
    Pycodestyle,
    McCabe,
    Isort,
    Pydocstyle,
    Pyupgrade,
    PEP8Naming,
    Flake82020,
    Flake8Annotations,
    Flake8Bandit,
    Flake8BlindExcept,
    Flake8BooleanTrap,
    Flake8Bugbear,
    Flake8Builtins,
    Flake8Comprehensions,
    Flake8Debugger,
    Flake8ErrMsg,
    Flake8ImplicitStrConcat,
    Flake8ImportConventions,
    Flake8Print,
    Flake8PytestStyle,
    Flake8Quotes,
    Flake8Return,
    Flake8Simplify,
    Flake8TidyImports,
    Flake8UnusedArguments,
    Flake8Datetimez,
    Eradicate,
    PandasVet,
    PygrepHooks,
    Pylint,
    Flake8Pie,
    Ruff,
}

pub enum Platform {
    PyPI,
    GitHub,
}

impl fmt::Display for Platform {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Platform::PyPI => fmt.write_str("PyPI"),
            Platform::GitHub => fmt.write_str("GitHub"),
        }
    }
}

impl CheckCategory {
    pub fn title(&self) -> &'static str {
        match self {
            CheckCategory::Eradicate => "eradicate",
            CheckCategory::Flake82020 => "flake8-2020",
            CheckCategory::Flake8Annotations => "flake8-annotations",
            CheckCategory::Flake8Bandit => "flake8-bandit",
            CheckCategory::Flake8BlindExcept => "flake8-blind-except",
            CheckCategory::Flake8BooleanTrap => "flake8-boolean-trap",
            CheckCategory::Flake8Bugbear => "flake8-bugbear",
            CheckCategory::Flake8Builtins => "flake8-builtins",
            CheckCategory::Flake8Comprehensions => "flake8-comprehensions",
            CheckCategory::Flake8Debugger => "flake8-debugger",
            CheckCategory::Flake8ErrMsg => "flake8-errmsg",
            CheckCategory::Flake8ImplicitStrConcat => "flake8-implicit-str-concat",
            CheckCategory::Flake8ImportConventions => "flake8-import-conventions",
            CheckCategory::Flake8Print => "flake8-print",
            CheckCategory::Flake8PytestStyle => "flake8-pytest-style",
            CheckCategory::Flake8Quotes => "flake8-quotes",
            CheckCategory::Flake8Return => "flake8-return",
            CheckCategory::Flake8TidyImports => "flake8-tidy-imports",
            CheckCategory::Flake8Simplify => "flake8-simplify",
            CheckCategory::Flake8UnusedArguments => "flake8-unused-arguments",
            CheckCategory::Flake8Datetimez => "flake8-datetimez",
            CheckCategory::Isort => "isort",
            CheckCategory::McCabe => "mccabe",
            CheckCategory::PandasVet => "pandas-vet",
            CheckCategory::PEP8Naming => "pep8-naming",
            CheckCategory::Pycodestyle => "pycodestyle",
            CheckCategory::Pydocstyle => "pydocstyle",
            CheckCategory::Pyflakes => "Pyflakes",
            CheckCategory::PygrepHooks => "pygrep-hooks",
            CheckCategory::Pylint => "Pylint",
            CheckCategory::Pyupgrade => "pyupgrade",
            CheckCategory::Flake8Pie => "flake8-pie",
            CheckCategory::Ruff => "Ruff-specific rules",
        }
    }

    pub fn codes(&self) -> Vec<CheckCodePrefix> {
        match self {
            CheckCategory::Eradicate => vec![CheckCodePrefix::ERA],
            CheckCategory::Flake82020 => vec![CheckCodePrefix::YTT],
            CheckCategory::Flake8Annotations => vec![CheckCodePrefix::ANN],
            CheckCategory::Flake8Bandit => vec![CheckCodePrefix::S],
            CheckCategory::Flake8BlindExcept => vec![CheckCodePrefix::BLE],
            CheckCategory::Flake8BooleanTrap => vec![CheckCodePrefix::FBT],
            CheckCategory::Flake8Bugbear => vec![CheckCodePrefix::B],
            CheckCategory::Flake8Builtins => vec![CheckCodePrefix::A],
            CheckCategory::Flake8Comprehensions => vec![CheckCodePrefix::C4],
            CheckCategory::Flake8Datetimez => vec![CheckCodePrefix::DTZ],
            CheckCategory::Flake8Debugger => vec![CheckCodePrefix::T10],
            CheckCategory::Flake8ErrMsg => vec![CheckCodePrefix::EM],
            CheckCategory::Flake8ImplicitStrConcat => vec![CheckCodePrefix::ISC],
            CheckCategory::Flake8ImportConventions => vec![CheckCodePrefix::ICN],
            CheckCategory::Flake8Print => vec![CheckCodePrefix::T20],
            CheckCategory::Flake8PytestStyle => vec![CheckCodePrefix::PT],
            CheckCategory::Flake8Quotes => vec![CheckCodePrefix::Q],
            CheckCategory::Flake8Return => vec![CheckCodePrefix::RET],
            CheckCategory::Flake8Simplify => vec![CheckCodePrefix::SIM],
            CheckCategory::Flake8TidyImports => vec![CheckCodePrefix::TID],
            CheckCategory::Flake8UnusedArguments => vec![CheckCodePrefix::ARG],
            CheckCategory::Isort => vec![CheckCodePrefix::I],
            CheckCategory::McCabe => vec![CheckCodePrefix::C90],
            CheckCategory::PEP8Naming => vec![CheckCodePrefix::N],
            CheckCategory::PandasVet => vec![CheckCodePrefix::PD],
            CheckCategory::Pycodestyle => vec![CheckCodePrefix::E, CheckCodePrefix::W],
            CheckCategory::Pydocstyle => vec![CheckCodePrefix::D],
            CheckCategory::Pyflakes => vec![CheckCodePrefix::F],
            CheckCategory::PygrepHooks => vec![CheckCodePrefix::PGH],
            CheckCategory::Pylint => vec![
                CheckCodePrefix::PLC,
                CheckCodePrefix::PLE,
                CheckCodePrefix::PLR,
                CheckCodePrefix::PLW,
            ],
            CheckCategory::Pyupgrade => vec![CheckCodePrefix::UP],
            CheckCategory::Flake8Pie => vec![CheckCodePrefix::PIE],
            CheckCategory::Ruff => vec![CheckCodePrefix::RUF],
        }
    }

    pub fn url(&self) -> Option<(&'static str, &'static Platform)> {
        match self {
            CheckCategory::Eradicate => {
                Some(("https://pypi.org/project/eradicate/2.1.0/", &Platform::PyPI))
            }
            CheckCategory::Flake82020 => Some((
                "https://pypi.org/project/flake8-2020/1.7.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Annotations => Some((
                "https://pypi.org/project/flake8-annotations/2.9.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Bandit => Some((
                "https://pypi.org/project/flake8-bandit/4.1.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8BlindExcept => Some((
                "https://pypi.org/project/flake8-blind-except/0.2.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8BooleanTrap => Some((
                "https://pypi.org/project/flake8-boolean-trap/0.1.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Bugbear => Some((
                "https://pypi.org/project/flake8-bugbear/22.10.27/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Builtins => Some((
                "https://pypi.org/project/flake8-builtins/2.0.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Comprehensions => Some((
                "https://pypi.org/project/flake8-comprehensions/3.10.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Debugger => Some((
                "https://pypi.org/project/flake8-debugger/4.1.2/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8ErrMsg => Some((
                "https://pypi.org/project/flake8-errmsg/0.4.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8ImplicitStrConcat => Some((
                "https://pypi.org/project/flake8-implicit-str-concat/0.3.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8ImportConventions => None,
            CheckCategory::Flake8Print => Some((
                "https://pypi.org/project/flake8-print/5.0.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8PytestStyle => Some((
                "https://pypi.org/project/flake8-pytest-style/1.6.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Quotes => Some((
                "https://pypi.org/project/flake8-quotes/3.3.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Return => Some((
                "https://pypi.org/project/flake8-return/1.2.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Simplify => Some((
                "https://pypi.org/project/flake8-simplify/0.19.3/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8TidyImports => Some((
                "https://pypi.org/project/flake8-tidy-imports/4.8.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8UnusedArguments => Some((
                "https://pypi.org/project/flake8-unused-arguments/0.0.12/",
                &Platform::PyPI,
            )),
            CheckCategory::Flake8Datetimez => Some((
                "https://pypi.org/project/flake8-datetimez/20.10.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Isort => {
                Some(("https://pypi.org/project/isort/5.10.1/", &Platform::PyPI))
            }
            CheckCategory::McCabe => {
                Some(("https://pypi.org/project/mccabe/0.7.0/", &Platform::PyPI))
            }
            CheckCategory::PandasVet => Some((
                "https://pypi.org/project/pandas-vet/0.2.3/",
                &Platform::PyPI,
            )),
            CheckCategory::PEP8Naming => Some((
                "https://pypi.org/project/pep8-naming/0.13.2/",
                &Platform::PyPI,
            )),
            CheckCategory::Pycodestyle => Some((
                "https://pypi.org/project/pycodestyle/2.9.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Pydocstyle => Some((
                "https://pypi.org/project/pydocstyle/6.1.1/",
                &Platform::PyPI,
            )),
            CheckCategory::Pyflakes => {
                Some(("https://pypi.org/project/pyflakes/2.5.0/", &Platform::PyPI))
            }
            CheckCategory::Pylint => {
                Some(("https://pypi.org/project/pylint/2.15.7/", &Platform::PyPI))
            }
            CheckCategory::PygrepHooks => Some((
                "https://github.com/pre-commit/pygrep-hooks",
                &Platform::GitHub,
            )),
            CheckCategory::Pyupgrade => {
                Some(("https://pypi.org/project/pyupgrade/3.2.0/", &Platform::PyPI))
            }
            CheckCategory::Flake8Pie => Some((
                "https://pypi.org/project/flake8-pie/0.16.0/",
                &Platform::PyPI,
            )),
            CheckCategory::Ruff => None,
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub enum LintSource {
    AST,
    FileSystem,
    Lines,
    Tokens,
    Imports,
    NoQA,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqCmpop {
    Eq,
    NotEq,
}

impl From<&Cmpop> for EqCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Eq => EqCmpop::Eq,
            Cmpop::NotEq => EqCmpop::NotEq,
            _ => unreachable!("Expected Cmpop::Eq | Cmpop::NotEq"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsCmpop {
    Is,
    IsNot,
}

impl From<&Cmpop> for IsCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Is => IsCmpop::Is,
            Cmpop::IsNot => IsCmpop::IsNot,
            _ => unreachable!("Expected Cmpop::Is | Cmpop::IsNot"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferralKeyword {
    Yield,
    YieldFrom,
    Await,
}

impl fmt::Display for DeferralKeyword {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeferralKeyword::Yield => fmt.write_str("yield"),
            DeferralKeyword::YieldFrom => fmt.write_str("yield from"),
            DeferralKeyword::Await => fmt.write_str("await"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Branch {
    Elif,
    Else,
}

impl fmt::Display for Branch {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Branch::Elif => fmt.write_str("elif"),
            Branch::Else => fmt.write_str("else"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiteralType {
    Str,
    Bytes,
}

impl fmt::Display for LiteralType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LiteralType::Str => fmt.write_str("str"),
            LiteralType::Bytes => fmt.write_str("bytes"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusedCodes {
    pub unknown: Vec<String>,
    pub disabled: Vec<String>,
    pub unmatched: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MockReference {
    Import,
    Attribute,
}

impl CheckCode {
    /// The source for the check (either the AST, the filesystem, or the
    /// physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::RUF100 => &LintSource::NoQA,
            CheckCode::E501
            | CheckCode::W292
            | CheckCode::UP009
            | CheckCode::PGH003
            | CheckCode::PGH004 => &LintSource::Lines,
            CheckCode::ERA001
            | CheckCode::ISC001
            | CheckCode::ISC002
            | CheckCode::Q000
            | CheckCode::Q001
            | CheckCode::Q002
            | CheckCode::Q003
            | CheckCode::W605
            | CheckCode::RUF001
            | CheckCode::RUF002
            | CheckCode::RUF003 => &LintSource::Tokens,
            CheckCode::E902 => &LintSource::FileSystem,
            CheckCode::I001 => &LintSource::Imports,
            _ => &LintSource::AST,
        }
    }

    pub fn category(&self) -> CheckCategory {
        #[allow(clippy::match_same_arms)]
        match self {
            // flake8-builtins
            CheckCode::A001 => CheckCategory::Flake8Builtins,
            CheckCode::A002 => CheckCategory::Flake8Builtins,
            CheckCode::A003 => CheckCategory::Flake8Builtins,
            // flake8-annotations
            CheckCode::ANN001 => CheckCategory::Flake8Annotations,
            CheckCode::ANN002 => CheckCategory::Flake8Annotations,
            CheckCode::ANN003 => CheckCategory::Flake8Annotations,
            CheckCode::ANN101 => CheckCategory::Flake8Annotations,
            CheckCode::ANN102 => CheckCategory::Flake8Annotations,
            CheckCode::ANN201 => CheckCategory::Flake8Annotations,
            CheckCode::ANN202 => CheckCategory::Flake8Annotations,
            CheckCode::ANN204 => CheckCategory::Flake8Annotations,
            CheckCode::ANN205 => CheckCategory::Flake8Annotations,
            CheckCode::ANN206 => CheckCategory::Flake8Annotations,
            CheckCode::ANN401 => CheckCategory::Flake8Annotations,
            // flake8-unused-arguments
            CheckCode::ARG001 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG002 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG003 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG004 => CheckCategory::Flake8UnusedArguments,
            CheckCode::ARG005 => CheckCategory::Flake8UnusedArguments,
            // flake8-bugbear
            CheckCode::B002 => CheckCategory::Flake8Bugbear,
            CheckCode::B003 => CheckCategory::Flake8Bugbear,
            CheckCode::B004 => CheckCategory::Flake8Bugbear,
            CheckCode::B005 => CheckCategory::Flake8Bugbear,
            CheckCode::B006 => CheckCategory::Flake8Bugbear,
            CheckCode::B007 => CheckCategory::Flake8Bugbear,
            CheckCode::B008 => CheckCategory::Flake8Bugbear,
            CheckCode::B009 => CheckCategory::Flake8Bugbear,
            CheckCode::B010 => CheckCategory::Flake8Bugbear,
            CheckCode::B011 => CheckCategory::Flake8Bugbear,
            CheckCode::B012 => CheckCategory::Flake8Bugbear,
            CheckCode::B013 => CheckCategory::Flake8Bugbear,
            CheckCode::B014 => CheckCategory::Flake8Bugbear,
            CheckCode::B015 => CheckCategory::Flake8Bugbear,
            CheckCode::B016 => CheckCategory::Flake8Bugbear,
            CheckCode::B017 => CheckCategory::Flake8Bugbear,
            CheckCode::B018 => CheckCategory::Flake8Bugbear,
            CheckCode::B019 => CheckCategory::Flake8Bugbear,
            CheckCode::B020 => CheckCategory::Flake8Bugbear,
            CheckCode::B021 => CheckCategory::Flake8Bugbear,
            CheckCode::B022 => CheckCategory::Flake8Bugbear,
            CheckCode::B023 => CheckCategory::Flake8Bugbear,
            CheckCode::B024 => CheckCategory::Flake8Bugbear,
            CheckCode::B025 => CheckCategory::Flake8Bugbear,
            CheckCode::B026 => CheckCategory::Flake8Bugbear,
            CheckCode::B027 => CheckCategory::Flake8Bugbear,
            CheckCode::B904 => CheckCategory::Flake8Bugbear,
            CheckCode::B905 => CheckCategory::Flake8Bugbear,
            // flake8-blind-except
            CheckCode::BLE001 => CheckCategory::Flake8BlindExcept,
            // flake8-comprehensions
            CheckCode::C400 => CheckCategory::Flake8Comprehensions,
            CheckCode::C401 => CheckCategory::Flake8Comprehensions,
            CheckCode::C402 => CheckCategory::Flake8Comprehensions,
            CheckCode::C403 => CheckCategory::Flake8Comprehensions,
            CheckCode::C404 => CheckCategory::Flake8Comprehensions,
            CheckCode::C405 => CheckCategory::Flake8Comprehensions,
            CheckCode::C406 => CheckCategory::Flake8Comprehensions,
            CheckCode::C408 => CheckCategory::Flake8Comprehensions,
            CheckCode::C409 => CheckCategory::Flake8Comprehensions,
            CheckCode::C410 => CheckCategory::Flake8Comprehensions,
            CheckCode::C411 => CheckCategory::Flake8Comprehensions,
            CheckCode::C413 => CheckCategory::Flake8Comprehensions,
            CheckCode::C414 => CheckCategory::Flake8Comprehensions,
            CheckCode::C415 => CheckCategory::Flake8Comprehensions,
            CheckCode::C416 => CheckCategory::Flake8Comprehensions,
            CheckCode::C417 => CheckCategory::Flake8Comprehensions,
            // mccabe
            CheckCode::C901 => CheckCategory::McCabe,
            // pydocstyle
            CheckCode::D100 => CheckCategory::Pydocstyle,
            CheckCode::D101 => CheckCategory::Pydocstyle,
            CheckCode::D102 => CheckCategory::Pydocstyle,
            CheckCode::D103 => CheckCategory::Pydocstyle,
            CheckCode::D104 => CheckCategory::Pydocstyle,
            CheckCode::D105 => CheckCategory::Pydocstyle,
            CheckCode::D106 => CheckCategory::Pydocstyle,
            CheckCode::D107 => CheckCategory::Pydocstyle,
            CheckCode::D200 => CheckCategory::Pydocstyle,
            CheckCode::D201 => CheckCategory::Pydocstyle,
            CheckCode::D202 => CheckCategory::Pydocstyle,
            CheckCode::D203 => CheckCategory::Pydocstyle,
            CheckCode::D204 => CheckCategory::Pydocstyle,
            CheckCode::D205 => CheckCategory::Pydocstyle,
            CheckCode::D206 => CheckCategory::Pydocstyle,
            CheckCode::D207 => CheckCategory::Pydocstyle,
            CheckCode::D208 => CheckCategory::Pydocstyle,
            CheckCode::D209 => CheckCategory::Pydocstyle,
            CheckCode::D210 => CheckCategory::Pydocstyle,
            CheckCode::D211 => CheckCategory::Pydocstyle,
            CheckCode::D212 => CheckCategory::Pydocstyle,
            CheckCode::D213 => CheckCategory::Pydocstyle,
            CheckCode::D214 => CheckCategory::Pydocstyle,
            CheckCode::D215 => CheckCategory::Pydocstyle,
            CheckCode::D300 => CheckCategory::Pydocstyle,
            CheckCode::D301 => CheckCategory::Pydocstyle,
            CheckCode::D400 => CheckCategory::Pydocstyle,
            CheckCode::D402 => CheckCategory::Pydocstyle,
            CheckCode::D403 => CheckCategory::Pydocstyle,
            CheckCode::D404 => CheckCategory::Pydocstyle,
            CheckCode::D405 => CheckCategory::Pydocstyle,
            CheckCode::D406 => CheckCategory::Pydocstyle,
            CheckCode::D407 => CheckCategory::Pydocstyle,
            CheckCode::D408 => CheckCategory::Pydocstyle,
            CheckCode::D409 => CheckCategory::Pydocstyle,
            CheckCode::D410 => CheckCategory::Pydocstyle,
            CheckCode::D411 => CheckCategory::Pydocstyle,
            CheckCode::D412 => CheckCategory::Pydocstyle,
            CheckCode::D413 => CheckCategory::Pydocstyle,
            CheckCode::D414 => CheckCategory::Pydocstyle,
            CheckCode::D415 => CheckCategory::Pydocstyle,
            CheckCode::D416 => CheckCategory::Pydocstyle,
            CheckCode::D417 => CheckCategory::Pydocstyle,
            CheckCode::D418 => CheckCategory::Pydocstyle,
            CheckCode::D419 => CheckCategory::Pydocstyle,
            // flake8-datetimez
            CheckCode::DTZ001 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ002 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ003 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ004 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ005 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ006 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ007 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ011 => CheckCategory::Flake8Datetimez,
            CheckCode::DTZ012 => CheckCategory::Flake8Datetimez,
            // pycodestyle (errors)
            CheckCode::E401 => CheckCategory::Pycodestyle,
            CheckCode::E402 => CheckCategory::Pycodestyle,
            CheckCode::E501 => CheckCategory::Pycodestyle,
            CheckCode::E711 => CheckCategory::Pycodestyle,
            CheckCode::E712 => CheckCategory::Pycodestyle,
            CheckCode::E713 => CheckCategory::Pycodestyle,
            CheckCode::E714 => CheckCategory::Pycodestyle,
            CheckCode::E721 => CheckCategory::Pycodestyle,
            CheckCode::E722 => CheckCategory::Pycodestyle,
            CheckCode::E731 => CheckCategory::Pycodestyle,
            CheckCode::E741 => CheckCategory::Pycodestyle,
            CheckCode::E742 => CheckCategory::Pycodestyle,
            CheckCode::E743 => CheckCategory::Pycodestyle,
            CheckCode::E902 => CheckCategory::Pycodestyle,
            CheckCode::E999 => CheckCategory::Pycodestyle,
            // flake8-errmsg
            CheckCode::EM101 => CheckCategory::Flake8ErrMsg,
            CheckCode::EM102 => CheckCategory::Flake8ErrMsg,
            CheckCode::EM103 => CheckCategory::Flake8ErrMsg,
            // eradicate
            CheckCode::ERA001 => CheckCategory::Eradicate,
            // pyflakes
            CheckCode::F401 => CheckCategory::Pyflakes,
            CheckCode::F402 => CheckCategory::Pyflakes,
            CheckCode::F403 => CheckCategory::Pyflakes,
            CheckCode::F404 => CheckCategory::Pyflakes,
            CheckCode::F405 => CheckCategory::Pyflakes,
            CheckCode::F406 => CheckCategory::Pyflakes,
            CheckCode::F407 => CheckCategory::Pyflakes,
            CheckCode::F501 => CheckCategory::Pyflakes,
            CheckCode::F502 => CheckCategory::Pyflakes,
            CheckCode::F503 => CheckCategory::Pyflakes,
            CheckCode::F504 => CheckCategory::Pyflakes,
            CheckCode::F505 => CheckCategory::Pyflakes,
            CheckCode::F506 => CheckCategory::Pyflakes,
            CheckCode::F507 => CheckCategory::Pyflakes,
            CheckCode::F508 => CheckCategory::Pyflakes,
            CheckCode::F509 => CheckCategory::Pyflakes,
            CheckCode::F521 => CheckCategory::Pyflakes,
            CheckCode::F522 => CheckCategory::Pyflakes,
            CheckCode::F523 => CheckCategory::Pyflakes,
            CheckCode::F524 => CheckCategory::Pyflakes,
            CheckCode::F525 => CheckCategory::Pyflakes,
            CheckCode::F541 => CheckCategory::Pyflakes,
            CheckCode::F601 => CheckCategory::Pyflakes,
            CheckCode::F602 => CheckCategory::Pyflakes,
            CheckCode::F621 => CheckCategory::Pyflakes,
            CheckCode::F622 => CheckCategory::Pyflakes,
            CheckCode::F631 => CheckCategory::Pyflakes,
            CheckCode::F632 => CheckCategory::Pyflakes,
            CheckCode::F633 => CheckCategory::Pyflakes,
            CheckCode::F634 => CheckCategory::Pyflakes,
            CheckCode::F701 => CheckCategory::Pyflakes,
            CheckCode::F702 => CheckCategory::Pyflakes,
            CheckCode::F704 => CheckCategory::Pyflakes,
            CheckCode::F706 => CheckCategory::Pyflakes,
            CheckCode::F707 => CheckCategory::Pyflakes,
            CheckCode::F722 => CheckCategory::Pyflakes,
            CheckCode::F811 => CheckCategory::Pyflakes,
            CheckCode::F821 => CheckCategory::Pyflakes,
            CheckCode::F822 => CheckCategory::Pyflakes,
            CheckCode::F823 => CheckCategory::Pyflakes,
            CheckCode::F841 => CheckCategory::Pyflakes,
            CheckCode::F842 => CheckCategory::Pyflakes,
            CheckCode::F901 => CheckCategory::Pyflakes,
            // flake8-boolean-trap
            CheckCode::FBT001 => CheckCategory::Flake8BooleanTrap,
            CheckCode::FBT002 => CheckCategory::Flake8BooleanTrap,
            CheckCode::FBT003 => CheckCategory::Flake8BooleanTrap,
            // isort
            CheckCode::I001 => CheckCategory::Isort,
            // flake8-import-conventions
            CheckCode::ICN001 => CheckCategory::Flake8ImportConventions,
            // flake8-implicit-str-concat
            CheckCode::ISC001 => CheckCategory::Flake8ImplicitStrConcat,
            CheckCode::ISC002 => CheckCategory::Flake8ImplicitStrConcat,
            CheckCode::ISC003 => CheckCategory::Flake8ImplicitStrConcat,
            // pep8-naming
            CheckCode::N801 => CheckCategory::PEP8Naming,
            CheckCode::N802 => CheckCategory::PEP8Naming,
            CheckCode::N803 => CheckCategory::PEP8Naming,
            CheckCode::N804 => CheckCategory::PEP8Naming,
            CheckCode::N805 => CheckCategory::PEP8Naming,
            CheckCode::N806 => CheckCategory::PEP8Naming,
            CheckCode::N807 => CheckCategory::PEP8Naming,
            CheckCode::N811 => CheckCategory::PEP8Naming,
            CheckCode::N812 => CheckCategory::PEP8Naming,
            CheckCode::N813 => CheckCategory::PEP8Naming,
            CheckCode::N814 => CheckCategory::PEP8Naming,
            CheckCode::N815 => CheckCategory::PEP8Naming,
            CheckCode::N816 => CheckCategory::PEP8Naming,
            CheckCode::N817 => CheckCategory::PEP8Naming,
            CheckCode::N818 => CheckCategory::PEP8Naming,
            // pandas-vet
            CheckCode::PD002 => CheckCategory::PandasVet,
            CheckCode::PD003 => CheckCategory::PandasVet,
            CheckCode::PD004 => CheckCategory::PandasVet,
            CheckCode::PD007 => CheckCategory::PandasVet,
            CheckCode::PD008 => CheckCategory::PandasVet,
            CheckCode::PD009 => CheckCategory::PandasVet,
            CheckCode::PD010 => CheckCategory::PandasVet,
            CheckCode::PD011 => CheckCategory::PandasVet,
            CheckCode::PD012 => CheckCategory::PandasVet,
            CheckCode::PD013 => CheckCategory::PandasVet,
            CheckCode::PD015 => CheckCategory::PandasVet,
            CheckCode::PD901 => CheckCategory::PandasVet,
            // pygrep-hooks
            CheckCode::PGH001 => CheckCategory::PygrepHooks,
            CheckCode::PGH002 => CheckCategory::PygrepHooks,
            CheckCode::PGH003 => CheckCategory::PygrepHooks,
            CheckCode::PGH004 => CheckCategory::PygrepHooks,
            // pylint
            CheckCode::PLC0414 => CheckCategory::Pylint,
            CheckCode::PLC2201 => CheckCategory::Pylint,
            CheckCode::PLC3002 => CheckCategory::Pylint,
            CheckCode::PLE0117 => CheckCategory::Pylint,
            CheckCode::PLE0118 => CheckCategory::Pylint,
            CheckCode::PLE1142 => CheckCategory::Pylint,
            CheckCode::PLR0206 => CheckCategory::Pylint,
            CheckCode::PLR0402 => CheckCategory::Pylint,
            CheckCode::PLR1701 => CheckCategory::Pylint,
            CheckCode::PLR1722 => CheckCategory::Pylint,
            CheckCode::PLW0120 => CheckCategory::Pylint,
            CheckCode::PLW0602 => CheckCategory::Pylint,
            // flake8-pytest-style
            CheckCode::PT001 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT002 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT003 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT004 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT005 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT006 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT007 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT008 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT009 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT010 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT011 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT012 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT013 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT015 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT016 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT017 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT018 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT019 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT020 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT021 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT022 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT023 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT024 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT025 => CheckCategory::Flake8PytestStyle,
            CheckCode::PT026 => CheckCategory::Flake8PytestStyle,
            // flake8-quotes
            CheckCode::Q000 => CheckCategory::Flake8Quotes,
            CheckCode::Q001 => CheckCategory::Flake8Quotes,
            CheckCode::Q002 => CheckCategory::Flake8Quotes,
            CheckCode::Q003 => CheckCategory::Flake8Quotes,
            // flake8-return
            CheckCode::RET501 => CheckCategory::Flake8Return,
            CheckCode::RET502 => CheckCategory::Flake8Return,
            CheckCode::RET503 => CheckCategory::Flake8Return,
            CheckCode::RET504 => CheckCategory::Flake8Return,
            CheckCode::RET505 => CheckCategory::Flake8Return,
            CheckCode::RET506 => CheckCategory::Flake8Return,
            CheckCode::RET507 => CheckCategory::Flake8Return,
            CheckCode::RET508 => CheckCategory::Flake8Return,
            // flake8-bandit
            CheckCode::S101 => CheckCategory::Flake8Bandit,
            CheckCode::S102 => CheckCategory::Flake8Bandit,
            CheckCode::S103 => CheckCategory::Flake8Bandit,
            CheckCode::S104 => CheckCategory::Flake8Bandit,
            CheckCode::S105 => CheckCategory::Flake8Bandit,
            CheckCode::S106 => CheckCategory::Flake8Bandit,
            CheckCode::S107 => CheckCategory::Flake8Bandit,
            CheckCode::S108 => CheckCategory::Flake8Bandit,
            CheckCode::S113 => CheckCategory::Flake8Bandit,
            CheckCode::S324 => CheckCategory::Flake8Bandit,
            CheckCode::S501 => CheckCategory::Flake8Bandit,
            CheckCode::S506 => CheckCategory::Flake8Bandit,
            // flake8-simplify
            CheckCode::SIM103 => CheckCategory::Flake8Simplify,
            CheckCode::SIM101 => CheckCategory::Flake8Simplify,
            CheckCode::SIM102 => CheckCategory::Flake8Simplify,
            CheckCode::SIM105 => CheckCategory::Flake8Simplify,
            CheckCode::SIM107 => CheckCategory::Flake8Simplify,
            CheckCode::SIM108 => CheckCategory::Flake8Simplify,
            CheckCode::SIM109 => CheckCategory::Flake8Simplify,
            CheckCode::SIM110 => CheckCategory::Flake8Simplify,
            CheckCode::SIM111 => CheckCategory::Flake8Simplify,
            CheckCode::SIM117 => CheckCategory::Flake8Simplify,
            CheckCode::SIM118 => CheckCategory::Flake8Simplify,
            CheckCode::SIM201 => CheckCategory::Flake8Simplify,
            CheckCode::SIM202 => CheckCategory::Flake8Simplify,
            CheckCode::SIM208 => CheckCategory::Flake8Simplify,
            CheckCode::SIM220 => CheckCategory::Flake8Simplify,
            CheckCode::SIM221 => CheckCategory::Flake8Simplify,
            CheckCode::SIM222 => CheckCategory::Flake8Simplify,
            CheckCode::SIM223 => CheckCategory::Flake8Simplify,
            CheckCode::SIM300 => CheckCategory::Flake8Simplify,
            // flake8-debugger
            CheckCode::T100 => CheckCategory::Flake8Debugger,
            // flake8-print
            CheckCode::T201 => CheckCategory::Flake8Print,
            CheckCode::T203 => CheckCategory::Flake8Print,
            // flake8-tidy-imports
            CheckCode::TID251 => CheckCategory::Flake8TidyImports,
            CheckCode::TID252 => CheckCategory::Flake8TidyImports,
            // pyupgrade
            CheckCode::UP001 => CheckCategory::Pyupgrade,
            CheckCode::UP003 => CheckCategory::Pyupgrade,
            CheckCode::UP004 => CheckCategory::Pyupgrade,
            CheckCode::UP005 => CheckCategory::Pyupgrade,
            CheckCode::UP006 => CheckCategory::Pyupgrade,
            CheckCode::UP007 => CheckCategory::Pyupgrade,
            CheckCode::UP008 => CheckCategory::Pyupgrade,
            CheckCode::UP009 => CheckCategory::Pyupgrade,
            CheckCode::UP010 => CheckCategory::Pyupgrade,
            CheckCode::UP011 => CheckCategory::Pyupgrade,
            CheckCode::UP012 => CheckCategory::Pyupgrade,
            CheckCode::UP013 => CheckCategory::Pyupgrade,
            CheckCode::UP014 => CheckCategory::Pyupgrade,
            CheckCode::UP015 => CheckCategory::Pyupgrade,
            CheckCode::UP016 => CheckCategory::Pyupgrade,
            CheckCode::UP017 => CheckCategory::Pyupgrade,
            CheckCode::UP018 => CheckCategory::Pyupgrade,
            CheckCode::UP019 => CheckCategory::Pyupgrade,
            CheckCode::UP020 => CheckCategory::Pyupgrade,
            CheckCode::UP021 => CheckCategory::Pyupgrade,
            CheckCode::UP022 => CheckCategory::Pyupgrade,
            CheckCode::UP023 => CheckCategory::Pyupgrade,
            CheckCode::UP024 => CheckCategory::Pyupgrade,
            CheckCode::UP025 => CheckCategory::Pyupgrade,
            CheckCode::UP026 => CheckCategory::Pyupgrade,
            CheckCode::UP027 => CheckCategory::Pyupgrade,
            CheckCode::UP028 => CheckCategory::Pyupgrade,
            CheckCode::UP029 => CheckCategory::Pyupgrade,
            // pycodestyle (warnings)
            CheckCode::W292 => CheckCategory::Pycodestyle,
            CheckCode::W605 => CheckCategory::Pycodestyle,
            // flake8-2020
            CheckCode::YTT101 => CheckCategory::Flake82020,
            CheckCode::YTT102 => CheckCategory::Flake82020,
            CheckCode::YTT103 => CheckCategory::Flake82020,
            CheckCode::YTT201 => CheckCategory::Flake82020,
            CheckCode::YTT202 => CheckCategory::Flake82020,
            CheckCode::YTT203 => CheckCategory::Flake82020,
            CheckCode::YTT204 => CheckCategory::Flake82020,
            CheckCode::YTT301 => CheckCategory::Flake82020,
            CheckCode::YTT302 => CheckCategory::Flake82020,
            CheckCode::YTT303 => CheckCategory::Flake82020,
            // flake8-pie
            CheckCode::PIE790 => CheckCategory::Flake8Pie,
            CheckCode::PIE794 => CheckCategory::Flake8Pie,
            CheckCode::PIE807 => CheckCategory::Flake8Pie,
            // Ruff
            CheckCode::RUF001 => CheckCategory::Ruff,
            CheckCode::RUF002 => CheckCategory::Ruff,
            CheckCode::RUF003 => CheckCategory::Ruff,
            CheckCode::RUF004 => CheckCategory::Ruff,
            CheckCode::RUF100 => CheckCategory::Ruff,
        }
    }
}

impl CheckKind {
    /// The summary text for the check. Typically a truncated form of the body
    /// text.
    pub fn summary(&self) -> String {
        match self {
            CheckKind::UnaryPrefixIncrement(..) => {
                "Python does not support the unary prefix increment".to_string()
            }
            CheckKind::UnusedLoopControlVariable(violations::UnusedLoopControlVariable(name)) => {
                format!("Loop control variable `{name}` not used within the loop body")
            }
            CheckKind::NoAssertRaisesException(..) => {
                "`assertRaises(Exception)` should be considered evil".to_string()
            }
            CheckKind::StarArgUnpackingAfterKeywordArg(..) => {
                "Star-arg unpacking after a keyword argument is strongly discouraged".to_string()
            }

            // flake8-datetimez
            CheckKind::CallDatetimeToday(..) => {
                "The use of `datetime.datetime.today()` is not allowed".to_string()
            }
            CheckKind::CallDatetimeUtcnow(..) => {
                "The use of `datetime.datetime.utcnow()` is not allowed".to_string()
            }
            CheckKind::CallDatetimeUtcfromtimestamp(..) => {
                "The use of `datetime.datetime.utcfromtimestamp()` is not allowed".to_string()
            }
            CheckKind::CallDateToday(..) => {
                "The use of `datetime.date.today()` is not allowed.".to_string()
            }
            CheckKind::CallDateFromtimestamp(..) => {
                "The use of `datetime.date.fromtimestamp()` is not allowed".to_string()
            }
            _ => self.body(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
    pub parent: Option<Location>,
}

impl Check {
    pub fn new<K: Into<CheckKind>>(kind: K, range: Range) -> Self {
        Self {
            kind: kind.into(),
            location: range.location,
            end_location: range.end_location,
            fix: None,
            parent: None,
        }
    }

    pub fn amend(&mut self, fix: Fix) -> &mut Self {
        self.fix = Some(fix);
        self
    }

    pub fn parent(&mut self, parent: Location) -> &mut Self {
        self.parent = Some(parent);
        self
    }
}

/// Pairs of checks that shouldn't be enabled together.
pub const INCOMPATIBLE_CODES: &[(CheckCode, CheckCode, &str)] = &[(
    CheckCode::D203,
    CheckCode::D211,
    "`D203` (OneBlankLineBeforeClass) and `D211` (NoBlankLinesBeforeClass) are incompatible. \
     Consider adding `D203` to `ignore`.",
)];

/// A hash map from deprecated to latest `CheckCode`.
pub static CODE_REDIRECTS: Lazy<FxHashMap<&'static str, CheckCode>> = Lazy::new(|| {
    FxHashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U001", CheckCode::UP001),
        ("U003", CheckCode::UP003),
        ("U004", CheckCode::UP004),
        ("U005", CheckCode::UP005),
        ("U006", CheckCode::UP006),
        ("U007", CheckCode::UP007),
        ("U008", CheckCode::UP008),
        ("U009", CheckCode::UP009),
        ("U010", CheckCode::UP010),
        ("U011", CheckCode::UP011),
        ("U012", CheckCode::UP012),
        ("U013", CheckCode::UP013),
        ("U014", CheckCode::UP014),
        ("U015", CheckCode::UP015),
        ("U016", CheckCode::UP016),
        ("U017", CheckCode::UP017),
        ("U019", CheckCode::UP019),
        // TODO(charlie): Remove by 2023-02-01.
        ("I252", CheckCode::TID252),
        ("M001", CheckCode::RUF100),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV002", CheckCode::PD002),
        ("PDV003", CheckCode::PD003),
        ("PDV004", CheckCode::PD004),
        ("PDV007", CheckCode::PD007),
        ("PDV008", CheckCode::PD008),
        ("PDV009", CheckCode::PD009),
        ("PDV010", CheckCode::PD010),
        ("PDV011", CheckCode::PD011),
        ("PDV012", CheckCode::PD012),
        ("PDV013", CheckCode::PD013),
        ("PDV015", CheckCode::PD015),
        ("PDV901", CheckCode::PD901),
        // TODO(charlie): Remove by 2023-02-01.
        ("R501", CheckCode::RET501),
        ("R502", CheckCode::RET502),
        ("R503", CheckCode::RET503),
        ("R504", CheckCode::RET504),
        ("R505", CheckCode::RET505),
        ("R506", CheckCode::RET506),
        ("R507", CheckCode::RET507),
        ("R508", CheckCode::RET508),
        // TODO(charlie): Remove by 2023-02-01.
        ("IC001", CheckCode::ICN001),
        ("IC002", CheckCode::ICN001),
        ("IC003", CheckCode::ICN001),
        ("IC004", CheckCode::ICN001),
    ])
});

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use strum::IntoEnumIterator;

    use crate::registry::CheckCode;

    #[test]
    fn check_code_serialization() {
        for check_code in CheckCode::iter() {
            assert!(
                CheckCode::from_str(check_code.as_ref()).is_ok(),
                "{check_code:?} could not be round-trip serialized."
            );
        }
    }

    #[test]
    fn fixable_codes() {
        for check_code in CheckCode::iter() {
            let kind = check_code.kind();
            if kind.fixable() {
                assert!(
                    kind.commit().is_some(),
                    "{check_code:?} is fixable but has no commit message."
                );
            }
        }
    }
}
