/// In this module we generate [`Rule`], an enum of all rules, and [`RuleCodePrefix`], an enum of
/// all rules categories. A rule category is something like pyflakes or flake8-todos. Each rule
/// category contains all rules and their common prefixes, i.e. everything you can specify in
/// `--select`. For pylint this is e.g. C0414 and E0118 but also C and E01.
use std::fmt::Formatter;

use ruff_db::diagnostic::SecondaryCode;
use serde::Serialize;
use strum_macros::EnumIter;

use crate::registry::Linter;
use crate::rule_selector::is_single_rule_selector;
use crate::rules;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoqaCode(&'static str, &'static str);

impl NoqaCode {
    /// Return the prefix for the [`NoqaCode`], e.g., `SIM` for `SIM101`.
    pub fn prefix(&self) -> &str {
        self.0
    }

    /// Return the suffix for the [`NoqaCode`], e.g., `101` for `SIM101`.
    pub fn suffix(&self) -> &str {
        self.1
    }
}

impl std::fmt::Debug for NoqaCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for NoqaCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}{}", self.0, self.1)
    }
}

impl PartialEq<&str> for NoqaCode {
    fn eq(&self, other: &&str) -> bool {
        match other.strip_prefix(self.0) {
            Some(suffix) => suffix == self.1,
            None => false,
        }
    }
}

impl PartialEq<NoqaCode> for &str {
    fn eq(&self, other: &NoqaCode) -> bool {
        other.eq(self)
    }
}

impl PartialEq<NoqaCode> for SecondaryCode {
    fn eq(&self, other: &NoqaCode) -> bool {
        &self.as_str() == other
    }
}

impl PartialEq<SecondaryCode> for NoqaCode {
    fn eq(&self, other: &SecondaryCode) -> bool {
        other.eq(self)
    }
}

impl serde::Serialize for NoqaCode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Copy, Clone, Serialize)]
pub enum RuleGroup {
    /// The rule is stable since the provided Ruff version.
    Stable { since: &'static str },
    /// The rule has been unstable since the provided Ruff version, and preview mode must be enabled
    /// for usage.
    Preview { since: &'static str },
    /// The rule has been deprecated since the provided Ruff version, warnings will be displayed
    /// during selection in stable and errors will be raised if used with preview mode enabled.
    Deprecated { since: &'static str },
    /// The rule was removed in the provided Ruff version, and errors will be displayed on use.
    Removed { since: &'static str },
}

#[ruff_macros::map_codes]
pub fn code_to_rule(linter: Linter, code: &str) -> Option<(RuleGroup, Rule)> {
    #[expect(clippy::enum_glob_use)]
    use Linter::*;

    #[rustfmt::skip]
    Some(match (linter, code) {
        // pycodestyle errors
        (Pycodestyle, "E101") => rules::pycodestyle::rules::MixedSpacesAndTabs,
        (Pycodestyle, "E111") => rules::pycodestyle::rules::logical_lines::IndentationWithInvalidMultiple,
        (Pycodestyle, "E112") => rules::pycodestyle::rules::logical_lines::NoIndentedBlock,
        (Pycodestyle, "E113") => rules::pycodestyle::rules::logical_lines::UnexpectedIndentation,
        (Pycodestyle, "E114") => rules::pycodestyle::rules::logical_lines::IndentationWithInvalidMultipleComment,
        (Pycodestyle, "E115") => rules::pycodestyle::rules::logical_lines::NoIndentedBlockComment,
        (Pycodestyle, "E116") => rules::pycodestyle::rules::logical_lines::UnexpectedIndentationComment,
        (Pycodestyle, "E117") => rules::pycodestyle::rules::logical_lines::OverIndented,
        (Pycodestyle, "E201") => rules::pycodestyle::rules::logical_lines::WhitespaceAfterOpenBracket,
        (Pycodestyle, "E202") => rules::pycodestyle::rules::logical_lines::WhitespaceBeforeCloseBracket,
        (Pycodestyle, "E203") => rules::pycodestyle::rules::logical_lines::WhitespaceBeforePunctuation,
        (Pycodestyle, "E204") => rules::pycodestyle::rules::WhitespaceAfterDecorator,
        (Pycodestyle, "E211") => rules::pycodestyle::rules::logical_lines::WhitespaceBeforeParameters,
        (Pycodestyle, "E221") => rules::pycodestyle::rules::logical_lines::MultipleSpacesBeforeOperator,
        (Pycodestyle, "E222") => rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterOperator,
        (Pycodestyle, "E223") => rules::pycodestyle::rules::logical_lines::TabBeforeOperator,
        (Pycodestyle, "E224") => rules::pycodestyle::rules::logical_lines::TabAfterOperator,
        (Pycodestyle, "E225") => rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundOperator,
        (Pycodestyle, "E226") => rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundArithmeticOperator,
        (Pycodestyle, "E227") => rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundBitwiseOrShiftOperator,
        (Pycodestyle, "E228") => rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundModuloOperator,
        (Pycodestyle, "E231") => rules::pycodestyle::rules::logical_lines::MissingWhitespace,
        (Pycodestyle, "E241") => rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterComma,
        (Pycodestyle, "E242") => rules::pycodestyle::rules::logical_lines::TabAfterComma,
        (Pycodestyle, "E251") => rules::pycodestyle::rules::logical_lines::UnexpectedSpacesAroundKeywordParameterEquals,
        (Pycodestyle, "E252") => rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundParameterEquals,
        (Pycodestyle, "E261") => rules::pycodestyle::rules::logical_lines::TooFewSpacesBeforeInlineComment,
        (Pycodestyle, "E262") => rules::pycodestyle::rules::logical_lines::NoSpaceAfterInlineComment,
        (Pycodestyle, "E265") => rules::pycodestyle::rules::logical_lines::NoSpaceAfterBlockComment,
        (Pycodestyle, "E266") => rules::pycodestyle::rules::logical_lines::MultipleLeadingHashesForBlockComment,
        (Pycodestyle, "E271") => rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterKeyword,
        (Pycodestyle, "E272") => rules::pycodestyle::rules::logical_lines::MultipleSpacesBeforeKeyword,
        (Pycodestyle, "E273") => rules::pycodestyle::rules::logical_lines::TabAfterKeyword,
        (Pycodestyle, "E274") => rules::pycodestyle::rules::logical_lines::TabBeforeKeyword,
        (Pycodestyle, "E275") => rules::pycodestyle::rules::logical_lines::MissingWhitespaceAfterKeyword,
        (Pycodestyle, "E301") => rules::pycodestyle::rules::BlankLineBetweenMethods,
        (Pycodestyle, "E302") => rules::pycodestyle::rules::BlankLinesTopLevel,
        (Pycodestyle, "E303") => rules::pycodestyle::rules::TooManyBlankLines,
        (Pycodestyle, "E304") => rules::pycodestyle::rules::BlankLineAfterDecorator,
        (Pycodestyle, "E305") => rules::pycodestyle::rules::BlankLinesAfterFunctionOrClass,
        (Pycodestyle, "E306") => rules::pycodestyle::rules::BlankLinesBeforeNestedDefinition,
        (Pycodestyle, "E401") => rules::pycodestyle::rules::MultipleImportsOnOneLine,
        (Pycodestyle, "E402") => rules::pycodestyle::rules::ModuleImportNotAtTopOfFile,
        (Pycodestyle, "E501") => rules::pycodestyle::rules::LineTooLong,
        (Pycodestyle, "E502") => rules::pycodestyle::rules::logical_lines::RedundantBackslash,
        (Pycodestyle, "E701") => rules::pycodestyle::rules::MultipleStatementsOnOneLineColon,
        (Pycodestyle, "E702") => rules::pycodestyle::rules::MultipleStatementsOnOneLineSemicolon,
        (Pycodestyle, "E703") => rules::pycodestyle::rules::UselessSemicolon,
        (Pycodestyle, "E711") => rules::pycodestyle::rules::NoneComparison,
        (Pycodestyle, "E712") => rules::pycodestyle::rules::TrueFalseComparison,
        (Pycodestyle, "E713") => rules::pycodestyle::rules::NotInTest,
        (Pycodestyle, "E714") => rules::pycodestyle::rules::NotIsTest,
        (Pycodestyle, "E721") => rules::pycodestyle::rules::TypeComparison,
        (Pycodestyle, "E722") => rules::pycodestyle::rules::BareExcept,
        (Pycodestyle, "E731") => rules::pycodestyle::rules::LambdaAssignment,
        (Pycodestyle, "E741") => rules::pycodestyle::rules::AmbiguousVariableName,
        (Pycodestyle, "E742") => rules::pycodestyle::rules::AmbiguousClassName,
        (Pycodestyle, "E743") => rules::pycodestyle::rules::AmbiguousFunctionName,
        (Pycodestyle, "E902") => rules::pycodestyle::rules::IOError,
        #[allow(deprecated)]
        (Pycodestyle, "E999") => rules::pycodestyle::rules::SyntaxError,

        // pycodestyle warnings
        (Pycodestyle, "W191") => rules::pycodestyle::rules::TabIndentation,
        (Pycodestyle, "W291") => rules::pycodestyle::rules::TrailingWhitespace,
        (Pycodestyle, "W292") => rules::pycodestyle::rules::MissingNewlineAtEndOfFile,
        (Pycodestyle, "W293") => rules::pycodestyle::rules::BlankLineWithWhitespace,
        (Pycodestyle, "W391") => rules::pycodestyle::rules::TooManyNewlinesAtEndOfFile,
        (Pycodestyle, "W505") => rules::pycodestyle::rules::DocLineTooLong,
        (Pycodestyle, "W605") => rules::pycodestyle::rules::InvalidEscapeSequence,

        // pyflakes
        (Pyflakes, "401") => rules::pyflakes::rules::UnusedImport,
        (Pyflakes, "402") => rules::pyflakes::rules::ImportShadowedByLoopVar,
        (Pyflakes, "403") => rules::pyflakes::rules::UndefinedLocalWithImportStar,
        (Pyflakes, "404") => rules::pyflakes::rules::LateFutureImport,
        (Pyflakes, "405") => rules::pyflakes::rules::UndefinedLocalWithImportStarUsage,
        (Pyflakes, "406") => rules::pyflakes::rules::UndefinedLocalWithNestedImportStarUsage,
        (Pyflakes, "407") => rules::pyflakes::rules::FutureFeatureNotDefined,
        (Pyflakes, "501") => rules::pyflakes::rules::PercentFormatInvalidFormat,
        (Pyflakes, "502") => rules::pyflakes::rules::PercentFormatExpectedMapping,
        (Pyflakes, "503") => rules::pyflakes::rules::PercentFormatExpectedSequence,
        (Pyflakes, "504") => rules::pyflakes::rules::PercentFormatExtraNamedArguments,
        (Pyflakes, "505") => rules::pyflakes::rules::PercentFormatMissingArgument,
        (Pyflakes, "506") => rules::pyflakes::rules::PercentFormatMixedPositionalAndNamed,
        (Pyflakes, "507") => rules::pyflakes::rules::PercentFormatPositionalCountMismatch,
        (Pyflakes, "508") => rules::pyflakes::rules::PercentFormatStarRequiresSequence,
        (Pyflakes, "509") => rules::pyflakes::rules::PercentFormatUnsupportedFormatCharacter,
        (Pyflakes, "521") => rules::pyflakes::rules::StringDotFormatInvalidFormat,
        (Pyflakes, "522") => rules::pyflakes::rules::StringDotFormatExtraNamedArguments,
        (Pyflakes, "523") => rules::pyflakes::rules::StringDotFormatExtraPositionalArguments,
        (Pyflakes, "524") => rules::pyflakes::rules::StringDotFormatMissingArguments,
        (Pyflakes, "525") => rules::pyflakes::rules::StringDotFormatMixingAutomatic,
        (Pyflakes, "541") => rules::pyflakes::rules::FStringMissingPlaceholders,
        (Pyflakes, "601") => rules::pyflakes::rules::MultiValueRepeatedKeyLiteral,
        (Pyflakes, "602") => rules::pyflakes::rules::MultiValueRepeatedKeyVariable,
        (Pyflakes, "621") => rules::pyflakes::rules::ExpressionsInStarAssignment,
        (Pyflakes, "622") => rules::pyflakes::rules::MultipleStarredExpressions,
        (Pyflakes, "631") => rules::pyflakes::rules::AssertTuple,
        (Pyflakes, "632") => rules::pyflakes::rules::IsLiteral,
        (Pyflakes, "633") => rules::pyflakes::rules::InvalidPrintSyntax,
        (Pyflakes, "634") => rules::pyflakes::rules::IfTuple,
        (Pyflakes, "701") => rules::pyflakes::rules::BreakOutsideLoop,
        (Pyflakes, "702") => rules::pyflakes::rules::ContinueOutsideLoop,
        (Pyflakes, "704") => rules::pyflakes::rules::YieldOutsideFunction,
        (Pyflakes, "706") => rules::pyflakes::rules::ReturnOutsideFunction,
        (Pyflakes, "707") => rules::pyflakes::rules::DefaultExceptNotLast,
        (Pyflakes, "722") => rules::pyflakes::rules::ForwardAnnotationSyntaxError,
        (Pyflakes, "811") => rules::pyflakes::rules::RedefinedWhileUnused,
        (Pyflakes, "821") => rules::pyflakes::rules::UndefinedName,
        (Pyflakes, "822") => rules::pyflakes::rules::UndefinedExport,
        (Pyflakes, "823") => rules::pyflakes::rules::UndefinedLocal,
        (Pyflakes, "841") => rules::pyflakes::rules::UnusedVariable,
        (Pyflakes, "842") => rules::pyflakes::rules::UnusedAnnotation,
        (Pyflakes, "901") => rules::pyflakes::rules::RaiseNotImplemented,

        // pylint
        (Pylint, "C0105") => rules::pylint::rules::TypeNameIncorrectVariance,
        (Pylint, "C0131") => rules::pylint::rules::TypeBivariance,
        (Pylint, "C0132") => rules::pylint::rules::TypeParamNameMismatch,
        (Pylint, "C0205") => rules::pylint::rules::SingleStringSlots,
        (Pylint, "C0206") => rules::pylint::rules::DictIndexMissingItems,
        (Pylint, "C0207") => rules::pylint::rules::MissingMaxsplitArg,
        (Pylint, "C0208") => rules::pylint::rules::IterationOverSet,
        (Pylint, "C0414") => rules::pylint::rules::UselessImportAlias,
        (Pylint, "C0415") => rules::pylint::rules::ImportOutsideTopLevel,
        (Pylint, "C1802") => rules::pylint::rules::LenTest,
        (Pylint, "C1901") => rules::pylint::rules::CompareToEmptyString,
        (Pylint, "C2401") => rules::pylint::rules::NonAsciiName,
        (Pylint, "C2403") => rules::pylint::rules::NonAsciiImportName,
        (Pylint, "C2701") => rules::pylint::rules::ImportPrivateName,
        (Pylint, "C2801") => rules::pylint::rules::UnnecessaryDunderCall,
        (Pylint, "C3002") => rules::pylint::rules::UnnecessaryDirectLambdaCall,
        (Pylint, "E0100") => rules::pylint::rules::YieldInInit,
        (Pylint, "E0101") => rules::pylint::rules::ReturnInInit,
        (Pylint, "E0115") => rules::pylint::rules::NonlocalAndGlobal,
        (Pylint, "E0116") => rules::pylint::rules::ContinueInFinally,
        (Pylint, "E0117") => rules::pylint::rules::NonlocalWithoutBinding,
        (Pylint, "E0118") => rules::pylint::rules::LoadBeforeGlobalDeclaration,
        (Pylint, "E0237") => rules::pylint::rules::NonSlotAssignment,
        (Pylint, "E0241") => rules::pylint::rules::DuplicateBases,
        (Pylint, "E0302") => rules::pylint::rules::UnexpectedSpecialMethodSignature,
        (Pylint, "E0303") => rules::pylint::rules::InvalidLengthReturnType,
        (Pylint, "E0304") => rules::pylint::rules::InvalidBoolReturnType,
        (Pylint, "E0305") => rules::pylint::rules::InvalidIndexReturnType,
        (Pylint, "E0307") => rules::pylint::rules::InvalidStrReturnType,
        (Pylint, "E0308") => rules::pylint::rules::InvalidBytesReturnType,
        (Pylint, "E0309") => rules::pylint::rules::InvalidHashReturnType,
        (Pylint, "E0604") => rules::pylint::rules::InvalidAllObject,
        (Pylint, "E0605") => rules::pylint::rules::InvalidAllFormat,
        (Pylint, "E0643") => rules::pylint::rules::PotentialIndexError,
        (Pylint, "E0704") => rules::pylint::rules::MisplacedBareRaise,
        (Pylint, "E1132") => rules::pylint::rules::RepeatedKeywordArgument,
        (Pylint, "E1141") => rules::pylint::rules::DictIterMissingItems,
        (Pylint, "E1142") => rules::pylint::rules::AwaitOutsideAsync,
        (Pylint, "E1205") => rules::pylint::rules::LoggingTooManyArgs,
        (Pylint, "E1206") => rules::pylint::rules::LoggingTooFewArgs,
        (Pylint, "E1300") => rules::pylint::rules::BadStringFormatCharacter,
        (Pylint, "E1307") => rules::pylint::rules::BadStringFormatType,
        (Pylint, "E1310") => rules::pylint::rules::BadStrStripCall,
        (Pylint, "E1507") => rules::pylint::rules::InvalidEnvvarValue,
        (Pylint, "E1519") => rules::pylint::rules::SingledispatchMethod,
        (Pylint, "E1520") => rules::pylint::rules::SingledispatchmethodFunction,
        (Pylint, "E1700") => rules::pylint::rules::YieldFromInAsyncFunction,
        (Pylint, "E2502") => rules::pylint::rules::BidirectionalUnicode,
        (Pylint, "E2510") => rules::pylint::rules::InvalidCharacterBackspace,
        (Pylint, "E2512") => rules::pylint::rules::InvalidCharacterSub,
        (Pylint, "E2513") => rules::pylint::rules::InvalidCharacterEsc,
        (Pylint, "E2514") => rules::pylint::rules::InvalidCharacterNul,
        (Pylint, "E2515") => rules::pylint::rules::InvalidCharacterZeroWidthSpace,
        (Pylint, "E4703") => rules::pylint::rules::ModifiedIteratingSet,
        (Pylint, "R0124") => rules::pylint::rules::ComparisonWithItself,
        (Pylint, "R0133") => rules::pylint::rules::ComparisonOfConstant,
        (Pylint, "R0202") => rules::pylint::rules::NoClassmethodDecorator,
        (Pylint, "R0203") => rules::pylint::rules::NoStaticmethodDecorator,
        (Pylint, "R0206") => rules::pylint::rules::PropertyWithParameters,
        (Pylint, "R0402") => rules::pylint::rules::ManualFromImport,
        (Pylint, "R0904") => rules::pylint::rules::TooManyPublicMethods,
        (Pylint, "R0911") => rules::pylint::rules::TooManyReturnStatements,
        (Pylint, "R0912") => rules::pylint::rules::TooManyBranches,
        (Pylint, "R0913") => rules::pylint::rules::TooManyArguments,
        (Pylint, "R0914") => rules::pylint::rules::TooManyLocals,
        (Pylint, "R0915") => rules::pylint::rules::TooManyStatements,
        (Pylint, "R0916") => rules::pylint::rules::TooManyBooleanExpressions,
        (Pylint, "R0917") => rules::pylint::rules::TooManyPositionalArguments,
        (Pylint, "R1701") => rules::pylint::rules::RepeatedIsinstanceCalls,
        (Pylint, "R1702") => rules::pylint::rules::TooManyNestedBlocks,
        (Pylint, "R1704") => rules::pylint::rules::RedefinedArgumentFromLocal,
        (Pylint, "R1706") => rules::pylint::rules::AndOrTernary,
        (Pylint, "R1708") => rules::pylint::rules::StopIterationReturn,
        (Pylint, "R1711") => rules::pylint::rules::UselessReturn,
        (Pylint, "R1714") => rules::pylint::rules::RepeatedEqualityComparison,
        (Pylint, "R1722") => rules::pylint::rules::SysExitAlias,
        (Pylint, "R1730") => rules::pylint::rules::IfStmtMinMax,
        (Pylint, "R1716") => rules::pylint::rules::BooleanChainedComparison,
        (Pylint, "R1733") => rules::pylint::rules::UnnecessaryDictIndexLookup,
        (Pylint, "R1736") => rules::pylint::rules::UnnecessaryListIndexLookup,
        (Pylint, "R2004") => rules::pylint::rules::MagicValueComparison,
        (Pylint, "R2044") => rules::pylint::rules::EmptyComment,
        (Pylint, "R5501") => rules::pylint::rules::CollapsibleElseIf,
        (Pylint, "R6104") => rules::pylint::rules::NonAugmentedAssignment,
        (Pylint, "R6201") => rules::pylint::rules::LiteralMembership,
        (Pylint, "R6301") => rules::pylint::rules::NoSelfUse,
        #[cfg(any(feature = "test-rules", test))]
        (Pylint, "W0101") => rules::pylint::rules::UnreachableCode,
        (Pylint, "W0108") => rules::pylint::rules::UnnecessaryLambda,
        (Pylint, "W0177") => rules::pylint::rules::NanComparison,
        (Pylint, "W0120") => rules::pylint::rules::UselessElseOnLoop,
        (Pylint, "W0127") => rules::pylint::rules::SelfAssigningVariable,
        (Pylint, "W0128") => rules::pylint::rules::RedeclaredAssignedName,
        (Pylint, "W0129") => rules::pylint::rules::AssertOnStringLiteral,
        (Pylint, "W0131") => rules::pylint::rules::NamedExprWithoutContext,
        (Pylint, "W0133") => rules::pylint::rules::UselessExceptionStatement,
        (Pylint, "W0211") => rules::pylint::rules::BadStaticmethodArgument,
        (Pylint, "W0244") => rules::pylint::rules::RedefinedSlotsInSubclass,
        (Pylint, "W0245") => rules::pylint::rules::SuperWithoutBrackets,
        (Pylint, "W0406") => rules::pylint::rules::ImportSelf,
        (Pylint, "W0602") => rules::pylint::rules::GlobalVariableNotAssigned,
        (Pylint, "W0603") => rules::pylint::rules::GlobalStatement,
        (Pylint, "W0604") => rules::pylint::rules::GlobalAtModuleLevel,
        (Pylint, "W0642") => rules::pylint::rules::SelfOrClsAssignment,
        (Pylint, "W0711") => rules::pylint::rules::BinaryOpException,
        (Pylint, "W1501") => rules::pylint::rules::BadOpenMode,
        (Pylint, "W1507") => rules::pylint::rules::ShallowCopyEnviron,
        (Pylint, "W1508") => rules::pylint::rules::InvalidEnvvarDefault,
        (Pylint, "W1509") => rules::pylint::rules::SubprocessPopenPreexecFn,
        (Pylint, "W1510") => rules::pylint::rules::SubprocessRunWithoutCheck,
        (Pylint, "W1514") => rules::pylint::rules::UnspecifiedEncoding,
        (Pylint, "W1641") => rules::pylint::rules::EqWithoutHash,
        (Pylint, "W2101") => rules::pylint::rules::UselessWithLock,
        (Pylint, "W2901") => rules::pylint::rules::RedefinedLoopName,
        (Pylint, "W3201") => rules::pylint::rules::BadDunderMethodName,
        (Pylint, "W3301") => rules::pylint::rules::NestedMinMax,

        // flake8-async
        (Flake8Async, "100") => rules::flake8_async::rules::CancelScopeNoCheckpoint,
        (Flake8Async, "105") => rules::flake8_async::rules::TrioSyncCall,
        (Flake8Async, "109") => rules::flake8_async::rules::AsyncFunctionWithTimeout,
        (Flake8Async, "110") => rules::flake8_async::rules::AsyncBusyWait,
        (Flake8Async, "115") => rules::flake8_async::rules::AsyncZeroSleep,
        (Flake8Async, "116") => rules::flake8_async::rules::LongSleepNotForever,
        (Flake8Async, "210") => rules::flake8_async::rules::BlockingHttpCallInAsyncFunction,
        (Flake8Async, "212") => rules::flake8_async::rules::BlockingHttpCallHttpxInAsyncFunction,
        (Flake8Async, "220") => rules::flake8_async::rules::CreateSubprocessInAsyncFunction,
        (Flake8Async, "221") => rules::flake8_async::rules::RunProcessInAsyncFunction,
        (Flake8Async, "222") => rules::flake8_async::rules::WaitForProcessInAsyncFunction,
        (Flake8Async, "230") => rules::flake8_async::rules::BlockingOpenCallInAsyncFunction,
        (Flake8Async, "240") => rules::flake8_async::rules::BlockingPathMethodInAsyncFunction,
        (Flake8Async, "250") => rules::flake8_async::rules::BlockingInputInAsyncFunction,
        (Flake8Async, "251") => rules::flake8_async::rules::BlockingSleepInAsyncFunction,

        // flake8-builtins
        (Flake8Builtins, "001") => rules::flake8_builtins::rules::BuiltinVariableShadowing,
        (Flake8Builtins, "002") => rules::flake8_builtins::rules::BuiltinArgumentShadowing,
        (Flake8Builtins, "003") => rules::flake8_builtins::rules::BuiltinAttributeShadowing,
        (Flake8Builtins, "004") => rules::flake8_builtins::rules::BuiltinImportShadowing,
        (Flake8Builtins, "005") => rules::flake8_builtins::rules::StdlibModuleShadowing,
        (Flake8Builtins, "006") => rules::flake8_builtins::rules::BuiltinLambdaArgumentShadowing,

        // flake8-bugbear
        (Flake8Bugbear, "002") => rules::flake8_bugbear::rules::UnaryPrefixIncrementDecrement,
        (Flake8Bugbear, "003") => rules::flake8_bugbear::rules::AssignmentToOsEnviron,
        (Flake8Bugbear, "004") => rules::flake8_bugbear::rules::UnreliableCallableCheck,
        (Flake8Bugbear, "005") => rules::flake8_bugbear::rules::StripWithMultiCharacters,
        (Flake8Bugbear, "006") => rules::flake8_bugbear::rules::MutableArgumentDefault,
        (Flake8Bugbear, "007") => rules::flake8_bugbear::rules::UnusedLoopControlVariable,
        (Flake8Bugbear, "008") => rules::flake8_bugbear::rules::FunctionCallInDefaultArgument,
        (Flake8Bugbear, "009") => rules::flake8_bugbear::rules::GetAttrWithConstant,
        (Flake8Bugbear, "010") => rules::flake8_bugbear::rules::SetAttrWithConstant,
        (Flake8Bugbear, "011") => rules::flake8_bugbear::rules::AssertFalse,
        (Flake8Bugbear, "012") => rules::flake8_bugbear::rules::JumpStatementInFinally,
        (Flake8Bugbear, "013") => rules::flake8_bugbear::rules::RedundantTupleInExceptionHandler,
        (Flake8Bugbear, "014") => rules::flake8_bugbear::rules::DuplicateHandlerException,
        (Flake8Bugbear, "015") => rules::flake8_bugbear::rules::UselessComparison,
        (Flake8Bugbear, "016") => rules::flake8_bugbear::rules::RaiseLiteral,
        (Flake8Bugbear, "017") => rules::flake8_bugbear::rules::AssertRaisesException,
        (Flake8Bugbear, "018") => rules::flake8_bugbear::rules::UselessExpression,
        (Flake8Bugbear, "019") => rules::flake8_bugbear::rules::CachedInstanceMethod,
        (Flake8Bugbear, "020") => rules::flake8_bugbear::rules::LoopVariableOverridesIterator,
        (Flake8Bugbear, "021") => rules::flake8_bugbear::rules::FStringDocstring,
        (Flake8Bugbear, "022") => rules::flake8_bugbear::rules::UselessContextlibSuppress,
        (Flake8Bugbear, "023") => rules::flake8_bugbear::rules::FunctionUsesLoopVariable,
        (Flake8Bugbear, "024") => rules::flake8_bugbear::rules::AbstractBaseClassWithoutAbstractMethod,
        (Flake8Bugbear, "025") => rules::flake8_bugbear::rules::DuplicateTryBlockException,
        (Flake8Bugbear, "026") => rules::flake8_bugbear::rules::StarArgUnpackingAfterKeywordArg,
        (Flake8Bugbear, "027") => rules::flake8_bugbear::rules::EmptyMethodWithoutAbstractDecorator,
        (Flake8Bugbear, "028") => rules::flake8_bugbear::rules::NoExplicitStacklevel,
        (Flake8Bugbear, "029") => rules::flake8_bugbear::rules::ExceptWithEmptyTuple,
        (Flake8Bugbear, "030") => rules::flake8_bugbear::rules::ExceptWithNonExceptionClasses,
        (Flake8Bugbear, "031") => rules::flake8_bugbear::rules::ReuseOfGroupbyGenerator,
        (Flake8Bugbear, "032") => rules::flake8_bugbear::rules::UnintentionalTypeAnnotation,
        (Flake8Bugbear, "033") => rules::flake8_bugbear::rules::DuplicateValue,
        (Flake8Bugbear, "034") => rules::flake8_bugbear::rules::ReSubPositionalArgs,
        (Flake8Bugbear, "035") => rules::flake8_bugbear::rules::StaticKeyDictComprehension,
        (Flake8Bugbear, "039") => rules::flake8_bugbear::rules::MutableContextvarDefault,
        (Flake8Bugbear, "901") => rules::flake8_bugbear::rules::ReturnInGenerator,
        (Flake8Bugbear, "903") => rules::flake8_bugbear::rules::ClassAsDataStructure,
        (Flake8Bugbear, "904") => rules::flake8_bugbear::rules::RaiseWithoutFromInsideExcept,
        (Flake8Bugbear, "905") => rules::flake8_bugbear::rules::ZipWithoutExplicitStrict,
        (Flake8Bugbear, "909") => rules::flake8_bugbear::rules::LoopIteratorMutation,
        (Flake8Bugbear, "911") => rules::flake8_bugbear::rules::BatchedWithoutExplicitStrict,
        (Flake8Bugbear, "912") => rules::flake8_bugbear::rules::MapWithoutExplicitStrict,

        // flake8-blind-except
        (Flake8BlindExcept, "001") => rules::flake8_blind_except::rules::BlindExcept,

        // flake8-comprehensions
        (Flake8Comprehensions, "00") => rules::flake8_comprehensions::rules::UnnecessaryGeneratorList,
        (Flake8Comprehensions, "01") => rules::flake8_comprehensions::rules::UnnecessaryGeneratorSet,
        (Flake8Comprehensions, "02") => rules::flake8_comprehensions::rules::UnnecessaryGeneratorDict,
        (Flake8Comprehensions, "03") => rules::flake8_comprehensions::rules::UnnecessaryListComprehensionSet,
        (Flake8Comprehensions, "04") => rules::flake8_comprehensions::rules::UnnecessaryListComprehensionDict,
        (Flake8Comprehensions, "05") => rules::flake8_comprehensions::rules::UnnecessaryLiteralSet,
        (Flake8Comprehensions, "06") => rules::flake8_comprehensions::rules::UnnecessaryLiteralDict,
        (Flake8Comprehensions, "08") => rules::flake8_comprehensions::rules::UnnecessaryCollectionCall,
        (Flake8Comprehensions, "09") => rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinTupleCall,
        (Flake8Comprehensions, "10") => rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinListCall,
        (Flake8Comprehensions, "11") => rules::flake8_comprehensions::rules::UnnecessaryListCall,
        (Flake8Comprehensions, "13") => rules::flake8_comprehensions::rules::UnnecessaryCallAroundSorted,
        (Flake8Comprehensions, "14") => rules::flake8_comprehensions::rules::UnnecessaryDoubleCastOrProcess,
        (Flake8Comprehensions, "15") => rules::flake8_comprehensions::rules::UnnecessarySubscriptReversal,
        (Flake8Comprehensions, "16") => rules::flake8_comprehensions::rules::UnnecessaryComprehension,
        (Flake8Comprehensions, "17") => rules::flake8_comprehensions::rules::UnnecessaryMap,
        (Flake8Comprehensions, "18") => rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinDictCall,
        (Flake8Comprehensions, "19") => rules::flake8_comprehensions::rules::UnnecessaryComprehensionInCall,
        (Flake8Comprehensions, "20") => rules::flake8_comprehensions::rules::UnnecessaryDictComprehensionForIterable,

        // flake8-debugger
        (Flake8Debugger, "0") => rules::flake8_debugger::rules::Debugger,

        // mccabe
        (McCabe, "1") => rules::mccabe::rules::ComplexStructure,

        // flake8-tidy-imports
        (Flake8TidyImports, "251") => rules::flake8_tidy_imports::rules::BannedApi,
        (Flake8TidyImports, "252") => rules::flake8_tidy_imports::rules::RelativeImports,
        (Flake8TidyImports, "253") => rules::flake8_tidy_imports::rules::BannedModuleLevelImports,

        // flake8-return
        (Flake8Return, "501") => rules::flake8_return::rules::UnnecessaryReturnNone,
        (Flake8Return, "502") => rules::flake8_return::rules::ImplicitReturnValue,
        (Flake8Return, "503") => rules::flake8_return::rules::ImplicitReturn,
        (Flake8Return, "504") => rules::flake8_return::rules::UnnecessaryAssign,
        (Flake8Return, "505") => rules::flake8_return::rules::SuperfluousElseReturn,
        (Flake8Return, "506") => rules::flake8_return::rules::SuperfluousElseRaise,
        (Flake8Return, "507") => rules::flake8_return::rules::SuperfluousElseContinue,
        (Flake8Return, "508") => rules::flake8_return::rules::SuperfluousElseBreak,

        // flake8-gettext
        (Flake8GetText, "001") => rules::flake8_gettext::rules::FStringInGetTextFuncCall,
        (Flake8GetText, "002") => rules::flake8_gettext::rules::FormatInGetTextFuncCall,
        (Flake8GetText, "003") => rules::flake8_gettext::rules::PrintfInGetTextFuncCall,

        // flake8-implicit-str-concat
        (Flake8ImplicitStrConcat, "001") => rules::flake8_implicit_str_concat::rules::SingleLineImplicitStringConcatenation,
        (Flake8ImplicitStrConcat, "002") => rules::flake8_implicit_str_concat::rules::MultiLineImplicitStringConcatenation,
        (Flake8ImplicitStrConcat, "003") => rules::flake8_implicit_str_concat::rules::ExplicitStringConcatenation,

        // flake8-print
        (Flake8Print, "1") => rules::flake8_print::rules::Print,
        (Flake8Print, "3") => rules::flake8_print::rules::PPrint,

        // flake8-quotes
        (Flake8Quotes, "000") => rules::flake8_quotes::rules::BadQuotesInlineString,
        (Flake8Quotes, "001") => rules::flake8_quotes::rules::BadQuotesMultilineString,
        (Flake8Quotes, "002") => rules::flake8_quotes::rules::BadQuotesDocstring,
        (Flake8Quotes, "003") => rules::flake8_quotes::rules::AvoidableEscapedQuote,
        (Flake8Quotes, "004") => rules::flake8_quotes::rules::UnnecessaryEscapedQuote,

        // flake8-annotations
        (Flake8Annotations, "001") => rules::flake8_annotations::rules::MissingTypeFunctionArgument,
        (Flake8Annotations, "002") => rules::flake8_annotations::rules::MissingTypeArgs,
        (Flake8Annotations, "003") => rules::flake8_annotations::rules::MissingTypeKwargs,
        #[allow(deprecated)]
        (Flake8Annotations, "101") => rules::flake8_annotations::rules::MissingTypeSelf,
        #[allow(deprecated)]
        (Flake8Annotations, "102") => rules::flake8_annotations::rules::MissingTypeCls,
        (Flake8Annotations, "201") => rules::flake8_annotations::rules::MissingReturnTypeUndocumentedPublicFunction,
        (Flake8Annotations, "202") => rules::flake8_annotations::rules::MissingReturnTypePrivateFunction,
        (Flake8Annotations, "204") => rules::flake8_annotations::rules::MissingReturnTypeSpecialMethod,
        (Flake8Annotations, "205") => rules::flake8_annotations::rules::MissingReturnTypeStaticMethod,
        (Flake8Annotations, "206") => rules::flake8_annotations::rules::MissingReturnTypeClassMethod,
        (Flake8Annotations, "401") => rules::flake8_annotations::rules::AnyType,

        // flake8-future-annotations
        (Flake8FutureAnnotations, "100") => rules::flake8_future_annotations::rules::FutureRewritableTypeAnnotation,
        (Flake8FutureAnnotations, "102") => rules::flake8_future_annotations::rules::FutureRequiredTypeAnnotation,

        // flake8-2020
        (Flake82020, "101") => rules::flake8_2020::rules::SysVersionSlice3,
        (Flake82020, "102") => rules::flake8_2020::rules::SysVersion2,
        (Flake82020, "103") => rules::flake8_2020::rules::SysVersionCmpStr3,
        (Flake82020, "201") => rules::flake8_2020::rules::SysVersionInfo0Eq3,
        (Flake82020, "202") => rules::flake8_2020::rules::SixPY3,
        (Flake82020, "203") => rules::flake8_2020::rules::SysVersionInfo1CmpInt,
        (Flake82020, "204") => rules::flake8_2020::rules::SysVersionInfoMinorCmpInt,
        (Flake82020, "301") => rules::flake8_2020::rules::SysVersion0,
        (Flake82020, "302") => rules::flake8_2020::rules::SysVersionCmpStr10,
        (Flake82020, "303") => rules::flake8_2020::rules::SysVersionSlice1,

        // flake8-simplify
        (Flake8Simplify, "101") => rules::flake8_simplify::rules::DuplicateIsinstanceCall,
        (Flake8Simplify, "102") => rules::flake8_simplify::rules::CollapsibleIf,
        (Flake8Simplify, "103") => rules::flake8_simplify::rules::NeedlessBool,
        (Flake8Simplify, "105") => rules::flake8_simplify::rules::SuppressibleException,
        (Flake8Simplify, "107") => rules::flake8_simplify::rules::ReturnInTryExceptFinally,
        (Flake8Simplify, "108") => rules::flake8_simplify::rules::IfElseBlockInsteadOfIfExp,
        (Flake8Simplify, "109") => rules::flake8_simplify::rules::CompareWithTuple,
        (Flake8Simplify, "110") => rules::flake8_simplify::rules::ReimplementedBuiltin,
        (Flake8Simplify, "112") => rules::flake8_simplify::rules::UncapitalizedEnvironmentVariables,
        (Flake8Simplify, "113") => rules::flake8_simplify::rules::EnumerateForLoop,
        (Flake8Simplify, "114") => rules::flake8_simplify::rules::IfWithSameArms,
        (Flake8Simplify, "115") => rules::flake8_simplify::rules::OpenFileWithContextHandler,
        (Flake8Simplify, "116") => rules::flake8_simplify::rules::IfElseBlockInsteadOfDictLookup,
        (Flake8Simplify, "117") => rules::flake8_simplify::rules::MultipleWithStatements,
        (Flake8Simplify, "118") => rules::flake8_simplify::rules::InDictKeys,
        (Flake8Simplify, "201") => rules::flake8_simplify::rules::NegateEqualOp,
        (Flake8Simplify, "202") => rules::flake8_simplify::rules::NegateNotEqualOp,
        (Flake8Simplify, "208") => rules::flake8_simplify::rules::DoubleNegation,
        (Flake8Simplify, "210") => rules::flake8_simplify::rules::IfExprWithTrueFalse,
        (Flake8Simplify, "211") => rules::flake8_simplify::rules::IfExprWithFalseTrue,
        (Flake8Simplify, "212") => rules::flake8_simplify::rules::IfExprWithTwistedArms,
        (Flake8Simplify, "220") => rules::flake8_simplify::rules::ExprAndNotExpr,
        (Flake8Simplify, "221") => rules::flake8_simplify::rules::ExprOrNotExpr,
        (Flake8Simplify, "222") => rules::flake8_simplify::rules::ExprOrTrue,
        (Flake8Simplify, "223") => rules::flake8_simplify::rules::ExprAndFalse,
        (Flake8Simplify, "300") => rules::flake8_simplify::rules::YodaConditions,
        (Flake8Simplify, "401") => rules::flake8_simplify::rules::IfElseBlockInsteadOfDictGet,
        (Flake8Simplify, "905") => rules::flake8_simplify::rules::SplitStaticString,
        (Flake8Simplify, "910") => rules::flake8_simplify::rules::DictGetWithNoneDefault,
        (Flake8Simplify, "911") => rules::flake8_simplify::rules::ZipDictKeysAndValues,

        // flake8-copyright
        (Flake8Copyright, "001") => rules::flake8_copyright::rules::MissingCopyrightNotice,

        // pyupgrade
        (Pyupgrade, "001") => rules::pyupgrade::rules::UselessMetaclassType,
        (Pyupgrade, "003") => rules::pyupgrade::rules::TypeOfPrimitive,
        (Pyupgrade, "004") => rules::pyupgrade::rules::UselessObjectInheritance,
        (Pyupgrade, "005") => rules::pyupgrade::rules::DeprecatedUnittestAlias,
        (Pyupgrade, "006") => rules::pyupgrade::rules::NonPEP585Annotation,
        (Pyupgrade, "007") => rules::pyupgrade::rules::NonPEP604AnnotationUnion,
        (Pyupgrade, "008") => rules::pyupgrade::rules::SuperCallWithParameters,
        (Pyupgrade, "009") => rules::pyupgrade::rules::UTF8EncodingDeclaration,
        (Pyupgrade, "010") => rules::pyupgrade::rules::UnnecessaryFutureImport,
        (Pyupgrade, "011") => rules::pyupgrade::rules::LRUCacheWithoutParameters,
        (Pyupgrade, "012") => rules::pyupgrade::rules::UnnecessaryEncodeUTF8,
        (Pyupgrade, "013") => rules::pyupgrade::rules::ConvertTypedDictFunctionalToClass,
        (Pyupgrade, "014") => rules::pyupgrade::rules::ConvertNamedTupleFunctionalToClass,
        (Pyupgrade, "015") => rules::pyupgrade::rules::RedundantOpenModes,
        (Pyupgrade, "017") => rules::pyupgrade::rules::DatetimeTimezoneUTC,
        (Pyupgrade, "018") => rules::pyupgrade::rules::NativeLiterals,
        (Pyupgrade, "019") => rules::pyupgrade::rules::TypingTextStrAlias,
        (Pyupgrade, "020") => rules::pyupgrade::rules::OpenAlias,
        (Pyupgrade, "021") => rules::pyupgrade::rules::ReplaceUniversalNewlines,
        (Pyupgrade, "022") => rules::pyupgrade::rules::ReplaceStdoutStderr,
        (Pyupgrade, "023") => rules::pyupgrade::rules::DeprecatedCElementTree,
        (Pyupgrade, "024") => rules::pyupgrade::rules::OSErrorAlias,
        (Pyupgrade, "025") => rules::pyupgrade::rules::UnicodeKindPrefix,
        (Pyupgrade, "026") => rules::pyupgrade::rules::DeprecatedMockImport,
        (Pyupgrade, "027") => rules::pyupgrade::rules::UnpackedListComprehension,
        (Pyupgrade, "028") => rules::pyupgrade::rules::YieldInForLoop,
        (Pyupgrade, "029") => rules::pyupgrade::rules::UnnecessaryBuiltinImport,
        (Pyupgrade, "030") => rules::pyupgrade::rules::FormatLiterals,
        (Pyupgrade, "031") => rules::pyupgrade::rules::PrintfStringFormatting,
        (Pyupgrade, "032") => rules::pyupgrade::rules::FString,
        (Pyupgrade, "033") => rules::pyupgrade::rules::LRUCacheWithMaxsizeNone,
        (Pyupgrade, "034") => rules::pyupgrade::rules::ExtraneousParentheses,
        (Pyupgrade, "035") => rules::pyupgrade::rules::DeprecatedImport,
        (Pyupgrade, "036") => rules::pyupgrade::rules::OutdatedVersionBlock,
        (Pyupgrade, "037") => rules::pyupgrade::rules::QuotedAnnotation,
        (Pyupgrade, "038") => rules::pyupgrade::rules::NonPEP604Isinstance,
        (Pyupgrade, "039") => rules::pyupgrade::rules::UnnecessaryClassParentheses,
        (Pyupgrade, "040") => rules::pyupgrade::rules::NonPEP695TypeAlias,
        (Pyupgrade, "041") => rules::pyupgrade::rules::TimeoutErrorAlias,
        (Pyupgrade, "042") => rules::pyupgrade::rules::ReplaceStrEnum,
        (Pyupgrade, "043") => rules::pyupgrade::rules::UnnecessaryDefaultTypeArgs,
        (Pyupgrade, "044") => rules::pyupgrade::rules::NonPEP646Unpack,
        (Pyupgrade, "045") => rules::pyupgrade::rules::NonPEP604AnnotationOptional,
        (Pyupgrade, "046") => rules::pyupgrade::rules::NonPEP695GenericClass,
        (Pyupgrade, "047") => rules::pyupgrade::rules::NonPEP695GenericFunction,
        (Pyupgrade, "049") => rules::pyupgrade::rules::PrivateTypeParameter,
        (Pyupgrade, "050") => rules::pyupgrade::rules::UselessClassMetaclassType,

        // pydocstyle
        (Pydocstyle, "100") => rules::pydocstyle::rules::UndocumentedPublicModule,
        (Pydocstyle, "101") => rules::pydocstyle::rules::UndocumentedPublicClass,
        (Pydocstyle, "102") => rules::pydocstyle::rules::UndocumentedPublicMethod,
        (Pydocstyle, "103") => rules::pydocstyle::rules::UndocumentedPublicFunction,
        (Pydocstyle, "104") => rules::pydocstyle::rules::UndocumentedPublicPackage,
        (Pydocstyle, "105") => rules::pydocstyle::rules::UndocumentedMagicMethod,
        (Pydocstyle, "106") => rules::pydocstyle::rules::UndocumentedPublicNestedClass,
        (Pydocstyle, "107") => rules::pydocstyle::rules::UndocumentedPublicInit,
        (Pydocstyle, "200") => rules::pydocstyle::rules::UnnecessaryMultilineDocstring,
        (Pydocstyle, "201") => rules::pydocstyle::rules::BlankLineBeforeFunction,
        (Pydocstyle, "202") => rules::pydocstyle::rules::BlankLineAfterFunction,
        (Pydocstyle, "203") => rules::pydocstyle::rules::IncorrectBlankLineBeforeClass,
        (Pydocstyle, "204") => rules::pydocstyle::rules::IncorrectBlankLineAfterClass,
        (Pydocstyle, "205") => rules::pydocstyle::rules::MissingBlankLineAfterSummary,
        (Pydocstyle, "206") => rules::pydocstyle::rules::DocstringTabIndentation,
        (Pydocstyle, "207") => rules::pydocstyle::rules::UnderIndentation,
        (Pydocstyle, "208") => rules::pydocstyle::rules::OverIndentation,
        (Pydocstyle, "209") => rules::pydocstyle::rules::NewLineAfterLastParagraph,
        (Pydocstyle, "210") => rules::pydocstyle::rules::SurroundingWhitespace,
        (Pydocstyle, "211") => rules::pydocstyle::rules::BlankLineBeforeClass,
        (Pydocstyle, "212") => rules::pydocstyle::rules::MultiLineSummaryFirstLine,
        (Pydocstyle, "213") => rules::pydocstyle::rules::MultiLineSummarySecondLine,
        (Pydocstyle, "214") => rules::pydocstyle::rules::OverindentedSection,
        (Pydocstyle, "215") => rules::pydocstyle::rules::OverindentedSectionUnderline,
        (Pydocstyle, "300") => rules::pydocstyle::rules::TripleSingleQuotes,
        (Pydocstyle, "301") => rules::pydocstyle::rules::EscapeSequenceInDocstring,
        (Pydocstyle, "400") => rules::pydocstyle::rules::MissingTrailingPeriod,
        (Pydocstyle, "401") => rules::pydocstyle::rules::NonImperativeMood,
        (Pydocstyle, "402") => rules::pydocstyle::rules::SignatureInDocstring,
        (Pydocstyle, "403") => rules::pydocstyle::rules::FirstWordUncapitalized,
        (Pydocstyle, "404") => rules::pydocstyle::rules::DocstringStartsWithThis,
        (Pydocstyle, "405") => rules::pydocstyle::rules::NonCapitalizedSectionName,
        (Pydocstyle, "406") => rules::pydocstyle::rules::MissingNewLineAfterSectionName,
        (Pydocstyle, "407") => rules::pydocstyle::rules::MissingDashedUnderlineAfterSection,
        (Pydocstyle, "408") => rules::pydocstyle::rules::MissingSectionUnderlineAfterName,
        (Pydocstyle, "409") => rules::pydocstyle::rules::MismatchedSectionUnderlineLength,
        (Pydocstyle, "410") => rules::pydocstyle::rules::NoBlankLineAfterSection,
        (Pydocstyle, "411") => rules::pydocstyle::rules::NoBlankLineBeforeSection,
        (Pydocstyle, "412") => rules::pydocstyle::rules::BlankLinesBetweenHeaderAndContent,
        (Pydocstyle, "413") => rules::pydocstyle::rules::MissingBlankLineAfterLastSection,
        (Pydocstyle, "414") => rules::pydocstyle::rules::EmptyDocstringSection,
        (Pydocstyle, "415") => rules::pydocstyle::rules::MissingTerminalPunctuation,
        (Pydocstyle, "416") => rules::pydocstyle::rules::MissingSectionNameColon,
        (Pydocstyle, "417") => rules::pydocstyle::rules::UndocumentedParam,
        (Pydocstyle, "418") => rules::pydocstyle::rules::OverloadWithDocstring,
        (Pydocstyle, "419") => rules::pydocstyle::rules::EmptyDocstring,

        // pep8-naming
        (PEP8Naming, "801") => rules::pep8_naming::rules::InvalidClassName,
        (PEP8Naming, "802") => rules::pep8_naming::rules::InvalidFunctionName,
        (PEP8Naming, "803") => rules::pep8_naming::rules::InvalidArgumentName,
        (PEP8Naming, "804") => rules::pep8_naming::rules::InvalidFirstArgumentNameForClassMethod,
        (PEP8Naming, "805") => rules::pep8_naming::rules::InvalidFirstArgumentNameForMethod,
        (PEP8Naming, "806") => rules::pep8_naming::rules::NonLowercaseVariableInFunction,
        (PEP8Naming, "807") => rules::pep8_naming::rules::DunderFunctionName,
        (PEP8Naming, "811") => rules::pep8_naming::rules::ConstantImportedAsNonConstant,
        (PEP8Naming, "812") => rules::pep8_naming::rules::LowercaseImportedAsNonLowercase,
        (PEP8Naming, "813") => rules::pep8_naming::rules::CamelcaseImportedAsLowercase,
        (PEP8Naming, "814") => rules::pep8_naming::rules::CamelcaseImportedAsConstant,
        (PEP8Naming, "815") => rules::pep8_naming::rules::MixedCaseVariableInClassScope,
        (PEP8Naming, "816") => rules::pep8_naming::rules::MixedCaseVariableInGlobalScope,
        (PEP8Naming, "817") => rules::pep8_naming::rules::CamelcaseImportedAsAcronym,
        (PEP8Naming, "818") => rules::pep8_naming::rules::ErrorSuffixOnExceptionName,
        (PEP8Naming, "999") => rules::pep8_naming::rules::InvalidModuleName,

        // isort
        (Isort, "001") => rules::isort::rules::UnsortedImports,
        (Isort, "002") => rules::isort::rules::MissingRequiredImport,

        // eradicate
        (Eradicate, "001") => rules::eradicate::rules::CommentedOutCode,

        // flake8-bandit
        (Flake8Bandit, "101") => rules::flake8_bandit::rules::Assert,
        (Flake8Bandit, "102") => rules::flake8_bandit::rules::ExecBuiltin,
        (Flake8Bandit, "103") => rules::flake8_bandit::rules::BadFilePermissions,
        (Flake8Bandit, "104") => rules::flake8_bandit::rules::HardcodedBindAllInterfaces,
        (Flake8Bandit, "105") => rules::flake8_bandit::rules::HardcodedPasswordString,
        (Flake8Bandit, "106") => rules::flake8_bandit::rules::HardcodedPasswordFuncArg,
        (Flake8Bandit, "107") => rules::flake8_bandit::rules::HardcodedPasswordDefault,
        (Flake8Bandit, "108") => rules::flake8_bandit::rules::HardcodedTempFile,
        (Flake8Bandit, "110") => rules::flake8_bandit::rules::TryExceptPass,
        (Flake8Bandit, "112") => rules::flake8_bandit::rules::TryExceptContinue,
        (Flake8Bandit, "113") => rules::flake8_bandit::rules::RequestWithoutTimeout,
        (Flake8Bandit, "201") => rules::flake8_bandit::rules::FlaskDebugTrue,
        (Flake8Bandit, "202") => rules::flake8_bandit::rules::TarfileUnsafeMembers,
        (Flake8Bandit, "301") => rules::flake8_bandit::rules::SuspiciousPickleUsage,
        (Flake8Bandit, "302") => rules::flake8_bandit::rules::SuspiciousMarshalUsage,
        (Flake8Bandit, "303") => rules::flake8_bandit::rules::SuspiciousInsecureHashUsage,
        (Flake8Bandit, "304") => rules::flake8_bandit::rules::SuspiciousInsecureCipherUsage,
        (Flake8Bandit, "305") => rules::flake8_bandit::rules::SuspiciousInsecureCipherModeUsage,
        (Flake8Bandit, "306") => rules::flake8_bandit::rules::SuspiciousMktempUsage,
        (Flake8Bandit, "307") => rules::flake8_bandit::rules::SuspiciousEvalUsage,
        (Flake8Bandit, "308") => rules::flake8_bandit::rules::SuspiciousMarkSafeUsage,
        (Flake8Bandit, "310") => rules::flake8_bandit::rules::SuspiciousURLOpenUsage,
        (Flake8Bandit, "311") => rules::flake8_bandit::rules::SuspiciousNonCryptographicRandomUsage,
        (Flake8Bandit, "312") => rules::flake8_bandit::rules::SuspiciousTelnetUsage,
        (Flake8Bandit, "313") => rules::flake8_bandit::rules::SuspiciousXMLCElementTreeUsage,
        (Flake8Bandit, "314") => rules::flake8_bandit::rules::SuspiciousXMLElementTreeUsage,
        (Flake8Bandit, "315") => rules::flake8_bandit::rules::SuspiciousXMLExpatReaderUsage,
        (Flake8Bandit, "316") => rules::flake8_bandit::rules::SuspiciousXMLExpatBuilderUsage,
        (Flake8Bandit, "317") => rules::flake8_bandit::rules::SuspiciousXMLSaxUsage,
        (Flake8Bandit, "318") => rules::flake8_bandit::rules::SuspiciousXMLMiniDOMUsage,
        (Flake8Bandit, "319") => rules::flake8_bandit::rules::SuspiciousXMLPullDOMUsage,
        (Flake8Bandit, "320") => rules::flake8_bandit::rules::SuspiciousXMLETreeUsage,
        (Flake8Bandit, "321") => rules::flake8_bandit::rules::SuspiciousFTPLibUsage,
        (Flake8Bandit, "323") => rules::flake8_bandit::rules::SuspiciousUnverifiedContextUsage,
        (Flake8Bandit, "324") => rules::flake8_bandit::rules::HashlibInsecureHashFunction,
        (Flake8Bandit, "401") => rules::flake8_bandit::rules::SuspiciousTelnetlibImport,
        (Flake8Bandit, "402") => rules::flake8_bandit::rules::SuspiciousFtplibImport,
        (Flake8Bandit, "403") => rules::flake8_bandit::rules::SuspiciousPickleImport,
        (Flake8Bandit, "404") => rules::flake8_bandit::rules::SuspiciousSubprocessImport,
        (Flake8Bandit, "405") => rules::flake8_bandit::rules::SuspiciousXmlEtreeImport,
        (Flake8Bandit, "406") => rules::flake8_bandit::rules::SuspiciousXmlSaxImport,
        (Flake8Bandit, "407") => rules::flake8_bandit::rules::SuspiciousXmlExpatImport,
        (Flake8Bandit, "408") => rules::flake8_bandit::rules::SuspiciousXmlMinidomImport,
        (Flake8Bandit, "409") => rules::flake8_bandit::rules::SuspiciousXmlPulldomImport,
        (Flake8Bandit, "410") => rules::flake8_bandit::rules::SuspiciousLxmlImport,
        (Flake8Bandit, "411") => rules::flake8_bandit::rules::SuspiciousXmlrpcImport,
        (Flake8Bandit, "412") => rules::flake8_bandit::rules::SuspiciousHttpoxyImport,
        (Flake8Bandit, "413") => rules::flake8_bandit::rules::SuspiciousPycryptoImport,
        (Flake8Bandit, "415") => rules::flake8_bandit::rules::SuspiciousPyghmiImport,
        (Flake8Bandit, "501") => rules::flake8_bandit::rules::RequestWithNoCertValidation,
        (Flake8Bandit, "502") => rules::flake8_bandit::rules::SslInsecureVersion,
        (Flake8Bandit, "503") => rules::flake8_bandit::rules::SslWithBadDefaults,
        (Flake8Bandit, "504") => rules::flake8_bandit::rules::SslWithNoVersion,
        (Flake8Bandit, "505") => rules::flake8_bandit::rules::WeakCryptographicKey,
        (Flake8Bandit, "506") => rules::flake8_bandit::rules::UnsafeYAMLLoad,
        (Flake8Bandit, "507") => rules::flake8_bandit::rules::SSHNoHostKeyVerification,
        (Flake8Bandit, "508") => rules::flake8_bandit::rules::SnmpInsecureVersion,
        (Flake8Bandit, "509") => rules::flake8_bandit::rules::SnmpWeakCryptography,
        (Flake8Bandit, "601") => rules::flake8_bandit::rules::ParamikoCall,
        (Flake8Bandit, "602") => rules::flake8_bandit::rules::SubprocessPopenWithShellEqualsTrue,
        (Flake8Bandit, "603") => rules::flake8_bandit::rules::SubprocessWithoutShellEqualsTrue,
        (Flake8Bandit, "604") => rules::flake8_bandit::rules::CallWithShellEqualsTrue,
        (Flake8Bandit, "605") => rules::flake8_bandit::rules::StartProcessWithAShell,
        (Flake8Bandit, "606") => rules::flake8_bandit::rules::StartProcessWithNoShell,
        (Flake8Bandit, "607") => rules::flake8_bandit::rules::StartProcessWithPartialPath,
        (Flake8Bandit, "608") => rules::flake8_bandit::rules::HardcodedSQLExpression,
        (Flake8Bandit, "609") => rules::flake8_bandit::rules::UnixCommandWildcardInjection,
        (Flake8Bandit, "610") => rules::flake8_bandit::rules::DjangoExtra,
        (Flake8Bandit, "611") => rules::flake8_bandit::rules::DjangoRawSql,
        (Flake8Bandit, "612") => rules::flake8_bandit::rules::LoggingConfigInsecureListen,
        (Flake8Bandit, "701") => rules::flake8_bandit::rules::Jinja2AutoescapeFalse,
        (Flake8Bandit, "702") => rules::flake8_bandit::rules::MakoTemplates,
        (Flake8Bandit, "704") => rules::flake8_bandit::rules::UnsafeMarkupUse,

        // flake8-boolean-trap
        (Flake8BooleanTrap, "001") => rules::flake8_boolean_trap::rules::BooleanTypeHintPositionalArgument,
        (Flake8BooleanTrap, "002") => rules::flake8_boolean_trap::rules::BooleanDefaultValuePositionalArgument,
        (Flake8BooleanTrap, "003") => rules::flake8_boolean_trap::rules::BooleanPositionalValueInCall,

        // flake8-unused-arguments
        (Flake8UnusedArguments, "001") => rules::flake8_unused_arguments::rules::UnusedFunctionArgument,
        (Flake8UnusedArguments, "002") => rules::flake8_unused_arguments::rules::UnusedMethodArgument,
        (Flake8UnusedArguments, "003") => rules::flake8_unused_arguments::rules::UnusedClassMethodArgument,
        (Flake8UnusedArguments, "004") => rules::flake8_unused_arguments::rules::UnusedStaticMethodArgument,
        (Flake8UnusedArguments, "005") => rules::flake8_unused_arguments::rules::UnusedLambdaArgument,

        // flake8-import-conventions
        (Flake8ImportConventions, "001") => rules::flake8_import_conventions::rules::UnconventionalImportAlias,
        (Flake8ImportConventions, "002") => rules::flake8_import_conventions::rules::BannedImportAlias,
        (Flake8ImportConventions, "003") => rules::flake8_import_conventions::rules::BannedImportFrom,

        // flake8-datetimez
        (Flake8Datetimez, "001") => rules::flake8_datetimez::rules::CallDatetimeWithoutTzinfo,
        (Flake8Datetimez, "002") => rules::flake8_datetimez::rules::CallDatetimeToday,
        (Flake8Datetimez, "003") => rules::flake8_datetimez::rules::CallDatetimeUtcnow,
        (Flake8Datetimez, "004") => rules::flake8_datetimez::rules::CallDatetimeUtcfromtimestamp,
        (Flake8Datetimez, "005") => rules::flake8_datetimez::rules::CallDatetimeNowWithoutTzinfo,
        (Flake8Datetimez, "006") => rules::flake8_datetimez::rules::CallDatetimeFromtimestamp,
        (Flake8Datetimez, "007") => rules::flake8_datetimez::rules::CallDatetimeStrptimeWithoutZone,
        (Flake8Datetimez, "011") => rules::flake8_datetimez::rules::CallDateToday,
        (Flake8Datetimez, "012") => rules::flake8_datetimez::rules::CallDateFromtimestamp,
        (Flake8Datetimez, "901") => rules::flake8_datetimez::rules::DatetimeMinMax,

        // pygrep-hooks
        (PygrepHooks, "001") => rules::pygrep_hooks::rules::Eval,
        (PygrepHooks, "002") => rules::pygrep_hooks::rules::DeprecatedLogWarn,
        (PygrepHooks, "003") => rules::pygrep_hooks::rules::BlanketTypeIgnore,
        (PygrepHooks, "004") => rules::pygrep_hooks::rules::BlanketNOQA,
        (PygrepHooks, "005") => rules::pygrep_hooks::rules::InvalidMockAccess,

        // pandas-vet
        (PandasVet, "002") => rules::pandas_vet::rules::PandasUseOfInplaceArgument,
        (PandasVet, "003") => rules::pandas_vet::rules::PandasUseOfDotIsNull,
        (PandasVet, "004") => rules::pandas_vet::rules::PandasUseOfDotNotNull,
        (PandasVet, "007") => rules::pandas_vet::rules::PandasUseOfDotIx,
        (PandasVet, "008") => rules::pandas_vet::rules::PandasUseOfDotAt,
        (PandasVet, "009") => rules::pandas_vet::rules::PandasUseOfDotIat,
        (PandasVet, "010") => rules::pandas_vet::rules::PandasUseOfDotPivotOrUnstack,
        (PandasVet, "011") => rules::pandas_vet::rules::PandasUseOfDotValues,
        (PandasVet, "012") => rules::pandas_vet::rules::PandasUseOfDotReadTable,
        (PandasVet, "013") => rules::pandas_vet::rules::PandasUseOfDotStack,
        (PandasVet, "015") => rules::pandas_vet::rules::PandasUseOfPdMerge,
        (PandasVet, "101") => rules::pandas_vet::rules::PandasNuniqueConstantSeriesCheck,
        (PandasVet, "901") => rules::pandas_vet::rules::PandasDfVariableName,

        // flake8-errmsg
        (Flake8ErrMsg, "101") => rules::flake8_errmsg::rules::RawStringInException,
        (Flake8ErrMsg, "102") => rules::flake8_errmsg::rules::FStringInException,
        (Flake8ErrMsg, "103") => rules::flake8_errmsg::rules::DotFormatInException,

        // flake8-pyi
        (Flake8Pyi, "001") => rules::flake8_pyi::rules::UnprefixedTypeParam,
        (Flake8Pyi, "002") => rules::flake8_pyi::rules::ComplexIfStatementInStub,
        (Flake8Pyi, "003") => rules::flake8_pyi::rules::UnrecognizedVersionInfoCheck,
        (Flake8Pyi, "004") => rules::flake8_pyi::rules::PatchVersionComparison,
        (Flake8Pyi, "005") => rules::flake8_pyi::rules::WrongTupleLengthVersionComparison,
        (Flake8Pyi, "006") => rules::flake8_pyi::rules::BadVersionInfoComparison,
        (Flake8Pyi, "007") => rules::flake8_pyi::rules::UnrecognizedPlatformCheck,
        (Flake8Pyi, "008") => rules::flake8_pyi::rules::UnrecognizedPlatformName,
        (Flake8Pyi, "009") => rules::flake8_pyi::rules::PassStatementStubBody,
        (Flake8Pyi, "010") => rules::flake8_pyi::rules::NonEmptyStubBody,
        (Flake8Pyi, "011") => rules::flake8_pyi::rules::TypedArgumentDefaultInStub,
        (Flake8Pyi, "012") => rules::flake8_pyi::rules::PassInClassBody,
        (Flake8Pyi, "013") => rules::flake8_pyi::rules::EllipsisInNonEmptyClassBody,
        (Flake8Pyi, "014") => rules::flake8_pyi::rules::ArgumentDefaultInStub,
        (Flake8Pyi, "015") => rules::flake8_pyi::rules::AssignmentDefaultInStub,
        (Flake8Pyi, "016") => rules::flake8_pyi::rules::DuplicateUnionMember,
        (Flake8Pyi, "017") => rules::flake8_pyi::rules::ComplexAssignmentInStub,
        (Flake8Pyi, "018") => rules::flake8_pyi::rules::UnusedPrivateTypeVar,
        (Flake8Pyi, "019") => rules::flake8_pyi::rules::CustomTypeVarForSelf,
        (Flake8Pyi, "020") => rules::flake8_pyi::rules::QuotedAnnotationInStub,
        (Flake8Pyi, "021") => rules::flake8_pyi::rules::DocstringInStub,
        (Flake8Pyi, "024") => rules::flake8_pyi::rules::CollectionsNamedTuple,
        (Flake8Pyi, "025") => rules::flake8_pyi::rules::UnaliasedCollectionsAbcSetImport,
        (Flake8Pyi, "026") => rules::flake8_pyi::rules::TypeAliasWithoutAnnotation,
        (Flake8Pyi, "029") => rules::flake8_pyi::rules::StrOrReprDefinedInStub,
        (Flake8Pyi, "030") => rules::flake8_pyi::rules::UnnecessaryLiteralUnion,
        (Flake8Pyi, "032") => rules::flake8_pyi::rules::AnyEqNeAnnotation,
        (Flake8Pyi, "033") => rules::flake8_pyi::rules::TypeCommentInStub,
        (Flake8Pyi, "034") => rules::flake8_pyi::rules::NonSelfReturnType,
        (Flake8Pyi, "035") => rules::flake8_pyi::rules::UnassignedSpecialVariableInStub,
        (Flake8Pyi, "036") => rules::flake8_pyi::rules::BadExitAnnotation,
        (Flake8Pyi, "041") => rules::flake8_pyi::rules::RedundantNumericUnion,
        (Flake8Pyi, "042") => rules::flake8_pyi::rules::SnakeCaseTypeAlias,
        (Flake8Pyi, "043") => rules::flake8_pyi::rules::TSuffixedTypeAlias,
        (Flake8Pyi, "044") => rules::flake8_pyi::rules::FutureAnnotationsInStub,
        (Flake8Pyi, "045") => rules::flake8_pyi::rules::IterMethodReturnIterable,
        (Flake8Pyi, "046") => rules::flake8_pyi::rules::UnusedPrivateProtocol,
        (Flake8Pyi, "047") => rules::flake8_pyi::rules::UnusedPrivateTypeAlias,
        (Flake8Pyi, "048") => rules::flake8_pyi::rules::StubBodyMultipleStatements,
        (Flake8Pyi, "049") => rules::flake8_pyi::rules::UnusedPrivateTypedDict,
        (Flake8Pyi, "050") => rules::flake8_pyi::rules::NoReturnArgumentAnnotationInStub,
        (Flake8Pyi, "051") => rules::flake8_pyi::rules::RedundantLiteralUnion,
        (Flake8Pyi, "052") => rules::flake8_pyi::rules::UnannotatedAssignmentInStub,
        (Flake8Pyi, "054") => rules::flake8_pyi::rules::NumericLiteralTooLong,
        (Flake8Pyi, "053") => rules::flake8_pyi::rules::StringOrBytesTooLong,
        (Flake8Pyi, "055") => rules::flake8_pyi::rules::UnnecessaryTypeUnion,
        (Flake8Pyi, "056") => rules::flake8_pyi::rules::UnsupportedMethodCallOnAll,
        (Flake8Pyi, "058") => rules::flake8_pyi::rules::GeneratorReturnFromIterMethod,
        (Flake8Pyi, "057") => rules::flake8_pyi::rules::ByteStringUsage,
        (Flake8Pyi, "059") => rules::flake8_pyi::rules::GenericNotLastBaseClass,
        (Flake8Pyi, "061") => rules::flake8_pyi::rules::RedundantNoneLiteral,
        (Flake8Pyi, "062") => rules::flake8_pyi::rules::DuplicateLiteralMember,
        (Flake8Pyi, "063") => rules::flake8_pyi::rules::Pep484StylePositionalOnlyParameter,
        (Flake8Pyi, "064") => rules::flake8_pyi::rules::RedundantFinalLiteral,
        (Flake8Pyi, "066") => rules::flake8_pyi::rules::BadVersionInfoOrder,

        // flake8-pytest-style
        (Flake8PytestStyle, "001") => rules::flake8_pytest_style::rules::PytestFixtureIncorrectParenthesesStyle,
        (Flake8PytestStyle, "002") => rules::flake8_pytest_style::rules::PytestFixturePositionalArgs,
        (Flake8PytestStyle, "003") => rules::flake8_pytest_style::rules::PytestExtraneousScopeFunction,
        #[allow(deprecated)]
        (Flake8PytestStyle, "004") => rules::flake8_pytest_style::rules::PytestMissingFixtureNameUnderscore,
        #[allow(deprecated)]
        (Flake8PytestStyle, "005") => rules::flake8_pytest_style::rules::PytestIncorrectFixtureNameUnderscore,
        (Flake8PytestStyle, "006") => rules::flake8_pytest_style::rules::PytestParametrizeNamesWrongType,
        (Flake8PytestStyle, "007") => rules::flake8_pytest_style::rules::PytestParametrizeValuesWrongType,
        (Flake8PytestStyle, "008") => rules::flake8_pytest_style::rules::PytestPatchWithLambda,
        (Flake8PytestStyle, "009") => rules::flake8_pytest_style::rules::PytestUnittestAssertion,
        (Flake8PytestStyle, "010") => rules::flake8_pytest_style::rules::PytestRaisesWithoutException,
        (Flake8PytestStyle, "011") => rules::flake8_pytest_style::rules::PytestRaisesTooBroad,
        (Flake8PytestStyle, "012") => rules::flake8_pytest_style::rules::PytestRaisesWithMultipleStatements,
        (Flake8PytestStyle, "013") => rules::flake8_pytest_style::rules::PytestIncorrectPytestImport,
        (Flake8PytestStyle, "014") => rules::flake8_pytest_style::rules::PytestDuplicateParametrizeTestCases,
        (Flake8PytestStyle, "015") => rules::flake8_pytest_style::rules::PytestAssertAlwaysFalse,
        (Flake8PytestStyle, "016") => rules::flake8_pytest_style::rules::PytestFailWithoutMessage,
        (Flake8PytestStyle, "017") => rules::flake8_pytest_style::rules::PytestAssertInExcept,
        (Flake8PytestStyle, "018") => rules::flake8_pytest_style::rules::PytestCompositeAssertion,
        (Flake8PytestStyle, "019") => rules::flake8_pytest_style::rules::PytestFixtureParamWithoutValue,
        (Flake8PytestStyle, "020") => rules::flake8_pytest_style::rules::PytestDeprecatedYieldFixture,
        (Flake8PytestStyle, "021") => rules::flake8_pytest_style::rules::PytestFixtureFinalizerCallback,
        (Flake8PytestStyle, "022") => rules::flake8_pytest_style::rules::PytestUselessYieldFixture,
        (Flake8PytestStyle, "023") => rules::flake8_pytest_style::rules::PytestIncorrectMarkParenthesesStyle,
        (Flake8PytestStyle, "024") => rules::flake8_pytest_style::rules::PytestUnnecessaryAsyncioMarkOnFixture,
        (Flake8PytestStyle, "025") => rules::flake8_pytest_style::rules::PytestErroneousUseFixturesOnFixture,
        (Flake8PytestStyle, "026") => rules::flake8_pytest_style::rules::PytestUseFixturesWithoutParameters,
        (Flake8PytestStyle, "027") => rules::flake8_pytest_style::rules::PytestUnittestRaisesAssertion,
        (Flake8PytestStyle, "028") => rules::flake8_pytest_style::rules::PytestParameterWithDefaultArgument,
        (Flake8PytestStyle, "029") => rules::flake8_pytest_style::rules::PytestWarnsWithoutWarning,
        (Flake8PytestStyle, "030") => rules::flake8_pytest_style::rules::PytestWarnsTooBroad,
        (Flake8PytestStyle, "031") => rules::flake8_pytest_style::rules::PytestWarnsWithMultipleStatements,

        // flake8-pie
        (Flake8Pie, "790") => rules::flake8_pie::rules::UnnecessaryPlaceholder,
        (Flake8Pie, "794") => rules::flake8_pie::rules::DuplicateClassFieldDefinition,
        (Flake8Pie, "796") => rules::flake8_pie::rules::NonUniqueEnums,
        (Flake8Pie, "800") => rules::flake8_pie::rules::UnnecessarySpread,
        (Flake8Pie, "804") => rules::flake8_pie::rules::UnnecessaryDictKwargs,
        (Flake8Pie, "807") => rules::flake8_pie::rules::ReimplementedContainerBuiltin,
        (Flake8Pie, "808") => rules::flake8_pie::rules::UnnecessaryRangeStart,
        (Flake8Pie, "810") => rules::flake8_pie::rules::MultipleStartsEndsWith,

        // flake8-commas
        (Flake8Commas, "812") => rules::flake8_commas::rules::MissingTrailingComma,
        (Flake8Commas, "818") => rules::flake8_commas::rules::TrailingCommaOnBareTuple,
        (Flake8Commas, "819") => rules::flake8_commas::rules::ProhibitedTrailingComma,

        // flake8-no-pep420
        (Flake8NoPep420, "001") => rules::flake8_no_pep420::rules::ImplicitNamespacePackage,

        // flake8-executable
        (Flake8Executable, "001") => rules::flake8_executable::rules::ShebangNotExecutable,
        (Flake8Executable, "002") => rules::flake8_executable::rules::ShebangMissingExecutableFile,
        (Flake8Executable, "003") => rules::flake8_executable::rules::ShebangMissingPython,
        (Flake8Executable, "004") => rules::flake8_executable::rules::ShebangLeadingWhitespace,
        (Flake8Executable, "005") => rules::flake8_executable::rules::ShebangNotFirstLine,

        // flake8-type-checking
        (Flake8TypeChecking, "001") => rules::flake8_type_checking::rules::TypingOnlyFirstPartyImport,
        (Flake8TypeChecking, "002") => rules::flake8_type_checking::rules::TypingOnlyThirdPartyImport,
        (Flake8TypeChecking, "003") => rules::flake8_type_checking::rules::TypingOnlyStandardLibraryImport,
        (Flake8TypeChecking, "004") => rules::flake8_type_checking::rules::RuntimeImportInTypeCheckingBlock,
        (Flake8TypeChecking, "005") => rules::flake8_type_checking::rules::EmptyTypeCheckingBlock,
        (Flake8TypeChecking, "006") => rules::flake8_type_checking::rules::RuntimeCastValue,
        (Flake8TypeChecking, "007") => rules::flake8_type_checking::rules::UnquotedTypeAlias,
        (Flake8TypeChecking, "008") => rules::flake8_type_checking::rules::QuotedTypeAlias,
        (Flake8TypeChecking, "010") => rules::flake8_type_checking::rules::RuntimeStringUnion,

        // tryceratops
        (Tryceratops, "002") => rules::tryceratops::rules::RaiseVanillaClass,
        (Tryceratops, "003") => rules::tryceratops::rules::RaiseVanillaArgs,
        (Tryceratops, "004") => rules::tryceratops::rules::TypeCheckWithoutTypeError,
        (Tryceratops, "200") => rules::tryceratops::rules::ReraiseNoCause,
        (Tryceratops, "201") => rules::tryceratops::rules::VerboseRaise,
        (Tryceratops, "203") => rules::tryceratops::rules::UselessTryExcept,
        (Tryceratops, "300") => rules::tryceratops::rules::TryConsiderElse,
        (Tryceratops, "301") => rules::tryceratops::rules::RaiseWithinTry,
        (Tryceratops, "400") => rules::tryceratops::rules::ErrorInsteadOfException,
        (Tryceratops, "401") => rules::tryceratops::rules::VerboseLogMessage,

        // flake8-use-pathlib
        (Flake8UsePathlib, "100") => rules::flake8_use_pathlib::rules::OsPathAbspath,
        (Flake8UsePathlib, "101") => rules::flake8_use_pathlib::rules::OsChmod,
        (Flake8UsePathlib, "102") => rules::flake8_use_pathlib::rules::OsMkdir,
        (Flake8UsePathlib, "103") => rules::flake8_use_pathlib::rules::OsMakedirs,
        (Flake8UsePathlib, "104") => rules::flake8_use_pathlib::rules::OsRename,
        (Flake8UsePathlib, "105") => rules::flake8_use_pathlib::rules::OsReplace,
        (Flake8UsePathlib, "106") => rules::flake8_use_pathlib::rules::OsRmdir,
        (Flake8UsePathlib, "107") => rules::flake8_use_pathlib::rules::OsRemove,
        (Flake8UsePathlib, "108") => rules::flake8_use_pathlib::rules::OsUnlink,
        (Flake8UsePathlib, "109") => rules::flake8_use_pathlib::rules::OsGetcwd,
        (Flake8UsePathlib, "110") => rules::flake8_use_pathlib::rules::OsPathExists,
        (Flake8UsePathlib, "111") => rules::flake8_use_pathlib::rules::OsPathExpanduser,
        (Flake8UsePathlib, "112") => rules::flake8_use_pathlib::rules::OsPathIsdir,
        (Flake8UsePathlib, "113") => rules::flake8_use_pathlib::rules::OsPathIsfile,
        (Flake8UsePathlib, "114") => rules::flake8_use_pathlib::rules::OsPathIslink,
        (Flake8UsePathlib, "115") => rules::flake8_use_pathlib::rules::OsReadlink,
        (Flake8UsePathlib, "116") => rules::flake8_use_pathlib::violations::OsStat,
        (Flake8UsePathlib, "117") => rules::flake8_use_pathlib::rules::OsPathIsabs,
        (Flake8UsePathlib, "118") => rules::flake8_use_pathlib::violations::OsPathJoin,
        (Flake8UsePathlib, "119") => rules::flake8_use_pathlib::rules::OsPathBasename,
        (Flake8UsePathlib, "120") => rules::flake8_use_pathlib::rules::OsPathDirname,
        (Flake8UsePathlib, "121") => rules::flake8_use_pathlib::rules::OsPathSamefile,
        (Flake8UsePathlib, "122") => rules::flake8_use_pathlib::violations::OsPathSplitext,
        (Flake8UsePathlib, "123") => rules::flake8_use_pathlib::rules::BuiltinOpen,
        (Flake8UsePathlib, "124") => rules::flake8_use_pathlib::violations::PyPath,
        (Flake8UsePathlib, "201") => rules::flake8_use_pathlib::rules::PathConstructorCurrentDirectory,
        (Flake8UsePathlib, "202") => rules::flake8_use_pathlib::rules::OsPathGetsize,
        (Flake8UsePathlib, "202") => rules::flake8_use_pathlib::rules::OsPathGetsize,
        (Flake8UsePathlib, "203") => rules::flake8_use_pathlib::rules::OsPathGetatime,
        (Flake8UsePathlib, "204") => rules::flake8_use_pathlib::rules::OsPathGetmtime,
        (Flake8UsePathlib, "205") => rules::flake8_use_pathlib::rules::OsPathGetctime,
        (Flake8UsePathlib, "206") => rules::flake8_use_pathlib::rules::OsSepSplit,
        (Flake8UsePathlib, "207") => rules::flake8_use_pathlib::rules::Glob,
        (Flake8UsePathlib, "208") => rules::flake8_use_pathlib::violations::OsListdir,
        (Flake8UsePathlib, "210") => rules::flake8_use_pathlib::rules::InvalidPathlibWithSuffix,
        (Flake8UsePathlib, "211") => rules::flake8_use_pathlib::rules::OsSymlink,

        // flake8-logging-format
        (Flake8LoggingFormat, "001") => rules::flake8_logging_format::violations::LoggingStringFormat,
        (Flake8LoggingFormat, "002") => rules::flake8_logging_format::violations::LoggingPercentFormat,
        (Flake8LoggingFormat, "003") => rules::flake8_logging_format::violations::LoggingStringConcat,
        (Flake8LoggingFormat, "004") => rules::flake8_logging_format::violations::LoggingFString,
        (Flake8LoggingFormat, "010") => rules::flake8_logging_format::violations::LoggingWarn,
        (Flake8LoggingFormat, "101") => rules::flake8_logging_format::violations::LoggingExtraAttrClash,
        (Flake8LoggingFormat, "201") => rules::flake8_logging_format::violations::LoggingExcInfo,
        (Flake8LoggingFormat, "202") => rules::flake8_logging_format::violations::LoggingRedundantExcInfo,

        // flake8-raise
        (Flake8Raise, "102") => rules::flake8_raise::rules::UnnecessaryParenOnRaiseException,

        // flake8-self
        (Flake8Self, "001") => rules::flake8_self::rules::PrivateMemberAccess,

        // numpy
        (Numpy, "001") => rules::numpy::rules::NumpyDeprecatedTypeAlias,
        (Numpy, "002") => rules::numpy::rules::NumpyLegacyRandom,
        (Numpy, "003") => rules::numpy::rules::NumpyDeprecatedFunction,
        (Numpy, "201") => rules::numpy::rules::Numpy2Deprecation,

        // fastapi
        (FastApi, "001") => rules::fastapi::rules::FastApiRedundantResponseModel,
        (FastApi, "002") => rules::fastapi::rules::FastApiNonAnnotatedDependency,
        (FastApi, "003") => rules::fastapi::rules::FastApiUnusedPathParameter,

        // pydoclint
        (Pydoclint, "102") => rules::pydoclint::rules::DocstringExtraneousParameter,
        (Pydoclint, "201") => rules::pydoclint::rules::DocstringMissingReturns,
        (Pydoclint, "202") => rules::pydoclint::rules::DocstringExtraneousReturns,
        (Pydoclint, "402") => rules::pydoclint::rules::DocstringMissingYields,
        (Pydoclint, "403") => rules::pydoclint::rules::DocstringExtraneousYields,
        (Pydoclint, "501") => rules::pydoclint::rules::DocstringMissingException,
        (Pydoclint, "502") => rules::pydoclint::rules::DocstringExtraneousException,

        // ruff
        (Ruff, "001") => rules::ruff::rules::AmbiguousUnicodeCharacterString,
        (Ruff, "002") => rules::ruff::rules::AmbiguousUnicodeCharacterDocstring,
        (Ruff, "003") => rules::ruff::rules::AmbiguousUnicodeCharacterComment,
        (Ruff, "005") => rules::ruff::rules::CollectionLiteralConcatenation,
        (Ruff, "006") => rules::ruff::rules::AsyncioDanglingTask,
        (Ruff, "007") => rules::ruff::rules::ZipInsteadOfPairwise,
        (Ruff, "008") => rules::ruff::rules::MutableDataclassDefault,
        (Ruff, "009") => rules::ruff::rules::FunctionCallInDataclassDefaultArgument,
        (Ruff, "010") => rules::ruff::rules::ExplicitFStringTypeConversion,
        (Ruff, "011") => rules::ruff::rules::RuffStaticKeyDictComprehension,
        (Ruff, "012") => rules::ruff::rules::MutableClassDefault,
        (Ruff, "013") => rules::ruff::rules::ImplicitOptional,
        (Ruff, "015") => rules::ruff::rules::UnnecessaryIterableAllocationForFirstElement,
        (Ruff, "016") => rules::ruff::rules::InvalidIndexType,
        (Ruff, "017") => rules::ruff::rules::QuadraticListSummation,
        (Ruff, "018") => rules::ruff::rules::AssignmentInAssert,
        (Ruff, "019") => rules::ruff::rules::UnnecessaryKeyCheck,
        (Ruff, "020") => rules::ruff::rules::NeverUnion,
        (Ruff, "021") => rules::ruff::rules::ParenthesizeChainedOperators,
        (Ruff, "022") => rules::ruff::rules::UnsortedDunderAll,
        (Ruff, "023") => rules::ruff::rules::UnsortedDunderSlots,
        (Ruff, "024") => rules::ruff::rules::MutableFromkeysValue,
        (Ruff, "026") => rules::ruff::rules::DefaultFactoryKwarg,
        (Ruff, "027") => rules::ruff::rules::MissingFStringSyntax,
        (Ruff, "028") => rules::ruff::rules::InvalidFormatterSuppressionComment,
        (Ruff, "029") => rules::ruff::rules::UnusedAsync,
        (Ruff, "030") => rules::ruff::rules::AssertWithPrintMessage,
        (Ruff, "031") => rules::ruff::rules::IncorrectlyParenthesizedTupleInSubscript,
        (Ruff, "032") => rules::ruff::rules::DecimalFromFloatLiteral,
        (Ruff, "033") => rules::ruff::rules::PostInitDefault,
        (Ruff, "034") => rules::ruff::rules::UselessIfElse,
        (Ruff, "035") => rules::ruff::rules::RuffUnsafeMarkupUse,
        (Ruff, "036") => rules::ruff::rules::NoneNotAtEndOfUnion,
        (Ruff, "037") => rules::ruff::rules::UnnecessaryEmptyIterableWithinDequeCall,
        (Ruff, "038") => rules::ruff::rules::RedundantBoolLiteral,
        (Ruff, "039") => rules::ruff::rules::UnrawRePattern,
        (Ruff, "040") => rules::ruff::rules::InvalidAssertMessageLiteralArgument,
        (Ruff, "041") => rules::ruff::rules::UnnecessaryNestedLiteral,
        (Ruff, "043") => rules::ruff::rules::PytestRaisesAmbiguousPattern,
        (Ruff, "045") => rules::ruff::rules::ImplicitClassVarInDataclass,
        (Ruff, "046") => rules::ruff::rules::UnnecessaryCastToInt,
        (Ruff, "047") => rules::ruff::rules::NeedlessElse,
        (Ruff, "048") => rules::ruff::rules::MapIntVersionParsing,
        (Ruff, "049") => rules::ruff::rules::DataclassEnum,
        (Ruff, "051") => rules::ruff::rules::IfKeyInDictDel,
        (Ruff, "052") => rules::ruff::rules::UsedDummyVariable,
        (Ruff, "053") => rules::ruff::rules::ClassWithMixedTypeVars,
        (Ruff, "054") => rules::ruff::rules::IndentedFormFeed,
        (Ruff, "055") => rules::ruff::rules::UnnecessaryRegularExpression,
        (Ruff, "056") => rules::ruff::rules::FalsyDictGetFallback,
        (Ruff, "057") => rules::ruff::rules::UnnecessaryRound,
        (Ruff, "058") => rules::ruff::rules::StarmapZip,
        (Ruff, "059") => rules::ruff::rules::UnusedUnpackedVariable,
        (Ruff, "060") => rules::ruff::rules::InEmptyCollection,
        (Ruff, "061") => rules::ruff::rules::LegacyFormPytestRaises,
        (Ruff, "063") => rules::ruff::rules::AccessAnnotationsFromClassDict,
        (Ruff, "064") => rules::ruff::rules::NonOctalPermissions,
        (Ruff, "065") => rules::ruff::rules::LoggingEagerConversion,
        (Ruff, "066") => rules::ruff::rules::PropertyWithoutReturn,

        (Ruff, "100") => rules::ruff::rules::UnusedNOQA,
        (Ruff, "101") => rules::ruff::rules::RedirectedNOQA,
        (Ruff, "102") => rules::ruff::rules::InvalidRuleCode,

        (Ruff, "200") => rules::ruff::rules::InvalidPyprojectToml,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "900") => rules::ruff::rules::StableTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "901") => rules::ruff::rules::StableTestRuleSafeFix,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "902") => rules::ruff::rules::StableTestRuleUnsafeFix,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "903") => rules::ruff::rules::StableTestRuleDisplayOnlyFix,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "911") => rules::ruff::rules::PreviewTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "920") => rules::ruff::rules::DeprecatedTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "921") => rules::ruff::rules::AnotherDeprecatedTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "930") => rules::ruff::rules::RemovedTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "931") => rules::ruff::rules::AnotherRemovedTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "940") => rules::ruff::rules::RedirectedFromTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "950") => rules::ruff::rules::RedirectedToTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "960") => rules::ruff::rules::RedirectedFromPrefixTestRule,
        #[cfg(any(feature = "test-rules", test))]
        (Ruff, "990") => rules::ruff::rules::PanicyTestRule,


        // flake8-django
        (Flake8Django, "001") => rules::flake8_django::rules::DjangoNullableModelStringField,
        (Flake8Django, "003") => rules::flake8_django::rules::DjangoLocalsInRenderFunction,
        (Flake8Django, "006") => rules::flake8_django::rules::DjangoExcludeWithModelForm,
        (Flake8Django, "007") => rules::flake8_django::rules::DjangoAllWithModelForm,
        (Flake8Django, "008") => rules::flake8_django::rules::DjangoModelWithoutDunderStr,
        (Flake8Django, "012") => rules::flake8_django::rules::DjangoUnorderedBodyContentInModel,
        (Flake8Django, "013") => rules::flake8_django::rules::DjangoNonLeadingReceiverDecorator,

        // flynt
        // Reserved: (Flynt, "001") => Rule: :StringConcatenationToFString,
        (Flynt, "002") => rules::flynt::rules::StaticJoinToFString,

        // flake8-todos
        (Flake8Todos, "001") => rules::flake8_todos::rules::InvalidTodoTag,
        (Flake8Todos, "002") => rules::flake8_todos::rules::MissingTodoAuthor,
        (Flake8Todos, "003") => rules::flake8_todos::rules::MissingTodoLink,
        (Flake8Todos, "004") => rules::flake8_todos::rules::MissingTodoColon,
        (Flake8Todos, "005") => rules::flake8_todos::rules::MissingTodoDescription,
        (Flake8Todos, "006") => rules::flake8_todos::rules::InvalidTodoCapitalization,
        (Flake8Todos, "007") => rules::flake8_todos::rules::MissingSpaceAfterTodoColon,

        // airflow
        (Airflow, "001") => rules::airflow::rules::AirflowVariableNameTaskIdMismatch,
        (Airflow, "002") => rules::airflow::rules::AirflowDagNoScheduleArgument,
        (Airflow, "301") => rules::airflow::rules::Airflow3Removal,
        (Airflow, "302") => rules::airflow::rules::Airflow3MovedToProvider,
        (Airflow, "311") => rules::airflow::rules::Airflow3SuggestedUpdate,
        (Airflow, "312") => rules::airflow::rules::Airflow3SuggestedToMoveToProvider,

        // perflint
        (Perflint, "101") => rules::perflint::rules::UnnecessaryListCast,
        (Perflint, "102") => rules::perflint::rules::IncorrectDictIterator,
        (Perflint, "203") => rules::perflint::rules::TryExceptInLoop,
        (Perflint, "401") => rules::perflint::rules::ManualListComprehension,
        (Perflint, "402") => rules::perflint::rules::ManualListCopy,
        (Perflint, "403") => rules::perflint::rules::ManualDictComprehension,

        // flake8-fixme
        (Flake8Fixme, "001") => rules::flake8_fixme::rules::LineContainsFixme,
        (Flake8Fixme, "002") => rules::flake8_fixme::rules::LineContainsTodo,
        (Flake8Fixme, "003") => rules::flake8_fixme::rules::LineContainsXxx,
        (Flake8Fixme, "004") => rules::flake8_fixme::rules::LineContainsHack,

        // flake8-slots
        (Flake8Slots, "000") => rules::flake8_slots::rules::NoSlotsInStrSubclass,
        (Flake8Slots, "001") => rules::flake8_slots::rules::NoSlotsInTupleSubclass,
        (Flake8Slots, "002") => rules::flake8_slots::rules::NoSlotsInNamedtupleSubclass,

        // refurb
        (Refurb, "101") => rules::refurb::rules::ReadWholeFile,
        (Refurb, "103") => rules::refurb::rules::WriteWholeFile,
        (Refurb, "105") => rules::refurb::rules::PrintEmptyString,
        (Refurb, "110") => rules::refurb::rules::IfExpInsteadOfOrOperator,
        (Refurb, "113") => rules::refurb::rules::RepeatedAppend,
        (Refurb, "116") => rules::refurb::rules::FStringNumberFormat,
        (Refurb, "118") => rules::refurb::rules::ReimplementedOperator,
        (Refurb, "122") => rules::refurb::rules::ForLoopWrites,
        (Refurb, "129") => rules::refurb::rules::ReadlinesInFor,
        (Refurb, "131") => rules::refurb::rules::DeleteFullSlice,
        (Refurb, "132") => rules::refurb::rules::CheckAndRemoveFromSet,
        (Refurb, "136") => rules::refurb::rules::IfExprMinMax,
        (Refurb, "140") => rules::refurb::rules::ReimplementedStarmap,
        (Refurb, "142") => rules::refurb::rules::ForLoopSetMutations,
        (Refurb, "145") => rules::refurb::rules::SliceCopy,
        (Refurb, "148") => rules::refurb::rules::UnnecessaryEnumerate,
        (Refurb, "152") => rules::refurb::rules::MathConstant,
        (Refurb, "154") => rules::refurb::rules::RepeatedGlobal,
        (Refurb, "156") => rules::refurb::rules::HardcodedStringCharset,
        (Refurb, "157") => rules::refurb::rules::VerboseDecimalConstructor,
        (Refurb, "161") => rules::refurb::rules::BitCount,
        (Refurb, "162") => rules::refurb::rules::FromisoformatReplaceZ,
        (Refurb, "163") => rules::refurb::rules::RedundantLogBase,
        (Refurb, "164") => rules::refurb::rules::UnnecessaryFromFloat,
        (Refurb, "166") => rules::refurb::rules::IntOnSlicedStr,
        (Refurb, "167") => rules::refurb::rules::RegexFlagAlias,
        (Refurb, "168") => rules::refurb::rules::IsinstanceTypeNone,
        (Refurb, "169") => rules::refurb::rules::TypeNoneComparison,
        (Refurb, "171") => rules::refurb::rules::SingleItemMembershipTest,
        (Refurb, "177") => rules::refurb::rules::ImplicitCwd,
        (Refurb, "180") => rules::refurb::rules::MetaClassABCMeta,
        (Refurb, "181") => rules::refurb::rules::HashlibDigestHex,
        (Refurb, "187") => rules::refurb::rules::ListReverseCopy,
        (Refurb, "188") => rules::refurb::rules::SliceToRemovePrefixOrSuffix,
        (Refurb, "189") => rules::refurb::rules::SubclassBuiltin,
        (Refurb, "192") => rules::refurb::rules::SortedMinMax,

        // flake8-logging
        (Flake8Logging, "001") => rules::flake8_logging::rules::DirectLoggerInstantiation,
        (Flake8Logging, "002") => rules::flake8_logging::rules::InvalidGetLoggerArgument,
        (Flake8Logging, "004") => rules::flake8_logging::rules::LogExceptionOutsideExceptHandler,
        (Flake8Logging, "007") => rules::flake8_logging::rules::ExceptionWithoutExcInfo,
        (Flake8Logging, "009") => rules::flake8_logging::rules::UndocumentedWarn,
        (Flake8Logging, "014") => rules::flake8_logging::rules::ExcInfoOutsideExceptHandler,
        (Flake8Logging, "015") => rules::flake8_logging::rules::RootLoggerCall,

        _ => return None,
    })
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(self.into())
    }
}
