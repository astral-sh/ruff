use std::str::FromStr;

use itertools::Itertools;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumIter, EnumString};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::flake8_quotes::settings::Quote;
use crate::pyupgrade::types::Primitive;

#[derive(
    AsRefStr,
    EnumIter,
    EnumString,
    Debug,
    PartialEq,
    Eq,
    Clone,
    Serialize,
    Deserialize,
    Hash,
    PartialOrd,
    Ord,
)]
pub enum CheckCode {
    // pycodestyle errors
    E402,
    E501,
    E711,
    E712,
    E713,
    E714,
    E721,
    E722,
    E731,
    E741,
    E742,
    E743,
    E902,
    E999,
    // pycodestyle warnings
    W292,
    W605,
    // pyflakes
    F401,
    F402,
    F403,
    F404,
    F405,
    F406,
    F407,
    F541,
    F601,
    F602,
    F621,
    F622,
    F631,
    F632,
    F633,
    F634,
    F701,
    F702,
    F704,
    F706,
    F707,
    F722,
    F821,
    F822,
    F823,
    F831,
    F841,
    F901,
    // flake8-builtins
    A001,
    A002,
    A003,
    // flake8-bugbear
    B002,
    B006,
    B007,
    B011,
    B013,
    B014,
    B017,
    B025,
    // flake8-comprehensions
    C400,
    C401,
    C402,
    C403,
    C404,
    C405,
    C406,
    C408,
    C409,
    C410,
    C411,
    C413,
    C414,
    C415,
    C416,
    C417,
    // flake8-print
    T201,
    T203,
    // flake8-quotes
    Q000,
    Q001,
    Q002,
    Q003,
    // pyupgrade
    U001,
    U002,
    U003,
    U004,
    U005,
    U006,
    U007,
    U008,
    // pydocstyle
    D100,
    D101,
    D102,
    D103,
    D104,
    D105,
    D106,
    D107,
    D200,
    D201,
    D202,
    D203,
    D204,
    D205,
    D206,
    D207,
    D208,
    D209,
    D210,
    D211,
    D212,
    D213,
    D214,
    D215,
    D300,
    D400,
    D402,
    D403,
    D404,
    D405,
    D406,
    D407,
    D408,
    D409,
    D410,
    D411,
    D412,
    D413,
    D414,
    D415,
    D416,
    D417,
    D418,
    D419,
    // pep8-naming
    N801,
    N802,
    N803,
    N804,
    N805,
    N806,
    N807,
    N811,
    N812,
    N813,
    N814,
    N815,
    N816,
    N817,
    N818,
    // Meta
    M001,
}

#[derive(EnumIter, Debug, PartialEq, Eq)]
pub enum CheckCategory {
    Pyflakes,
    PycodestyleError,
    PycodestyleWarning,
    Pydocstyle,
    Pyupgrade,
    PEP8Naming,
    Flake8Comprehensions,
    Flake8Bugbear,
    Flake8Builtins,
    Flake8Print,
    Flake8Quotes,
    Meta,
}

impl CheckCategory {
    pub fn title(&self) -> &'static str {
        match self {
            CheckCategory::PycodestyleError => "pycodestyle (error)",
            CheckCategory::PycodestyleWarning => "pycodestyle (warning)",
            CheckCategory::Pyflakes => "Pyflakes",
            CheckCategory::Flake8Builtins => "flake8-builtins",
            CheckCategory::Flake8Bugbear => "flake8-bugbear",
            CheckCategory::Flake8Comprehensions => "flake8-comprehensions",
            CheckCategory::Flake8Print => "flake8-print",
            CheckCategory::Flake8Quotes => "flake8-quotes",
            CheckCategory::Pyupgrade => "pyupgrade",
            CheckCategory::Pydocstyle => "pydocstyle",
            CheckCategory::PEP8Naming => "pep8-naming",
            CheckCategory::Meta => "Meta rules",
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub enum LintSource {
    AST,
    FileSystem,
    Lines,
    Tokens,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectedCmpop {
    Eq,
    NotEq,
}

#[derive(AsRefStr, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    // pycodestyle errors
    AmbiguousClassName(String),
    AmbiguousFunctionName(String),
    AmbiguousVariableName(String),
    DoNotAssignLambda,
    DoNotUseBareExcept,
    IOError(String),
    LineTooLong(usize, usize),
    ModuleImportNotAtTopOfFile,
    NoneComparison(RejectedCmpop),
    NotInTest,
    NotIsTest,
    SyntaxError(String),
    TrueFalseComparison(bool, RejectedCmpop),
    TypeComparison,
    // pycodestyle warnings
    NoNewLineAtEndOfFile,
    InvalidEscapeSequence(char),
    // pyflakes
    AssertTuple,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    DefaultExceptNotLast,
    DuplicateArgumentName,
    ExpressionsInStarAssignment,
    FStringMissingPlaceholders,
    ForwardAnnotationSyntaxError(String),
    FutureFeatureNotDefined(String),
    IfTuple,
    ImportShadowedByLoopVar(String, usize),
    ImportStarNotPermitted(String),
    ImportStarUsage(String, Vec<String>),
    ImportStarUsed(String),
    InvalidPrintSyntax,
    IsLiteral,
    LateFutureImport,
    MultiValueRepeatedKeyLiteral,
    MultiValueRepeatedKeyVariable(String),
    RaiseNotImplemented,
    ReturnOutsideFunction,
    TwoStarredExpressions,
    UndefinedExport(String),
    UndefinedLocal(String),
    UndefinedName(String),
    UnusedImport(Vec<String>, bool),
    UnusedVariable(String),
    YieldOutsideFunction,
    // flake8-builtins
    BuiltinVariableShadowing(String),
    BuiltinArgumentShadowing(String),
    BuiltinAttributeShadowing(String),
    // flake8-bugbear
    UnaryPrefixIncrement,
    MutableArgumentDefault,
    UnusedLoopControlVariable(String),
    DoNotAssertFalse,
    RedundantTupleInExceptionHandler(String),
    DuplicateHandlerException(Vec<String>),
    NoAssertRaisesException,
    DuplicateTryBlockException(String),
    // flake8-comprehensions
    UnnecessaryGeneratorList,
    UnnecessaryGeneratorSet,
    UnnecessaryGeneratorDict,
    UnnecessaryListComprehensionSet,
    UnnecessaryListComprehensionDict,
    UnnecessaryLiteralSet(String),
    UnnecessaryLiteralDict(String),
    UnnecessaryCollectionCall(String),
    UnnecessaryLiteralWithinTupleCall(String),
    UnnecessaryLiteralWithinListCall(String),
    UnnecessaryListCall,
    UnnecessaryCallAroundSorted(String),
    UnnecessaryDoubleCastOrProcess(String, String),
    UnnecessarySubscriptReversal(String),
    UnnecessaryComprehension(String),
    UnnecessaryMap(String),
    // flake8-print
    PrintFound,
    PPrintFound,
    // flake8-quotes
    BadQuotesInlineString(Quote),
    BadQuotesMultilineString(Quote),
    BadQuotesDocstring(Quote),
    AvoidQuoteEscape,
    // pyupgrade
    TypeOfPrimitive(Primitive),
    UnnecessaryAbspath,
    UselessMetaclassType,
    DeprecatedUnittestAlias(String, String),
    UselessObjectInheritance(String),
    UsePEP585Annotation(String),
    UsePEP604Annotation,
    SuperCallWithParameters,
    // pydocstyle
    BlankLineAfterLastSection(String),
    BlankLineAfterSection(String),
    BlankLineAfterSummary,
    BlankLineBeforeSection(String),
    CapitalizeSectionName(String),
    DashedUnderlineAfterSection(String),
    DocumentAllArguments(Vec<String>),
    EndsInPeriod,
    EndsInPunctuation,
    FirstLineCapitalized,
    FitsOnOneLine,
    IndentWithSpaces,
    MagicMethod,
    MultiLineSummaryFirstLine,
    MultiLineSummarySecondLine,
    NewLineAfterLastParagraph,
    NewLineAfterSectionName(String),
    NoBlankLineAfterFunction(usize),
    NoBlankLineBeforeClass(usize),
    NoBlankLineBeforeFunction(usize),
    NoBlankLinesBetweenHeaderAndContent(String),
    NoOverIndentation,
    NoSignature,
    NoSurroundingWhitespace,
    NoThisPrefix,
    NoUnderIndentation,
    NonEmpty,
    NonEmptySection(String),
    OneBlankLineAfterClass(usize),
    OneBlankLineBeforeClass(usize),
    PublicClass,
    PublicFunction,
    PublicInit,
    PublicMethod,
    PublicModule,
    PublicNestedClass,
    PublicPackage,
    SectionNameEndsInColon(String),
    SectionNotOverIndented(String),
    SectionUnderlineAfterName(String),
    SectionUnderlineMatchesSectionLength(String),
    SectionUnderlineNotOverIndented(String),
    SkipDocstring,
    UsesTripleQuotes,
    // pep8-naming
    InvalidClassName(String),
    InvalidFunctionName(String),
    InvalidArgumentName(String),
    InvalidFirstArgumentNameForClassMethod,
    InvalidFirstArgumentNameForMethod,
    NonLowercaseVariableInFunction(String),
    DunderFunctionName,
    ConstantImportedAsNonConstant(String, String),
    LowercaseImportedAsNonLowercase(String, String),
    CamelcaseImportedAsLowercase(String, String),
    CamelcaseImportedAsConstant(String, String),
    MixedCaseVariableInClassScope(String),
    MixedCaseVariableInGlobalScope(String),
    CamelcaseImportedAsAcronym(String, String),
    ErrorSuffixOnExceptionName(String),
    // Meta
    UnusedNOQA(Option<Vec<String>>),
}

impl CheckCode {
    /// The source for the check (either the AST, the filesystem, or the
    /// physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::E501 | CheckCode::W292 | CheckCode::M001 => &LintSource::Lines,
            CheckCode::W605
            | CheckCode::Q000
            | CheckCode::Q001
            | CheckCode::Q002
            | CheckCode::Q003 => &LintSource::Tokens,
            CheckCode::E902 => &LintSource::FileSystem,
            _ => &LintSource::AST,
        }
    }

    /// A placeholder representation of the CheckKind for the check.
    pub fn kind(&self) -> CheckKind {
        match self {
            // pycodestyle errors
            CheckCode::E402 => CheckKind::ModuleImportNotAtTopOfFile,
            CheckCode::E501 => CheckKind::LineTooLong(89, 88),
            CheckCode::E711 => CheckKind::NoneComparison(RejectedCmpop::Eq),
            CheckCode::E712 => CheckKind::TrueFalseComparison(true, RejectedCmpop::Eq),
            CheckCode::E713 => CheckKind::NotInTest,
            CheckCode::E714 => CheckKind::NotIsTest,
            CheckCode::E721 => CheckKind::TypeComparison,
            CheckCode::E722 => CheckKind::DoNotUseBareExcept,
            CheckCode::E731 => CheckKind::DoNotAssignLambda,
            CheckCode::E741 => CheckKind::AmbiguousVariableName("...".to_string()),
            CheckCode::E742 => CheckKind::AmbiguousClassName("...".to_string()),
            CheckCode::E743 => CheckKind::AmbiguousFunctionName("...".to_string()),
            CheckCode::E902 => CheckKind::IOError("IOError: `...`".to_string()),
            CheckCode::E999 => CheckKind::SyntaxError("`...`".to_string()),
            // pycodestyle warnings
            CheckCode::W292 => CheckKind::NoNewLineAtEndOfFile,
            CheckCode::W605 => CheckKind::InvalidEscapeSequence('c'),
            // pyflakes
            CheckCode::F401 => CheckKind::UnusedImport(vec!["...".to_string()], false),
            CheckCode::F402 => CheckKind::ImportShadowedByLoopVar("...".to_string(), 1),
            CheckCode::F403 => CheckKind::ImportStarUsed("...".to_string()),
            CheckCode::F404 => CheckKind::LateFutureImport,
            CheckCode::F405 => {
                CheckKind::ImportStarUsage("...".to_string(), vec!["...".to_string()])
            }
            CheckCode::F406 => CheckKind::ImportStarNotPermitted("...".to_string()),
            CheckCode::F407 => CheckKind::FutureFeatureNotDefined("...".to_string()),
            CheckCode::F541 => CheckKind::FStringMissingPlaceholders,
            CheckCode::F601 => CheckKind::MultiValueRepeatedKeyLiteral,
            CheckCode::F602 => CheckKind::MultiValueRepeatedKeyVariable("...".to_string()),
            CheckCode::F621 => CheckKind::ExpressionsInStarAssignment,
            CheckCode::F622 => CheckKind::TwoStarredExpressions,
            CheckCode::F631 => CheckKind::AssertTuple,
            CheckCode::F632 => CheckKind::IsLiteral,
            CheckCode::F633 => CheckKind::InvalidPrintSyntax,
            CheckCode::F634 => CheckKind::IfTuple,
            CheckCode::F701 => CheckKind::BreakOutsideLoop,
            CheckCode::F702 => CheckKind::ContinueOutsideLoop,
            CheckCode::F704 => CheckKind::YieldOutsideFunction,
            CheckCode::F706 => CheckKind::ReturnOutsideFunction,
            CheckCode::F707 => CheckKind::DefaultExceptNotLast,
            CheckCode::F722 => CheckKind::ForwardAnnotationSyntaxError("...".to_string()),
            CheckCode::F821 => CheckKind::UndefinedName("...".to_string()),
            CheckCode::F822 => CheckKind::UndefinedExport("...".to_string()),
            CheckCode::F823 => CheckKind::UndefinedLocal("...".to_string()),
            CheckCode::F831 => CheckKind::DuplicateArgumentName,
            CheckCode::F841 => CheckKind::UnusedVariable("...".to_string()),
            CheckCode::F901 => CheckKind::RaiseNotImplemented,
            // flake8-builtins
            CheckCode::A001 => CheckKind::BuiltinVariableShadowing("...".to_string()),
            CheckCode::A002 => CheckKind::BuiltinArgumentShadowing("...".to_string()),
            CheckCode::A003 => CheckKind::BuiltinAttributeShadowing("...".to_string()),
            // flake8-bugbear
            CheckCode::B002 => CheckKind::UnaryPrefixIncrement,
            CheckCode::B006 => CheckKind::MutableArgumentDefault,
            CheckCode::B007 => CheckKind::UnusedLoopControlVariable("i".to_string()),
            CheckCode::B011 => CheckKind::DoNotAssertFalse,
            CheckCode::B013 => {
                CheckKind::RedundantTupleInExceptionHandler("ValueError".to_string())
            }
            CheckCode::B014 => CheckKind::DuplicateHandlerException(vec!["ValueError".to_string()]),
            CheckCode::B017 => CheckKind::NoAssertRaisesException,
            CheckCode::B025 => CheckKind::DuplicateTryBlockException("Exception".to_string()),
            // flake8-comprehensions
            CheckCode::C400 => CheckKind::UnnecessaryGeneratorList,
            CheckCode::C401 => CheckKind::UnnecessaryGeneratorSet,
            CheckCode::C402 => CheckKind::UnnecessaryGeneratorDict,
            CheckCode::C403 => CheckKind::UnnecessaryListComprehensionSet,
            CheckCode::C404 => CheckKind::UnnecessaryListComprehensionDict,
            CheckCode::C405 => CheckKind::UnnecessaryLiteralSet("(list|tuple)".to_string()),
            CheckCode::C406 => CheckKind::UnnecessaryLiteralDict("(list|tuple)".to_string()),
            CheckCode::C408 => {
                CheckKind::UnnecessaryCollectionCall("(dict|list|tuple)".to_string())
            }
            CheckCode::C409 => {
                CheckKind::UnnecessaryLiteralWithinTupleCall("(list|tuple)".to_string())
            }
            CheckCode::C410 => {
                CheckKind::UnnecessaryLiteralWithinListCall("(list|tuple)".to_string())
            }
            CheckCode::C411 => CheckKind::UnnecessaryListCall,
            CheckCode::C413 => {
                CheckKind::UnnecessaryCallAroundSorted("(list|reversed)".to_string())
            }
            CheckCode::C414 => CheckKind::UnnecessaryDoubleCastOrProcess(
                "(list|reversed|set|sorted|tuple)".to_string(),
                "(list|set|sorted|tuple)".to_string(),
            ),
            CheckCode::C415 => {
                CheckKind::UnnecessarySubscriptReversal("(reversed|set|sorted)".to_string())
            }
            CheckCode::C416 => CheckKind::UnnecessaryComprehension("(list|set)".to_string()),
            CheckCode::C417 => CheckKind::UnnecessaryMap("(list|set|dict)".to_string()),
            // flake8-print
            CheckCode::T201 => CheckKind::PrintFound,
            CheckCode::T203 => CheckKind::PPrintFound,
            // flake8-quotes
            CheckCode::Q000 => CheckKind::BadQuotesInlineString(Quote::Double),
            CheckCode::Q001 => CheckKind::BadQuotesMultilineString(Quote::Double),
            CheckCode::Q002 => CheckKind::BadQuotesDocstring(Quote::Double),
            CheckCode::Q003 => CheckKind::AvoidQuoteEscape,
            // pyupgrade
            CheckCode::U001 => CheckKind::UselessMetaclassType,
            CheckCode::U002 => CheckKind::UnnecessaryAbspath,
            CheckCode::U003 => CheckKind::TypeOfPrimitive(Primitive::Str),
            CheckCode::U004 => CheckKind::UselessObjectInheritance("...".to_string()),
            CheckCode::U005 => CheckKind::DeprecatedUnittestAlias(
                "assertEquals".to_string(),
                "assertEqual".to_string(),
            ),
            CheckCode::U006 => CheckKind::UsePEP585Annotation("List".to_string()),
            CheckCode::U007 => CheckKind::UsePEP604Annotation,
            CheckCode::U008 => CheckKind::SuperCallWithParameters,
            // pydocstyle
            CheckCode::D100 => CheckKind::PublicModule,
            CheckCode::D101 => CheckKind::PublicClass,
            CheckCode::D102 => CheckKind::PublicMethod,
            CheckCode::D103 => CheckKind::PublicFunction,
            CheckCode::D104 => CheckKind::PublicPackage,
            CheckCode::D105 => CheckKind::MagicMethod,
            CheckCode::D106 => CheckKind::PublicNestedClass,
            CheckCode::D107 => CheckKind::PublicInit,
            CheckCode::D200 => CheckKind::FitsOnOneLine,
            CheckCode::D201 => CheckKind::NoBlankLineBeforeFunction(1),
            CheckCode::D202 => CheckKind::NoBlankLineAfterFunction(1),
            CheckCode::D203 => CheckKind::OneBlankLineBeforeClass(0),
            CheckCode::D204 => CheckKind::OneBlankLineAfterClass(0),
            CheckCode::D205 => CheckKind::BlankLineAfterSummary,
            CheckCode::D206 => CheckKind::IndentWithSpaces,
            CheckCode::D207 => CheckKind::NoUnderIndentation,
            CheckCode::D208 => CheckKind::NoOverIndentation,
            CheckCode::D209 => CheckKind::NewLineAfterLastParagraph,
            CheckCode::D210 => CheckKind::NoSurroundingWhitespace,
            CheckCode::D211 => CheckKind::NoBlankLineBeforeClass(1),
            CheckCode::D212 => CheckKind::MultiLineSummaryFirstLine,
            CheckCode::D213 => CheckKind::MultiLineSummarySecondLine,
            CheckCode::D214 => CheckKind::SectionNotOverIndented("Returns".to_string()),
            CheckCode::D215 => CheckKind::SectionUnderlineNotOverIndented("Returns".to_string()),
            CheckCode::D300 => CheckKind::UsesTripleQuotes,
            CheckCode::D400 => CheckKind::EndsInPeriod,
            CheckCode::D402 => CheckKind::NoSignature,
            CheckCode::D403 => CheckKind::FirstLineCapitalized,
            CheckCode::D404 => CheckKind::NoThisPrefix,
            CheckCode::D405 => CheckKind::CapitalizeSectionName("returns".to_string()),
            CheckCode::D406 => CheckKind::NewLineAfterSectionName("Returns".to_string()),
            CheckCode::D407 => CheckKind::DashedUnderlineAfterSection("Returns".to_string()),
            CheckCode::D408 => CheckKind::SectionUnderlineAfterName("Returns".to_string()),
            CheckCode::D409 => {
                CheckKind::SectionUnderlineMatchesSectionLength("Returns".to_string())
            }
            CheckCode::D410 => CheckKind::BlankLineAfterSection("Returns".to_string()),
            CheckCode::D411 => CheckKind::BlankLineBeforeSection("Returns".to_string()),
            CheckCode::D412 => {
                CheckKind::NoBlankLinesBetweenHeaderAndContent("Returns".to_string())
            }
            CheckCode::D413 => CheckKind::BlankLineAfterLastSection("Returns".to_string()),
            CheckCode::D414 => CheckKind::NonEmptySection("Returns".to_string()),
            CheckCode::D415 => CheckKind::EndsInPunctuation,
            CheckCode::D416 => CheckKind::SectionNameEndsInColon("Returns".to_string()),
            CheckCode::D417 => {
                CheckKind::DocumentAllArguments(vec!["x".to_string(), "y".to_string()])
            }
            CheckCode::D418 => CheckKind::SkipDocstring,
            CheckCode::D419 => CheckKind::NonEmpty,
            // pep8-naming
            CheckCode::N801 => CheckKind::InvalidClassName("...".to_string()),
            CheckCode::N802 => CheckKind::InvalidFunctionName("...".to_string()),
            CheckCode::N803 => CheckKind::InvalidArgumentName("...".to_string()),
            CheckCode::N804 => CheckKind::InvalidFirstArgumentNameForClassMethod,
            CheckCode::N805 => CheckKind::InvalidFirstArgumentNameForMethod,
            CheckCode::N806 => CheckKind::NonLowercaseVariableInFunction("...".to_string()),
            CheckCode::N807 => CheckKind::DunderFunctionName,
            CheckCode::N811 => {
                CheckKind::ConstantImportedAsNonConstant("...".to_string(), "...".to_string())
            }
            CheckCode::N812 => {
                CheckKind::LowercaseImportedAsNonLowercase("...".to_string(), "...".to_string())
            }
            CheckCode::N813 => {
                CheckKind::CamelcaseImportedAsLowercase("...".to_string(), "...".to_string())
            }
            CheckCode::N814 => {
                CheckKind::CamelcaseImportedAsConstant("...".to_string(), "...".to_string())
            }
            CheckCode::N815 => CheckKind::MixedCaseVariableInClassScope("mixedCase".to_string()),
            CheckCode::N816 => CheckKind::MixedCaseVariableInGlobalScope("mixedCase".to_string()),
            CheckCode::N817 => {
                CheckKind::CamelcaseImportedAsAcronym("...".to_string(), "...".to_string())
            }
            CheckCode::N818 => CheckKind::ErrorSuffixOnExceptionName("...".to_string()),
            // Meta
            CheckCode::M001 => CheckKind::UnusedNOQA(None),
        }
    }

    pub fn category(&self) -> CheckCategory {
        match self {
            CheckCode::E402 => CheckCategory::PycodestyleError,
            CheckCode::E501 => CheckCategory::PycodestyleError,
            CheckCode::E711 => CheckCategory::PycodestyleError,
            CheckCode::E712 => CheckCategory::PycodestyleError,
            CheckCode::E713 => CheckCategory::PycodestyleError,
            CheckCode::E714 => CheckCategory::PycodestyleError,
            CheckCode::E721 => CheckCategory::PycodestyleError,
            CheckCode::E722 => CheckCategory::PycodestyleError,
            CheckCode::E731 => CheckCategory::PycodestyleError,
            CheckCode::E741 => CheckCategory::PycodestyleError,
            CheckCode::E742 => CheckCategory::PycodestyleError,
            CheckCode::E743 => CheckCategory::PycodestyleError,
            CheckCode::E902 => CheckCategory::PycodestyleError,
            CheckCode::E999 => CheckCategory::PycodestyleError,
            CheckCode::W292 => CheckCategory::PycodestyleWarning,
            CheckCode::W605 => CheckCategory::PycodestyleWarning,
            CheckCode::F401 => CheckCategory::Pyflakes,
            CheckCode::F402 => CheckCategory::Pyflakes,
            CheckCode::F403 => CheckCategory::Pyflakes,
            CheckCode::F404 => CheckCategory::Pyflakes,
            CheckCode::F405 => CheckCategory::Pyflakes,
            CheckCode::F406 => CheckCategory::Pyflakes,
            CheckCode::F407 => CheckCategory::Pyflakes,
            CheckCode::F541 => CheckCategory::Pyflakes,
            CheckCode::F601 => CheckCategory::Pyflakes,
            CheckCode::F602 => CheckCategory::Pyflakes,
            CheckCode::F621 => CheckCategory::Pyflakes,
            CheckCode::F622 => CheckCategory::Pyflakes,
            CheckCode::F631 => CheckCategory::Pyflakes,
            CheckCode::F632 => CheckCategory::Pyflakes,
            CheckCode::F633 => CheckCategory::Pyflakes,
            CheckCode::F634 => CheckCategory::Pyflakes,
            CheckCode::F701 => CheckCategory::Pyflakes,
            CheckCode::F702 => CheckCategory::Pyflakes,
            CheckCode::F704 => CheckCategory::Pyflakes,
            CheckCode::F706 => CheckCategory::Pyflakes,
            CheckCode::F707 => CheckCategory::Pyflakes,
            CheckCode::F722 => CheckCategory::Pyflakes,
            CheckCode::F821 => CheckCategory::Pyflakes,
            CheckCode::F822 => CheckCategory::Pyflakes,
            CheckCode::F823 => CheckCategory::Pyflakes,
            CheckCode::F831 => CheckCategory::Pyflakes,
            CheckCode::F841 => CheckCategory::Pyflakes,
            CheckCode::F901 => CheckCategory::Pyflakes,
            CheckCode::A001 => CheckCategory::Flake8Builtins,
            CheckCode::A002 => CheckCategory::Flake8Builtins,
            CheckCode::A003 => CheckCategory::Flake8Builtins,
            CheckCode::B002 => CheckCategory::Flake8Bugbear,
            CheckCode::B006 => CheckCategory::Flake8Bugbear,
            CheckCode::B007 => CheckCategory::Flake8Bugbear,
            CheckCode::B011 => CheckCategory::Flake8Bugbear,
            CheckCode::B013 => CheckCategory::Flake8Bugbear,
            CheckCode::B014 => CheckCategory::Flake8Bugbear,
            CheckCode::B017 => CheckCategory::Flake8Bugbear,
            CheckCode::B025 => CheckCategory::Flake8Bugbear,
            CheckCode::C400 => CheckCategory::Flake8Comprehensions,
            CheckCode::C401 => CheckCategory::Flake8Comprehensions,
            CheckCode::C402 => CheckCategory::Flake8Comprehensions,
            CheckCode::C403 => CheckCategory::Flake8Comprehensions,
            CheckCode::C404 => CheckCategory::Flake8Comprehensions,
            CheckCode::C405 => CheckCategory::Flake8Comprehensions,
            CheckCode::C406 => CheckCategory::Flake8Comprehensions,
            CheckCode::C408 => CheckCategory::Flake8Comprehensions,
            CheckCode::C409 => CheckCategory::Flake8Comprehensions,
            CheckCode::C410 => CheckCategory::Flake8Comprehensions,
            CheckCode::C411 => CheckCategory::Flake8Comprehensions,
            CheckCode::C413 => CheckCategory::Flake8Comprehensions,
            CheckCode::C414 => CheckCategory::Flake8Comprehensions,
            CheckCode::C415 => CheckCategory::Flake8Comprehensions,
            CheckCode::C416 => CheckCategory::Flake8Comprehensions,
            CheckCode::C417 => CheckCategory::Flake8Comprehensions,
            CheckCode::T201 => CheckCategory::Flake8Print,
            CheckCode::T203 => CheckCategory::Flake8Print,
            CheckCode::Q000 => CheckCategory::Flake8Quotes,
            CheckCode::Q001 => CheckCategory::Flake8Quotes,
            CheckCode::Q002 => CheckCategory::Flake8Quotes,
            CheckCode::Q003 => CheckCategory::Flake8Quotes,
            CheckCode::U001 => CheckCategory::Pyupgrade,
            CheckCode::U002 => CheckCategory::Pyupgrade,
            CheckCode::U003 => CheckCategory::Pyupgrade,
            CheckCode::U004 => CheckCategory::Pyupgrade,
            CheckCode::U005 => CheckCategory::Pyupgrade,
            CheckCode::U006 => CheckCategory::Pyupgrade,
            CheckCode::U007 => CheckCategory::Pyupgrade,
            CheckCode::U008 => CheckCategory::Pyupgrade,
            CheckCode::D100 => CheckCategory::Pydocstyle,
            CheckCode::D101 => CheckCategory::Pydocstyle,
            CheckCode::D102 => CheckCategory::Pydocstyle,
            CheckCode::D103 => CheckCategory::Pydocstyle,
            CheckCode::D104 => CheckCategory::Pydocstyle,
            CheckCode::D105 => CheckCategory::Pydocstyle,
            CheckCode::D106 => CheckCategory::Pydocstyle,
            CheckCode::D107 => CheckCategory::Pydocstyle,
            CheckCode::D200 => CheckCategory::Pydocstyle,
            CheckCode::D201 => CheckCategory::Pydocstyle,
            CheckCode::D202 => CheckCategory::Pydocstyle,
            CheckCode::D203 => CheckCategory::Pydocstyle,
            CheckCode::D204 => CheckCategory::Pydocstyle,
            CheckCode::D205 => CheckCategory::Pydocstyle,
            CheckCode::D206 => CheckCategory::Pydocstyle,
            CheckCode::D207 => CheckCategory::Pydocstyle,
            CheckCode::D208 => CheckCategory::Pydocstyle,
            CheckCode::D209 => CheckCategory::Pydocstyle,
            CheckCode::D210 => CheckCategory::Pydocstyle,
            CheckCode::D211 => CheckCategory::Pydocstyle,
            CheckCode::D212 => CheckCategory::Pydocstyle,
            CheckCode::D213 => CheckCategory::Pydocstyle,
            CheckCode::D214 => CheckCategory::Pydocstyle,
            CheckCode::D215 => CheckCategory::Pydocstyle,
            CheckCode::D300 => CheckCategory::Pydocstyle,
            CheckCode::D400 => CheckCategory::Pydocstyle,
            CheckCode::D402 => CheckCategory::Pydocstyle,
            CheckCode::D403 => CheckCategory::Pydocstyle,
            CheckCode::D404 => CheckCategory::Pydocstyle,
            CheckCode::D405 => CheckCategory::Pydocstyle,
            CheckCode::D406 => CheckCategory::Pydocstyle,
            CheckCode::D407 => CheckCategory::Pydocstyle,
            CheckCode::D408 => CheckCategory::Pydocstyle,
            CheckCode::D409 => CheckCategory::Pydocstyle,
            CheckCode::D410 => CheckCategory::Pydocstyle,
            CheckCode::D411 => CheckCategory::Pydocstyle,
            CheckCode::D412 => CheckCategory::Pydocstyle,
            CheckCode::D413 => CheckCategory::Pydocstyle,
            CheckCode::D414 => CheckCategory::Pydocstyle,
            CheckCode::D415 => CheckCategory::Pydocstyle,
            CheckCode::D416 => CheckCategory::Pydocstyle,
            CheckCode::D417 => CheckCategory::Pydocstyle,
            CheckCode::D418 => CheckCategory::Pydocstyle,
            CheckCode::D419 => CheckCategory::Pydocstyle,
            CheckCode::N801 => CheckCategory::PEP8Naming,
            CheckCode::N802 => CheckCategory::PEP8Naming,
            CheckCode::N803 => CheckCategory::PEP8Naming,
            CheckCode::N804 => CheckCategory::PEP8Naming,
            CheckCode::N805 => CheckCategory::PEP8Naming,
            CheckCode::N806 => CheckCategory::PEP8Naming,
            CheckCode::N807 => CheckCategory::PEP8Naming,
            CheckCode::N811 => CheckCategory::PEP8Naming,
            CheckCode::N812 => CheckCategory::PEP8Naming,
            CheckCode::N813 => CheckCategory::PEP8Naming,
            CheckCode::N814 => CheckCategory::PEP8Naming,
            CheckCode::N815 => CheckCategory::PEP8Naming,
            CheckCode::N816 => CheckCategory::PEP8Naming,
            CheckCode::N817 => CheckCategory::PEP8Naming,
            CheckCode::N818 => CheckCategory::PEP8Naming,
            CheckCode::M001 => CheckCategory::Meta,
        }
    }
}

impl CheckKind {
    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static CheckCode {
        match self {
            // pycodestyle errors
            CheckKind::AmbiguousClassName(_) => &CheckCode::E742,
            CheckKind::AmbiguousFunctionName(_) => &CheckCode::E743,
            CheckKind::AmbiguousVariableName(_) => &CheckCode::E741,
            CheckKind::AssertTuple => &CheckCode::F631,
            CheckKind::BreakOutsideLoop => &CheckCode::F701,
            CheckKind::ContinueOutsideLoop => &CheckCode::F702,
            CheckKind::DefaultExceptNotLast => &CheckCode::F707,
            CheckKind::DoNotAssignLambda => &CheckCode::E731,
            CheckKind::DoNotUseBareExcept => &CheckCode::E722,
            CheckKind::DuplicateArgumentName => &CheckCode::F831,
            CheckKind::FStringMissingPlaceholders => &CheckCode::F541,
            CheckKind::ForwardAnnotationSyntaxError(_) => &CheckCode::F722,
            CheckKind::FutureFeatureNotDefined(_) => &CheckCode::F407,
            CheckKind::IOError(_) => &CheckCode::E902,
            CheckKind::IfTuple => &CheckCode::F634,
            CheckKind::ImportShadowedByLoopVar(..) => &CheckCode::F402,
            CheckKind::ImportStarNotPermitted(_) => &CheckCode::F406,
            CheckKind::ImportStarUsage(..) => &CheckCode::F405,
            CheckKind::ImportStarUsed(_) => &CheckCode::F403,
            CheckKind::InvalidPrintSyntax => &CheckCode::F633,
            CheckKind::IsLiteral => &CheckCode::F632,
            CheckKind::LateFutureImport => &CheckCode::F404,
            CheckKind::LineTooLong(..) => &CheckCode::E501,
            CheckKind::ModuleImportNotAtTopOfFile => &CheckCode::E402,
            CheckKind::MultiValueRepeatedKeyLiteral => &CheckCode::F601,
            CheckKind::MultiValueRepeatedKeyVariable(_) => &CheckCode::F602,
            CheckKind::NoneComparison(_) => &CheckCode::E711,
            CheckKind::NotInTest => &CheckCode::E713,
            CheckKind::NotIsTest => &CheckCode::E714,
            CheckKind::RaiseNotImplemented => &CheckCode::F901,
            CheckKind::ReturnOutsideFunction => &CheckCode::F706,
            CheckKind::SyntaxError(_) => &CheckCode::E999,
            CheckKind::ExpressionsInStarAssignment => &CheckCode::F621,
            CheckKind::TrueFalseComparison(..) => &CheckCode::E712,
            CheckKind::TwoStarredExpressions => &CheckCode::F622,
            CheckKind::TypeComparison => &CheckCode::E721,
            CheckKind::UndefinedExport(_) => &CheckCode::F822,
            CheckKind::UndefinedLocal(_) => &CheckCode::F823,
            CheckKind::UndefinedName(_) => &CheckCode::F821,
            CheckKind::UnusedImport(..) => &CheckCode::F401,
            CheckKind::UnusedVariable(_) => &CheckCode::F841,
            CheckKind::YieldOutsideFunction => &CheckCode::F704,
            // pycodestyle warnings
            CheckKind::NoNewLineAtEndOfFile => &CheckCode::W292,
            CheckKind::InvalidEscapeSequence(_) => &CheckCode::W605,
            // flake8-builtins
            CheckKind::BuiltinVariableShadowing(_) => &CheckCode::A001,
            CheckKind::BuiltinArgumentShadowing(_) => &CheckCode::A002,
            CheckKind::BuiltinAttributeShadowing(_) => &CheckCode::A003,
            // flake8-bugbear
            CheckKind::UnaryPrefixIncrement => &CheckCode::B002,
            CheckKind::MutableArgumentDefault => &CheckCode::B006,
            CheckKind::UnusedLoopControlVariable(_) => &CheckCode::B007,
            CheckKind::DoNotAssertFalse => &CheckCode::B011,
            CheckKind::RedundantTupleInExceptionHandler(_) => &CheckCode::B013,
            CheckKind::DuplicateHandlerException(_) => &CheckCode::B014,
            CheckKind::NoAssertRaisesException => &CheckCode::B017,
            CheckKind::DuplicateTryBlockException(_) => &CheckCode::B025,
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => &CheckCode::C400,
            CheckKind::UnnecessaryGeneratorSet => &CheckCode::C401,
            CheckKind::UnnecessaryGeneratorDict => &CheckCode::C402,
            CheckKind::UnnecessaryListComprehensionSet => &CheckCode::C403,
            CheckKind::UnnecessaryListComprehensionDict => &CheckCode::C404,
            CheckKind::UnnecessaryLiteralSet(_) => &CheckCode::C405,
            CheckKind::UnnecessaryLiteralDict(_) => &CheckCode::C406,
            CheckKind::UnnecessaryCollectionCall(_) => &CheckCode::C408,
            CheckKind::UnnecessaryLiteralWithinTupleCall(..) => &CheckCode::C409,
            CheckKind::UnnecessaryLiteralWithinListCall(..) => &CheckCode::C410,
            CheckKind::UnnecessaryListCall => &CheckCode::C411,
            CheckKind::UnnecessaryCallAroundSorted(_) => &CheckCode::C413,
            CheckKind::UnnecessaryDoubleCastOrProcess(..) => &CheckCode::C414,
            CheckKind::UnnecessarySubscriptReversal(_) => &CheckCode::C415,
            CheckKind::UnnecessaryComprehension(..) => &CheckCode::C416,
            CheckKind::UnnecessaryMap(_) => &CheckCode::C417,
            // flake8-print
            CheckKind::PrintFound => &CheckCode::T201,
            CheckKind::PPrintFound => &CheckCode::T203,
            // flake8-quotes
            CheckKind::BadQuotesInlineString(_) => &CheckCode::Q000,
            CheckKind::BadQuotesMultilineString(_) => &CheckCode::Q001,
            CheckKind::BadQuotesDocstring(_) => &CheckCode::Q002,
            CheckKind::AvoidQuoteEscape => &CheckCode::Q003,
            // pyupgrade
            CheckKind::TypeOfPrimitive(_) => &CheckCode::U003,
            CheckKind::UnnecessaryAbspath => &CheckCode::U002,
            CheckKind::UselessMetaclassType => &CheckCode::U001,
            CheckKind::DeprecatedUnittestAlias(..) => &CheckCode::U005,
            CheckKind::UsePEP585Annotation(_) => &CheckCode::U006,
            CheckKind::UsePEP604Annotation => &CheckCode::U007,
            CheckKind::UselessObjectInheritance(_) => &CheckCode::U004,
            CheckKind::SuperCallWithParameters => &CheckCode::U008,
            // pydocstyle
            CheckKind::BlankLineAfterLastSection(_) => &CheckCode::D413,
            CheckKind::BlankLineAfterSection(_) => &CheckCode::D410,
            CheckKind::BlankLineBeforeSection(_) => &CheckCode::D411,
            CheckKind::CapitalizeSectionName(_) => &CheckCode::D405,
            CheckKind::DashedUnderlineAfterSection(_) => &CheckCode::D407,
            CheckKind::DocumentAllArguments(_) => &CheckCode::D417,
            CheckKind::EndsInPeriod => &CheckCode::D400,
            CheckKind::EndsInPunctuation => &CheckCode::D415,
            CheckKind::FirstLineCapitalized => &CheckCode::D403,
            CheckKind::FitsOnOneLine => &CheckCode::D200,
            CheckKind::IndentWithSpaces => &CheckCode::D206,
            CheckKind::MagicMethod => &CheckCode::D105,
            CheckKind::MultiLineSummaryFirstLine => &CheckCode::D212,
            CheckKind::MultiLineSummarySecondLine => &CheckCode::D213,
            CheckKind::NewLineAfterLastParagraph => &CheckCode::D209,
            CheckKind::NewLineAfterSectionName(_) => &CheckCode::D406,
            CheckKind::NoBlankLineAfterFunction(_) => &CheckCode::D202,
            CheckKind::BlankLineAfterSummary => &CheckCode::D205,
            CheckKind::NoBlankLineBeforeClass(_) => &CheckCode::D211,
            CheckKind::NoBlankLineBeforeFunction(_) => &CheckCode::D201,
            CheckKind::NoBlankLinesBetweenHeaderAndContent(_) => &CheckCode::D412,
            CheckKind::NoOverIndentation => &CheckCode::D208,
            CheckKind::NoSignature => &CheckCode::D402,
            CheckKind::NoSurroundingWhitespace => &CheckCode::D210,
            CheckKind::NoThisPrefix => &CheckCode::D404,
            CheckKind::NoUnderIndentation => &CheckCode::D207,
            CheckKind::NonEmpty => &CheckCode::D419,
            CheckKind::NonEmptySection(_) => &CheckCode::D414,
            CheckKind::OneBlankLineAfterClass(_) => &CheckCode::D204,
            CheckKind::OneBlankLineBeforeClass(_) => &CheckCode::D203,
            CheckKind::PublicClass => &CheckCode::D101,
            CheckKind::PublicFunction => &CheckCode::D103,
            CheckKind::PublicInit => &CheckCode::D107,
            CheckKind::PublicMethod => &CheckCode::D102,
            CheckKind::PublicModule => &CheckCode::D100,
            CheckKind::PublicNestedClass => &CheckCode::D106,
            CheckKind::PublicPackage => &CheckCode::D104,
            CheckKind::SectionNameEndsInColon(_) => &CheckCode::D416,
            CheckKind::SectionNotOverIndented(_) => &CheckCode::D214,
            CheckKind::SectionUnderlineAfterName(_) => &CheckCode::D408,
            CheckKind::SectionUnderlineMatchesSectionLength(_) => &CheckCode::D409,
            CheckKind::SectionUnderlineNotOverIndented(_) => &CheckCode::D215,
            CheckKind::SkipDocstring => &CheckCode::D418,
            CheckKind::UsesTripleQuotes => &CheckCode::D300,
            // pep8-naming
            CheckKind::InvalidClassName(_) => &CheckCode::N801,
            CheckKind::InvalidFunctionName(_) => &CheckCode::N802,
            CheckKind::InvalidArgumentName(_) => &CheckCode::N803,
            CheckKind::InvalidFirstArgumentNameForClassMethod => &CheckCode::N804,
            CheckKind::InvalidFirstArgumentNameForMethod => &CheckCode::N805,
            CheckKind::NonLowercaseVariableInFunction(..) => &CheckCode::N806,
            CheckKind::DunderFunctionName => &CheckCode::N807,
            CheckKind::ConstantImportedAsNonConstant(..) => &CheckCode::N811,
            CheckKind::LowercaseImportedAsNonLowercase(..) => &CheckCode::N812,
            CheckKind::CamelcaseImportedAsLowercase(..) => &CheckCode::N813,
            CheckKind::CamelcaseImportedAsConstant(..) => &CheckCode::N814,
            CheckKind::MixedCaseVariableInClassScope(..) => &CheckCode::N815,
            CheckKind::MixedCaseVariableInGlobalScope(..) => &CheckCode::N816,
            CheckKind::CamelcaseImportedAsAcronym(..) => &CheckCode::N817,
            CheckKind::ErrorSuffixOnExceptionName(..) => &CheckCode::N818,
            // Meta
            CheckKind::UnusedNOQA(_) => &CheckCode::M001,
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> String {
        match self {
            // pycodestyle errors
            CheckKind::AmbiguousClassName(name) => {
                format!("Ambiguous class name: `{}`", name)
            }
            CheckKind::AmbiguousFunctionName(name) => {
                format!("Ambiguous function name: `{}`", name)
            }
            CheckKind::AmbiguousVariableName(name) => {
                format!("Ambiguous variable name: `{}`", name)
            }
            CheckKind::AssertTuple => {
                "Assert test is a non-empty tuple, which is always `True`".to_string()
            }
            CheckKind::BreakOutsideLoop => "`break` outside loop".to_string(),
            CheckKind::ContinueOutsideLoop => "`continue` not properly in loop".to_string(),
            CheckKind::DefaultExceptNotLast => {
                "An `except:` block as not the last exception handler".to_string()
            }
            CheckKind::DoNotAssignLambda => {
                "Do not assign a lambda expression, use a def".to_string()
            }
            CheckKind::DoNotUseBareExcept => "Do not use bare `except`".to_string(),
            CheckKind::DuplicateArgumentName => {
                "Duplicate argument name in function definition".to_string()
            }
            CheckKind::ForwardAnnotationSyntaxError(body) => {
                format!("Syntax error in forward annotation: `{body}`")
            }
            CheckKind::FStringMissingPlaceholders => {
                "f-string without any placeholders".to_string()
            }
            CheckKind::FutureFeatureNotDefined(name) => {
                format!("Future feature `{name}` is not defined")
            }
            CheckKind::IOError(message) => message.clone(),
            CheckKind::IfTuple => "If test is a tuple, which is always `True`".to_string(),
            CheckKind::InvalidPrintSyntax => {
                "Use of `>>` is invalid with `print` function".to_string()
            }
            CheckKind::ImportShadowedByLoopVar(name, line) => {
                format!("Import `{name}` from line {line} shadowed by loop variable")
            }
            CheckKind::ImportStarNotPermitted(name) => {
                format!("`from {name} import *` only allowed at module level")
            }
            CheckKind::ImportStarUsed(name) => {
                format!("`from {name} import *` used; unable to detect undefined names")
            }
            CheckKind::ImportStarUsage(name, sources) => {
                let sources = sources
                    .iter()
                    .map(|source| format!("`{}`", source))
                    .join(", ");
                format!("`{name}` may be undefined, or defined from star imports: {sources}")
            }
            CheckKind::IsLiteral => "Use `==` and `!=` to compare constant literals".to_string(),
            CheckKind::LateFutureImport => {
                "`from __future__` imports must occur at the beginning of the file".to_string()
            }
            CheckKind::LineTooLong(length, limit) => {
                format!("Line too long ({length} > {limit} characters)")
            }
            CheckKind::ModuleImportNotAtTopOfFile => {
                "Module level import not at top of file".to_string()
            }
            CheckKind::MultiValueRepeatedKeyLiteral => {
                "Dictionary key literal repeated".to_string()
            }
            CheckKind::MultiValueRepeatedKeyVariable(name) => {
                format!("Dictionary key `{name}` repeated")
            }
            CheckKind::NoneComparison(op) => match op {
                RejectedCmpop::Eq => "Comparison to `None` should be `cond is None`".to_string(),
                RejectedCmpop::NotEq => {
                    "Comparison to `None` should be `cond is not None`".to_string()
                }
            },
            CheckKind::NotInTest => "Test for membership should be `not in`".to_string(),
            CheckKind::NotIsTest => "Test for object identity should be `is not`".to_string(),
            CheckKind::RaiseNotImplemented => {
                "`raise NotImplemented` should be `raise NotImplementedError`".to_string()
            }
            CheckKind::ReturnOutsideFunction => {
                "`return` statement outside of a function/method".to_string()
            }
            CheckKind::SyntaxError(message) => format!("SyntaxError: {message}"),
            CheckKind::ExpressionsInStarAssignment => {
                "Too many expressions in star-unpacking assignment".to_string()
            }
            CheckKind::TrueFalseComparison(value, op) => match *value {
                true => match op {
                    RejectedCmpop::Eq => {
                        "Comparison to `True` should be `cond is True`".to_string()
                    }
                    RejectedCmpop::NotEq => {
                        "Comparison to `True` should be `cond is not True`".to_string()
                    }
                },
                false => match op {
                    RejectedCmpop::Eq => {
                        "Comparison to `False` should be `cond is False`".to_string()
                    }
                    RejectedCmpop::NotEq => {
                        "Comparison to `False` should be `cond is not False`".to_string()
                    }
                },
            },
            CheckKind::TwoStarredExpressions => "Two starred expressions in assignment".to_string(),
            CheckKind::TypeComparison => "Do not compare types, use `isinstance()`".to_string(),
            CheckKind::UndefinedExport(name) => {
                format!("Undefined name `{name}` in `__all__`")
            }
            CheckKind::UndefinedLocal(name) => {
                format!("Local variable `{name}` referenced before assignment")
            }
            CheckKind::UndefinedName(name) => {
                format!("Undefined name `{name}`")
            }
            CheckKind::UnusedImport(names, in_init_py) => {
                let names = names.iter().map(|name| format!("`{name}`")).join(", ");
                if *in_init_py {
                    format!("{names} imported but unused and missing from `__all__`")
                } else {
                    format!("{names} imported but unused")
                }
            }
            CheckKind::UnusedVariable(name) => {
                format!("Local variable `{name}` is assigned to but never used")
            }
            CheckKind::YieldOutsideFunction => {
                "`yield` or `yield from` statement outside of a function".to_string()
            }
            // pycodestyle warnings
            CheckKind::NoNewLineAtEndOfFile => "No newline at end of file".to_string(),
            CheckKind::InvalidEscapeSequence(char) => {
                format!("Invalid escape sequence: '\\{char}'")
            }
            // flake8-builtins
            CheckKind::BuiltinVariableShadowing(name) => {
                format!("Variable `{name}` is shadowing a python builtin")
            }
            CheckKind::BuiltinArgumentShadowing(name) => {
                format!("Argument `{name}` is shadowing a python builtin")
            }
            CheckKind::BuiltinAttributeShadowing(name) => {
                format!("Class attribute `{name}` is shadowing a python builtin")
            }
            // flake8-bugbear
            CheckKind::UnaryPrefixIncrement => "Python does not support the unary prefix \
                                                increment. Writing `++n` is equivalent to \
                                                `+(+(n))`, which equals `n`. You meant `n += 1`."
                .to_string(),
            CheckKind::MutableArgumentDefault => {
                "Do not use mutable data structures for argument defaults.".to_string()
            }
            CheckKind::UnusedLoopControlVariable(name) => format!(
                "Loop control variable `{name}` not used within the loop body. If this is \
                 intended, start the name with an underscore."
            ),
            CheckKind::DoNotAssertFalse => "Do not `assert False` (`python -O` removes these \
                                            calls), raise `AssertionError()`"
                .to_string(),
            CheckKind::RedundantTupleInExceptionHandler(name) => {
                format!(
                    "A length-one tuple literal is redundant. Write `except {name}:` instead of \
                     `except ({name},):`."
                )
            }
            CheckKind::DuplicateHandlerException(names) => {
                if names.len() == 1 {
                    let name = &names[0];
                    format!("Exception handler with duplicate exception: `{name}`")
                } else {
                    let names = names.iter().map(|name| format!("`{name}`")).join(", ");
                    format!("Exception handler with duplicate exceptions: {names}")
                }
            }
            CheckKind::NoAssertRaisesException => {
                "`assertRaises(Exception):` should be considered evil. It can lead to your test \
                 passing even if the code being tested is never executed due to a typo. Either \
                 assert for a more specific exception (builtin or custom), use \
                 `assertRaisesRegex`, or use the context manager form of `assertRaises`."
                    .to_string()
            }
            CheckKind::DuplicateTryBlockException(name) => {
                format!("try-except block with duplicate exception `{name}`")
            }
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => {
                "Unnecessary generator (rewrite as a `list` comprehension)".to_string()
            }
            CheckKind::UnnecessaryGeneratorSet => {
                "Unnecessary generator (rewrite as a `set` comprehension)".to_string()
            }
            CheckKind::UnnecessaryGeneratorDict => {
                "Unnecessary generator (rewrite as a `dict` comprehension)".to_string()
            }
            CheckKind::UnnecessaryListComprehensionSet => {
                "Unnecessary `list` comprehension (rewrite as a `set` comprehension)".to_string()
            }
            CheckKind::UnnecessaryListComprehensionDict => {
                "Unnecessary `list` comprehension (rewrite as a `dict` comprehension)".to_string()
            }
            CheckKind::UnnecessaryLiteralSet(obj_type) => {
                format!("Unnecessary `{obj_type}` literal (rewrite as a `set` literal)")
            }
            CheckKind::UnnecessaryLiteralDict(obj_type) => {
                format!("Unnecessary `{obj_type}` literal (rewrite as a `dict` literal)")
            }
            CheckKind::UnnecessaryCollectionCall(obj_type) => {
                format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
            }
            CheckKind::UnnecessaryLiteralWithinTupleCall(literal) => {
                if literal == "list" {
                    format!(
                        "Unnecessary `{literal}` literal passed to `tuple()` (rewrite as a \
                         `tuple` literal)"
                    )
                } else {
                    format!(
                        "Unnecessary `{literal}` literal passed to `tuple()` (remove the outer \
                         call to `tuple()`)"
                    )
                }
            }
            CheckKind::UnnecessaryLiteralWithinListCall(literal) => {
                if literal == "list" {
                    format!(
                        "Unnecessary `{literal}` literal passed to `list()` (remove the outer \
                         call to `list()`)"
                    )
                } else {
                    format!(
                        "Unnecessary `{literal}` literal passed to `list()` (rewrite as a `list` \
                         literal)"
                    )
                }
            }
            CheckKind::UnnecessaryListCall => {
                "Unnecessary `list` call (remove the outer call to `list()`)".to_string()
            }
            CheckKind::UnnecessaryCallAroundSorted(func) => {
                format!("Unnecessary `{func}` call around `sorted()`")
            }
            CheckKind::UnnecessaryDoubleCastOrProcess(inner, outer) => {
                format!("Unnecessary `{inner}` call within `{outer}()`")
            }
            CheckKind::UnnecessarySubscriptReversal(func) => {
                format!("Unnecessary subscript reversal of iterable within `{func}()`")
            }
            CheckKind::UnnecessaryComprehension(obj_type) => {
                format!("Unnecessary `{obj_type}` comprehension (rewrite using `{obj_type}()`)")
            }
            CheckKind::UnnecessaryMap(obj_type) => {
                if obj_type == "generator" {
                    "Unnecessary `map` usage (rewrite using a generator expression)".to_string()
                } else {
                    format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
                }
            }
            // flake8-print
            CheckKind::PrintFound => "`print` found".to_string(),
            CheckKind::PPrintFound => "`pprint` found".to_string(),
            // flake8-quotes
            CheckKind::BadQuotesInlineString(quote) => match quote {
                Quote::Single => "Double quotes found but single quotes preferred".to_string(),
                Quote::Double => "Single quotes found but double quotes preferred".to_string(),
            },
            CheckKind::BadQuotesMultilineString(quote) => match quote {
                Quote::Single => {
                    "Double quote multiline found but single quotes preferred".to_string()
                }
                Quote::Double => {
                    "Single quote multiline found but double quotes preferred".to_string()
                }
            },
            CheckKind::BadQuotesDocstring(quote) => match quote {
                Quote::Single => {
                    "Double quote docstring found but single quotes preferred".to_string()
                }
                Quote::Double => {
                    "Single quote docstring found but double quotes preferred".to_string()
                }
            },
            CheckKind::AvoidQuoteEscape => {
                "Change outer quotes to avoid escaping inner quotes".to_string()
            }
            // pyupgrade
            CheckKind::TypeOfPrimitive(primitive) => {
                format!("Use `{}` instead of `type(...)`", primitive.builtin())
            }
            CheckKind::UnnecessaryAbspath => {
                "`abspath(__file__)` is unnecessary in Python 3.9 and later".to_string()
            }
            CheckKind::UselessMetaclassType => "`__metaclass__ = type` is implied".to_string(),
            CheckKind::DeprecatedUnittestAlias(alias, target) => {
                format!("`{alias}` is deprecated, use `{target}` instead")
            }
            CheckKind::UselessObjectInheritance(name) => {
                format!("Class `{name}` inherits from object")
            }
            CheckKind::UsePEP585Annotation(name) => {
                format!(
                    "Use `{}` instead of `{}` for type annotations",
                    name.to_lowercase(),
                    name,
                )
            }
            CheckKind::UsePEP604Annotation => "Use `X | Y` for type annotations".to_string(),
            CheckKind::SuperCallWithParameters => {
                "Use `super()` instead of `super(__class__, self)`".to_string()
            }
            // pydocstyle
            CheckKind::FitsOnOneLine => "One-line docstring should fit on one line".to_string(),
            CheckKind::BlankLineAfterSummary => {
                "1 blank line required between summary line and description".to_string()
            }
            CheckKind::NewLineAfterLastParagraph => {
                "Multi-line docstring closing quotes should be on a separate line".to_string()
            }
            CheckKind::NoSurroundingWhitespace => {
                "No whitespaces allowed surrounding docstring text".to_string()
            }
            CheckKind::EndsInPeriod => "First line should end with a period".to_string(),
            CheckKind::NonEmpty => "Docstring is empty".to_string(),
            CheckKind::EndsInPunctuation => "First line should end with a period, question mark, \
                                             or exclamation point"
                .to_string(),
            CheckKind::FirstLineCapitalized => {
                "First word of the first line should be properly capitalized".to_string()
            }
            CheckKind::UsesTripleQuotes => r#"Use """triple double quotes""""#.to_string(),
            CheckKind::MultiLineSummaryFirstLine => {
                "Multi-line docstring summary should start at the first line".to_string()
            }
            CheckKind::MultiLineSummarySecondLine => {
                "Multi-line docstring summary should start at the second line".to_string()
            }
            CheckKind::NoSignature => {
                "First line should not be the function's signature".to_string()
            }
            CheckKind::NoBlankLineBeforeFunction(num_lines) => {
                format!("No blank lines allowed before function docstring (found {num_lines})")
            }
            CheckKind::NoBlankLineAfterFunction(num_lines) => {
                format!("No blank lines allowed after function docstring (found {num_lines})")
            }
            CheckKind::NoBlankLineBeforeClass(_) => {
                "No blank lines allowed before class docstring".to_string()
            }
            CheckKind::OneBlankLineBeforeClass(_) => {
                "1 blank line required before class docstring".to_string()
            }
            CheckKind::OneBlankLineAfterClass(_) => {
                "1 blank line required after class docstring".to_string()
            }
            CheckKind::PublicModule => "Missing docstring in public module".to_string(),
            CheckKind::PublicClass => "Missing docstring in public class".to_string(),
            CheckKind::PublicMethod => "Missing docstring in public method".to_string(),
            CheckKind::PublicFunction => "Missing docstring in public function".to_string(),
            CheckKind::PublicPackage => "Missing docstring in public package".to_string(),
            CheckKind::MagicMethod => "Missing docstring in magic method".to_string(),
            CheckKind::PublicNestedClass => "Missing docstring in public nested class".to_string(),
            CheckKind::PublicInit => "Missing docstring in `__init__`".to_string(),
            CheckKind::NoThisPrefix => {
                "First word of the docstring should not be 'This'".to_string()
            }
            CheckKind::SkipDocstring => {
                "Function decorated with `@overload` shouldn't contain a docstring".to_string()
            }
            CheckKind::CapitalizeSectionName(name) => {
                format!("Section name should be properly capitalized (\"{name}\")")
            }
            CheckKind::BlankLineAfterLastSection(name) => {
                format!("Missing blank line after last section (\"{name}\")")
            }
            CheckKind::BlankLineAfterSection(name) => {
                format!("Missing blank line after section (\"{name}\")")
            }
            CheckKind::BlankLineBeforeSection(name) => {
                format!("Missing blank line before section (\"{name}\")")
            }
            CheckKind::NewLineAfterSectionName(name) => {
                format!("Section name should end with a newline (\"{name}\")")
            }
            CheckKind::DashedUnderlineAfterSection(name) => {
                format!("Missing dashed underline after section (\"{name}\")")
            }
            CheckKind::SectionUnderlineAfterName(name) => {
                format!(
                    "Section underline should be in the line following the section's name \
                     (\"{name}\")"
                )
            }
            CheckKind::SectionUnderlineMatchesSectionLength(name) => {
                format!("Section underline should match the length of its name (\"{name}\")")
            }
            CheckKind::NoBlankLinesBetweenHeaderAndContent(name) => {
                format!(
                    "No blank lines allowed between a section header and its content (\"{name}\")"
                )
            }
            CheckKind::NonEmptySection(name) => format!("Section has no content (\"{name}\")"),
            CheckKind::SectionNotOverIndented(name) => {
                format!("Section is over-indented (\"{name}\")")
            }
            CheckKind::SectionUnderlineNotOverIndented(name) => {
                format!("Section underline is over-indented (\"{name}\")")
            }
            CheckKind::SectionNameEndsInColon(name) => {
                format!("Section name should end with a colon (\"{name}\")")
            }
            CheckKind::DocumentAllArguments(names) => {
                if names.len() == 1 {
                    let name = &names[0];
                    format!("Missing argument description in the docstring: `{name}`")
                } else {
                    let names = names.iter().map(|name| format!("`{name}`")).join(", ");
                    format!("Missing argument descriptions in the docstring: {names}")
                }
            }
            CheckKind::IndentWithSpaces => {
                "Docstring should be indented with spaces, not tabs".to_string()
            }
            CheckKind::NoUnderIndentation => "Docstring is under-indented".to_string(),
            CheckKind::NoOverIndentation => "Docstring is over-indented".to_string(),
            // pep8-naming
            CheckKind::InvalidClassName(name) => {
                format!("Class name `{name}` should use CapWords convention ")
            }
            CheckKind::InvalidFunctionName(name) => {
                format!("Function name `{name}` should be lowercase")
            }
            CheckKind::InvalidArgumentName(name) => {
                format!("Argument name `{name}` should be lowercase")
            }
            CheckKind::InvalidFirstArgumentNameForClassMethod => {
                "First argument of a class method should be named `cls`".to_string()
            }
            CheckKind::InvalidFirstArgumentNameForMethod => {
                "First argument of a method should be named `self`".to_string()
            }
            CheckKind::NonLowercaseVariableInFunction(name) => {
                format!("Variable `{name}` in function should be lowercase")
            }
            CheckKind::DunderFunctionName => {
                "Function name should not start and end with `__`".to_string()
            }
            CheckKind::ConstantImportedAsNonConstant(name, asname) => {
                format!("Constant `{name}` imported as non-constant `{asname}`")
            }
            CheckKind::LowercaseImportedAsNonLowercase(name, asname) => {
                format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
            }
            CheckKind::CamelcaseImportedAsLowercase(name, asname) => {
                format!("Camelcase `{name}` imported as lowercase `{asname}`")
            }
            CheckKind::CamelcaseImportedAsConstant(name, asname) => {
                format!("Camelcase `{name}` imported as constant `{asname}`")
            }
            CheckKind::MixedCaseVariableInClassScope(name) => {
                format!("Variable `{name}` in class scope should not be mixedCase")
            }
            CheckKind::MixedCaseVariableInGlobalScope(name) => {
                format!("Variable `{name}` in global scope should not be mixedCase")
            }
            CheckKind::CamelcaseImportedAsAcronym(name, asname) => {
                format!("Camelcase `{name}` imported as acronym `{asname}`")
            }
            CheckKind::ErrorSuffixOnExceptionName(name) => {
                format!("Exception name `{name}` should be named with an Error suffix")
            }
            // Meta
            CheckKind::UnusedNOQA(codes) => match codes {
                None => "Unused `noqa` directive".to_string(),
                Some(codes) => {
                    let codes = codes
                        .iter()
                        .map(|code| {
                            if CheckCode::from_str(code).is_ok() {
                                code.to_string()
                            } else {
                                format!("{code} (not implemented)")
                            }
                        })
                        .join(", ");
                    format!("Unused `noqa` directive for: {codes}")
                }
            },
        }
    }

    /// The summary text for the check. Typically a truncated form of the body
    /// text.
    pub fn summary(&self) -> String {
        match self {
            CheckKind::UnaryPrefixIncrement => {
                "Python does not support the unary prefix increment.".to_string()
            }
            CheckKind::UnusedLoopControlVariable(name) => {
                format!("Loop control variable `{name}` not used within the loop body.")
            }
            CheckKind::NoAssertRaisesException => {
                "`assertRaises(Exception):` should be considered evil.".to_string()
            }
            _ => self.body(),
        }
    }

    /// Whether the check kind is (potentially) fixable.
    pub fn fixable(&self) -> bool {
        matches!(
            self,
            CheckKind::BlankLineAfterLastSection(_)
                | CheckKind::BlankLineAfterSection(_)
                | CheckKind::BlankLineAfterSummary
                | CheckKind::BlankLineBeforeSection(_)
                | CheckKind::CapitalizeSectionName(_)
                | CheckKind::DashedUnderlineAfterSection(_)
                | CheckKind::DeprecatedUnittestAlias(_, _)
                | CheckKind::DoNotAssertFalse
                | CheckKind::DuplicateHandlerException(_)
                | CheckKind::NewLineAfterLastParagraph
                | CheckKind::NewLineAfterSectionName(_)
                | CheckKind::NoBlankLineAfterFunction(_)
                | CheckKind::NoBlankLineBeforeClass(_)
                | CheckKind::NoBlankLineBeforeFunction(_)
                | CheckKind::NoBlankLinesBetweenHeaderAndContent(_)
                | CheckKind::NoOverIndentation
                | CheckKind::NoSurroundingWhitespace
                | CheckKind::NoUnderIndentation
                | CheckKind::OneBlankLineAfterClass(_)
                | CheckKind::OneBlankLineBeforeClass(_)
                | CheckKind::PPrintFound
                | CheckKind::PrintFound
                | CheckKind::SectionNameEndsInColon(_)
                | CheckKind::SectionNotOverIndented(_)
                | CheckKind::SectionUnderlineAfterName(_)
                | CheckKind::SectionUnderlineMatchesSectionLength(_)
                | CheckKind::SectionUnderlineNotOverIndented(_)
                | CheckKind::SuperCallWithParameters
                | CheckKind::TypeOfPrimitive(_)
                | CheckKind::UnnecessaryAbspath
                | CheckKind::UnnecessaryCollectionCall(_)
                | CheckKind::UnnecessaryGeneratorDict
                | CheckKind::UnnecessaryGeneratorList
                | CheckKind::UnnecessaryGeneratorSet
                | CheckKind::UnnecessaryListCall
                | CheckKind::UnnecessaryListComprehensionSet
                | CheckKind::UnnecessaryLiteralSet(_)
                | CheckKind::UnnecessaryLiteralWithinListCall(_)
                | CheckKind::UnnecessaryLiteralWithinTupleCall(_)
                | CheckKind::UnusedImport(_, false)
                | CheckKind::UnusedLoopControlVariable(_)
                | CheckKind::UnusedNOQA(_)
                | CheckKind::UsePEP585Annotation(_)
                | CheckKind::UsePEP604Annotation
                | CheckKind::UselessMetaclassType
                | CheckKind::UselessObjectInheritance(_)
        )
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
}

impl Check {
    pub fn new(kind: CheckKind, range: Range) -> Self {
        Self {
            kind,
            location: range.location,
            end_location: range.end_location,
            fix: None,
        }
    }

    pub fn amend(&mut self, fix: Fix) {
        self.fix = Some(fix);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use anyhow::Result;
    use strum::IntoEnumIterator;

    use crate::checks::CheckCode;

    #[test]
    fn check_code_serialization() -> Result<()> {
        for check_code in CheckCode::iter() {
            assert!(
                CheckCode::from_str(check_code.as_ref()).is_ok(),
                "{:?} could not be round-trip serialized.",
                check_code
            );
        }
        Ok(())
    }
}
