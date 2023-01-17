use std::fmt;

use itertools::Itertools;
use rustpython_ast::Cmpop;
use serde::{Deserialize, Serialize};

use crate::define_violation;
use crate::rules::flake8_debugger::types::DebuggerUsingType;
use crate::rules::flake8_pytest_style::types::{
    ParametrizeNameType, ParametrizeValuesRowType, ParametrizeValuesType,
};
use crate::rules::flake8_quotes::settings::Quote;
use crate::rules::pyupgrade::types::Primitive;
use crate::violation::{AlwaysAutofixableViolation, Violation};

// pycodestyle errors

define_violation!(
    pub struct MultipleImportsOnOneLine;
);
impl Violation for MultipleImportsOnOneLine {
    fn message(&self) -> String {
        "Multiple imports on one line".to_string()
    }

    fn placeholder() -> Self {
        MultipleImportsOnOneLine
    }
}

define_violation!(
    pub struct ModuleImportNotAtTopOfFile;
);
impl Violation for ModuleImportNotAtTopOfFile {
    fn message(&self) -> String {
        "Module level import not at top of file".to_string()
    }

    fn placeholder() -> Self {
        ModuleImportNotAtTopOfFile
    }
}

define_violation!(
    pub struct LineTooLong(pub usize, pub usize);
);
impl Violation for LineTooLong {
    fn message(&self) -> String {
        let LineTooLong(length, limit) = self;
        format!("Line too long ({length} > {limit} characters)")
    }

    fn placeholder() -> Self {
        LineTooLong(89, 88)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqCmpop {
    Eq,
    NotEq,
}

impl From<&Cmpop> for EqCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Eq => EqCmpop::Eq,
            Cmpop::NotEq => EqCmpop::NotEq,
            _ => unreachable!("Expected Cmpop::Eq | Cmpop::NotEq"),
        }
    }
}

define_violation!(
    pub struct NoneComparison(pub EqCmpop);
);
impl AlwaysAutofixableViolation for NoneComparison {
    fn message(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpop::Eq => "Comparison to `None` should be `cond is None`".to_string(),
            EqCmpop::NotEq => "Comparison to `None` should be `cond is not None`".to_string(),
        }
    }

    fn autofix_title(&self) -> String {
        let NoneComparison(op) = self;
        match op {
            EqCmpop::Eq => "Replace with `cond is None`".to_string(),
            EqCmpop::NotEq => "Replace with `cond is not None`".to_string(),
        }
    }

    fn placeholder() -> Self {
        NoneComparison(EqCmpop::Eq)
    }
}

define_violation!(
    pub struct TrueFalseComparison(pub bool, pub EqCmpop);
);
impl AlwaysAutofixableViolation for TrueFalseComparison {
    fn message(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpop::Eq) => "Comparison to `True` should be `cond is True`".to_string(),
            (true, EqCmpop::NotEq) => {
                "Comparison to `True` should be `cond is not True`".to_string()
            }
            (false, EqCmpop::Eq) => "Comparison to `False` should be `cond is False`".to_string(),
            (false, EqCmpop::NotEq) => {
                "Comparison to `False` should be `cond is not False`".to_string()
            }
        }
    }

    fn autofix_title(&self) -> String {
        let TrueFalseComparison(value, op) = self;
        match (value, op) {
            (true, EqCmpop::Eq) => "Replace with `cond is True`".to_string(),
            (true, EqCmpop::NotEq) => "Replace with `cond is not True`".to_string(),
            (false, EqCmpop::Eq) => "Replace with `cond is False`".to_string(),
            (false, EqCmpop::NotEq) => "Replace with `cond is not False`".to_string(),
        }
    }

    fn placeholder() -> Self {
        TrueFalseComparison(true, EqCmpop::Eq)
    }
}

define_violation!(
    pub struct NotInTest;
);
impl AlwaysAutofixableViolation for NotInTest {
    fn message(&self) -> String {
        "Test for membership should be `not in`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Convert to `not in`".to_string()
    }

    fn placeholder() -> Self {
        NotInTest
    }
}

define_violation!(
    pub struct NotIsTest;
);
impl AlwaysAutofixableViolation for NotIsTest {
    fn message(&self) -> String {
        "Test for object identity should be `is not`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Convert to `is not`".to_string()
    }

    fn placeholder() -> Self {
        NotIsTest
    }
}

define_violation!(
    pub struct TypeComparison;
);
impl Violation for TypeComparison {
    fn message(&self) -> String {
        "Do not compare types, use `isinstance()`".to_string()
    }

    fn placeholder() -> Self {
        TypeComparison
    }
}

define_violation!(
    pub struct DoNotUseBareExcept;
);
impl Violation for DoNotUseBareExcept {
    fn message(&self) -> String {
        "Do not use bare `except`".to_string()
    }

    fn placeholder() -> Self {
        DoNotUseBareExcept
    }
}

define_violation!(
    pub struct DoNotAssignLambda(pub String);
);
impl AlwaysAutofixableViolation for DoNotAssignLambda {
    fn message(&self) -> String {
        "Do not assign a `lambda` expression, use a `def`".to_string()
    }

    fn autofix_title(&self) -> String {
        let DoNotAssignLambda(name) = self;
        format!("Rewrite `{name}` as a `def`")
    }

    fn placeholder() -> Self {
        DoNotAssignLambda("...".to_string())
    }
}

define_violation!(
    pub struct AmbiguousVariableName(pub String);
);
impl Violation for AmbiguousVariableName {
    fn message(&self) -> String {
        let AmbiguousVariableName(name) = self;
        format!("Ambiguous variable name: `{name}`")
    }

    fn placeholder() -> Self {
        AmbiguousVariableName("...".to_string())
    }
}

define_violation!(
    pub struct AmbiguousClassName(pub String);
);
impl Violation for AmbiguousClassName {
    fn message(&self) -> String {
        let AmbiguousClassName(name) = self;
        format!("Ambiguous class name: `{name}`")
    }

    fn placeholder() -> Self {
        AmbiguousClassName("...".to_string())
    }
}

define_violation!(
    pub struct AmbiguousFunctionName(pub String);
);
impl Violation for AmbiguousFunctionName {
    fn message(&self) -> String {
        let AmbiguousFunctionName(name) = self;
        format!("Ambiguous function name: `{name}`")
    }

    fn placeholder() -> Self {
        AmbiguousFunctionName("...".to_string())
    }
}

define_violation!(
    pub struct IOError(pub String);
);
impl Violation for IOError {
    fn message(&self) -> String {
        let IOError(message) = self;
        message.clone()
    }

    fn placeholder() -> Self {
        IOError("IOError: `...`".to_string())
    }
}

define_violation!(
    pub struct SyntaxError(pub String);
);
impl Violation for SyntaxError {
    fn message(&self) -> String {
        let SyntaxError(message) = self;
        format!("SyntaxError: {message}")
    }

    fn placeholder() -> Self {
        SyntaxError("`...`".to_string())
    }
}

// pycodestyle warnings

define_violation!(
    pub struct NoNewLineAtEndOfFile;
);
impl AlwaysAutofixableViolation for NoNewLineAtEndOfFile {
    fn message(&self) -> String {
        "No newline at end of file".to_string()
    }

    fn autofix_title(&self) -> String {
        "Add trailing newline".to_string()
    }

    fn placeholder() -> Self {
        NoNewLineAtEndOfFile
    }
}

define_violation!(
    pub struct InvalidEscapeSequence(pub char);
);
impl AlwaysAutofixableViolation for InvalidEscapeSequence {
    fn message(&self) -> String {
        let InvalidEscapeSequence(char) = self;
        format!("Invalid escape sequence: '\\{char}'")
    }

    fn autofix_title(&self) -> String {
        "Add backslash to escape sequence".to_string()
    }

    fn placeholder() -> Self {
        InvalidEscapeSequence('c')
    }
}

define_violation!(
    pub struct DocLineTooLong(pub usize, pub usize);
);
impl Violation for DocLineTooLong {
    fn message(&self) -> String {
        let DocLineTooLong(length, limit) = self;
        format!("Doc line too long ({length} > {limit} characters)")
    }

    fn placeholder() -> Self {
        DocLineTooLong(89, 88)
    }
}

// pyflakes

define_violation!(
    pub struct UnusedImport(pub String, pub bool, pub bool);
);
fn fmt_unused_import_autofix_msg(unused_import: &UnusedImport) -> String {
    let UnusedImport(name, _, multiple) = unused_import;
    if *multiple {
        "Remove unused import".to_string()
    } else {
        format!("Remove unused import: `{name}`")
    }
}
impl Violation for UnusedImport {
    fn message(&self) -> String {
        let UnusedImport(name, ignore_init, ..) = self;
        if *ignore_init {
            format!(
                "`{name}` imported but unused; consider adding to `__all__` or using a redundant \
                 alias"
            )
        } else {
            format!("`{name}` imported but unused")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let UnusedImport(_, ignore_init, _) = self;
        if *ignore_init {
            None
        } else {
            Some(fmt_unused_import_autofix_msg)
        }
    }

    fn placeholder() -> Self {
        UnusedImport("...".to_string(), false, false)
    }
}

define_violation!(
    pub struct ImportShadowedByLoopVar(pub String, pub usize);
);
impl Violation for ImportShadowedByLoopVar {
    fn message(&self) -> String {
        let ImportShadowedByLoopVar(name, line) = self;
        format!("Import `{name}` from line {line} shadowed by loop variable")
    }

    fn placeholder() -> Self {
        ImportShadowedByLoopVar("...".to_string(), 1)
    }
}

define_violation!(
    pub struct ImportStarUsed(pub String);
);
impl Violation for ImportStarUsed {
    fn message(&self) -> String {
        let ImportStarUsed(name) = self;
        format!("`from {name} import *` used; unable to detect undefined names")
    }

    fn placeholder() -> Self {
        ImportStarUsed("...".to_string())
    }
}

define_violation!(
    pub struct LateFutureImport;
);
impl Violation for LateFutureImport {
    fn message(&self) -> String {
        "`from __future__` imports must occur at the beginning of the file".to_string()
    }

    fn placeholder() -> Self {
        LateFutureImport
    }
}

define_violation!(
    pub struct ImportStarUsage(pub String, pub Vec<String>);
);
impl Violation for ImportStarUsage {
    fn message(&self) -> String {
        let ImportStarUsage(name, sources) = self;
        let sources = sources
            .iter()
            .map(|source| format!("`{source}`"))
            .join(", ");
        format!("`{name}` may be undefined, or defined from star imports: {sources}")
    }

    fn placeholder() -> Self {
        ImportStarUsage("...".to_string(), vec!["...".to_string()])
    }
}

define_violation!(
    pub struct ImportStarNotPermitted(pub String);
);
impl Violation for ImportStarNotPermitted {
    fn message(&self) -> String {
        let ImportStarNotPermitted(name) = self;
        format!("`from {name} import *` only allowed at module level")
    }

    fn placeholder() -> Self {
        ImportStarNotPermitted("...".to_string())
    }
}

define_violation!(
    pub struct FutureFeatureNotDefined(pub String);
);
impl Violation for FutureFeatureNotDefined {
    fn message(&self) -> String {
        let FutureFeatureNotDefined(name) = self;
        format!("Future feature `{name}` is not defined")
    }

    fn placeholder() -> Self {
        FutureFeatureNotDefined("...".to_string())
    }
}

define_violation!(
    pub struct PercentFormatInvalidFormat(pub String);
);
impl Violation for PercentFormatInvalidFormat {
    fn message(&self) -> String {
        let PercentFormatInvalidFormat(message) = self;
        format!("'...' % ... has invalid format string: {message}")
    }

    fn placeholder() -> Self {
        PercentFormatInvalidFormat("...".to_string())
    }
}

define_violation!(
    pub struct PercentFormatExpectedMapping;
);
impl Violation for PercentFormatExpectedMapping {
    fn message(&self) -> String {
        "'...' % ... expected mapping but got sequence".to_string()
    }

    fn placeholder() -> Self {
        PercentFormatExpectedMapping
    }
}

define_violation!(
    pub struct PercentFormatExpectedSequence;
);
impl Violation for PercentFormatExpectedSequence {
    fn message(&self) -> String {
        "'...' % ... expected sequence but got mapping".to_string()
    }

    fn placeholder() -> Self {
        PercentFormatExpectedSequence
    }
}

define_violation!(
    pub struct PercentFormatExtraNamedArguments(pub Vec<String>);
);
impl AlwaysAutofixableViolation for PercentFormatExtraNamedArguments {
    fn message(&self) -> String {
        let PercentFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("'...' % ... has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let PercentFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }

    fn placeholder() -> Self {
        PercentFormatExtraNamedArguments(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct PercentFormatMissingArgument(pub Vec<String>);
);
impl Violation for PercentFormatMissingArgument {
    fn message(&self) -> String {
        let PercentFormatMissingArgument(missing) = self;
        let message = missing.join(", ");
        format!("'...' % ... is missing argument(s) for placeholder(s): {message}")
    }

    fn placeholder() -> Self {
        PercentFormatMissingArgument(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct PercentFormatMixedPositionalAndNamed;
);
impl Violation for PercentFormatMixedPositionalAndNamed {
    fn message(&self) -> String {
        "'...' % ... has mixed positional and named placeholders".to_string()
    }

    fn placeholder() -> Self {
        PercentFormatMixedPositionalAndNamed
    }
}

define_violation!(
    pub struct PercentFormatPositionalCountMismatch(pub usize, pub usize);
);
impl Violation for PercentFormatPositionalCountMismatch {
    fn message(&self) -> String {
        let PercentFormatPositionalCountMismatch(wanted, got) = self;
        format!("'...' % ... has {wanted} placeholder(s) but {got} substitution(s)")
    }

    fn placeholder() -> Self {
        PercentFormatPositionalCountMismatch(4, 2)
    }
}

define_violation!(
    pub struct PercentFormatStarRequiresSequence;
);
impl Violation for PercentFormatStarRequiresSequence {
    fn message(&self) -> String {
        "'...' % ... `*` specifier requires sequence".to_string()
    }

    fn placeholder() -> Self {
        PercentFormatStarRequiresSequence
    }
}

define_violation!(
    pub struct PercentFormatUnsupportedFormatCharacter(pub char);
);
impl Violation for PercentFormatUnsupportedFormatCharacter {
    fn message(&self) -> String {
        let PercentFormatUnsupportedFormatCharacter(char) = self;
        format!("'...' % ... has unsupported format character '{char}'")
    }

    fn placeholder() -> Self {
        PercentFormatUnsupportedFormatCharacter('c')
    }
}

define_violation!(
    pub struct StringDotFormatInvalidFormat(pub String);
);
impl Violation for StringDotFormatInvalidFormat {
    fn message(&self) -> String {
        let StringDotFormatInvalidFormat(message) = self;
        format!("'...'.format(...) has invalid format string: {message}")
    }

    fn placeholder() -> Self {
        StringDotFormatInvalidFormat("...".to_string())
    }
}

define_violation!(
    pub struct StringDotFormatExtraNamedArguments(pub Vec<String>);
);
impl AlwaysAutofixableViolation for StringDotFormatExtraNamedArguments {
    fn message(&self) -> String {
        let StringDotFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("'...'.format(...) has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let StringDotFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }

    fn placeholder() -> Self {
        StringDotFormatExtraNamedArguments(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct StringDotFormatExtraPositionalArguments(pub Vec<String>);
);
impl Violation for StringDotFormatExtraPositionalArguments {
    fn message(&self) -> String {
        let StringDotFormatExtraPositionalArguments(missing) = self;
        let message = missing.join(", ");
        format!("'...'.format(...) has unused arguments at position(s): {message}")
    }

    fn placeholder() -> Self {
        StringDotFormatExtraPositionalArguments(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct StringDotFormatMissingArguments(pub Vec<String>);
);
impl Violation for StringDotFormatMissingArguments {
    fn message(&self) -> String {
        let StringDotFormatMissingArguments(missing) = self;
        let message = missing.join(", ");
        format!("'...'.format(...) is missing argument(s) for placeholder(s): {message}")
    }

    fn placeholder() -> Self {
        StringDotFormatMissingArguments(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct StringDotFormatMixingAutomatic;
);
impl Violation for StringDotFormatMixingAutomatic {
    fn message(&self) -> String {
        "'...'.format(...) mixes automatic and manual numbering".to_string()
    }

    fn placeholder() -> Self {
        StringDotFormatMixingAutomatic
    }
}

define_violation!(
    pub struct FStringMissingPlaceholders;
);
impl AlwaysAutofixableViolation for FStringMissingPlaceholders {
    fn message(&self) -> String {
        "f-string without any placeholders".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous `f` prefix".to_string()
    }

    fn placeholder() -> Self {
        FStringMissingPlaceholders
    }
}

define_violation!(
    pub struct MultiValueRepeatedKeyLiteral(pub String, pub bool);
);
impl Violation for MultiValueRepeatedKeyLiteral {
    fn message(&self) -> String {
        let MultiValueRepeatedKeyLiteral(name, ..) = self;
        format!("Dictionary key literal `{name}` repeated")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let MultiValueRepeatedKeyLiteral(.., repeated_value) = self;
        if *repeated_value {
            Some(|MultiValueRepeatedKeyLiteral(name, ..)| {
                format!("Remove repeated key literal `{name}`")
            })
        } else {
            None
        }
    }

    fn placeholder() -> Self {
        MultiValueRepeatedKeyLiteral("...".to_string(), true)
    }
}

define_violation!(
    pub struct MultiValueRepeatedKeyVariable(pub String, pub bool);
);
impl Violation for MultiValueRepeatedKeyVariable {
    fn message(&self) -> String {
        let MultiValueRepeatedKeyVariable(name, ..) = self;
        format!("Dictionary key `{name}` repeated")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let MultiValueRepeatedKeyVariable(.., repeated_value) = self;
        if *repeated_value {
            Some(|MultiValueRepeatedKeyVariable(name, ..)| format!("Remove repeated key `{name}`"))
        } else {
            None
        }
    }

    fn placeholder() -> Self {
        MultiValueRepeatedKeyVariable("...".to_string(), true)
    }
}

define_violation!(
    pub struct ExpressionsInStarAssignment;
);
impl Violation for ExpressionsInStarAssignment {
    fn message(&self) -> String {
        "Too many expressions in star-unpacking assignment".to_string()
    }

    fn placeholder() -> Self {
        ExpressionsInStarAssignment
    }
}

define_violation!(
    pub struct TwoStarredExpressions;
);
impl Violation for TwoStarredExpressions {
    fn message(&self) -> String {
        "Two starred expressions in assignment".to_string()
    }

    fn placeholder() -> Self {
        TwoStarredExpressions
    }
}

define_violation!(
    pub struct AssertTuple;
);
impl Violation for AssertTuple {
    fn message(&self) -> String {
        "Assert test is a non-empty tuple, which is always `True`".to_string()
    }

    fn placeholder() -> Self {
        AssertTuple
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsCmpop {
    Is,
    IsNot,
}

impl From<&Cmpop> for IsCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Is => IsCmpop::Is,
            Cmpop::IsNot => IsCmpop::IsNot,
            _ => unreachable!("Expected Cmpop::Is | Cmpop::IsNot"),
        }
    }
}

define_violation!(
    pub struct IsLiteral(pub IsCmpop);
);
impl AlwaysAutofixableViolation for IsLiteral {
    fn message(&self) -> String {
        let IsLiteral(cmpop) = self;
        match cmpop {
            IsCmpop::Is => "Use `==` to compare constant literals".to_string(),
            IsCmpop::IsNot => "Use `!=` to compare constant literals".to_string(),
        }
    }

    fn autofix_title(&self) -> String {
        let IsLiteral(cmpop) = self;
        match cmpop {
            IsCmpop::Is => "Replace `is` with `==`".to_string(),
            IsCmpop::IsNot => "Replace `is not` with `!=`".to_string(),
        }
    }

    fn placeholder() -> Self {
        IsLiteral(IsCmpop::Is)
    }
}

define_violation!(
    pub struct InvalidPrintSyntax;
);
impl Violation for InvalidPrintSyntax {
    fn message(&self) -> String {
        "Use of `>>` is invalid with `print` function".to_string()
    }

    fn placeholder() -> Self {
        InvalidPrintSyntax
    }
}

define_violation!(
    pub struct IfTuple;
);
impl Violation for IfTuple {
    fn message(&self) -> String {
        "If test is a tuple, which is always `True`".to_string()
    }

    fn placeholder() -> Self {
        IfTuple
    }
}

define_violation!(
    pub struct BreakOutsideLoop;
);
impl Violation for BreakOutsideLoop {
    fn message(&self) -> String {
        "`break` outside loop".to_string()
    }

    fn placeholder() -> Self {
        BreakOutsideLoop
    }
}

define_violation!(
    pub struct ContinueOutsideLoop;
);
impl Violation for ContinueOutsideLoop {
    fn message(&self) -> String {
        "`continue` not properly in loop".to_string()
    }

    fn placeholder() -> Self {
        ContinueOutsideLoop
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeferralKeyword {
    Yield,
    YieldFrom,
    Await,
}

impl fmt::Display for DeferralKeyword {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeferralKeyword::Yield => fmt.write_str("yield"),
            DeferralKeyword::YieldFrom => fmt.write_str("yield from"),
            DeferralKeyword::Await => fmt.write_str("await"),
        }
    }
}

define_violation!(
    pub struct YieldOutsideFunction(pub DeferralKeyword);
);
impl Violation for YieldOutsideFunction {
    fn message(&self) -> String {
        let YieldOutsideFunction(keyword) = self;
        format!("`{keyword}` statement outside of a function")
    }

    fn placeholder() -> Self {
        YieldOutsideFunction(DeferralKeyword::Yield)
    }
}

define_violation!(
    pub struct ReturnOutsideFunction;
);
impl Violation for ReturnOutsideFunction {
    fn message(&self) -> String {
        "`return` statement outside of a function/method".to_string()
    }

    fn placeholder() -> Self {
        ReturnOutsideFunction
    }
}

define_violation!(
    pub struct DefaultExceptNotLast;
);
impl Violation for DefaultExceptNotLast {
    fn message(&self) -> String {
        "An `except` block as not the last exception handler".to_string()
    }

    fn placeholder() -> Self {
        DefaultExceptNotLast
    }
}

define_violation!(
    pub struct ForwardAnnotationSyntaxError(pub String);
);
impl Violation for ForwardAnnotationSyntaxError {
    fn message(&self) -> String {
        let ForwardAnnotationSyntaxError(body) = self;
        format!("Syntax error in forward annotation: `{body}`")
    }

    fn placeholder() -> Self {
        ForwardAnnotationSyntaxError("...".to_string())
    }
}

define_violation!(
    pub struct RedefinedWhileUnused(pub String, pub usize);
);
impl Violation for RedefinedWhileUnused {
    fn message(&self) -> String {
        let RedefinedWhileUnused(name, line) = self;
        format!("Redefinition of unused `{name}` from line {line}")
    }

    fn placeholder() -> Self {
        RedefinedWhileUnused("...".to_string(), 1)
    }
}

define_violation!(
    pub struct UndefinedName(pub String);
);
impl Violation for UndefinedName {
    fn message(&self) -> String {
        let UndefinedName(name) = self;
        format!("Undefined name `{name}`")
    }

    fn placeholder() -> Self {
        UndefinedName("...".to_string())
    }
}

define_violation!(
    pub struct UndefinedExport(pub String);
);
impl Violation for UndefinedExport {
    fn message(&self) -> String {
        let UndefinedExport(name) = self;
        format!("Undefined name `{name}` in `__all__`")
    }

    fn placeholder() -> Self {
        UndefinedExport("...".to_string())
    }
}

define_violation!(
    pub struct UndefinedLocal(pub String);
);
impl Violation for UndefinedLocal {
    fn message(&self) -> String {
        let UndefinedLocal(name) = self;
        format!("Local variable `{name}` referenced before assignment")
    }

    fn placeholder() -> Self {
        UndefinedLocal("...".to_string())
    }
}

define_violation!(
    pub struct UnusedVariable(pub String);
);
impl AlwaysAutofixableViolation for UnusedVariable {
    fn message(&self) -> String {
        let UnusedVariable(name) = self;
        format!("Local variable `{name}` is assigned to but never used")
    }

    fn autofix_title(&self) -> String {
        let UnusedVariable(name) = self;
        format!("Remove assignment to unused variable `{name}`")
    }

    fn placeholder() -> Self {
        UnusedVariable("...".to_string())
    }
}

define_violation!(
    pub struct UnusedAnnotation(pub String);
);
impl Violation for UnusedAnnotation {
    fn message(&self) -> String {
        let UnusedAnnotation(name) = self;
        format!("Local variable `{name}` is annotated but never used")
    }

    fn placeholder() -> Self {
        UnusedAnnotation("...".to_string())
    }
}

define_violation!(
    pub struct RaiseNotImplemented;
);
impl AlwaysAutofixableViolation for RaiseNotImplemented {
    fn message(&self) -> String {
        "`raise NotImplemented` should be `raise NotImplementedError`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Use `raise NotImplementedError`".to_string()
    }

    fn placeholder() -> Self {
        RaiseNotImplemented
    }
}

// pylint

define_violation!(
    pub struct UselessImportAlias;
);
impl AlwaysAutofixableViolation for UselessImportAlias {
    fn message(&self) -> String {
        "Import alias does not rename original package".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove import alias".to_string()
    }

    fn placeholder() -> Self {
        UselessImportAlias
    }
}

define_violation!(
    pub struct MisplacedComparisonConstant(pub String);
);
impl AlwaysAutofixableViolation for MisplacedComparisonConstant {
    fn message(&self) -> String {
        let MisplacedComparisonConstant(comparison) = self;
        format!("Comparison should be {comparison}")
    }

    fn autofix_title(&self) -> String {
        let MisplacedComparisonConstant(comparison) = self;
        format!("Replace with {comparison}")
    }

    fn placeholder() -> Self {
        MisplacedComparisonConstant("...".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryDirectLambdaCall;
);
impl Violation for UnnecessaryDirectLambdaCall {
    fn message(&self) -> String {
        "Lambda expression called directly. Execute the expression inline instead.".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryDirectLambdaCall
    }
}

define_violation!(
    pub struct NonlocalWithoutBinding(pub String);
);
impl Violation for NonlocalWithoutBinding {
    fn message(&self) -> String {
        let NonlocalWithoutBinding(name) = self;
        format!("Nonlocal name `{name}` found without binding")
    }

    fn placeholder() -> Self {
        NonlocalWithoutBinding("...".to_string())
    }
}

define_violation!(
    pub struct UsedPriorGlobalDeclaration(pub String, pub usize);
);
impl Violation for UsedPriorGlobalDeclaration {
    fn message(&self) -> String {
        let UsedPriorGlobalDeclaration(name, line) = self;
        format!("Name `{name}` is used prior to global declaration on line {line}")
    }

    fn placeholder() -> Self {
        UsedPriorGlobalDeclaration("...".to_string(), 1)
    }
}

define_violation!(
    pub struct AwaitOutsideAsync;
);
impl Violation for AwaitOutsideAsync {
    fn message(&self) -> String {
        "`await` should be used within an async function".to_string()
    }

    fn placeholder() -> Self {
        AwaitOutsideAsync
    }
}

define_violation!(
    pub struct PropertyWithParameters;
);
impl Violation for PropertyWithParameters {
    fn message(&self) -> String {
        "Cannot have defined parameters for properties".to_string()
    }

    fn placeholder() -> Self {
        PropertyWithParameters
    }
}

define_violation!(
    pub struct ConsiderUsingFromImport(pub String, pub String);
);
impl Violation for ConsiderUsingFromImport {
    fn message(&self) -> String {
        let ConsiderUsingFromImport(module, name) = self;
        format!("Use `from {module} import {name}` in lieu of alias")
    }

    fn placeholder() -> Self {
        ConsiderUsingFromImport("...".to_string(), "...".to_string())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationsCmpop {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

impl From<&Cmpop> for ViolationsCmpop {
    fn from(cmpop: &Cmpop) -> Self {
        match cmpop {
            Cmpop::Eq => Self::Eq,
            Cmpop::NotEq => Self::NotEq,
            Cmpop::Lt => Self::Lt,
            Cmpop::LtE => Self::LtE,
            Cmpop::Gt => Self::Gt,
            Cmpop::GtE => Self::GtE,
            Cmpop::Is => Self::Is,
            Cmpop::IsNot => Self::IsNot,
            Cmpop::In => Self::In,
            Cmpop::NotIn => Self::NotIn,
        }
    }
}

impl fmt::Display for ViolationsCmpop {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self {
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::Lt => "<",
            Self::LtE => "<=",
            Self::Gt => ">",
            Self::GtE => ">=",
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::In => "in",
            Self::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}

define_violation!(
    pub struct ConstantComparison {
        pub left_constant: String,
        pub op: ViolationsCmpop,
        pub right_constant: String,
    }
);
impl Violation for ConstantComparison {
    fn message(&self) -> String {
        let ConstantComparison {
            left_constant,
            op,
            right_constant,
        } = self;

        format!(
            "Two constants compared in a comparison, consider replacing `{left_constant} {op} \
             {right_constant}`"
        )
    }

    fn placeholder() -> Self {
        ConstantComparison {
            left_constant: "0".to_string(),
            op: ViolationsCmpop::Eq,
            right_constant: "0".to_string(),
        }
    }
}

define_violation!(
    pub struct ConsiderMergingIsinstance(pub String, pub Vec<String>);
);
impl Violation for ConsiderMergingIsinstance {
    fn message(&self) -> String {
        let ConsiderMergingIsinstance(obj, types) = self;
        let types = types.join(", ");
        format!("Merge these isinstance calls: `isinstance({obj}, ({types}))`")
    }

    fn placeholder() -> Self {
        ConsiderMergingIsinstance("...".to_string(), vec!["...".to_string()])
    }
}

define_violation!(
    pub struct UseSysExit(pub String);
);
impl AlwaysAutofixableViolation for UseSysExit {
    fn message(&self) -> String {
        let UseSysExit(name) = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn autofix_title(&self) -> String {
        let UseSysExit(name) = self;
        format!("Replace `{name}` with `sys.exit()`")
    }

    fn placeholder() -> Self {
        UseSysExit("exit".to_string())
    }
}

define_violation!(
    pub struct MagicValueComparison(pub String);
);
impl Violation for MagicValueComparison {
    fn message(&self) -> String {
        let MagicValueComparison(value) = self;
        format!(
            "Magic value used in comparison, consider replacing {value} with a constant variable"
        )
    }

    fn placeholder() -> Self {
        MagicValueComparison("magic".to_string())
    }
}

define_violation!(
    pub struct UselessElseOnLoop;
);
impl Violation for UselessElseOnLoop {
    fn message(&self) -> String {
        "Else clause on loop without a break statement, remove the else and de-indent all the code \
         inside it"
            .to_string()
    }

    fn placeholder() -> Self {
        UselessElseOnLoop
    }
}

define_violation!(
    pub struct GlobalVariableNotAssigned(pub String);
);
impl Violation for GlobalVariableNotAssigned {
    fn message(&self) -> String {
        let GlobalVariableNotAssigned(name) = self;
        format!("Using global for `{name}` but no assignment is done")
    }

    fn placeholder() -> Self {
        GlobalVariableNotAssigned("...".to_string())
    }
}

// flake8-builtins

define_violation!(
    pub struct BuiltinVariableShadowing(pub String);
);
impl Violation for BuiltinVariableShadowing {
    fn message(&self) -> String {
        let BuiltinVariableShadowing(name) = self;
        format!("Variable `{name}` is shadowing a python builtin")
    }

    fn placeholder() -> Self {
        BuiltinVariableShadowing("...".to_string())
    }
}

define_violation!(
    pub struct BuiltinArgumentShadowing(pub String);
);
impl Violation for BuiltinArgumentShadowing {
    fn message(&self) -> String {
        let BuiltinArgumentShadowing(name) = self;
        format!("Argument `{name}` is shadowing a python builtin")
    }

    fn placeholder() -> Self {
        BuiltinArgumentShadowing("...".to_string())
    }
}

define_violation!(
    pub struct BuiltinAttributeShadowing(pub String);
);
impl Violation for BuiltinAttributeShadowing {
    fn message(&self) -> String {
        let BuiltinAttributeShadowing(name) = self;
        format!("Class attribute `{name}` is shadowing a python builtin")
    }

    fn placeholder() -> Self {
        BuiltinAttributeShadowing("...".to_string())
    }
}

// flake8-bugbear

define_violation!(
    pub struct UnaryPrefixIncrement;
);
impl Violation for UnaryPrefixIncrement {
    fn message(&self) -> String {
        "Python does not support the unary prefix increment. Writing `++n` is equivalent to \
         `+(+(n))`, which equals `n`. You meant `n += 1`."
            .to_string()
    }

    fn placeholder() -> Self {
        UnaryPrefixIncrement
    }
}

define_violation!(
    pub struct AssignmentToOsEnviron;
);
impl Violation for AssignmentToOsEnviron {
    fn message(&self) -> String {
        "Assigning to `os.environ` doesn't clear the environment".to_string()
    }

    fn placeholder() -> Self {
        AssignmentToOsEnviron
    }
}

define_violation!(
    pub struct UnreliableCallableCheck;
);
impl Violation for UnreliableCallableCheck {
    fn message(&self) -> String {
        " Using `hasattr(x, '__call__')` to test if x is callable is unreliable. Use `callable(x)` \
         for consistent results."
            .to_string()
    }

    fn placeholder() -> Self {
        UnreliableCallableCheck
    }
}

define_violation!(
    pub struct StripWithMultiCharacters;
);
impl Violation for StripWithMultiCharacters {
    fn message(&self) -> String {
        "Using `.strip()` with multi-character strings is misleading the reader".to_string()
    }

    fn placeholder() -> Self {
        StripWithMultiCharacters
    }
}

define_violation!(
    pub struct MutableArgumentDefault;
);
impl Violation for MutableArgumentDefault {
    fn message(&self) -> String {
        "Do not use mutable data structures for argument defaults".to_string()
    }

    fn placeholder() -> Self {
        MutableArgumentDefault
    }
}

define_violation!(
    pub struct UnusedLoopControlVariable(pub String);
);
impl AlwaysAutofixableViolation for UnusedLoopControlVariable {
    fn message(&self) -> String {
        let UnusedLoopControlVariable(name) = self;
        format!(
            "Loop control variable `{name}` not used within the loop body. If this is intended, \
             start the name with an underscore."
        )
    }

    fn autofix_title(&self) -> String {
        let UnusedLoopControlVariable(name) = self;
        format!("Rename unused `{name}` to `_{name}`")
    }

    fn placeholder() -> Self {
        UnusedLoopControlVariable("i".to_string())
    }
}

define_violation!(
    pub struct FunctionCallArgumentDefault(pub Option<String>);
);
impl Violation for FunctionCallArgumentDefault {
    fn message(&self) -> String {
        let FunctionCallArgumentDefault(name) = self;
        if let Some(name) = name {
            format!("Do not perform function call `{name}` in argument defaults")
        } else {
            "Do not perform function call in argument defaults".to_string()
        }
    }

    fn placeholder() -> Self {
        FunctionCallArgumentDefault(None)
    }
}

define_violation!(
    pub struct GetAttrWithConstant;
);
impl AlwaysAutofixableViolation for GetAttrWithConstant {
    fn message(&self) -> String {
        "Do not call `getattr` with a constant attribute value. It is not any safer than normal \
         property access."
            .to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace `getattr` with attribute access".to_string()
    }

    fn placeholder() -> Self {
        GetAttrWithConstant
    }
}

define_violation!(
    pub struct SetAttrWithConstant;
);
impl AlwaysAutofixableViolation for SetAttrWithConstant {
    fn message(&self) -> String {
        "Do not call `setattr` with a constant attribute value. It is not any safer than normal \
         property access."
            .to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace `setattr` with assignment".to_string()
    }

    fn placeholder() -> Self {
        SetAttrWithConstant
    }
}

define_violation!(
    pub struct DoNotAssertFalse;
);
impl AlwaysAutofixableViolation for DoNotAssertFalse {
    fn message(&self) -> String {
        "Do not `assert False` (`python -O` removes these calls), raise `AssertionError()`"
            .to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace `assert False`".to_string()
    }

    fn placeholder() -> Self {
        DoNotAssertFalse
    }
}

define_violation!(
    pub struct JumpStatementInFinally(pub String);
);
impl Violation for JumpStatementInFinally {
    fn message(&self) -> String {
        let JumpStatementInFinally(name) = self;
        format!("`{name}` inside finally blocks cause exceptions to be silenced")
    }

    fn placeholder() -> Self {
        JumpStatementInFinally("return/continue/break".to_string())
    }
}

define_violation!(
    pub struct RedundantTupleInExceptionHandler(pub String);
);
impl AlwaysAutofixableViolation for RedundantTupleInExceptionHandler {
    fn message(&self) -> String {
        let RedundantTupleInExceptionHandler(name) = self;
        format!(
            "A length-one tuple literal is redundant. Write `except {name}` instead of `except \
             ({name},)`."
        )
    }

    fn autofix_title(&self) -> String {
        let RedundantTupleInExceptionHandler(name) = self;
        format!("Replace with `except {name}`")
    }

    fn placeholder() -> Self {
        RedundantTupleInExceptionHandler("ValueError".to_string())
    }
}

define_violation!(
    pub struct DuplicateHandlerException(pub Vec<String>);
);
impl AlwaysAutofixableViolation for DuplicateHandlerException {
    fn message(&self) -> String {
        let DuplicateHandlerException(names) = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Exception handler with duplicate exception: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Exception handler with duplicate exceptions: {names}")
        }
    }

    fn autofix_title(&self) -> String {
        "De-duplicate exceptions".to_string()
    }

    fn placeholder() -> Self {
        DuplicateHandlerException(vec!["ValueError".to_string()])
    }
}

define_violation!(
    pub struct UselessComparison;
);
impl Violation for UselessComparison {
    fn message(&self) -> String {
        "Pointless comparison. This comparison does nothing but waste CPU instructions. Either \
         prepend `assert` or remove it."
            .to_string()
    }

    fn placeholder() -> Self {
        UselessComparison
    }
}

define_violation!(
    pub struct CannotRaiseLiteral;
);
impl Violation for CannotRaiseLiteral {
    fn message(&self) -> String {
        "Cannot raise a literal. Did you intend to return it or raise an Exception?".to_string()
    }

    fn placeholder() -> Self {
        CannotRaiseLiteral
    }
}

define_violation!(
    pub struct NoAssertRaisesException;
);
impl Violation for NoAssertRaisesException {
    fn message(&self) -> String {
        "`assertRaises(Exception)` should be considered evil. It can lead to your test passing \
         even if the code being tested is never executed due to a typo. Either assert for a more \
         specific exception (builtin or custom), use `assertRaisesRegex`, or use the context \
         manager form of `assertRaises`."
            .to_string()
    }

    fn placeholder() -> Self {
        NoAssertRaisesException
    }
}

define_violation!(
    pub struct UselessExpression;
);
impl Violation for UselessExpression {
    fn message(&self) -> String {
        "Found useless expression. Either assign it to a variable or remove it.".to_string()
    }

    fn placeholder() -> Self {
        UselessExpression
    }
}

define_violation!(
    pub struct CachedInstanceMethod;
);
impl Violation for CachedInstanceMethod {
    fn message(&self) -> String {
        "Use of `functools.lru_cache` or `functools.cache` on methods can lead to memory leaks"
            .to_string()
    }

    fn placeholder() -> Self {
        CachedInstanceMethod
    }
}

define_violation!(
    pub struct LoopVariableOverridesIterator(pub String);
);
impl Violation for LoopVariableOverridesIterator {
    fn message(&self) -> String {
        let LoopVariableOverridesIterator(name) = self;
        format!("Loop control variable `{name}` overrides iterable it iterates")
    }

    fn placeholder() -> Self {
        LoopVariableOverridesIterator("...".to_string())
    }
}

define_violation!(
    pub struct FStringDocstring;
);
impl Violation for FStringDocstring {
    fn message(&self) -> String {
        "f-string used as docstring. This will be interpreted by python as a joined string rather \
         than a docstring."
            .to_string()
    }

    fn placeholder() -> Self {
        FStringDocstring
    }
}

define_violation!(
    pub struct UselessContextlibSuppress;
);
impl Violation for UselessContextlibSuppress {
    fn message(&self) -> String {
        "No arguments passed to `contextlib.suppress`. No exceptions will be suppressed and \
         therefore this context manager is redundant"
            .to_string()
    }

    fn placeholder() -> Self {
        UselessContextlibSuppress
    }
}

define_violation!(
    pub struct FunctionUsesLoopVariable(pub String);
);
impl Violation for FunctionUsesLoopVariable {
    fn message(&self) -> String {
        let FunctionUsesLoopVariable(name) = self;
        format!("Function definition does not bind loop variable `{name}`")
    }

    fn placeholder() -> Self {
        FunctionUsesLoopVariable("...".to_string())
    }
}

define_violation!(
    pub struct AbstractBaseClassWithoutAbstractMethod(pub String);
);
impl Violation for AbstractBaseClassWithoutAbstractMethod {
    fn message(&self) -> String {
        let AbstractBaseClassWithoutAbstractMethod(name) = self;
        format!("`{name}` is an abstract base class, but it has no abstract methods")
    }

    fn placeholder() -> Self {
        AbstractBaseClassWithoutAbstractMethod("...".to_string())
    }
}

define_violation!(
    pub struct DuplicateTryBlockException(pub String);
);
impl Violation for DuplicateTryBlockException {
    fn message(&self) -> String {
        let DuplicateTryBlockException(name) = self;
        format!("try-except block with duplicate exception `{name}`")
    }

    fn placeholder() -> Self {
        DuplicateTryBlockException("Exception".to_string())
    }
}

define_violation!(
    pub struct StarArgUnpackingAfterKeywordArg;
);
impl Violation for StarArgUnpackingAfterKeywordArg {
    fn message(&self) -> String {
        "Star-arg unpacking after a keyword argument is strongly discouraged. It only works when \
         the keyword parameter is declared after all parameters supplied by the unpacked sequence, \
         and this change of ordering can surprise and mislead readers."
            .to_string()
    }

    fn placeholder() -> Self {
        StarArgUnpackingAfterKeywordArg
    }
}

define_violation!(
    pub struct EmptyMethodWithoutAbstractDecorator(pub String);
);
impl Violation for EmptyMethodWithoutAbstractDecorator {
    fn message(&self) -> String {
        let EmptyMethodWithoutAbstractDecorator(name) = self;
        format!(
            "`{name}` is an empty method in an abstract base class, but has no abstract decorator"
        )
    }

    fn placeholder() -> Self {
        EmptyMethodWithoutAbstractDecorator("...".to_string())
    }
}

define_violation!(
    pub struct RaiseWithoutFromInsideExcept;
);
impl Violation for RaiseWithoutFromInsideExcept {
    fn message(&self) -> String {
        "Within an except clause, raise exceptions with `raise ... from err` or `raise ... from \
         None` to distinguish them from errors in exception handling"
            .to_string()
    }

    fn placeholder() -> Self {
        RaiseWithoutFromInsideExcept
    }
}

define_violation!(
    pub struct ZipWithoutExplicitStrict;
);
impl Violation for ZipWithoutExplicitStrict {
    fn message(&self) -> String {
        "`zip()` without an explicit `strict=` parameter".to_string()
    }

    fn placeholder() -> Self {
        ZipWithoutExplicitStrict
    }
}

// flake8-blind-except

define_violation!(
    pub struct BlindExcept(pub String);
);
impl Violation for BlindExcept {
    fn message(&self) -> String {
        let BlindExcept(name) = self;
        format!("Do not catch blind exception: `{name}`")
    }

    fn placeholder() -> Self {
        BlindExcept("Exception".to_string())
    }
}

// flake8-comprehensions

define_violation!(
    pub struct UnnecessaryGeneratorList;
);
impl AlwaysAutofixableViolation for UnnecessaryGeneratorList {
    fn message(&self) -> String {
        "Unnecessary generator (rewrite as a `list` comprehension)".to_string()
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `list` comprehension".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryGeneratorList
    }
}

define_violation!(
    pub struct UnnecessaryGeneratorSet;
);
impl AlwaysAutofixableViolation for UnnecessaryGeneratorSet {
    fn message(&self) -> String {
        "Unnecessary generator (rewrite as a `set` comprehension)".to_string()
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryGeneratorSet
    }
}

define_violation!(
    pub struct UnnecessaryGeneratorDict;
);
impl AlwaysAutofixableViolation for UnnecessaryGeneratorDict {
    fn message(&self) -> String {
        "Unnecessary generator (rewrite as a `dict` comprehension)".to_string()
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryGeneratorDict
    }
}

define_violation!(
    pub struct UnnecessaryListComprehensionSet;
);
impl AlwaysAutofixableViolation for UnnecessaryListComprehensionSet {
    fn message(&self) -> String {
        "Unnecessary `list` comprehension (rewrite as a `set` comprehension)".to_string()
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryListComprehensionSet
    }
}

define_violation!(
    pub struct UnnecessaryListComprehensionDict;
);
impl AlwaysAutofixableViolation for UnnecessaryListComprehensionDict {
    fn message(&self) -> String {
        "Unnecessary `list` comprehension (rewrite as a `dict` comprehension)".to_string()
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryListComprehensionDict
    }
}

define_violation!(
    pub struct UnnecessaryLiteralSet(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralSet {
    fn message(&self) -> String {
        let UnnecessaryLiteralSet(obj_type) = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `set` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` literal".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryLiteralSet("(list|tuple)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryLiteralDict(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralDict {
    fn message(&self) -> String {
        let UnnecessaryLiteralDict(obj_type) = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `dict` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` literal".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryLiteralDict("(list|tuple)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryCollectionCall(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryCollectionCall {
    fn message(&self) -> String {
        let UnnecessaryCollectionCall(obj_type) = self;
        format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a literal".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryCollectionCall("(dict|list|tuple)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryLiteralWithinTupleCall(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralWithinTupleCall {
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall(literal) = self;
        if literal == "list" {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (rewrite as a `tuple` \
                 literal)"
            )
        } else {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (remove the outer call to \
                 `tuple()`)"
            )
        }
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall(literal) = self;
        {
            if literal == "list" {
                "Rewrite as a `tuple` literal".to_string()
            } else {
                "Remove outer `tuple` call".to_string()
            }
        }
    }

    fn placeholder() -> Self {
        UnnecessaryLiteralWithinTupleCall("(list|tuple)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryLiteralWithinListCall(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralWithinListCall {
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinListCall(literal) = self;
        if literal == "list" {
            format!(
                "Unnecessary `{literal}` literal passed to `list()` (remove the outer call to \
                 `list()`)"
            )
        } else {
            format!(
                "Unnecessary `{literal}` literal passed to `list()` (rewrite as a `list` literal)"
            )
        }
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryLiteralWithinListCall(literal) = self;
        {
            if literal == "list" {
                "Remove outer `list` call".to_string()
            } else {
                "Rewrite as a `list` literal".to_string()
            }
        }
    }

    fn placeholder() -> Self {
        UnnecessaryLiteralWithinListCall("(list|tuple)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryListCall;
);
impl AlwaysAutofixableViolation for UnnecessaryListCall {
    fn message(&self) -> String {
        "Unnecessary `list` call (remove the outer call to `list()`)".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove outer `list` call".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryListCall
    }
}

define_violation!(
    pub struct UnnecessaryCallAroundSorted(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryCallAroundSorted {
    fn message(&self) -> String {
        let UnnecessaryCallAroundSorted(func) = self;
        format!("Unnecessary `{func}` call around `sorted()`")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryCallAroundSorted(func) = self;
        format!("Remove unnecessary `{func}` call")
    }

    fn placeholder() -> Self {
        UnnecessaryCallAroundSorted("(list|reversed)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryDoubleCastOrProcess(pub String, pub String);
);
impl Violation for UnnecessaryDoubleCastOrProcess {
    fn message(&self) -> String {
        let UnnecessaryDoubleCastOrProcess(inner, outer) = self;
        format!("Unnecessary `{inner}` call within `{outer}()`")
    }

    fn placeholder() -> Self {
        UnnecessaryDoubleCastOrProcess(
            "(list|reversed|set|sorted|tuple)".to_string(),
            "(list|set|sorted|tuple)".to_string(),
        )
    }
}

define_violation!(
    pub struct UnnecessarySubscriptReversal(pub String);
);
impl Violation for UnnecessarySubscriptReversal {
    fn message(&self) -> String {
        let UnnecessarySubscriptReversal(func) = self;
        format!("Unnecessary subscript reversal of iterable within `{func}()`")
    }

    fn placeholder() -> Self {
        UnnecessarySubscriptReversal("(reversed|set|sorted)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryComprehension(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryComprehension {
    fn message(&self) -> String {
        let UnnecessaryComprehension(obj_type) = self;
        format!("Unnecessary `{obj_type}` comprehension (rewrite using `{obj_type}()`)")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryComprehension(obj_type) = self;
        format!("Rewrite using `{obj_type}()`")
    }

    fn placeholder() -> Self {
        UnnecessaryComprehension("(list|set)".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryMap(pub String);
);
impl Violation for UnnecessaryMap {
    fn message(&self) -> String {
        let UnnecessaryMap(obj_type) = self;
        if obj_type == "generator" {
            "Unnecessary `map` usage (rewrite using a generator expression)".to_string()
        } else {
            format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
        }
    }

    fn placeholder() -> Self {
        UnnecessaryMap("(list|set|dict)".to_string())
    }
}

// flake8-debugger

define_violation!(
    pub struct Debugger(pub DebuggerUsingType);
);
impl Violation for Debugger {
    fn message(&self) -> String {
        let Debugger(using_type) = self;
        match using_type {
            DebuggerUsingType::Call(name) => format!("Trace found: `{name}` used"),
            DebuggerUsingType::Import(name) => format!("Import for `{name}` found"),
        }
    }

    fn placeholder() -> Self {
        Debugger(DebuggerUsingType::Import("...".to_string()))
    }
}

// mccabe

define_violation!(
    pub struct FunctionIsTooComplex(pub String, pub usize);
);
impl Violation for FunctionIsTooComplex {
    fn message(&self) -> String {
        let FunctionIsTooComplex(name, complexity) = self;
        format!("`{name}` is too complex ({complexity})")
    }

    fn placeholder() -> Self {
        FunctionIsTooComplex("...".to_string(), 10)
    }
}

// flake8-return

define_violation!(
    pub struct UnnecessaryReturnNone;
);
impl AlwaysAutofixableViolation for UnnecessaryReturnNone {
    fn message(&self) -> String {
        "Do not explicitly `return None` in function if it is the only possible return value"
            .to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove explicit `return None`".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryReturnNone
    }
}

define_violation!(
    pub struct ImplicitReturnValue;
);
impl AlwaysAutofixableViolation for ImplicitReturnValue {
    fn message(&self) -> String {
        "Do not implicitly `return None` in function able to return non-`None` value".to_string()
    }

    fn autofix_title(&self) -> String {
        "Add explicit `None` return value".to_string()
    }

    fn placeholder() -> Self {
        ImplicitReturnValue
    }
}

define_violation!(
    pub struct ImplicitReturn;
);
impl AlwaysAutofixableViolation for ImplicitReturn {
    fn message(&self) -> String {
        "Missing explicit `return` at the end of function able to return non-`None` value"
            .to_string()
    }

    fn autofix_title(&self) -> String {
        "Add explicit `return` statement".to_string()
    }

    fn placeholder() -> Self {
        ImplicitReturn
    }
}

define_violation!(
    pub struct UnnecessaryAssign;
);
impl Violation for UnnecessaryAssign {
    fn message(&self) -> String {
        "Unnecessary variable assignment before `return` statement".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryAssign
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Branch {
    Elif,
    Else,
}

impl fmt::Display for Branch {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Branch::Elif => fmt.write_str("elif"),
            Branch::Else => fmt.write_str("else"),
        }
    }
}

define_violation!(
    pub struct SuperfluousElseReturn(pub Branch);
);
impl Violation for SuperfluousElseReturn {
    fn message(&self) -> String {
        let SuperfluousElseReturn(branch) = self;
        format!("Unnecessary `{branch}` after `return` statement")
    }

    fn placeholder() -> Self {
        SuperfluousElseReturn(Branch::Else)
    }
}

define_violation!(
    pub struct SuperfluousElseRaise(pub Branch);
);
impl Violation for SuperfluousElseRaise {
    fn message(&self) -> String {
        let SuperfluousElseRaise(branch) = self;
        format!("Unnecessary `{branch}` after `raise` statement")
    }

    fn placeholder() -> Self {
        SuperfluousElseRaise(Branch::Else)
    }
}

define_violation!(
    pub struct SuperfluousElseContinue(pub Branch);
);
impl Violation for SuperfluousElseContinue {
    fn message(&self) -> String {
        let SuperfluousElseContinue(branch) = self;
        format!("Unnecessary `{branch}` after `continue` statement")
    }

    fn placeholder() -> Self {
        SuperfluousElseContinue(Branch::Else)
    }
}

define_violation!(
    pub struct SuperfluousElseBreak(pub Branch);
);
impl Violation for SuperfluousElseBreak {
    fn message(&self) -> String {
        let SuperfluousElseBreak(branch) = self;
        format!("Unnecessary `{branch}` after `break` statement")
    }

    fn placeholder() -> Self {
        SuperfluousElseBreak(Branch::Else)
    }
}

// flake8-implicit-str-concat

define_violation!(
    pub struct SingleLineImplicitStringConcatenation;
);
impl Violation for SingleLineImplicitStringConcatenation {
    fn message(&self) -> String {
        "Implicitly concatenated string literals on one line".to_string()
    }

    fn placeholder() -> Self {
        SingleLineImplicitStringConcatenation
    }
}

define_violation!(
    pub struct MultiLineImplicitStringConcatenation;
);
impl Violation for MultiLineImplicitStringConcatenation {
    fn message(&self) -> String {
        "Implicitly concatenated string literals over continuation line".to_string()
    }

    fn placeholder() -> Self {
        MultiLineImplicitStringConcatenation
    }
}

define_violation!(
    pub struct ExplicitStringConcatenation;
);
impl Violation for ExplicitStringConcatenation {
    fn message(&self) -> String {
        "Explicitly concatenated string should be implicitly concatenated".to_string()
    }

    fn placeholder() -> Self {
        ExplicitStringConcatenation
    }
}

// flake8-print

define_violation!(
    pub struct PrintFound;
);
impl AlwaysAutofixableViolation for PrintFound {
    fn message(&self) -> String {
        "`print` found".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `print`".to_string()
    }

    fn placeholder() -> Self {
        PrintFound
    }
}

define_violation!(
    pub struct PPrintFound;
);
impl AlwaysAutofixableViolation for PPrintFound {
    fn message(&self) -> String {
        "`pprint` found".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `pprint`".to_string()
    }

    fn placeholder() -> Self {
        PPrintFound
    }
}

// flake8-quotes

define_violation!(
    pub struct BadQuotesInlineString(pub Quote);
);
impl AlwaysAutofixableViolation for BadQuotesInlineString {
    fn message(&self) -> String {
        let BadQuotesInlineString(quote) = self;
        match quote {
            Quote::Single => "Double quotes found but single quotes preferred".to_string(),
            Quote::Double => "Single quotes found but double quotes preferred".to_string(),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesInlineString(quote) = self;
        match quote {
            Quote::Single => "Replace double quotes with single quotes".to_string(),
            Quote::Double => "Replace single quotes with double quotes".to_string(),
        }
    }

    fn placeholder() -> Self {
        BadQuotesInlineString(Quote::Double)
    }
}

define_violation!(
    pub struct BadQuotesMultilineString(pub Quote);
);
impl AlwaysAutofixableViolation for BadQuotesMultilineString {
    fn message(&self) -> String {
        let BadQuotesMultilineString(quote) = self;
        match quote {
            Quote::Single => "Double quote multiline found but single quotes preferred".to_string(),
            Quote::Double => "Single quote multiline found but double quotes preferred".to_string(),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesMultilineString(quote) = self;
        match quote {
            Quote::Single => "Replace double multiline quotes with single quotes".to_string(),
            Quote::Double => "Replace single multiline quotes with double quotes".to_string(),
        }
    }

    fn placeholder() -> Self {
        BadQuotesMultilineString(Quote::Double)
    }
}

define_violation!(
    pub struct BadQuotesDocstring(pub Quote);
);
impl AlwaysAutofixableViolation for BadQuotesDocstring {
    fn message(&self) -> String {
        let BadQuotesDocstring(quote) = self;
        match quote {
            Quote::Single => "Double quote docstring found but single quotes preferred".to_string(),
            Quote::Double => "Single quote docstring found but double quotes preferred".to_string(),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesDocstring(quote) = self;
        match quote {
            Quote::Single => "Replace double quotes docstring with single quotes".to_string(),
            Quote::Double => "Replace single quotes docstring with double quotes".to_string(),
        }
    }

    fn placeholder() -> Self {
        BadQuotesDocstring(Quote::Double)
    }
}

define_violation!(
    pub struct AvoidQuoteEscape;
);
impl AlwaysAutofixableViolation for AvoidQuoteEscape {
    fn message(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }

    fn autofix_title(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }

    fn placeholder() -> Self {
        AvoidQuoteEscape
    }
}

// flake8-annotations

define_violation!(
    pub struct MissingTypeFunctionArgument(pub String);
);
impl Violation for MissingTypeFunctionArgument {
    fn message(&self) -> String {
        let MissingTypeFunctionArgument(name) = self;
        format!("Missing type annotation for function argument `{name}`")
    }

    fn placeholder() -> Self {
        MissingTypeFunctionArgument("...".to_string())
    }
}

define_violation!(
    pub struct MissingTypeArgs(pub String);
);
impl Violation for MissingTypeArgs {
    fn message(&self) -> String {
        let MissingTypeArgs(name) = self;
        format!("Missing type annotation for `*{name}`")
    }

    fn placeholder() -> Self {
        MissingTypeArgs("...".to_string())
    }
}

define_violation!(
    pub struct MissingTypeKwargs(pub String);
);
impl Violation for MissingTypeKwargs {
    fn message(&self) -> String {
        let MissingTypeKwargs(name) = self;
        format!("Missing type annotation for `**{name}`")
    }

    fn placeholder() -> Self {
        MissingTypeKwargs("...".to_string())
    }
}

define_violation!(
    pub struct MissingTypeSelf(pub String);
);
impl Violation for MissingTypeSelf {
    fn message(&self) -> String {
        let MissingTypeSelf(name) = self;
        format!("Missing type annotation for `{name}` in method")
    }

    fn placeholder() -> Self {
        MissingTypeSelf("...".to_string())
    }
}

define_violation!(
    pub struct MissingTypeCls(pub String);
);
impl Violation for MissingTypeCls {
    fn message(&self) -> String {
        let MissingTypeCls(name) = self;
        format!("Missing type annotation for `{name}` in classmethod")
    }

    fn placeholder() -> Self {
        MissingTypeCls("...".to_string())
    }
}

define_violation!(
    pub struct MissingReturnTypePublicFunction(pub String);
);
impl Violation for MissingReturnTypePublicFunction {
    fn message(&self) -> String {
        let MissingReturnTypePublicFunction(name) = self;
        format!("Missing return type annotation for public function `{name}`")
    }

    fn placeholder() -> Self {
        MissingReturnTypePublicFunction("...".to_string())
    }
}

define_violation!(
    pub struct MissingReturnTypePrivateFunction(pub String);
);
impl Violation for MissingReturnTypePrivateFunction {
    fn message(&self) -> String {
        let MissingReturnTypePrivateFunction(name) = self;
        format!("Missing return type annotation for private function `{name}`")
    }

    fn placeholder() -> Self {
        MissingReturnTypePrivateFunction("...".to_string())
    }
}

define_violation!(
    pub struct MissingReturnTypeSpecialMethod(pub String);
);
impl AlwaysAutofixableViolation for MissingReturnTypeSpecialMethod {
    fn message(&self) -> String {
        let MissingReturnTypeSpecialMethod(name) = self;
        format!("Missing return type annotation for special method `{name}`")
    }

    fn autofix_title(&self) -> String {
        "Add `None` return type".to_string()
    }

    fn placeholder() -> Self {
        MissingReturnTypeSpecialMethod("...".to_string())
    }
}

define_violation!(
    pub struct MissingReturnTypeStaticMethod(pub String);
);
impl Violation for MissingReturnTypeStaticMethod {
    fn message(&self) -> String {
        let MissingReturnTypeStaticMethod(name) = self;
        format!("Missing return type annotation for staticmethod `{name}`")
    }

    fn placeholder() -> Self {
        MissingReturnTypeStaticMethod("...".to_string())
    }
}

define_violation!(
    pub struct MissingReturnTypeClassMethod(pub String);
);
impl Violation for MissingReturnTypeClassMethod {
    fn message(&self) -> String {
        let MissingReturnTypeClassMethod(name) = self;
        format!("Missing return type annotation for classmethod `{name}`")
    }

    fn placeholder() -> Self {
        MissingReturnTypeClassMethod("...".to_string())
    }
}

define_violation!(
    pub struct DynamicallyTypedExpression(pub String);
);
impl Violation for DynamicallyTypedExpression {
    fn message(&self) -> String {
        let DynamicallyTypedExpression(name) = self;
        format!("Dynamically typed expressions (typing.Any) are disallowed in `{name}`")
    }

    fn placeholder() -> Self {
        DynamicallyTypedExpression("...".to_string())
    }
}

// flake8-2020

define_violation!(
    pub struct SysVersionSlice3Referenced;
);
impl Violation for SysVersionSlice3Referenced {
    fn message(&self) -> String {
        "`sys.version[:3]` referenced (python3.10), use `sys.version_info`".to_string()
    }

    fn placeholder() -> Self {
        SysVersionSlice3Referenced
    }
}

define_violation!(
    pub struct SysVersion2Referenced;
);
impl Violation for SysVersion2Referenced {
    fn message(&self) -> String {
        "`sys.version[2]` referenced (python3.10), use `sys.version_info`".to_string()
    }

    fn placeholder() -> Self {
        SysVersion2Referenced
    }
}

define_violation!(
    pub struct SysVersionCmpStr3;
);
impl Violation for SysVersionCmpStr3 {
    fn message(&self) -> String {
        "`sys.version` compared to string (python3.10), use `sys.version_info`".to_string()
    }

    fn placeholder() -> Self {
        SysVersionCmpStr3
    }
}

define_violation!(
    pub struct SysVersionInfo0Eq3Referenced;
);
impl Violation for SysVersionInfo0Eq3Referenced {
    fn message(&self) -> String {
        "`sys.version_info[0] == 3` referenced (python4), use `>=`".to_string()
    }

    fn placeholder() -> Self {
        SysVersionInfo0Eq3Referenced
    }
}

define_violation!(
    pub struct SixPY3Referenced;
);
impl Violation for SixPY3Referenced {
    fn message(&self) -> String {
        "`six.PY3` referenced (python4), use `not six.PY2`".to_string()
    }

    fn placeholder() -> Self {
        SixPY3Referenced
    }
}

define_violation!(
    pub struct SysVersionInfo1CmpInt;
);
impl Violation for SysVersionInfo1CmpInt {
    fn message(&self) -> String {
        "`sys.version_info[1]` compared to integer (python4), compare `sys.version_info` to tuple"
            .to_string()
    }

    fn placeholder() -> Self {
        SysVersionInfo1CmpInt
    }
}

define_violation!(
    pub struct SysVersionInfoMinorCmpInt;
);
impl Violation for SysVersionInfoMinorCmpInt {
    fn message(&self) -> String {
        "`sys.version_info.minor` compared to integer (python4), compare `sys.version_info` to \
         tuple"
            .to_string()
    }

    fn placeholder() -> Self {
        SysVersionInfoMinorCmpInt
    }
}

define_violation!(
    pub struct SysVersion0Referenced;
);
impl Violation for SysVersion0Referenced {
    fn message(&self) -> String {
        "`sys.version[0]` referenced (python10), use `sys.version_info`".to_string()
    }

    fn placeholder() -> Self {
        SysVersion0Referenced
    }
}

define_violation!(
    pub struct SysVersionCmpStr10;
);
impl Violation for SysVersionCmpStr10 {
    fn message(&self) -> String {
        "`sys.version` compared to string (python10), use `sys.version_info`".to_string()
    }

    fn placeholder() -> Self {
        SysVersionCmpStr10
    }
}

define_violation!(
    pub struct SysVersionSlice1Referenced;
);
impl Violation for SysVersionSlice1Referenced {
    fn message(&self) -> String {
        "`sys.version[:1]` referenced (python10), use `sys.version_info`".to_string()
    }

    fn placeholder() -> Self {
        SysVersionSlice1Referenced
    }
}

// flake8-simplify

define_violation!(
    pub struct OpenFileWithContextHandler;
);
impl Violation for OpenFileWithContextHandler {
    fn message(&self) -> String {
        "Use context handler for opening files".to_string()
    }

    fn placeholder() -> Self {
        OpenFileWithContextHandler
    }
}

define_violation!(
    pub struct UseCapitalEnvironmentVariables(pub String, pub String);
);
impl AlwaysAutofixableViolation for UseCapitalEnvironmentVariables {
    fn message(&self) -> String {
        let UseCapitalEnvironmentVariables(expected, original) = self;
        format!("Use capitalized environment variable `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> String {
        let UseCapitalEnvironmentVariables(expected, original) = self;
        format!("Replace `{original}` with `{expected}`")
    }

    fn placeholder() -> Self {
        UseCapitalEnvironmentVariables("...".to_string(), "...".to_string())
    }
}

define_violation!(
    pub struct DuplicateIsinstanceCall(pub String);
);
impl AlwaysAutofixableViolation for DuplicateIsinstanceCall {
    fn message(&self) -> String {
        let DuplicateIsinstanceCall(name) = self;
        format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
    }

    fn autofix_title(&self) -> String {
        let DuplicateIsinstanceCall(name) = self;
        format!("Merge `isinstance` calls for `{name}`")
    }

    fn placeholder() -> Self {
        DuplicateIsinstanceCall("...".to_string())
    }
}

define_violation!(
    pub struct NestedIfStatements;
);
impl Violation for NestedIfStatements {
    fn message(&self) -> String {
        "Use a single `if` statement instead of nested `if` statements".to_string()
    }

    fn placeholder() -> Self {
        NestedIfStatements
    }
}

define_violation!(
    pub struct ReturnBoolConditionDirectly(pub String);
);
impl AlwaysAutofixableViolation for ReturnBoolConditionDirectly {
    fn message(&self) -> String {
        let ReturnBoolConditionDirectly(cond) = self;
        format!("Return the condition `{cond}` directly")
    }

    fn autofix_title(&self) -> String {
        let ReturnBoolConditionDirectly(cond) = self;
        format!("Replace with `return {cond}`")
    }

    fn placeholder() -> Self {
        ReturnBoolConditionDirectly("...".to_string())
    }
}

define_violation!(
    pub struct UseContextlibSuppress(pub String);
);
impl Violation for UseContextlibSuppress {
    fn message(&self) -> String {
        let UseContextlibSuppress(exception) = self;
        format!("Use `contextlib.suppress({exception})` instead of try-except-pass")
    }

    fn placeholder() -> Self {
        UseContextlibSuppress("...".to_string())
    }
}

define_violation!(
    pub struct ReturnInTryExceptFinally;
);
impl Violation for ReturnInTryExceptFinally {
    fn message(&self) -> String {
        "Don't use `return` in `try`/`except` and `finally`".to_string()
    }

    fn placeholder() -> Self {
        ReturnInTryExceptFinally
    }
}

define_violation!(
    pub struct UseTernaryOperator(pub String);
);
impl AlwaysAutofixableViolation for UseTernaryOperator {
    fn message(&self) -> String {
        let UseTernaryOperator(contents) = self;
        format!("Use ternary operator `{contents}` instead of if-else-block")
    }

    fn autofix_title(&self) -> String {
        let UseTernaryOperator(contents) = self;
        format!("Replace if-else-block with `{contents}`")
    }

    fn placeholder() -> Self {
        UseTernaryOperator("...".to_string())
    }
}

define_violation!(
    pub struct CompareWithTuple(pub String, pub Vec<String>, pub String);
);
impl AlwaysAutofixableViolation for CompareWithTuple {
    fn message(&self) -> String {
        let CompareWithTuple(value, values, or_op) = self;
        let values = values.join(", ");
        format!("Use `{value} in ({values})` instead of `{or_op}`")
    }

    fn autofix_title(&self) -> String {
        let CompareWithTuple(value, values, or_op) = self;
        let values = values.join(", ");
        format!("Replace `{or_op}` with `{value} in {values}`")
    }

    fn placeholder() -> Self {
        CompareWithTuple(
            "value".to_string(),
            vec!["...".to_string(), "...".to_string()],
            "value == ... or value == ...".to_string(),
        )
    }
}

define_violation!(
    pub struct ConvertLoopToAny(pub String);
);
impl AlwaysAutofixableViolation for ConvertLoopToAny {
    fn message(&self) -> String {
        let ConvertLoopToAny(any) = self;
        format!("Use `{any}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ConvertLoopToAny(any) = self;
        format!("Replace with `{any}`")
    }

    fn placeholder() -> Self {
        ConvertLoopToAny("return any(x for x in y)".to_string())
    }
}

define_violation!(
    pub struct ConvertLoopToAll(pub String);
);
impl AlwaysAutofixableViolation for ConvertLoopToAll {
    fn message(&self) -> String {
        let ConvertLoopToAll(all) = self;
        format!("Use `{all}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ConvertLoopToAll(all) = self;
        format!("Replace with `{all}`")
    }

    fn placeholder() -> Self {
        ConvertLoopToAll("return all(x for x in y)".to_string())
    }
}

define_violation!(
    pub struct MultipleWithStatements;
);
impl Violation for MultipleWithStatements {
    fn message(&self) -> String {
        "Use a single `with` statement with multiple contexts instead of nested `with` statements"
            .to_string()
    }

    fn placeholder() -> Self {
        MultipleWithStatements
    }
}

define_violation!(
    pub struct KeyInDict(pub String, pub String);
);
impl AlwaysAutofixableViolation for KeyInDict {
    fn message(&self) -> String {
        let KeyInDict(key, dict) = self;
        format!("Use `{key} in {dict}` instead of `{key} in {dict}.keys()`")
    }

    fn autofix_title(&self) -> String {
        let KeyInDict(key, dict) = self;
        format!("Convert to `{key} in {dict}`")
    }

    fn placeholder() -> Self {
        KeyInDict("key".to_string(), "dict".to_string())
    }
}

define_violation!(
    pub struct NegateEqualOp(pub String, pub String);
);
impl AlwaysAutofixableViolation for NegateEqualOp {
    fn message(&self) -> String {
        let NegateEqualOp(left, right) = self;
        format!("Use `{left} != {right}` instead of `not {left} == {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `!=` operator".to_string()
    }

    fn placeholder() -> Self {
        NegateEqualOp("left".to_string(), "right".to_string())
    }
}

define_violation!(
    pub struct NegateNotEqualOp(pub String, pub String);
);
impl AlwaysAutofixableViolation for NegateNotEqualOp {
    fn message(&self) -> String {
        let NegateNotEqualOp(left, right) = self;
        format!("Use `{left} == {right}` instead of `not {left} != {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `==` operator".to_string()
    }

    fn placeholder() -> Self {
        NegateNotEqualOp("left".to_string(), "right".to_string())
    }
}

define_violation!(
    pub struct DoubleNegation(pub String);
);
impl AlwaysAutofixableViolation for DoubleNegation {
    fn message(&self) -> String {
        let DoubleNegation(expr) = self;
        format!("Use `{expr}` instead of `not (not {expr})`")
    }

    fn autofix_title(&self) -> String {
        let DoubleNegation(expr) = self;
        format!("Replace with `{expr}`")
    }

    fn placeholder() -> Self {
        DoubleNegation("expr".to_string())
    }
}

define_violation!(
    pub struct AAndNotA(pub String);
);
impl AlwaysAutofixableViolation for AAndNotA {
    fn message(&self) -> String {
        let AAndNotA(name) = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }

    fn placeholder() -> Self {
        AAndNotA("...".to_string())
    }
}

define_violation!(
    pub struct AOrNotA(pub String);
);
impl AlwaysAutofixableViolation for AOrNotA {
    fn message(&self) -> String {
        let AOrNotA(name) = self;
        format!("Use `True` instead of `{name} or not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }

    fn placeholder() -> Self {
        AOrNotA("...".to_string())
    }
}

define_violation!(
    pub struct OrTrue;
);
impl AlwaysAutofixableViolation for OrTrue {
    fn message(&self) -> String {
        "Use `True` instead of `... or True`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }

    fn placeholder() -> Self {
        OrTrue
    }
}

define_violation!(
    pub struct AndFalse;
);
impl AlwaysAutofixableViolation for AndFalse {
    fn message(&self) -> String {
        "Use `False` instead of `... and False`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }

    fn placeholder() -> Self {
        AndFalse
    }
}

define_violation!(
    pub struct YodaConditions(pub String, pub String);
);
impl AlwaysAutofixableViolation for YodaConditions {
    fn message(&self) -> String {
        let YodaConditions(left, right) = self;
        format!("Yoda conditions are discouraged, use `{left} == {right}` instead")
    }

    fn autofix_title(&self) -> String {
        let YodaConditions(left, right) = self;
        format!("Replace Yoda condition with `{left} == {right}`")
    }

    fn placeholder() -> Self {
        YodaConditions("left".to_string(), "right".to_string())
    }
}

define_violation!(
    pub struct IfExprWithTrueFalse(pub String);
);
impl AlwaysAutofixableViolation for IfExprWithTrueFalse {
    fn message(&self) -> String {
        let IfExprWithTrueFalse(expr) = self;
        format!("Use `bool({expr})` instead of `True if {expr} else False`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTrueFalse(expr) = self;
        format!("Replace with `not {expr}")
    }

    fn placeholder() -> Self {
        IfExprWithTrueFalse("expr".to_string())
    }
}

define_violation!(
    pub struct IfExprWithFalseTrue(pub String);
);
impl AlwaysAutofixableViolation for IfExprWithFalseTrue {
    fn message(&self) -> String {
        let IfExprWithFalseTrue(expr) = self;
        format!("Use `not {expr}` instead of `False if {expr} else True`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithFalseTrue(expr) = self;
        format!("Replace with `bool({expr})")
    }

    fn placeholder() -> Self {
        IfExprWithFalseTrue("expr".to_string())
    }
}

define_violation!(
    pub struct IfExprWithTwistedArms(pub String, pub String);
);
impl AlwaysAutofixableViolation for IfExprWithTwistedArms {
    fn message(&self) -> String {
        let IfExprWithTwistedArms(expr_body, expr_else) = self;
        format!(
            "Use `{expr_else} if {expr_else} else {expr_body}` instead of `{expr_body} if not \
             {expr_else} else {expr_else}`"
        )
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTwistedArms(expr_body, expr_else) = self;
        format!("Replace with `{expr_else} if {expr_else} else {expr_body}`")
    }

    fn placeholder() -> Self {
        IfExprWithTwistedArms("a".to_string(), "b".to_string())
    }
}

define_violation!(
    pub struct DictGetWithDefault(pub String);
);
impl AlwaysAutofixableViolation for DictGetWithDefault {
    fn message(&self) -> String {
        let DictGetWithDefault(contents) = self;
        format!("Use `{contents}` instead of an `if` block")
    }

    fn autofix_title(&self) -> String {
        let DictGetWithDefault(contents) = self;
        format!("Replace with `{contents}`")
    }

    fn placeholder() -> Self {
        DictGetWithDefault("var = dict.get(key, \"default\")".to_string())
    }
}
// pyupgrade

define_violation!(
    pub struct UselessMetaclassType;
);
impl AlwaysAutofixableViolation for UselessMetaclassType {
    fn message(&self) -> String {
        "`__metaclass__ = type` is implied".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `__metaclass__ = type`".to_string()
    }

    fn placeholder() -> Self {
        UselessMetaclassType
    }
}

define_violation!(
    pub struct TypeOfPrimitive(pub Primitive);
);
impl AlwaysAutofixableViolation for TypeOfPrimitive {
    fn message(&self) -> String {
        let TypeOfPrimitive(primitive) = self;
        format!("Use `{}` instead of `type(...)`", primitive.builtin())
    }

    fn autofix_title(&self) -> String {
        let TypeOfPrimitive(primitive) = self;
        format!("Replace `type(...)` with `{}`", primitive.builtin())
    }

    fn placeholder() -> Self {
        TypeOfPrimitive(Primitive::Str)
    }
}

define_violation!(
    pub struct UselessObjectInheritance(pub String);
);
impl AlwaysAutofixableViolation for UselessObjectInheritance {
    fn message(&self) -> String {
        let UselessObjectInheritance(name) = self;
        format!("Class `{name}` inherits from `object`")
    }

    fn autofix_title(&self) -> String {
        "Remove `object` inheritance".to_string()
    }

    fn placeholder() -> Self {
        UselessObjectInheritance("...".to_string())
    }
}

define_violation!(
    pub struct DeprecatedUnittestAlias(pub String, pub String);
);
impl AlwaysAutofixableViolation for DeprecatedUnittestAlias {
    fn message(&self) -> String {
        let DeprecatedUnittestAlias(alias, target) = self;
        format!("`{alias}` is deprecated, use `{target}`")
    }

    fn autofix_title(&self) -> String {
        let DeprecatedUnittestAlias(alias, target) = self;
        format!("Replace `{target}` with `{alias}`")
    }

    fn placeholder() -> Self {
        DeprecatedUnittestAlias("assertEquals".to_string(), "assertEqual".to_string())
    }
}

define_violation!(
    pub struct UsePEP585Annotation(pub String);
);
impl AlwaysAutofixableViolation for UsePEP585Annotation {
    fn message(&self) -> String {
        let UsePEP585Annotation(name) = self;
        format!(
            "Use `{}` instead of `{}` for type annotations",
            name.to_lowercase(),
            name,
        )
    }

    fn autofix_title(&self) -> String {
        let UsePEP585Annotation(name) = self;
        format!("Replace `{name}` with `{}`", name.to_lowercase(),)
    }

    fn placeholder() -> Self {
        UsePEP585Annotation("List".to_string())
    }
}

define_violation!(
    pub struct UsePEP604Annotation;
);
impl AlwaysAutofixableViolation for UsePEP604Annotation {
    fn message(&self) -> String {
        "Use `X | Y` for type annotations".to_string()
    }

    fn autofix_title(&self) -> String {
        "Convert to `X | Y`".to_string()
    }

    fn placeholder() -> Self {
        UsePEP604Annotation
    }
}

define_violation!(
    pub struct SuperCallWithParameters;
);
impl AlwaysAutofixableViolation for SuperCallWithParameters {
    fn message(&self) -> String {
        "Use `super()` instead of `super(__class__, self)`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `__super__` parameters".to_string()
    }

    fn placeholder() -> Self {
        SuperCallWithParameters
    }
}

define_violation!(
    pub struct PEP3120UnnecessaryCodingComment;
);
impl AlwaysAutofixableViolation for PEP3120UnnecessaryCodingComment {
    fn message(&self) -> String {
        "UTF-8 encoding declaration is unnecessary".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary coding comment".to_string()
    }

    fn placeholder() -> Self {
        PEP3120UnnecessaryCodingComment
    }
}

define_violation!(
    pub struct UnnecessaryFutureImport(pub Vec<String>);
);
impl AlwaysAutofixableViolation for UnnecessaryFutureImport {
    fn message(&self) -> String {
        let UnnecessaryFutureImport(names) = self;
        if names.len() == 1 {
            let import = &names[0];
            format!("Unnecessary `__future__` import `{import}` for target Python version")
        } else {
            let imports = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Unnecessary `__future__` imports {imports} for target Python version")
        }
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `__future__` import".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryFutureImport(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct UnnecessaryLRUCacheParams;
);
impl AlwaysAutofixableViolation for UnnecessaryLRUCacheParams {
    fn message(&self) -> String {
        "Unnecessary parameters to `functools.lru_cache`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary parameters".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryLRUCacheParams
    }
}

define_violation!(
    pub struct UnnecessaryEncodeUTF8;
);
impl AlwaysAutofixableViolation for UnnecessaryEncodeUTF8 {
    fn message(&self) -> String {
        "Unnecessary call to `encode` as UTF-8".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `encode`".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryEncodeUTF8
    }
}

define_violation!(
    pub struct ConvertTypedDictFunctionalToClass(pub String);
);
impl AlwaysAutofixableViolation for ConvertTypedDictFunctionalToClass {
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass(name) = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertTypedDictFunctionalToClass(name) = self;
        format!("Convert `{name}` to class syntax")
    }

    fn placeholder() -> Self {
        ConvertTypedDictFunctionalToClass("...".to_string())
    }
}

define_violation!(
    pub struct ConvertNamedTupleFunctionalToClass(pub String);
);
impl AlwaysAutofixableViolation for ConvertNamedTupleFunctionalToClass {
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass(name) = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertNamedTupleFunctionalToClass(name) = self;
        format!("Convert `{name}` to class syntax")
    }

    fn placeholder() -> Self {
        ConvertNamedTupleFunctionalToClass("...".to_string())
    }
}

define_violation!(
    pub struct RedundantOpenModes(pub Option<String>);
);
impl AlwaysAutofixableViolation for RedundantOpenModes {
    fn message(&self) -> String {
        let RedundantOpenModes(replacement) = self;
        match replacement {
            None => "Unnecessary open mode parameters".to_string(),
            Some(replacement) => {
                format!("Unnecessary open mode parameters, use \"{replacement}\"")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let RedundantOpenModes(replacement) = self;
        match replacement {
            None => "Remove open mode parameters".to_string(),
            Some(replacement) => {
                format!("Replace with \"{replacement}\"")
            }
        }
    }

    fn placeholder() -> Self {
        RedundantOpenModes(None)
    }
}

define_violation!(
    pub struct RemoveSixCompat;
);
impl AlwaysAutofixableViolation for RemoveSixCompat {
    fn message(&self) -> String {
        "Unnecessary `six` compatibility usage".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `six` usage".to_string()
    }

    fn placeholder() -> Self {
        RemoveSixCompat
    }
}

define_violation!(
    pub struct DatetimeTimezoneUTC {
        pub straight_import: bool,
    }
);
impl Violation for DatetimeTimezoneUTC {
    fn message(&self) -> String {
        "Use `datetime.UTC` alias".to_string()
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        if self.straight_import {
            Some(|_| "Convert to `datetime.UTC` alias".to_string())
        } else {
            None
        }
    }

    fn placeholder() -> Self {
        DatetimeTimezoneUTC {
            straight_import: true,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiteralType {
    Str,
    Bytes,
}

impl fmt::Display for LiteralType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LiteralType::Str => fmt.write_str("str"),
            LiteralType::Bytes => fmt.write_str("bytes"),
        }
    }
}

define_violation!(
    pub struct NativeLiterals(pub LiteralType);
);
impl AlwaysAutofixableViolation for NativeLiterals {
    fn message(&self) -> String {
        let NativeLiterals(literal_type) = self;
        format!("Unnecessary call to `{literal_type}`")
    }

    fn autofix_title(&self) -> String {
        let NativeLiterals(literal_type) = self;
        format!("Replace with `{literal_type}`")
    }

    fn placeholder() -> Self {
        NativeLiterals(LiteralType::Str)
    }
}

define_violation!(
    pub struct TypingTextStrAlias;
);
impl AlwaysAutofixableViolation for TypingTextStrAlias {
    fn message(&self) -> String {
        "`typing.Text` is deprecated, use `str`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `str`".to_string()
    }

    fn placeholder() -> Self {
        TypingTextStrAlias
    }
}

define_violation!(
    pub struct OpenAlias;
);
impl AlwaysAutofixableViolation for OpenAlias {
    fn message(&self) -> String {
        "Use builtin `open`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with builtin `open`".to_string()
    }

    fn placeholder() -> Self {
        OpenAlias
    }
}

define_violation!(
    pub struct ReplaceUniversalNewlines;
);
impl AlwaysAutofixableViolation for ReplaceUniversalNewlines {
    fn message(&self) -> String {
        "`universal_newlines` is deprecated, use `text`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `text` keyword argument".to_string()
    }

    fn placeholder() -> Self {
        ReplaceUniversalNewlines
    }
}

define_violation!(
    pub struct ReplaceStdoutStderr;
);
impl AlwaysAutofixableViolation for ReplaceStdoutStderr {
    fn message(&self) -> String {
        "Sending stdout and stderr to pipe is deprecated, use `capture_output`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `capture_output` keyword argument".to_string()
    }

    fn placeholder() -> Self {
        ReplaceStdoutStderr
    }
}

define_violation!(
    pub struct RewriteCElementTree;
);
impl AlwaysAutofixableViolation for RewriteCElementTree {
    fn message(&self) -> String {
        "`cElementTree` is deprecated, use `ElementTree`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `ElementTree`".to_string()
    }

    fn placeholder() -> Self {
        RewriteCElementTree
    }
}

define_violation!(
    pub struct OSErrorAlias(pub Option<String>);
);
impl AlwaysAutofixableViolation for OSErrorAlias {
    fn message(&self) -> String {
        "Replace aliased errors with `OSError`".to_string()
    }

    fn autofix_title(&self) -> String {
        let OSErrorAlias(name) = self;
        match name {
            None => "Replace with builtin `OSError`".to_string(),
            Some(name) => format!("Replace `{name}` with builtin `OSError`"),
        }
    }

    fn placeholder() -> Self {
        OSErrorAlias(None)
    }
}

define_violation!(
    pub struct RewriteUnicodeLiteral;
);
impl AlwaysAutofixableViolation for RewriteUnicodeLiteral {
    fn message(&self) -> String {
        "Remove unicode literals from strings".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove unicode prefix".to_string()
    }

    fn placeholder() -> Self {
        RewriteUnicodeLiteral
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MockReference {
    Import,
    Attribute,
}

define_violation!(
    pub struct RewriteMockImport(pub MockReference);
);
impl AlwaysAutofixableViolation for RewriteMockImport {
    fn message(&self) -> String {
        "`mock` is deprecated, use `unittest.mock`".to_string()
    }

    fn autofix_title(&self) -> String {
        let RewriteMockImport(reference_type) = self;
        match reference_type {
            MockReference::Import => "Import from `unittest.mock` instead".to_string(),
            MockReference::Attribute => "Replace `mock.mock` with `mock`".to_string(),
        }
    }

    fn placeholder() -> Self {
        RewriteMockImport(MockReference::Import)
    }
}

define_violation!(
    pub struct RewriteListComprehension;
);
impl AlwaysAutofixableViolation for RewriteListComprehension {
    fn message(&self) -> String {
        "Replace unpacked list comprehension with a generator expression".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with generator expression".to_string()
    }

    fn placeholder() -> Self {
        RewriteListComprehension
    }
}

define_violation!(
    pub struct RewriteYieldFrom;
);
impl AlwaysAutofixableViolation for RewriteYieldFrom {
    fn message(&self) -> String {
        "Replace `yield` over `for` loop with `yield from`".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `yield from`".to_string()
    }

    fn placeholder() -> Self {
        RewriteYieldFrom
    }
}

define_violation!(
    pub struct UnnecessaryBuiltinImport(pub Vec<String>);
);
impl AlwaysAutofixableViolation for UnnecessaryBuiltinImport {
    fn message(&self) -> String {
        let UnnecessaryBuiltinImport(names) = self;
        if names.len() == 1 {
            let import = &names[0];
            format!("Unnecessary builtin import: `{import}`")
        } else {
            let imports = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Unnecessary builtin imports: {imports}")
        }
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary builtin import".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryBuiltinImport(vec!["...".to_string()])
    }
}

define_violation!(
    pub struct FormatLiterals;
);
impl AlwaysAutofixableViolation for FormatLiterals {
    fn message(&self) -> String {
        "Use implicit references for positional format fields".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove explicit positional indexes".to_string()
    }

    fn placeholder() -> Self {
        FormatLiterals
    }
}

define_violation!(
    pub struct FString;
);
impl AlwaysAutofixableViolation for FString {
    fn message(&self) -> String {
        "Use f-string instead of `format` call".to_string()
    }

    fn autofix_title(&self) -> String {
        "Convert to f-string".to_string()
    }

    fn placeholder() -> Self {
        FString
    }
}

// pydocstyle

define_violation!(
    pub struct PublicModule;
);
impl Violation for PublicModule {
    fn message(&self) -> String {
        "Missing docstring in public module".to_string()
    }

    fn placeholder() -> Self {
        PublicModule
    }
}

define_violation!(
    pub struct PublicClass;
);
impl Violation for PublicClass {
    fn message(&self) -> String {
        "Missing docstring in public class".to_string()
    }

    fn placeholder() -> Self {
        PublicClass
    }
}

define_violation!(
    pub struct PublicMethod;
);
impl Violation for PublicMethod {
    fn message(&self) -> String {
        "Missing docstring in public method".to_string()
    }

    fn placeholder() -> Self {
        PublicMethod
    }
}

define_violation!(
    pub struct PublicFunction;
);
impl Violation for PublicFunction {
    fn message(&self) -> String {
        "Missing docstring in public function".to_string()
    }

    fn placeholder() -> Self {
        PublicFunction
    }
}

define_violation!(
    pub struct PublicPackage;
);
impl Violation for PublicPackage {
    fn message(&self) -> String {
        "Missing docstring in public package".to_string()
    }

    fn placeholder() -> Self {
        PublicPackage
    }
}

define_violation!(
    pub struct MagicMethod;
);
impl Violation for MagicMethod {
    fn message(&self) -> String {
        "Missing docstring in magic method".to_string()
    }

    fn placeholder() -> Self {
        MagicMethod
    }
}

define_violation!(
    pub struct PublicNestedClass;
);
impl Violation for PublicNestedClass {
    fn message(&self) -> String {
        "Missing docstring in public nested class".to_string()
    }

    fn placeholder() -> Self {
        PublicNestedClass
    }
}

define_violation!(
    pub struct PublicInit;
);
impl Violation for PublicInit {
    fn message(&self) -> String {
        "Missing docstring in `__init__`".to_string()
    }

    fn placeholder() -> Self {
        PublicInit
    }
}

define_violation!(
    pub struct FitsOnOneLine;
);
impl Violation for FitsOnOneLine {
    fn message(&self) -> String {
        "One-line docstring should fit on one line".to_string()
    }

    fn placeholder() -> Self {
        FitsOnOneLine
    }
}

define_violation!(
    pub struct NoBlankLineBeforeFunction(pub usize);
);
impl AlwaysAutofixableViolation for NoBlankLineBeforeFunction {
    fn message(&self) -> String {
        let NoBlankLineBeforeFunction(num_lines) = self;
        format!("No blank lines allowed before function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before function docstring".to_string()
    }

    fn placeholder() -> Self {
        NoBlankLineBeforeFunction(1)
    }
}

define_violation!(
    pub struct NoBlankLineAfterFunction(pub usize);
);
impl AlwaysAutofixableViolation for NoBlankLineAfterFunction {
    fn message(&self) -> String {
        let NoBlankLineAfterFunction(num_lines) = self;
        format!("No blank lines allowed after function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) after function docstring".to_string()
    }

    fn placeholder() -> Self {
        NoBlankLineAfterFunction(1)
    }
}

define_violation!(
    pub struct OneBlankLineBeforeClass(pub usize);
);
impl AlwaysAutofixableViolation for OneBlankLineBeforeClass {
    fn message(&self) -> String {
        "1 blank line required before class docstring".to_string()
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line before class docstring".to_string()
    }

    fn placeholder() -> Self {
        OneBlankLineBeforeClass(0)
    }
}

define_violation!(
    pub struct OneBlankLineAfterClass(pub usize);
);
impl AlwaysAutofixableViolation for OneBlankLineAfterClass {
    fn message(&self) -> String {
        "1 blank line required after class docstring".to_string()
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line after class docstring".to_string()
    }

    fn placeholder() -> Self {
        OneBlankLineAfterClass(0)
    }
}

define_violation!(
    pub struct BlankLineAfterSummary(pub usize);
);
fn fmt_blank_line_after_summary_autofix_msg(_: &BlankLineAfterSummary) -> String {
    "Insert single blank line".to_string()
}
impl Violation for BlankLineAfterSummary {
    fn message(&self) -> String {
        let BlankLineAfterSummary(num_lines) = self;
        if *num_lines == 0 {
            "1 blank line required between summary line and description".to_string()
        } else {
            format!(
                "1 blank line required between summary line and description (found {num_lines})"
            )
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let num_lines = self.0;
        if num_lines > 0 {
            return Some(fmt_blank_line_after_summary_autofix_msg);
        }
        None
    }

    fn placeholder() -> Self {
        BlankLineAfterSummary(2)
    }
}

define_violation!(
    pub struct IndentWithSpaces;
);
impl Violation for IndentWithSpaces {
    fn message(&self) -> String {
        "Docstring should be indented with spaces, not tabs".to_string()
    }

    fn placeholder() -> Self {
        IndentWithSpaces
    }
}

define_violation!(
    pub struct NoUnderIndentation;
);
impl AlwaysAutofixableViolation for NoUnderIndentation {
    fn message(&self) -> String {
        "Docstring is under-indented".to_string()
    }

    fn autofix_title(&self) -> String {
        "Increase indentation".to_string()
    }

    fn placeholder() -> Self {
        NoUnderIndentation
    }
}

define_violation!(
    pub struct NoOverIndentation;
);
impl AlwaysAutofixableViolation for NoOverIndentation {
    fn message(&self) -> String {
        "Docstring is over-indented".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove over-indentation".to_string()
    }

    fn placeholder() -> Self {
        NoOverIndentation
    }
}

define_violation!(
    pub struct NewLineAfterLastParagraph;
);
impl AlwaysAutofixableViolation for NewLineAfterLastParagraph {
    fn message(&self) -> String {
        "Multi-line docstring closing quotes should be on a separate line".to_string()
    }

    fn autofix_title(&self) -> String {
        "Move closing quotes to new line".to_string()
    }

    fn placeholder() -> Self {
        NewLineAfterLastParagraph
    }
}

define_violation!(
    pub struct NoSurroundingWhitespace;
);
impl AlwaysAutofixableViolation for NoSurroundingWhitespace {
    fn message(&self) -> String {
        "No whitespaces allowed surrounding docstring text".to_string()
    }

    fn autofix_title(&self) -> String {
        "Trim surrounding whitespace".to_string()
    }

    fn placeholder() -> Self {
        NoSurroundingWhitespace
    }
}

define_violation!(
    pub struct NoBlankLineBeforeClass(pub usize);
);
impl AlwaysAutofixableViolation for NoBlankLineBeforeClass {
    fn message(&self) -> String {
        "No blank lines allowed before class docstring".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before class docstring".to_string()
    }

    fn placeholder() -> Self {
        NoBlankLineBeforeClass(1)
    }
}

define_violation!(
    pub struct MultiLineSummaryFirstLine;
);
impl Violation for MultiLineSummaryFirstLine {
    fn message(&self) -> String {
        "Multi-line docstring summary should start at the first line".to_string()
    }

    fn placeholder() -> Self {
        MultiLineSummaryFirstLine
    }
}

define_violation!(
    pub struct MultiLineSummarySecondLine;
);
impl Violation for MultiLineSummarySecondLine {
    fn message(&self) -> String {
        "Multi-line docstring summary should start at the second line".to_string()
    }

    fn placeholder() -> Self {
        MultiLineSummarySecondLine
    }
}

define_violation!(
    pub struct SectionNotOverIndented(pub String);
);
impl AlwaysAutofixableViolation for SectionNotOverIndented {
    fn message(&self) -> String {
        let SectionNotOverIndented(name) = self;
        format!("Section is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNotOverIndented(name) = self;
        format!("Remove over-indentation from \"{name}\"")
    }

    fn placeholder() -> Self {
        SectionNotOverIndented("Returns".to_string())
    }
}

define_violation!(
    pub struct SectionUnderlineNotOverIndented(pub String);
);
impl AlwaysAutofixableViolation for SectionUnderlineNotOverIndented {
    fn message(&self) -> String {
        let SectionUnderlineNotOverIndented(name) = self;
        format!("Section underline is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineNotOverIndented(name) = self;
        format!("Remove over-indentation from \"{name}\" underline")
    }

    fn placeholder() -> Self {
        SectionUnderlineNotOverIndented("Returns".to_string())
    }
}

define_violation!(
    pub struct UsesTripleQuotes;
);
impl Violation for UsesTripleQuotes {
    fn message(&self) -> String {
        r#"Use """triple double quotes""""#.to_string()
    }

    fn placeholder() -> Self {
        UsesTripleQuotes
    }
}

define_violation!(
    pub struct UsesRPrefixForBackslashedContent;
);
impl Violation for UsesRPrefixForBackslashedContent {
    fn message(&self) -> String {
        r#"Use r""" if any backslashes in a docstring"#.to_string()
    }

    fn placeholder() -> Self {
        UsesRPrefixForBackslashedContent
    }
}

define_violation!(
    pub struct EndsInPeriod;
);
impl AlwaysAutofixableViolation for EndsInPeriod {
    fn message(&self) -> String {
        "First line should end with a period".to_string()
    }

    fn autofix_title(&self) -> String {
        "Add period".to_string()
    }

    fn placeholder() -> Self {
        EndsInPeriod
    }
}

define_violation!(
    pub struct NoSignature;
);
impl Violation for NoSignature {
    fn message(&self) -> String {
        "First line should not be the function's signature".to_string()
    }

    fn placeholder() -> Self {
        NoSignature
    }
}

define_violation!(
    pub struct FirstLineCapitalized;
);
impl Violation for FirstLineCapitalized {
    fn message(&self) -> String {
        "First word of the first line should be properly capitalized".to_string()
    }

    fn placeholder() -> Self {
        FirstLineCapitalized
    }
}

define_violation!(
    pub struct NoThisPrefix;
);
impl Violation for NoThisPrefix {
    fn message(&self) -> String {
        "First word of the docstring should not be \"This\"".to_string()
    }

    fn placeholder() -> Self {
        NoThisPrefix
    }
}

define_violation!(
    pub struct CapitalizeSectionName(pub String);
);
impl AlwaysAutofixableViolation for CapitalizeSectionName {
    fn message(&self) -> String {
        let CapitalizeSectionName(name) = self;
        format!("Section name should be properly capitalized (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let CapitalizeSectionName(name) = self;
        format!("Capitalize \"{name}\"")
    }

    fn placeholder() -> Self {
        CapitalizeSectionName("returns".to_string())
    }
}

define_violation!(
    pub struct NewLineAfterSectionName(pub String);
);
impl AlwaysAutofixableViolation for NewLineAfterSectionName {
    fn message(&self) -> String {
        let NewLineAfterSectionName(name) = self;
        format!("Section name should end with a newline (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let NewLineAfterSectionName(name) = self;
        format!("Add newline after \"{name}\"")
    }

    fn placeholder() -> Self {
        NewLineAfterSectionName("Returns".to_string())
    }
}

define_violation!(
    pub struct DashedUnderlineAfterSection(pub String);
);
impl AlwaysAutofixableViolation for DashedUnderlineAfterSection {
    fn message(&self) -> String {
        let DashedUnderlineAfterSection(name) = self;
        format!("Missing dashed underline after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let DashedUnderlineAfterSection(name) = self;
        format!("Add dashed line under \"{name}\"")
    }

    fn placeholder() -> Self {
        DashedUnderlineAfterSection("Returns".to_string())
    }
}

define_violation!(
    pub struct SectionUnderlineAfterName(pub String);
);
impl AlwaysAutofixableViolation for SectionUnderlineAfterName {
    fn message(&self) -> String {
        let SectionUnderlineAfterName(name) = self;
        format!("Section underline should be in the line following the section's name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineAfterName(name) = self;
        format!("Add underline to \"{name}\"")
    }

    fn placeholder() -> Self {
        SectionUnderlineAfterName("Returns".to_string())
    }
}

define_violation!(
    pub struct SectionUnderlineMatchesSectionLength(pub String);
);
impl AlwaysAutofixableViolation for SectionUnderlineMatchesSectionLength {
    fn message(&self) -> String {
        let SectionUnderlineMatchesSectionLength(name) = self;
        format!("Section underline should match the length of its name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineMatchesSectionLength(name) = self;
        format!("Adjust underline length to match \"{name}\"")
    }

    fn placeholder() -> Self {
        SectionUnderlineMatchesSectionLength("Returns".to_string())
    }
}

define_violation!(
    pub struct BlankLineAfterSection(pub String);
);
impl AlwaysAutofixableViolation for BlankLineAfterSection {
    fn message(&self) -> String {
        let BlankLineAfterSection(name) = self;
        format!("Missing blank line after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterSection(name) = self;
        format!("Add blank line after \"{name}\"")
    }

    fn placeholder() -> Self {
        BlankLineAfterSection("Returns".to_string())
    }
}

define_violation!(
    pub struct BlankLineBeforeSection(pub String);
);
impl AlwaysAutofixableViolation for BlankLineBeforeSection {
    fn message(&self) -> String {
        let BlankLineBeforeSection(name) = self;
        format!("Missing blank line before section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineBeforeSection(name) = self;
        format!("Add blank line before \"{name}\"")
    }

    fn placeholder() -> Self {
        BlankLineBeforeSection("Returns".to_string())
    }
}

define_violation!(
    pub struct NoBlankLinesBetweenHeaderAndContent(pub String);
);
impl AlwaysAutofixableViolation for NoBlankLinesBetweenHeaderAndContent {
    fn message(&self) -> String {
        let NoBlankLinesBetweenHeaderAndContent(name) = self;
        format!("No blank lines allowed between a section header and its content (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s)".to_string()
    }

    fn placeholder() -> Self {
        NoBlankLinesBetweenHeaderAndContent("Returns".to_string())
    }
}

define_violation!(
    pub struct BlankLineAfterLastSection(pub String);
);
impl AlwaysAutofixableViolation for BlankLineAfterLastSection {
    fn message(&self) -> String {
        let BlankLineAfterLastSection(name) = self;
        format!("Missing blank line after last section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterLastSection(name) = self;
        format!("Add blank line after \"{name}\"")
    }

    fn placeholder() -> Self {
        BlankLineAfterLastSection("Returns".to_string())
    }
}

define_violation!(
    pub struct NonEmptySection(pub String);
);
impl Violation for NonEmptySection {
    fn message(&self) -> String {
        let NonEmptySection(name) = self;
        format!("Section has no content (\"{name}\")")
    }

    fn placeholder() -> Self {
        NonEmptySection("Returns".to_string())
    }
}

define_violation!(
    pub struct EndsInPunctuation;
);
impl AlwaysAutofixableViolation for EndsInPunctuation {
    fn message(&self) -> String {
        "First line should end with a period, question mark, or exclamation point".to_string()
    }

    fn autofix_title(&self) -> String {
        "Add closing punctuation".to_string()
    }

    fn placeholder() -> Self {
        EndsInPunctuation
    }
}

define_violation!(
    pub struct SectionNameEndsInColon(pub String);
);
impl AlwaysAutofixableViolation for SectionNameEndsInColon {
    fn message(&self) -> String {
        let SectionNameEndsInColon(name) = self;
        format!("Section name should end with a colon (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNameEndsInColon(name) = self;
        format!("Add colon to \"{name}\"")
    }

    fn placeholder() -> Self {
        SectionNameEndsInColon("Returns".to_string())
    }
}

define_violation!(
    pub struct DocumentAllArguments(pub Vec<String>);
);
impl Violation for DocumentAllArguments {
    fn message(&self) -> String {
        let DocumentAllArguments(names) = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Missing argument description in the docstring: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Missing argument descriptions in the docstring: {names}")
        }
    }

    fn placeholder() -> Self {
        DocumentAllArguments(vec!["x".to_string(), "y".to_string()])
    }
}

define_violation!(
    pub struct SkipDocstring;
);
impl Violation for SkipDocstring {
    fn message(&self) -> String {
        "Function decorated with `@overload` shouldn't contain a docstring".to_string()
    }

    fn placeholder() -> Self {
        SkipDocstring
    }
}

define_violation!(
    pub struct NonEmpty;
);
impl Violation for NonEmpty {
    fn message(&self) -> String {
        "Docstring is empty".to_string()
    }

    fn placeholder() -> Self {
        NonEmpty
    }
}

// pep8-naming

define_violation!(
    pub struct InvalidClassName(pub String);
);
impl Violation for InvalidClassName {
    fn message(&self) -> String {
        let InvalidClassName(name) = self;
        format!("Class name `{name}` should use CapWords convention ")
    }

    fn placeholder() -> Self {
        InvalidClassName("...".to_string())
    }
}

define_violation!(
    pub struct InvalidFunctionName(pub String);
);
impl Violation for InvalidFunctionName {
    fn message(&self) -> String {
        let InvalidFunctionName(name) = self;
        format!("Function name `{name}` should be lowercase")
    }

    fn placeholder() -> Self {
        InvalidFunctionName("...".to_string())
    }
}

define_violation!(
    pub struct InvalidArgumentName(pub String);
);
impl Violation for InvalidArgumentName {
    fn message(&self) -> String {
        let InvalidArgumentName(name) = self;
        format!("Argument name `{name}` should be lowercase")
    }

    fn placeholder() -> Self {
        InvalidArgumentName("...".to_string())
    }
}

define_violation!(
    pub struct InvalidFirstArgumentNameForClassMethod;
);
impl Violation for InvalidFirstArgumentNameForClassMethod {
    fn message(&self) -> String {
        "First argument of a class method should be named `cls`".to_string()
    }

    fn placeholder() -> Self {
        InvalidFirstArgumentNameForClassMethod
    }
}

define_violation!(
    pub struct InvalidFirstArgumentNameForMethod;
);
impl Violation for InvalidFirstArgumentNameForMethod {
    fn message(&self) -> String {
        "First argument of a method should be named `self`".to_string()
    }

    fn placeholder() -> Self {
        InvalidFirstArgumentNameForMethod
    }
}

define_violation!(
    pub struct NonLowercaseVariableInFunction(pub String);
);
impl Violation for NonLowercaseVariableInFunction {
    fn message(&self) -> String {
        let NonLowercaseVariableInFunction(name) = self;
        format!("Variable `{name}` in function should be lowercase")
    }

    fn placeholder() -> Self {
        NonLowercaseVariableInFunction("...".to_string())
    }
}

define_violation!(
    pub struct DunderFunctionName;
);
impl Violation for DunderFunctionName {
    fn message(&self) -> String {
        "Function name should not start and end with `__`".to_string()
    }

    fn placeholder() -> Self {
        DunderFunctionName
    }
}

define_violation!(
    pub struct ConstantImportedAsNonConstant(pub String, pub String);
);
impl Violation for ConstantImportedAsNonConstant {
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant(name, asname) = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }

    fn placeholder() -> Self {
        ConstantImportedAsNonConstant("...".to_string(), "...".to_string())
    }
}

define_violation!(
    pub struct LowercaseImportedAsNonLowercase(pub String, pub String);
);
impl Violation for LowercaseImportedAsNonLowercase {
    fn message(&self) -> String {
        let LowercaseImportedAsNonLowercase(name, asname) = self;
        format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
    }

    fn placeholder() -> Self {
        LowercaseImportedAsNonLowercase("...".to_string(), "...".to_string())
    }
}

define_violation!(
    pub struct CamelcaseImportedAsLowercase(pub String, pub String);
);
impl Violation for CamelcaseImportedAsLowercase {
    fn message(&self) -> String {
        let CamelcaseImportedAsLowercase(name, asname) = self;
        format!("Camelcase `{name}` imported as lowercase `{asname}`")
    }

    fn placeholder() -> Self {
        CamelcaseImportedAsLowercase("...".to_string(), "...".to_string())
    }
}

define_violation!(
    pub struct CamelcaseImportedAsConstant(pub String, pub String);
);
impl Violation for CamelcaseImportedAsConstant {
    fn message(&self) -> String {
        let CamelcaseImportedAsConstant(name, asname) = self;
        format!("Camelcase `{name}` imported as constant `{asname}`")
    }

    fn placeholder() -> Self {
        CamelcaseImportedAsConstant("...".to_string(), "...".to_string())
    }
}

define_violation!(
    pub struct MixedCaseVariableInClassScope(pub String);
);
impl Violation for MixedCaseVariableInClassScope {
    fn message(&self) -> String {
        let MixedCaseVariableInClassScope(name) = self;
        format!("Variable `{name}` in class scope should not be mixedCase")
    }

    fn placeholder() -> Self {
        MixedCaseVariableInClassScope("mixedCase".to_string())
    }
}

define_violation!(
    pub struct MixedCaseVariableInGlobalScope(pub String);
);
impl Violation for MixedCaseVariableInGlobalScope {
    fn message(&self) -> String {
        let MixedCaseVariableInGlobalScope(name) = self;
        format!("Variable `{name}` in global scope should not be mixedCase")
    }

    fn placeholder() -> Self {
        MixedCaseVariableInGlobalScope("mixedCase".to_string())
    }
}

define_violation!(
    pub struct CamelcaseImportedAsAcronym(pub String, pub String);
);
impl Violation for CamelcaseImportedAsAcronym {
    fn message(&self) -> String {
        let CamelcaseImportedAsAcronym(name, asname) = self;
        format!("Camelcase `{name}` imported as acronym `{asname}`")
    }

    fn placeholder() -> Self {
        CamelcaseImportedAsAcronym("...".to_string(), "...".to_string())
    }
}

define_violation!(
    pub struct ErrorSuffixOnExceptionName(pub String);
);
impl Violation for ErrorSuffixOnExceptionName {
    fn message(&self) -> String {
        let ErrorSuffixOnExceptionName(name) = self;
        format!("Exception name `{name}` should be named with an Error suffix")
    }

    fn placeholder() -> Self {
        ErrorSuffixOnExceptionName("...".to_string())
    }
}

// isort

define_violation!(
    pub struct UnsortedImports;
);
impl AlwaysAutofixableViolation for UnsortedImports {
    fn message(&self) -> String {
        "Import block is un-sorted or un-formatted".to_string()
    }

    fn autofix_title(&self) -> String {
        "Organize imports".to_string()
    }

    fn placeholder() -> Self {
        UnsortedImports
    }
}

define_violation!(
    pub struct MissingRequiredImport(pub String);
);
impl AlwaysAutofixableViolation for MissingRequiredImport {
    fn message(&self) -> String {
        let MissingRequiredImport(name) = self;
        format!("Missing required import: `{name}`")
    }

    fn autofix_title(&self) -> String {
        let MissingRequiredImport(name) = self;
        format!("Insert required import: `{name}`")
    }

    fn placeholder() -> Self {
        MissingRequiredImport("from __future__ import ...".to_string())
    }
}

// eradicate

define_violation!(
    pub struct CommentedOutCode;
);
impl AlwaysAutofixableViolation for CommentedOutCode {
    fn message(&self) -> String {
        "Found commented-out code".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove commented-out code".to_string()
    }

    fn placeholder() -> Self {
        CommentedOutCode
    }
}

// flake8-bandit

define_violation!(
    pub struct Jinja2AutoescapeFalse(pub bool);
);
impl Violation for Jinja2AutoescapeFalse {
    fn message(&self) -> String {
        let Jinja2AutoescapeFalse(value) = self;
        match value {
            true => "Using jinja2 templates with `autoescape=False` is dangerous and can lead to \
                     XSS. Ensure `autoescape=True` or use the `select_autoescape` function."
                .to_string(),
            false => "By default, jinja2 sets `autoescape` to `False`. Consider using \
                      `autoescape=True` or the `select_autoescape` function to mitigate XSS \
                      vulnerabilities."
                .to_string(),
        }
    }

    fn placeholder() -> Self {
        Jinja2AutoescapeFalse(false)
    }
}

define_violation!(
    pub struct AssertUsed;
);
impl Violation for AssertUsed {
    fn message(&self) -> String {
        "Use of `assert` detected".to_string()
    }

    fn placeholder() -> Self {
        AssertUsed
    }
}

define_violation!(
    pub struct ExecUsed;
);
impl Violation for ExecUsed {
    fn message(&self) -> String {
        "Use of `exec` detected".to_string()
    }

    fn placeholder() -> Self {
        ExecUsed
    }
}

define_violation!(
    pub struct BadFilePermissions(pub u16);
);
impl Violation for BadFilePermissions {
    fn message(&self) -> String {
        let BadFilePermissions(mask) = self;
        format!("`os.chmod` setting a permissive mask `{mask:#o}` on file or directory",)
    }

    fn placeholder() -> Self {
        BadFilePermissions(0o777)
    }
}

define_violation!(
    pub struct HardcodedBindAllInterfaces;
);
impl Violation for HardcodedBindAllInterfaces {
    fn message(&self) -> String {
        "Possible binding to all interfaces".to_string()
    }

    fn placeholder() -> Self {
        HardcodedBindAllInterfaces
    }
}

define_violation!(
    pub struct HardcodedPasswordString(pub String);
);
impl Violation for HardcodedPasswordString {
    fn message(&self) -> String {
        let HardcodedPasswordString(string) = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }

    fn placeholder() -> Self {
        HardcodedPasswordString("...".to_string())
    }
}

define_violation!(
    pub struct HardcodedPasswordFuncArg(pub String);
);
impl Violation for HardcodedPasswordFuncArg {
    fn message(&self) -> String {
        let HardcodedPasswordFuncArg(string) = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }

    fn placeholder() -> Self {
        HardcodedPasswordFuncArg("...".to_string())
    }
}

define_violation!(
    pub struct HardcodedPasswordDefault(pub String);
);
impl Violation for HardcodedPasswordDefault {
    fn message(&self) -> String {
        let HardcodedPasswordDefault(string) = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }

    fn placeholder() -> Self {
        HardcodedPasswordDefault("...".to_string())
    }
}

define_violation!(
    pub struct HardcodedTempFile(pub String);
);
impl Violation for HardcodedTempFile {
    fn message(&self) -> String {
        let HardcodedTempFile(string) = self;
        format!(
            "Probable insecure usage of temporary file or directory: \"{}\"",
            string.escape_debug()
        )
    }

    fn placeholder() -> Self {
        HardcodedTempFile("...".to_string())
    }
}

define_violation!(
    pub struct RequestWithoutTimeout(pub Option<String>);
);
impl Violation for RequestWithoutTimeout {
    fn message(&self) -> String {
        let RequestWithoutTimeout(timeout) = self;
        match timeout {
            Some(value) => {
                format!("Probable use of requests call with timeout set to `{value}`")
            }
            None => "Probable use of requests call without timeout".to_string(),
        }
    }

    fn placeholder() -> Self {
        RequestWithoutTimeout(None)
    }
}

define_violation!(
    pub struct HashlibInsecureHashFunction(pub String);
);
impl Violation for HashlibInsecureHashFunction {
    fn message(&self) -> String {
        let HashlibInsecureHashFunction(string) = self;
        format!(
            "Probable use of insecure hash functions in `hashlib`: \"{}\"",
            string.escape_debug()
        )
    }

    fn placeholder() -> Self {
        HashlibInsecureHashFunction("...".to_string())
    }
}

define_violation!(
    pub struct RequestWithNoCertValidation(pub String);
);
impl Violation for RequestWithNoCertValidation {
    fn message(&self) -> String {
        let RequestWithNoCertValidation(string) = self;
        format!(
            "Probable use of `{string}` call with `verify=False` disabling SSL certificate checks"
        )
    }

    fn placeholder() -> Self {
        RequestWithNoCertValidation("...".to_string())
    }
}

define_violation!(
    pub struct UnsafeYAMLLoad(pub Option<String>);
);
impl Violation for UnsafeYAMLLoad {
    fn message(&self) -> String {
        let UnsafeYAMLLoad(loader) = self;
        match loader {
            Some(name) => {
                format!(
                    "Probable use of unsafe loader `{name}` with `yaml.load`. Allows \
                     instantiation of arbitrary objects. Consider `yaml.safe_load`."
                )
            }
            None => "Probable use of unsafe `yaml.load`. Allows instantiation of arbitrary \
                     objects. Consider `yaml.safe_load`."
                .to_string(),
        }
    }

    fn placeholder() -> Self {
        UnsafeYAMLLoad(None)
    }
}

define_violation!(
    pub struct SnmpInsecureVersion;
);
impl Violation for SnmpInsecureVersion {
    fn message(&self) -> String {
        "The use of SNMPv1 and SNMPv2 is insecure. Use SNMPv3 if able.".to_string()
    }

    fn placeholder() -> Self {
        SnmpInsecureVersion
    }
}

define_violation!(
    pub struct SnmpWeakCryptography;
);
impl Violation for SnmpWeakCryptography {
    fn message(&self) -> String {
        "You should not use SNMPv3 without encryption. `noAuthNoPriv` & `authNoPriv` is insecure."
            .to_string()
    }

    fn placeholder() -> Self {
        SnmpWeakCryptography
    }
}

// flake8-boolean-trap

define_violation!(
    pub struct BooleanPositionalArgInFunctionDefinition;
);
impl Violation for BooleanPositionalArgInFunctionDefinition {
    fn message(&self) -> String {
        "Boolean positional arg in function definition".to_string()
    }

    fn placeholder() -> Self {
        BooleanPositionalArgInFunctionDefinition
    }
}

define_violation!(
    pub struct BooleanDefaultValueInFunctionDefinition;
);
impl Violation for BooleanDefaultValueInFunctionDefinition {
    fn message(&self) -> String {
        "Boolean default value in function definition".to_string()
    }

    fn placeholder() -> Self {
        BooleanDefaultValueInFunctionDefinition
    }
}

define_violation!(
    pub struct BooleanPositionalValueInFunctionCall;
);
impl Violation for BooleanPositionalValueInFunctionCall {
    fn message(&self) -> String {
        "Boolean positional value in function call".to_string()
    }

    fn placeholder() -> Self {
        BooleanPositionalValueInFunctionCall
    }
}

// flake8-unused-arguments

define_violation!(
    pub struct UnusedFunctionArgument(pub String);
);
impl Violation for UnusedFunctionArgument {
    fn message(&self) -> String {
        let UnusedFunctionArgument(name) = self;
        format!("Unused function argument: `{name}`")
    }

    fn placeholder() -> Self {
        UnusedFunctionArgument("...".to_string())
    }
}

define_violation!(
    pub struct UnusedMethodArgument(pub String);
);
impl Violation for UnusedMethodArgument {
    fn message(&self) -> String {
        let UnusedMethodArgument(name) = self;
        format!("Unused method argument: `{name}`")
    }

    fn placeholder() -> Self {
        UnusedMethodArgument("...".to_string())
    }
}

define_violation!(
    pub struct UnusedClassMethodArgument(pub String);
);
impl Violation for UnusedClassMethodArgument {
    fn message(&self) -> String {
        let UnusedClassMethodArgument(name) = self;
        format!("Unused class method argument: `{name}`")
    }

    fn placeholder() -> Self {
        UnusedClassMethodArgument("...".to_string())
    }
}

define_violation!(
    pub struct UnusedStaticMethodArgument(pub String);
);
impl Violation for UnusedStaticMethodArgument {
    fn message(&self) -> String {
        let UnusedStaticMethodArgument(name) = self;
        format!("Unused static method argument: `{name}`")
    }

    fn placeholder() -> Self {
        UnusedStaticMethodArgument("...".to_string())
    }
}

define_violation!(
    pub struct UnusedLambdaArgument(pub String);
);
impl Violation for UnusedLambdaArgument {
    fn message(&self) -> String {
        let UnusedLambdaArgument(name) = self;
        format!("Unused lambda argument: `{name}`")
    }

    fn placeholder() -> Self {
        UnusedLambdaArgument("...".to_string())
    }
}

// flake8-import-conventions

define_violation!(
    pub struct ImportAliasIsNotConventional(pub String, pub String);
);
impl Violation for ImportAliasIsNotConventional {
    fn message(&self) -> String {
        let ImportAliasIsNotConventional(name, asname) = self;
        format!("`{name}` should be imported as `{asname}`")
    }

    fn placeholder() -> Self {
        ImportAliasIsNotConventional("...".to_string(), "...".to_string())
    }
}

// flake8-datetimez

define_violation!(
    pub struct CallDatetimeWithoutTzinfo;
);
impl Violation for CallDatetimeWithoutTzinfo {
    fn message(&self) -> String {
        "The use of `datetime.datetime()` without `tzinfo` argument is not allowed".to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeWithoutTzinfo
    }
}

define_violation!(
    pub struct CallDatetimeToday;
);
impl Violation for CallDatetimeToday {
    fn message(&self) -> String {
        "The use of `datetime.datetime.today()` is not allowed. Use `datetime.datetime.now(tz=)` \
         instead."
            .to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeToday
    }
}

define_violation!(
    pub struct CallDatetimeUtcnow;
);
impl Violation for CallDatetimeUtcnow {
    fn message(&self) -> String {
        "The use of `datetime.datetime.utcnow()` is not allowed. Use `datetime.datetime.now(tz=)` \
         instead."
            .to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeUtcnow
    }
}

define_violation!(
    pub struct CallDatetimeUtcfromtimestamp;
);
impl Violation for CallDatetimeUtcfromtimestamp {
    fn message(&self) -> String {
        "The use of `datetime.datetime.utcfromtimestamp()` is not allowed. Use \
         `datetime.datetime.fromtimestamp(, tz=)` instead."
            .to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeUtcfromtimestamp
    }
}

define_violation!(
    pub struct CallDatetimeNowWithoutTzinfo;
);
impl Violation for CallDatetimeNowWithoutTzinfo {
    fn message(&self) -> String {
        "The use of `datetime.datetime.now()` without `tz` argument is not allowed".to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeNowWithoutTzinfo
    }
}

define_violation!(
    pub struct CallDatetimeFromtimestamp;
);
impl Violation for CallDatetimeFromtimestamp {
    fn message(&self) -> String {
        "The use of `datetime.datetime.fromtimestamp()` without `tz` argument is not allowed"
            .to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeFromtimestamp
    }
}

define_violation!(
    pub struct CallDatetimeStrptimeWithoutZone;
);
impl Violation for CallDatetimeStrptimeWithoutZone {
    fn message(&self) -> String {
        "The use of `datetime.datetime.strptime()` without %z must be followed by \
         `.replace(tzinfo=)`"
            .to_string()
    }

    fn placeholder() -> Self {
        CallDatetimeStrptimeWithoutZone
    }
}

define_violation!(
    pub struct CallDateToday;
);
impl Violation for CallDateToday {
    fn message(&self) -> String {
        "The use of `datetime.date.today()` is not allowed. Use \
         `datetime.datetime.now(tz=).date()` instead."
            .to_string()
    }

    fn placeholder() -> Self {
        CallDateToday
    }
}

define_violation!(
    pub struct CallDateFromtimestamp;
);
impl Violation for CallDateFromtimestamp {
    fn message(&self) -> String {
        "The use of `datetime.date.fromtimestamp()` is not allowed. Use \
         `datetime.datetime.fromtimestamp(, tz=).date()` instead."
            .to_string()
    }

    fn placeholder() -> Self {
        CallDateFromtimestamp
    }
}

// pygrep-hooks

define_violation!(
    pub struct NoEval;
);
impl Violation for NoEval {
    fn message(&self) -> String {
        "No builtin `eval()` allowed".to_string()
    }

    fn placeholder() -> Self {
        NoEval
    }
}

define_violation!(
    pub struct DeprecatedLogWarn;
);
impl Violation for DeprecatedLogWarn {
    fn message(&self) -> String {
        "`warn` is deprecated in favor of `warning`".to_string()
    }

    fn placeholder() -> Self {
        DeprecatedLogWarn
    }
}

define_violation!(
    pub struct BlanketTypeIgnore;
);
impl Violation for BlanketTypeIgnore {
    fn message(&self) -> String {
        "Use specific rule codes when ignoring type issues".to_string()
    }

    fn placeholder() -> Self {
        BlanketTypeIgnore
    }
}

define_violation!(
    pub struct BlanketNOQA;
);
impl Violation for BlanketNOQA {
    fn message(&self) -> String {
        "Use specific rule codes when using `noqa`".to_string()
    }

    fn placeholder() -> Self {
        BlanketNOQA
    }
}

// pandas-vet

define_violation!(
    pub struct UseOfInplaceArgument;
);
impl Violation for UseOfInplaceArgument {
    fn message(&self) -> String {
        "`inplace=True` should be avoided; it has inconsistent behavior".to_string()
    }

    fn placeholder() -> Self {
        UseOfInplaceArgument
    }
}

define_violation!(
    pub struct UseOfDotIsNull;
);
impl Violation for UseOfDotIsNull {
    fn message(&self) -> String {
        "`.isna` is preferred to `.isnull`; functionality is equivalent".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotIsNull
    }
}

define_violation!(
    pub struct UseOfDotNotNull;
);
impl Violation for UseOfDotNotNull {
    fn message(&self) -> String {
        "`.notna` is preferred to `.notnull`; functionality is equivalent".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotNotNull
    }
}

define_violation!(
    pub struct UseOfDotIx;
);
impl Violation for UseOfDotIx {
    fn message(&self) -> String {
        "`.ix` is deprecated; use more explicit `.loc` or `.iloc`".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotIx
    }
}

define_violation!(
    pub struct UseOfDotAt;
);
impl Violation for UseOfDotAt {
    fn message(&self) -> String {
        "Use `.loc` instead of `.at`.  If speed is important, use numpy.".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotAt
    }
}

define_violation!(
    pub struct UseOfDotIat;
);
impl Violation for UseOfDotIat {
    fn message(&self) -> String {
        "Use `.iloc` instead of `.iat`.  If speed is important, use numpy.".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotIat
    }
}

define_violation!(
    pub struct UseOfDotPivotOrUnstack;
);
impl Violation for UseOfDotPivotOrUnstack {
    fn message(&self) -> String {
        "`.pivot_table` is preferred to `.pivot` or `.unstack`; provides same functionality"
            .to_string()
    }

    fn placeholder() -> Self {
        UseOfDotPivotOrUnstack
    }
}

define_violation!(
    pub struct UseOfDotValues;
);
impl Violation for UseOfDotValues {
    fn message(&self) -> String {
        "Use `.to_numpy()` instead of `.values`".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotValues
    }
}

define_violation!(
    pub struct UseOfDotReadTable;
);
impl Violation for UseOfDotReadTable {
    fn message(&self) -> String {
        "`.read_csv` is preferred to `.read_table`; provides same functionality".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotReadTable
    }
}

define_violation!(
    pub struct UseOfDotStack;
);
impl Violation for UseOfDotStack {
    fn message(&self) -> String {
        "`.melt` is preferred to `.stack`; provides same functionality".to_string()
    }

    fn placeholder() -> Self {
        UseOfDotStack
    }
}

define_violation!(
    pub struct UseOfPdMerge;
);
impl Violation for UseOfPdMerge {
    fn message(&self) -> String {
        "Use `.merge` method instead of `pd.merge` function. They have equivalent functionality."
            .to_string()
    }

    fn placeholder() -> Self {
        UseOfPdMerge
    }
}

define_violation!(
    pub struct DfIsABadVariableName;
);
impl Violation for DfIsABadVariableName {
    fn message(&self) -> String {
        "`df` is a bad variable name. Be kinder to your future self.".to_string()
    }

    fn placeholder() -> Self {
        DfIsABadVariableName
    }
}

// flake8-errmsg

define_violation!(
    pub struct RawStringInException;
);
impl Violation for RawStringInException {
    fn message(&self) -> String {
        "Exception must not use a string literal, assign to variable first".to_string()
    }

    fn placeholder() -> Self {
        RawStringInException
    }
}

define_violation!(
    pub struct FStringInException;
);
impl Violation for FStringInException {
    fn message(&self) -> String {
        "Exception must not use an f-string literal, assign to variable first".to_string()
    }

    fn placeholder() -> Self {
        FStringInException
    }
}

define_violation!(
    pub struct DotFormatInException;
);
impl Violation for DotFormatInException {
    fn message(&self) -> String {
        "Exception must not use a `.format()` string directly, assign to variable first".to_string()
    }

    fn placeholder() -> Self {
        DotFormatInException
    }
}

// flake8-pytest-style

define_violation!(
    pub struct IncorrectFixtureParenthesesStyle(pub String, pub String);
);
impl AlwaysAutofixableViolation for IncorrectFixtureParenthesesStyle {
    fn message(&self) -> String {
        let IncorrectFixtureParenthesesStyle(expected_parens, actual_parens) = self;
        format!("Use `@pytest.fixture{expected_parens}` over `@pytest.fixture{actual_parens}`")
    }

    fn autofix_title(&self) -> String {
        "Add/remove parentheses".to_string()
    }

    fn placeholder() -> Self {
        IncorrectFixtureParenthesesStyle("()".to_string(), String::new())
    }
}

define_violation!(
    pub struct FixturePositionalArgs(pub String);
);
impl Violation for FixturePositionalArgs {
    fn message(&self) -> String {
        let FixturePositionalArgs(function) = self;
        format!("Configuration for fixture `{function}` specified via positional args, use kwargs")
    }

    fn placeholder() -> Self {
        FixturePositionalArgs("...".to_string())
    }
}

define_violation!(
    pub struct ExtraneousScopeFunction;
);
impl Violation for ExtraneousScopeFunction {
    fn message(&self) -> String {
        "`scope='function'` is implied in `@pytest.fixture()`".to_string()
    }

    fn placeholder() -> Self {
        ExtraneousScopeFunction
    }
}

define_violation!(
    pub struct MissingFixtureNameUnderscore(pub String);
);
impl Violation for MissingFixtureNameUnderscore {
    fn message(&self) -> String {
        let MissingFixtureNameUnderscore(function) = self;
        format!("Fixture `{function}` does not return anything, add leading underscore")
    }

    fn placeholder() -> Self {
        MissingFixtureNameUnderscore("...".to_string())
    }
}

define_violation!(
    pub struct IncorrectFixtureNameUnderscore(pub String);
);
impl Violation for IncorrectFixtureNameUnderscore {
    fn message(&self) -> String {
        let IncorrectFixtureNameUnderscore(function) = self;
        format!("Fixture `{function}` returns a value, remove leading underscore")
    }

    fn placeholder() -> Self {
        IncorrectFixtureNameUnderscore("...".to_string())
    }
}

define_violation!(
    pub struct ParametrizeNamesWrongType(pub ParametrizeNameType);
);
impl AlwaysAutofixableViolation for ParametrizeNamesWrongType {
    fn message(&self) -> String {
        let ParametrizeNamesWrongType(expected) = self;
        format!("Wrong name(s) type in `@pytest.mark.parametrize`, expected `{expected}`")
    }

    fn autofix_title(&self) -> String {
        let ParametrizeNamesWrongType(expected) = self;
        format!("Use a `{expected}` for parameter names")
    }

    fn placeholder() -> Self {
        ParametrizeNamesWrongType(ParametrizeNameType::Tuple)
    }
}

define_violation!(
    pub struct ParametrizeValuesWrongType(pub ParametrizeValuesType, pub ParametrizeValuesRowType);
);
impl Violation for ParametrizeValuesWrongType {
    fn message(&self) -> String {
        let ParametrizeValuesWrongType(values, row) = self;
        format!("Wrong values type in `@pytest.mark.parametrize` expected `{values}` of `{row}`")
    }

    fn placeholder() -> Self {
        ParametrizeValuesWrongType(ParametrizeValuesType::List, ParametrizeValuesRowType::Tuple)
    }
}

define_violation!(
    pub struct PatchWithLambda;
);
impl Violation for PatchWithLambda {
    fn message(&self) -> String {
        "Use `return_value=` instead of patching with `lambda`".to_string()
    }

    fn placeholder() -> Self {
        PatchWithLambda
    }
}

define_violation!(
    pub struct UnittestAssertion(pub String);
);
impl AlwaysAutofixableViolation for UnittestAssertion {
    fn message(&self) -> String {
        let UnittestAssertion(assertion) = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn autofix_title(&self) -> String {
        let UnittestAssertion(assertion) = self;
        format!("Replace `{assertion}(...)` with `assert ...`")
    }

    fn placeholder() -> Self {
        UnittestAssertion("...".to_string())
    }
}

define_violation!(
    pub struct RaisesWithoutException;
);
impl Violation for RaisesWithoutException {
    fn message(&self) -> String {
        "set the expected exception in `pytest.raises()`".to_string()
    }

    fn placeholder() -> Self {
        RaisesWithoutException
    }
}

define_violation!(
    pub struct RaisesTooBroad(pub String);
);
impl Violation for RaisesTooBroad {
    fn message(&self) -> String {
        let RaisesTooBroad(exception) = self;
        format!(
            "`pytest.raises({exception})` is too broad, set the `match` parameter or use a more \
             specific exception"
        )
    }

    fn placeholder() -> Self {
        RaisesTooBroad("...".to_string())
    }
}

define_violation!(
    pub struct RaisesWithMultipleStatements;
);
impl Violation for RaisesWithMultipleStatements {
    fn message(&self) -> String {
        "`pytest.raises()` block should contain a single simple statement".to_string()
    }

    fn placeholder() -> Self {
        RaisesWithMultipleStatements
    }
}

define_violation!(
    pub struct IncorrectPytestImport;
);
impl Violation for IncorrectPytestImport {
    fn message(&self) -> String {
        "Found incorrect import of pytest, use simple `import pytest` instead".to_string()
    }

    fn placeholder() -> Self {
        IncorrectPytestImport
    }
}

define_violation!(
    pub struct AssertAlwaysFalse;
);
impl Violation for AssertAlwaysFalse {
    fn message(&self) -> String {
        "Assertion always fails, replace with `pytest.fail()`".to_string()
    }

    fn placeholder() -> Self {
        AssertAlwaysFalse
    }
}

define_violation!(
    pub struct FailWithoutMessage;
);
impl Violation for FailWithoutMessage {
    fn message(&self) -> String {
        "No message passed to `pytest.fail()`".to_string()
    }

    fn placeholder() -> Self {
        FailWithoutMessage
    }
}

define_violation!(
    pub struct AssertInExcept(pub String);
);
impl Violation for AssertInExcept {
    fn message(&self) -> String {
        let AssertInExcept(name) = self;
        format!(
            "Found assertion on exception `{name}` in except block, use `pytest.raises()` instead"
        )
    }

    fn placeholder() -> Self {
        AssertInExcept("...".to_string())
    }
}

define_violation!(
    pub struct CompositeAssertion;
);
impl Violation for CompositeAssertion {
    fn message(&self) -> String {
        "Assertion should be broken down into multiple parts".to_string()
    }

    fn placeholder() -> Self {
        CompositeAssertion
    }
}

define_violation!(
    pub struct FixtureParamWithoutValue(pub String);
);
impl Violation for FixtureParamWithoutValue {
    fn message(&self) -> String {
        let FixtureParamWithoutValue(name) = self;
        format!(
            "Fixture `{name}` without value is injected as parameter, use \
             `@pytest.mark.usefixtures` instead"
        )
    }

    fn placeholder() -> Self {
        FixtureParamWithoutValue("...".to_string())
    }
}

define_violation!(
    pub struct DeprecatedYieldFixture;
);
impl Violation for DeprecatedYieldFixture {
    fn message(&self) -> String {
        "`@pytest.yield_fixture` is deprecated, use `@pytest.fixture`".to_string()
    }

    fn placeholder() -> Self {
        DeprecatedYieldFixture
    }
}

define_violation!(
    pub struct FixtureFinalizerCallback;
);
impl Violation for FixtureFinalizerCallback {
    fn message(&self) -> String {
        "Use `yield` instead of `request.addfinalizer`".to_string()
    }

    fn placeholder() -> Self {
        FixtureFinalizerCallback
    }
}

define_violation!(
    pub struct UselessYieldFixture(pub String);
);
impl AlwaysAutofixableViolation for UselessYieldFixture {
    fn message(&self) -> String {
        let UselessYieldFixture(name) = self;
        format!("No teardown in fixture `{name}`, use `return` instead of `yield`")
    }

    fn autofix_title(&self) -> String {
        "Replace `yield` with `return`".to_string()
    }

    fn placeholder() -> Self {
        UselessYieldFixture("...".to_string())
    }
}

define_violation!(
    pub struct IncorrectMarkParenthesesStyle(pub String, pub String, pub String);
);
impl AlwaysAutofixableViolation for IncorrectMarkParenthesesStyle {
    fn message(&self) -> String {
        let IncorrectMarkParenthesesStyle(mark_name, expected_parens, actual_parens) = self;
        format!(
            "Use `@pytest.mark.{mark_name}{expected_parens}` over \
             `@pytest.mark.{mark_name}{actual_parens}`"
        )
    }

    fn autofix_title(&self) -> String {
        "Add/remove parentheses".to_string()
    }

    fn placeholder() -> Self {
        IncorrectMarkParenthesesStyle("...".to_string(), String::new(), "()".to_string())
    }
}

define_violation!(
    pub struct UnnecessaryAsyncioMarkOnFixture;
);
impl AlwaysAutofixableViolation for UnnecessaryAsyncioMarkOnFixture {
    fn message(&self) -> String {
        "`pytest.mark.asyncio` is unnecessary for fixtures".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `pytest.mark.asyncio`".to_string()
    }

    fn placeholder() -> Self {
        UnnecessaryAsyncioMarkOnFixture
    }
}

define_violation!(
    pub struct ErroneousUseFixturesOnFixture;
);
impl AlwaysAutofixableViolation for ErroneousUseFixturesOnFixture {
    fn message(&self) -> String {
        "`pytest.mark.usefixtures` has no effect on fixtures".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `pytest.mark.usefixtures`".to_string()
    }

    fn placeholder() -> Self {
        ErroneousUseFixturesOnFixture
    }
}

define_violation!(
    pub struct UseFixturesWithoutParameters;
);
impl AlwaysAutofixableViolation for UseFixturesWithoutParameters {
    fn message(&self) -> String {
        "Useless `pytest.mark.usefixtures` without parameters".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove `usefixtures` decorator or pass parameters".to_string()
    }

    fn placeholder() -> Self {
        UseFixturesWithoutParameters
    }
}

// flake8-pie

define_violation!(
    pub struct NoUnnecessaryPass;
);
impl AlwaysAutofixableViolation for NoUnnecessaryPass {
    fn message(&self) -> String {
        "Unnecessary `pass` statement".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }

    fn placeholder() -> Self {
        NoUnnecessaryPass
    }
}

define_violation!(
    pub struct DupeClassFieldDefinitions(pub String);
);
impl AlwaysAutofixableViolation for DupeClassFieldDefinitions {
    fn message(&self) -> String {
        let DupeClassFieldDefinitions(name) = self;
        format!("Class field `{name}` is defined multiple times")
    }

    fn autofix_title(&self) -> String {
        let DupeClassFieldDefinitions(name) = self;
        format!("Remove duplicate field definition for `{name}`")
    }

    fn placeholder() -> Self {
        DupeClassFieldDefinitions("...".to_string())
    }
}

define_violation!(
    pub struct PreferUniqueEnums {
        pub value: String,
    }
);
impl Violation for PreferUniqueEnums {
    fn message(&self) -> String {
        let PreferUniqueEnums { value } = self;
        format!("Enum contains duplicate value: `{value}`")
    }

    fn placeholder() -> Self {
        PreferUniqueEnums {
            value: "...".to_string(),
        }
    }
}

define_violation!(
    pub struct PreferListBuiltin;
);
impl AlwaysAutofixableViolation for PreferListBuiltin {
    fn message(&self) -> String {
        "Prefer `list()` over useless lambda".to_string()
    }

    fn autofix_title(&self) -> String {
        "Replace with `list`".to_string()
    }

    fn placeholder() -> Self {
        PreferListBuiltin
    }
}

// flake8-commas

define_violation!(
    pub struct TrailingCommaMissing;
);
impl AlwaysAutofixableViolation for TrailingCommaMissing {
    fn message(&self) -> String {
        "Trailing comma missing".to_string()
    }

    fn autofix_title(&self) -> String {
        "Add trailing comma".to_string()
    }

    fn placeholder() -> Self {
        TrailingCommaMissing
    }
}

define_violation!(
    pub struct TrailingCommaOnBareTupleProhibited;
);
impl Violation for TrailingCommaOnBareTupleProhibited {
    fn message(&self) -> String {
        "Trailing comma on bare tuple prohibited".to_string()
    }

    fn placeholder() -> Self {
        TrailingCommaOnBareTupleProhibited
    }
}

define_violation!(
    pub struct TrailingCommaProhibited;
);
impl AlwaysAutofixableViolation for TrailingCommaProhibited {
    fn message(&self) -> String {
        "Trailing comma prohibited".to_string()
    }

    fn autofix_title(&self) -> String {
        "Remove trailing comma".to_string()
    }

    fn placeholder() -> Self {
        TrailingCommaProhibited
    }
}

// Ruff

define_violation!(
    pub struct AmbiguousUnicodeCharacterString {
        pub confusable: char,
        pub representant: char,
    }
);
impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterString {
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterString {
            confusable,
            representant,
        } = self;
        format!(
            "String contains ambiguous unicode character '{confusable}' (did you mean \
             '{representant}'?)"
        )
    }

    fn autofix_title(&self) -> String {
        let AmbiguousUnicodeCharacterString {
            confusable,
            representant,
        } = self;
        format!("Replace '{confusable}' with '{representant}'")
    }

    fn placeholder() -> Self {
        AmbiguousUnicodeCharacterString {
            confusable: '𝐁',
            representant: 'B',
        }
    }
}

define_violation!(
    pub struct AmbiguousUnicodeCharacterDocstring {
        pub confusable: char,
        pub representant: char,
    }
);
impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterDocstring {
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterDocstring {
            confusable,
            representant,
        } = self;
        format!(
            "Docstring contains ambiguous unicode character '{confusable}' (did you mean \
             '{representant}'?)"
        )
    }

    fn autofix_title(&self) -> String {
        let AmbiguousUnicodeCharacterDocstring {
            confusable,
            representant,
        } = self;
        format!("Replace '{confusable}' with '{representant}'")
    }

    fn placeholder() -> Self {
        AmbiguousUnicodeCharacterDocstring {
            confusable: '𝐁',
            representant: 'B',
        }
    }
}

define_violation!(
    pub struct AmbiguousUnicodeCharacterComment {
        pub confusable: char,
        pub representant: char,
    }
);
impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterComment {
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterComment {
            confusable,
            representant,
        } = self;
        format!(
            "Comment contains ambiguous unicode character '{confusable}' (did you mean \
             '{representant}'?)"
        )
    }

    fn autofix_title(&self) -> String {
        let AmbiguousUnicodeCharacterComment {
            confusable,
            representant,
        } = self;
        format!("Replace '{confusable}' with '{representant}'")
    }

    fn placeholder() -> Self {
        AmbiguousUnicodeCharacterComment {
            confusable: '𝐁',
            representant: 'B',
        }
    }
}

define_violation!(
    pub struct KeywordArgumentBeforeStarArgument(pub String);
);
impl Violation for KeywordArgumentBeforeStarArgument {
    fn message(&self) -> String {
        let KeywordArgumentBeforeStarArgument(name) = self;
        format!("Keyword argument `{name}` must come after starred arguments")
    }

    fn placeholder() -> Self {
        KeywordArgumentBeforeStarArgument("...".to_string())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnusedCodes {
    pub unknown: Vec<String>,
    pub disabled: Vec<String>,
    pub unmatched: Vec<String>,
}

define_violation!(
    pub struct UnusedNOQA(pub Option<UnusedCodes>);
);
impl AlwaysAutofixableViolation for UnusedNOQA {
    fn message(&self) -> String {
        let UnusedNOQA(codes) = self;
        match codes {
            None => "Unused blanket `noqa` directive".to_string(),
            Some(codes) => {
                let mut codes_by_reason = vec![];
                if !codes.unmatched.is_empty() {
                    codes_by_reason.push(format!(
                        "unused: {}",
                        codes
                            .unmatched
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if !codes.disabled.is_empty() {
                    codes_by_reason.push(format!(
                        "non-enabled: {}",
                        codes
                            .disabled
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if !codes.unknown.is_empty() {
                    codes_by_reason.push(format!(
                        "unknown: {}",
                        codes
                            .unknown
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if codes_by_reason.is_empty() {
                    "Unused `noqa` directive".to_string()
                } else {
                    format!("Unused `noqa` directive ({})", codes_by_reason.join("; "))
                }
            }
        }
    }

    fn autofix_title(&self) -> String {
        "Remove unused `noqa` directive".to_string()
    }

    fn placeholder() -> Self {
        UnusedNOQA(None)
    }
}
