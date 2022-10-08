use std::str::FromStr;

use crate::ast::checks::Primitive;
use anyhow::Result;
use itertools::Itertools;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;

pub const DEFAULT_CHECK_CODES: [CheckCode; 43] = [
    // pycodestyle errors
    CheckCode::E402,
    CheckCode::E501,
    CheckCode::E711,
    CheckCode::E712,
    CheckCode::E713,
    CheckCode::E714,
    CheckCode::E721,
    CheckCode::E722,
    CheckCode::E731,
    CheckCode::E741,
    CheckCode::E742,
    CheckCode::E743,
    CheckCode::E902,
    CheckCode::E999,
    // pycodestyle warnings
    CheckCode::W292,
    // pyflakes
    CheckCode::F401,
    CheckCode::F402,
    CheckCode::F403,
    CheckCode::F404,
    CheckCode::F405,
    CheckCode::F406,
    CheckCode::F407,
    CheckCode::F541,
    CheckCode::F601,
    CheckCode::F602,
    CheckCode::F621,
    CheckCode::F622,
    CheckCode::F631,
    CheckCode::F632,
    CheckCode::F633,
    CheckCode::F634,
    CheckCode::F701,
    CheckCode::F702,
    CheckCode::F704,
    CheckCode::F706,
    CheckCode::F707,
    CheckCode::F722,
    CheckCode::F821,
    CheckCode::F822,
    CheckCode::F823,
    CheckCode::F831,
    CheckCode::F841,
    CheckCode::F901,
];

pub const ALL_CHECK_CODES: [CheckCode; 63] = [
    // pycodestyle errors
    CheckCode::E402,
    CheckCode::E501,
    CheckCode::E711,
    CheckCode::E712,
    CheckCode::E713,
    CheckCode::E714,
    CheckCode::E721,
    CheckCode::E722,
    CheckCode::E731,
    CheckCode::E741,
    CheckCode::E742,
    CheckCode::E743,
    CheckCode::E902,
    CheckCode::E999,
    // pycodestyle warnings
    CheckCode::W292,
    // pyflakes
    CheckCode::F401,
    CheckCode::F402,
    CheckCode::F403,
    CheckCode::F404,
    CheckCode::F405,
    CheckCode::F406,
    CheckCode::F407,
    CheckCode::F541,
    CheckCode::F601,
    CheckCode::F602,
    CheckCode::F621,
    CheckCode::F622,
    CheckCode::F631,
    CheckCode::F632,
    CheckCode::F633,
    CheckCode::F634,
    CheckCode::F701,
    CheckCode::F702,
    CheckCode::F704,
    CheckCode::F706,
    CheckCode::F707,
    CheckCode::F722,
    CheckCode::F821,
    CheckCode::F822,
    CheckCode::F823,
    CheckCode::F831,
    CheckCode::F841,
    CheckCode::F901,
    // flake8-builtins
    CheckCode::A001,
    CheckCode::A002,
    CheckCode::A003,
    // flake8-comprehensions
    CheckCode::C400,
    CheckCode::C401,
    CheckCode::C402,
    CheckCode::C403,
    CheckCode::C404,
    CheckCode::C405,
    CheckCode::C406,
    CheckCode::C408,
    // flake8-super
    CheckCode::SPR001,
    // flake8-print
    CheckCode::T201,
    CheckCode::T203,
    // pyupgrade
    CheckCode::U001,
    CheckCode::U002,
    CheckCode::U003,
    CheckCode::U004,
    CheckCode::U005,
    // Meta
    CheckCode::M001,
];

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash, PartialOrd, Ord)]
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
    // flake8-comprehensions
    C400,
    C401,
    C402,
    C403,
    C404,
    C405,
    C406,
    C408,
    // flake8-super
    SPR001,
    // flake8-print
    T201,
    T203,
    // pyupgrade
    U001,
    U002,
    U003,
    U004,
    U005,
    // Meta
    M001,
}

impl FromStr for CheckCode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            // pycodestyle errors
            "E402" => Ok(CheckCode::E402),
            "E501" => Ok(CheckCode::E501),
            "E711" => Ok(CheckCode::E711),
            "E712" => Ok(CheckCode::E712),
            "E713" => Ok(CheckCode::E713),
            "E714" => Ok(CheckCode::E714),
            "E721" => Ok(CheckCode::E721),
            "E722" => Ok(CheckCode::E722),
            "E731" => Ok(CheckCode::E731),
            "E741" => Ok(CheckCode::E741),
            "E742" => Ok(CheckCode::E742),
            "E743" => Ok(CheckCode::E743),
            "E902" => Ok(CheckCode::E902),
            "E999" => Ok(CheckCode::E999),
            // pycodestyle warnings
            "W292" => Ok(CheckCode::W292),
            // pyflakes
            "F401" => Ok(CheckCode::F401),
            "F402" => Ok(CheckCode::F402),
            "F403" => Ok(CheckCode::F403),
            "F404" => Ok(CheckCode::F404),
            "F405" => Ok(CheckCode::F405),
            "F406" => Ok(CheckCode::F406),
            "F407" => Ok(CheckCode::F407),
            "F541" => Ok(CheckCode::F541),
            "F601" => Ok(CheckCode::F601),
            "F602" => Ok(CheckCode::F602),
            "F621" => Ok(CheckCode::F621),
            "F622" => Ok(CheckCode::F622),
            "F631" => Ok(CheckCode::F631),
            "F632" => Ok(CheckCode::F632),
            "F633" => Ok(CheckCode::F633),
            "F634" => Ok(CheckCode::F634),
            "F701" => Ok(CheckCode::F701),
            "F702" => Ok(CheckCode::F702),
            "F704" => Ok(CheckCode::F704),
            "F706" => Ok(CheckCode::F706),
            "F707" => Ok(CheckCode::F707),
            "F722" => Ok(CheckCode::F722),
            "F821" => Ok(CheckCode::F821),
            "F822" => Ok(CheckCode::F822),
            "F823" => Ok(CheckCode::F823),
            "F831" => Ok(CheckCode::F831),
            "F841" => Ok(CheckCode::F841),
            "F901" => Ok(CheckCode::F901),
            // flake8-builtins
            "A001" => Ok(CheckCode::A001),
            "A002" => Ok(CheckCode::A002),
            "A003" => Ok(CheckCode::A003),
            // flake8-comprehensions
            "C400" => Ok(CheckCode::C400),
            "C401" => Ok(CheckCode::C401),
            "C402" => Ok(CheckCode::C402),
            "C403" => Ok(CheckCode::C403),
            "C404" => Ok(CheckCode::C404),
            "C405" => Ok(CheckCode::C405),
            "C406" => Ok(CheckCode::C406),
            "C408" => Ok(CheckCode::C408),
            // flake8-super
            "SPR001" => Ok(CheckCode::SPR001),
            // flake8-print
            "T201" => Ok(CheckCode::T201),
            "T203" => Ok(CheckCode::T203),
            // pyupgrade
            "U001" => Ok(CheckCode::U001),
            "U002" => Ok(CheckCode::U002),
            "U003" => Ok(CheckCode::U003),
            "U004" => Ok(CheckCode::U004),
            "U005" => Ok(CheckCode::U005),
            // Meta
            "M001" => Ok(CheckCode::M001),
            _ => Err(anyhow::anyhow!("Unknown check code: {s}")),
        }
    }
}

impl CheckCode {
    pub fn as_str(&self) -> &str {
        match self {
            // pycodestyle errors
            CheckCode::E402 => "E402",
            CheckCode::E501 => "E501",
            CheckCode::E711 => "E711",
            CheckCode::E712 => "E712",
            CheckCode::E713 => "E713",
            CheckCode::E714 => "E714",
            CheckCode::E721 => "E721",
            CheckCode::E722 => "E722",
            CheckCode::E731 => "E731",
            CheckCode::E741 => "E741",
            CheckCode::E742 => "E742",
            CheckCode::E743 => "E743",
            CheckCode::E902 => "E902",
            CheckCode::E999 => "E999",
            // pycodestyle warnings
            CheckCode::W292 => "W292",
            // pyflakes
            CheckCode::F401 => "F401",
            CheckCode::F402 => "F402",
            CheckCode::F403 => "F403",
            CheckCode::F404 => "F404",
            CheckCode::F405 => "F405",
            CheckCode::F406 => "F406",
            CheckCode::F407 => "F407",
            CheckCode::F541 => "F541",
            CheckCode::F601 => "F601",
            CheckCode::F602 => "F602",
            CheckCode::F621 => "F621",
            CheckCode::F622 => "F622",
            CheckCode::F631 => "F631",
            CheckCode::F632 => "F632",
            CheckCode::F633 => "F633",
            CheckCode::F634 => "F634",
            CheckCode::F701 => "F701",
            CheckCode::F702 => "F702",
            CheckCode::F704 => "F704",
            CheckCode::F706 => "F706",
            CheckCode::F707 => "F707",
            CheckCode::F722 => "F722",
            CheckCode::F821 => "F821",
            CheckCode::F822 => "F822",
            CheckCode::F823 => "F823",
            CheckCode::F831 => "F831",
            CheckCode::F841 => "F841",
            CheckCode::F901 => "F901",
            // flake8-builtins
            CheckCode::A001 => "A001",
            CheckCode::A002 => "A002",
            CheckCode::A003 => "A003",
            // flake8-comprehensions
            CheckCode::C400 => "C400",
            CheckCode::C401 => "C401",
            CheckCode::C402 => "C402",
            CheckCode::C403 => "C403",
            CheckCode::C404 => "C404",
            CheckCode::C405 => "C405",
            CheckCode::C406 => "C406",
            CheckCode::C408 => "C408",
            // flake8-super
            CheckCode::SPR001 => "SPR001",
            // flake8-print
            CheckCode::T201 => "T201",
            CheckCode::T203 => "T203",
            // pyupgrade
            CheckCode::U001 => "U001",
            CheckCode::U002 => "U002",
            CheckCode::U003 => "U003",
            CheckCode::U004 => "U004",
            CheckCode::U005 => "U005",
            // Meta
            CheckCode::M001 => "M001",
        }
    }

    /// The source for the check (either the AST, the filesystem, or the physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::E501 | CheckCode::M001 => &LintSource::Lines,
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
            // pyflakes
            CheckCode::F401 => CheckKind::UnusedImport(vec!["...".to_string()]),
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
            // flake8-comprehensions
            CheckCode::C400 => CheckKind::UnnecessaryGeneratorList,
            CheckCode::C401 => CheckKind::UnnecessaryGeneratorSet,
            CheckCode::C402 => CheckKind::UnnecessaryGeneratorDict,
            CheckCode::C403 => CheckKind::UnnecessaryListComprehensionSet,
            CheckCode::C404 => CheckKind::UnnecessaryListComprehensionDict,
            CheckCode::C405 => CheckKind::UnnecessaryLiteralSet("<list/tuple>".to_string()),
            CheckCode::C406 => CheckKind::UnnecessaryLiteralDict("<list/tuple>".to_string()),
            CheckCode::C408 => {
                CheckKind::UnnecessaryCollectionCall("<dict/list/tuple>".to_string())
            }
            // flake8-super
            CheckCode::SPR001 => CheckKind::SuperCallWithParameters,
            // flake8-print
            CheckCode::T201 => CheckKind::PrintFound,
            CheckCode::T203 => CheckKind::PPrintFound,
            // pyupgrade
            CheckCode::U001 => CheckKind::UselessMetaclassType,
            CheckCode::U002 => CheckKind::UnnecessaryAbspath,
            CheckCode::U003 => CheckKind::TypeOfPrimitive(Primitive::Str),
            CheckCode::U004 => CheckKind::UselessObjectInheritance("...".to_string()),
            CheckCode::U005 => CheckKind::NoAssertEquals,
            // Meta
            CheckCode::M001 => CheckKind::UnusedNOQA(None),
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub enum LintSource {
    AST,
    Lines,
    FileSystem,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectedCmpop {
    Eq,
    NotEq,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    AmbiguousClassName(String),
    AmbiguousFunctionName(String),
    AmbiguousVariableName(String),
    AssertTuple,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    DefaultExceptNotLast,
    DoNotAssignLambda,
    DoNotUseBareExcept,
    DuplicateArgumentName,
    ExpressionsInStarAssignment,
    FStringMissingPlaceholders,
    ForwardAnnotationSyntaxError(String),
    FutureFeatureNotDefined(String),
    IOError(String),
    IfTuple,
    ImportShadowedByLoopVar(String, usize),
    ImportStarNotPermitted(String),
    ImportStarUsage(String, Vec<String>),
    ImportStarUsed(String),
    InvalidPrintSyntax,
    IsLiteral,
    LateFutureImport,
    LineTooLong(usize, usize),
    ModuleImportNotAtTopOfFile,
    MultiValueRepeatedKeyLiteral,
    MultiValueRepeatedKeyVariable(String),
    NoneComparison(RejectedCmpop),
    NotInTest,
    NotIsTest,
    RaiseNotImplemented,
    ReturnOutsideFunction,
    SyntaxError(String),
    TrueFalseComparison(bool, RejectedCmpop),
    TwoStarredExpressions,
    TypeComparison,
    UndefinedExport(String),
    UndefinedLocal(String),
    UndefinedName(String),
    UnusedImport(Vec<String>),
    UnusedVariable(String),
    YieldOutsideFunction,
    // More style
    NoNewLineAtEndOfFile,
    // flake8-builtin
    BuiltinVariableShadowing(String),
    BuiltinArgumentShadowing(String),
    BuiltinAttributeShadowing(String),
    // flakes8-comprehensions
    UnnecessaryGeneratorList,
    UnnecessaryGeneratorSet,
    UnnecessaryGeneratorDict,
    UnnecessaryListComprehensionSet,
    UnnecessaryListComprehensionDict,
    UnnecessaryLiteralSet(String),
    UnnecessaryLiteralDict(String),
    UnnecessaryCollectionCall(String),
    // flake8-super
    SuperCallWithParameters,
    // flake8-print
    PrintFound,
    PPrintFound,
    // pyupgrade
    TypeOfPrimitive(Primitive),
    UnnecessaryAbspath,
    UselessMetaclassType,
    NoAssertEquals,
    UselessObjectInheritance(String),
    // Meta
    UnusedNOQA(Option<String>),
}

impl CheckKind {
    /// The name of the check.
    pub fn name(&self) -> &'static str {
        match self {
            CheckKind::AmbiguousClassName(_) => "AmbiguousClassName",
            CheckKind::AmbiguousFunctionName(_) => "AmbiguousFunctionName",
            CheckKind::AmbiguousVariableName(_) => "AmbiguousVariableName",
            CheckKind::AssertTuple => "AssertTuple",
            CheckKind::BreakOutsideLoop => "BreakOutsideLoop",
            CheckKind::ContinueOutsideLoop => "ContinueOutsideLoop",
            CheckKind::DefaultExceptNotLast => "DefaultExceptNotLast",
            CheckKind::DoNotAssignLambda => "DoNotAssignLambda",
            CheckKind::DoNotUseBareExcept => "DoNotUseBareExcept",
            CheckKind::DuplicateArgumentName => "DuplicateArgumentName",
            CheckKind::ExpressionsInStarAssignment => "ExpressionsInStarAssignment",
            CheckKind::FStringMissingPlaceholders => "FStringMissingPlaceholders",
            CheckKind::ForwardAnnotationSyntaxError(_) => "ForwardAnnotationSyntaxError",
            CheckKind::FutureFeatureNotDefined(_) => "FutureFeatureNotDefined",
            CheckKind::IOError(_) => "IOError",
            CheckKind::IfTuple => "IfTuple",
            CheckKind::ImportShadowedByLoopVar(_, _) => "ImportShadowedByLoopVar",
            CheckKind::ImportStarNotPermitted(_) => "ImportStarNotPermitted",
            CheckKind::ImportStarUsage(_, _) => "ImportStarUsage",
            CheckKind::ImportStarUsed(_) => "ImportStarUsed",
            CheckKind::InvalidPrintSyntax => "InvalidPrintSyntax",
            CheckKind::IsLiteral => "IsLiteral",
            CheckKind::LateFutureImport => "LateFutureImport",
            CheckKind::LineTooLong(_, _) => "LineTooLong",
            CheckKind::ModuleImportNotAtTopOfFile => "ModuleImportNotAtTopOfFile",
            CheckKind::MultiValueRepeatedKeyLiteral => "MultiValueRepeatedKeyLiteral",
            CheckKind::MultiValueRepeatedKeyVariable(_) => "MultiValueRepeatedKeyVariable",
            CheckKind::NoneComparison(_) => "NoneComparison",
            CheckKind::NotInTest => "NotInTest",
            CheckKind::NotIsTest => "NotIsTest",
            CheckKind::RaiseNotImplemented => "RaiseNotImplemented",
            CheckKind::ReturnOutsideFunction => "ReturnOutsideFunction",
            CheckKind::SyntaxError(_) => "SyntaxError",
            CheckKind::TrueFalseComparison(_, _) => "TrueFalseComparison",
            CheckKind::TwoStarredExpressions => "TwoStarredExpressions",
            CheckKind::TypeComparison => "TypeComparison",
            CheckKind::UndefinedExport(_) => "UndefinedExport",
            CheckKind::UndefinedLocal(_) => "UndefinedLocal",
            CheckKind::UndefinedName(_) => "UndefinedName",
            CheckKind::UnusedImport(_) => "UnusedImport",
            CheckKind::UnusedVariable(_) => "UnusedVariable",
            CheckKind::YieldOutsideFunction => "YieldOutsideFunction",
            // More style
            CheckKind::NoNewLineAtEndOfFile => "NoNewLineAtEndOfFile",
            // flake8-builtins
            CheckKind::BuiltinVariableShadowing(_) => "BuiltinVariableShadowing",
            CheckKind::BuiltinArgumentShadowing(_) => "BuiltinArgumentShadowing",
            CheckKind::BuiltinAttributeShadowing(_) => "BuiltinAttributeShadowing",
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => "UnnecessaryGeneratorList",
            CheckKind::UnnecessaryGeneratorSet => "UnnecessaryGeneratorSet",
            CheckKind::UnnecessaryGeneratorDict => "UnnecessaryGeneratorDict",
            CheckKind::UnnecessaryListComprehensionSet => "UnnecessaryListComprehensionSet",
            CheckKind::UnnecessaryListComprehensionDict => "UnnecessaryListComprehensionDict",
            CheckKind::UnnecessaryLiteralSet(_) => "UnnecessaryLiteralSet",
            CheckKind::UnnecessaryLiteralDict(_) => "UnnecessaryLiteralDict",
            CheckKind::UnnecessaryCollectionCall(_) => "UnnecessaryCollectionCall",
            // flake8-super
            CheckKind::SuperCallWithParameters => "SuperCallWithParameters",
            // flake8-print
            CheckKind::PrintFound => "PrintFound",
            CheckKind::PPrintFound => "PPrintFound",
            // pyupgrade
            CheckKind::TypeOfPrimitive(_) => "TypeOfPrimitive",
            CheckKind::UnnecessaryAbspath => "UnnecessaryAbspath",
            CheckKind::UselessMetaclassType => "UselessMetaclassType",
            CheckKind::NoAssertEquals => "NoAssertEquals",
            CheckKind::UselessObjectInheritance(_) => "UselessObjectInheritance",
            // Meta
            CheckKind::UnusedNOQA(_) => "UnusedNOQA",
        }
    }

    /// A four-letter shorthand code for the check.
    pub fn code(&self) -> &'static CheckCode {
        match self {
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
            CheckKind::ImportShadowedByLoopVar(_, _) => &CheckCode::F402,
            CheckKind::ImportStarNotPermitted(_) => &CheckCode::F406,
            CheckKind::ImportStarUsage(_, _) => &CheckCode::F405,
            CheckKind::ImportStarUsed(_) => &CheckCode::F403,
            CheckKind::InvalidPrintSyntax => &CheckCode::F633,
            CheckKind::IsLiteral => &CheckCode::F632,
            CheckKind::LateFutureImport => &CheckCode::F404,
            CheckKind::LineTooLong(_, _) => &CheckCode::E501,
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
            CheckKind::TrueFalseComparison(_, _) => &CheckCode::E712,
            CheckKind::TwoStarredExpressions => &CheckCode::F622,
            CheckKind::TypeComparison => &CheckCode::E721,
            CheckKind::UndefinedExport(_) => &CheckCode::F822,
            CheckKind::UndefinedLocal(_) => &CheckCode::F823,
            CheckKind::UndefinedName(_) => &CheckCode::F821,
            CheckKind::UnusedImport(_) => &CheckCode::F401,
            CheckKind::UnusedVariable(_) => &CheckCode::F841,
            CheckKind::YieldOutsideFunction => &CheckCode::F704,
            // More style
            CheckKind::NoNewLineAtEndOfFile => &CheckCode::W292,
            // flake8-builtins
            CheckKind::BuiltinVariableShadowing(_) => &CheckCode::A001,
            CheckKind::BuiltinArgumentShadowing(_) => &CheckCode::A002,
            CheckKind::BuiltinAttributeShadowing(_) => &CheckCode::A003,
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => &CheckCode::C400,
            CheckKind::UnnecessaryGeneratorSet => &CheckCode::C401,
            CheckKind::UnnecessaryGeneratorDict => &CheckCode::C402,
            CheckKind::UnnecessaryListComprehensionSet => &CheckCode::C403,
            CheckKind::UnnecessaryListComprehensionDict => &CheckCode::C404,
            CheckKind::UnnecessaryLiteralSet(_) => &CheckCode::C405,
            CheckKind::UnnecessaryLiteralDict(_) => &CheckCode::C406,
            CheckKind::UnnecessaryCollectionCall(_) => &CheckCode::C408,
            // flake8-super
            CheckKind::SuperCallWithParameters => &CheckCode::SPR001,
            // flake8-print
            CheckKind::PrintFound => &CheckCode::T201,
            CheckKind::PPrintFound => &CheckCode::T203,
            // pyupgrade
            CheckKind::TypeOfPrimitive(_) => &CheckCode::U003,
            CheckKind::UnnecessaryAbspath => &CheckCode::U002,
            CheckKind::UselessMetaclassType => &CheckCode::U001,
            CheckKind::NoAssertEquals => &CheckCode::U005,
            CheckKind::UselessObjectInheritance(_) => &CheckCode::U004,
            // Meta
            CheckKind::UnusedNOQA(_) => &CheckCode::M001,
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> String {
        match self {
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
            CheckKind::UnusedImport(names) => {
                let names = names.iter().map(|name| format!("`{name}`")).join(", ");
                format!("{names} imported but unused")
            }
            CheckKind::UnusedVariable(name) => {
                format!("Local variable `{name}` is assigned to but never used")
            }
            CheckKind::YieldOutsideFunction => {
                "`yield` or `yield from` statement outside of a function/method".to_string()
            }
            // More style
            CheckKind::NoNewLineAtEndOfFile => "No newline at end of file".to_string(),
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
            // flake8-comprehensions
            CheckKind::UnnecessaryGeneratorList => {
                "Unnecessary generator - rewrite as a list comprehension".to_string()
            }
            CheckKind::UnnecessaryGeneratorSet => {
                "Unnecessary generator - rewrite as a set comprehension".to_string()
            }
            CheckKind::UnnecessaryGeneratorDict => {
                "Unnecessary generator - rewrite as a dict comprehension".to_string()
            }
            CheckKind::UnnecessaryListComprehensionSet => {
                "Unnecessary list comprehension - rewrite as a set comprehension".to_string()
            }
            CheckKind::UnnecessaryListComprehensionDict => {
                "Unnecessary list comprehension - rewrite as a dict comprehension".to_string()
            }
            CheckKind::UnnecessaryLiteralSet(obj_type) => {
                format!("Unnecessary {obj_type} literal - rewrite as a set literal")
            }
            CheckKind::UnnecessaryLiteralDict(obj_type) => {
                format!("Unnecessary {obj_type} literal - rewrite as a dict literal")
            }
            CheckKind::UnnecessaryCollectionCall(obj_type) => {
                format!("Unnecessary {obj_type} call - rewrite as a literal")
            }
            // flake8-super
            CheckKind::SuperCallWithParameters => {
                "Use `super()` instead of `super(__class__, self)`".to_string()
            }
            // flake8-print
            CheckKind::PrintFound => "`print` found".to_string(),
            CheckKind::PPrintFound => "`pprint` found".to_string(),
            // pyupgrade
            CheckKind::TypeOfPrimitive(primitive) => {
                format!("Use `{}` instead of `type(...)`", primitive.builtin())
            }
            CheckKind::UnnecessaryAbspath => {
                "`abspath(__file__)` is unnecessary in Python 3.9 and later".to_string()
            }
            CheckKind::UselessMetaclassType => "`__metaclass__ = type` is implied".to_string(),
            CheckKind::NoAssertEquals => {
                "`assertEquals` is deprecated, use `assertEqual` instead".to_string()
            }
            CheckKind::UselessObjectInheritance(name) => {
                format!("Class `{name}` inherits from object")
            }
            // Meta
            CheckKind::UnusedNOQA(code) => match code {
                None => "Unused `noqa` directive".to_string(),
                Some(code) => format!("Unused `noqa` directive for: {code}"),
            },
        }
    }

    /// Whether the check kind is (potentially) fixable.
    pub fn fixable(&self) -> bool {
        matches!(
            self,
            CheckKind::NoAssertEquals
                | CheckKind::PPrintFound
                | CheckKind::PrintFound
                | CheckKind::SuperCallWithParameters
                | CheckKind::TypeOfPrimitive(_)
                | CheckKind::UnnecessaryAbspath
                | CheckKind::UnusedImport(_)
                | CheckKind::UnusedNOQA(_)
                | CheckKind::UselessMetaclassType
                | CheckKind::UselessObjectInheritance(_)
        )
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    pub content: String,
    pub location: Location,
    pub end_location: Location,
    pub applied: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<Fix>,
}

impl Check {
    pub fn new(kind: CheckKind, span: Range) -> Self {
        Self {
            kind,
            location: span.location,
            end_location: span.end_location,
            fix: None,
        }
    }

    pub fn amend(&mut self, fix: Fix) {
        self.fix = Some(fix);
    }
}
