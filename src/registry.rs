//! Registry of [`Rule`] to [`DiagnosticKind`] mappings.

use ruff_macros::RuleNamespace;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumIter};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::rule_selector::{prefix_to_selector, RuleSelector};
use crate::violation::Violation;
use crate::{rules, violations};

ruff_macros::define_rule_mapping!(
    // pycodestyle errors
    E101 => rules::pycodestyle::rules::MixedSpacesAndTabs,
    E401 => violations::MultipleImportsOnOneLine,
    E402 => violations::ModuleImportNotAtTopOfFile,
    E501 => rules::pycodestyle::rules::LineTooLong,
    E711 => rules::pycodestyle::rules::NoneComparison,
    E712 => rules::pycodestyle::rules::TrueFalseComparison,
    E713 => rules::pycodestyle::rules::NotInTest,
    E714 => rules::pycodestyle::rules::NotIsTest,
    E721 => rules::pycodestyle::rules::TypeComparison,
    E722 => rules::pycodestyle::rules::DoNotUseBareExcept,
    E731 => rules::pycodestyle::rules::DoNotAssignLambda,
    E741 => rules::pycodestyle::rules::AmbiguousVariableName,
    E742 => rules::pycodestyle::rules::AmbiguousClassName,
    E743 => rules::pycodestyle::rules::AmbiguousFunctionName,
    E902 => violations::IOError,
    E999 => violations::SyntaxError,
    // pycodestyle warnings
    W292 => rules::pycodestyle::rules::NoNewLineAtEndOfFile,
    W505 => rules::pycodestyle::rules::DocLineTooLong,
    W605 => rules::pycodestyle::rules::InvalidEscapeSequence,
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
    PLE0604 => rules::pylint::rules::InvalidAllObject,
    PLE0605 => rules::pylint::rules::InvalidAllFormat,
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
    I001 => rules::isort::rules::UnsortedImports,
    I002 => rules::isort::rules::MissingRequiredImport,
    // eradicate
    ERA001 => rules::eradicate::rules::CommentedOutCode,
    // flake8-bandit
    S101 => violations::AssertUsed,
    S102 => violations::ExecUsed,
    S103 => violations::BadFilePermissions,
    S104 => violations::HardcodedBindAllInterfaces,
    S105 => violations::HardcodedPasswordString,
    S106 => violations::HardcodedPasswordFuncArg,
    S107 => violations::HardcodedPasswordDefault,
    S108 => violations::HardcodedTempFile,
    S110 => rules::flake8_bandit::rules::TryExceptPass,
    S113 => violations::RequestWithoutTimeout,
    S324 => violations::HashlibInsecureHashFunction,
    S501 => violations::RequestWithNoCertValidation,
    S506 => violations::UnsafeYAMLLoad,
    S508 => violations::SnmpInsecureVersion,
    S509 => violations::SnmpWeakCryptography,
    S612 => rules::flake8_bandit::rules::LoggingConfigInsecureListen,
    S701 => violations::Jinja2AutoescapeFalse,
    // flake8-boolean-trap
    FBT001 => rules::flake8_boolean_trap::rules::BooleanPositionalArgInFunctionDefinition,
    FBT002 => rules::flake8_boolean_trap::rules::BooleanDefaultValueInFunctionDefinition,
    FBT003 => rules::flake8_boolean_trap::rules::BooleanPositionalValueInFunctionCall,
    // flake8-unused-arguments
    ARG001 => violations::UnusedFunctionArgument,
    ARG002 => violations::UnusedMethodArgument,
    ARG003 => violations::UnusedClassMethodArgument,
    ARG004 => violations::UnusedStaticMethodArgument,
    ARG005 => violations::UnusedLambdaArgument,
    // flake8-import-conventions
    ICN001 => rules::flake8_import_conventions::rules::ImportAliasIsNotConventional,
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
    PIE790 => rules::flake8_pie::rules::NoUnnecessaryPass,
    PIE794 => rules::flake8_pie::rules::DupeClassFieldDefinitions,
    PIE796 => rules::flake8_pie::rules::PreferUniqueEnums,
    PIE800 => rules::flake8_pie::rules::NoUnnecessarySpread,
    PIE804 => rules::flake8_pie::rules::NoUnnecessaryDictKwargs,
    PIE807 => rules::flake8_pie::rules::PreferListBuiltin,
    // flake8-commas
    COM812 => rules::flake8_commas::rules::TrailingCommaMissing,
    COM818 => rules::flake8_commas::rules::TrailingCommaOnBareTupleProhibited,
    COM819 => rules::flake8_commas::rules::TrailingCommaProhibited,
    // flake8-no-pep420
    INP001 => rules::flake8_no_pep420::rules::ImplicitNamespacePackage,
    // flake8-executable
    EXE001 => rules::flake8_executable::rules::ShebangNotExecutable,
    EXE002 => rules::flake8_executable::rules::ShebangMissingExecutableFile,
    EXE003 => rules::flake8_executable::rules::ShebangPython,
    EXE004 => rules::flake8_executable::rules::ShebangWhitespace,
    EXE005 => rules::flake8_executable::rules::ShebangNewline,
    // flake8-type-checking
    TCH001 => rules::flake8_type_checking::rules::TypingOnlyFirstPartyImport,
    TCH002 => rules::flake8_type_checking::rules::TypingOnlyThirdPartyImport,
    TCH003 => rules::flake8_type_checking::rules::TypingOnlyStandardLibraryImport,
    TCH004 => rules::flake8_type_checking::rules::RuntimeImportInTypeCheckingBlock,
    TCH005 => rules::flake8_type_checking::rules::EmptyTypeCheckingBlock,
    // tryceratops
    TRY002 => rules::tryceratops::rules::RaiseVanillaClass,
    TRY003 => rules::tryceratops::rules::RaiseVanillaArgs,
    TRY004 => rules::tryceratops::rules::PreferTypeError,
    TRY200 => rules::tryceratops::rules::ReraiseNoCause,
    TRY201 => rules::tryceratops::rules::VerboseRaise,
    TRY300 => rules::tryceratops::rules::TryConsiderElse,
    TRY301 => rules::tryceratops::rules::RaiseWithinTry,
    TRY400 => rules::tryceratops::rules::ErrorInsteadOfException,
    // flake8-use-pathlib
    PTH100 => rules::flake8_use_pathlib::violations::PathlibAbspath,
    PTH101 => rules::flake8_use_pathlib::violations::PathlibChmod,
    PTH102 => rules::flake8_use_pathlib::violations::PathlibMkdir,
    PTH103 => rules::flake8_use_pathlib::violations::PathlibMakedirs,
    PTH104 => rules::flake8_use_pathlib::violations::PathlibRename,
    PTH105 => rules::flake8_use_pathlib::violations::PathlibReplace,
    PTH106 => rules::flake8_use_pathlib::violations::PathlibRmdir,
    PTH107 => rules::flake8_use_pathlib::violations::PathlibRemove,
    PTH108 => rules::flake8_use_pathlib::violations::PathlibUnlink,
    PTH109 => rules::flake8_use_pathlib::violations::PathlibGetcwd,
    PTH110 => rules::flake8_use_pathlib::violations::PathlibExists,
    PTH111 => rules::flake8_use_pathlib::violations::PathlibExpanduser,
    PTH112 => rules::flake8_use_pathlib::violations::PathlibIsDir,
    PTH113 => rules::flake8_use_pathlib::violations::PathlibIsFile,
    PTH114 => rules::flake8_use_pathlib::violations::PathlibIsLink,
    PTH115 => rules::flake8_use_pathlib::violations::PathlibReadlink,
    PTH116 => rules::flake8_use_pathlib::violations::PathlibStat,
    PTH117 => rules::flake8_use_pathlib::violations::PathlibIsAbs,
    PTH118 => rules::flake8_use_pathlib::violations::PathlibJoin,
    PTH119 => rules::flake8_use_pathlib::violations::PathlibBasename,
    PTH120 => rules::flake8_use_pathlib::violations::PathlibDirname,
    PTH121 => rules::flake8_use_pathlib::violations::PathlibSamefile,
    PTH122 => rules::flake8_use_pathlib::violations::PathlibSplitext,
    PTH123 => rules::flake8_use_pathlib::violations::PathlibOpen,
    PTH124 => rules::flake8_use_pathlib::violations::PathlibPyPath,
    // flake8-logging-format
    G001 => rules::flake8_logging_format::violations::LoggingStringFormat,
    G002 => rules::flake8_logging_format::violations::LoggingPercentFormat,
    G003 => rules::flake8_logging_format::violations::LoggingStringConcat,
    G004 => rules::flake8_logging_format::violations::LoggingFString,
    G010 => rules::flake8_logging_format::violations::LoggingWarn,
    G101 => rules::flake8_logging_format::violations::LoggingExtraAttrClash,
    G201 => rules::flake8_logging_format::violations::LoggingExcInfo,
    G202 => rules::flake8_logging_format::violations::LoggingRedundantExcInfo,
    // ruff
    RUF001 => violations::AmbiguousUnicodeCharacterString,
    RUF002 => violations::AmbiguousUnicodeCharacterDocstring,
    RUF003 => violations::AmbiguousUnicodeCharacterComment,
    RUF004 => violations::KeywordArgumentBeforeStarArgument,
    RUF005 => violations::UnpackInsteadOfConcatenatingToCollectionLiteral,
    RUF100 => violations::UnusedNOQA,
);

#[derive(EnumIter, Debug, PartialEq, Eq, RuleNamespace)]
pub enum Linter {
    /// [Pyflakes](https://pypi.org/project/pyflakes/)
    #[prefix = "F"]
    Pyflakes,
    /// [pycodestyle](https://pypi.org/project/pycodestyle/)
    #[prefix = "E"]
    #[prefix = "W"]
    Pycodestyle,
    /// [mccabe](https://pypi.org/project/mccabe/)
    #[prefix = "C90"]
    McCabe,
    /// [isort](https://pypi.org/project/isort/)
    #[prefix = "I"]
    Isort,
    /// [pep8-naming](https://pypi.org/project/pep8-naming/)
    #[prefix = "N"]
    PEP8Naming,
    /// [pydocstyle](https://pypi.org/project/pydocstyle/)
    #[prefix = "D"]
    Pydocstyle,
    /// [pyupgrade](https://pypi.org/project/pyupgrade/)
    #[prefix = "UP"]
    Pyupgrade,
    /// [flake8-2020](https://pypi.org/project/flake8-2020/)
    #[prefix = "YTT"]
    Flake82020,
    /// [flake8-annotations](https://pypi.org/project/flake8-annotations/)
    #[prefix = "ANN"]
    Flake8Annotations,
    /// [flake8-bandit](https://pypi.org/project/flake8-bandit/)
    #[prefix = "S"]
    Flake8Bandit,
    /// [flake8-blind-except](https://pypi.org/project/flake8-blind-except/)
    #[prefix = "BLE"]
    Flake8BlindExcept,
    /// [flake8-boolean-trap](https://pypi.org/project/flake8-boolean-trap/)
    #[prefix = "FBT"]
    Flake8BooleanTrap,
    /// [flake8-bugbear](https://pypi.org/project/flake8-bugbear/)
    #[prefix = "B"]
    Flake8Bugbear,
    /// [flake8-builtins](https://pypi.org/project/flake8-builtins/)
    #[prefix = "A"]
    Flake8Builtins,
    /// [flake8-commas](https://pypi.org/project/flake8-commas/)
    #[prefix = "COM"]
    Flake8Commas,
    /// [flake8-comprehensions](https://pypi.org/project/flake8-comprehensions/)
    #[prefix = "C4"]
    Flake8Comprehensions,
    /// [flake8-datetimez](https://pypi.org/project/flake8-datetimez/)
    #[prefix = "DTZ"]
    Flake8Datetimez,
    /// [flake8-debugger](https://pypi.org/project/flake8-debugger/)
    #[prefix = "T10"]
    Flake8Debugger,
    /// [flake8-errmsg](https://pypi.org/project/flake8-errmsg/)
    #[prefix = "EM"]
    Flake8ErrMsg,
    /// [flake8-executable](https://pypi.org/project/flake8-executable/)
    #[prefix = "EXE"]
    Flake8Executable,
    /// [flake8-implicit-str-concat](https://pypi.org/project/flake8-implicit-str-concat/)
    #[prefix = "ISC"]
    Flake8ImplicitStrConcat,
    /// [flake8-import-conventions](https://github.com/joaopalmeiro/flake8-import-conventions)
    #[prefix = "ICN"]
    Flake8ImportConventions,
    /// [flake8-logging-format](https://pypi.org/project/flake8-logging-format/0.9.0/)
    #[prefix = "G"]
    Flake8LoggingFormat,
    /// [flake8-no-pep420](https://pypi.org/project/flake8-no-pep420/)
    #[prefix = "INP"]
    Flake8NoPep420,
    /// [flake8-pie](https://pypi.org/project/flake8-pie/)
    #[prefix = "PIE"]
    Flake8Pie,
    /// [flake8-print](https://pypi.org/project/flake8-print/)
    #[prefix = "T20"]
    Flake8Print,
    /// [flake8-pytest-style](https://pypi.org/project/flake8-pytest-style/)
    #[prefix = "PT"]
    Flake8PytestStyle,
    /// [flake8-quotes](https://pypi.org/project/flake8-quotes/)
    #[prefix = "Q"]
    Flake8Quotes,
    /// [flake8-return](https://pypi.org/project/flake8-return/)
    #[prefix = "RET"]
    Flake8Return,
    /// [flake8-simplify](https://pypi.org/project/flake8-simplify/)
    #[prefix = "SIM"]
    Flake8Simplify,
    /// [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/)
    #[prefix = "TID"]
    Flake8TidyImports,
    /// [flake8-type-checking](https://pypi.org/project/flake8-type-checking/)
    #[prefix = "TCH"]
    Flake8TypeChecking,
    /// [flake8-unused-arguments](https://pypi.org/project/flake8-unused-arguments/)
    #[prefix = "ARG"]
    Flake8UnusedArguments,
    /// [flake8-use-pathlib](https://pypi.org/project/flake8-use-pathlib/)
    #[prefix = "PTH"]
    Flake8UsePathlib,
    /// [eradicate](https://pypi.org/project/eradicate/)
    #[prefix = "ERA"]
    Eradicate,
    /// [pandas-vet](https://pypi.org/project/pandas-vet/)
    #[prefix = "PD"]
    PandasVet,
    /// [pygrep-hooks](https://github.com/pre-commit/pygrep-hooks)
    #[prefix = "PGH"]
    PygrepHooks,
    /// [Pylint](https://pypi.org/project/pylint/)
    #[prefix = "PL"]
    Pylint,
    /// [tryceratops](https://pypi.org/project/tryceratops/1.1.0/)
    #[prefix = "TRY"]
    Tryceratops,
    /// Ruff-specific rules
    #[prefix = "RUF"]
    Ruff,
}

pub trait RuleNamespace: Sized {
    fn parse_code(code: &str) -> Option<(Self, &str)>;

    fn prefixes(&self) -> &'static [&'static str];

    fn name(&self) -> &'static str;

    fn url(&self) -> Option<&'static str>;
}

/// The prefix, name and selector for an upstream linter category.
pub struct LinterCategory(pub &'static str, pub &'static str, pub RuleSelector);

// TODO(martin): Move these constant definitions back to Linter::categories impl
// once RuleSelector is an enum with a Linter variant
const PYCODESTYLE_CATEGORIES: &[LinterCategory] = &[
    LinterCategory("E", "Error", prefix_to_selector(RuleCodePrefix::E)),
    LinterCategory("W", "Warning", prefix_to_selector(RuleCodePrefix::W)),
];

const PYLINT_CATEGORIES: &[LinterCategory] = &[
    LinterCategory("PLC", "Convention", prefix_to_selector(RuleCodePrefix::PLC)),
    LinterCategory("PLE", "Error", prefix_to_selector(RuleCodePrefix::PLE)),
    LinterCategory("PLR", "Refactor", prefix_to_selector(RuleCodePrefix::PLR)),
    LinterCategory("PLW", "Warning", prefix_to_selector(RuleCodePrefix::PLW)),
];

impl Linter {
    pub fn categories(&self) -> Option<&'static [LinterCategory]> {
        match self {
            Linter::Pycodestyle => Some(PYCODESTYLE_CATEGORIES),
            Linter::Pylint => Some(PYLINT_CATEGORIES),
            _ => None,
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
            | Rule::ShebangMissingExecutableFile
            | Rule::ShebangNotExecutable
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
            | Rule::MultiLineImplicitStringConcatenation
            | Rule::ExtraneousParentheses
            | Rule::InvalidEscapeSequence
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
pub const INCOMPATIBLE_CODES: &[(Rule, Rule, &str)] = &[
    (
        Rule::OneBlankLineBeforeClass,
        Rule::NoBlankLineBeforeClass,
        "`one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are \
         incompatible. Consider ignoring `one-blank-line-before-class`.",
    ),
    (
        Rule::MultiLineSummaryFirstLine,
        Rule::MultiLineSummarySecondLine,
        "`multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are \
         incompatible. Consider ignoring one.",
    ),
];

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::{Linter, Rule, RuleNamespace};

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
