//! Registry of [`Rule`] to [`DiagnosticKind`] mappings.

use ruff_macros::RuleNamespace;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumIter};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::rules;
use crate::violation::Violation;

ruff_macros::define_rule_mapping!(
    // pycodestyle errors
    E101 => rules::pycodestyle::rules::MixedSpacesAndTabs,
    #[cfg(feature = "logical_lines")]
    E111 => rules::pycodestyle::rules::IndentationWithInvalidMultiple,
    #[cfg(feature = "logical_lines")]
    E112 => rules::pycodestyle::rules::NoIndentedBlock,
    #[cfg(feature = "logical_lines")]
    E113 => rules::pycodestyle::rules::UnexpectedIndentation,
    #[cfg(feature = "logical_lines")]
    E114 => rules::pycodestyle::rules::IndentationWithInvalidMultipleComment,
    #[cfg(feature = "logical_lines")]
    E115 => rules::pycodestyle::rules::NoIndentedBlockComment,
    #[cfg(feature = "logical_lines")]
    E116 => rules::pycodestyle::rules::UnexpectedIndentationComment,
    #[cfg(feature = "logical_lines")]
    E117 => rules::pycodestyle::rules::OverIndented,
    #[cfg(feature = "logical_lines")]
    E201 => rules::pycodestyle::rules::WhitespaceAfterOpenBracket,
    #[cfg(feature = "logical_lines")]
    E202 => rules::pycodestyle::rules::WhitespaceBeforeCloseBracket,
    #[cfg(feature = "logical_lines")]
    E203 => rules::pycodestyle::rules::WhitespaceBeforePunctuation,
    #[cfg(feature = "logical_lines")]
    E221 => rules::pycodestyle::rules::MultipleSpacesBeforeOperator,
    #[cfg(feature = "logical_lines")]
    E222 => rules::pycodestyle::rules::MultipleSpacesAfterOperator,
    #[cfg(feature = "logical_lines")]
    E223 => rules::pycodestyle::rules::TabBeforeOperator,
    #[cfg(feature = "logical_lines")]
    E224 => rules::pycodestyle::rules::TabAfterOperator,
    #[cfg(feature = "logical_lines")]
    E261 => rules::pycodestyle::rules::TooFewSpacesBeforeInlineComment,
    #[cfg(feature = "logical_lines")]
    E262 => rules::pycodestyle::rules::NoSpaceAfterInlineComment,
    #[cfg(feature = "logical_lines")]
    E265 => rules::pycodestyle::rules::NoSpaceAfterBlockComment,
    #[cfg(feature = "logical_lines")]
    E266 => rules::pycodestyle::rules::MultipleLeadingHashesForBlockComment,
    #[cfg(feature = "logical_lines")]
    E271 => rules::pycodestyle::rules::MultipleSpacesAfterKeyword,
    #[cfg(feature = "logical_lines")]
    E272 => rules::pycodestyle::rules::MultipleSpacesBeforeKeyword,
    #[cfg(feature = "logical_lines")]
    E273 => rules::pycodestyle::rules::TabAfterKeyword,
    #[cfg(feature = "logical_lines")]
    E274 => rules::pycodestyle::rules::TabBeforeKeyword,
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
    PLE2502 => rules::pylint::rules::BidirectionalUnicode,
    PLE1310 => rules::pylint::rules::BadStrStripCall,
    PLC0414 => rules::pylint::rules::UselessImportAlias,
    PLC3002 => rules::pylint::rules::UnnecessaryDirectLambdaCall,
    PLE0117 => rules::pylint::rules::NonlocalWithoutBinding,
    PLE0118 => rules::pylint::rules::UsedPriorGlobalDeclaration,
    PLE1142 => rules::pylint::rules::AwaitOutsideAsync,
    PLR0206 => rules::pylint::rules::PropertyWithParameters,
    PLR0402 => rules::pylint::rules::ConsiderUsingFromImport,
    PLR0133 => rules::pylint::rules::ComparisonOfConstant,
    PLR1701 => rules::pylint::rules::ConsiderMergingIsinstance,
    PLR1722 => rules::pylint::rules::ConsiderUsingSysExit,
    PLR2004 => rules::pylint::rules::MagicValueComparison,
    PLW0120 => rules::pylint::rules::UselessElseOnLoop,
    PLW0602 => rules::pylint::rules::GlobalVariableNotAssigned,
    PLR0911 => rules::pylint::rules::TooManyReturnStatements,
    PLR0913 => rules::pylint::rules::TooManyArguments,
    PLR0912 => rules::pylint::rules::TooManyBranches,
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
    B017 => rules::flake8_bugbear::rules::AssertRaisesException,
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
    T100 => rules::flake8_debugger::rules::Debugger,
    // mccabe
    C901 => rules::mccabe::rules::FunctionIsTooComplex,
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
    ISC001 => rules::flake8_implicit_str_concat::rules::SingleLineImplicitStringConcatenation,
    ISC002 => rules::flake8_implicit_str_concat::rules::MultiLineImplicitStringConcatenation,
    ISC003 => rules::flake8_implicit_str_concat::rules::ExplicitStringConcatenation,
    // flake8-print
    T201 => rules::flake8_print::rules::PrintFound,
    T203 => rules::flake8_print::rules::PPrintFound,
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
    YTT101 => rules::flake8_2020::rules::SysVersionSlice3Referenced,
    YTT102 => rules::flake8_2020::rules::SysVersion2Referenced,
    YTT103 => rules::flake8_2020::rules::SysVersionCmpStr3,
    YTT201 => rules::flake8_2020::rules::SysVersionInfo0Eq3Referenced,
    YTT202 => rules::flake8_2020::rules::SixPY3Referenced,
    YTT203 => rules::flake8_2020::rules::SysVersionInfo1CmpInt,
    YTT204 => rules::flake8_2020::rules::SysVersionInfoMinorCmpInt,
    YTT301 => rules::flake8_2020::rules::SysVersion0Referenced,
    YTT302 => rules::flake8_2020::rules::SysVersionCmpStr10,
    YTT303 => rules::flake8_2020::rules::SysVersionSlice1Referenced,
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
    UP037 => rules::pyupgrade::rules::QuotedAnnotation,
    // pydocstyle
    D100 => rules::pydocstyle::rules::PublicModule,
    D101 => rules::pydocstyle::rules::PublicClass,
    D102 => rules::pydocstyle::rules::PublicMethod,
    D103 => rules::pydocstyle::rules::PublicFunction,
    D104 => rules::pydocstyle::rules::PublicPackage,
    D105 => rules::pydocstyle::rules::MagicMethod,
    D106 => rules::pydocstyle::rules::PublicNestedClass,
    D107 => rules::pydocstyle::rules::PublicInit,
    D200 => rules::pydocstyle::rules::FitsOnOneLine,
    D201 => rules::pydocstyle::rules::NoBlankLineBeforeFunction,
    D202 => rules::pydocstyle::rules::NoBlankLineAfterFunction,
    D203 => rules::pydocstyle::rules::OneBlankLineBeforeClass,
    D204 => rules::pydocstyle::rules::OneBlankLineAfterClass,
    D205 => rules::pydocstyle::rules::BlankLineAfterSummary,
    D206 => rules::pydocstyle::rules::IndentWithSpaces,
    D207 => rules::pydocstyle::rules::NoUnderIndentation,
    D208 => rules::pydocstyle::rules::NoOverIndentation,
    D209 => rules::pydocstyle::rules::NewLineAfterLastParagraph,
    D210 => rules::pydocstyle::rules::NoSurroundingWhitespace,
    D211 => rules::pydocstyle::rules::NoBlankLineBeforeClass,
    D212 => rules::pydocstyle::rules::MultiLineSummaryFirstLine,
    D213 => rules::pydocstyle::rules::MultiLineSummarySecondLine,
    D214 => rules::pydocstyle::rules::SectionNotOverIndented,
    D215 => rules::pydocstyle::rules::SectionUnderlineNotOverIndented,
    D300 => rules::pydocstyle::rules::UsesTripleQuotes,
    D301 => rules::pydocstyle::rules::UsesRPrefixForBackslashedContent,
    D400 => rules::pydocstyle::rules::EndsInPeriod,
    D401 => rules::pydocstyle::rules::NonImperativeMood,
    D402 => rules::pydocstyle::rules::NoSignature,
    D403 => rules::pydocstyle::rules::FirstLineCapitalized,
    D404 => rules::pydocstyle::rules::NoThisPrefix,
    D405 => rules::pydocstyle::rules::CapitalizeSectionName,
    D406 => rules::pydocstyle::rules::NewLineAfterSectionName,
    D407 => rules::pydocstyle::rules::DashedUnderlineAfterSection,
    D408 => rules::pydocstyle::rules::SectionUnderlineAfterName,
    D409 => rules::pydocstyle::rules::SectionUnderlineMatchesSectionLength,
    D410 => rules::pydocstyle::rules::BlankLineAfterSection,
    D411 => rules::pydocstyle::rules::BlankLineBeforeSection,
    D412 => rules::pydocstyle::rules::NoBlankLinesBetweenHeaderAndContent,
    D413 => rules::pydocstyle::rules::BlankLineAfterLastSection,
    D414 => rules::pydocstyle::rules::NonEmptySection,
    D415 => rules::pydocstyle::rules::EndsInPunctuation,
    D416 => rules::pydocstyle::rules::SectionNameEndsInColon,
    D417 => rules::pydocstyle::rules::DocumentAllArguments,
    D418 => rules::pydocstyle::rules::SkipDocstring,
    D419 => rules::pydocstyle::rules::NonEmpty,
    // pep8-naming
    N801 => rules::pep8_naming::rules::InvalidClassName,
    N802 => rules::pep8_naming::rules::InvalidFunctionName,
    N803 => rules::pep8_naming::rules::InvalidArgumentName,
    N804 => rules::pep8_naming::rules::InvalidFirstArgumentNameForClassMethod,
    N805 => rules::pep8_naming::rules::InvalidFirstArgumentNameForMethod,
    N806 => rules::pep8_naming::rules::NonLowercaseVariableInFunction,
    N807 => rules::pep8_naming::rules::DunderFunctionName,
    N811 => rules::pep8_naming::rules::ConstantImportedAsNonConstant,
    N812 => rules::pep8_naming::rules::LowercaseImportedAsNonLowercase,
    N813 => rules::pep8_naming::rules::CamelcaseImportedAsLowercase,
    N814 => rules::pep8_naming::rules::CamelcaseImportedAsConstant,
    N815 => rules::pep8_naming::rules::MixedCaseVariableInClassScope,
    N816 => rules::pep8_naming::rules::MixedCaseVariableInGlobalScope,
    N817 => rules::pep8_naming::rules::CamelcaseImportedAsAcronym,
    N818 => rules::pep8_naming::rules::ErrorSuffixOnExceptionName,
    // isort
    I001 => rules::isort::rules::UnsortedImports,
    I002 => rules::isort::rules::MissingRequiredImport,
    // eradicate
    ERA001 => rules::eradicate::rules::CommentedOutCode,
    // flake8-bandit
    S101 => rules::flake8_bandit::rules::AssertUsed,
    S102 => rules::flake8_bandit::rules::ExecUsed,
    S103 => rules::flake8_bandit::rules::BadFilePermissions,
    S104 => rules::flake8_bandit::rules::HardcodedBindAllInterfaces,
    S105 => rules::flake8_bandit::rules::HardcodedPasswordString,
    S106 => rules::flake8_bandit::rules::HardcodedPasswordFuncArg,
    S107 => rules::flake8_bandit::rules::HardcodedPasswordDefault,
    S108 => rules::flake8_bandit::rules::HardcodedTempFile,
    S110 => rules::flake8_bandit::rules::TryExceptPass,
    S113 => rules::flake8_bandit::rules::RequestWithoutTimeout,
    S324 => rules::flake8_bandit::rules::HashlibInsecureHashFunction,
    S501 => rules::flake8_bandit::rules::RequestWithNoCertValidation,
    S506 => rules::flake8_bandit::rules::UnsafeYAMLLoad,
    S508 => rules::flake8_bandit::rules::SnmpInsecureVersion,
    S509 => rules::flake8_bandit::rules::SnmpWeakCryptography,
    S612 => rules::flake8_bandit::rules::LoggingConfigInsecureListen,
    S701 => rules::flake8_bandit::rules::Jinja2AutoescapeFalse,
    // flake8-boolean-trap
    FBT001 => rules::flake8_boolean_trap::rules::BooleanPositionalArgInFunctionDefinition,
    FBT002 => rules::flake8_boolean_trap::rules::BooleanDefaultValueInFunctionDefinition,
    FBT003 => rules::flake8_boolean_trap::rules::BooleanPositionalValueInFunctionCall,
    // flake8-unused-arguments
    ARG001 => rules::flake8_unused_arguments::rules::UnusedFunctionArgument,
    ARG002 => rules::flake8_unused_arguments::rules::UnusedMethodArgument,
    ARG003 => rules::flake8_unused_arguments::rules::UnusedClassMethodArgument,
    ARG004 => rules::flake8_unused_arguments::rules::UnusedStaticMethodArgument,
    ARG005 => rules::flake8_unused_arguments::rules::UnusedLambdaArgument,
    // flake8-import-conventions
    ICN001 => rules::flake8_import_conventions::rules::UnconventionalImportAlias,
    // flake8-datetimez
    DTZ001 => rules::flake8_datetimez::rules::CallDatetimeWithoutTzinfo,
    DTZ002 => rules::flake8_datetimez::rules::CallDatetimeToday,
    DTZ003 => rules::flake8_datetimez::rules::CallDatetimeUtcnow,
    DTZ004 => rules::flake8_datetimez::rules::CallDatetimeUtcfromtimestamp,
    DTZ005 => rules::flake8_datetimez::rules::CallDatetimeNowWithoutTzinfo,
    DTZ006 => rules::flake8_datetimez::rules::CallDatetimeFromtimestamp,
    DTZ007 => rules::flake8_datetimez::rules::CallDatetimeStrptimeWithoutZone,
    DTZ011 => rules::flake8_datetimez::rules::CallDateToday,
    DTZ012 => rules::flake8_datetimez::rules::CallDateFromtimestamp,
    // pygrep-hooks
    PGH001 => rules::pygrep_hooks::rules::NoEval,
    PGH002 => rules::pygrep_hooks::rules::DeprecatedLogWarn,
    PGH003 => rules::pygrep_hooks::rules::BlanketTypeIgnore,
    PGH004 => rules::pygrep_hooks::rules::BlanketNOQA,
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
    EM101 => rules::flake8_errmsg::rules::RawStringInException,
    EM102 => rules::flake8_errmsg::rules::FStringInException,
    EM103 => rules::flake8_errmsg::rules::DotFormatInException,
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
    PIE810 => rules::flake8_pie::rules::SingleStartsEndsWith,
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
    RUF001 => rules::ruff::rules::AmbiguousUnicodeCharacterString,
    RUF002 => rules::ruff::rules::AmbiguousUnicodeCharacterDocstring,
    RUF003 => rules::ruff::rules::AmbiguousUnicodeCharacterComment,
    RUF004 => rules::ruff::rules::KeywordArgumentBeforeStarArgument,
    RUF005 => rules::ruff::rules::UnpackInsteadOfConcatenatingToCollectionLiteral,
    RUF100 => rules::ruff::rules::UnusedNOQA,
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

/// The prefix and name for an upstream linter category.
pub struct UpstreamCategory(pub RuleCodePrefix, pub &'static str);

impl Linter {
    pub const fn upstream_categories(&self) -> Option<&'static [UpstreamCategory]> {
        match self {
            Linter::Pycodestyle => Some(&[
                UpstreamCategory(RuleCodePrefix::E, "Error"),
                UpstreamCategory(RuleCodePrefix::W, "Warning"),
            ]),
            Linter::Pylint => Some(&[
                UpstreamCategory(RuleCodePrefix::PLC, "Convention"),
                UpstreamCategory(RuleCodePrefix::PLE, "Error"),
                UpstreamCategory(RuleCodePrefix::PLR, "Refactor"),
                UpstreamCategory(RuleCodePrefix::PLW, "Warning"),
            ]),
            _ => None,
        }
    }
}

pub enum LintSource {
    Ast,
    Io,
    PhysicalLines,
    LogicalLines,
    Tokens,
    Imports,
    NoQa,
    Filesystem,
}

impl Rule {
    /// The source for the diagnostic (either the AST, the filesystem, or the
    /// physical lines).
    pub const fn lint_source(&self) -> &'static LintSource {
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
            | Rule::ShebangWhitespace => &LintSource::PhysicalLines,
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
            #[cfg(feature = "logical_lines")]
            Rule::IndentationWithInvalidMultiple
            | Rule::IndentationWithInvalidMultipleComment
            | Rule::MultipleLeadingHashesForBlockComment
            | Rule::MultipleSpacesAfterKeyword
            | Rule::MultipleSpacesAfterOperator
            | Rule::MultipleSpacesBeforeKeyword
            | Rule::MultipleSpacesBeforeOperator
            | Rule::NoIndentedBlock
            | Rule::NoIndentedBlockComment
            | Rule::NoSpaceAfterBlockComment
            | Rule::NoSpaceAfterInlineComment
            | Rule::OverIndented
            | Rule::TabAfterKeyword
            | Rule::TabAfterOperator
            | Rule::TabBeforeKeyword
            | Rule::TabBeforeOperator
            | Rule::TooFewSpacesBeforeInlineComment
            | Rule::UnexpectedIndentation
            | Rule::UnexpectedIndentationComment
            | Rule::WhitespaceAfterOpenBracket
            | Rule::WhitespaceBeforeCloseBracket
            | Rule::WhitespaceBeforePunctuation => &LintSource::LogicalLines,
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
