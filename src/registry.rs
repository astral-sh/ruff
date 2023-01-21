//! Registry of [`Rule`] to [`DiagnosticKind`] mappings.

use itertools::Itertools;
use once_cell::sync::Lazy;
use ruff_macros::ParseCode;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumIter};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::violation::Violation;
use crate::{rules, violations};

ruff_macros::define_rule_mapping!(
    // pycodestyle errors
    E101 => violations::MixedSpacesAndTabs,
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
    W505 => violations::DocLineTooLong,
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
    PLC3002 => violations::UnnecessaryDirectLambdaCall,
    PLE0117 => violations::NonlocalWithoutBinding,
    PLE0118 => violations::UsedPriorGlobalDeclaration,
    PLE1142 => violations::AwaitOutsideAsync,
    PLR0206 => violations::PropertyWithParameters,
    PLR0402 => violations::ConsiderUsingFromImport,
    PLR0133 => violations::ConstantComparison,
    PLR1701 => violations::ConsiderMergingIsinstance,
    PLR1722 => violations::UseSysExit,
    PLR2004 => violations::MagicValueComparison,
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
    TID251 => rules::flake8_tidy_imports::banned_api::BannedApi,
    TID252 => rules::flake8_tidy_imports::relative_imports::RelativeImports,
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
    SIM115 => violations::OpenFileWithContextHandler,
    SIM101 => violations::DuplicateIsinstanceCall,
    SIM102 => violations::NestedIfStatements,
    SIM103 => violations::ReturnBoolConditionDirectly,
    SIM105 => violations::UseContextlibSuppress,
    SIM107 => violations::ReturnInTryExceptFinally,
    SIM108 => violations::UseTernaryOperator,
    SIM109 => violations::CompareWithTuple,
    SIM110 => violations::ConvertLoopToAny,
    SIM111 => violations::ConvertLoopToAll,
    SIM112 => violations::UseCapitalEnvironmentVariables,
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
    SIM401 => violations::DictGetWithDefault,
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
    UP011 => violations::LRUCacheWithoutParameters,
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
    UP030 => violations::FormatLiterals,
    UP031 => violations::PrintfStringFormatting,
    UP032 => violations::FString,
    UP033 => violations::FunctoolsCache,
    UP034 => violations::ExtraneousParentheses,
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
    D401 => crate::rules::pydocstyle::rules::non_imperative_mood::NonImperativeMood,
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
    I002 => violations::MissingRequiredImport,
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
    S508 => violations::SnmpInsecureVersion,
    S509 => violations::SnmpWeakCryptography,
    S701 => violations::Jinja2AutoescapeFalse,
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
    PIE796 => violations::PreferUniqueEnums,
    PIE807 => violations::PreferListBuiltin,
    // flake8-commas
    COM812 => violations::TrailingCommaMissing,
    COM818 => violations::TrailingCommaOnBareTupleProhibited,
    COM819 => violations::TrailingCommaProhibited,
    // flake8-no-pep420
    INP001 => violations::ImplicitNamespacePackage,
    // flake8-executable
    EXE003 => rules::flake8_executable::rules::ShebangPython,
    EXE004 => rules::flake8_executable::rules::ShebangWhitespace,
    EXE005 => rules::flake8_executable::rules::ShebangNewline,
    // flake8-type-checking
    TYP005 => rules::flake8_type_checking::rules::EmptyTypeCheckingBlock,
    // tryceratops
    TRY300 => rules::tryceratops::rules::TryConsiderElse,
    // Ruff
    RUF001 => violations::AmbiguousUnicodeCharacterString,
    RUF002 => violations::AmbiguousUnicodeCharacterDocstring,
    RUF003 => violations::AmbiguousUnicodeCharacterComment,
    RUF004 => violations::KeywordArgumentBeforeStarArgument,
    RUF005 => violations::UnpackInsteadOfConcatenatingToCollectionLiteral,
    RUF100 => violations::UnusedNOQA,
);

#[derive(EnumIter, Debug, PartialEq, Eq, ParseCode)]
pub enum Linter {
    #[prefix = "F"]
    Pyflakes,
    #[prefix = "E"]
    #[prefix = "W"]
    Pycodestyle,
    #[prefix = "C9"]
    McCabe,
    #[prefix = "I"]
    Isort,
    #[prefix = "D"]
    Pydocstyle,
    #[prefix = "UP"]
    Pyupgrade,
    #[prefix = "N"]
    PEP8Naming,
    #[prefix = "YTT"]
    Flake82020,
    #[prefix = "ANN"]
    Flake8Annotations,
    #[prefix = "S"]
    Flake8Bandit,
    #[prefix = "BLE"]
    Flake8BlindExcept,
    #[prefix = "FBT"]
    Flake8BooleanTrap,
    #[prefix = "B"]
    Flake8Bugbear,
    #[prefix = "A"]
    Flake8Builtins,
    #[prefix = "C4"]
    Flake8Comprehensions,
    #[prefix = "T10"]
    Flake8Debugger,
    #[prefix = "EM"]
    Flake8ErrMsg,
    #[prefix = "ISC"]
    Flake8ImplicitStrConcat,
    #[prefix = "ICN"]
    Flake8ImportConventions,
    #[prefix = "T20"]
    Flake8Print,
    #[prefix = "PT"]
    Flake8PytestStyle,
    #[prefix = "Q"]
    Flake8Quotes,
    #[prefix = "RET"]
    Flake8Return,
    #[prefix = "SIM"]
    Flake8Simplify,
    #[prefix = "TID"]
    Flake8TidyImports,
    #[prefix = "ARG"]
    Flake8UnusedArguments,
    #[prefix = "DTZ"]
    Flake8Datetimez,
    #[prefix = "ERA"]
    Eradicate,
    #[prefix = "PD"]
    PandasVet,
    #[prefix = "PGH"]
    PygrepHooks,
    #[prefix = "PL"]
    Pylint,
    #[prefix = "PIE"]
    Flake8Pie,
    #[prefix = "COM"]
    Flake8Commas,
    #[prefix = "INP"]
    Flake8NoPep420,
    #[prefix = "EXE"]
    Flake8Executable,
    #[prefix = "TYP"]
    Flake8TypeChecking,
    #[prefix = "TRY"]
    Tryceratops,
    #[prefix = "RUF"]
    Ruff,
}

pub trait ParseCode: Sized {
    fn parse_code(code: &str) -> Option<(Self, &str)>;
}

pub enum Prefixes {
    Single(RuleSelector),
    Multiple(Vec<(RuleSelector, &'static str)>),
}

impl Prefixes {
    pub fn as_list(&self, separator: &str) -> String {
        match self {
            Prefixes::Single(prefix) => prefix.as_ref().to_string(),
            Prefixes::Multiple(entries) => entries
                .iter()
                .map(|(prefix, _)| prefix.as_ref())
                .join(separator),
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/linter.rs"));

impl Linter {
    pub fn prefixes(&self) -> Prefixes {
        match self {
            Linter::Eradicate => Prefixes::Single(RuleSelector::ERA),
            Linter::Flake82020 => Prefixes::Single(RuleSelector::YTT),
            Linter::Flake8Annotations => Prefixes::Single(RuleSelector::ANN),
            Linter::Flake8Bandit => Prefixes::Single(RuleSelector::S),
            Linter::Flake8BlindExcept => Prefixes::Single(RuleSelector::BLE),
            Linter::Flake8BooleanTrap => Prefixes::Single(RuleSelector::FBT),
            Linter::Flake8Bugbear => Prefixes::Single(RuleSelector::B),
            Linter::Flake8Builtins => Prefixes::Single(RuleSelector::A),
            Linter::Flake8Comprehensions => Prefixes::Single(RuleSelector::C4),
            Linter::Flake8Datetimez => Prefixes::Single(RuleSelector::DTZ),
            Linter::Flake8Debugger => Prefixes::Single(RuleSelector::T10),
            Linter::Flake8ErrMsg => Prefixes::Single(RuleSelector::EM),
            Linter::Flake8ImplicitStrConcat => Prefixes::Single(RuleSelector::ISC),
            Linter::Flake8ImportConventions => Prefixes::Single(RuleSelector::ICN),
            Linter::Flake8Print => Prefixes::Single(RuleSelector::T20),
            Linter::Flake8PytestStyle => Prefixes::Single(RuleSelector::PT),
            Linter::Flake8Quotes => Prefixes::Single(RuleSelector::Q),
            Linter::Flake8Return => Prefixes::Single(RuleSelector::RET),
            Linter::Flake8Simplify => Prefixes::Single(RuleSelector::SIM),
            Linter::Flake8TidyImports => Prefixes::Single(RuleSelector::TID),
            Linter::Flake8UnusedArguments => Prefixes::Single(RuleSelector::ARG),
            Linter::Isort => Prefixes::Single(RuleSelector::I),
            Linter::McCabe => Prefixes::Single(RuleSelector::C90),
            Linter::PEP8Naming => Prefixes::Single(RuleSelector::N),
            Linter::PandasVet => Prefixes::Single(RuleSelector::PD),
            Linter::Pycodestyle => Prefixes::Multiple(vec![
                (RuleSelector::E, "Error"),
                (RuleSelector::W, "Warning"),
            ]),
            Linter::Pydocstyle => Prefixes::Single(RuleSelector::D),
            Linter::Pyflakes => Prefixes::Single(RuleSelector::F),
            Linter::PygrepHooks => Prefixes::Single(RuleSelector::PGH),
            Linter::Pylint => Prefixes::Multiple(vec![
                (RuleSelector::PLC, "Convention"),
                (RuleSelector::PLE, "Error"),
                (RuleSelector::PLR, "Refactor"),
                (RuleSelector::PLW, "Warning"),
            ]),
            Linter::Pyupgrade => Prefixes::Single(RuleSelector::UP),
            Linter::Flake8Pie => Prefixes::Single(RuleSelector::PIE),
            Linter::Flake8Commas => Prefixes::Single(RuleSelector::COM),
            Linter::Flake8NoPep420 => Prefixes::Single(RuleSelector::INP),
            Linter::Flake8Executable => Prefixes::Single(RuleSelector::EXE),
            Linter::Flake8TypeChecking => Prefixes::Single(RuleSelector::TYP),
            Linter::Tryceratops => Prefixes::Single(RuleSelector::TRY),
            Linter::Ruff => Prefixes::Single(RuleSelector::RUF),
        }
    }
}

pub enum LintSource {
    Ast,
    Io,
    Lines,
    Tokens,
    Imports,
    NoQa,
    Filesystem,
}

impl Rule {
    /// The source for the diagnostic (either the AST, the filesystem, or the
    /// physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            Rule::UnusedNOQA => &LintSource::NoQa,
            Rule::BlanketNOQA
            | Rule::BlanketTypeIgnore
            | Rule::DocLineTooLong
            | Rule::LineTooLong
            | Rule::MixedSpacesAndTabs
            | Rule::NoNewLineAtEndOfFile
            | Rule::PEP3120UnnecessaryCodingComment
            | Rule::ShebangNewline
            | Rule::ShebangPython
            | Rule::ShebangWhitespace => &LintSource::Lines,
            Rule::AmbiguousUnicodeCharacterComment
            | Rule::AmbiguousUnicodeCharacterDocstring
            | Rule::AmbiguousUnicodeCharacterString
            | Rule::AvoidQuoteEscape
            | Rule::BadQuotesDocstring
            | Rule::BadQuotesInlineString
            | Rule::BadQuotesMultilineString
            | Rule::CommentedOutCode
            | Rule::ExtraneousParentheses
            | Rule::InvalidEscapeSequence
            | Rule::MultiLineImplicitStringConcatenation
            | Rule::SingleLineImplicitStringConcatenation
            | Rule::TrailingCommaMissing
            | Rule::TrailingCommaOnBareTupleProhibited
            | Rule::TrailingCommaProhibited => &LintSource::Tokens,
            Rule::IOError => &LintSource::Io,
            Rule::UnsortedImports | Rule::MissingRequiredImport => &LintSource::Imports,
            Rule::ImplicitNamespacePackage => &LintSource::Filesystem,
            _ => &LintSource::Ast,
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
pub const INCOMPATIBLE_CODES: &[(Rule, Rule, &str)] = &[(
    Rule::OneBlankLineBeforeClass,
    Rule::NoBlankLineBeforeClass,
    "`D203` (OneBlankLineBeforeClass) and `D211` (NoBlankLinesBeforeClass) are incompatible. \
     Consider adding `D203` to `ignore`.",
)];

/// A hash map from deprecated to latest `Rule`.
pub static CODE_REDIRECTS: Lazy<FxHashMap<&'static str, Rule>> = Lazy::new(|| {
    FxHashMap::from_iter([
        // TODO(charlie): Remove by 2023-01-01.
        ("U001", Rule::UselessMetaclassType),
        ("U003", Rule::TypeOfPrimitive),
        ("U004", Rule::UselessObjectInheritance),
        ("U005", Rule::DeprecatedUnittestAlias),
        ("U006", Rule::UsePEP585Annotation),
        ("U007", Rule::UsePEP604Annotation),
        ("U008", Rule::SuperCallWithParameters),
        ("U009", Rule::PEP3120UnnecessaryCodingComment),
        ("U010", Rule::UnnecessaryFutureImport),
        ("U011", Rule::LRUCacheWithoutParameters),
        ("U012", Rule::UnnecessaryEncodeUTF8),
        ("U013", Rule::ConvertTypedDictFunctionalToClass),
        ("U014", Rule::ConvertNamedTupleFunctionalToClass),
        ("U015", Rule::RedundantOpenModes),
        ("U016", Rule::RemoveSixCompat),
        ("U017", Rule::DatetimeTimezoneUTC),
        ("U019", Rule::TypingTextStrAlias),
        // TODO(charlie): Remove by 2023-02-01.
        ("I252", Rule::RelativeImports),
        ("M001", Rule::UnusedNOQA),
        // TODO(charlie): Remove by 2023-02-01.
        ("PDV002", Rule::UseOfInplaceArgument),
        ("PDV003", Rule::UseOfDotIsNull),
        ("PDV004", Rule::UseOfDotNotNull),
        ("PDV007", Rule::UseOfDotIx),
        ("PDV008", Rule::UseOfDotAt),
        ("PDV009", Rule::UseOfDotIat),
        ("PDV010", Rule::UseOfDotPivotOrUnstack),
        ("PDV011", Rule::UseOfDotValues),
        ("PDV012", Rule::UseOfDotReadTable),
        ("PDV013", Rule::UseOfDotStack),
        ("PDV015", Rule::UseOfPdMerge),
        ("PDV901", Rule::DfIsABadVariableName),
        // TODO(charlie): Remove by 2023-02-01.
        ("R501", Rule::UnnecessaryReturnNone),
        ("R502", Rule::ImplicitReturnValue),
        ("R503", Rule::ImplicitReturn),
        ("R504", Rule::UnnecessaryAssign),
        ("R505", Rule::SuperfluousElseReturn),
        ("R506", Rule::SuperfluousElseRaise),
        ("R507", Rule::SuperfluousElseContinue),
        ("R508", Rule::SuperfluousElseBreak),
        // TODO(charlie): Remove by 2023-02-01.
        ("IC001", Rule::ImportAliasIsNotConventional),
        ("IC002", Rule::ImportAliasIsNotConventional),
        ("IC003", Rule::ImportAliasIsNotConventional),
        ("IC004", Rule::ImportAliasIsNotConventional),
    ])
});

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::{Linter, ParseCode, Rule};

    #[test]
    fn check_code_serialization() {
        for rule in Rule::iter() {
            assert!(
                Rule::from_code(rule.code()).is_ok(),
                "{rule:?} could not be round-trip serialized."
            );
        }
    }

    #[test]
    fn test_linter_prefixes() {
        for rule in Rule::iter() {
            Linter::parse_code(rule.code())
                .unwrap_or_else(|| panic!("couldn't parse {:?}", rule.code()));
        }
    }
}
