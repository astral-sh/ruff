//! Registry to `DiagnosticCode` to `DiagnosticKind` mappings.

use std::fmt;

use once_cell::sync::Lazy;
use ruff_macros::DiagnosticCodePrefix;
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
            DiagnosticCodePrefix,
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
        pub enum DiagnosticCode {
            $(
                $code,
            )+
        }

        #[derive(AsRefStr, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum DiagnosticKind {
            $(
                $name($mod::$name),
            )+
        }

        impl DiagnosticCode {
            /// A placeholder representation of the `DiagnosticKind` for the diagnostic.
            pub fn kind(&self) -> DiagnosticKind {
                match self {
                    $(
                        DiagnosticCode::$code => DiagnosticKind::$name(<$mod::$name as Violation>::placeholder()),
                    )+
                }
            }
        }

        impl DiagnosticKind {
            /// A four-letter shorthand code for the diagnostic.
            pub fn code(&self) -> &'static DiagnosticCode {
                match self {
                    $(
                        DiagnosticKind::$name(..) => &DiagnosticCode::$code,
                    )+
                }
            }

            /// The body text for the diagnostic.
            pub fn body(&self) -> String {
                match self {
                    $(
                        DiagnosticKind::$name(x) => Violation::message(x),
                    )+
                }
            }

            /// Whether the check kind is (potentially) fixable.
            pub fn fixable(&self) -> bool {
                match self {
                    $(
                        DiagnosticKind::$name(x) => x.autofix_title_formatter().is_some(),
                    )+
                }
            }

            /// The message used to describe the fix action for a given `DiagnosticKind`.
            pub fn commit(&self) -> Option<String> {
                match self {
                    $(
                        DiagnosticKind::$name(x) => x.autofix_title_formatter().map(|f| f(x)),
                    )+
                }
            }
        }

        $(
            impl From<$mod::$name> for DiagnosticKind {
                fn from(x: $mod::$name) -> Self {
                    DiagnosticKind::$name(x)
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
    SIM210 => violations::IfExprWithTrueFalse,
    SIM211 => violations::IfExprWithFalseTrue,
    SIM212 => violations::IfExprWithTwistedArms,
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

    pub fn codes(&self) -> Vec<DiagnosticCodePrefix> {
        match self {
            CheckCategory::Eradicate => vec![DiagnosticCodePrefix::ERA],
            CheckCategory::Flake82020 => vec![DiagnosticCodePrefix::YTT],
            CheckCategory::Flake8Annotations => vec![DiagnosticCodePrefix::ANN],
            CheckCategory::Flake8Bandit => vec![DiagnosticCodePrefix::S],
            CheckCategory::Flake8BlindExcept => vec![DiagnosticCodePrefix::BLE],
            CheckCategory::Flake8BooleanTrap => vec![DiagnosticCodePrefix::FBT],
            CheckCategory::Flake8Bugbear => vec![DiagnosticCodePrefix::B],
            CheckCategory::Flake8Builtins => vec![DiagnosticCodePrefix::A],
            CheckCategory::Flake8Comprehensions => vec![DiagnosticCodePrefix::C4],
            CheckCategory::Flake8Datetimez => vec![DiagnosticCodePrefix::DTZ],
            CheckCategory::Flake8Debugger => vec![DiagnosticCodePrefix::T10],
            CheckCategory::Flake8ErrMsg => vec![DiagnosticCodePrefix::EM],
            CheckCategory::Flake8ImplicitStrConcat => vec![DiagnosticCodePrefix::ISC],
            CheckCategory::Flake8ImportConventions => vec![DiagnosticCodePrefix::ICN],
            CheckCategory::Flake8Print => vec![DiagnosticCodePrefix::T20],
            CheckCategory::Flake8PytestStyle => vec![DiagnosticCodePrefix::PT],
            CheckCategory::Flake8Quotes => vec![DiagnosticCodePrefix::Q],
            CheckCategory::Flake8Return => vec![DiagnosticCodePrefix::RET],
            CheckCategory::Flake8Simplify => vec![DiagnosticCodePrefix::SIM],
            CheckCategory::Flake8TidyImports => vec![DiagnosticCodePrefix::TID],
            CheckCategory::Flake8UnusedArguments => vec![DiagnosticCodePrefix::ARG],
            CheckCategory::Isort => vec![DiagnosticCodePrefix::I],
            CheckCategory::McCabe => vec![DiagnosticCodePrefix::C90],
            CheckCategory::PEP8Naming => vec![DiagnosticCodePrefix::N],
            CheckCategory::PandasVet => vec![DiagnosticCodePrefix::PD],
            CheckCategory::Pycodestyle => vec![DiagnosticCodePrefix::E, DiagnosticCodePrefix::W],
            CheckCategory::Pydocstyle => vec![DiagnosticCodePrefix::D],
            CheckCategory::Pyflakes => vec![DiagnosticCodePrefix::F],
            CheckCategory::PygrepHooks => vec![DiagnosticCodePrefix::PGH],
            CheckCategory::Pylint => vec![
                DiagnosticCodePrefix::PLC,
                DiagnosticCodePrefix::PLE,
                DiagnosticCodePrefix::PLR,
                DiagnosticCodePrefix::PLW,
            ],
            CheckCategory::Pyupgrade => vec![DiagnosticCodePrefix::UP],
            CheckCategory::Flake8Pie => vec![DiagnosticCodePrefix::PIE],
            CheckCategory::Ruff => vec![DiagnosticCodePrefix::RUF],
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

impl DiagnosticCode {
    /// The source for the diagnostic (either the AST, the filesystem, or the
    /// physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            DiagnosticCode::RUF100 => &LintSource::NoQA,
            DiagnosticCode::E501
            | DiagnosticCode::W292
            | DiagnosticCode::UP009
            | DiagnosticCode::PGH003
            | DiagnosticCode::PGH004 => &LintSource::Lines,
            DiagnosticCode::ERA001
            | DiagnosticCode::ISC001
            | DiagnosticCode::ISC002
            | DiagnosticCode::Q000
            | DiagnosticCode::Q001
            | DiagnosticCode::Q002
            | DiagnosticCode::Q003
            | DiagnosticCode::W605
            | DiagnosticCode::RUF001
            | DiagnosticCode::RUF002
            | DiagnosticCode::RUF003 => &LintSource::Tokens,
            DiagnosticCode::E902 => &LintSource::FileSystem,
            DiagnosticCode::I001 => &LintSource::Imports,
            _ => &LintSource::AST,
        }
    }

    pub fn category(&self) -> CheckCategory {
        #[allow(clippy::match_same_arms)]
        match self {
            // flake8-builtins
            DiagnosticCode::A001 => CheckCategory::Flake8Builtins,
            DiagnosticCode::A002 => CheckCategory::Flake8Builtins,
            DiagnosticCode::A003 => CheckCategory::Flake8Builtins,
            // flake8-annotations
            DiagnosticCode::ANN001 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN002 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN003 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN101 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN102 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN201 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN202 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN204 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN205 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN206 => CheckCategory::Flake8Annotations,
            DiagnosticCode::ANN401 => CheckCategory::Flake8Annotations,
            // flake8-unused-arguments
            DiagnosticCode::ARG001 => CheckCategory::Flake8UnusedArguments,
            DiagnosticCode::ARG002 => CheckCategory::Flake8UnusedArguments,
            DiagnosticCode::ARG003 => CheckCategory::Flake8UnusedArguments,
            DiagnosticCode::ARG004 => CheckCategory::Flake8UnusedArguments,
            DiagnosticCode::ARG005 => CheckCategory::Flake8UnusedArguments,
            // flake8-bugbear
            DiagnosticCode::B002 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B003 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B004 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B005 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B006 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B007 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B008 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B009 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B010 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B011 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B012 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B013 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B014 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B015 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B016 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B017 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B018 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B019 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B020 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B021 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B022 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B023 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B024 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B025 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B026 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B027 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B904 => CheckCategory::Flake8Bugbear,
            DiagnosticCode::B905 => CheckCategory::Flake8Bugbear,
            // flake8-blind-except
            DiagnosticCode::BLE001 => CheckCategory::Flake8BlindExcept,
            // flake8-comprehensions
            DiagnosticCode::C400 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C401 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C402 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C403 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C404 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C405 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C406 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C408 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C409 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C410 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C411 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C413 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C414 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C415 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C416 => CheckCategory::Flake8Comprehensions,
            DiagnosticCode::C417 => CheckCategory::Flake8Comprehensions,
            // mccabe
            DiagnosticCode::C901 => CheckCategory::McCabe,
            // pydocstyle
            DiagnosticCode::D100 => CheckCategory::Pydocstyle,
            DiagnosticCode::D101 => CheckCategory::Pydocstyle,
            DiagnosticCode::D102 => CheckCategory::Pydocstyle,
            DiagnosticCode::D103 => CheckCategory::Pydocstyle,
            DiagnosticCode::D104 => CheckCategory::Pydocstyle,
            DiagnosticCode::D105 => CheckCategory::Pydocstyle,
            DiagnosticCode::D106 => CheckCategory::Pydocstyle,
            DiagnosticCode::D107 => CheckCategory::Pydocstyle,
            DiagnosticCode::D200 => CheckCategory::Pydocstyle,
            DiagnosticCode::D201 => CheckCategory::Pydocstyle,
            DiagnosticCode::D202 => CheckCategory::Pydocstyle,
            DiagnosticCode::D203 => CheckCategory::Pydocstyle,
            DiagnosticCode::D204 => CheckCategory::Pydocstyle,
            DiagnosticCode::D205 => CheckCategory::Pydocstyle,
            DiagnosticCode::D206 => CheckCategory::Pydocstyle,
            DiagnosticCode::D207 => CheckCategory::Pydocstyle,
            DiagnosticCode::D208 => CheckCategory::Pydocstyle,
            DiagnosticCode::D209 => CheckCategory::Pydocstyle,
            DiagnosticCode::D210 => CheckCategory::Pydocstyle,
            DiagnosticCode::D211 => CheckCategory::Pydocstyle,
            DiagnosticCode::D212 => CheckCategory::Pydocstyle,
            DiagnosticCode::D213 => CheckCategory::Pydocstyle,
            DiagnosticCode::D214 => CheckCategory::Pydocstyle,
            DiagnosticCode::D215 => CheckCategory::Pydocstyle,
            DiagnosticCode::D300 => CheckCategory::Pydocstyle,
            DiagnosticCode::D301 => CheckCategory::Pydocstyle,
            DiagnosticCode::D400 => CheckCategory::Pydocstyle,
            DiagnosticCode::D402 => CheckCategory::Pydocstyle,
            DiagnosticCode::D403 => CheckCategory::Pydocstyle,
            DiagnosticCode::D404 => CheckCategory::Pydocstyle,
            DiagnosticCode::D405 => CheckCategory::Pydocstyle,
            DiagnosticCode::D406 => CheckCategory::Pydocstyle,
            DiagnosticCode::D407 => CheckCategory::Pydocstyle,
            DiagnosticCode::D408 => CheckCategory::Pydocstyle,
            DiagnosticCode::D409 => CheckCategory::Pydocstyle,
            DiagnosticCode::D410 => CheckCategory::Pydocstyle,
            DiagnosticCode::D411 => CheckCategory::Pydocstyle,
            DiagnosticCode::D412 => CheckCategory::Pydocstyle,
            DiagnosticCode::D413 => CheckCategory::Pydocstyle,
            DiagnosticCode::D414 => CheckCategory::Pydocstyle,
            DiagnosticCode::D415 => CheckCategory::Pydocstyle,
            DiagnosticCode::D416 => CheckCategory::Pydocstyle,
            DiagnosticCode::D417 => CheckCategory::Pydocstyle,
            DiagnosticCode::D418 => CheckCategory::Pydocstyle,
            DiagnosticCode::D419 => CheckCategory::Pydocstyle,
            // flake8-datetimez
            DiagnosticCode::DTZ001 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ002 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ003 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ004 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ005 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ006 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ007 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ011 => CheckCategory::Flake8Datetimez,
            DiagnosticCode::DTZ012 => CheckCategory::Flake8Datetimez,
            // pycodestyle (errors)
            DiagnosticCode::E401 => CheckCategory::Pycodestyle,
            DiagnosticCode::E402 => CheckCategory::Pycodestyle,
            DiagnosticCode::E501 => CheckCategory::Pycodestyle,
            DiagnosticCode::E711 => CheckCategory::Pycodestyle,
            DiagnosticCode::E712 => CheckCategory::Pycodestyle,
            DiagnosticCode::E713 => CheckCategory::Pycodestyle,
            DiagnosticCode::E714 => CheckCategory::Pycodestyle,
            DiagnosticCode::E721 => CheckCategory::Pycodestyle,
            DiagnosticCode::E722 => CheckCategory::Pycodestyle,
            DiagnosticCode::E731 => CheckCategory::Pycodestyle,
            DiagnosticCode::E741 => CheckCategory::Pycodestyle,
            DiagnosticCode::E742 => CheckCategory::Pycodestyle,
            DiagnosticCode::E743 => CheckCategory::Pycodestyle,
            DiagnosticCode::E902 => CheckCategory::Pycodestyle,
            DiagnosticCode::E999 => CheckCategory::Pycodestyle,
            // flake8-errmsg
            DiagnosticCode::EM101 => CheckCategory::Flake8ErrMsg,
            DiagnosticCode::EM102 => CheckCategory::Flake8ErrMsg,
            DiagnosticCode::EM103 => CheckCategory::Flake8ErrMsg,
            // eradicate
            DiagnosticCode::ERA001 => CheckCategory::Eradicate,
            // pyflakes
            DiagnosticCode::F401 => CheckCategory::Pyflakes,
            DiagnosticCode::F402 => CheckCategory::Pyflakes,
            DiagnosticCode::F403 => CheckCategory::Pyflakes,
            DiagnosticCode::F404 => CheckCategory::Pyflakes,
            DiagnosticCode::F405 => CheckCategory::Pyflakes,
            DiagnosticCode::F406 => CheckCategory::Pyflakes,
            DiagnosticCode::F407 => CheckCategory::Pyflakes,
            DiagnosticCode::F501 => CheckCategory::Pyflakes,
            DiagnosticCode::F502 => CheckCategory::Pyflakes,
            DiagnosticCode::F503 => CheckCategory::Pyflakes,
            DiagnosticCode::F504 => CheckCategory::Pyflakes,
            DiagnosticCode::F505 => CheckCategory::Pyflakes,
            DiagnosticCode::F506 => CheckCategory::Pyflakes,
            DiagnosticCode::F507 => CheckCategory::Pyflakes,
            DiagnosticCode::F508 => CheckCategory::Pyflakes,
            DiagnosticCode::F509 => CheckCategory::Pyflakes,
            DiagnosticCode::F521 => CheckCategory::Pyflakes,
            DiagnosticCode::F522 => CheckCategory::Pyflakes,
            DiagnosticCode::F523 => CheckCategory::Pyflakes,
            DiagnosticCode::F524 => CheckCategory::Pyflakes,
            DiagnosticCode::F525 => CheckCategory::Pyflakes,
            DiagnosticCode::F541 => CheckCategory::Pyflakes,
            DiagnosticCode::F601 => CheckCategory::Pyflakes,
            DiagnosticCode::F602 => CheckCategory::Pyflakes,
            DiagnosticCode::F621 => CheckCategory::Pyflakes,
            DiagnosticCode::F622 => CheckCategory::Pyflakes,
            DiagnosticCode::F631 => CheckCategory::Pyflakes,
            DiagnosticCode::F632 => CheckCategory::Pyflakes,
            DiagnosticCode::F633 => CheckCategory::Pyflakes,
            DiagnosticCode::F634 => CheckCategory::Pyflakes,
            DiagnosticCode::F701 => CheckCategory::Pyflakes,
            DiagnosticCode::F702 => CheckCategory::Pyflakes,
            DiagnosticCode::F704 => CheckCategory::Pyflakes,
            DiagnosticCode::F706 => CheckCategory::Pyflakes,
            DiagnosticCode::F707 => CheckCategory::Pyflakes,
            DiagnosticCode::F722 => CheckCategory::Pyflakes,
            DiagnosticCode::F811 => CheckCategory::Pyflakes,
            DiagnosticCode::F821 => CheckCategory::Pyflakes,
            DiagnosticCode::F822 => CheckCategory::Pyflakes,
            DiagnosticCode::F823 => CheckCategory::Pyflakes,
            DiagnosticCode::F841 => CheckCategory::Pyflakes,
            DiagnosticCode::F842 => CheckCategory::Pyflakes,
            DiagnosticCode::F901 => CheckCategory::Pyflakes,
            // flake8-boolean-trap
            DiagnosticCode::FBT001 => CheckCategory::Flake8BooleanTrap,
            DiagnosticCode::FBT002 => CheckCategory::Flake8BooleanTrap,
            DiagnosticCode::FBT003 => CheckCategory::Flake8BooleanTrap,
            // isort
            DiagnosticCode::I001 => CheckCategory::Isort,
            // flake8-import-conventions
            DiagnosticCode::ICN001 => CheckCategory::Flake8ImportConventions,
            // flake8-implicit-str-concat
            DiagnosticCode::ISC001 => CheckCategory::Flake8ImplicitStrConcat,
            DiagnosticCode::ISC002 => CheckCategory::Flake8ImplicitStrConcat,
            DiagnosticCode::ISC003 => CheckCategory::Flake8ImplicitStrConcat,
            // pep8-naming
            DiagnosticCode::N801 => CheckCategory::PEP8Naming,
            DiagnosticCode::N802 => CheckCategory::PEP8Naming,
            DiagnosticCode::N803 => CheckCategory::PEP8Naming,
            DiagnosticCode::N804 => CheckCategory::PEP8Naming,
            DiagnosticCode::N805 => CheckCategory::PEP8Naming,
            DiagnosticCode::N806 => CheckCategory::PEP8Naming,
            DiagnosticCode::N807 => CheckCategory::PEP8Naming,
            DiagnosticCode::N811 => CheckCategory::PEP8Naming,
            DiagnosticCode::N812 => CheckCategory::PEP8Naming,
            DiagnosticCode::N813 => CheckCategory::PEP8Naming,
            DiagnosticCode::N814 => CheckCategory::PEP8Naming,
            DiagnosticCode::N815 => CheckCategory::PEP8Naming,
            DiagnosticCode::N816 => CheckCategory::PEP8Naming,
            DiagnosticCode::N817 => CheckCategory::PEP8Naming,
            DiagnosticCode::N818 => CheckCategory::PEP8Naming,
            // pandas-vet
            DiagnosticCode::PD002 => CheckCategory::PandasVet,
            DiagnosticCode::PD003 => CheckCategory::PandasVet,
            DiagnosticCode::PD004 => CheckCategory::PandasVet,
            DiagnosticCode::PD007 => CheckCategory::PandasVet,
            DiagnosticCode::PD008 => CheckCategory::PandasVet,
            DiagnosticCode::PD009 => CheckCategory::PandasVet,
            DiagnosticCode::PD010 => CheckCategory::PandasVet,
            DiagnosticCode::PD011 => CheckCategory::PandasVet,
            DiagnosticCode::PD012 => CheckCategory::PandasVet,
            DiagnosticCode::PD013 => CheckCategory::PandasVet,
            DiagnosticCode::PD015 => CheckCategory::PandasVet,
            DiagnosticCode::PD901 => CheckCategory::PandasVet,
            // pygrep-hooks
            DiagnosticCode::PGH001 => CheckCategory::PygrepHooks,
            DiagnosticCode::PGH002 => CheckCategory::PygrepHooks,
            DiagnosticCode::PGH003 => CheckCategory::PygrepHooks,
            DiagnosticCode::PGH004 => CheckCategory::PygrepHooks,
            // pylint
            DiagnosticCode::PLC0414 => CheckCategory::Pylint,
            DiagnosticCode::PLC2201 => CheckCategory::Pylint,
            DiagnosticCode::PLC3002 => CheckCategory::Pylint,
            DiagnosticCode::PLE0117 => CheckCategory::Pylint,
            DiagnosticCode::PLE0118 => CheckCategory::Pylint,
            DiagnosticCode::PLE1142 => CheckCategory::Pylint,
            DiagnosticCode::PLR0206 => CheckCategory::Pylint,
            DiagnosticCode::PLR0402 => CheckCategory::Pylint,
            DiagnosticCode::PLR1701 => CheckCategory::Pylint,
            DiagnosticCode::PLR1722 => CheckCategory::Pylint,
            DiagnosticCode::PLW0120 => CheckCategory::Pylint,
            DiagnosticCode::PLW0602 => CheckCategory::Pylint,
            // flake8-pytest-style
            DiagnosticCode::PT001 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT002 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT003 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT004 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT005 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT006 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT007 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT008 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT009 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT010 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT011 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT012 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT013 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT015 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT016 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT017 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT018 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT019 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT020 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT021 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT022 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT023 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT024 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT025 => CheckCategory::Flake8PytestStyle,
            DiagnosticCode::PT026 => CheckCategory::Flake8PytestStyle,
            // flake8-quotes
            DiagnosticCode::Q000 => CheckCategory::Flake8Quotes,
            DiagnosticCode::Q001 => CheckCategory::Flake8Quotes,
            DiagnosticCode::Q002 => CheckCategory::Flake8Quotes,
            DiagnosticCode::Q003 => CheckCategory::Flake8Quotes,
            // flake8-return
            DiagnosticCode::RET501 => CheckCategory::Flake8Return,
            DiagnosticCode::RET502 => CheckCategory::Flake8Return,
            DiagnosticCode::RET503 => CheckCategory::Flake8Return,
            DiagnosticCode::RET504 => CheckCategory::Flake8Return,
            DiagnosticCode::RET505 => CheckCategory::Flake8Return,
            DiagnosticCode::RET506 => CheckCategory::Flake8Return,
            DiagnosticCode::RET507 => CheckCategory::Flake8Return,
            DiagnosticCode::RET508 => CheckCategory::Flake8Return,
            // flake8-bandit
            DiagnosticCode::S101 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S102 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S103 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S104 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S105 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S106 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S107 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S108 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S113 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S324 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S501 => CheckCategory::Flake8Bandit,
            DiagnosticCode::S506 => CheckCategory::Flake8Bandit,
            // flake8-simplify
            DiagnosticCode::SIM103 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM101 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM102 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM105 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM107 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM108 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM109 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM110 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM111 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM117 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM118 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM201 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM202 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM208 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM210 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM211 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM212 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM220 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM221 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM222 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM223 => CheckCategory::Flake8Simplify,
            DiagnosticCode::SIM300 => CheckCategory::Flake8Simplify,
            // flake8-debugger
            DiagnosticCode::T100 => CheckCategory::Flake8Debugger,
            // flake8-print
            DiagnosticCode::T201 => CheckCategory::Flake8Print,
            DiagnosticCode::T203 => CheckCategory::Flake8Print,
            // flake8-tidy-imports
            DiagnosticCode::TID251 => CheckCategory::Flake8TidyImports,
            DiagnosticCode::TID252 => CheckCategory::Flake8TidyImports,
            // pyupgrade
            DiagnosticCode::UP001 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP003 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP004 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP005 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP006 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP007 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP008 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP009 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP010 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP011 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP012 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP013 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP014 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP015 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP016 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP017 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP018 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP019 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP020 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP021 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP022 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP023 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP024 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP025 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP026 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP027 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP028 => CheckCategory::Pyupgrade,
            DiagnosticCode::UP029 => CheckCategory::Pyupgrade,
            // pycodestyle (warnings)
            DiagnosticCode::W292 => CheckCategory::Pycodestyle,
            DiagnosticCode::W605 => CheckCategory::Pycodestyle,
            // flake8-2020
            DiagnosticCode::YTT101 => CheckCategory::Flake82020,
            DiagnosticCode::YTT102 => CheckCategory::Flake82020,
            DiagnosticCode::YTT103 => CheckCategory::Flake82020,
            DiagnosticCode::YTT201 => CheckCategory::Flake82020,
            DiagnosticCode::YTT202 => CheckCategory::Flake82020,
            DiagnosticCode::YTT203 => CheckCategory::Flake82020,
            DiagnosticCode::YTT204 => CheckCategory::Flake82020,
            DiagnosticCode::YTT301 => CheckCategory::Flake82020,
            DiagnosticCode::YTT302 => CheckCategory::Flake82020,
            DiagnosticCode::YTT303 => CheckCategory::Flake82020,
            // flake8-pie
            DiagnosticCode::PIE790 => CheckCategory::Flake8Pie,
            DiagnosticCode::PIE794 => CheckCategory::Flake8Pie,
            DiagnosticCode::PIE807 => CheckCategory::Flake8Pie,
            // Ruff
            DiagnosticCode::RUF001 => CheckCategory::Ruff,
            DiagnosticCode::RUF002 => CheckCategory::Ruff,
            DiagnosticCode::RUF003 => CheckCategory::Ruff,
            DiagnosticCode::RUF004 => CheckCategory::Ruff,
            DiagnosticCode::RUF100 => CheckCategory::Ruff,
        }
    }
}

impl DiagnosticKind {
    /// The summary text for the diagnostic. Typically a truncated form of the
    /// body text.
    pub fn summary(&self) -> String {
        match self {
            DiagnosticKind::UnaryPrefixIncrement(..) => {
                "Python does not support the unary prefix increment".to_string()
            }
            DiagnosticKind::UnusedLoopControlVariable(violations::UnusedLoopControlVariable(
                name,
            )) => {
                format!("Loop control variable `{name}` not used within the loop body")
            }
            DiagnosticKind::NoAssertRaisesException(..) => {
                "`assertRaises(Exception)` should be considered evil".to_string()
            }
            DiagnosticKind::StarArgUnpackingAfterKeywordArg(..) => {
                "Star-arg unpacking after a keyword argument is strongly discouraged".to_string()
            }

            // flake8-datetimez
            DiagnosticKind::CallDatetimeToday(..) => {
                "The use of `datetime.datetime.today()` is not allowed".to_string()
            }
            DiagnosticKind::CallDatetimeUtcnow(..) => {
                "The use of `datetime.datetime.utcnow()` is not allowed".to_string()
            }
            DiagnosticKind::CallDatetimeUtcfromtimestamp(..) => {
                "The use of `datetime.datetime.utcfromtimestamp()` is not allowed".to_string()
            }
            DiagnosticKind::CallDateToday(..) => {
                "The use of `datetime.date.today()` is not allowed.".to_string()
            }
            DiagnosticKind::CallDateFromtimestamp(..) => {
                "The use of `datetime.date.fromtimestamp()` is not allowed".to_string()
            }
            _ => self.body(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
    pub parent: Option<Location>,
}

impl Diagnostic {
    pub fn new<K: Into<DiagnosticKind>>(kind: K, range: Range) -> Self {
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
pub const INCOMPATIBLE_CODES: &[(DiagnosticCode, DiagnosticCode, &str)] = &[(
    DiagnosticCode::D203,
    DiagnosticCode::D211,
    "`D203` (OneBlankLineBeforeClass) and `D211` (NoBlankLinesBeforeClass) are incompatible. \
     Consider adding `D203` to `ignore`.",
)];

/// A hash map from deprecated to latest `DiagnosticCode`.
pub static CODE_REDIRECTS: Lazy<FxHashMap<&'static str, DiagnosticCode>> = Lazy::new(|| {
    FxHashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U001", DiagnosticCode::UP001),
        ("U003", DiagnosticCode::UP003),
        ("U004", DiagnosticCode::UP004),
        ("U005", DiagnosticCode::UP005),
        ("U006", DiagnosticCode::UP006),
        ("U007", DiagnosticCode::UP007),
        ("U008", DiagnosticCode::UP008),
        ("U009", DiagnosticCode::UP009),
        ("U010", DiagnosticCode::UP010),
        ("U011", DiagnosticCode::UP011),
        ("U012", DiagnosticCode::UP012),
        ("U013", DiagnosticCode::UP013),
        ("U014", DiagnosticCode::UP014),
        ("U015", DiagnosticCode::UP015),
        ("U016", DiagnosticCode::UP016),
        ("U017", DiagnosticCode::UP017),
        ("U019", DiagnosticCode::UP019),
        // TODO(charlie): Remove by 2023-02-01.
        ("I252", DiagnosticCode::TID252),
        ("M001", DiagnosticCode::RUF100),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV002", DiagnosticCode::PD002),
        ("PDV003", DiagnosticCode::PD003),
        ("PDV004", DiagnosticCode::PD004),
        ("PDV007", DiagnosticCode::PD007),
        ("PDV008", DiagnosticCode::PD008),
        ("PDV009", DiagnosticCode::PD009),
        ("PDV010", DiagnosticCode::PD010),
        ("PDV011", DiagnosticCode::PD011),
        ("PDV012", DiagnosticCode::PD012),
        ("PDV013", DiagnosticCode::PD013),
        ("PDV015", DiagnosticCode::PD015),
        ("PDV901", DiagnosticCode::PD901),
        // TODO(charlie): Remove by 2023-02-01.
        ("R501", DiagnosticCode::RET501),
        ("R502", DiagnosticCode::RET502),
        ("R503", DiagnosticCode::RET503),
        ("R504", DiagnosticCode::RET504),
        ("R505", DiagnosticCode::RET505),
        ("R506", DiagnosticCode::RET506),
        ("R507", DiagnosticCode::RET507),
        ("R508", DiagnosticCode::RET508),
        // TODO(charlie): Remove by 2023-02-01.
        ("IC001", DiagnosticCode::ICN001),
        ("IC002", DiagnosticCode::ICN001),
        ("IC003", DiagnosticCode::ICN001),
        ("IC004", DiagnosticCode::ICN001),
    ])
});

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use strum::IntoEnumIterator;

    use crate::registry::DiagnosticCode;

    #[test]
    fn check_code_serialization() {
        for check_code in DiagnosticCode::iter() {
            assert!(
                DiagnosticCode::from_str(check_code.as_ref()).is_ok(),
                "{check_code:?} could not be round-trip serialized."
            );
        }
    }

    #[test]
    fn fixable_codes() {
        for check_code in DiagnosticCode::iter() {
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
