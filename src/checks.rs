use std::str::FromStr;

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

pub const ALL_CHECK_CODES: [CheckCode; 42] = [
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
    CheckCode::F401,
    CheckCode::F403,
    CheckCode::F404,
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
    CheckCode::R001,
    CheckCode::R002,
];

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum CheckCode {
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
    F401,
    F403,
    F404,
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
    R001,
    R002,
}

impl FromStr for CheckCode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
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
            "F401" => Ok(CheckCode::F401),
            "F403" => Ok(CheckCode::F403),
            "F404" => Ok(CheckCode::F404),
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
            "R001" => Ok(CheckCode::R001),
            "R002" => Ok(CheckCode::R002),
            _ => Err(anyhow::anyhow!("Unknown check code: {s}")),
        }
    }
}

impl CheckCode {
    pub fn as_str(&self) -> &str {
        match self {
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
            CheckCode::F401 => "F401",
            CheckCode::F403 => "F403",
            CheckCode::F404 => "F404",
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
            CheckCode::R001 => "R001",
            CheckCode::R002 => "R002",
        }
    }

    /// The source for the check (either the AST, the filesystem, or the physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::E501 => &LintSource::Lines,
            CheckCode::E902 | CheckCode::E999 => &LintSource::FileSystem,
            _ => &LintSource::AST,
        }
    }

    /// A placeholder representation of the CheckKind for the check.
    pub fn kind(&self) -> CheckKind {
        match self {
            CheckCode::E742 => CheckKind::AmbiguousClassName("...".to_string()),
            CheckCode::E743 => CheckKind::AmbiguousFunctionName("...".to_string()),
            CheckCode::E741 => CheckKind::AmbiguousVariableName("...".to_string()),
            CheckCode::F631 => CheckKind::AssertTuple,
            CheckCode::F701 => CheckKind::BreakOutsideLoop,
            CheckCode::F702 => CheckKind::ContinueOutsideLoop,
            CheckCode::F707 => CheckKind::DefaultExceptNotLast,
            CheckCode::E731 => CheckKind::DoNotAssignLambda,
            CheckCode::E722 => CheckKind::DoNotUseBareExcept,
            CheckCode::F831 => CheckKind::DuplicateArgumentName,
            CheckCode::F541 => CheckKind::FStringMissingPlaceholders,
            CheckCode::F722 => CheckKind::ForwardAnnotationSyntaxError("...".to_string()),
            CheckCode::F407 => CheckKind::FutureFeatureNotDefined("...".to_string()),
            CheckCode::E902 => CheckKind::IOError("...".to_string()),
            CheckCode::F634 => CheckKind::IfTuple,
            CheckCode::F406 => CheckKind::ImportStarNotPermitted("...".to_string()),
            CheckCode::F403 => CheckKind::ImportStarUsage("...".to_string()),
            CheckCode::F633 => CheckKind::InvalidPrintSyntax,
            CheckCode::F632 => CheckKind::IsLiteral,
            CheckCode::F404 => CheckKind::LateFutureImport,
            CheckCode::E501 => CheckKind::LineTooLong(89, 88),
            CheckCode::E402 => CheckKind::ModuleImportNotAtTopOfFile,
            CheckCode::F601 => CheckKind::MultiValueRepeatedKeyLiteral,
            CheckCode::F602 => CheckKind::MultiValueRepeatedKeyVariable("...".to_string()),
            CheckCode::R002 => CheckKind::NoAssertEquals,
            CheckCode::E711 => CheckKind::NoneComparison(RejectedCmpop::Eq),
            CheckCode::E713 => CheckKind::NotInTest,
            CheckCode::E714 => CheckKind::NotIsTest,
            CheckCode::F901 => CheckKind::RaiseNotImplemented,
            CheckCode::F706 => CheckKind::ReturnOutsideFunction,
            CheckCode::E999 => CheckKind::SyntaxError("...".to_string()),
            CheckCode::F621 => CheckKind::TooManyExpressionsInStarredAssignment,
            CheckCode::E712 => CheckKind::TrueFalseComparison(true, RejectedCmpop::Eq),
            CheckCode::F622 => CheckKind::TwoStarredExpressions,
            CheckCode::E721 => CheckKind::TypeComparison,
            CheckCode::F822 => CheckKind::UndefinedExport("...".to_string()),
            CheckCode::F823 => CheckKind::UndefinedLocal("...".to_string()),
            CheckCode::F821 => CheckKind::UndefinedName("...".to_string()),
            CheckCode::F401 => CheckKind::UnusedImport("...".to_string()),
            CheckCode::F841 => CheckKind::UnusedVariable("...".to_string()),
            CheckCode::R001 => CheckKind::UselessObjectInheritance("...".to_string()),
            CheckCode::F704 => CheckKind::YieldOutsideFunction,
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
    ForwardAnnotationSyntaxError(String),
    FStringMissingPlaceholders,
    FutureFeatureNotDefined(String),
    IOError(String),
    IfTuple,
    ImportStarNotPermitted(String),
    ImportStarUsage(String),
    InvalidPrintSyntax,
    IsLiteral,
    LateFutureImport,
    LineTooLong(usize, usize),
    ModuleImportNotAtTopOfFile,
    MultiValueRepeatedKeyLiteral,
    MultiValueRepeatedKeyVariable(String),
    NoAssertEquals,
    NoneComparison(RejectedCmpop),
    NotInTest,
    NotIsTest,
    RaiseNotImplemented,
    ReturnOutsideFunction,
    SyntaxError(String),
    TooManyExpressionsInStarredAssignment,
    TrueFalseComparison(bool, RejectedCmpop),
    TwoStarredExpressions,
    TypeComparison,
    UndefinedExport(String),
    UndefinedLocal(String),
    UndefinedName(String),
    UnusedImport(String),
    UnusedVariable(String),
    UselessObjectInheritance(String),
    YieldOutsideFunction,
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
            CheckKind::FStringMissingPlaceholders => "FStringMissingPlaceholders",
            CheckKind::ForwardAnnotationSyntaxError(_) => "ForwardAnnotationSyntaxError",
            CheckKind::FutureFeatureNotDefined(_) => "FutureFeatureNotDefined",
            CheckKind::IOError(_) => "IOError",
            CheckKind::IfTuple => "IfTuple",
            CheckKind::ImportStarNotPermitted(_) => "ImportStarNotPermitted",
            CheckKind::ImportStarUsage(_) => "ImportStarUsage",
            CheckKind::InvalidPrintSyntax => "InvalidPrintSyntax",
            CheckKind::IsLiteral => "IsLiteral",
            CheckKind::LateFutureImport => "LateFutureImport",
            CheckKind::LineTooLong(_, _) => "LineTooLong",
            CheckKind::ModuleImportNotAtTopOfFile => "ModuleImportNotAtTopOfFile",
            CheckKind::MultiValueRepeatedKeyLiteral => "MultiValueRepeatedKeyLiteral",
            CheckKind::MultiValueRepeatedKeyVariable(_) => "MultiValueRepeatedKeyVariable",
            CheckKind::NoAssertEquals => "NoAssertEquals",
            CheckKind::NoneComparison(_) => "NoneComparison",
            CheckKind::NotInTest => "NotInTest",
            CheckKind::NotIsTest => "NotIsTest",
            CheckKind::RaiseNotImplemented => "RaiseNotImplemented",
            CheckKind::ReturnOutsideFunction => "ReturnOutsideFunction",
            CheckKind::SyntaxError(_) => "SyntaxError",
            CheckKind::TooManyExpressionsInStarredAssignment => {
                "TooManyExpressionsInStarredAssignment"
            }
            CheckKind::TrueFalseComparison(_, _) => "TrueFalseComparison",
            CheckKind::TwoStarredExpressions => "TwoStarredExpressions",
            CheckKind::TypeComparison => "TypeComparison",
            CheckKind::UndefinedExport(_) => "UndefinedExport",
            CheckKind::UndefinedLocal(_) => "UndefinedLocal",
            CheckKind::UndefinedName(_) => "UndefinedName",
            CheckKind::UnusedImport(_) => "UnusedImport",
            CheckKind::UnusedVariable(_) => "UnusedVariable",
            CheckKind::UselessObjectInheritance(_) => "UselessObjectInheritance",
            CheckKind::YieldOutsideFunction => "YieldOutsideFunction",
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
            CheckKind::ImportStarNotPermitted(_) => &CheckCode::F406,
            CheckKind::ImportStarUsage(_) => &CheckCode::F403,
            CheckKind::InvalidPrintSyntax => &CheckCode::F633,
            CheckKind::IsLiteral => &CheckCode::F632,
            CheckKind::LateFutureImport => &CheckCode::F404,
            CheckKind::LineTooLong(_, _) => &CheckCode::E501,
            CheckKind::ModuleImportNotAtTopOfFile => &CheckCode::E402,
            CheckKind::MultiValueRepeatedKeyLiteral => &CheckCode::F601,
            CheckKind::MultiValueRepeatedKeyVariable(_) => &CheckCode::F602,
            CheckKind::NoAssertEquals => &CheckCode::R002,
            CheckKind::NoneComparison(_) => &CheckCode::E711,
            CheckKind::NotInTest => &CheckCode::E713,
            CheckKind::NotIsTest => &CheckCode::E714,
            CheckKind::RaiseNotImplemented => &CheckCode::F901,
            CheckKind::ReturnOutsideFunction => &CheckCode::F706,
            CheckKind::SyntaxError(_) => &CheckCode::E999,
            CheckKind::TooManyExpressionsInStarredAssignment => &CheckCode::F621,
            CheckKind::TrueFalseComparison(_, _) => &CheckCode::E712,
            CheckKind::TwoStarredExpressions => &CheckCode::F622,
            CheckKind::TypeComparison => &CheckCode::E721,
            CheckKind::UndefinedExport(_) => &CheckCode::F822,
            CheckKind::UndefinedLocal(_) => &CheckCode::F823,
            CheckKind::UndefinedName(_) => &CheckCode::F821,
            CheckKind::UnusedImport(_) => &CheckCode::F401,
            CheckKind::UnusedVariable(_) => &CheckCode::F841,
            CheckKind::UselessObjectInheritance(_) => &CheckCode::R001,
            CheckKind::YieldOutsideFunction => &CheckCode::F704,
        }
    }

    /// The body text for the check.
    pub fn body(&self) -> String {
        match self {
            CheckKind::AmbiguousClassName(name) => {
                format!("ambiguous class name '{}'", name)
            }
            CheckKind::AmbiguousFunctionName(name) => {
                format!("ambiguous function name '{}'", name)
            }
            CheckKind::AmbiguousVariableName(name) => {
                format!("ambiguous variable name '{}'", name)
            }
            CheckKind::AssertTuple => {
                "Assert test is a non-empty tuple, which is always `True`".to_string()
            }
            CheckKind::BreakOutsideLoop => "`break` outside loop".to_string(),
            CheckKind::ContinueOutsideLoop => "`continue` not properly in loop".to_string(),
            CheckKind::DefaultExceptNotLast => {
                "an `except:` block as not the last exception handler".to_string()
            }
            CheckKind::DoNotAssignLambda => {
                "Do not assign a lambda expression, use a def".to_string()
            }
            CheckKind::DoNotUseBareExcept => "Do not use bare `except`".to_string(),
            CheckKind::DuplicateArgumentName => {
                "Duplicate argument name in function definition".to_string()
            }
            CheckKind::ForwardAnnotationSyntaxError(body) => {
                format!("syntax error in forward annotation '{body}'")
            }
            CheckKind::FStringMissingPlaceholders => {
                "f-string without any placeholders".to_string()
            }
            CheckKind::FutureFeatureNotDefined(name) => {
                format!("future feature '{name}' is not defined")
            }
            CheckKind::IOError(name) => {
                format!("No such file or directory: `{name}`")
            }
            CheckKind::IfTuple => "If test is a tuple, which is always `True`".to_string(),
            CheckKind::InvalidPrintSyntax => "use of >> is invalid with print function".to_string(),
            CheckKind::ImportStarNotPermitted(name) => {
                format!("`from {name} import *` only allowed at module level")
            }
            CheckKind::ImportStarUsage(name) => {
                format!("`from {name} import *` used; unable to detect undefined names")
            }
            CheckKind::IsLiteral => "use ==/!= to compare constant literals".to_string(),
            CheckKind::LateFutureImport => {
                "from __future__ imports must occur at the beginning of the file".to_string()
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
            CheckKind::NoAssertEquals => {
                "`assertEquals` is deprecated, use `assertEqual` instead".to_string()
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
                "a `return` statement outside of a function/method".to_string()
            }
            CheckKind::SyntaxError(message) => format!("SyntaxError: {message}"),
            CheckKind::TooManyExpressionsInStarredAssignment => {
                "too many expressions in star-unpacking assignment".to_string()
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
            CheckKind::TwoStarredExpressions => "two starred expressions in assignment".to_string(),
            CheckKind::TypeComparison => "do not compare types, use `isinstance()`".to_string(),
            CheckKind::UndefinedExport(name) => {
                format!("Undefined name `{name}` in `__all__`")
            }
            CheckKind::UndefinedLocal(name) => {
                format!("Local variable `{name}` referenced before assignment")
            }
            CheckKind::UndefinedName(name) => {
                format!("Undefined name `{name}`")
            }
            CheckKind::UnusedImport(name) => format!("`{name}` imported but unused"),
            CheckKind::UnusedVariable(name) => {
                format!("Local variable `{name}` is assigned to but never used")
            }
            CheckKind::UselessObjectInheritance(name) => {
                format!("Class `{name}` inherits from object")
            }
            CheckKind::YieldOutsideFunction => {
                "a `yield` or `yield from` statement outside of a function/method".to_string()
            }
        }
    }

    /// Whether the check kind is (potentially) fixable.
    pub fn fixable(&self) -> bool {
        matches!(
            self,
            CheckKind::NoAssertEquals | CheckKind::UselessObjectInheritance(_)
        )
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    pub content: String,
    pub start: Location,
    pub end: Location,
    pub applied: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub location: Location,
    pub fix: Option<Fix>,
}

static NO_QA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)# noqa(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?").expect("Invalid regex")
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").expect("Invalid regex"));

impl Check {
    pub fn new(kind: CheckKind, location: Location) -> Self {
        Self {
            kind,
            location,
            fix: None,
        }
    }

    pub fn amend(&mut self, fix: Fix) {
        self.fix = Some(fix);
    }

    pub fn is_inline_ignored(&self, line: &str) -> bool {
        match NO_QA_REGEX.captures(line) {
            Some(caps) => match caps.name("codes") {
                Some(codes) => {
                    for code in SPLIT_COMMA_REGEX
                        .split(codes.as_str())
                        .map(|code| code.trim())
                        .filter(|code| !code.is_empty())
                    {
                        if code == self.kind.code().as_str() {
                            return true;
                        }
                    }
                    false
                }
                None => true,
            },
            None => false,
        }
    }
}
