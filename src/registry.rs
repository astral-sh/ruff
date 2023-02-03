//! Registry of [`Rule`] to [`DiagnosticKind`] mappings.

use ruff_macros::RuleNamespace;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumIter};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::violation::Violation;
use crate::{rules, violations};

ruff_macros::define_rule_mapping!(
    // pycodestyle errors
    E101 => rules::pycodestyle::rules::MixedSpacesAndTabs,
    E401 => rules::pycodestyle::rules::MultipleImportsOnOneLine,
    E402 => rules::pycodestyle::rules::ModuleImportNotAtTopOfFile,
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
    E902 => rules::pycodestyle::rules::IOError,
    E999 => rules::pycodestyle::rules::SyntaxError,
    // pycodestyle warnings
    W292 => rules::pycodestyle::rules::NoNewLineAtEndOfFile,
    W505 => rules::pycodestyle::rules::DocLineTooLong,
    W605 => rules::pycodestyle::rules::InvalidEscapeSequence,
    // pyflakes
    F401 => rules::pyflakes::rules::UnusedImport,
    F402 => rules::pyflakes::rules::ImportShadowedByLoopVar,
    F403 => rules::pyflakes::rules::ImportStarUsed,
    F404 => rules::pyflakes::rules::LateFutureImport,
    F405 => rules::pyflakes::rules::ImportStarUsage,
    F406 => rules::pyflakes::rules::ImportStarNotPermitted,
    F407 => rules::pyflakes::rules::FutureFeatureNotDefined,
    F501 => rules::pyflakes::rules::PercentFormatInvalidFormat,
    F502 => rules::pyflakes::rules::PercentFormatExpectedMapping,
    F503 => rules::pyflakes::rules::PercentFormatExpectedSequence,
    F504 => rules::pyflakes::rules::PercentFormatExtraNamedArguments,
    F505 => rules::pyflakes::rules::PercentFormatMissingArgument,
    F506 => rules::pyflakes::rules::PercentFormatMixedPositionalAndNamed,
    F507 => rules::pyflakes::rules::PercentFormatPositionalCountMismatch,
    F508 => rules::pyflakes::rules::PercentFormatStarRequiresSequence,
    F509 => rules::pyflakes::rules::PercentFormatUnsupportedFormatCharacter,
    F521 => rules::pyflakes::rules::StringDotFormatInvalidFormat,
    F522 => rules::pyflakes::rules::StringDotFormatExtraNamedArguments,
    F523 => rules::pyflakes::rules::StringDotFormatExtraPositionalArguments,
    F524 => rules::pyflakes::rules::StringDotFormatMissingArguments,
    F525 => rules::pyflakes::rules::StringDotFormatMixingAutomatic,
    F541 => rules::pyflakes::rules::FStringMissingPlaceholders,
    F601 => rules::pyflakes::rules::MultiValueRepeatedKeyLiteral,
    F602 => rules::pyflakes::rules::MultiValueRepeatedKeyVariable,
    F621 => rules::pyflakes::rules::ExpressionsInStarAssignment,
    F622 => rules::pyflakes::rules::TwoStarredExpressions,
    F631 => rules::pyflakes::rules::AssertTuple,
    F632 => rules::pyflakes::rules::IsLiteral,
    F633 => rules::pyflakes::rules::InvalidPrintSyntax,
    F634 => rules::pyflakes::rules::IfTuple,
    F701 => rules::pyflakes::rules::BreakOutsideLoop,
    F702 => rules::pyflakes::rules::ContinueOutsideLoop,
    F704 => rules::pyflakes::rules::YieldOutsideFunction,
    F706 => rules::pyflakes::rules::ReturnOutsideFunction,
    F707 => rules::pyflakes::rules::DefaultExceptNotLast,
    F722 => rules::pyflakes::rules::ForwardAnnotationSyntaxError,
    F811 => rules::pyflakes::rules::RedefinedWhileUnused,
    F821 => rules::pyflakes::rules::UndefinedName,
    F822 => rules::pyflakes::rules::UndefinedExport,
    F823 => rules::pyflakes::rules::UndefinedLocal,
    F841 => rules::pyflakes::rules::UnusedVariable,
    F842 => rules::pyflakes::rules::UnusedAnnotation,
    F901 => rules::pyflakes::rules::RaiseNotImplemented,
    // pylint
    PLE0604 => rules::pylint::rules::InvalidAllObject,
    PLE0605 => rules::pylint::rules::InvalidAllFormat,
    PLC0414 => rules::pylint::rules::UselessImportAlias,
    PLC3002 => rules::pylint::rules::UnnecessaryDirectLambdaCall,
    PLE0117 => rules::pylint::rules::NonlocalWithoutBinding,
    PLE0118 => rules::pylint::rules::UsedPriorGlobalDeclaration,
    PLE1142 => rules::pylint::rules::AwaitOutsideAsync,
    PLR0206 => rules::pylint::rules::PropertyWithParameters,
    PLR0402 => rules::pylint::rules::ConsiderUsingFromImport,
    PLR0133 => rules::pylint::rules::ConstantComparison,
    PLR1701 => rules::pylint::rules::ConsiderMergingIsinstance,
    PLR1722 => rules::pylint::rules::UseSysExit,
    PLR2004 => rules::pylint::rules::MagicValueComparison,
    PLW0120 => rules::pylint::rules::UselessElseOnLoop,
    PLW0602 => rules::pylint::rules::GlobalVariableNotAssigned,
    PLR0913 => rules::pylint::rules::TooManyArgs,
    PLR0915 => rules::pylint::rules::TooManyStatements,
    // flake8-builtins
    A001 => rules::flake8_builtins::rules::BuiltinVariableShadowing,
    A002 => rules::flake8_builtins::rules::BuiltinArgumentShadowing,
    A003 => rules::flake8_builtins::rules::BuiltinAttributeShadowing,
    // flake8-bugbear
    B002 => rules::flake8_bugbear::rules::UnaryPrefixIncrement,
    B003 => rules::flake8_bugbear::rules::AssignmentToOsEnviron,
    B004 => rules::flake8_bugbear::rules::UnreliableCallableCheck,
    B005 => rules::flake8_bugbear::rules::StripWithMultiCharacters,
    B006 => rules::flake8_bugbear::rules::MutableArgumentDefault,
    B007 => rules::flake8_bugbear::rules::UnusedLoopControlVariable,
    B008 => rules::flake8_bugbear::rules::FunctionCallArgumentDefault,
    B009 => rules::flake8_bugbear::rules::GetAttrWithConstant,
    B010 => rules::flake8_bugbear::rules::SetAttrWithConstant,
    B011 => rules::flake8_bugbear::rules::DoNotAssertFalse,
    B012 => rules::flake8_bugbear::rules::JumpStatementInFinally,
    B013 => rules::flake8_bugbear::rules::RedundantTupleInExceptionHandler,
    B014 => rules::flake8_bugbear::rules::DuplicateHandlerException,
    B015 => rules::flake8_bugbear::rules::UselessComparison,
    B016 => rules::flake8_bugbear::rules::CannotRaiseLiteral,
    B017 => rules::flake8_bugbear::rules::NoAssertRaisesException,
    B018 => rules::flake8_bugbear::rules::UselessExpression,
    B019 => rules::flake8_bugbear::rules::CachedInstanceMethod,
    B020 => rules::flake8_bugbear::rules::LoopVariableOverridesIterator,
    B021 => rules::flake8_bugbear::rules::FStringDocstring,
    B022 => rules::flake8_bugbear::rules::UselessContextlibSuppress,
    B023 => rules::flake8_bugbear::rules::FunctionUsesLoopVariable,
    B024 => rules::flake8_bugbear::rules::AbstractBaseClassWithoutAbstractMethod,
    B025 => rules::flake8_bugbear::rules::DuplicateTryBlockException,
    B026 => rules::flake8_bugbear::rules::StarArgUnpackingAfterKeywordArg,
    B027 => rules::flake8_bugbear::rules::EmptyMethodWithoutAbstractDecorator,
    B904 => rules::flake8_bugbear::rules::RaiseWithoutFromInsideExcept,
    B905 => rules::flake8_bugbear::rules::ZipWithoutExplicitStrict,
    // flake8-blind-except
    BLE001 => rules::flake8_blind_except::rules::BlindExcept,
    // flake8-comprehensions
    C400 => rules::flake8_comprehensions::rules::UnnecessaryGeneratorList,
    C401 => rules::flake8_comprehensions::rules::UnnecessaryGeneratorSet,
    C402 => rules::flake8_comprehensions::rules::UnnecessaryGeneratorDict,
    C403 => rules::flake8_comprehensions::rules::UnnecessaryListComprehensionSet,
    C404 => rules::flake8_comprehensions::rules::UnnecessaryListComprehensionDict,
    C405 => rules::flake8_comprehensions::rules::UnnecessaryLiteralSet,
    C406 => rules::flake8_comprehensions::rules::UnnecessaryLiteralDict,
    C408 => rules::flake8_comprehensions::rules::UnnecessaryCollectionCall,
    C409 => rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinTupleCall,
    C410 => rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinListCall,
    C411 => rules::flake8_comprehensions::rules::UnnecessaryListCall,
    C413 => rules::flake8_comprehensions::rules::UnnecessaryCallAroundSorted,
    C414 => rules::flake8_comprehensions::rules::UnnecessaryDoubleCastOrProcess,
    C415 => rules::flake8_comprehensions::rules::UnnecessarySubscriptReversal,
    C416 => rules::flake8_comprehensions::rules::UnnecessaryComprehension,
    C417 => rules::flake8_comprehensions::rules::UnnecessaryMap,
    // flake8-debugger
    T100 => violations::Debugger,
    // mccabe
    C901 => violations::FunctionIsTooComplex,
    // flake8-tidy-imports
    TID251 => rules::flake8_tidy_imports::banned_api::BannedApi,
    TID252 => rules::flake8_tidy_imports::relative_imports::RelativeImports,
    // flake8-return
    RET501 => rules::flake8_return::rules::UnnecessaryReturnNone,
    RET502 => rules::flake8_return::rules::ImplicitReturnValue,
    RET503 => rules::flake8_return::rules::ImplicitReturn,
    RET504 => rules::flake8_return::rules::UnnecessaryAssign,
    RET505 => rules::flake8_return::rules::SuperfluousElseReturn,
    RET506 => rules::flake8_return::rules::SuperfluousElseRaise,
    RET507 => rules::flake8_return::rules::SuperfluousElseContinue,
    RET508 => rules::flake8_return::rules::SuperfluousElseBreak,
    // flake8-implicit-str-concat
    ISC001 => violations::SingleLineImplicitStringConcatenation,
    ISC002 => violations::MultiLineImplicitStringConcatenation,
    ISC003 => violations::ExplicitStringConcatenation,
    // flake8-print
    T201 => violations::PrintFound,
    T203 => violations::PPrintFound,
    // flake8-quotes
    Q000 => rules::flake8_quotes::rules::BadQuotesInlineString,
    Q001 => rules::flake8_quotes::rules::BadQuotesMultilineString,
    Q002 => rules::flake8_quotes::rules::BadQuotesDocstring,
    Q003 => rules::flake8_quotes::rules::AvoidQuoteEscape,
    // flake8-annotations
    ANN001 => rules::flake8_annotations::rules::MissingTypeFunctionArgument,
    ANN002 => rules::flake8_annotations::rules::MissingTypeArgs,
    ANN003 => rules::flake8_annotations::rules::MissingTypeKwargs,
    ANN101 => rules::flake8_annotations::rules::MissingTypeSelf,
    ANN102 => rules::flake8_annotations::rules::MissingTypeCls,
    ANN201 => rules::flake8_annotations::rules::MissingReturnTypePublicFunction,
    ANN202 => rules::flake8_annotations::rules::MissingReturnTypePrivateFunction,
    ANN204 => rules::flake8_annotations::rules::MissingReturnTypeSpecialMethod,
    ANN205 => rules::flake8_annotations::rules::MissingReturnTypeStaticMethod,
    ANN206 => rules::flake8_annotations::rules::MissingReturnTypeClassMethod,
    ANN401 => rules::flake8_annotations::rules::DynamicallyTypedExpression,
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
    SIM115 => rules::flake8_simplify::rules::OpenFileWithContextHandler,
    SIM101 => rules::flake8_simplify::rules::DuplicateIsinstanceCall,
    SIM102 => rules::flake8_simplify::rules::NestedIfStatements,
    SIM103 => rules::flake8_simplify::rules::ReturnBoolConditionDirectly,
    SIM105 => rules::flake8_simplify::rules::UseContextlibSuppress,
    SIM107 => rules::flake8_simplify::rules::ReturnInTryExceptFinally,
    SIM108 => rules::flake8_simplify::rules::UseTernaryOperator,
    SIM109 => rules::flake8_simplify::rules::CompareWithTuple,
    SIM110 => rules::flake8_simplify::rules::ConvertLoopToAny,
    SIM111 => rules::flake8_simplify::rules::ConvertLoopToAll,
    SIM112 => rules::flake8_simplify::rules::UseCapitalEnvironmentVariables,
    SIM117 => rules::flake8_simplify::rules::MultipleWithStatements,
    SIM118 => rules::flake8_simplify::rules::KeyInDict,
    SIM201 => rules::flake8_simplify::rules::NegateEqualOp,
    SIM202 => rules::flake8_simplify::rules::NegateNotEqualOp,
    SIM208 => rules::flake8_simplify::rules::DoubleNegation,
    SIM210 => rules::flake8_simplify::rules::IfExprWithTrueFalse,
    SIM211 => rules::flake8_simplify::rules::IfExprWithFalseTrue,
    SIM212 => rules::flake8_simplify::rules::IfExprWithTwistedArms,
    SIM220 => rules::flake8_simplify::rules::AAndNotA,
    SIM221 => rules::flake8_simplify::rules::AOrNotA,
    SIM222 => rules::flake8_simplify::rules::OrTrue,
    SIM223 => rules::flake8_simplify::rules::AndFalse,
    SIM300 => rules::flake8_simplify::rules::YodaConditions,
    SIM401 => rules::flake8_simplify::rules::DictGetWithDefault,
    // pyupgrade
    UP001 => rules::pyupgrade::rules::UselessMetaclassType,
    UP003 => rules::pyupgrade::rules::TypeOfPrimitive,
    UP004 => rules::pyupgrade::rules::UselessObjectInheritance,
    UP005 => rules::pyupgrade::rules::DeprecatedUnittestAlias,
    UP006 => rules::pyupgrade::rules::UsePEP585Annotation,
    UP007 => rules::pyupgrade::rules::UsePEP604Annotation,
    UP008 => rules::pyupgrade::rules::SuperCallWithParameters,
    UP009 => rules::pyupgrade::rules::PEP3120UnnecessaryCodingComment,
    UP010 => rules::pyupgrade::rules::UnnecessaryFutureImport,
    UP011 => rules::pyupgrade::rules::LRUCacheWithoutParameters,
    UP012 => rules::pyupgrade::rules::UnnecessaryEncodeUTF8,
    UP013 => rules::pyupgrade::rules::ConvertTypedDictFunctionalToClass,
    UP014 => rules::pyupgrade::rules::ConvertNamedTupleFunctionalToClass,
    UP015 => rules::pyupgrade::rules::RedundantOpenModes,
    UP017 => rules::pyupgrade::rules::DatetimeTimezoneUTC,
    UP018 => rules::pyupgrade::rules::NativeLiterals,
    UP019 => rules::pyupgrade::rules::TypingTextStrAlias,
    UP020 => rules::pyupgrade::rules::OpenAlias,
    UP021 => rules::pyupgrade::rules::ReplaceUniversalNewlines,
    UP022 => rules::pyupgrade::rules::ReplaceStdoutStderr,
    UP023 => rules::pyupgrade::rules::RewriteCElementTree,
    UP024 => rules::pyupgrade::rules::OSErrorAlias,
    UP025 => rules::pyupgrade::rules::RewriteUnicodeLiteral,
    UP026 => rules::pyupgrade::rules::RewriteMockImport,
    UP027 => rules::pyupgrade::rules::RewriteListComprehension,
    UP028 => rules::pyupgrade::rules::RewriteYieldFrom,
    UP029 => rules::pyupgrade::rules::UnnecessaryBuiltinImport,
    UP030 => rules::pyupgrade::rules::FormatLiterals,
    UP031 => rules::pyupgrade::rules::PrintfStringFormatting,
    UP032 => rules::pyupgrade::rules::FString,
    UP033 => rules::pyupgrade::rules::FunctoolsCache,
    UP034 => rules::pyupgrade::rules::ExtraneousParentheses,
    UP035 => rules::pyupgrade::rules::ImportReplacements,
    UP036 => rules::pyupgrade::rules::OutdatedVersionBlock,
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
    D401 => rules::pydocstyle::rules::non_imperative_mood::NonImperativeMood,
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
    PD002 => rules::pandas_vet::rules::UseOfInplaceArgument,
    PD003 => rules::pandas_vet::rules::UseOfDotIsNull,
    PD004 => rules::pandas_vet::rules::UseOfDotNotNull,
    PD007 => rules::pandas_vet::rules::UseOfDotIx,
    PD008 => rules::pandas_vet::rules::UseOfDotAt,
    PD009 => rules::pandas_vet::rules::UseOfDotIat,
    PD010 => rules::pandas_vet::rules::UseOfDotPivotOrUnstack,
    PD011 => rules::pandas_vet::rules::UseOfDotValues,
    PD012 => rules::pandas_vet::rules::UseOfDotReadTable,
    PD013 => rules::pandas_vet::rules::UseOfDotStack,
    PD015 => rules::pandas_vet::rules::UseOfPdMerge,
    PD901 => rules::pandas_vet::rules::DfIsABadVariableName,
    // flake8-errmsg
    EM101 => violations::RawStringInException,
    EM102 => violations::FStringInException,
    EM103 => violations::DotFormatInException,
    // flake8-pytest-style
    PT001 => rules::flake8_pytest_style::rules::IncorrectFixtureParenthesesStyle,
    PT002 => rules::flake8_pytest_style::rules::FixturePositionalArgs,
    PT003 => rules::flake8_pytest_style::rules::ExtraneousScopeFunction,
    PT004 => rules::flake8_pytest_style::rules::MissingFixtureNameUnderscore,
    PT005 => rules::flake8_pytest_style::rules::IncorrectFixtureNameUnderscore,
    PT006 => rules::flake8_pytest_style::rules::ParametrizeNamesWrongType,
    PT007 => rules::flake8_pytest_style::rules::ParametrizeValuesWrongType,
    PT008 => rules::flake8_pytest_style::rules::PatchWithLambda,
    PT009 => rules::flake8_pytest_style::rules::UnittestAssertion,
    PT010 => rules::flake8_pytest_style::rules::RaisesWithoutException,
    PT011 => rules::flake8_pytest_style::rules::RaisesTooBroad,
    PT012 => rules::flake8_pytest_style::rules::RaisesWithMultipleStatements,
    PT013 => rules::flake8_pytest_style::rules::IncorrectPytestImport,
    PT015 => rules::flake8_pytest_style::rules::AssertAlwaysFalse,
    PT016 => rules::flake8_pytest_style::rules::FailWithoutMessage,
    PT017 => rules::flake8_pytest_style::rules::AssertInExcept,
    PT018 => rules::flake8_pytest_style::rules::CompositeAssertion,
    PT019 => rules::flake8_pytest_style::rules::FixtureParamWithoutValue,
    PT020 => rules::flake8_pytest_style::rules::DeprecatedYieldFixture,
    PT021 => rules::flake8_pytest_style::rules::FixtureFinalizerCallback,
    PT022 => rules::flake8_pytest_style::rules::UselessYieldFixture,
    PT023 => rules::flake8_pytest_style::rules::IncorrectMarkParenthesesStyle,
    PT024 => rules::flake8_pytest_style::rules::UnnecessaryAsyncioMarkOnFixture,
    PT025 => rules::flake8_pytest_style::rules::ErroneousUseFixturesOnFixture,
    PT026 => rules::flake8_pytest_style::rules::UseFixturesWithoutParameters,
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
    // flake8-raise
    RSE102 => rules::flake8_raise::rules::UnnecessaryParenOnRaiseException,
    // flake8-self
    SLF001 => rules::flake8_self::rules::PrivateMemberAccess,
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
    /// [flake8-raise](https://pypi.org/project/flake8-raise/)
    #[prefix = "RSE"]
    Flake8Raise,
    /// [flake8-self](https://pypi.org/project/flake8-self/)
    #[prefix = "SLF"]
    Flake8Self,
    /// Ruff-specific rules
    #[prefix = "RUF"]
    Ruff,
}

pub trait RuleNamespace: Sized {
    /// Returns the prefix that every single code that ruff uses to identify
    /// rules from this linter starts with.  In the case that multiple
    /// `#[prefix]`es are configured for the variant in the `Linter` enum
    /// definition this is the empty string.
    fn common_prefix(&self) -> &'static str;

    /// Attempts to parse the given rule code. If the prefix is recognized
    /// returns the respective variant along with the code with the common
    /// prefix stripped.
    fn parse_code(code: &str) -> Option<(Self, &str)>;

    fn name(&self) -> &'static str;

    fn url(&self) -> Option<&'static str>;
}

/// The prefix, name and selector for an upstream linter category.
pub struct UpstreamCategory(pub &'static str, pub &'static str, pub RuleCodePrefix);

impl Linter {
    pub fn upstream_categories(&self) -> Option<&'static [UpstreamCategory]> {
        match self {
            Linter::Pycodestyle => Some(&[
                UpstreamCategory("E", "Error", RuleCodePrefix::E),
                UpstreamCategory("W", "Warning", RuleCodePrefix::W),
            ]),
            Linter::Pylint => Some(&[
                UpstreamCategory("PLC", "Convention", RuleCodePrefix::PLC),
                UpstreamCategory("PLE", "Error", RuleCodePrefix::PLE),
                UpstreamCategory("PLR", "Refactor", RuleCodePrefix::PLR),
                UpstreamCategory("PLW", "Warning", RuleCodePrefix::PLW),
            ]),
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
pub const INCOMPATIBLE_CODES: &[(Rule, Rule, &str); 2] = &[
    (
        Rule::NoBlankLineBeforeClass,
        Rule::OneBlankLineBeforeClass,
        "`one-blank-line-before-class` (D203) and `no-blank-line-before-class` (D211) are \
         incompatible. Ignoring `one-blank-line-before-class`.",
    ),
    (
        Rule::MultiLineSummaryFirstLine,
        Rule::MultiLineSummarySecondLine,
        "`multi-line-summary-first-line` (D212) and `multi-line-summary-second-line` (D213) are \
         incompatible. Ignoring `multi-line-summary-second-line`.",
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
    fn test_linter_parse_code() {
        for rule in Rule::iter() {
            let code = rule.code();
            let (linter, rest) =
                Linter::parse_code(code).unwrap_or_else(|| panic!("couldn't parse {:?}", code));
            assert_eq!(code, format!("{}{rest}", linter.common_prefix()));
        }
    }
}
