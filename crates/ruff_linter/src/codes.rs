/// In this module we generate [`Rule`], an enum of all rules, and [`RuleCodePrefix`], an enum of
/// all rules categories. A rule category is something like pyflakes or flake8-todos. Each rule
/// category contains all rules and their common prefixes, i.e. everything you can specify in
/// `--select`. For pylint this is e.g. C0414 and E0118 but also C and E01.
use std::fmt::Formatter;

use crate::registry::{AsRule, Linter};
use crate::rule_selector::is_single_rule_selector;
use crate::rules;

use strum_macros::{AsRefStr, EnumIter};

#[derive(PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Debug, Copy, Clone)]
pub enum RuleGroup {
    /// The rule is stable.
    Stable,
    /// The rule is unstable, and preview mode must be enabled for usage.
    Preview,
    /// Legacy category for unstable rules, supports backwards compatible selection.
    #[deprecated(note = "Use `RuleGroup::Preview` for new rules instead")]
    Nursery,
}

#[ruff_macros::map_codes]
pub fn code_to_rule(linter: Linter, code: &str) -> Option<(RuleGroup, Rule)> {
    #[allow(clippy::enum_glob_use)]
    use Linter::*;

    #[rustfmt::skip]
    Some(match (linter, code) {
        // pycodestyle errors
        (Pycodestyle, "E101") => (RuleGroup::Stable, rules::pycodestyle::rules::MixedSpacesAndTabs),
        #[allow(deprecated)]
        (Pycodestyle, "E111") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::IndentationWithInvalidMultiple),
        #[allow(deprecated)]
        (Pycodestyle, "E112") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::NoIndentedBlock),
        #[allow(deprecated)]
        (Pycodestyle, "E113") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::UnexpectedIndentation),
        #[allow(deprecated)]
        (Pycodestyle, "E114") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::IndentationWithInvalidMultipleComment),
        #[allow(deprecated)]
        (Pycodestyle, "E115") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::NoIndentedBlockComment),
        #[allow(deprecated)]
        (Pycodestyle, "E116") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::UnexpectedIndentationComment),
        #[allow(deprecated)]
        (Pycodestyle, "E117") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::OverIndented),
        #[allow(deprecated)]
        (Pycodestyle, "E201") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::WhitespaceAfterOpenBracket),
        #[allow(deprecated)]
        (Pycodestyle, "E202") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::WhitespaceBeforeCloseBracket),
        #[allow(deprecated)]
        (Pycodestyle, "E203") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::WhitespaceBeforePunctuation),
        #[allow(deprecated)]
        (Pycodestyle, "E211") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::WhitespaceBeforeParameters),
        #[allow(deprecated)]
        (Pycodestyle, "E221") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MultipleSpacesBeforeOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E222") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E223") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::TabBeforeOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E224") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::TabAfterOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E225") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E226") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundArithmeticOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E227") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundBitwiseOrShiftOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E228") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundModuloOperator),
        #[allow(deprecated)]
        (Pycodestyle, "E231") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespace),
        #[allow(deprecated)]
        (Pycodestyle, "E241") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterComma),
        #[allow(deprecated)]
        (Pycodestyle, "E242") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::TabAfterComma),
        #[allow(deprecated)]
        (Pycodestyle, "E251") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::UnexpectedSpacesAroundKeywordParameterEquals),
        #[allow(deprecated)]
        (Pycodestyle, "E252") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespaceAroundParameterEquals),
        #[allow(deprecated)]
        (Pycodestyle, "E261") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::TooFewSpacesBeforeInlineComment),
        #[allow(deprecated)]
        (Pycodestyle, "E262") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::NoSpaceAfterInlineComment),
        #[allow(deprecated)]
        (Pycodestyle, "E265") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::NoSpaceAfterBlockComment),
        #[allow(deprecated)]
        (Pycodestyle, "E266") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MultipleLeadingHashesForBlockComment),
        #[allow(deprecated)]
        (Pycodestyle, "E271") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MultipleSpacesAfterKeyword),
        #[allow(deprecated)]
        (Pycodestyle, "E272") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MultipleSpacesBeforeKeyword),
        #[allow(deprecated)]
        (Pycodestyle, "E273") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::TabAfterKeyword),
        #[allow(deprecated)]
        (Pycodestyle, "E274") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::TabBeforeKeyword),
        #[allow(deprecated)]
        (Pycodestyle, "E275") => (RuleGroup::Nursery, rules::pycodestyle::rules::logical_lines::MissingWhitespaceAfterKeyword),
        (Pycodestyle, "E401") => (RuleGroup::Stable, rules::pycodestyle::rules::MultipleImportsOnOneLine),
        (Pycodestyle, "E402") => (RuleGroup::Stable, rules::pycodestyle::rules::ModuleImportNotAtTopOfFile),
        (Pycodestyle, "E501") => (RuleGroup::Stable, rules::pycodestyle::rules::LineTooLong),
        (Pycodestyle, "E701") => (RuleGroup::Stable, rules::pycodestyle::rules::MultipleStatementsOnOneLineColon),
        (Pycodestyle, "E702") => (RuleGroup::Stable, rules::pycodestyle::rules::MultipleStatementsOnOneLineSemicolon),
        (Pycodestyle, "E703") => (RuleGroup::Stable, rules::pycodestyle::rules::UselessSemicolon),
        (Pycodestyle, "E711") => (RuleGroup::Stable, rules::pycodestyle::rules::NoneComparison),
        (Pycodestyle, "E712") => (RuleGroup::Stable, rules::pycodestyle::rules::TrueFalseComparison),
        (Pycodestyle, "E713") => (RuleGroup::Stable, rules::pycodestyle::rules::NotInTest),
        (Pycodestyle, "E714") => (RuleGroup::Stable, rules::pycodestyle::rules::NotIsTest),
        (Pycodestyle, "E721") => (RuleGroup::Stable, rules::pycodestyle::rules::TypeComparison),
        (Pycodestyle, "E722") => (RuleGroup::Stable, rules::pycodestyle::rules::BareExcept),
        (Pycodestyle, "E731") => (RuleGroup::Stable, rules::pycodestyle::rules::LambdaAssignment),
        (Pycodestyle, "E741") => (RuleGroup::Stable, rules::pycodestyle::rules::AmbiguousVariableName),
        (Pycodestyle, "E742") => (RuleGroup::Stable, rules::pycodestyle::rules::AmbiguousClassName),
        (Pycodestyle, "E743") => (RuleGroup::Stable, rules::pycodestyle::rules::AmbiguousFunctionName),
        (Pycodestyle, "E902") => (RuleGroup::Stable, rules::pycodestyle::rules::IOError),
        (Pycodestyle, "E999") => (RuleGroup::Stable, rules::pycodestyle::rules::SyntaxError),

        // pycodestyle warnings
        (Pycodestyle, "W191") => (RuleGroup::Stable, rules::pycodestyle::rules::TabIndentation),
        (Pycodestyle, "W291") => (RuleGroup::Stable, rules::pycodestyle::rules::TrailingWhitespace),
        (Pycodestyle, "W292") => (RuleGroup::Stable, rules::pycodestyle::rules::MissingNewlineAtEndOfFile),
        (Pycodestyle, "W293") => (RuleGroup::Stable, rules::pycodestyle::rules::BlankLineWithWhitespace),
        (Pycodestyle, "W505") => (RuleGroup::Stable, rules::pycodestyle::rules::DocLineTooLong),
        (Pycodestyle, "W605") => (RuleGroup::Stable, rules::pycodestyle::rules::InvalidEscapeSequence),

        // pyflakes
        (Pyflakes, "401") => (RuleGroup::Stable, rules::pyflakes::rules::UnusedImport),
        (Pyflakes, "402") => (RuleGroup::Stable, rules::pyflakes::rules::ImportShadowedByLoopVar),
        (Pyflakes, "403") => (RuleGroup::Stable, rules::pyflakes::rules::UndefinedLocalWithImportStar),
        (Pyflakes, "404") => (RuleGroup::Stable, rules::pyflakes::rules::LateFutureImport),
        (Pyflakes, "405") => (RuleGroup::Stable, rules::pyflakes::rules::UndefinedLocalWithImportStarUsage),
        (Pyflakes, "406") => (RuleGroup::Stable, rules::pyflakes::rules::UndefinedLocalWithNestedImportStarUsage),
        (Pyflakes, "407") => (RuleGroup::Stable, rules::pyflakes::rules::FutureFeatureNotDefined),
        (Pyflakes, "501") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatInvalidFormat),
        (Pyflakes, "502") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatExpectedMapping),
        (Pyflakes, "503") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatExpectedSequence),
        (Pyflakes, "504") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatExtraNamedArguments),
        (Pyflakes, "505") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatMissingArgument),
        (Pyflakes, "506") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatMixedPositionalAndNamed),
        (Pyflakes, "507") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatPositionalCountMismatch),
        (Pyflakes, "508") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatStarRequiresSequence),
        (Pyflakes, "509") => (RuleGroup::Stable, rules::pyflakes::rules::PercentFormatUnsupportedFormatCharacter),
        (Pyflakes, "521") => (RuleGroup::Stable, rules::pyflakes::rules::StringDotFormatInvalidFormat),
        (Pyflakes, "522") => (RuleGroup::Stable, rules::pyflakes::rules::StringDotFormatExtraNamedArguments),
        (Pyflakes, "523") => (RuleGroup::Stable, rules::pyflakes::rules::StringDotFormatExtraPositionalArguments),
        (Pyflakes, "524") => (RuleGroup::Stable, rules::pyflakes::rules::StringDotFormatMissingArguments),
        (Pyflakes, "525") => (RuleGroup::Stable, rules::pyflakes::rules::StringDotFormatMixingAutomatic),
        (Pyflakes, "541") => (RuleGroup::Stable, rules::pyflakes::rules::FStringMissingPlaceholders),
        (Pyflakes, "601") => (RuleGroup::Stable, rules::pyflakes::rules::MultiValueRepeatedKeyLiteral),
        (Pyflakes, "602") => (RuleGroup::Stable, rules::pyflakes::rules::MultiValueRepeatedKeyVariable),
        (Pyflakes, "621") => (RuleGroup::Stable, rules::pyflakes::rules::ExpressionsInStarAssignment),
        (Pyflakes, "622") => (RuleGroup::Stable, rules::pyflakes::rules::MultipleStarredExpressions),
        (Pyflakes, "631") => (RuleGroup::Stable, rules::pyflakes::rules::AssertTuple),
        (Pyflakes, "632") => (RuleGroup::Stable, rules::pyflakes::rules::IsLiteral),
        (Pyflakes, "633") => (RuleGroup::Stable, rules::pyflakes::rules::InvalidPrintSyntax),
        (Pyflakes, "634") => (RuleGroup::Stable, rules::pyflakes::rules::IfTuple),
        (Pyflakes, "701") => (RuleGroup::Stable, rules::pyflakes::rules::BreakOutsideLoop),
        (Pyflakes, "702") => (RuleGroup::Stable, rules::pyflakes::rules::ContinueOutsideLoop),
        (Pyflakes, "704") => (RuleGroup::Stable, rules::pyflakes::rules::YieldOutsideFunction),
        (Pyflakes, "706") => (RuleGroup::Stable, rules::pyflakes::rules::ReturnOutsideFunction),
        (Pyflakes, "707") => (RuleGroup::Stable, rules::pyflakes::rules::DefaultExceptNotLast),
        (Pyflakes, "722") => (RuleGroup::Stable, rules::pyflakes::rules::ForwardAnnotationSyntaxError),
        (Pyflakes, "811") => (RuleGroup::Stable, rules::pyflakes::rules::RedefinedWhileUnused),
        (Pyflakes, "821") => (RuleGroup::Stable, rules::pyflakes::rules::UndefinedName),
        (Pyflakes, "822") => (RuleGroup::Stable, rules::pyflakes::rules::UndefinedExport),
        (Pyflakes, "823") => (RuleGroup::Stable, rules::pyflakes::rules::UndefinedLocal),
        (Pyflakes, "841") => (RuleGroup::Stable, rules::pyflakes::rules::UnusedVariable),
        (Pyflakes, "842") => (RuleGroup::Stable, rules::pyflakes::rules::UnusedAnnotation),
        (Pyflakes, "901") => (RuleGroup::Stable, rules::pyflakes::rules::RaiseNotImplemented),

        // pylint
        (Pylint, "C0105") => (RuleGroup::Stable, rules::pylint::rules::TypeNameIncorrectVariance),
        (Pylint, "C0131") => (RuleGroup::Stable, rules::pylint::rules::TypeBivariance),
        (Pylint, "C0132") => (RuleGroup::Stable, rules::pylint::rules::TypeParamNameMismatch),
        (Pylint, "C0205") => (RuleGroup::Stable, rules::pylint::rules::SingleStringSlots),
        (Pylint, "C0208") => (RuleGroup::Stable, rules::pylint::rules::IterationOverSet),
        (Pylint, "C0414") => (RuleGroup::Stable, rules::pylint::rules::UselessImportAlias),
        #[allow(deprecated)]
        (Pylint, "C1901") => (RuleGroup::Nursery, rules::pylint::rules::CompareToEmptyString),
        (Pylint, "C3002") => (RuleGroup::Stable, rules::pylint::rules::UnnecessaryDirectLambdaCall),
        (Pylint, "E0100") => (RuleGroup::Stable, rules::pylint::rules::YieldInInit),
        (Pylint, "E0101") => (RuleGroup::Stable, rules::pylint::rules::ReturnInInit),
        (Pylint, "E0116") => (RuleGroup::Stable, rules::pylint::rules::ContinueInFinally),
        (Pylint, "E0117") => (RuleGroup::Stable, rules::pylint::rules::NonlocalWithoutBinding),
        (Pylint, "E0118") => (RuleGroup::Stable, rules::pylint::rules::LoadBeforeGlobalDeclaration),
        (Pylint, "E0241") => (RuleGroup::Stable, rules::pylint::rules::DuplicateBases),
        (Pylint, "E0302") => (RuleGroup::Stable, rules::pylint::rules::UnexpectedSpecialMethodSignature),
        (Pylint, "E0307") => (RuleGroup::Stable, rules::pylint::rules::InvalidStrReturnType),
        (Pylint, "E0604") => (RuleGroup::Stable, rules::pylint::rules::InvalidAllObject),
        (Pylint, "E0605") => (RuleGroup::Stable, rules::pylint::rules::InvalidAllFormat),
        (Pylint, "E0704") => (RuleGroup::Preview, rules::pylint::rules::MisplacedBareRaise),
        (Pylint, "E1142") => (RuleGroup::Stable, rules::pylint::rules::AwaitOutsideAsync),
        (Pylint, "E1205") => (RuleGroup::Stable, rules::pylint::rules::LoggingTooManyArgs),
        (Pylint, "E1206") => (RuleGroup::Stable, rules::pylint::rules::LoggingTooFewArgs),
        (Pylint, "E1300") => (RuleGroup::Stable, rules::pylint::rules::BadStringFormatCharacter),
        (Pylint, "E1307") => (RuleGroup::Stable, rules::pylint::rules::BadStringFormatType),
        (Pylint, "E1310") => (RuleGroup::Stable, rules::pylint::rules::BadStrStripCall),
        (Pylint, "E1507") => (RuleGroup::Stable, rules::pylint::rules::InvalidEnvvarValue),
        (Pylint, "E1700") => (RuleGroup::Stable, rules::pylint::rules::YieldFromInAsyncFunction),
        (Pylint, "E2502") => (RuleGroup::Stable, rules::pylint::rules::BidirectionalUnicode),
        (Pylint, "E2510") => (RuleGroup::Stable, rules::pylint::rules::InvalidCharacterBackspace),
        (Pylint, "E2512") => (RuleGroup::Stable, rules::pylint::rules::InvalidCharacterSub),
        (Pylint, "E2513") => (RuleGroup::Stable, rules::pylint::rules::InvalidCharacterEsc),
        (Pylint, "E2514") => (RuleGroup::Stable, rules::pylint::rules::InvalidCharacterNul),
        (Pylint, "E2515") => (RuleGroup::Stable, rules::pylint::rules::InvalidCharacterZeroWidthSpace),
        (Pylint, "R0124") => (RuleGroup::Stable, rules::pylint::rules::ComparisonWithItself),
        (Pylint, "R0133") => (RuleGroup::Stable, rules::pylint::rules::ComparisonOfConstant),
        (Pylint, "R0206") => (RuleGroup::Stable, rules::pylint::rules::PropertyWithParameters),
        (Pylint, "R0402") => (RuleGroup::Stable, rules::pylint::rules::ManualFromImport),
        (Pylint, "R0911") => (RuleGroup::Stable, rules::pylint::rules::TooManyReturnStatements),
        (Pylint, "R0912") => (RuleGroup::Stable, rules::pylint::rules::TooManyBranches),
        (Pylint, "R0913") => (RuleGroup::Stable, rules::pylint::rules::TooManyArguments),
        (Pylint, "R0915") => (RuleGroup::Stable, rules::pylint::rules::TooManyStatements),
        (Pylint, "R0916") => (RuleGroup::Preview, rules::pylint::rules::TooManyBooleanExpressions),
        (Pylint, "R1701") => (RuleGroup::Stable, rules::pylint::rules::RepeatedIsinstanceCalls),
        (Pylint, "R1711") => (RuleGroup::Stable, rules::pylint::rules::UselessReturn),
        (Pylint, "R1714") => (RuleGroup::Stable, rules::pylint::rules::RepeatedEqualityComparison),
        (Pylint, "R1706") => (RuleGroup::Preview, rules::pylint::rules::AndOrTernary),
        (Pylint, "R1722") => (RuleGroup::Stable, rules::pylint::rules::SysExitAlias),
        (Pylint, "R2004") => (RuleGroup::Stable, rules::pylint::rules::MagicValueComparison),
        (Pylint, "R5501") => (RuleGroup::Stable, rules::pylint::rules::CollapsibleElseIf),
        (Pylint, "R6201") => (RuleGroup::Preview, rules::pylint::rules::LiteralMembership),
        #[allow(deprecated)]
        (Pylint, "R6301") => (RuleGroup::Nursery, rules::pylint::rules::NoSelfUse),
        (Pylint, "W0120") => (RuleGroup::Stable, rules::pylint::rules::UselessElseOnLoop),
        (Pylint, "W0127") => (RuleGroup::Stable, rules::pylint::rules::SelfAssigningVariable),
        (Pylint, "W0129") => (RuleGroup::Stable, rules::pylint::rules::AssertOnStringLiteral),
        (Pylint, "W0131") => (RuleGroup::Stable, rules::pylint::rules::NamedExprWithoutContext),
        (Pylint, "W0406") => (RuleGroup::Stable, rules::pylint::rules::ImportSelf),
        (Pylint, "W0602") => (RuleGroup::Stable, rules::pylint::rules::GlobalVariableNotAssigned),
        (Pylint, "W0604") => (RuleGroup::Preview, rules::pylint::rules::GlobalAtModuleLevel),
        (Pylint, "W0603") => (RuleGroup::Stable, rules::pylint::rules::GlobalStatement),
        (Pylint, "W0711") => (RuleGroup::Stable, rules::pylint::rules::BinaryOpException),
        (Pylint, "W1508") => (RuleGroup::Stable, rules::pylint::rules::InvalidEnvvarDefault),
        (Pylint, "W1509") => (RuleGroup::Stable, rules::pylint::rules::SubprocessPopenPreexecFn),
        (Pylint, "W1510") => (RuleGroup::Stable, rules::pylint::rules::SubprocessRunWithoutCheck),
        (Pylint, "W1514") => (RuleGroup::Preview, rules::pylint::rules::UnspecifiedEncoding),
        #[allow(deprecated)]
        (Pylint, "W1641") => (RuleGroup::Nursery, rules::pylint::rules::EqWithoutHash),
        (Pylint, "R0904") => (RuleGroup::Preview, rules::pylint::rules::TooManyPublicMethods),
        (Pylint, "W2901") => (RuleGroup::Stable, rules::pylint::rules::RedefinedLoopName),
        #[allow(deprecated)]
        (Pylint, "W3201") => (RuleGroup::Nursery, rules::pylint::rules::BadDunderMethodName),
        (Pylint, "W3301") => (RuleGroup::Stable, rules::pylint::rules::NestedMinMax),

        // flake8-async
        (Flake8Async, "100") => (RuleGroup::Stable, rules::flake8_async::rules::BlockingHttpCallInAsyncFunction),
        (Flake8Async, "101") => (RuleGroup::Stable, rules::flake8_async::rules::OpenSleepOrSubprocessInAsyncFunction),
        (Flake8Async, "102") => (RuleGroup::Stable, rules::flake8_async::rules::BlockingOsCallInAsyncFunction),

        // flake8-builtins
        (Flake8Builtins, "001") => (RuleGroup::Stable, rules::flake8_builtins::rules::BuiltinVariableShadowing),
        (Flake8Builtins, "002") => (RuleGroup::Stable, rules::flake8_builtins::rules::BuiltinArgumentShadowing),
        (Flake8Builtins, "003") => (RuleGroup::Stable, rules::flake8_builtins::rules::BuiltinAttributeShadowing),

        // flake8-bugbear
        (Flake8Bugbear, "002") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UnaryPrefixIncrementDecrement),
        (Flake8Bugbear, "003") => (RuleGroup::Stable, rules::flake8_bugbear::rules::AssignmentToOsEnviron),
        (Flake8Bugbear, "004") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UnreliableCallableCheck),
        (Flake8Bugbear, "005") => (RuleGroup::Stable, rules::flake8_bugbear::rules::StripWithMultiCharacters),
        (Flake8Bugbear, "006") => (RuleGroup::Stable, rules::flake8_bugbear::rules::MutableArgumentDefault),
        (Flake8Bugbear, "007") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UnusedLoopControlVariable),
        (Flake8Bugbear, "008") => (RuleGroup::Stable, rules::flake8_bugbear::rules::FunctionCallInDefaultArgument),
        (Flake8Bugbear, "009") => (RuleGroup::Stable, rules::flake8_bugbear::rules::GetAttrWithConstant),
        (Flake8Bugbear, "010") => (RuleGroup::Stable, rules::flake8_bugbear::rules::SetAttrWithConstant),
        (Flake8Bugbear, "011") => (RuleGroup::Stable, rules::flake8_bugbear::rules::AssertFalse),
        (Flake8Bugbear, "012") => (RuleGroup::Stable, rules::flake8_bugbear::rules::JumpStatementInFinally),
        (Flake8Bugbear, "013") => (RuleGroup::Stable, rules::flake8_bugbear::rules::RedundantTupleInExceptionHandler),
        (Flake8Bugbear, "014") => (RuleGroup::Stable, rules::flake8_bugbear::rules::DuplicateHandlerException),
        (Flake8Bugbear, "015") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UselessComparison),
        (Flake8Bugbear, "016") => (RuleGroup::Stable, rules::flake8_bugbear::rules::RaiseLiteral),
        (Flake8Bugbear, "017") => (RuleGroup::Stable, rules::flake8_bugbear::rules::AssertRaisesException),
        (Flake8Bugbear, "018") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UselessExpression),
        (Flake8Bugbear, "019") => (RuleGroup::Stable, rules::flake8_bugbear::rules::CachedInstanceMethod),
        (Flake8Bugbear, "020") => (RuleGroup::Stable, rules::flake8_bugbear::rules::LoopVariableOverridesIterator),
        (Flake8Bugbear, "021") => (RuleGroup::Stable, rules::flake8_bugbear::rules::FStringDocstring),
        (Flake8Bugbear, "022") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UselessContextlibSuppress),
        (Flake8Bugbear, "023") => (RuleGroup::Stable, rules::flake8_bugbear::rules::FunctionUsesLoopVariable),
        (Flake8Bugbear, "024") => (RuleGroup::Stable, rules::flake8_bugbear::rules::AbstractBaseClassWithoutAbstractMethod),
        (Flake8Bugbear, "025") => (RuleGroup::Stable, rules::flake8_bugbear::rules::DuplicateTryBlockException),
        (Flake8Bugbear, "026") => (RuleGroup::Stable, rules::flake8_bugbear::rules::StarArgUnpackingAfterKeywordArg),
        (Flake8Bugbear, "027") => (RuleGroup::Stable, rules::flake8_bugbear::rules::EmptyMethodWithoutAbstractDecorator),
        (Flake8Bugbear, "028") => (RuleGroup::Stable, rules::flake8_bugbear::rules::NoExplicitStacklevel),
        (Flake8Bugbear, "029") => (RuleGroup::Stable, rules::flake8_bugbear::rules::ExceptWithEmptyTuple),
        (Flake8Bugbear, "030") => (RuleGroup::Stable, rules::flake8_bugbear::rules::ExceptWithNonExceptionClasses),
        (Flake8Bugbear, "031") => (RuleGroup::Stable, rules::flake8_bugbear::rules::ReuseOfGroupbyGenerator),
        (Flake8Bugbear, "032") => (RuleGroup::Stable, rules::flake8_bugbear::rules::UnintentionalTypeAnnotation),
        (Flake8Bugbear, "033") => (RuleGroup::Stable, rules::flake8_bugbear::rules::DuplicateValue),
        (Flake8Bugbear, "034") => (RuleGroup::Stable, rules::flake8_bugbear::rules::ReSubPositionalArgs),
        (Flake8Bugbear, "904") => (RuleGroup::Stable, rules::flake8_bugbear::rules::RaiseWithoutFromInsideExcept),
        (Flake8Bugbear, "905") => (RuleGroup::Stable, rules::flake8_bugbear::rules::ZipWithoutExplicitStrict),

        // flake8-blind-except
        (Flake8BlindExcept, "001") => (RuleGroup::Stable, rules::flake8_blind_except::rules::BlindExcept),

        // flake8-comprehensions
        (Flake8Comprehensions, "00") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryGeneratorList),
        (Flake8Comprehensions, "01") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryGeneratorSet),
        (Flake8Comprehensions, "02") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryGeneratorDict),
        (Flake8Comprehensions, "03") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryListComprehensionSet),
        (Flake8Comprehensions, "04") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryListComprehensionDict),
        (Flake8Comprehensions, "05") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryLiteralSet),
        (Flake8Comprehensions, "06") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryLiteralDict),
        (Flake8Comprehensions, "08") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryCollectionCall),
        (Flake8Comprehensions, "09") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinTupleCall),
        (Flake8Comprehensions, "10") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinListCall),
        (Flake8Comprehensions, "11") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryListCall),
        (Flake8Comprehensions, "13") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryCallAroundSorted),
        (Flake8Comprehensions, "14") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryDoubleCastOrProcess),
        (Flake8Comprehensions, "15") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessarySubscriptReversal),
        (Flake8Comprehensions, "16") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryComprehension),
        (Flake8Comprehensions, "17") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryMap),
        (Flake8Comprehensions, "18") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryLiteralWithinDictCall),
        (Flake8Comprehensions, "19") => (RuleGroup::Stable, rules::flake8_comprehensions::rules::UnnecessaryComprehensionAnyAll),

        // flake8-debugger
        (Flake8Debugger, "0") => (RuleGroup::Stable, rules::flake8_debugger::rules::Debugger),

        // mccabe
        (McCabe, "1") => (RuleGroup::Stable, rules::mccabe::rules::ComplexStructure),

        // flake8-tidy-imports
        (Flake8TidyImports, "251") => (RuleGroup::Stable, rules::flake8_tidy_imports::rules::BannedApi),
        (Flake8TidyImports, "252") => (RuleGroup::Stable, rules::flake8_tidy_imports::rules::RelativeImports),
        (Flake8TidyImports, "253") => (RuleGroup::Stable, rules::flake8_tidy_imports::rules::BannedModuleLevelImports),

        // flake8-return
        (Flake8Return, "501") => (RuleGroup::Stable, rules::flake8_return::rules::UnnecessaryReturnNone),
        (Flake8Return, "502") => (RuleGroup::Stable, rules::flake8_return::rules::ImplicitReturnValue),
        (Flake8Return, "503") => (RuleGroup::Stable, rules::flake8_return::rules::ImplicitReturn),
        (Flake8Return, "504") => (RuleGroup::Stable, rules::flake8_return::rules::UnnecessaryAssign),
        (Flake8Return, "505") => (RuleGroup::Stable, rules::flake8_return::rules::SuperfluousElseReturn),
        (Flake8Return, "506") => (RuleGroup::Stable, rules::flake8_return::rules::SuperfluousElseRaise),
        (Flake8Return, "507") => (RuleGroup::Stable, rules::flake8_return::rules::SuperfluousElseContinue),
        (Flake8Return, "508") => (RuleGroup::Stable, rules::flake8_return::rules::SuperfluousElseBreak),

        // flake8-gettext
        (Flake8GetText, "001") => (RuleGroup::Stable, rules::flake8_gettext::rules::FStringInGetTextFuncCall),
        (Flake8GetText, "002") => (RuleGroup::Stable, rules::flake8_gettext::rules::FormatInGetTextFuncCall),
        (Flake8GetText, "003") => (RuleGroup::Stable, rules::flake8_gettext::rules::PrintfInGetTextFuncCall),

        // flake8-implicit-str-concat
        (Flake8ImplicitStrConcat, "001") => (RuleGroup::Stable, rules::flake8_implicit_str_concat::rules::SingleLineImplicitStringConcatenation),
        (Flake8ImplicitStrConcat, "002") => (RuleGroup::Stable, rules::flake8_implicit_str_concat::rules::MultiLineImplicitStringConcatenation),
        (Flake8ImplicitStrConcat, "003") => (RuleGroup::Stable, rules::flake8_implicit_str_concat::rules::ExplicitStringConcatenation),

        // flake8-print
        (Flake8Print, "1") => (RuleGroup::Stable, rules::flake8_print::rules::Print),
        (Flake8Print, "3") => (RuleGroup::Stable, rules::flake8_print::rules::PPrint),

        // flake8-quotes
        (Flake8Quotes, "000") => (RuleGroup::Stable, rules::flake8_quotes::rules::BadQuotesInlineString),
        (Flake8Quotes, "001") => (RuleGroup::Stable, rules::flake8_quotes::rules::BadQuotesMultilineString),
        (Flake8Quotes, "002") => (RuleGroup::Stable, rules::flake8_quotes::rules::BadQuotesDocstring),
        (Flake8Quotes, "003") => (RuleGroup::Stable, rules::flake8_quotes::rules::AvoidableEscapedQuote),

        // flake8-annotations
        (Flake8Annotations, "001") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingTypeFunctionArgument),
        (Flake8Annotations, "002") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingTypeArgs),
        (Flake8Annotations, "003") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingTypeKwargs),
        (Flake8Annotations, "101") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingTypeSelf),
        (Flake8Annotations, "102") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingTypeCls),
        (Flake8Annotations, "201") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingReturnTypeUndocumentedPublicFunction),
        (Flake8Annotations, "202") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingReturnTypePrivateFunction),
        (Flake8Annotations, "204") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingReturnTypeSpecialMethod),
        (Flake8Annotations, "205") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingReturnTypeStaticMethod),
        (Flake8Annotations, "206") => (RuleGroup::Stable, rules::flake8_annotations::rules::MissingReturnTypeClassMethod),
        (Flake8Annotations, "401") => (RuleGroup::Stable, rules::flake8_annotations::rules::AnyType),

        // flake8-future-annotations
        (Flake8FutureAnnotations, "100") => (RuleGroup::Stable, rules::flake8_future_annotations::rules::FutureRewritableTypeAnnotation),
        (Flake8FutureAnnotations, "102") => (RuleGroup::Stable, rules::flake8_future_annotations::rules::FutureRequiredTypeAnnotation),

        // flake8-2020
        (Flake82020, "101") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionSlice3),
        (Flake82020, "102") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersion2),
        (Flake82020, "103") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionCmpStr3),
        (Flake82020, "201") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionInfo0Eq3),
        (Flake82020, "202") => (RuleGroup::Stable, rules::flake8_2020::rules::SixPY3),
        (Flake82020, "203") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionInfo1CmpInt),
        (Flake82020, "204") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionInfoMinorCmpInt),
        (Flake82020, "301") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersion0),
        (Flake82020, "302") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionCmpStr10),
        (Flake82020, "303") => (RuleGroup::Stable, rules::flake8_2020::rules::SysVersionSlice1),

        // flake8-simplify
        (Flake8Simplify, "101") => (RuleGroup::Stable, rules::flake8_simplify::rules::DuplicateIsinstanceCall),
        (Flake8Simplify, "102") => (RuleGroup::Stable, rules::flake8_simplify::rules::CollapsibleIf),
        (Flake8Simplify, "103") => (RuleGroup::Stable, rules::flake8_simplify::rules::NeedlessBool),
        (Flake8Simplify, "105") => (RuleGroup::Stable, rules::flake8_simplify::rules::SuppressibleException),
        (Flake8Simplify, "107") => (RuleGroup::Stable, rules::flake8_simplify::rules::ReturnInTryExceptFinally),
        (Flake8Simplify, "108") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfElseBlockInsteadOfIfExp),
        (Flake8Simplify, "109") => (RuleGroup::Stable, rules::flake8_simplify::rules::CompareWithTuple),
        (Flake8Simplify, "110") => (RuleGroup::Stable, rules::flake8_simplify::rules::ReimplementedBuiltin),
        (Flake8Simplify, "112") => (RuleGroup::Stable, rules::flake8_simplify::rules::UncapitalizedEnvironmentVariables),
        (Flake8Simplify, "114") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfWithSameArms),
        (Flake8Simplify, "115") => (RuleGroup::Stable, rules::flake8_simplify::rules::OpenFileWithContextHandler),
        (Flake8Simplify, "116") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfElseBlockInsteadOfDictLookup),
        (Flake8Simplify, "117") => (RuleGroup::Stable, rules::flake8_simplify::rules::MultipleWithStatements),
        (Flake8Simplify, "118") => (RuleGroup::Stable, rules::flake8_simplify::rules::InDictKeys),
        (Flake8Simplify, "201") => (RuleGroup::Stable, rules::flake8_simplify::rules::NegateEqualOp),
        (Flake8Simplify, "202") => (RuleGroup::Stable, rules::flake8_simplify::rules::NegateNotEqualOp),
        (Flake8Simplify, "208") => (RuleGroup::Stable, rules::flake8_simplify::rules::DoubleNegation),
        (Flake8Simplify, "210") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfExprWithTrueFalse),
        (Flake8Simplify, "211") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfExprWithFalseTrue),
        (Flake8Simplify, "212") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfExprWithTwistedArms),
        (Flake8Simplify, "220") => (RuleGroup::Stable, rules::flake8_simplify::rules::ExprAndNotExpr),
        (Flake8Simplify, "221") => (RuleGroup::Stable, rules::flake8_simplify::rules::ExprOrNotExpr),
        (Flake8Simplify, "222") => (RuleGroup::Stable, rules::flake8_simplify::rules::ExprOrTrue),
        (Flake8Simplify, "223") => (RuleGroup::Stable, rules::flake8_simplify::rules::ExprAndFalse),
        (Flake8Simplify, "300") => (RuleGroup::Stable, rules::flake8_simplify::rules::YodaConditions),
        (Flake8Simplify, "401") => (RuleGroup::Stable, rules::flake8_simplify::rules::IfElseBlockInsteadOfDictGet),
        (Flake8Simplify, "910") => (RuleGroup::Stable, rules::flake8_simplify::rules::DictGetWithNoneDefault),

        // flake8-copyright
        #[allow(deprecated)]
        (Flake8Copyright, "001") => (RuleGroup::Nursery, rules::flake8_copyright::rules::MissingCopyrightNotice),

        // pyupgrade
        (Pyupgrade, "001") => (RuleGroup::Stable, rules::pyupgrade::rules::UselessMetaclassType),
        (Pyupgrade, "003") => (RuleGroup::Stable, rules::pyupgrade::rules::TypeOfPrimitive),
        (Pyupgrade, "004") => (RuleGroup::Stable, rules::pyupgrade::rules::UselessObjectInheritance),
        (Pyupgrade, "005") => (RuleGroup::Stable, rules::pyupgrade::rules::DeprecatedUnittestAlias),
        (Pyupgrade, "006") => (RuleGroup::Stable, rules::pyupgrade::rules::NonPEP585Annotation),
        (Pyupgrade, "007") => (RuleGroup::Stable, rules::pyupgrade::rules::NonPEP604Annotation),
        (Pyupgrade, "008") => (RuleGroup::Stable, rules::pyupgrade::rules::SuperCallWithParameters),
        (Pyupgrade, "009") => (RuleGroup::Stable, rules::pyupgrade::rules::UTF8EncodingDeclaration),
        (Pyupgrade, "010") => (RuleGroup::Stable, rules::pyupgrade::rules::UnnecessaryFutureImport),
        (Pyupgrade, "011") => (RuleGroup::Stable, rules::pyupgrade::rules::LRUCacheWithoutParameters),
        (Pyupgrade, "012") => (RuleGroup::Stable, rules::pyupgrade::rules::UnnecessaryEncodeUTF8),
        (Pyupgrade, "013") => (RuleGroup::Stable, rules::pyupgrade::rules::ConvertTypedDictFunctionalToClass),
        (Pyupgrade, "014") => (RuleGroup::Stable, rules::pyupgrade::rules::ConvertNamedTupleFunctionalToClass),
        (Pyupgrade, "015") => (RuleGroup::Stable, rules::pyupgrade::rules::RedundantOpenModes),
        (Pyupgrade, "017") => (RuleGroup::Stable, rules::pyupgrade::rules::DatetimeTimezoneUTC),
        (Pyupgrade, "018") => (RuleGroup::Stable, rules::pyupgrade::rules::NativeLiterals),
        (Pyupgrade, "019") => (RuleGroup::Stable, rules::pyupgrade::rules::TypingTextStrAlias),
        (Pyupgrade, "020") => (RuleGroup::Stable, rules::pyupgrade::rules::OpenAlias),
        (Pyupgrade, "021") => (RuleGroup::Stable, rules::pyupgrade::rules::ReplaceUniversalNewlines),
        (Pyupgrade, "022") => (RuleGroup::Stable, rules::pyupgrade::rules::ReplaceStdoutStderr),
        (Pyupgrade, "023") => (RuleGroup::Stable, rules::pyupgrade::rules::DeprecatedCElementTree),
        (Pyupgrade, "024") => (RuleGroup::Stable, rules::pyupgrade::rules::OSErrorAlias),
        (Pyupgrade, "025") => (RuleGroup::Stable, rules::pyupgrade::rules::UnicodeKindPrefix),
        (Pyupgrade, "026") => (RuleGroup::Stable, rules::pyupgrade::rules::DeprecatedMockImport),
        (Pyupgrade, "027") => (RuleGroup::Stable, rules::pyupgrade::rules::UnpackedListComprehension),
        (Pyupgrade, "028") => (RuleGroup::Stable, rules::pyupgrade::rules::YieldInForLoop),
        (Pyupgrade, "029") => (RuleGroup::Stable, rules::pyupgrade::rules::UnnecessaryBuiltinImport),
        (Pyupgrade, "030") => (RuleGroup::Stable, rules::pyupgrade::rules::FormatLiterals),
        (Pyupgrade, "031") => (RuleGroup::Stable, rules::pyupgrade::rules::PrintfStringFormatting),
        (Pyupgrade, "032") => (RuleGroup::Stable, rules::pyupgrade::rules::FString),
        (Pyupgrade, "033") => (RuleGroup::Stable, rules::pyupgrade::rules::LRUCacheWithMaxsizeNone),
        (Pyupgrade, "034") => (RuleGroup::Stable, rules::pyupgrade::rules::ExtraneousParentheses),
        (Pyupgrade, "035") => (RuleGroup::Stable, rules::pyupgrade::rules::DeprecatedImport),
        (Pyupgrade, "036") => (RuleGroup::Stable, rules::pyupgrade::rules::OutdatedVersionBlock),
        (Pyupgrade, "037") => (RuleGroup::Stable, rules::pyupgrade::rules::QuotedAnnotation),
        (Pyupgrade, "038") => (RuleGroup::Stable, rules::pyupgrade::rules::NonPEP604Isinstance),
        (Pyupgrade, "039") => (RuleGroup::Stable, rules::pyupgrade::rules::UnnecessaryClassParentheses),
        (Pyupgrade, "040") => (RuleGroup::Stable, rules::pyupgrade::rules::NonPEP695TypeAlias),

        // pydocstyle
        (Pydocstyle, "100") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicModule),
        (Pydocstyle, "101") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicClass),
        (Pydocstyle, "102") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicMethod),
        (Pydocstyle, "103") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicFunction),
        (Pydocstyle, "104") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicPackage),
        (Pydocstyle, "105") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedMagicMethod),
        (Pydocstyle, "106") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicNestedClass),
        (Pydocstyle, "107") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedPublicInit),
        (Pydocstyle, "200") => (RuleGroup::Stable, rules::pydocstyle::rules::FitsOnOneLine),
        (Pydocstyle, "201") => (RuleGroup::Stable, rules::pydocstyle::rules::NoBlankLineBeforeFunction),
        (Pydocstyle, "202") => (RuleGroup::Stable, rules::pydocstyle::rules::NoBlankLineAfterFunction),
        (Pydocstyle, "203") => (RuleGroup::Stable, rules::pydocstyle::rules::OneBlankLineBeforeClass),
        (Pydocstyle, "204") => (RuleGroup::Stable, rules::pydocstyle::rules::OneBlankLineAfterClass),
        (Pydocstyle, "205") => (RuleGroup::Stable, rules::pydocstyle::rules::BlankLineAfterSummary),
        (Pydocstyle, "206") => (RuleGroup::Stable, rules::pydocstyle::rules::IndentWithSpaces),
        (Pydocstyle, "207") => (RuleGroup::Stable, rules::pydocstyle::rules::UnderIndentation),
        (Pydocstyle, "208") => (RuleGroup::Stable, rules::pydocstyle::rules::OverIndentation),
        (Pydocstyle, "209") => (RuleGroup::Stable, rules::pydocstyle::rules::NewLineAfterLastParagraph),
        (Pydocstyle, "210") => (RuleGroup::Stable, rules::pydocstyle::rules::SurroundingWhitespace),
        (Pydocstyle, "211") => (RuleGroup::Stable, rules::pydocstyle::rules::BlankLineBeforeClass),
        (Pydocstyle, "212") => (RuleGroup::Stable, rules::pydocstyle::rules::MultiLineSummaryFirstLine),
        (Pydocstyle, "213") => (RuleGroup::Stable, rules::pydocstyle::rules::MultiLineSummarySecondLine),
        (Pydocstyle, "214") => (RuleGroup::Stable, rules::pydocstyle::rules::SectionNotOverIndented),
        (Pydocstyle, "215") => (RuleGroup::Stable, rules::pydocstyle::rules::SectionUnderlineNotOverIndented),
        (Pydocstyle, "300") => (RuleGroup::Stable, rules::pydocstyle::rules::TripleSingleQuotes),
        (Pydocstyle, "301") => (RuleGroup::Stable, rules::pydocstyle::rules::EscapeSequenceInDocstring),
        (Pydocstyle, "400") => (RuleGroup::Stable, rules::pydocstyle::rules::EndsInPeriod),
        (Pydocstyle, "401") => (RuleGroup::Stable, rules::pydocstyle::rules::NonImperativeMood),
        (Pydocstyle, "402") => (RuleGroup::Stable, rules::pydocstyle::rules::NoSignature),
        (Pydocstyle, "403") => (RuleGroup::Stable, rules::pydocstyle::rules::FirstLineCapitalized),
        (Pydocstyle, "404") => (RuleGroup::Stable, rules::pydocstyle::rules::DocstringStartsWithThis),
        (Pydocstyle, "405") => (RuleGroup::Stable, rules::pydocstyle::rules::CapitalizeSectionName),
        (Pydocstyle, "406") => (RuleGroup::Stable, rules::pydocstyle::rules::NewLineAfterSectionName),
        (Pydocstyle, "407") => (RuleGroup::Stable, rules::pydocstyle::rules::DashedUnderlineAfterSection),
        (Pydocstyle, "408") => (RuleGroup::Stable, rules::pydocstyle::rules::SectionUnderlineAfterName),
        (Pydocstyle, "409") => (RuleGroup::Stable, rules::pydocstyle::rules::SectionUnderlineMatchesSectionLength),
        (Pydocstyle, "410") => (RuleGroup::Stable, rules::pydocstyle::rules::NoBlankLineAfterSection),
        (Pydocstyle, "411") => (RuleGroup::Stable, rules::pydocstyle::rules::NoBlankLineBeforeSection),
        (Pydocstyle, "412") => (RuleGroup::Stable, rules::pydocstyle::rules::BlankLinesBetweenHeaderAndContent),
        (Pydocstyle, "413") => (RuleGroup::Stable, rules::pydocstyle::rules::BlankLineAfterLastSection),
        (Pydocstyle, "414") => (RuleGroup::Stable, rules::pydocstyle::rules::EmptyDocstringSection),
        (Pydocstyle, "415") => (RuleGroup::Stable, rules::pydocstyle::rules::EndsInPunctuation),
        (Pydocstyle, "416") => (RuleGroup::Stable, rules::pydocstyle::rules::SectionNameEndsInColon),
        (Pydocstyle, "417") => (RuleGroup::Stable, rules::pydocstyle::rules::UndocumentedParam),
        (Pydocstyle, "418") => (RuleGroup::Stable, rules::pydocstyle::rules::OverloadWithDocstring),
        (Pydocstyle, "419") => (RuleGroup::Stable, rules::pydocstyle::rules::EmptyDocstring),

        // pep8-naming
        (PEP8Naming, "801") => (RuleGroup::Stable, rules::pep8_naming::rules::InvalidClassName),
        (PEP8Naming, "802") => (RuleGroup::Stable, rules::pep8_naming::rules::InvalidFunctionName),
        (PEP8Naming, "803") => (RuleGroup::Stable, rules::pep8_naming::rules::InvalidArgumentName),
        (PEP8Naming, "804") => (RuleGroup::Stable, rules::pep8_naming::rules::InvalidFirstArgumentNameForClassMethod),
        (PEP8Naming, "805") => (RuleGroup::Stable, rules::pep8_naming::rules::InvalidFirstArgumentNameForMethod),
        (PEP8Naming, "806") => (RuleGroup::Stable, rules::pep8_naming::rules::NonLowercaseVariableInFunction),
        (PEP8Naming, "807") => (RuleGroup::Stable, rules::pep8_naming::rules::DunderFunctionName),
        (PEP8Naming, "811") => (RuleGroup::Stable, rules::pep8_naming::rules::ConstantImportedAsNonConstant),
        (PEP8Naming, "812") => (RuleGroup::Stable, rules::pep8_naming::rules::LowercaseImportedAsNonLowercase),
        (PEP8Naming, "813") => (RuleGroup::Stable, rules::pep8_naming::rules::CamelcaseImportedAsLowercase),
        (PEP8Naming, "814") => (RuleGroup::Stable, rules::pep8_naming::rules::CamelcaseImportedAsConstant),
        (PEP8Naming, "815") => (RuleGroup::Stable, rules::pep8_naming::rules::MixedCaseVariableInClassScope),
        (PEP8Naming, "816") => (RuleGroup::Stable, rules::pep8_naming::rules::MixedCaseVariableInGlobalScope),
        (PEP8Naming, "817") => (RuleGroup::Stable, rules::pep8_naming::rules::CamelcaseImportedAsAcronym),
        (PEP8Naming, "818") => (RuleGroup::Stable, rules::pep8_naming::rules::ErrorSuffixOnExceptionName),
        (PEP8Naming, "999") => (RuleGroup::Stable, rules::pep8_naming::rules::InvalidModuleName),

        // isort
        (Isort, "001") => (RuleGroup::Stable, rules::isort::rules::UnsortedImports),
        (Isort, "002") => (RuleGroup::Stable, rules::isort::rules::MissingRequiredImport),

        // eradicate
        (Eradicate, "001") => (RuleGroup::Stable, rules::eradicate::rules::CommentedOutCode),

        // flake8-bandit
        (Flake8Bandit, "101") => (RuleGroup::Stable, rules::flake8_bandit::rules::Assert),
        (Flake8Bandit, "102") => (RuleGroup::Stable, rules::flake8_bandit::rules::ExecBuiltin),
        (Flake8Bandit, "103") => (RuleGroup::Stable, rules::flake8_bandit::rules::BadFilePermissions),
        (Flake8Bandit, "104") => (RuleGroup::Stable, rules::flake8_bandit::rules::HardcodedBindAllInterfaces),
        (Flake8Bandit, "105") => (RuleGroup::Stable, rules::flake8_bandit::rules::HardcodedPasswordString),
        (Flake8Bandit, "106") => (RuleGroup::Stable, rules::flake8_bandit::rules::HardcodedPasswordFuncArg),
        (Flake8Bandit, "107") => (RuleGroup::Stable, rules::flake8_bandit::rules::HardcodedPasswordDefault),
        (Flake8Bandit, "108") => (RuleGroup::Stable, rules::flake8_bandit::rules::HardcodedTempFile),
        (Flake8Bandit, "110") => (RuleGroup::Stable, rules::flake8_bandit::rules::TryExceptPass),
        (Flake8Bandit, "112") => (RuleGroup::Stable, rules::flake8_bandit::rules::TryExceptContinue),
        (Flake8Bandit, "113") => (RuleGroup::Stable, rules::flake8_bandit::rules::RequestWithoutTimeout),
        (Flake8Bandit, "201") => (RuleGroup::Preview, rules::flake8_bandit::rules::FlaskDebugTrue),
        (Flake8Bandit, "301") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousPickleUsage),
        (Flake8Bandit, "302") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousMarshalUsage),
        (Flake8Bandit, "303") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousInsecureHashUsage),
        (Flake8Bandit, "304") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousInsecureCipherUsage),
        (Flake8Bandit, "305") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousInsecureCipherModeUsage),
        (Flake8Bandit, "306") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousMktempUsage),
        (Flake8Bandit, "307") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousEvalUsage),
        (Flake8Bandit, "308") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousMarkSafeUsage),
        (Flake8Bandit, "310") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousURLOpenUsage),
        (Flake8Bandit, "311") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousNonCryptographicRandomUsage),
        (Flake8Bandit, "312") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousTelnetUsage),
        (Flake8Bandit, "313") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLCElementTreeUsage),
        (Flake8Bandit, "314") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLElementTreeUsage),
        (Flake8Bandit, "315") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLExpatReaderUsage),
        (Flake8Bandit, "316") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLExpatBuilderUsage),
        (Flake8Bandit, "317") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLSaxUsage),
        (Flake8Bandit, "318") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLMiniDOMUsage),
        (Flake8Bandit, "319") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLPullDOMUsage),
        (Flake8Bandit, "320") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousXMLETreeUsage),
        (Flake8Bandit, "321") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousFTPLibUsage),
        (Flake8Bandit, "323") => (RuleGroup::Stable, rules::flake8_bandit::rules::SuspiciousUnverifiedContextUsage),
        (Flake8Bandit, "324") => (RuleGroup::Stable, rules::flake8_bandit::rules::HashlibInsecureHashFunction),
        (Flake8Bandit, "501") => (RuleGroup::Stable, rules::flake8_bandit::rules::RequestWithNoCertValidation),
        (Flake8Bandit, "505") => (RuleGroup::Preview, rules::flake8_bandit::rules::WeakCryptographicKey),
        (Flake8Bandit, "506") => (RuleGroup::Stable, rules::flake8_bandit::rules::UnsafeYAMLLoad),
        (Flake8Bandit, "507") => (RuleGroup::Preview, rules::flake8_bandit::rules::SSHNoHostKeyVerification),
        (Flake8Bandit, "508") => (RuleGroup::Stable, rules::flake8_bandit::rules::SnmpInsecureVersion),
        (Flake8Bandit, "509") => (RuleGroup::Stable, rules::flake8_bandit::rules::SnmpWeakCryptography),
        (Flake8Bandit, "601") => (RuleGroup::Stable, rules::flake8_bandit::rules::ParamikoCall),
        (Flake8Bandit, "602") => (RuleGroup::Stable, rules::flake8_bandit::rules::SubprocessPopenWithShellEqualsTrue),
        (Flake8Bandit, "603") => (RuleGroup::Stable, rules::flake8_bandit::rules::SubprocessWithoutShellEqualsTrue),
        (Flake8Bandit, "604") => (RuleGroup::Stable, rules::flake8_bandit::rules::CallWithShellEqualsTrue),
        (Flake8Bandit, "605") => (RuleGroup::Stable, rules::flake8_bandit::rules::StartProcessWithAShell),
        (Flake8Bandit, "606") => (RuleGroup::Stable, rules::flake8_bandit::rules::StartProcessWithNoShell),
        (Flake8Bandit, "607") => (RuleGroup::Stable, rules::flake8_bandit::rules::StartProcessWithPartialPath),
        (Flake8Bandit, "608") => (RuleGroup::Stable, rules::flake8_bandit::rules::HardcodedSQLExpression),
        (Flake8Bandit, "609") => (RuleGroup::Stable, rules::flake8_bandit::rules::UnixCommandWildcardInjection),
        (Flake8Bandit, "612") => (RuleGroup::Stable, rules::flake8_bandit::rules::LoggingConfigInsecureListen),
        (Flake8Bandit, "701") => (RuleGroup::Stable, rules::flake8_bandit::rules::Jinja2AutoescapeFalse),

        // flake8-boolean-trap
        (Flake8BooleanTrap, "001") => (RuleGroup::Stable, rules::flake8_boolean_trap::rules::BooleanTypeHintPositionalArgument),
        (Flake8BooleanTrap, "002") => (RuleGroup::Stable, rules::flake8_boolean_trap::rules::BooleanDefaultValuePositionalArgument),
        (Flake8BooleanTrap, "003") => (RuleGroup::Stable, rules::flake8_boolean_trap::rules::BooleanPositionalValueInCall),

        // flake8-unused-arguments
        (Flake8UnusedArguments, "001") => (RuleGroup::Stable, rules::flake8_unused_arguments::rules::UnusedFunctionArgument),
        (Flake8UnusedArguments, "002") => (RuleGroup::Stable, rules::flake8_unused_arguments::rules::UnusedMethodArgument),
        (Flake8UnusedArguments, "003") => (RuleGroup::Stable, rules::flake8_unused_arguments::rules::UnusedClassMethodArgument),
        (Flake8UnusedArguments, "004") => (RuleGroup::Stable, rules::flake8_unused_arguments::rules::UnusedStaticMethodArgument),
        (Flake8UnusedArguments, "005") => (RuleGroup::Stable, rules::flake8_unused_arguments::rules::UnusedLambdaArgument),

        // flake8-import-conventions
        (Flake8ImportConventions, "001") => (RuleGroup::Stable, rules::flake8_import_conventions::rules::UnconventionalImportAlias),
        (Flake8ImportConventions, "002") => (RuleGroup::Stable, rules::flake8_import_conventions::rules::BannedImportAlias),
        (Flake8ImportConventions, "003") => (RuleGroup::Stable, rules::flake8_import_conventions::rules::BannedImportFrom),

        // flake8-datetimez
        (Flake8Datetimez, "001") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeWithoutTzinfo),
        (Flake8Datetimez, "002") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeToday),
        (Flake8Datetimez, "003") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeUtcnow),
        (Flake8Datetimez, "004") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeUtcfromtimestamp),
        (Flake8Datetimez, "005") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeNowWithoutTzinfo),
        (Flake8Datetimez, "006") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeFromtimestamp),
        (Flake8Datetimez, "007") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDatetimeStrptimeWithoutZone),
        (Flake8Datetimez, "011") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDateToday),
        (Flake8Datetimez, "012") => (RuleGroup::Stable, rules::flake8_datetimez::rules::CallDateFromtimestamp),

        // pygrep-hooks
        (PygrepHooks, "001") => (RuleGroup::Stable, rules::pygrep_hooks::rules::Eval),
        (PygrepHooks, "002") => (RuleGroup::Stable, rules::pygrep_hooks::rules::DeprecatedLogWarn),
        (PygrepHooks, "003") => (RuleGroup::Stable, rules::pygrep_hooks::rules::BlanketTypeIgnore),
        (PygrepHooks, "004") => (RuleGroup::Stable, rules::pygrep_hooks::rules::BlanketNOQA),
        (PygrepHooks, "005") => (RuleGroup::Stable, rules::pygrep_hooks::rules::InvalidMockAccess),

        // pandas-vet
        (PandasVet, "002") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfInplaceArgument),
        (PandasVet, "003") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotIsNull),
        (PandasVet, "004") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotNotNull),
        (PandasVet, "007") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotIx),
        (PandasVet, "008") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotAt),
        (PandasVet, "009") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotIat),
        (PandasVet, "010") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotPivotOrUnstack),
        (PandasVet, "011") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotValues),
        (PandasVet, "012") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotReadTable),
        (PandasVet, "013") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfDotStack),
        (PandasVet, "015") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasUseOfPdMerge),
        (PandasVet, "101") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasNuniqueConstantSeriesCheck),
        (PandasVet, "901") => (RuleGroup::Stable, rules::pandas_vet::rules::PandasDfVariableName),

        // flake8-errmsg
        (Flake8ErrMsg, "101") => (RuleGroup::Stable, rules::flake8_errmsg::rules::RawStringInException),
        (Flake8ErrMsg, "102") => (RuleGroup::Stable, rules::flake8_errmsg::rules::FStringInException),
        (Flake8ErrMsg, "103") => (RuleGroup::Stable, rules::flake8_errmsg::rules::DotFormatInException),

        // flake8-pyi
        (Flake8Pyi, "001") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnprefixedTypeParam),
        (Flake8Pyi, "002") => (RuleGroup::Stable, rules::flake8_pyi::rules::ComplexIfStatementInStub),
        (Flake8Pyi, "003") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnrecognizedVersionInfoCheck),
        (Flake8Pyi, "004") => (RuleGroup::Stable, rules::flake8_pyi::rules::PatchVersionComparison),
        (Flake8Pyi, "005") => (RuleGroup::Stable, rules::flake8_pyi::rules::WrongTupleLengthVersionComparison),
        (Flake8Pyi, "006") => (RuleGroup::Stable, rules::flake8_pyi::rules::BadVersionInfoComparison),
        (Flake8Pyi, "007") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnrecognizedPlatformCheck),
        (Flake8Pyi, "008") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnrecognizedPlatformName),
        (Flake8Pyi, "009") => (RuleGroup::Stable, rules::flake8_pyi::rules::PassStatementStubBody),
        (Flake8Pyi, "010") => (RuleGroup::Stable, rules::flake8_pyi::rules::NonEmptyStubBody),
        (Flake8Pyi, "011") => (RuleGroup::Stable, rules::flake8_pyi::rules::TypedArgumentDefaultInStub),
        (Flake8Pyi, "012") => (RuleGroup::Stable, rules::flake8_pyi::rules::PassInClassBody),
        (Flake8Pyi, "013") => (RuleGroup::Stable, rules::flake8_pyi::rules::EllipsisInNonEmptyClassBody),
        (Flake8Pyi, "014") => (RuleGroup::Stable, rules::flake8_pyi::rules::ArgumentDefaultInStub),
        (Flake8Pyi, "015") => (RuleGroup::Stable, rules::flake8_pyi::rules::AssignmentDefaultInStub),
        (Flake8Pyi, "016") => (RuleGroup::Stable, rules::flake8_pyi::rules::DuplicateUnionMember),
        (Flake8Pyi, "017") => (RuleGroup::Stable, rules::flake8_pyi::rules::ComplexAssignmentInStub),
        (Flake8Pyi, "018") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnusedPrivateTypeVar),
        (Flake8Pyi, "019") => (RuleGroup::Stable, rules::flake8_pyi::rules::CustomTypeVarReturnType),
        (Flake8Pyi, "020") => (RuleGroup::Stable, rules::flake8_pyi::rules::QuotedAnnotationInStub),
        (Flake8Pyi, "021") => (RuleGroup::Stable, rules::flake8_pyi::rules::DocstringInStub),
        (Flake8Pyi, "024") => (RuleGroup::Stable, rules::flake8_pyi::rules::CollectionsNamedTuple),
        (Flake8Pyi, "025") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnaliasedCollectionsAbcSetImport),
        (Flake8Pyi, "026") => (RuleGroup::Stable, rules::flake8_pyi::rules::TypeAliasWithoutAnnotation),
        (Flake8Pyi, "029") => (RuleGroup::Stable, rules::flake8_pyi::rules::StrOrReprDefinedInStub),
        (Flake8Pyi, "030") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnnecessaryLiteralUnion),
        (Flake8Pyi, "032") => (RuleGroup::Stable, rules::flake8_pyi::rules::AnyEqNeAnnotation),
        (Flake8Pyi, "033") => (RuleGroup::Stable, rules::flake8_pyi::rules::TypeCommentInStub),
        (Flake8Pyi, "034") => (RuleGroup::Stable, rules::flake8_pyi::rules::NonSelfReturnType),
        (Flake8Pyi, "035") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnassignedSpecialVariableInStub),
        (Flake8Pyi, "036") => (RuleGroup::Stable, rules::flake8_pyi::rules::BadExitAnnotation),
        (Flake8Pyi, "041") => (RuleGroup::Stable, rules::flake8_pyi::rules::RedundantNumericUnion),
        (Flake8Pyi, "042") => (RuleGroup::Stable, rules::flake8_pyi::rules::SnakeCaseTypeAlias),
        (Flake8Pyi, "043") => (RuleGroup::Stable, rules::flake8_pyi::rules::TSuffixedTypeAlias),
        (Flake8Pyi, "044") => (RuleGroup::Stable, rules::flake8_pyi::rules::FutureAnnotationsInStub),
        (Flake8Pyi, "045") => (RuleGroup::Stable, rules::flake8_pyi::rules::IterMethodReturnIterable),
        (Flake8Pyi, "046") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnusedPrivateProtocol),
        (Flake8Pyi, "047") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnusedPrivateTypeAlias),
        (Flake8Pyi, "048") => (RuleGroup::Stable, rules::flake8_pyi::rules::StubBodyMultipleStatements),
        (Flake8Pyi, "049") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnusedPrivateTypedDict),
        (Flake8Pyi, "050") => (RuleGroup::Stable, rules::flake8_pyi::rules::NoReturnArgumentAnnotationInStub),
        (Flake8Pyi, "051") => (RuleGroup::Stable, rules::flake8_pyi::rules::RedundantLiteralUnion),
        (Flake8Pyi, "052") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnannotatedAssignmentInStub),
        (Flake8Pyi, "054") => (RuleGroup::Stable, rules::flake8_pyi::rules::NumericLiteralTooLong),
        (Flake8Pyi, "053") => (RuleGroup::Stable, rules::flake8_pyi::rules::StringOrBytesTooLong),
        (Flake8Pyi, "055") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnnecessaryTypeUnion),
        (Flake8Pyi, "056") => (RuleGroup::Stable, rules::flake8_pyi::rules::UnsupportedMethodCallOnAll),

        // flake8-pytest-style
        (Flake8PytestStyle, "001") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestFixtureIncorrectParenthesesStyle),
        (Flake8PytestStyle, "002") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestFixturePositionalArgs),
        (Flake8PytestStyle, "003") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestExtraneousScopeFunction),
        (Flake8PytestStyle, "004") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestMissingFixtureNameUnderscore),
        (Flake8PytestStyle, "005") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestIncorrectFixtureNameUnderscore),
        (Flake8PytestStyle, "006") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestParametrizeNamesWrongType),
        (Flake8PytestStyle, "007") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestParametrizeValuesWrongType),
        (Flake8PytestStyle, "008") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestPatchWithLambda),
        (Flake8PytestStyle, "009") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestUnittestAssertion),
        (Flake8PytestStyle, "010") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestRaisesWithoutException),
        (Flake8PytestStyle, "011") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestRaisesTooBroad),
        (Flake8PytestStyle, "012") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestRaisesWithMultipleStatements),
        (Flake8PytestStyle, "013") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestIncorrectPytestImport),
        (Flake8PytestStyle, "014") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestDuplicateParametrizeTestCases),
        (Flake8PytestStyle, "015") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestAssertAlwaysFalse),
        (Flake8PytestStyle, "016") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestFailWithoutMessage),
        (Flake8PytestStyle, "017") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestAssertInExcept),
        (Flake8PytestStyle, "018") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestCompositeAssertion),
        (Flake8PytestStyle, "019") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestFixtureParamWithoutValue),
        (Flake8PytestStyle, "020") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestDeprecatedYieldFixture),
        (Flake8PytestStyle, "021") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestFixtureFinalizerCallback),
        (Flake8PytestStyle, "022") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestUselessYieldFixture),
        (Flake8PytestStyle, "023") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestIncorrectMarkParenthesesStyle),
        (Flake8PytestStyle, "024") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestUnnecessaryAsyncioMarkOnFixture),
        (Flake8PytestStyle, "025") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestErroneousUseFixturesOnFixture),
        (Flake8PytestStyle, "026") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestUseFixturesWithoutParameters),
        (Flake8PytestStyle, "027") => (RuleGroup::Stable, rules::flake8_pytest_style::rules::PytestUnittestRaisesAssertion),

        // flake8-pie
        (Flake8Pie, "790") => (RuleGroup::Stable, rules::flake8_pie::rules::UnnecessaryPass),
        (Flake8Pie, "794") => (RuleGroup::Stable, rules::flake8_pie::rules::DuplicateClassFieldDefinition),
        (Flake8Pie, "796") => (RuleGroup::Stable, rules::flake8_pie::rules::NonUniqueEnums),
        (Flake8Pie, "800") => (RuleGroup::Stable, rules::flake8_pie::rules::UnnecessarySpread),
        (Flake8Pie, "804") => (RuleGroup::Stable, rules::flake8_pie::rules::UnnecessaryDictKwargs),
        (Flake8Pie, "807") => (RuleGroup::Stable, rules::flake8_pie::rules::ReimplementedListBuiltin),
        (Flake8Pie, "808") => (RuleGroup::Stable, rules::flake8_pie::rules::UnnecessaryRangeStart),
        (Flake8Pie, "810") => (RuleGroup::Stable, rules::flake8_pie::rules::MultipleStartsEndsWith),

        // flake8-commas
        (Flake8Commas, "812") => (RuleGroup::Stable, rules::flake8_commas::rules::MissingTrailingComma),
        (Flake8Commas, "818") => (RuleGroup::Stable, rules::flake8_commas::rules::TrailingCommaOnBareTuple),
        (Flake8Commas, "819") => (RuleGroup::Stable, rules::flake8_commas::rules::ProhibitedTrailingComma),

        // flake8-no-pep420
        (Flake8NoPep420, "001") => (RuleGroup::Stable, rules::flake8_no_pep420::rules::ImplicitNamespacePackage),

        // flake8-executable
        (Flake8Executable, "001") => (RuleGroup::Stable, rules::flake8_executable::rules::ShebangNotExecutable),
        (Flake8Executable, "002") => (RuleGroup::Stable, rules::flake8_executable::rules::ShebangMissingExecutableFile),
        (Flake8Executable, "003") => (RuleGroup::Stable, rules::flake8_executable::rules::ShebangMissingPython),
        (Flake8Executable, "004") => (RuleGroup::Stable, rules::flake8_executable::rules::ShebangLeadingWhitespace),
        (Flake8Executable, "005") => (RuleGroup::Stable, rules::flake8_executable::rules::ShebangNotFirstLine),

        // flake8-type-checking
        (Flake8TypeChecking, "001") => (RuleGroup::Stable, rules::flake8_type_checking::rules::TypingOnlyFirstPartyImport),
        (Flake8TypeChecking, "002") => (RuleGroup::Stable, rules::flake8_type_checking::rules::TypingOnlyThirdPartyImport),
        (Flake8TypeChecking, "003") => (RuleGroup::Stable, rules::flake8_type_checking::rules::TypingOnlyStandardLibraryImport),
        (Flake8TypeChecking, "004") => (RuleGroup::Stable, rules::flake8_type_checking::rules::RuntimeImportInTypeCheckingBlock),
        (Flake8TypeChecking, "005") => (RuleGroup::Stable, rules::flake8_type_checking::rules::EmptyTypeCheckingBlock),

        // tryceratops
        (Tryceratops, "002") => (RuleGroup::Stable, rules::tryceratops::rules::RaiseVanillaClass),
        (Tryceratops, "003") => (RuleGroup::Stable, rules::tryceratops::rules::RaiseVanillaArgs),
        (Tryceratops, "004") => (RuleGroup::Stable, rules::tryceratops::rules::TypeCheckWithoutTypeError),
        (Tryceratops, "200") => (RuleGroup::Stable, rules::tryceratops::rules::ReraiseNoCause),
        (Tryceratops, "201") => (RuleGroup::Stable, rules::tryceratops::rules::VerboseRaise),
        (Tryceratops, "300") => (RuleGroup::Stable, rules::tryceratops::rules::TryConsiderElse),
        (Tryceratops, "301") => (RuleGroup::Stable, rules::tryceratops::rules::RaiseWithinTry),
        (Tryceratops, "302") => (RuleGroup::Stable, rules::tryceratops::rules::UselessTryExcept),
        (Tryceratops, "400") => (RuleGroup::Stable, rules::tryceratops::rules::ErrorInsteadOfException),
        (Tryceratops, "401") => (RuleGroup::Stable, rules::tryceratops::rules::VerboseLogMessage),

        // flake8-use-pathlib
        (Flake8UsePathlib, "100") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathAbspath),
        (Flake8UsePathlib, "101") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsChmod),
        (Flake8UsePathlib, "102") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsMkdir),
        (Flake8UsePathlib, "103") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsMakedirs),
        (Flake8UsePathlib, "104") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsRename),
        (Flake8UsePathlib, "105") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsReplace),
        (Flake8UsePathlib, "106") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsRmdir),
        (Flake8UsePathlib, "107") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsRemove),
        (Flake8UsePathlib, "108") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsUnlink),
        (Flake8UsePathlib, "109") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsGetcwd),
        (Flake8UsePathlib, "110") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathExists),
        (Flake8UsePathlib, "111") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathExpanduser),
        (Flake8UsePathlib, "112") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathIsdir),
        (Flake8UsePathlib, "113") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathIsfile),
        (Flake8UsePathlib, "114") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathIslink),
        (Flake8UsePathlib, "115") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsReadlink),
        (Flake8UsePathlib, "116") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsStat),
        (Flake8UsePathlib, "117") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathIsabs),
        (Flake8UsePathlib, "118") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathJoin),
        (Flake8UsePathlib, "119") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathBasename),
        (Flake8UsePathlib, "120") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathDirname),
        (Flake8UsePathlib, "121") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathSamefile),
        (Flake8UsePathlib, "122") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::OsPathSplitext),
        (Flake8UsePathlib, "123") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::BuiltinOpen),
        (Flake8UsePathlib, "124") => (RuleGroup::Stable, rules::flake8_use_pathlib::violations::PyPath),
        (Flake8UsePathlib, "201") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::PathConstructorCurrentDirectory),
        (Flake8UsePathlib, "202") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::OsPathGetsize),
        (Flake8UsePathlib, "202") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::OsPathGetsize),
        (Flake8UsePathlib, "203") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::OsPathGetatime),
        (Flake8UsePathlib, "204") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::OsPathGetmtime),
        (Flake8UsePathlib, "205") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::OsPathGetctime),
        (Flake8UsePathlib, "206") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::OsSepSplit),
        (Flake8UsePathlib, "207") => (RuleGroup::Stable, rules::flake8_use_pathlib::rules::Glob),

        // flake8-logging-format
        (Flake8LoggingFormat, "001") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingStringFormat),
        (Flake8LoggingFormat, "002") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingPercentFormat),
        (Flake8LoggingFormat, "003") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingStringConcat),
        (Flake8LoggingFormat, "004") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingFString),
        (Flake8LoggingFormat, "010") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingWarn),
        (Flake8LoggingFormat, "101") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingExtraAttrClash),
        (Flake8LoggingFormat, "201") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingExcInfo),
        (Flake8LoggingFormat, "202") => (RuleGroup::Stable, rules::flake8_logging_format::violations::LoggingRedundantExcInfo),

        // flake8-raise
        (Flake8Raise, "102") => (RuleGroup::Stable, rules::flake8_raise::rules::UnnecessaryParenOnRaiseException),

        // flake8-self
        (Flake8Self, "001") => (RuleGroup::Stable, rules::flake8_self::rules::PrivateMemberAccess),

        // numpy
        (Numpy, "001") => (RuleGroup::Stable, rules::numpy::rules::NumpyDeprecatedTypeAlias),
        (Numpy, "002") => (RuleGroup::Stable, rules::numpy::rules::NumpyLegacyRandom),
        (Numpy, "003") => (RuleGroup::Stable, rules::numpy::rules::NumpyDeprecatedFunction),

        // ruff
        (Ruff, "001") => (RuleGroup::Stable, rules::ruff::rules::AmbiguousUnicodeCharacterString),
        (Ruff, "002") => (RuleGroup::Stable, rules::ruff::rules::AmbiguousUnicodeCharacterDocstring),
        (Ruff, "003") => (RuleGroup::Stable, rules::ruff::rules::AmbiguousUnicodeCharacterComment),
        (Ruff, "005") => (RuleGroup::Stable, rules::ruff::rules::CollectionLiteralConcatenation),
        (Ruff, "006") => (RuleGroup::Stable, rules::ruff::rules::AsyncioDanglingTask),
        (Ruff, "007") => (RuleGroup::Stable, rules::ruff::rules::PairwiseOverZipped),
        (Ruff, "008") => (RuleGroup::Stable, rules::ruff::rules::MutableDataclassDefault),
        (Ruff, "009") => (RuleGroup::Stable, rules::ruff::rules::FunctionCallInDataclassDefaultArgument),
        (Ruff, "010") => (RuleGroup::Stable, rules::ruff::rules::ExplicitFStringTypeConversion),
        (Ruff, "011") => (RuleGroup::Stable, rules::ruff::rules::StaticKeyDictComprehension),
        (Ruff, "012") => (RuleGroup::Stable, rules::ruff::rules::MutableClassDefault),
        (Ruff, "013") => (RuleGroup::Stable, rules::ruff::rules::ImplicitOptional),
        #[cfg(feature = "unreachable-code")] // When removing this feature gate, also update rules_selector.rs
        #[allow(deprecated)]
        (Ruff, "014") => (RuleGroup::Nursery, rules::ruff::rules::UnreachableCode),
        (Ruff, "015") => (RuleGroup::Stable, rules::ruff::rules::UnnecessaryIterableAllocationForFirstElement),
        (Ruff, "016") => (RuleGroup::Stable, rules::ruff::rules::InvalidIndexType),
        #[allow(deprecated)]
        (Ruff, "017") => (RuleGroup::Nursery, rules::ruff::rules::QuadraticListSummation),
        (Ruff, "018") => (RuleGroup::Preview, rules::ruff::rules::AssignmentInAssert),
        (Ruff, "019") => (RuleGroup::Preview, rules::ruff::rules::UnnecessaryKeyCheck),
        (Ruff, "100") => (RuleGroup::Stable, rules::ruff::rules::UnusedNOQA),
        (Ruff, "200") => (RuleGroup::Stable, rules::ruff::rules::InvalidPyprojectToml),

        // flake8-django
        (Flake8Django, "001") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoNullableModelStringField),
        (Flake8Django, "003") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoLocalsInRenderFunction),
        (Flake8Django, "006") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoExcludeWithModelForm),
        (Flake8Django, "007") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoAllWithModelForm),
        (Flake8Django, "008") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoModelWithoutDunderStr),
        (Flake8Django, "012") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoUnorderedBodyContentInModel),
        (Flake8Django, "013") => (RuleGroup::Stable, rules::flake8_django::rules::DjangoNonLeadingReceiverDecorator),

        // flynt
        // Reserved: (Flynt, "001") => (RuleGroup::Stable, Rule: :StringConcatenationToFString),
        (Flynt, "002") => (RuleGroup::Stable, rules::flynt::rules::StaticJoinToFString),

        // flake8-todos
        (Flake8Todos, "001") => (RuleGroup::Stable, rules::flake8_todos::rules::InvalidTodoTag),
        (Flake8Todos, "002") => (RuleGroup::Stable, rules::flake8_todos::rules::MissingTodoAuthor),
        (Flake8Todos, "003") => (RuleGroup::Stable, rules::flake8_todos::rules::MissingTodoLink),
        (Flake8Todos, "004") => (RuleGroup::Stable, rules::flake8_todos::rules::MissingTodoColon),
        (Flake8Todos, "005") => (RuleGroup::Stable, rules::flake8_todos::rules::MissingTodoDescription),
        (Flake8Todos, "006") => (RuleGroup::Stable, rules::flake8_todos::rules::InvalidTodoCapitalization),
        (Flake8Todos, "007") => (RuleGroup::Stable, rules::flake8_todos::rules::MissingSpaceAfterTodoColon),

        // airflow
        (Airflow, "001") => (RuleGroup::Stable, rules::airflow::rules::AirflowVariableNameTaskIdMismatch),

        // perflint
        (Perflint, "101") => (RuleGroup::Stable, rules::perflint::rules::UnnecessaryListCast),
        (Perflint, "102") => (RuleGroup::Stable, rules::perflint::rules::IncorrectDictIterator),
        (Perflint, "203") => (RuleGroup::Stable, rules::perflint::rules::TryExceptInLoop),
        (Perflint, "401") => (RuleGroup::Stable, rules::perflint::rules::ManualListComprehension),
        (Perflint, "402") => (RuleGroup::Stable, rules::perflint::rules::ManualListCopy),
        (Perflint, "403") => (RuleGroup::Preview, rules::perflint::rules::ManualDictComprehension),

        // flake8-fixme
        (Flake8Fixme, "001") => (RuleGroup::Stable, rules::flake8_fixme::rules::LineContainsFixme),
        (Flake8Fixme, "002") => (RuleGroup::Stable, rules::flake8_fixme::rules::LineContainsTodo),
        (Flake8Fixme, "003") => (RuleGroup::Stable, rules::flake8_fixme::rules::LineContainsXxx),
        (Flake8Fixme, "004") => (RuleGroup::Stable, rules::flake8_fixme::rules::LineContainsHack),

        // flake8-slots
        (Flake8Slots, "000") => (RuleGroup::Stable, rules::flake8_slots::rules::NoSlotsInStrSubclass),
        (Flake8Slots, "001") => (RuleGroup::Stable, rules::flake8_slots::rules::NoSlotsInTupleSubclass),
        (Flake8Slots, "002") => (RuleGroup::Stable, rules::flake8_slots::rules::NoSlotsInNamedtupleSubclass),

        // refurb
        (Refurb, "105") => (RuleGroup::Preview, rules::refurb::rules::PrintEmptyString),
        #[allow(deprecated)]
        (Refurb, "113") => (RuleGroup::Nursery, rules::refurb::rules::RepeatedAppend),
        #[allow(deprecated)]
        (Refurb, "131") => (RuleGroup::Nursery, rules::refurb::rules::DeleteFullSlice),
        #[allow(deprecated)]
        (Refurb, "132") => (RuleGroup::Nursery, rules::refurb::rules::CheckAndRemoveFromSet),
        (Refurb, "140") => (RuleGroup::Preview, rules::refurb::rules::ReimplementedStarmap),
        (Refurb, "145") => (RuleGroup::Preview, rules::refurb::rules::SliceCopy),
        (Refurb, "148") => (RuleGroup::Preview, rules::refurb::rules::UnnecessaryEnumerate),
        (Refurb, "171") => (RuleGroup::Preview, rules::refurb::rules::SingleItemMembershipTest),
        (Refurb, "177") => (RuleGroup::Preview, rules::refurb::rules::ImplicitCwd),

        // flake8-logging
        (Flake8Logging, "001") => (RuleGroup::Preview, rules::flake8_logging::rules::DirectLoggerInstantiation),
        (Flake8Logging, "002") => (RuleGroup::Preview, rules::flake8_logging::rules::InvalidGetLoggerArgument),
        (Flake8Logging, "007") => (RuleGroup::Preview, rules::flake8_logging::rules::ExceptionWithoutExcInfo),
        (Flake8Logging, "009") => (RuleGroup::Preview, rules::flake8_logging::rules::UndocumentedWarn),

        _ => return None,
    })
}
