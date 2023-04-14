//! Registry of all [`Rule`] implementations.

mod rule_set;

use strum_macros::{AsRefStr, EnumIter};

use ruff_diagnostics::Violation;
use ruff_macros::RuleNamespace;

use crate::codes::{self, RuleCodePrefix};
use crate::rules;
pub use rule_set::{RuleSet, RuleSetIterator};

ruff_macros::register_rules!(
    // pycodestyle errors
    rules::pycodestyle::rules::MixedSpacesAndTabs,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::IndentationWithInvalidMultiple,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::NoIndentedBlock,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::UnexpectedIndentation,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::IndentationWithInvalidMultipleComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::NoIndentedBlockComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::UnexpectedIndentationComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::OverIndented,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::WhitespaceAfterOpenBracket,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::WhitespaceBeforeCloseBracket,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::WhitespaceBeforePunctuation,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MultipleSpacesBeforeOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::TabBeforeOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::TabAfterOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::TooFewSpacesBeforeInlineComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::NoSpaceAfterInlineComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::NoSpaceAfterBlockComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MultipleLeadingHashesForBlockComment,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterKeyword,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespace,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespaceAfterKeyword,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MultipleSpacesBeforeKeyword,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundArithmeticOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundBitwiseOrShiftOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundModuloOperator,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::TabAfterKeyword,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::UnexpectedSpacesAroundKeywordParameterEquals,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundParameterEquals,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::WhitespaceBeforeParameters,
    #[cfg(feature = "logical_lines")]
    rules::pycodestyle::rules::logical_lines::TabBeforeKeyword,
    rules::pycodestyle::rules::MultipleImportsOnOneLine,
    rules::pycodestyle::rules::ModuleImportNotAtTopOfFile,
    rules::pycodestyle::rules::LineTooLong,
    rules::pycodestyle::rules::MultipleStatementsOnOneLineColon,
    rules::pycodestyle::rules::MultipleStatementsOnOneLineSemicolon,
    rules::pycodestyle::rules::UselessSemicolon,
    rules::pycodestyle::rules::NoneComparison,
    rules::pycodestyle::rules::TrueFalseComparison,
    rules::pycodestyle::rules::NotInTest,
    rules::pycodestyle::rules::NotIsTest,
    rules::pycodestyle::rules::TypeComparison,
    rules::pycodestyle::rules::BareExcept,
    rules::pycodestyle::rules::LambdaAssignment,
    rules::pycodestyle::rules::AmbiguousVariableName,
    rules::pycodestyle::rules::AmbiguousClassName,
    rules::pycodestyle::rules::AmbiguousFunctionName,
    rules::pycodestyle::rules::IOError,
    rules::pycodestyle::rules::SyntaxError,
    // pycodestyle warnings
    rules::pycodestyle::rules::TabIndentation,
    rules::pycodestyle::rules::TrailingWhitespace,
    rules::pycodestyle::rules::MissingNewlineAtEndOfFile,
    rules::pycodestyle::rules::BlankLineWithWhitespace,
    rules::pycodestyle::rules::DocLineTooLong,
    rules::pycodestyle::rules::InvalidEscapeSequence,
    // pyflakes
    rules::pyflakes::rules::UnusedImport,
    rules::pyflakes::rules::ImportShadowedByLoopVar,
    rules::pyflakes::rules::UndefinedLocalWithImportStar,
    rules::pyflakes::rules::LateFutureImport,
    rules::pyflakes::rules::UndefinedLocalWithImportStarUsage,
    rules::pyflakes::rules::UndefinedLocalWithNestedImportStarUsage,
    rules::pyflakes::rules::FutureFeatureNotDefined,
    rules::pyflakes::rules::PercentFormatInvalidFormat,
    rules::pyflakes::rules::PercentFormatExpectedMapping,
    rules::pyflakes::rules::PercentFormatExpectedSequence,
    rules::pyflakes::rules::PercentFormatExtraNamedArguments,
    rules::pyflakes::rules::PercentFormatMissingArgument,
    rules::pyflakes::rules::PercentFormatMixedPositionalAndNamed,
    rules::pyflakes::rules::PercentFormatPositionalCountMismatch,
    rules::pyflakes::rules::PercentFormatStarRequiresSequence,
    rules::pyflakes::rules::PercentFormatUnsupportedFormatCharacter,
    rules::pyflakes::rules::StringDotFormatInvalidFormat,
    rules::pyflakes::rules::StringDotFormatExtraNamedArguments,
    rules::pyflakes::rules::StringDotFormatExtraPositionalArguments,
    rules::pyflakes::rules::StringDotFormatMissingArguments,
    rules::pyflakes::rules::StringDotFormatMixingAutomatic,
    rules::pyflakes::rules::FStringMissingPlaceholders,
    rules::pyflakes::rules::MultiValueRepeatedKeyLiteral,
    rules::pyflakes::rules::MultiValueRepeatedKeyVariable,
    rules::pyflakes::rules::ExpressionsInStarAssignment,
    rules::pyflakes::rules::MultipleStarredExpressions,
    rules::pyflakes::rules::AssertTuple,
    rules::pyflakes::rules::IsLiteral,
    rules::pyflakes::rules::InvalidPrintSyntax,
    rules::pyflakes::rules::IfTuple,
    rules::pyflakes::rules::BreakOutsideLoop,
    rules::pyflakes::rules::ContinueOutsideLoop,
    rules::pyflakes::rules::YieldOutsideFunction,
    rules::pyflakes::rules::ReturnOutsideFunction,
    rules::pyflakes::rules::DefaultExceptNotLast,
    rules::pyflakes::rules::ForwardAnnotationSyntaxError,
    rules::pyflakes::rules::RedefinedWhileUnused,
    rules::pyflakes::rules::UndefinedName,
    rules::pyflakes::rules::UndefinedExport,
    rules::pyflakes::rules::UndefinedLocal,
    rules::pyflakes::rules::UnusedVariable,
    rules::pyflakes::rules::UnusedAnnotation,
    rules::pyflakes::rules::RaiseNotImplemented,
    // pylint
    rules::pylint::rules::AssertOnStringLiteral,
    rules::pylint::rules::UselessReturn,
    rules::pylint::rules::YieldInInit,
    rules::pylint::rules::InvalidAllObject,
    rules::pylint::rules::InvalidAllFormat,
    rules::pylint::rules::InvalidEnvvarDefault,
    rules::pylint::rules::InvalidEnvvarValue,
    rules::pylint::rules::BadStringFormatType,
    rules::pylint::rules::BidirectionalUnicode,
    rules::pylint::rules::BinaryOpException,
    rules::pylint::rules::InvalidCharacterBackspace,
    rules::pylint::rules::InvalidCharacterSub,
    rules::pylint::rules::InvalidCharacterEsc,
    rules::pylint::rules::InvalidCharacterNul,
    rules::pylint::rules::InvalidCharacterZeroWidthSpace,
    rules::pylint::rules::BadStrStripCall,
    rules::pylint::rules::CollapsibleElseIf,
    rules::pylint::rules::ContinueInFinally,
    rules::pylint::rules::UselessImportAlias,
    rules::pylint::rules::UnnecessaryDirectLambdaCall,
    rules::pylint::rules::NonlocalWithoutBinding,
    rules::pylint::rules::LoadBeforeGlobalDeclaration,
    rules::pylint::rules::AwaitOutsideAsync,
    rules::pylint::rules::PropertyWithParameters,
    rules::pylint::rules::ReturnInInit,
    rules::pylint::rules::ManualFromImport,
    rules::pylint::rules::CompareToEmptyString,
    rules::pylint::rules::ComparisonOfConstant,
    rules::pylint::rules::RepeatedIsinstanceCalls,
    rules::pylint::rules::SysExitAlias,
    rules::pylint::rules::MagicValueComparison,
    rules::pylint::rules::UselessElseOnLoop,
    rules::pylint::rules::GlobalStatement,
    rules::pylint::rules::GlobalVariableNotAssigned,
    rules::pylint::rules::TooManyReturnStatements,
    rules::pylint::rules::TooManyArguments,
    rules::pylint::rules::TooManyBranches,
    rules::pylint::rules::TooManyStatements,
    rules::pylint::rules::RedefinedLoopName,
    rules::pylint::rules::LoggingTooFewArgs,
    rules::pylint::rules::LoggingTooManyArgs,
    // flake8-builtins
    rules::flake8_builtins::rules::BuiltinVariableShadowing,
    rules::flake8_builtins::rules::BuiltinArgumentShadowing,
    rules::flake8_builtins::rules::BuiltinAttributeShadowing,
    // flake8-bugbear
    rules::flake8_bugbear::rules::UnaryPrefixIncrement,
    rules::flake8_bugbear::rules::AssignmentToOsEnviron,
    rules::flake8_bugbear::rules::UnreliableCallableCheck,
    rules::flake8_bugbear::rules::StripWithMultiCharacters,
    rules::flake8_bugbear::rules::MutableArgumentDefault,
    rules::flake8_bugbear::rules::NoExplicitStacklevel,
    rules::flake8_bugbear::rules::UnusedLoopControlVariable,
    rules::flake8_bugbear::rules::FunctionCallInDefaultArgument,
    rules::flake8_bugbear::rules::GetAttrWithConstant,
    rules::flake8_bugbear::rules::SetAttrWithConstant,
    rules::flake8_bugbear::rules::AssertFalse,
    rules::flake8_bugbear::rules::JumpStatementInFinally,
    rules::flake8_bugbear::rules::RedundantTupleInExceptionHandler,
    rules::flake8_bugbear::rules::DuplicateHandlerException,
    rules::flake8_bugbear::rules::UselessComparison,
    rules::flake8_bugbear::rules::CannotRaiseLiteral,
    rules::flake8_bugbear::rules::AssertRaisesException,
    rules::flake8_bugbear::rules::UselessExpression,
    rules::flake8_bugbear::rules::CachedInstanceMethod,
    rules::flake8_bugbear::rules::LoopVariableOverridesIterator,
    rules::flake8_bugbear::rules::FStringDocstring,
    rules::flake8_bugbear::rules::UselessContextlibSuppress,
    rules::flake8_bugbear::rules::FunctionUsesLoopVariable,
    rules::flake8_bugbear::rules::AbstractBaseClassWithoutAbstractMethod,
    rules::flake8_bugbear::rules::DuplicateTryBlockException,
    rules::flake8_bugbear::rules::StarArgUnpackingAfterKeywordArg,
    rules::flake8_bugbear::rules::EmptyMethodWithoutAbstractDecorator,
    rules::flake8_bugbear::rules::RaiseWithoutFromInsideExcept,
    rules::flake8_bugbear::rules::ZipWithoutExplicitStrict,
    rules::flake8_bugbear::rules::ExceptWithEmptyTuple,
    rules::flake8_bugbear::rules::ExceptWithNonExceptionClasses,
    rules::flake8_bugbear::rules::ReuseOfGroupbyGenerator,
    rules::flake8_bugbear::rules::UnintentionalTypeAnnotation,
    // flake8-blind-except
    rules::flake8_blind_except::rules::BlindExcept,
    // flake8-comprehensions
    rules::flake8_comprehensions::rules::UnnecessaryCallAroundSorted,
    rules::flake8_comprehensions::rules::UnnecessaryCollectionCall,
    rules::flake8_comprehensions::rules::UnnecessaryComprehension,
    rules::flake8_comprehensions::rules::UnnecessaryComprehensionAnyAll,
    rules::flake8_comprehensions::rules::UnnecessaryDoubleCastOrProcess,
    rules::flake8_comprehensions::rules::UnnecessaryGeneratorDict,
    rules::flake8_comprehensions::rules::UnnecessaryGeneratorList,
    rules::flake8_comprehensions::rules::UnnecessaryGeneratorSet,
    rules::flake8_comprehensions::rules::UnnecessaryListCall,
    rules::flake8_comprehensions::rules::UnnecessaryListComprehensionDict,
    rules::flake8_comprehensions::rules::UnnecessaryListComprehensionSet,
    rules::flake8_comprehensions::rules::UnnecessaryLiteralDict,
    rules::flake8_comprehensions::rules::UnnecessaryLiteralSet,
    rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinDictCall,
    rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinListCall,
    rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinTupleCall,
    rules::flake8_comprehensions::rules::UnnecessaryMap,
    rules::flake8_comprehensions::rules::UnnecessarySubscriptReversal,
    // flake8-debugger
    rules::flake8_debugger::rules::Debugger,
    // mccabe
    rules::mccabe::rules::ComplexStructure,
    // flake8-tidy-imports
    rules::flake8_tidy_imports::banned_api::BannedApi,
    rules::flake8_tidy_imports::relative_imports::RelativeImports,
    // flake8-return
    rules::flake8_return::rules::UnnecessaryReturnNone,
    rules::flake8_return::rules::ImplicitReturnValue,
    rules::flake8_return::rules::ImplicitReturn,
    rules::flake8_return::rules::UnnecessaryAssign,
    rules::flake8_return::rules::SuperfluousElseReturn,
    rules::flake8_return::rules::SuperfluousElseRaise,
    rules::flake8_return::rules::SuperfluousElseContinue,
    rules::flake8_return::rules::SuperfluousElseBreak,
    // flake8-implicit-str-concat
    rules::flake8_implicit_str_concat::rules::SingleLineImplicitStringConcatenation,
    rules::flake8_implicit_str_concat::rules::MultiLineImplicitStringConcatenation,
    rules::flake8_implicit_str_concat::rules::ExplicitStringConcatenation,
    // flake8-print
    rules::flake8_print::rules::Print,
    rules::flake8_print::rules::PPrint,
    // flake8-quotes
    rules::flake8_quotes::rules::BadQuotesInlineString,
    rules::flake8_quotes::rules::BadQuotesMultilineString,
    rules::flake8_quotes::rules::BadQuotesDocstring,
    rules::flake8_quotes::rules::AvoidableEscapedQuote,
    // flake8-annotations
    rules::flake8_annotations::rules::MissingTypeFunctionArgument,
    rules::flake8_annotations::rules::MissingTypeArgs,
    rules::flake8_annotations::rules::MissingTypeKwargs,
    rules::flake8_annotations::rules::MissingTypeSelf,
    rules::flake8_annotations::rules::MissingTypeCls,
    rules::flake8_annotations::rules::MissingReturnTypeUndocumentedPublicFunction,
    rules::flake8_annotations::rules::MissingReturnTypePrivateFunction,
    rules::flake8_annotations::rules::MissingReturnTypeSpecialMethod,
    rules::flake8_annotations::rules::MissingReturnTypeStaticMethod,
    rules::flake8_annotations::rules::MissingReturnTypeClassMethod,
    rules::flake8_annotations::rules::AnyType,
    // flake8-2020
    rules::flake8_2020::rules::SysVersionSlice3,
    rules::flake8_2020::rules::SysVersion2,
    rules::flake8_2020::rules::SysVersionCmpStr3,
    rules::flake8_2020::rules::SysVersionInfo0Eq3,
    rules::flake8_2020::rules::SixPY3,
    rules::flake8_2020::rules::SysVersionInfo1CmpInt,
    rules::flake8_2020::rules::SysVersionInfoMinorCmpInt,
    rules::flake8_2020::rules::SysVersion0,
    rules::flake8_2020::rules::SysVersionCmpStr10,
    rules::flake8_2020::rules::SysVersionSlice1,
    // flake8-simplify
    rules::flake8_simplify::rules::IfElseBlockInsteadOfDictLookup,
    rules::flake8_simplify::rules::DuplicateIsinstanceCall,
    rules::flake8_simplify::rules::CollapsibleIf,
    rules::flake8_simplify::rules::NeedlessBool,
    rules::flake8_simplify::rules::SuppressibleException,
    rules::flake8_simplify::rules::ReturnInTryExceptFinally,
    rules::flake8_simplify::rules::IfElseBlockInsteadOfIfExp,
    rules::flake8_simplify::rules::CompareWithTuple,
    rules::flake8_simplify::rules::ReimplementedBuiltin,
    rules::flake8_simplify::rules::UncapitalizedEnvironmentVariables,
    rules::flake8_simplify::rules::IfWithSameArms,
    rules::flake8_simplify::rules::OpenFileWithContextHandler,
    rules::flake8_simplify::rules::MultipleWithStatements,
    rules::flake8_simplify::rules::InDictKeys,
    rules::flake8_simplify::rules::NegateEqualOp,
    rules::flake8_simplify::rules::NegateNotEqualOp,
    rules::flake8_simplify::rules::DoubleNegation,
    rules::flake8_simplify::rules::IfExprWithTrueFalse,
    rules::flake8_simplify::rules::IfExprWithFalseTrue,
    rules::flake8_simplify::rules::IfExprWithTwistedArms,
    rules::flake8_simplify::rules::ExprAndNotExpr,
    rules::flake8_simplify::rules::ExprOrNotExpr,
    rules::flake8_simplify::rules::ExprOrTrue,
    rules::flake8_simplify::rules::ExprAndFalse,
    rules::flake8_simplify::rules::YodaConditions,
    rules::flake8_simplify::rules::IfElseBlockInsteadOfDictGet,
    rules::flake8_simplify::rules::DictGetWithNoneDefault,
    // pyupgrade
    rules::pyupgrade::rules::UselessMetaclassType,
    rules::pyupgrade::rules::TypeOfPrimitive,
    rules::pyupgrade::rules::UselessObjectInheritance,
    rules::pyupgrade::rules::DeprecatedUnittestAlias,
    rules::pyupgrade::rules::NonPEP585Annotation,
    rules::pyupgrade::rules::NonPEP604Annotation,
    rules::pyupgrade::rules::SuperCallWithParameters,
    rules::pyupgrade::rules::UTF8EncodingDeclaration,
    rules::pyupgrade::rules::UnnecessaryFutureImport,
    rules::pyupgrade::rules::LRUCacheWithoutParameters,
    rules::pyupgrade::rules::UnnecessaryEncodeUTF8,
    rules::pyupgrade::rules::ConvertTypedDictFunctionalToClass,
    rules::pyupgrade::rules::ConvertNamedTupleFunctionalToClass,
    rules::pyupgrade::rules::RedundantOpenModes,
    rules::pyupgrade::rules::DatetimeTimezoneUTC,
    rules::pyupgrade::rules::NativeLiterals,
    rules::pyupgrade::rules::TypingTextStrAlias,
    rules::pyupgrade::rules::OpenAlias,
    rules::pyupgrade::rules::ReplaceUniversalNewlines,
    rules::pyupgrade::rules::ReplaceStdoutStderr,
    rules::pyupgrade::rules::DeprecatedCElementTree,
    rules::pyupgrade::rules::OSErrorAlias,
    rules::pyupgrade::rules::UnicodeKindPrefix,
    rules::pyupgrade::rules::DeprecatedMockImport,
    rules::pyupgrade::rules::UnpackedListComprehension,
    rules::pyupgrade::rules::YieldInForLoop,
    rules::pyupgrade::rules::UnnecessaryBuiltinImport,
    rules::pyupgrade::rules::FormatLiterals,
    rules::pyupgrade::rules::PrintfStringFormatting,
    rules::pyupgrade::rules::FString,
    rules::pyupgrade::rules::LRUCacheWithMaxsizeNone,
    rules::pyupgrade::rules::ExtraneousParentheses,
    rules::pyupgrade::rules::DeprecatedImport,
    rules::pyupgrade::rules::OutdatedVersionBlock,
    rules::pyupgrade::rules::QuotedAnnotation,
    rules::pyupgrade::rules::NonPEP604Isinstance,
    // pydocstyle
    rules::pydocstyle::rules::UndocumentedPublicModule,
    rules::pydocstyle::rules::UndocumentedPublicClass,
    rules::pydocstyle::rules::UndocumentedPublicMethod,
    rules::pydocstyle::rules::UndocumentedPublicFunction,
    rules::pydocstyle::rules::UndocumentedPublicPackage,
    rules::pydocstyle::rules::UndocumentedMagicMethod,
    rules::pydocstyle::rules::UndocumentedPublicNestedClass,
    rules::pydocstyle::rules::UndocumentedPublicInit,
    rules::pydocstyle::rules::FitsOnOneLine,
    rules::pydocstyle::rules::NoBlankLineBeforeFunction,
    rules::pydocstyle::rules::NoBlankLineAfterFunction,
    rules::pydocstyle::rules::OneBlankLineBeforeClass,
    rules::pydocstyle::rules::OneBlankLineAfterClass,
    rules::pydocstyle::rules::BlankLineAfterSummary,
    rules::pydocstyle::rules::IndentWithSpaces,
    rules::pydocstyle::rules::UnderIndentation,
    rules::pydocstyle::rules::OverIndentation,
    rules::pydocstyle::rules::NewLineAfterLastParagraph,
    rules::pydocstyle::rules::SurroundingWhitespace,
    rules::pydocstyle::rules::BlankLineBeforeClass,
    rules::pydocstyle::rules::MultiLineSummaryFirstLine,
    rules::pydocstyle::rules::MultiLineSummarySecondLine,
    rules::pydocstyle::rules::SectionNotOverIndented,
    rules::pydocstyle::rules::SectionUnderlineNotOverIndented,
    rules::pydocstyle::rules::TripleSingleQuotes,
    rules::pydocstyle::rules::EscapeSequenceInDocstring,
    rules::pydocstyle::rules::EndsInPeriod,
    rules::pydocstyle::rules::NonImperativeMood,
    rules::pydocstyle::rules::NoSignature,
    rules::pydocstyle::rules::FirstLineCapitalized,
    rules::pydocstyle::rules::DocstringStartsWithThis,
    rules::pydocstyle::rules::CapitalizeSectionName,
    rules::pydocstyle::rules::NewLineAfterSectionName,
    rules::pydocstyle::rules::DashedUnderlineAfterSection,
    rules::pydocstyle::rules::SectionUnderlineAfterName,
    rules::pydocstyle::rules::SectionUnderlineMatchesSectionLength,
    rules::pydocstyle::rules::NoBlankLineAfterSection,
    rules::pydocstyle::rules::NoBlankLineBeforeSection,
    rules::pydocstyle::rules::BlankLinesBetweenHeaderAndContent,
    rules::pydocstyle::rules::BlankLineAfterLastSection,
    rules::pydocstyle::rules::EmptyDocstringSection,
    rules::pydocstyle::rules::EndsInPunctuation,
    rules::pydocstyle::rules::SectionNameEndsInColon,
    rules::pydocstyle::rules::UndocumentedParam,
    rules::pydocstyle::rules::OverloadWithDocstring,
    rules::pydocstyle::rules::EmptyDocstring,
    // pep8-naming
    rules::pep8_naming::rules::InvalidClassName,
    rules::pep8_naming::rules::InvalidFunctionName,
    rules::pep8_naming::rules::InvalidArgumentName,
    rules::pep8_naming::rules::InvalidFirstArgumentNameForClassMethod,
    rules::pep8_naming::rules::InvalidFirstArgumentNameForMethod,
    rules::pep8_naming::rules::NonLowercaseVariableInFunction,
    rules::pep8_naming::rules::DunderFunctionName,
    rules::pep8_naming::rules::ConstantImportedAsNonConstant,
    rules::pep8_naming::rules::LowercaseImportedAsNonLowercase,
    rules::pep8_naming::rules::CamelcaseImportedAsLowercase,
    rules::pep8_naming::rules::CamelcaseImportedAsConstant,
    rules::pep8_naming::rules::MixedCaseVariableInClassScope,
    rules::pep8_naming::rules::MixedCaseVariableInGlobalScope,
    rules::pep8_naming::rules::CamelcaseImportedAsAcronym,
    rules::pep8_naming::rules::ErrorSuffixOnExceptionName,
    rules::pep8_naming::rules::InvalidModuleName,
    // isort
    rules::isort::rules::UnsortedImports,
    rules::isort::rules::MissingRequiredImport,
    // eradicate
    rules::eradicate::rules::CommentedOutCode,
    // flake8-bandit
    rules::flake8_bandit::rules::Assert,
    rules::flake8_bandit::rules::BadFilePermissions,
    rules::flake8_bandit::rules::ExecBuiltin,
    rules::flake8_bandit::rules::HardcodedBindAllInterfaces,
    rules::flake8_bandit::rules::HardcodedPasswordDefault,
    rules::flake8_bandit::rules::HardcodedPasswordFuncArg,
    rules::flake8_bandit::rules::HardcodedPasswordString,
    rules::flake8_bandit::rules::HardcodedSQLExpression,
    rules::flake8_bandit::rules::HardcodedTempFile,
    rules::flake8_bandit::rules::HashlibInsecureHashFunction,
    rules::flake8_bandit::rules::Jinja2AutoescapeFalse,
    rules::flake8_bandit::rules::LoggingConfigInsecureListen,
    rules::flake8_bandit::rules::RequestWithNoCertValidation,
    rules::flake8_bandit::rules::RequestWithoutTimeout,
    rules::flake8_bandit::rules::SnmpInsecureVersion,
    rules::flake8_bandit::rules::SnmpWeakCryptography,
    rules::flake8_bandit::rules::SubprocessPopenWithShellEqualsTrue,
    rules::flake8_bandit::rules::SubprocessWithoutShellEqualsTrue,
    rules::flake8_bandit::rules::CallWithShellEqualsTrue,
    rules::flake8_bandit::rules::StartProcessWithAShell,
    rules::flake8_bandit::rules::StartProcessWithNoShell,
    rules::flake8_bandit::rules::StartProcessWithPartialPath,
    rules::flake8_bandit::rules::SuspiciousEvalUsage,
    rules::flake8_bandit::rules::SuspiciousFTPLibUsage,
    rules::flake8_bandit::rules::SuspiciousInsecureCipherUsage,
    rules::flake8_bandit::rules::SuspiciousInsecureCipherModeUsage,
    rules::flake8_bandit::rules::SuspiciousInsecureHashUsage,
    rules::flake8_bandit::rules::SuspiciousMarkSafeUsage,
    rules::flake8_bandit::rules::SuspiciousMarshalUsage,
    rules::flake8_bandit::rules::SuspiciousMktempUsage,
    rules::flake8_bandit::rules::SuspiciousNonCryptographicRandomUsage,
    rules::flake8_bandit::rules::SuspiciousPickleUsage,
    rules::flake8_bandit::rules::SuspiciousTelnetUsage,
    rules::flake8_bandit::rules::SuspiciousURLOpenUsage,
    rules::flake8_bandit::rules::SuspiciousUnverifiedContextUsage,
    rules::flake8_bandit::rules::SuspiciousXMLCElementTreeUsage,
    rules::flake8_bandit::rules::SuspiciousXMLETreeUsage,
    rules::flake8_bandit::rules::SuspiciousXMLElementTreeUsage,
    rules::flake8_bandit::rules::SuspiciousXMLExpatBuilderUsage,
    rules::flake8_bandit::rules::SuspiciousXMLExpatReaderUsage,
    rules::flake8_bandit::rules::SuspiciousXMLMiniDOMUsage,
    rules::flake8_bandit::rules::SuspiciousXMLPullDOMUsage,
    rules::flake8_bandit::rules::SuspiciousXMLSaxUsage,
    rules::flake8_bandit::rules::TryExceptContinue,
    rules::flake8_bandit::rules::TryExceptPass,
    rules::flake8_bandit::rules::UnsafeYAMLLoad,
    // flake8-boolean-trap
    rules::flake8_boolean_trap::rules::BooleanPositionalArgInFunctionDefinition,
    rules::flake8_boolean_trap::rules::BooleanDefaultValueInFunctionDefinition,
    rules::flake8_boolean_trap::rules::BooleanPositionalValueInFunctionCall,
    // flake8-unused-arguments
    rules::flake8_unused_arguments::rules::UnusedFunctionArgument,
    rules::flake8_unused_arguments::rules::UnusedMethodArgument,
    rules::flake8_unused_arguments::rules::UnusedClassMethodArgument,
    rules::flake8_unused_arguments::rules::UnusedStaticMethodArgument,
    rules::flake8_unused_arguments::rules::UnusedLambdaArgument,
    // flake8-import-conventions
    rules::flake8_import_conventions::rules::UnconventionalImportAlias,
    rules::flake8_import_conventions::rules::BannedImportAlias,
    // flake8-datetimez
    rules::flake8_datetimez::rules::CallDatetimeWithoutTzinfo,
    rules::flake8_datetimez::rules::CallDatetimeToday,
    rules::flake8_datetimez::rules::CallDatetimeUtcnow,
    rules::flake8_datetimez::rules::CallDatetimeUtcfromtimestamp,
    rules::flake8_datetimez::rules::CallDatetimeNowWithoutTzinfo,
    rules::flake8_datetimez::rules::CallDatetimeFromtimestamp,
    rules::flake8_datetimez::rules::CallDatetimeStrptimeWithoutZone,
    rules::flake8_datetimez::rules::CallDateToday,
    rules::flake8_datetimez::rules::CallDateFromtimestamp,
    // pygrep-hooks
    rules::pygrep_hooks::rules::Eval,
    rules::pygrep_hooks::rules::DeprecatedLogWarn,
    rules::pygrep_hooks::rules::BlanketTypeIgnore,
    rules::pygrep_hooks::rules::BlanketNOQA,
    // pandas-vet
    rules::pandas_vet::rules::PandasUseOfInplaceArgument,
    rules::pandas_vet::rules::PandasUseOfDotIsNull,
    rules::pandas_vet::rules::PandasUseOfDotNotNull,
    rules::pandas_vet::rules::PandasUseOfDotIx,
    rules::pandas_vet::rules::PandasUseOfDotAt,
    rules::pandas_vet::rules::PandasUseOfDotIat,
    rules::pandas_vet::rules::PandasUseOfDotPivotOrUnstack,
    rules::pandas_vet::rules::PandasUseOfDotValues,
    rules::pandas_vet::rules::PandasUseOfDotReadTable,
    rules::pandas_vet::rules::PandasUseOfDotStack,
    rules::pandas_vet::rules::PandasUseOfPdMerge,
    rules::pandas_vet::rules::PandasDfVariableName,
    // flake8-errmsg
    rules::flake8_errmsg::rules::RawStringInException,
    rules::flake8_errmsg::rules::FStringInException,
    rules::flake8_errmsg::rules::DotFormatInException,
    // flake8-pyi
    rules::flake8_pyi::rules::ArgumentDefaultInStub,
    rules::flake8_pyi::rules::AssignmentDefaultInStub,
    rules::flake8_pyi::rules::BadVersionInfoComparison,
    rules::flake8_pyi::rules::DocstringInStub,
    rules::flake8_pyi::rules::NonEmptyStubBody,
    rules::flake8_pyi::rules::PassStatementStubBody,
    rules::flake8_pyi::rules::TypeCommentInStub,
    rules::flake8_pyi::rules::TypedArgumentDefaultInStub,
    rules::flake8_pyi::rules::UnprefixedTypeParam,
    rules::flake8_pyi::rules::UnrecognizedPlatformCheck,
    rules::flake8_pyi::rules::UnrecognizedPlatformName,
    rules::flake8_pyi::rules::PassInClassBody,
    rules::flake8_pyi::rules::DuplicateUnionMember,
    // flake8-pytest-style
    rules::flake8_pytest_style::rules::PytestFixtureIncorrectParenthesesStyle,
    rules::flake8_pytest_style::rules::PytestFixturePositionalArgs,
    rules::flake8_pytest_style::rules::PytestExtraneousScopeFunction,
    rules::flake8_pytest_style::rules::PytestMissingFixtureNameUnderscore,
    rules::flake8_pytest_style::rules::PytestIncorrectFixtureNameUnderscore,
    rules::flake8_pytest_style::rules::PytestParametrizeNamesWrongType,
    rules::flake8_pytest_style::rules::PytestParametrizeValuesWrongType,
    rules::flake8_pytest_style::rules::PytestPatchWithLambda,
    rules::flake8_pytest_style::rules::PytestUnittestAssertion,
    rules::flake8_pytest_style::rules::PytestRaisesWithoutException,
    rules::flake8_pytest_style::rules::PytestRaisesTooBroad,
    rules::flake8_pytest_style::rules::PytestRaisesWithMultipleStatements,
    rules::flake8_pytest_style::rules::PytestIncorrectPytestImport,
    rules::flake8_pytest_style::rules::PytestAssertAlwaysFalse,
    rules::flake8_pytest_style::rules::PytestFailWithoutMessage,
    rules::flake8_pytest_style::rules::PytestAssertInExcept,
    rules::flake8_pytest_style::rules::PytestCompositeAssertion,
    rules::flake8_pytest_style::rules::PytestFixtureParamWithoutValue,
    rules::flake8_pytest_style::rules::PytestDeprecatedYieldFixture,
    rules::flake8_pytest_style::rules::PytestFixtureFinalizerCallback,
    rules::flake8_pytest_style::rules::PytestUselessYieldFixture,
    rules::flake8_pytest_style::rules::PytestIncorrectMarkParenthesesStyle,
    rules::flake8_pytest_style::rules::PytestUnnecessaryAsyncioMarkOnFixture,
    rules::flake8_pytest_style::rules::PytestErroneousUseFixturesOnFixture,
    rules::flake8_pytest_style::rules::PytestUseFixturesWithoutParameters,
    // flake8-pie
    rules::flake8_pie::rules::UnnecessaryPass,
    rules::flake8_pie::rules::DuplicateClassFieldDefinition,
    rules::flake8_pie::rules::NonUniqueEnums,
    rules::flake8_pie::rules::UnnecessarySpread,
    rules::flake8_pie::rules::UnnecessaryDictKwargs,
    rules::flake8_pie::rules::ReimplementedListBuiltin,
    rules::flake8_pie::rules::MultipleStartsEndsWith,
    // flake8-commas
    rules::flake8_commas::rules::MissingTrailingComma,
    rules::flake8_commas::rules::TrailingCommaOnBareTuple,
    rules::flake8_commas::rules::ProhibitedTrailingComma,
    // flake8-no-pep420
    rules::flake8_no_pep420::rules::ImplicitNamespacePackage,
    // flake8-executable
    rules::flake8_executable::rules::ShebangNotExecutable,
    rules::flake8_executable::rules::ShebangMissingExecutableFile,
    rules::flake8_executable::rules::ShebangMissingPython,
    rules::flake8_executable::rules::ShebangLeadingWhitespace,
    rules::flake8_executable::rules::ShebangNotFirstLine,
    // flake8-type-checking
    rules::flake8_type_checking::rules::TypingOnlyFirstPartyImport,
    rules::flake8_type_checking::rules::TypingOnlyThirdPartyImport,
    rules::flake8_type_checking::rules::TypingOnlyStandardLibraryImport,
    rules::flake8_type_checking::rules::RuntimeImportInTypeCheckingBlock,
    rules::flake8_type_checking::rules::EmptyTypeCheckingBlock,
    // tryceratops
    rules::tryceratops::rules::RaiseVanillaClass,
    rules::tryceratops::rules::RaiseVanillaArgs,
    rules::tryceratops::rules::TypeCheckWithoutTypeError,
    rules::tryceratops::rules::ReraiseNoCause,
    rules::tryceratops::rules::VerboseRaise,
    rules::tryceratops::rules::TryConsiderElse,
    rules::tryceratops::rules::RaiseWithinTry,
    rules::tryceratops::rules::ErrorInsteadOfException,
    rules::tryceratops::rules::VerboseLogMessage,
    // flake8-use-pathlib
    rules::flake8_use_pathlib::violations::OsPathAbspath,
    rules::flake8_use_pathlib::violations::OsChmod,
    rules::flake8_use_pathlib::violations::OsMkdir,
    rules::flake8_use_pathlib::violations::OsMakedirs,
    rules::flake8_use_pathlib::violations::OsRename,
    rules::flake8_use_pathlib::violations::PathlibReplace,
    rules::flake8_use_pathlib::violations::OsRmdir,
    rules::flake8_use_pathlib::violations::OsRemove,
    rules::flake8_use_pathlib::violations::OsUnlink,
    rules::flake8_use_pathlib::violations::OsGetcwd,
    rules::flake8_use_pathlib::violations::OsPathExists,
    rules::flake8_use_pathlib::violations::OsPathExpanduser,
    rules::flake8_use_pathlib::violations::OsPathIsdir,
    rules::flake8_use_pathlib::violations::OsPathIsfile,
    rules::flake8_use_pathlib::violations::OsPathIslink,
    rules::flake8_use_pathlib::violations::OsReadlink,
    rules::flake8_use_pathlib::violations::OsStat,
    rules::flake8_use_pathlib::violations::OsPathIsabs,
    rules::flake8_use_pathlib::violations::OsPathJoin,
    rules::flake8_use_pathlib::violations::OsPathBasename,
    rules::flake8_use_pathlib::violations::OsPathDirname,
    rules::flake8_use_pathlib::violations::OsPathSamefile,
    rules::flake8_use_pathlib::violations::OsPathSplitext,
    rules::flake8_use_pathlib::violations::BuiltinOpen,
    rules::flake8_use_pathlib::violations::PyPath,
    // flake8-logging-format
    rules::flake8_logging_format::violations::LoggingStringFormat,
    rules::flake8_logging_format::violations::LoggingPercentFormat,
    rules::flake8_logging_format::violations::LoggingStringConcat,
    rules::flake8_logging_format::violations::LoggingFString,
    rules::flake8_logging_format::violations::LoggingWarn,
    rules::flake8_logging_format::violations::LoggingExtraAttrClash,
    rules::flake8_logging_format::violations::LoggingExcInfo,
    rules::flake8_logging_format::violations::LoggingRedundantExcInfo,
    // flake8-raise
    rules::flake8_raise::rules::UnnecessaryParenOnRaiseException,
    // flake8-self
    rules::flake8_self::rules::PrivateMemberAccess,
    // flake8-gettext
    rules::flake8_gettext::rules::FStringInGetTextFuncCall,
    rules::flake8_gettext::rules::FormatInGetTextFuncCall,
    rules::flake8_gettext::rules::PrintfInGetTextFuncCall,
    // numpy
    rules::numpy::rules::NumpyDeprecatedTypeAlias,
    rules::numpy::rules::NumpyLegacyRandom,
    // ruff
    rules::ruff::rules::AmbiguousUnicodeCharacterString,
    rules::ruff::rules::AmbiguousUnicodeCharacterDocstring,
    rules::ruff::rules::AmbiguousUnicodeCharacterComment,
    rules::ruff::rules::CollectionLiteralConcatenation,
    rules::ruff::rules::AsyncioDanglingTask,
    rules::ruff::rules::UnusedNOQA,
    rules::ruff::rules::PairwiseOverZipped,
    rules::ruff::rules::MutableDataclassDefault,
    rules::ruff::rules::FunctionCallInDataclassDefaultArgument,
    // flake8-django
    rules::flake8_django::rules::DjangoNullableModelStringField,
    rules::flake8_django::rules::DjangoLocalsInRenderFunction,
    rules::flake8_django::rules::DjangoExcludeWithModelForm,
    rules::flake8_django::rules::DjangoAllWithModelForm,
    rules::flake8_django::rules::DjangoModelWithoutDunderStr,
    rules::flake8_django::rules::DjangoUnorderedBodyContentInModel,
    rules::flake8_django::rules::DjangoNonLeadingReceiverDecorator,
);

pub trait AsRule {
    fn rule(&self) -> Rule;
}

impl Rule {
    pub fn from_code(code: &str) -> Result<Self, FromCodeError> {
        let (linter, code) = Linter::parse_code(code).ok_or(FromCodeError::Unknown)?;
        let prefix: RuleCodePrefix = RuleCodePrefix::parse(&linter, code)?;
        Ok(prefix.into_iter().next().unwrap())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FromCodeError {
    #[error("unknown rule code")]
    Unknown,
}

#[derive(EnumIter, Debug, PartialEq, Eq, Clone, Hash, RuleNamespace)]
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
    /// [flake8-django](https://pypi.org/project/flake8-django/)
    #[prefix = "DJ"]
    Flake8Django,
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
    /// [flake8-pyi](https://pypi.org/project/flake8-pyi/)
    #[prefix = "PYI"]
    Flake8Pyi,
    /// [flake8-pytest-style](https://pypi.org/project/flake8-pytest-style/)
    #[prefix = "PT"]
    Flake8PytestStyle,
    /// [flake8-quotes](https://pypi.org/project/flake8-quotes/)
    #[prefix = "Q"]
    Flake8Quotes,
    /// [flake8-raise](https://pypi.org/project/flake8-raise/)
    #[prefix = "RSE"]
    Flake8Raise,
    /// [flake8-return](https://pypi.org/project/flake8-return/)
    #[prefix = "RET"]
    Flake8Return,
    /// [flake8-self](https://pypi.org/project/flake8-self/)
    #[prefix = "SLF"]
    Flake8Self,
    /// [flake8-simplify](https://pypi.org/project/flake8-simplify/)
    #[prefix = "SIM"]
    Flake8Simplify,
    /// [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/)
    #[prefix = "TID"]
    Flake8TidyImports,
    /// [flake8-type-checking](https://pypi.org/project/flake8-type-checking/)
    #[prefix = "TCH"]
    Flake8TypeChecking,
    /// [flake8-gettext](https://pypi.org/project/flake8-gettext/)
    #[prefix = "INT"]
    Flake8GetText,
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
    /// NumPy-specific rules
    #[prefix = "NPY"]
    Numpy,
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
                UpstreamCategory(RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E), "Error"),
                UpstreamCategory(
                    RuleCodePrefix::Pycodestyle(codes::Pycodestyle::W),
                    "Warning",
                ),
            ]),
            Linter::Pylint => Some(&[
                UpstreamCategory(RuleCodePrefix::Pylint(codes::Pylint::C), "Convention"),
                UpstreamCategory(RuleCodePrefix::Pylint(codes::Pylint::E), "Error"),
                UpstreamCategory(RuleCodePrefix::Pylint(codes::Pylint::R), "Refactor"),
                UpstreamCategory(RuleCodePrefix::Pylint(codes::Pylint::W), "Warning"),
            ]),
            _ => None,
        }
    }
}

#[derive(is_macro::Is, Copy, Clone)]
pub enum LintSource {
    Ast,
    Io,
    PhysicalLines,
    LogicalLines,
    Tokens,
    Imports,
    Noqa,
    Filesystem,
}

impl Rule {
    /// The source for the diagnostic (either the AST, the filesystem, or the
    /// physical lines).
    pub const fn lint_source(&self) -> LintSource {
        match self {
            Rule::UnusedNOQA => LintSource::Noqa,
            Rule::BlanketNOQA
            | Rule::BlanketTypeIgnore
            | Rule::DocLineTooLong
            | Rule::LineTooLong
            | Rule::MixedSpacesAndTabs
            | Rule::MissingNewlineAtEndOfFile
            | Rule::UTF8EncodingDeclaration
            | Rule::ShebangMissingExecutableFile
            | Rule::ShebangNotExecutable
            | Rule::ShebangNotFirstLine
            | Rule::BidirectionalUnicode
            | Rule::ShebangMissingPython
            | Rule::ShebangLeadingWhitespace
            | Rule::TrailingWhitespace
            | Rule::TabIndentation
            | Rule::BlankLineWithWhitespace => LintSource::PhysicalLines,
            Rule::AmbiguousUnicodeCharacterComment
            | Rule::AmbiguousUnicodeCharacterDocstring
            | Rule::AmbiguousUnicodeCharacterString
            | Rule::AvoidableEscapedQuote
            | Rule::BadQuotesDocstring
            | Rule::BadQuotesInlineString
            | Rule::BadQuotesMultilineString
            | Rule::CommentedOutCode
            | Rule::MultiLineImplicitStringConcatenation
            | Rule::InvalidCharacterBackspace
            | Rule::InvalidCharacterSub
            | Rule::InvalidCharacterEsc
            | Rule::InvalidCharacterNul
            | Rule::InvalidCharacterZeroWidthSpace
            | Rule::ExtraneousParentheses
            | Rule::InvalidEscapeSequence
            | Rule::SingleLineImplicitStringConcatenation
            | Rule::MissingTrailingComma
            | Rule::TrailingCommaOnBareTuple
            | Rule::MultipleStatementsOnOneLineColon
            | Rule::UselessSemicolon
            | Rule::MultipleStatementsOnOneLineSemicolon
            | Rule::ProhibitedTrailingComma
            | Rule::TypeCommentInStub => LintSource::Tokens,
            Rule::IOError => LintSource::Io,
            Rule::UnsortedImports | Rule::MissingRequiredImport => LintSource::Imports,
            Rule::ImplicitNamespacePackage | Rule::InvalidModuleName => LintSource::Filesystem,
            #[cfg(feature = "logical_lines")]
            Rule::IndentationWithInvalidMultiple
            | Rule::IndentationWithInvalidMultipleComment
            | Rule::MissingWhitespace
            | Rule::MissingWhitespaceAfterKeyword
            | Rule::MissingWhitespaceAroundArithmeticOperator
            | Rule::MissingWhitespaceAroundBitwiseOrShiftOperator
            | Rule::MissingWhitespaceAroundModuloOperator
            | Rule::MissingWhitespaceAroundOperator
            | Rule::MissingWhitespaceAroundParameterEquals
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
            | Rule::UnexpectedSpacesAroundKeywordParameterEquals
            | Rule::WhitespaceAfterOpenBracket
            | Rule::WhitespaceBeforeCloseBracket
            | Rule::WhitespaceBeforeParameters
            | Rule::WhitespaceBeforePunctuation => LintSource::LogicalLines,
            _ => LintSource::Ast,
        }
    }
}

/// Pairs of checks that shouldn't be enabled together.
pub const INCOMPATIBLE_CODES: &[(Rule, Rule, &str); 2] = &[
    (
        Rule::BlankLineBeforeClass,
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
    use std::mem::size_of;
    use strum::IntoEnumIterator;

    use super::{Linter, Rule, RuleNamespace};

    #[test]
    fn test_rule_naming_convention() {
        // The disallowed rule names are defined in a separate file so that they can also be picked up by add_rule.py.
        let patterns: Vec<_> = include_str!("../resources/test/disallowed_rule_names.txt")
            .trim()
            .split('\n')
            .map(|line| {
                glob::Pattern::new(line).expect("malformed pattern in disallowed_rule_names.txt")
            })
            .collect();

        for rule in Rule::iter() {
            let rule_name = rule.as_ref();
            for pattern in &patterns {
                assert!(
                    !pattern.matches(rule_name),
                    "{rule_name} does not match naming convention, see CONTRIBUTING.md"
                );
            }
        }
    }

    #[test]
    fn check_code_serialization() {
        for rule in Rule::iter() {
            assert!(
                Rule::from_code(&format!("{}", rule.noqa_code())).is_ok(),
                "{rule:?} could not be round-trip serialized."
            );
        }
    }

    #[test]
    fn test_linter_parse_code() {
        for rule in Rule::iter() {
            let code = format!("{}", rule.noqa_code());
            let (linter, rest) =
                Linter::parse_code(&code).unwrap_or_else(|| panic!("couldn't parse {code:?}"));
            assert_eq!(code, format!("{}{rest}", linter.common_prefix()));
        }
    }

    #[test]
    fn rule_size() {
        assert_eq!(2, size_of::<Rule>());
    }
}
