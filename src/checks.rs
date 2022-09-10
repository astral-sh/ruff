use std::str::FromStr;

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum CheckCode {
    E402,
    E501,
    E711,
    E712,
    E713,
    E714,
    E731,
    E902,
    F401,
    F403,
    F541,
    F601,
    F602,
    F621,
    F622,
    F631,
    F634,
    F704,
    F706,
    F707,
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
            "E731" => Ok(CheckCode::E731),
            "E902" => Ok(CheckCode::E902),
            "F401" => Ok(CheckCode::F401),
            "F403" => Ok(CheckCode::F403),
            "F541" => Ok(CheckCode::F541),
            "F601" => Ok(CheckCode::F601),
            "F602" => Ok(CheckCode::F602),
            "F621" => Ok(CheckCode::F621),
            "F622" => Ok(CheckCode::F622),
            "F631" => Ok(CheckCode::F631),
            "F634" => Ok(CheckCode::F634),
            "F704" => Ok(CheckCode::F704),
            "F706" => Ok(CheckCode::F706),
            "F707" => Ok(CheckCode::F707),
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
            CheckCode::E731 => "E731",
            CheckCode::E902 => "E902",
            CheckCode::F401 => "F401",
            CheckCode::F403 => "F403",
            CheckCode::F541 => "F541",
            CheckCode::F601 => "F601",
            CheckCode::F602 => "F602",
            CheckCode::F621 => "F621",
            CheckCode::F622 => "F622",
            CheckCode::F631 => "F631",
            CheckCode::F634 => "F634",
            CheckCode::F704 => "F704",
            CheckCode::F706 => "F706",
            CheckCode::F707 => "F707",
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

    /// The source for the check (either the AST, or the physical lines).
    pub fn lint_source(&self) -> &'static LintSource {
        match self {
            CheckCode::E402 => &LintSource::AST,
            CheckCode::E501 => &LintSource::Lines,
            CheckCode::E711 => &LintSource::AST,
            CheckCode::E712 => &LintSource::AST,
            CheckCode::E713 => &LintSource::AST,
            CheckCode::E714 => &LintSource::AST,
            CheckCode::E731 => &LintSource::AST,
            CheckCode::E902 => &LintSource::FileSystem,
            CheckCode::F401 => &LintSource::AST,
            CheckCode::F403 => &LintSource::AST,
            CheckCode::F541 => &LintSource::AST,
            CheckCode::F601 => &LintSource::AST,
            CheckCode::F602 => &LintSource::AST,
            CheckCode::F621 => &LintSource::AST,
            CheckCode::F622 => &LintSource::AST,
            CheckCode::F631 => &LintSource::AST,
            CheckCode::F634 => &LintSource::AST,
            CheckCode::F704 => &LintSource::AST,
            CheckCode::F706 => &LintSource::AST,
            CheckCode::F707 => &LintSource::AST,
            CheckCode::F821 => &LintSource::AST,
            CheckCode::F822 => &LintSource::AST,
            CheckCode::F823 => &LintSource::AST,
            CheckCode::F831 => &LintSource::AST,
            CheckCode::F841 => &LintSource::AST,
            CheckCode::F901 => &LintSource::AST,
            CheckCode::R001 => &LintSource::AST,
            CheckCode::R002 => &LintSource::AST,
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
    AssertTuple,
    DefaultExceptNotLast,
    DoNotAssignLambda,
    DuplicateArgumentName,
    FStringMissingPlaceholders,
    IOError(String),
    IfTuple,
    ImportStarUsage,
    LineTooLong,
    ModuleImportNotAtTopOfFile,
    MultiValueRepeatedKeyLiteral,
    MultiValueRepeatedKeyVariable(String),
    NoAssertEquals,
    NoneComparison(RejectedCmpop),
    NotInTest,
    NotIsTest,
    RaiseNotImplemented,
    ReturnOutsideFunction,
    TooManyExpressionsInStarredAssignment,
    TrueFalseComparison(bool, RejectedCmpop),
    TwoStarredExpressions,
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
            CheckKind::AssertTuple => "AssertTuple",
            CheckKind::DefaultExceptNotLast => "DefaultExceptNotLast",
            CheckKind::DuplicateArgumentName => "DuplicateArgumentName",
            CheckKind::FStringMissingPlaceholders => "FStringMissingPlaceholders",
            CheckKind::IOError(_) => "IOError",
            CheckKind::IfTuple => "IfTuple",
            CheckKind::ImportStarUsage => "ImportStarUsage",
            CheckKind::LineTooLong => "LineTooLong",
            CheckKind::DoNotAssignLambda => "DoNotAssignLambda",
            CheckKind::ModuleImportNotAtTopOfFile => "ModuleImportNotAtTopOfFile",
            CheckKind::MultiValueRepeatedKeyLiteral => "MultiValueRepeatedKeyLiteral",
            CheckKind::MultiValueRepeatedKeyVariable(_) => "MultiValueRepeatedKeyVariable",
            CheckKind::NoAssertEquals => "NoAssertEquals",
            CheckKind::NoneComparison(_) => "NoneComparison",
            CheckKind::NotInTest => "NotInTest",
            CheckKind::NotIsTest => "NotIsTest",
            CheckKind::RaiseNotImplemented => "RaiseNotImplemented",
            CheckKind::ReturnOutsideFunction => "ReturnOutsideFunction",
            CheckKind::TooManyExpressionsInStarredAssignment => {
                "TooManyExpressionsInStarredAssignment"
            }
            CheckKind::TrueFalseComparison(_, _) => "TrueFalseComparison",
            CheckKind::TwoStarredExpressions => "TwoStarredExpressions",
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
            CheckKind::AssertTuple => &CheckCode::F631,
            CheckKind::DefaultExceptNotLast => &CheckCode::F707,
            CheckKind::DuplicateArgumentName => &CheckCode::F831,
            CheckKind::FStringMissingPlaceholders => &CheckCode::F541,
            CheckKind::IOError(_) => &CheckCode::E902,
            CheckKind::IfTuple => &CheckCode::F634,
            CheckKind::ImportStarUsage => &CheckCode::F403,
            CheckKind::LineTooLong => &CheckCode::E501,
            CheckKind::DoNotAssignLambda => &CheckCode::E731,
            CheckKind::ModuleImportNotAtTopOfFile => &CheckCode::E402,
            CheckKind::MultiValueRepeatedKeyLiteral => &CheckCode::F601,
            CheckKind::MultiValueRepeatedKeyVariable(_) => &CheckCode::F602,
            CheckKind::NoAssertEquals => &CheckCode::R002,
            CheckKind::NoneComparison(_) => &CheckCode::E711,
            CheckKind::NotInTest => &CheckCode::E713,
            CheckKind::NotIsTest => &CheckCode::E714,
            CheckKind::RaiseNotImplemented => &CheckCode::F901,
            CheckKind::ReturnOutsideFunction => &CheckCode::F706,
            CheckKind::TooManyExpressionsInStarredAssignment => &CheckCode::F621,
            CheckKind::TrueFalseComparison(_, _) => &CheckCode::E712,
            CheckKind::TwoStarredExpressions => &CheckCode::F622,
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
            CheckKind::AssertTuple => {
                "Assert test is a non-empty tuple, which is always `True`".to_string()
            }
            CheckKind::DefaultExceptNotLast => {
                "an `except:` block as not the last exception handler".to_string()
            }
            CheckKind::DuplicateArgumentName => {
                "Duplicate argument name in function definition".to_string()
            }
            CheckKind::FStringMissingPlaceholders => {
                "f-string without any placeholders".to_string()
            }
            CheckKind::IOError(name) => {
                format!("No such file or directory: `{name}`")
            }
            CheckKind::IfTuple => "If test is a tuple, which is always `True`".to_string(),
            CheckKind::ImportStarUsage => "Unable to detect undefined names".to_string(),
            CheckKind::LineTooLong => "Line too long".to_string(),
            CheckKind::DoNotAssignLambda => {
                "Do not assign a lambda expression, use a def".to_string()
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
            CheckKind::UndefinedExport(name) => {
                format!("Undefined name `{name}` in `__all__`")
            }
            CheckKind::UndefinedName(name) => {
                format!("Undefined name `{name}`")
            }
            CheckKind::UndefinedLocal(name) => {
                format!("Local variable `{name}` referenced before assignment")
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
        match self {
            CheckKind::AssertTuple => false,
            CheckKind::DefaultExceptNotLast => false,
            CheckKind::DuplicateArgumentName => false,
            CheckKind::FStringMissingPlaceholders => false,
            CheckKind::IOError(_) => false,
            CheckKind::IfTuple => false,
            CheckKind::ImportStarUsage => false,
            CheckKind::DoNotAssignLambda => false,
            CheckKind::LineTooLong => false,
            CheckKind::ModuleImportNotAtTopOfFile => false,
            CheckKind::MultiValueRepeatedKeyLiteral => false,
            CheckKind::MultiValueRepeatedKeyVariable(_) => false,
            CheckKind::NoAssertEquals => true,
            CheckKind::NotInTest => false,
            CheckKind::NotIsTest => false,
            CheckKind::NoneComparison(_) => false,
            CheckKind::RaiseNotImplemented => false,
            CheckKind::ReturnOutsideFunction => false,
            CheckKind::TooManyExpressionsInStarredAssignment => false,
            CheckKind::TrueFalseComparison(_, _) => false,
            CheckKind::TwoStarredExpressions => false,
            CheckKind::UndefinedExport(_) => false,
            CheckKind::UndefinedLocal(_) => false,
            CheckKind::UndefinedName(_) => false,
            CheckKind::UnusedImport(_) => false,
            CheckKind::UnusedVariable(_) => false,
            CheckKind::UselessObjectInheritance(_) => true,
            CheckKind::YieldOutsideFunction => false,
        }
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
