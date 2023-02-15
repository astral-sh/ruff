use crate::registry::{Linter, Rule};

#[ruff_macros::map_codes]
pub fn code_to_rule(linter: Linter, code: &str) -> Option<Rule> {
    #[allow(clippy::enum_glob_use)]
    use Linter::*;

    Some(match (linter, code) {
        // pycodestyle errors
        (Pycodestyle, "E101") => Rule::MixedSpacesAndTabs,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E111") => Rule::IndentationWithInvalidMultiple,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E112") => Rule::NoIndentedBlock,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E113") => Rule::UnexpectedIndentation,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E114") => Rule::IndentationWithInvalidMultipleComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E115") => Rule::NoIndentedBlockComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E116") => Rule::UnexpectedIndentationComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E117") => Rule::OverIndented,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E201") => Rule::WhitespaceAfterOpenBracket,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E202") => Rule::WhitespaceBeforeCloseBracket,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E203") => Rule::WhitespaceBeforePunctuation,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E221") => Rule::MultipleSpacesBeforeOperator,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E222") => Rule::MultipleSpacesAfterOperator,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E223") => Rule::TabBeforeOperator,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E224") => Rule::TabAfterOperator,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E261") => Rule::TooFewSpacesBeforeInlineComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E262") => Rule::NoSpaceAfterInlineComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E265") => Rule::NoSpaceAfterBlockComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E266") => Rule::MultipleLeadingHashesForBlockComment,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E271") => Rule::MultipleSpacesAfterKeyword,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E272") => Rule::MultipleSpacesBeforeKeyword,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E273") => Rule::TabAfterKeyword,
        #[cfg(feature = "logical_lines")]
        (Pycodestyle, "E274") => Rule::TabBeforeKeyword,
        (Pycodestyle, "E401") => Rule::MultipleImportsOnOneLine,
        (Pycodestyle, "E402") => Rule::ModuleImportNotAtTopOfFile,
        (Pycodestyle, "E501") => Rule::LineTooLong,
        (Pycodestyle, "E701") => Rule::MultipleStatementsOnOneLineColon,
        (Pycodestyle, "E702") => Rule::MultipleStatementsOnOneLineSemicolon,
        (Pycodestyle, "E703") => Rule::UselessSemicolon,
        (Pycodestyle, "E711") => Rule::NoneComparison,
        (Pycodestyle, "E712") => Rule::TrueFalseComparison,
        (Pycodestyle, "E713") => Rule::NotInTest,
        (Pycodestyle, "E714") => Rule::NotIsTest,
        (Pycodestyle, "E721") => Rule::TypeComparison,
        (Pycodestyle, "E722") => Rule::BareExcept,
        (Pycodestyle, "E731") => Rule::LambdaAssignment,
        (Pycodestyle, "E741") => Rule::AmbiguousVariableName,
        (Pycodestyle, "E742") => Rule::AmbiguousClassName,
        (Pycodestyle, "E743") => Rule::AmbiguousFunctionName,
        (Pycodestyle, "E902") => Rule::IOError,
        (Pycodestyle, "E999") => Rule::SyntaxError,

        // pycodestyle warnings
        (Pycodestyle, "W292") => Rule::NoNewLineAtEndOfFile,
        (Pycodestyle, "W505") => Rule::DocLineTooLong,
        (Pycodestyle, "W605") => Rule::InvalidEscapeSequence,

        // pyflakes
        (Pyflakes, "401") => Rule::UnusedImport,
        (Pyflakes, "402") => Rule::ImportShadowedByLoopVar,
        (Pyflakes, "403") => Rule::ImportStar,
        (Pyflakes, "404") => Rule::LateFutureImport,
        (Pyflakes, "405") => Rule::ImportStarUsage,
        (Pyflakes, "406") => Rule::ImportStarNotPermitted,
        (Pyflakes, "407") => Rule::FutureFeatureNotDefined,
        (Pyflakes, "501") => Rule::PercentFormatInvalidFormat,
        (Pyflakes, "502") => Rule::PercentFormatExpectedMapping,
        (Pyflakes, "503") => Rule::PercentFormatExpectedSequence,
        (Pyflakes, "504") => Rule::PercentFormatExtraNamedArguments,
        (Pyflakes, "505") => Rule::PercentFormatMissingArgument,
        (Pyflakes, "506") => Rule::PercentFormatMixedPositionalAndNamed,
        (Pyflakes, "507") => Rule::PercentFormatPositionalCountMismatch,
        (Pyflakes, "508") => Rule::PercentFormatStarRequiresSequence,
        (Pyflakes, "509") => Rule::PercentFormatUnsupportedFormatCharacter,
        (Pyflakes, "521") => Rule::StringDotFormatInvalidFormat,
        (Pyflakes, "522") => Rule::StringDotFormatExtraNamedArguments,
        (Pyflakes, "523") => Rule::StringDotFormatExtraPositionalArguments,
        (Pyflakes, "524") => Rule::StringDotFormatMissingArguments,
        (Pyflakes, "525") => Rule::StringDotFormatMixingAutomatic,
        (Pyflakes, "541") => Rule::FStringMissingPlaceholders,
        (Pyflakes, "601") => Rule::MultiValueRepeatedKeyLiteral,
        (Pyflakes, "602") => Rule::MultiValueRepeatedKeyVariable,
        (Pyflakes, "621") => Rule::ExpressionsInStarAssignment,
        (Pyflakes, "622") => Rule::TwoStarredExpressions,
        (Pyflakes, "631") => Rule::AssertTuple,
        (Pyflakes, "632") => Rule::IsLiteral,
        (Pyflakes, "633") => Rule::InvalidPrintSyntax,
        (Pyflakes, "634") => Rule::IfTuple,
        (Pyflakes, "701") => Rule::BreakOutsideLoop,
        (Pyflakes, "702") => Rule::ContinueOutsideLoop,
        (Pyflakes, "704") => Rule::YieldOutsideFunction,
        (Pyflakes, "706") => Rule::ReturnOutsideFunction,
        (Pyflakes, "707") => Rule::DefaultExceptNotLast,
        (Pyflakes, "722") => Rule::ForwardAnnotationSyntaxError,
        (Pyflakes, "811") => Rule::RedefinedWhileUnused,
        (Pyflakes, "821") => Rule::UndefinedName,
        (Pyflakes, "822") => Rule::UndefinedExport,
        (Pyflakes, "823") => Rule::UndefinedLocal,
        (Pyflakes, "841") => Rule::UnusedVariable,
        (Pyflakes, "842") => Rule::UnusedAnnotation,
        (Pyflakes, "901") => Rule::RaiseNotImplemented,

        // pylint
        (Pylint, "E0100") => Rule::YieldInInit,
        (Pylint, "E0604") => Rule::InvalidAllObject,
        (Pylint, "E0605") => Rule::InvalidAllFormat,
        (Pylint, "E1307") => Rule::BadStringFormatType,
        (Pylint, "E2502") => Rule::BidirectionalUnicode,
        (Pylint, "E1310") => Rule::BadStrStripCall,
        (Pylint, "C0414") => Rule::UselessImportAlias,
        (Pylint, "C3002") => Rule::UnnecessaryDirectLambdaCall,
        (Pylint, "E0117") => Rule::NonlocalWithoutBinding,
        (Pylint, "E0118") => Rule::UsedPriorGlobalDeclaration,
        (Pylint, "E1142") => Rule::AwaitOutsideAsync,
        (Pylint, "R0206") => Rule::PropertyWithParameters,
        (Pylint, "R0402") => Rule::ConsiderUsingFromImport,
        (Pylint, "R0133") => Rule::ComparisonOfConstant,
        (Pylint, "R1701") => Rule::ConsiderMergingIsinstance,
        (Pylint, "R1722") => Rule::ConsiderUsingSysExit,
        (Pylint, "R2004") => Rule::MagicValueComparison,
        (Pylint, "W0120") => Rule::UselessElseOnLoop,
        (Pylint, "W0602") => Rule::GlobalVariableNotAssigned,
        (Pylint, "R0911") => Rule::TooManyReturnStatements,
        (Pylint, "R0913") => Rule::TooManyArguments,
        (Pylint, "R0912") => Rule::TooManyBranches,
        (Pylint, "R0915") => Rule::TooManyStatements,

        // flake8-builtins
        (Flake8Builtins, "001") => Rule::BuiltinVariableShadowing,
        (Flake8Builtins, "002") => Rule::BuiltinArgumentShadowing,
        (Flake8Builtins, "003") => Rule::BuiltinAttributeShadowing,

        // flake8-bugbear
        (Flake8Bugbear, "002") => Rule::UnaryPrefixIncrement,
        (Flake8Bugbear, "003") => Rule::AssignmentToOsEnviron,
        (Flake8Bugbear, "004") => Rule::UnreliableCallableCheck,
        (Flake8Bugbear, "005") => Rule::StripWithMultiCharacters,
        (Flake8Bugbear, "006") => Rule::MutableArgumentDefault,
        (Flake8Bugbear, "007") => Rule::UnusedLoopControlVariable,
        (Flake8Bugbear, "008") => Rule::FunctionCallArgumentDefault,
        (Flake8Bugbear, "009") => Rule::GetAttrWithConstant,
        (Flake8Bugbear, "010") => Rule::SetAttrWithConstant,
        (Flake8Bugbear, "011") => Rule::AssertFalse,
        (Flake8Bugbear, "012") => Rule::JumpStatementInFinally,
        (Flake8Bugbear, "013") => Rule::RedundantTupleInExceptionHandler,
        (Flake8Bugbear, "014") => Rule::DuplicateHandlerException,
        (Flake8Bugbear, "015") => Rule::UselessComparison,
        (Flake8Bugbear, "016") => Rule::CannotRaiseLiteral,
        (Flake8Bugbear, "017") => Rule::AssertRaisesException,
        (Flake8Bugbear, "018") => Rule::UselessExpression,
        (Flake8Bugbear, "019") => Rule::CachedInstanceMethod,
        (Flake8Bugbear, "020") => Rule::LoopVariableOverridesIterator,
        (Flake8Bugbear, "021") => Rule::FStringDocstring,
        (Flake8Bugbear, "022") => Rule::UselessContextlibSuppress,
        (Flake8Bugbear, "023") => Rule::FunctionUsesLoopVariable,
        (Flake8Bugbear, "024") => Rule::AbstractBaseClassWithoutAbstractMethod,
        (Flake8Bugbear, "025") => Rule::DuplicateTryBlockException,
        (Flake8Bugbear, "026") => Rule::StarArgUnpackingAfterKeywordArg,
        (Flake8Bugbear, "027") => Rule::EmptyMethodWithoutAbstractDecorator,
        (Flake8Bugbear, "904") => Rule::RaiseWithoutFromInsideExcept,
        (Flake8Bugbear, "905") => Rule::ZipWithoutExplicitStrict,

        // flake8-blind-except
        (Flake8BlindExcept, "001") => Rule::BlindExcept,

        // flake8-comprehensions
        (Flake8Comprehensions, "00") => Rule::UnnecessaryGeneratorList,
        (Flake8Comprehensions, "01") => Rule::UnnecessaryGeneratorSet,
        (Flake8Comprehensions, "02") => Rule::UnnecessaryGeneratorDict,
        (Flake8Comprehensions, "03") => Rule::UnnecessaryListComprehensionSet,
        (Flake8Comprehensions, "04") => Rule::UnnecessaryListComprehensionDict,
        (Flake8Comprehensions, "05") => Rule::UnnecessaryLiteralSet,
        (Flake8Comprehensions, "06") => Rule::UnnecessaryLiteralDict,
        (Flake8Comprehensions, "08") => Rule::UnnecessaryCollectionCall,
        (Flake8Comprehensions, "09") => Rule::UnnecessaryLiteralWithinTupleCall,
        (Flake8Comprehensions, "10") => Rule::UnnecessaryLiteralWithinListCall,
        (Flake8Comprehensions, "11") => Rule::UnnecessaryListCall,
        (Flake8Comprehensions, "13") => Rule::UnnecessaryCallAroundSorted,
        (Flake8Comprehensions, "14") => Rule::UnnecessaryDoubleCastOrProcess,
        (Flake8Comprehensions, "15") => Rule::UnnecessarySubscriptReversal,
        (Flake8Comprehensions, "16") => Rule::UnnecessaryComprehension,
        (Flake8Comprehensions, "17") => Rule::UnnecessaryMap,

        // flake8-debugger
        (Flake8Debugger, "0") => Rule::Debugger,

        // mccabe
        (McCabe, "1") => Rule::ComplexStructure,

        // flake8-tidy-imports
        (Flake8TidyImports, "251") => Rule::BannedApi,
        (Flake8TidyImports, "252") => Rule::RelativeImports,

        // flake8-return
        (Flake8Return, "501") => Rule::UnnecessaryReturnNone,
        (Flake8Return, "502") => Rule::ImplicitReturnValue,
        (Flake8Return, "503") => Rule::ImplicitReturn,
        (Flake8Return, "504") => Rule::UnnecessaryAssign,
        (Flake8Return, "505") => Rule::SuperfluousElseReturn,
        (Flake8Return, "506") => Rule::SuperfluousElseRaise,
        (Flake8Return, "507") => Rule::SuperfluousElseContinue,
        (Flake8Return, "508") => Rule::SuperfluousElseBreak,

        // flake8-implicit-str-concat
        (Flake8ImplicitStrConcat, "001") => Rule::SingleLineImplicitStringConcatenation,
        (Flake8ImplicitStrConcat, "002") => Rule::MultiLineImplicitStringConcatenation,
        (Flake8ImplicitStrConcat, "003") => Rule::ExplicitStringConcatenation,

        // flake8-print
        (Flake8Print, "1") => Rule::PrintFound,
        (Flake8Print, "3") => Rule::PPrintFound,

        // flake8-quotes
        (Flake8Quotes, "000") => Rule::BadQuotesInlineString,
        (Flake8Quotes, "001") => Rule::BadQuotesMultilineString,
        (Flake8Quotes, "002") => Rule::BadQuotesDocstring,
        (Flake8Quotes, "003") => Rule::AvoidableEscapedQuote,

        // flake8-annotations
        (Flake8Annotations, "001") => Rule::MissingTypeFunctionArgument,
        (Flake8Annotations, "002") => Rule::MissingTypeArgs,
        (Flake8Annotations, "003") => Rule::MissingTypeKwargs,
        (Flake8Annotations, "101") => Rule::MissingTypeSelf,
        (Flake8Annotations, "102") => Rule::MissingTypeCls,
        (Flake8Annotations, "201") => Rule::MissingReturnTypePublicFunction,
        (Flake8Annotations, "202") => Rule::MissingReturnTypePrivateFunction,
        (Flake8Annotations, "204") => Rule::MissingReturnTypeSpecialMethod,
        (Flake8Annotations, "205") => Rule::MissingReturnTypeStaticMethod,
        (Flake8Annotations, "206") => Rule::MissingReturnTypeClassMethod,
        (Flake8Annotations, "401") => Rule::AnyType,

        // flake8-2020
        (Flake82020, "101") => Rule::SysVersionSlice3Referenced,
        (Flake82020, "102") => Rule::SysVersion2Referenced,
        (Flake82020, "103") => Rule::SysVersionCmpStr3,
        (Flake82020, "201") => Rule::SysVersionInfo0Eq3Referenced,
        (Flake82020, "202") => Rule::SixPY3Referenced,
        (Flake82020, "203") => Rule::SysVersionInfo1CmpInt,
        (Flake82020, "204") => Rule::SysVersionInfoMinorCmpInt,
        (Flake82020, "301") => Rule::SysVersion0Referenced,
        (Flake82020, "302") => Rule::SysVersionCmpStr10,
        (Flake82020, "303") => Rule::SysVersionSlice1Referenced,

        // flake8-simplify
        (Flake8Simplify, "101") => Rule::DuplicateIsinstanceCall,
        (Flake8Simplify, "102") => Rule::CollapsibleIf,
        (Flake8Simplify, "103") => Rule::NeedlessBool,
        (Flake8Simplify, "105") => Rule::UseContextlibSuppress,
        (Flake8Simplify, "107") => Rule::ReturnInTryExceptFinally,
        (Flake8Simplify, "108") => Rule::UseTernaryOperator,
        (Flake8Simplify, "109") => Rule::CompareWithTuple,
        (Flake8Simplify, "110") => Rule::ConvertLoopToAny,
        (Flake8Simplify, "111") => Rule::ConvertLoopToAll,
        (Flake8Simplify, "112") => Rule::UseCapitalEnvironmentVariables,
        (Flake8Simplify, "114") => Rule::IfWithSameArms,
        (Flake8Simplify, "115") => Rule::OpenFileWithContextHandler,
        (Flake8Simplify, "117") => Rule::MultipleWithStatements,
        (Flake8Simplify, "118") => Rule::KeyInDict,
        (Flake8Simplify, "201") => Rule::NegateEqualOp,
        (Flake8Simplify, "202") => Rule::NegateNotEqualOp,
        (Flake8Simplify, "208") => Rule::DoubleNegation,
        (Flake8Simplify, "210") => Rule::IfExprWithTrueFalse,
        (Flake8Simplify, "211") => Rule::IfExprWithFalseTrue,
        (Flake8Simplify, "212") => Rule::IfExprWithTwistedArms,
        (Flake8Simplify, "220") => Rule::AAndNotA,
        (Flake8Simplify, "221") => Rule::AOrNotA,
        (Flake8Simplify, "222") => Rule::OrTrue,
        (Flake8Simplify, "223") => Rule::AndFalse,
        (Flake8Simplify, "300") => Rule::YodaConditions,
        (Flake8Simplify, "401") => Rule::DictGetWithDefault,

        // pyupgrade
        (Pyupgrade, "001") => Rule::UselessMetaclassType,
        (Pyupgrade, "003") => Rule::TypeOfPrimitive,
        (Pyupgrade, "004") => Rule::UselessObjectInheritance,
        (Pyupgrade, "005") => Rule::DeprecatedUnittestAlias,
        (Pyupgrade, "006") => Rule::DeprecatedCollectionType,
        (Pyupgrade, "007") => Rule::TypingUnion,
        (Pyupgrade, "008") => Rule::SuperCallWithParameters,
        (Pyupgrade, "009") => Rule::UTF8EncodingDeclaration,
        (Pyupgrade, "010") => Rule::UnnecessaryFutureImport,
        (Pyupgrade, "011") => Rule::LRUCacheWithoutParameters,
        (Pyupgrade, "012") => Rule::UnnecessaryEncodeUTF8,
        (Pyupgrade, "013") => Rule::ConvertTypedDictFunctionalToClass,
        (Pyupgrade, "014") => Rule::ConvertNamedTupleFunctionalToClass,
        (Pyupgrade, "015") => Rule::RedundantOpenModes,
        (Pyupgrade, "017") => Rule::DatetimeTimezoneUTC,
        (Pyupgrade, "018") => Rule::NativeLiterals,
        (Pyupgrade, "019") => Rule::TypingTextStrAlias,
        (Pyupgrade, "020") => Rule::OpenAlias,
        (Pyupgrade, "021") => Rule::ReplaceUniversalNewlines,
        (Pyupgrade, "022") => Rule::ReplaceStdoutStderr,
        (Pyupgrade, "023") => Rule::RewriteCElementTree,
        (Pyupgrade, "024") => Rule::OSErrorAlias,
        (Pyupgrade, "025") => Rule::RewriteUnicodeLiteral,
        (Pyupgrade, "026") => Rule::RewriteMockImport,
        (Pyupgrade, "027") => Rule::RewriteListComprehension,
        (Pyupgrade, "028") => Rule::RewriteYieldFrom,
        (Pyupgrade, "029") => Rule::UnnecessaryBuiltinImport,
        (Pyupgrade, "030") => Rule::FormatLiterals,
        (Pyupgrade, "031") => Rule::PrintfStringFormatting,
        (Pyupgrade, "032") => Rule::FString,
        (Pyupgrade, "033") => Rule::FunctoolsCache,
        (Pyupgrade, "034") => Rule::ExtraneousParentheses,
        (Pyupgrade, "035") => Rule::ImportReplacements,
        (Pyupgrade, "036") => Rule::OutdatedVersionBlock,
        (Pyupgrade, "037") => Rule::QuotedAnnotation,

        // pydocstyle
        (Pydocstyle, "100") => Rule::PublicModule,
        (Pydocstyle, "101") => Rule::PublicClass,
        (Pydocstyle, "102") => Rule::PublicMethod,
        (Pydocstyle, "103") => Rule::PublicFunction,
        (Pydocstyle, "104") => Rule::PublicPackage,
        (Pydocstyle, "105") => Rule::MagicMethod,
        (Pydocstyle, "106") => Rule::PublicNestedClass,
        (Pydocstyle, "107") => Rule::PublicInit,
        (Pydocstyle, "200") => Rule::FitsOnOneLine,
        (Pydocstyle, "201") => Rule::NoBlankLineBeforeFunction,
        (Pydocstyle, "202") => Rule::NoBlankLineAfterFunction,
        (Pydocstyle, "203") => Rule::OneBlankLineBeforeClass,
        (Pydocstyle, "204") => Rule::OneBlankLineAfterClass,
        (Pydocstyle, "205") => Rule::BlankLineAfterSummary,
        (Pydocstyle, "206") => Rule::IndentWithSpaces,
        (Pydocstyle, "207") => Rule::NoUnderIndentation,
        (Pydocstyle, "208") => Rule::NoOverIndentation,
        (Pydocstyle, "209") => Rule::NewLineAfterLastParagraph,
        (Pydocstyle, "210") => Rule::NoSurroundingWhitespace,
        (Pydocstyle, "211") => Rule::NoBlankLineBeforeClass,
        (Pydocstyle, "212") => Rule::MultiLineSummaryFirstLine,
        (Pydocstyle, "213") => Rule::MultiLineSummarySecondLine,
        (Pydocstyle, "214") => Rule::SectionNotOverIndented,
        (Pydocstyle, "215") => Rule::SectionUnderlineNotOverIndented,
        (Pydocstyle, "300") => Rule::TripleSingleQuotes,
        (Pydocstyle, "301") => Rule::EscapeSequenceInDocstring,
        (Pydocstyle, "400") => Rule::EndsInPeriod,
        (Pydocstyle, "401") => Rule::NonImperativeMood,
        (Pydocstyle, "402") => Rule::NoSignature,
        (Pydocstyle, "403") => Rule::FirstLineCapitalized,
        (Pydocstyle, "404") => Rule::DocstringStartsWithThis,
        (Pydocstyle, "405") => Rule::CapitalizeSectionName,
        (Pydocstyle, "406") => Rule::NewLineAfterSectionName,
        (Pydocstyle, "407") => Rule::DashedUnderlineAfterSection,
        (Pydocstyle, "408") => Rule::SectionUnderlineAfterName,
        (Pydocstyle, "409") => Rule::SectionUnderlineMatchesSectionLength,
        (Pydocstyle, "410") => Rule::BlankLineAfterSection,
        (Pydocstyle, "411") => Rule::BlankLineBeforeSection,
        (Pydocstyle, "412") => Rule::NoBlankLinesBetweenHeaderAndContent,
        (Pydocstyle, "413") => Rule::BlankLineAfterLastSection,
        (Pydocstyle, "414") => Rule::EmptyDocstringSection,
        (Pydocstyle, "415") => Rule::EndsInPunctuation,
        (Pydocstyle, "416") => Rule::SectionNameEndsInColon,
        (Pydocstyle, "417") => Rule::UndocumentedParam,
        (Pydocstyle, "418") => Rule::OverloadWithDocstring,
        (Pydocstyle, "419") => Rule::EmptyDocstring,

        // pep8-naming
        (PEP8Naming, "801") => Rule::InvalidClassName,
        (PEP8Naming, "802") => Rule::InvalidFunctionName,
        (PEP8Naming, "803") => Rule::InvalidArgumentName,
        (PEP8Naming, "804") => Rule::InvalidFirstArgumentNameForClassMethod,
        (PEP8Naming, "805") => Rule::InvalidFirstArgumentNameForMethod,
        (PEP8Naming, "806") => Rule::NonLowercaseVariableInFunction,
        (PEP8Naming, "807") => Rule::DunderFunctionName,
        (PEP8Naming, "811") => Rule::ConstantImportedAsNonConstant,
        (PEP8Naming, "812") => Rule::LowercaseImportedAsNonLowercase,
        (PEP8Naming, "813") => Rule::CamelcaseImportedAsLowercase,
        (PEP8Naming, "814") => Rule::CamelcaseImportedAsConstant,
        (PEP8Naming, "815") => Rule::MixedCaseVariableInClassScope,
        (PEP8Naming, "816") => Rule::MixedCaseVariableInGlobalScope,
        (PEP8Naming, "817") => Rule::CamelcaseImportedAsAcronym,
        (PEP8Naming, "818") => Rule::ErrorSuffixOnExceptionName,

        // isort
        (Isort, "001") => Rule::UnsortedImports,
        (Isort, "002") => Rule::MissingRequiredImport,

        // eradicate
        (Eradicate, "001") => Rule::CommentedOutCode,

        // flake8-bandit
        (Flake8Bandit, "101") => Rule::Assert,
        (Flake8Bandit, "102") => Rule::ExecBuiltin,
        (Flake8Bandit, "103") => Rule::BadFilePermissions,
        (Flake8Bandit, "104") => Rule::HardcodedBindAllInterfaces,
        (Flake8Bandit, "105") => Rule::HardcodedPasswordString,
        (Flake8Bandit, "106") => Rule::HardcodedPasswordFuncArg,
        (Flake8Bandit, "107") => Rule::HardcodedPasswordDefault,
        (Flake8Bandit, "608") => Rule::HardcodedSQLExpression,
        (Flake8Bandit, "108") => Rule::HardcodedTempFile,
        (Flake8Bandit, "110") => Rule::TryExceptPass,
        (Flake8Bandit, "112") => Rule::TryExceptContinue,
        (Flake8Bandit, "113") => Rule::RequestWithoutTimeout,
        (Flake8Bandit, "324") => Rule::HashlibInsecureHashFunction,
        (Flake8Bandit, "501") => Rule::RequestWithNoCertValidation,
        (Flake8Bandit, "506") => Rule::UnsafeYAMLLoad,
        (Flake8Bandit, "508") => Rule::SnmpInsecureVersion,
        (Flake8Bandit, "509") => Rule::SnmpWeakCryptography,
        (Flake8Bandit, "612") => Rule::LoggingConfigInsecureListen,
        (Flake8Bandit, "701") => Rule::Jinja2AutoescapeFalse,

        // flake8-boolean-trap
        (Flake8BooleanTrap, "001") => Rule::BooleanPositionalArgInFunctionDefinition,
        (Flake8BooleanTrap, "002") => Rule::BooleanDefaultValueInFunctionDefinition,
        (Flake8BooleanTrap, "003") => Rule::BooleanPositionalValueInFunctionCall,

        // flake8-unused-arguments
        (Flake8UnusedArguments, "001") => Rule::UnusedFunctionArgument,
        (Flake8UnusedArguments, "002") => Rule::UnusedMethodArgument,
        (Flake8UnusedArguments, "003") => Rule::UnusedClassMethodArgument,
        (Flake8UnusedArguments, "004") => Rule::UnusedStaticMethodArgument,
        (Flake8UnusedArguments, "005") => Rule::UnusedLambdaArgument,

        // flake8-import-conventions
        (Flake8ImportConventions, "001") => Rule::UnconventionalImportAlias,

        // flake8-datetimez
        (Flake8Datetimez, "001") => Rule::CallDatetimeWithoutTzinfo,
        (Flake8Datetimez, "002") => Rule::CallDatetimeToday,
        (Flake8Datetimez, "003") => Rule::CallDatetimeUtcnow,
        (Flake8Datetimez, "004") => Rule::CallDatetimeUtcfromtimestamp,
        (Flake8Datetimez, "005") => Rule::CallDatetimeNowWithoutTzinfo,
        (Flake8Datetimez, "006") => Rule::CallDatetimeFromtimestamp,
        (Flake8Datetimez, "007") => Rule::CallDatetimeStrptimeWithoutZone,
        (Flake8Datetimez, "011") => Rule::CallDateToday,
        (Flake8Datetimez, "012") => Rule::CallDateFromtimestamp,

        // pygrep-hooks
        (PygrepHooks, "001") => Rule::NoEval,
        (PygrepHooks, "002") => Rule::DeprecatedLogWarn,
        (PygrepHooks, "003") => Rule::BlanketTypeIgnore,
        (PygrepHooks, "004") => Rule::BlanketNOQA,

        // pandas-vet
        (PandasVet, "002") => Rule::UseOfInplaceArgument,
        (PandasVet, "003") => Rule::UseOfDotIsNull,
        (PandasVet, "004") => Rule::UseOfDotNotNull,
        (PandasVet, "007") => Rule::UseOfDotIx,
        (PandasVet, "008") => Rule::UseOfDotAt,
        (PandasVet, "009") => Rule::UseOfDotIat,
        (PandasVet, "010") => Rule::UseOfDotPivotOrUnstack,
        (PandasVet, "011") => Rule::UseOfDotValues,
        (PandasVet, "012") => Rule::UseOfDotReadTable,
        (PandasVet, "013") => Rule::UseOfDotStack,
        (PandasVet, "015") => Rule::UseOfPdMerge,
        (PandasVet, "901") => Rule::DfIsABadVariableName,

        // flake8-errmsg
        (Flake8ErrMsg, "101") => Rule::RawStringInException,
        (Flake8ErrMsg, "102") => Rule::FStringInException,
        (Flake8ErrMsg, "103") => Rule::DotFormatInException,

        // flake8-pyi
        (Flake8Pyi, "001") => Rule::PrefixTypeParams,
        (Flake8Pyi, "007") => Rule::UnrecognizedPlatformCheck,
        (Flake8Pyi, "008") => Rule::UnrecognizedPlatformName,

        // flake8-pytest-style
        (Flake8PytestStyle, "001") => Rule::IncorrectFixtureParenthesesStyle,
        (Flake8PytestStyle, "002") => Rule::FixturePositionalArgs,
        (Flake8PytestStyle, "003") => Rule::ExtraneousScopeFunction,
        (Flake8PytestStyle, "004") => Rule::MissingFixtureNameUnderscore,
        (Flake8PytestStyle, "005") => Rule::IncorrectFixtureNameUnderscore,
        (Flake8PytestStyle, "006") => Rule::ParametrizeNamesWrongType,
        (Flake8PytestStyle, "007") => Rule::ParametrizeValuesWrongType,
        (Flake8PytestStyle, "008") => Rule::PatchWithLambda,
        (Flake8PytestStyle, "009") => Rule::UnittestAssertion,
        (Flake8PytestStyle, "010") => Rule::RaisesWithoutException,
        (Flake8PytestStyle, "011") => Rule::RaisesTooBroad,
        (Flake8PytestStyle, "012") => Rule::RaisesWithMultipleStatements,
        (Flake8PytestStyle, "013") => Rule::IncorrectPytestImport,
        (Flake8PytestStyle, "015") => Rule::AssertAlwaysFalse,
        (Flake8PytestStyle, "016") => Rule::FailWithoutMessage,
        (Flake8PytestStyle, "017") => Rule::AssertInExcept,
        (Flake8PytestStyle, "018") => Rule::CompositeAssertion,
        (Flake8PytestStyle, "019") => Rule::FixtureParamWithoutValue,
        (Flake8PytestStyle, "020") => Rule::DeprecatedYieldFixture,
        (Flake8PytestStyle, "021") => Rule::FixtureFinalizerCallback,
        (Flake8PytestStyle, "022") => Rule::UselessYieldFixture,
        (Flake8PytestStyle, "023") => Rule::IncorrectMarkParenthesesStyle,
        (Flake8PytestStyle, "024") => Rule::UnnecessaryAsyncioMarkOnFixture,
        (Flake8PytestStyle, "025") => Rule::ErroneousUseFixturesOnFixture,
        (Flake8PytestStyle, "026") => Rule::UseFixturesWithoutParameters,

        // flake8-pie
        (Flake8Pie, "790") => Rule::UnnecessaryPass,
        (Flake8Pie, "794") => Rule::DupeClassFieldDefinitions,
        (Flake8Pie, "796") => Rule::PreferUniqueEnums,
        (Flake8Pie, "800") => Rule::UnnecessarySpread,
        (Flake8Pie, "804") => Rule::UnnecessaryDictKwargs,
        (Flake8Pie, "807") => Rule::PreferListBuiltin,
        (Flake8Pie, "810") => Rule::SingleStartsEndsWith,

        // flake8-commas
        (Flake8Commas, "812") => Rule::TrailingCommaMissing,
        (Flake8Commas, "818") => Rule::TrailingCommaOnBareTupleProhibited,
        (Flake8Commas, "819") => Rule::TrailingCommaProhibited,

        // flake8-no-pep420
        (Flake8NoPep420, "001") => Rule::ImplicitNamespacePackage,

        // flake8-executable
        (Flake8Executable, "001") => Rule::ShebangNotExecutable,
        (Flake8Executable, "002") => Rule::ShebangMissingExecutableFile,
        (Flake8Executable, "003") => Rule::ShebangPython,
        (Flake8Executable, "004") => Rule::ShebangWhitespace,
        (Flake8Executable, "005") => Rule::ShebangNewline,

        // flake8-type-checking
        (Flake8TypeChecking, "001") => Rule::TypingOnlyFirstPartyImport,
        (Flake8TypeChecking, "002") => Rule::TypingOnlyThirdPartyImport,
        (Flake8TypeChecking, "003") => Rule::TypingOnlyStandardLibraryImport,
        (Flake8TypeChecking, "004") => Rule::RuntimeImportInTypeCheckingBlock,
        (Flake8TypeChecking, "005") => Rule::EmptyTypeCheckingBlock,

        // tryceratops
        (Tryceratops, "002") => Rule::RaiseVanillaClass,
        (Tryceratops, "003") => Rule::RaiseVanillaArgs,
        (Tryceratops, "004") => Rule::PreferTypeError,
        (Tryceratops, "200") => Rule::ReraiseNoCause,
        (Tryceratops, "201") => Rule::VerboseRaise,
        (Tryceratops, "300") => Rule::TryConsiderElse,
        (Tryceratops, "301") => Rule::RaiseWithinTry,
        (Tryceratops, "400") => Rule::ErrorInsteadOfException,

        // flake8-use-pathlib
        (Flake8UsePathlib, "100") => Rule::PathlibAbspath,
        (Flake8UsePathlib, "101") => Rule::PathlibChmod,
        (Flake8UsePathlib, "102") => Rule::PathlibMkdir,
        (Flake8UsePathlib, "103") => Rule::PathlibMakedirs,
        (Flake8UsePathlib, "104") => Rule::PathlibRename,
        (Flake8UsePathlib, "105") => Rule::PathlibReplace,
        (Flake8UsePathlib, "106") => Rule::PathlibRmdir,
        (Flake8UsePathlib, "107") => Rule::PathlibRemove,
        (Flake8UsePathlib, "108") => Rule::PathlibUnlink,
        (Flake8UsePathlib, "109") => Rule::PathlibGetcwd,
        (Flake8UsePathlib, "110") => Rule::PathlibExists,
        (Flake8UsePathlib, "111") => Rule::PathlibExpanduser,
        (Flake8UsePathlib, "112") => Rule::PathlibIsDir,
        (Flake8UsePathlib, "113") => Rule::PathlibIsFile,
        (Flake8UsePathlib, "114") => Rule::PathlibIsLink,
        (Flake8UsePathlib, "115") => Rule::PathlibReadlink,
        (Flake8UsePathlib, "116") => Rule::PathlibStat,
        (Flake8UsePathlib, "117") => Rule::PathlibIsAbs,
        (Flake8UsePathlib, "118") => Rule::PathlibJoin,
        (Flake8UsePathlib, "119") => Rule::PathlibBasename,
        (Flake8UsePathlib, "120") => Rule::PathlibDirname,
        (Flake8UsePathlib, "121") => Rule::PathlibSamefile,
        (Flake8UsePathlib, "122") => Rule::PathlibSplitext,
        (Flake8UsePathlib, "123") => Rule::PathlibOpen,
        (Flake8UsePathlib, "124") => Rule::PathlibPyPath,

        // flake8-logging-format
        (Flake8LoggingFormat, "001") => Rule::LoggingStringFormat,
        (Flake8LoggingFormat, "002") => Rule::LoggingPercentFormat,
        (Flake8LoggingFormat, "003") => Rule::LoggingStringConcat,
        (Flake8LoggingFormat, "004") => Rule::LoggingFString,
        (Flake8LoggingFormat, "010") => Rule::LoggingWarn,
        (Flake8LoggingFormat, "101") => Rule::LoggingExtraAttrClash,
        (Flake8LoggingFormat, "201") => Rule::LoggingExcInfo,
        (Flake8LoggingFormat, "202") => Rule::LoggingRedundantExcInfo,

        // flake8-raise
        (Flake8Raise, "102") => Rule::UnnecessaryParenOnRaiseException,

        // flake8-self
        (Flake8Self, "001") => Rule::PrivateMemberAccess,

        // numpy
        (Numpy, "001") => Rule::NumpyDeprecatedTypeAlias,

        // ruff
        (Ruff, "001") => Rule::AmbiguousUnicodeCharacterString,
        (Ruff, "002") => Rule::AmbiguousUnicodeCharacterDocstring,
        (Ruff, "003") => Rule::AmbiguousUnicodeCharacterComment,
        (Ruff, "004") => Rule::KeywordArgumentBeforeStarArgument,
        (Ruff, "005") => Rule::UnpackInsteadOfConcatenatingToCollectionLiteral,
        (Ruff, "006") => Rule::AsyncioDanglingTask,
        (Ruff, "100") => Rule::UnusedNOQA,

        // flake8-django
        (Flake8Django, "001") => Rule::NullableModelStringField,
        (Flake8Django, "008") => Rule::ModelWithoutDunderStr,
        (Flake8Django, "013") => Rule::NonLeadingReceiverDecorator,

        _ => return None,
    })
}
