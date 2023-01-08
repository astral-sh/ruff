//! Registry of `RuleCode` to `DiagnosticKind` mappings.

use std::fmt;

use once_cell::sync::Lazy;
use ruff_macros::RuleCodePrefix;
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
            RuleCodePrefix,
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
        pub enum RuleCode {
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

        impl RuleCode {
            /// A placeholder representation of the `DiagnosticKind` for the diagnostic.
            pub fn kind(&self) -> DiagnosticKind {
                match self {
                    $(
                        RuleCode::$code => DiagnosticKind::$name(<$mod::$name as Violation>::placeholder()),
                    )+
                }
            }
        }

        impl DiagnosticKind {
            /// A four-letter shorthand code for the diagnostic.
            pub fn code(&self) -> &'static RuleCode {
                match self {
                    $(
                        DiagnosticKind::$name(..) => &RuleCode::$code,
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

            /// Whether the diagnostic is (potentially) fixable.
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
pub enum RuleOrigin {
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

impl RuleOrigin {
    pub fn title(&self) -> &'static str {
        match self {
            RuleOrigin::Eradicate => "eradicate",
            RuleOrigin::Flake82020 => "flake8-2020",
            RuleOrigin::Flake8Annotations => "flake8-annotations",
            RuleOrigin::Flake8Bandit => "flake8-bandit",
            RuleOrigin::Flake8BlindExcept => "flake8-blind-except",
            RuleOrigin::Flake8BooleanTrap => "flake8-boolean-trap",
            RuleOrigin::Flake8Bugbear => "flake8-bugbear",
            RuleOrigin::Flake8Builtins => "flake8-builtins",
            RuleOrigin::Flake8Comprehensions => "flake8-comprehensions",
            RuleOrigin::Flake8Debugger => "flake8-debugger",
            RuleOrigin::Flake8ErrMsg => "flake8-errmsg",
            RuleOrigin::Flake8ImplicitStrConcat => "flake8-implicit-str-concat",
            RuleOrigin::Flake8ImportConventions => "flake8-import-conventions",
            RuleOrigin::Flake8Print => "flake8-print",
            RuleOrigin::Flake8PytestStyle => "flake8-pytest-style",
            RuleOrigin::Flake8Quotes => "flake8-quotes",
            RuleOrigin::Flake8Return => "flake8-return",
            RuleOrigin::Flake8TidyImports => "flake8-tidy-imports",
            RuleOrigin::Flake8Simplify => "flake8-simplify",
            RuleOrigin::Flake8UnusedArguments => "flake8-unused-arguments",
            RuleOrigin::Flake8Datetimez => "flake8-datetimez",
            RuleOrigin::Isort => "isort",
            RuleOrigin::McCabe => "mccabe",
            RuleOrigin::PandasVet => "pandas-vet",
            RuleOrigin::PEP8Naming => "pep8-naming",
            RuleOrigin::Pycodestyle => "pycodestyle",
            RuleOrigin::Pydocstyle => "pydocstyle",
            RuleOrigin::Pyflakes => "Pyflakes",
            RuleOrigin::PygrepHooks => "pygrep-hooks",
            RuleOrigin::Pylint => "Pylint",
            RuleOrigin::Pyupgrade => "pyupgrade",
            RuleOrigin::Flake8Pie => "flake8-pie",
            RuleOrigin::Ruff => "Ruff-specific rules",
        }
    }

    pub fn codes(&self) -> Vec<RuleCodePrefix> {
        match self {
            RuleOrigin::Eradicate => vec![RuleCodePrefix::ERA],
            RuleOrigin::Flake82020 => vec![RuleCodePrefix::YTT],
            RuleOrigin::Flake8Annotations => vec![RuleCodePrefix::ANN],
            RuleOrigin::Flake8Bandit => vec![RuleCodePrefix::S],
            RuleOrigin::Flake8BlindExcept => vec![RuleCodePrefix::BLE],
            RuleOrigin::Flake8BooleanTrap => vec![RuleCodePrefix::FBT],
            RuleOrigin::Flake8Bugbear => vec![RuleCodePrefix::B],
            RuleOrigin::Flake8Builtins => vec![RuleCodePrefix::A],
            RuleOrigin::Flake8Comprehensions => vec![RuleCodePrefix::C4],
            RuleOrigin::Flake8Datetimez => vec![RuleCodePrefix::DTZ],
            RuleOrigin::Flake8Debugger => vec![RuleCodePrefix::T10],
            RuleOrigin::Flake8ErrMsg => vec![RuleCodePrefix::EM],
            RuleOrigin::Flake8ImplicitStrConcat => vec![RuleCodePrefix::ISC],
            RuleOrigin::Flake8ImportConventions => vec![RuleCodePrefix::ICN],
            RuleOrigin::Flake8Print => vec![RuleCodePrefix::T20],
            RuleOrigin::Flake8PytestStyle => vec![RuleCodePrefix::PT],
            RuleOrigin::Flake8Quotes => vec![RuleCodePrefix::Q],
            RuleOrigin::Flake8Return => vec![RuleCodePrefix::RET],
            RuleOrigin::Flake8Simplify => vec![RuleCodePrefix::SIM],
            RuleOrigin::Flake8TidyImports => vec![RuleCodePrefix::TID],
            RuleOrigin::Flake8UnusedArguments => vec![RuleCodePrefix::ARG],
            RuleOrigin::Isort => vec![RuleCodePrefix::I],
            RuleOrigin::McCabe => vec![RuleCodePrefix::C90],
            RuleOrigin::PEP8Naming => vec![RuleCodePrefix::N],
            RuleOrigin::PandasVet => vec![RuleCodePrefix::PD],
            RuleOrigin::Pycodestyle => vec![RuleCodePrefix::E, RuleCodePrefix::W],
            RuleOrigin::Pydocstyle => vec![RuleCodePrefix::D],
            RuleOrigin::Pyflakes => vec![RuleCodePrefix::F],
            RuleOrigin::PygrepHooks => vec![RuleCodePrefix::PGH],
            RuleOrigin::Pylint => vec![
                RuleCodePrefix::PLC,
                RuleCodePrefix::PLE,
                RuleCodePrefix::PLR,
                RuleCodePrefix::PLW,
            ],
            RuleOrigin::Pyupgrade => vec![RuleCodePrefix::UP],
            RuleOrigin::Flake8Pie => vec![RuleCodePrefix::PIE],
            RuleOrigin::Ruff => vec![RuleCodePrefix::RUF],
        }
    }

    pub fn url(&self) -> Option<(&'static str, &'static Platform)> {
        match self {
            RuleOrigin::Eradicate => {
                Some(("https://pypi.org/project/eradicate/2.1.0/", &Platform::PyPI))
            }
            RuleOrigin::Flake82020 => Some((
                "https://pypi.org/project/flake8-2020/1.7.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Annotations => Some((
                "https://pypi.org/project/flake8-annotations/2.9.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Bandit => Some((
                "https://pypi.org/project/flake8-bandit/4.1.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8BlindExcept => Some((
                "https://pypi.org/project/flake8-blind-except/0.2.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8BooleanTrap => Some((
                "https://pypi.org/project/flake8-boolean-trap/0.1.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Bugbear => Some((
                "https://pypi.org/project/flake8-bugbear/22.10.27/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Builtins => Some((
                "https://pypi.org/project/flake8-builtins/2.0.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Comprehensions => Some((
                "https://pypi.org/project/flake8-comprehensions/3.10.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Debugger => Some((
                "https://pypi.org/project/flake8-debugger/4.1.2/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8ErrMsg => Some((
                "https://pypi.org/project/flake8-errmsg/0.4.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8ImplicitStrConcat => Some((
                "https://pypi.org/project/flake8-implicit-str-concat/0.3.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8ImportConventions => None,
            RuleOrigin::Flake8Print => Some((
                "https://pypi.org/project/flake8-print/5.0.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8PytestStyle => Some((
                "https://pypi.org/project/flake8-pytest-style/1.6.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Quotes => Some((
                "https://pypi.org/project/flake8-quotes/3.3.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Return => Some((
                "https://pypi.org/project/flake8-return/1.2.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Simplify => Some((
                "https://pypi.org/project/flake8-simplify/0.19.3/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8TidyImports => Some((
                "https://pypi.org/project/flake8-tidy-imports/4.8.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8UnusedArguments => Some((
                "https://pypi.org/project/flake8-unused-arguments/0.0.12/",
                &Platform::PyPI,
            )),
            RuleOrigin::Flake8Datetimez => Some((
                "https://pypi.org/project/flake8-datetimez/20.10.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Isort => Some(("https://pypi.org/project/isort/5.10.1/", &Platform::PyPI)),
            RuleOrigin::McCabe => Some(("https://pypi.org/project/mccabe/0.7.0/", &Platform::PyPI)),
            RuleOrigin::PandasVet => Some((
                "https://pypi.org/project/pandas-vet/0.2.3/",
                &Platform::PyPI,
            )),
            RuleOrigin::PEP8Naming => Some((
                "https://pypi.org/project/pep8-naming/0.13.2/",
                &Platform::PyPI,
            )),
            RuleOrigin::Pycodestyle => Some((
                "https://pypi.org/project/pycodestyle/2.9.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Pydocstyle => Some((
                "https://pypi.org/project/pydocstyle/6.1.1/",
                &Platform::PyPI,
            )),
            RuleOrigin::Pyflakes => {
                Some(("https://pypi.org/project/pyflakes/2.5.0/", &Platform::PyPI))
            }
            RuleOrigin::Pylint => {
                Some(("https://pypi.org/project/pylint/2.15.7/", &Platform::PyPI))
            }
            RuleOrigin::PygrepHooks => Some((
                "https://github.com/pre-commit/pygrep-hooks",
                &Platform::GitHub,
            )),
            RuleOrigin::Pyupgrade => {
                Some(("https://pypi.org/project/pyupgrade/3.2.0/", &Platform::PyPI))
            }
            RuleOrigin::Flake8Pie => Some((
                "https://pypi.org/project/flake8-pie/0.16.0/",
                &Platform::PyPI,
            )),
            RuleOrigin::Ruff => None,
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

impl RuleCode {
    /// The source for the diagnostic (either the AST, the filesystem, or the
    /// physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            RuleCode::RUF100 => &LintSource::NoQA,
            RuleCode::E501
            | RuleCode::W292
            | RuleCode::UP009
            | RuleCode::PGH003
            | RuleCode::PGH004 => &LintSource::Lines,
            RuleCode::ERA001
            | RuleCode::ISC001
            | RuleCode::ISC002
            | RuleCode::Q000
            | RuleCode::Q001
            | RuleCode::Q002
            | RuleCode::Q003
            | RuleCode::W605
            | RuleCode::RUF001
            | RuleCode::RUF002
            | RuleCode::RUF003 => &LintSource::Tokens,
            RuleCode::E902 => &LintSource::FileSystem,
            RuleCode::I001 => &LintSource::Imports,
            _ => &LintSource::AST,
        }
    }

    pub fn origin(&self) -> RuleOrigin {
        #[allow(clippy::match_same_arms)]
        match self {
            // flake8-builtins
            RuleCode::A001 => RuleOrigin::Flake8Builtins,
            RuleCode::A002 => RuleOrigin::Flake8Builtins,
            RuleCode::A003 => RuleOrigin::Flake8Builtins,
            // flake8-annotations
            RuleCode::ANN001 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN002 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN003 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN101 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN102 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN201 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN202 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN204 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN205 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN206 => RuleOrigin::Flake8Annotations,
            RuleCode::ANN401 => RuleOrigin::Flake8Annotations,
            // flake8-unused-arguments
            RuleCode::ARG001 => RuleOrigin::Flake8UnusedArguments,
            RuleCode::ARG002 => RuleOrigin::Flake8UnusedArguments,
            RuleCode::ARG003 => RuleOrigin::Flake8UnusedArguments,
            RuleCode::ARG004 => RuleOrigin::Flake8UnusedArguments,
            RuleCode::ARG005 => RuleOrigin::Flake8UnusedArguments,
            // flake8-bugbear
            RuleCode::B002 => RuleOrigin::Flake8Bugbear,
            RuleCode::B003 => RuleOrigin::Flake8Bugbear,
            RuleCode::B004 => RuleOrigin::Flake8Bugbear,
            RuleCode::B005 => RuleOrigin::Flake8Bugbear,
            RuleCode::B006 => RuleOrigin::Flake8Bugbear,
            RuleCode::B007 => RuleOrigin::Flake8Bugbear,
            RuleCode::B008 => RuleOrigin::Flake8Bugbear,
            RuleCode::B009 => RuleOrigin::Flake8Bugbear,
            RuleCode::B010 => RuleOrigin::Flake8Bugbear,
            RuleCode::B011 => RuleOrigin::Flake8Bugbear,
            RuleCode::B012 => RuleOrigin::Flake8Bugbear,
            RuleCode::B013 => RuleOrigin::Flake8Bugbear,
            RuleCode::B014 => RuleOrigin::Flake8Bugbear,
            RuleCode::B015 => RuleOrigin::Flake8Bugbear,
            RuleCode::B016 => RuleOrigin::Flake8Bugbear,
            RuleCode::B017 => RuleOrigin::Flake8Bugbear,
            RuleCode::B018 => RuleOrigin::Flake8Bugbear,
            RuleCode::B019 => RuleOrigin::Flake8Bugbear,
            RuleCode::B020 => RuleOrigin::Flake8Bugbear,
            RuleCode::B021 => RuleOrigin::Flake8Bugbear,
            RuleCode::B022 => RuleOrigin::Flake8Bugbear,
            RuleCode::B023 => RuleOrigin::Flake8Bugbear,
            RuleCode::B024 => RuleOrigin::Flake8Bugbear,
            RuleCode::B025 => RuleOrigin::Flake8Bugbear,
            RuleCode::B026 => RuleOrigin::Flake8Bugbear,
            RuleCode::B027 => RuleOrigin::Flake8Bugbear,
            RuleCode::B904 => RuleOrigin::Flake8Bugbear,
            RuleCode::B905 => RuleOrigin::Flake8Bugbear,
            // flake8-blind-except
            RuleCode::BLE001 => RuleOrigin::Flake8BlindExcept,
            // flake8-comprehensions
            RuleCode::C400 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C401 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C402 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C403 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C404 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C405 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C406 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C408 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C409 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C410 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C411 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C413 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C414 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C415 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C416 => RuleOrigin::Flake8Comprehensions,
            RuleCode::C417 => RuleOrigin::Flake8Comprehensions,
            // mccabe
            RuleCode::C901 => RuleOrigin::McCabe,
            // pydocstyle
            RuleCode::D100 => RuleOrigin::Pydocstyle,
            RuleCode::D101 => RuleOrigin::Pydocstyle,
            RuleCode::D102 => RuleOrigin::Pydocstyle,
            RuleCode::D103 => RuleOrigin::Pydocstyle,
            RuleCode::D104 => RuleOrigin::Pydocstyle,
            RuleCode::D105 => RuleOrigin::Pydocstyle,
            RuleCode::D106 => RuleOrigin::Pydocstyle,
            RuleCode::D107 => RuleOrigin::Pydocstyle,
            RuleCode::D200 => RuleOrigin::Pydocstyle,
            RuleCode::D201 => RuleOrigin::Pydocstyle,
            RuleCode::D202 => RuleOrigin::Pydocstyle,
            RuleCode::D203 => RuleOrigin::Pydocstyle,
            RuleCode::D204 => RuleOrigin::Pydocstyle,
            RuleCode::D205 => RuleOrigin::Pydocstyle,
            RuleCode::D206 => RuleOrigin::Pydocstyle,
            RuleCode::D207 => RuleOrigin::Pydocstyle,
            RuleCode::D208 => RuleOrigin::Pydocstyle,
            RuleCode::D209 => RuleOrigin::Pydocstyle,
            RuleCode::D210 => RuleOrigin::Pydocstyle,
            RuleCode::D211 => RuleOrigin::Pydocstyle,
            RuleCode::D212 => RuleOrigin::Pydocstyle,
            RuleCode::D213 => RuleOrigin::Pydocstyle,
            RuleCode::D214 => RuleOrigin::Pydocstyle,
            RuleCode::D215 => RuleOrigin::Pydocstyle,
            RuleCode::D300 => RuleOrigin::Pydocstyle,
            RuleCode::D301 => RuleOrigin::Pydocstyle,
            RuleCode::D400 => RuleOrigin::Pydocstyle,
            RuleCode::D402 => RuleOrigin::Pydocstyle,
            RuleCode::D403 => RuleOrigin::Pydocstyle,
            RuleCode::D404 => RuleOrigin::Pydocstyle,
            RuleCode::D405 => RuleOrigin::Pydocstyle,
            RuleCode::D406 => RuleOrigin::Pydocstyle,
            RuleCode::D407 => RuleOrigin::Pydocstyle,
            RuleCode::D408 => RuleOrigin::Pydocstyle,
            RuleCode::D409 => RuleOrigin::Pydocstyle,
            RuleCode::D410 => RuleOrigin::Pydocstyle,
            RuleCode::D411 => RuleOrigin::Pydocstyle,
            RuleCode::D412 => RuleOrigin::Pydocstyle,
            RuleCode::D413 => RuleOrigin::Pydocstyle,
            RuleCode::D414 => RuleOrigin::Pydocstyle,
            RuleCode::D415 => RuleOrigin::Pydocstyle,
            RuleCode::D416 => RuleOrigin::Pydocstyle,
            RuleCode::D417 => RuleOrigin::Pydocstyle,
            RuleCode::D418 => RuleOrigin::Pydocstyle,
            RuleCode::D419 => RuleOrigin::Pydocstyle,
            // flake8-datetimez
            RuleCode::DTZ001 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ002 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ003 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ004 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ005 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ006 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ007 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ011 => RuleOrigin::Flake8Datetimez,
            RuleCode::DTZ012 => RuleOrigin::Flake8Datetimez,
            // pycodestyle (errors)
            RuleCode::E401 => RuleOrigin::Pycodestyle,
            RuleCode::E402 => RuleOrigin::Pycodestyle,
            RuleCode::E501 => RuleOrigin::Pycodestyle,
            RuleCode::E711 => RuleOrigin::Pycodestyle,
            RuleCode::E712 => RuleOrigin::Pycodestyle,
            RuleCode::E713 => RuleOrigin::Pycodestyle,
            RuleCode::E714 => RuleOrigin::Pycodestyle,
            RuleCode::E721 => RuleOrigin::Pycodestyle,
            RuleCode::E722 => RuleOrigin::Pycodestyle,
            RuleCode::E731 => RuleOrigin::Pycodestyle,
            RuleCode::E741 => RuleOrigin::Pycodestyle,
            RuleCode::E742 => RuleOrigin::Pycodestyle,
            RuleCode::E743 => RuleOrigin::Pycodestyle,
            RuleCode::E902 => RuleOrigin::Pycodestyle,
            RuleCode::E999 => RuleOrigin::Pycodestyle,
            // flake8-errmsg
            RuleCode::EM101 => RuleOrigin::Flake8ErrMsg,
            RuleCode::EM102 => RuleOrigin::Flake8ErrMsg,
            RuleCode::EM103 => RuleOrigin::Flake8ErrMsg,
            // eradicate
            RuleCode::ERA001 => RuleOrigin::Eradicate,
            // pyflakes
            RuleCode::F401 => RuleOrigin::Pyflakes,
            RuleCode::F402 => RuleOrigin::Pyflakes,
            RuleCode::F403 => RuleOrigin::Pyflakes,
            RuleCode::F404 => RuleOrigin::Pyflakes,
            RuleCode::F405 => RuleOrigin::Pyflakes,
            RuleCode::F406 => RuleOrigin::Pyflakes,
            RuleCode::F407 => RuleOrigin::Pyflakes,
            RuleCode::F501 => RuleOrigin::Pyflakes,
            RuleCode::F502 => RuleOrigin::Pyflakes,
            RuleCode::F503 => RuleOrigin::Pyflakes,
            RuleCode::F504 => RuleOrigin::Pyflakes,
            RuleCode::F505 => RuleOrigin::Pyflakes,
            RuleCode::F506 => RuleOrigin::Pyflakes,
            RuleCode::F507 => RuleOrigin::Pyflakes,
            RuleCode::F508 => RuleOrigin::Pyflakes,
            RuleCode::F509 => RuleOrigin::Pyflakes,
            RuleCode::F521 => RuleOrigin::Pyflakes,
            RuleCode::F522 => RuleOrigin::Pyflakes,
            RuleCode::F523 => RuleOrigin::Pyflakes,
            RuleCode::F524 => RuleOrigin::Pyflakes,
            RuleCode::F525 => RuleOrigin::Pyflakes,
            RuleCode::F541 => RuleOrigin::Pyflakes,
            RuleCode::F601 => RuleOrigin::Pyflakes,
            RuleCode::F602 => RuleOrigin::Pyflakes,
            RuleCode::F621 => RuleOrigin::Pyflakes,
            RuleCode::F622 => RuleOrigin::Pyflakes,
            RuleCode::F631 => RuleOrigin::Pyflakes,
            RuleCode::F632 => RuleOrigin::Pyflakes,
            RuleCode::F633 => RuleOrigin::Pyflakes,
            RuleCode::F634 => RuleOrigin::Pyflakes,
            RuleCode::F701 => RuleOrigin::Pyflakes,
            RuleCode::F702 => RuleOrigin::Pyflakes,
            RuleCode::F704 => RuleOrigin::Pyflakes,
            RuleCode::F706 => RuleOrigin::Pyflakes,
            RuleCode::F707 => RuleOrigin::Pyflakes,
            RuleCode::F722 => RuleOrigin::Pyflakes,
            RuleCode::F811 => RuleOrigin::Pyflakes,
            RuleCode::F821 => RuleOrigin::Pyflakes,
            RuleCode::F822 => RuleOrigin::Pyflakes,
            RuleCode::F823 => RuleOrigin::Pyflakes,
            RuleCode::F841 => RuleOrigin::Pyflakes,
            RuleCode::F842 => RuleOrigin::Pyflakes,
            RuleCode::F901 => RuleOrigin::Pyflakes,
            // flake8-boolean-trap
            RuleCode::FBT001 => RuleOrigin::Flake8BooleanTrap,
            RuleCode::FBT002 => RuleOrigin::Flake8BooleanTrap,
            RuleCode::FBT003 => RuleOrigin::Flake8BooleanTrap,
            // isort
            RuleCode::I001 => RuleOrigin::Isort,
            // flake8-import-conventions
            RuleCode::ICN001 => RuleOrigin::Flake8ImportConventions,
            // flake8-implicit-str-concat
            RuleCode::ISC001 => RuleOrigin::Flake8ImplicitStrConcat,
            RuleCode::ISC002 => RuleOrigin::Flake8ImplicitStrConcat,
            RuleCode::ISC003 => RuleOrigin::Flake8ImplicitStrConcat,
            // pep8-naming
            RuleCode::N801 => RuleOrigin::PEP8Naming,
            RuleCode::N802 => RuleOrigin::PEP8Naming,
            RuleCode::N803 => RuleOrigin::PEP8Naming,
            RuleCode::N804 => RuleOrigin::PEP8Naming,
            RuleCode::N805 => RuleOrigin::PEP8Naming,
            RuleCode::N806 => RuleOrigin::PEP8Naming,
            RuleCode::N807 => RuleOrigin::PEP8Naming,
            RuleCode::N811 => RuleOrigin::PEP8Naming,
            RuleCode::N812 => RuleOrigin::PEP8Naming,
            RuleCode::N813 => RuleOrigin::PEP8Naming,
            RuleCode::N814 => RuleOrigin::PEP8Naming,
            RuleCode::N815 => RuleOrigin::PEP8Naming,
            RuleCode::N816 => RuleOrigin::PEP8Naming,
            RuleCode::N817 => RuleOrigin::PEP8Naming,
            RuleCode::N818 => RuleOrigin::PEP8Naming,
            // pandas-vet
            RuleCode::PD002 => RuleOrigin::PandasVet,
            RuleCode::PD003 => RuleOrigin::PandasVet,
            RuleCode::PD004 => RuleOrigin::PandasVet,
            RuleCode::PD007 => RuleOrigin::PandasVet,
            RuleCode::PD008 => RuleOrigin::PandasVet,
            RuleCode::PD009 => RuleOrigin::PandasVet,
            RuleCode::PD010 => RuleOrigin::PandasVet,
            RuleCode::PD011 => RuleOrigin::PandasVet,
            RuleCode::PD012 => RuleOrigin::PandasVet,
            RuleCode::PD013 => RuleOrigin::PandasVet,
            RuleCode::PD015 => RuleOrigin::PandasVet,
            RuleCode::PD901 => RuleOrigin::PandasVet,
            // pygrep-hooks
            RuleCode::PGH001 => RuleOrigin::PygrepHooks,
            RuleCode::PGH002 => RuleOrigin::PygrepHooks,
            RuleCode::PGH003 => RuleOrigin::PygrepHooks,
            RuleCode::PGH004 => RuleOrigin::PygrepHooks,
            // pylint
            RuleCode::PLC0414 => RuleOrigin::Pylint,
            RuleCode::PLC2201 => RuleOrigin::Pylint,
            RuleCode::PLC3002 => RuleOrigin::Pylint,
            RuleCode::PLE0117 => RuleOrigin::Pylint,
            RuleCode::PLE0118 => RuleOrigin::Pylint,
            RuleCode::PLE1142 => RuleOrigin::Pylint,
            RuleCode::PLR0206 => RuleOrigin::Pylint,
            RuleCode::PLR0402 => RuleOrigin::Pylint,
            RuleCode::PLR1701 => RuleOrigin::Pylint,
            RuleCode::PLR1722 => RuleOrigin::Pylint,
            RuleCode::PLW0120 => RuleOrigin::Pylint,
            RuleCode::PLW0602 => RuleOrigin::Pylint,
            // flake8-pytest-style
            RuleCode::PT001 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT002 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT003 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT004 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT005 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT006 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT007 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT008 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT009 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT010 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT011 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT012 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT013 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT015 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT016 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT017 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT018 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT019 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT020 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT021 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT022 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT023 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT024 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT025 => RuleOrigin::Flake8PytestStyle,
            RuleCode::PT026 => RuleOrigin::Flake8PytestStyle,
            // flake8-quotes
            RuleCode::Q000 => RuleOrigin::Flake8Quotes,
            RuleCode::Q001 => RuleOrigin::Flake8Quotes,
            RuleCode::Q002 => RuleOrigin::Flake8Quotes,
            RuleCode::Q003 => RuleOrigin::Flake8Quotes,
            // flake8-return
            RuleCode::RET501 => RuleOrigin::Flake8Return,
            RuleCode::RET502 => RuleOrigin::Flake8Return,
            RuleCode::RET503 => RuleOrigin::Flake8Return,
            RuleCode::RET504 => RuleOrigin::Flake8Return,
            RuleCode::RET505 => RuleOrigin::Flake8Return,
            RuleCode::RET506 => RuleOrigin::Flake8Return,
            RuleCode::RET507 => RuleOrigin::Flake8Return,
            RuleCode::RET508 => RuleOrigin::Flake8Return,
            // flake8-bandit
            RuleCode::S101 => RuleOrigin::Flake8Bandit,
            RuleCode::S102 => RuleOrigin::Flake8Bandit,
            RuleCode::S103 => RuleOrigin::Flake8Bandit,
            RuleCode::S104 => RuleOrigin::Flake8Bandit,
            RuleCode::S105 => RuleOrigin::Flake8Bandit,
            RuleCode::S106 => RuleOrigin::Flake8Bandit,
            RuleCode::S107 => RuleOrigin::Flake8Bandit,
            RuleCode::S108 => RuleOrigin::Flake8Bandit,
            RuleCode::S113 => RuleOrigin::Flake8Bandit,
            RuleCode::S324 => RuleOrigin::Flake8Bandit,
            RuleCode::S501 => RuleOrigin::Flake8Bandit,
            RuleCode::S506 => RuleOrigin::Flake8Bandit,
            // flake8-simplify
            RuleCode::SIM103 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM101 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM102 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM105 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM107 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM108 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM109 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM110 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM111 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM117 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM118 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM201 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM202 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM208 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM210 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM211 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM212 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM220 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM221 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM222 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM223 => RuleOrigin::Flake8Simplify,
            RuleCode::SIM300 => RuleOrigin::Flake8Simplify,
            // flake8-debugger
            RuleCode::T100 => RuleOrigin::Flake8Debugger,
            // flake8-print
            RuleCode::T201 => RuleOrigin::Flake8Print,
            RuleCode::T203 => RuleOrigin::Flake8Print,
            // flake8-tidy-imports
            RuleCode::TID251 => RuleOrigin::Flake8TidyImports,
            RuleCode::TID252 => RuleOrigin::Flake8TidyImports,
            // pyupgrade
            RuleCode::UP001 => RuleOrigin::Pyupgrade,
            RuleCode::UP003 => RuleOrigin::Pyupgrade,
            RuleCode::UP004 => RuleOrigin::Pyupgrade,
            RuleCode::UP005 => RuleOrigin::Pyupgrade,
            RuleCode::UP006 => RuleOrigin::Pyupgrade,
            RuleCode::UP007 => RuleOrigin::Pyupgrade,
            RuleCode::UP008 => RuleOrigin::Pyupgrade,
            RuleCode::UP009 => RuleOrigin::Pyupgrade,
            RuleCode::UP010 => RuleOrigin::Pyupgrade,
            RuleCode::UP011 => RuleOrigin::Pyupgrade,
            RuleCode::UP012 => RuleOrigin::Pyupgrade,
            RuleCode::UP013 => RuleOrigin::Pyupgrade,
            RuleCode::UP014 => RuleOrigin::Pyupgrade,
            RuleCode::UP015 => RuleOrigin::Pyupgrade,
            RuleCode::UP016 => RuleOrigin::Pyupgrade,
            RuleCode::UP017 => RuleOrigin::Pyupgrade,
            RuleCode::UP018 => RuleOrigin::Pyupgrade,
            RuleCode::UP019 => RuleOrigin::Pyupgrade,
            RuleCode::UP020 => RuleOrigin::Pyupgrade,
            RuleCode::UP021 => RuleOrigin::Pyupgrade,
            RuleCode::UP022 => RuleOrigin::Pyupgrade,
            RuleCode::UP023 => RuleOrigin::Pyupgrade,
            RuleCode::UP024 => RuleOrigin::Pyupgrade,
            RuleCode::UP025 => RuleOrigin::Pyupgrade,
            RuleCode::UP026 => RuleOrigin::Pyupgrade,
            RuleCode::UP027 => RuleOrigin::Pyupgrade,
            RuleCode::UP028 => RuleOrigin::Pyupgrade,
            RuleCode::UP029 => RuleOrigin::Pyupgrade,
            // pycodestyle (warnings)
            RuleCode::W292 => RuleOrigin::Pycodestyle,
            RuleCode::W605 => RuleOrigin::Pycodestyle,
            // flake8-2020
            RuleCode::YTT101 => RuleOrigin::Flake82020,
            RuleCode::YTT102 => RuleOrigin::Flake82020,
            RuleCode::YTT103 => RuleOrigin::Flake82020,
            RuleCode::YTT201 => RuleOrigin::Flake82020,
            RuleCode::YTT202 => RuleOrigin::Flake82020,
            RuleCode::YTT203 => RuleOrigin::Flake82020,
            RuleCode::YTT204 => RuleOrigin::Flake82020,
            RuleCode::YTT301 => RuleOrigin::Flake82020,
            RuleCode::YTT302 => RuleOrigin::Flake82020,
            RuleCode::YTT303 => RuleOrigin::Flake82020,
            // flake8-pie
            RuleCode::PIE790 => RuleOrigin::Flake8Pie,
            RuleCode::PIE794 => RuleOrigin::Flake8Pie,
            RuleCode::PIE807 => RuleOrigin::Flake8Pie,
            // Ruff
            RuleCode::RUF001 => RuleOrigin::Ruff,
            RuleCode::RUF002 => RuleOrigin::Ruff,
            RuleCode::RUF003 => RuleOrigin::Ruff,
            RuleCode::RUF004 => RuleOrigin::Ruff,
            RuleCode::RUF100 => RuleOrigin::Ruff,
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
pub const INCOMPATIBLE_CODES: &[(RuleCode, RuleCode, &str)] = &[(
    RuleCode::D203,
    RuleCode::D211,
    "`D203` (OneBlankLineBeforeClass) and `D211` (NoBlankLinesBeforeClass) are incompatible. \
     Consider adding `D203` to `ignore`.",
)];

/// A hash map from deprecated to latest `RuleCode`.
pub static CODE_REDIRECTS: Lazy<FxHashMap<&'static str, RuleCode>> = Lazy::new(|| {
    FxHashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U001", RuleCode::UP001),
        ("U003", RuleCode::UP003),
        ("U004", RuleCode::UP004),
        ("U005", RuleCode::UP005),
        ("U006", RuleCode::UP006),
        ("U007", RuleCode::UP007),
        ("U008", RuleCode::UP008),
        ("U009", RuleCode::UP009),
        ("U010", RuleCode::UP010),
        ("U011", RuleCode::UP011),
        ("U012", RuleCode::UP012),
        ("U013", RuleCode::UP013),
        ("U014", RuleCode::UP014),
        ("U015", RuleCode::UP015),
        ("U016", RuleCode::UP016),
        ("U017", RuleCode::UP017),
        ("U019", RuleCode::UP019),
        // TODO(charlie): Remove by 2023-02-01.
        ("I252", RuleCode::TID252),
        ("M001", RuleCode::RUF100),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV002", RuleCode::PD002),
        ("PDV003", RuleCode::PD003),
        ("PDV004", RuleCode::PD004),
        ("PDV007", RuleCode::PD007),
        ("PDV008", RuleCode::PD008),
        ("PDV009", RuleCode::PD009),
        ("PDV010", RuleCode::PD010),
        ("PDV011", RuleCode::PD011),
        ("PDV012", RuleCode::PD012),
        ("PDV013", RuleCode::PD013),
        ("PDV015", RuleCode::PD015),
        ("PDV901", RuleCode::PD901),
        // TODO(charlie): Remove by 2023-02-01.
        ("R501", RuleCode::RET501),
        ("R502", RuleCode::RET502),
        ("R503", RuleCode::RET503),
        ("R504", RuleCode::RET504),
        ("R505", RuleCode::RET505),
        ("R506", RuleCode::RET506),
        ("R507", RuleCode::RET507),
        ("R508", RuleCode::RET508),
        // TODO(charlie): Remove by 2023-02-01.
        ("IC001", RuleCode::ICN001),
        ("IC002", RuleCode::ICN001),
        ("IC003", RuleCode::ICN001),
        ("IC004", RuleCode::ICN001),
    ])
});

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use strum::IntoEnumIterator;

    use crate::registry::RuleCode;

    #[test]
    fn check_code_serialization() {
        for check_code in RuleCode::iter() {
            assert!(
                RuleCode::from_str(check_code.as_ref()).is_ok(),
                "{check_code:?} could not be round-trip serialized."
            );
        }
    }

    #[test]
    fn fixable_codes() {
        for check_code in RuleCode::iter() {
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
