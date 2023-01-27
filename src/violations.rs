#![allow(clippy::useless_format)]
use std::fmt;

use itertools::Itertools;
use ruff_macros::derive_message_formats;
use rustpython_ast::Cmpop;
use serde::{Deserialize, Serialize};

use crate::define_violation;
use crate::rules::flake8_debugger::types::DebuggerUsingType;
use crate::rules::flake8_pytest_style::types::{
    ParametrizeNameType, ParametrizeValuesRowType, ParametrizeValuesType,
};
use crate::rules::flake8_quotes::settings::Quote;
use crate::rules::pyupgrade::types::Primitive;
use crate::violation::{AlwaysAutofixableViolation, AutofixKind, Availability, Violation};

// pycodestyle errors

define_violation!(
    pub struct MultipleImportsOnOneLine;
);
impl Violation for MultipleImportsOnOneLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple imports on one line")
    }
}

define_violation!(
    pub struct ModuleImportNotAtTopOfFile;
);
impl Violation for ModuleImportNotAtTopOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Module level import not at top of file")
    }
}

define_violation!(
    pub struct IOError(pub String);
);
impl Violation for IOError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IOError(message) = self;
        format!("{message}")
    }
}

define_violation!(
    pub struct SyntaxError(pub String);
);
impl Violation for SyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SyntaxError(message) = self;
        format!("SyntaxError: {message}")
    }
}

// pyflakes

define_violation!(
    pub struct UnusedImport {
        pub name: String,
        pub ignore_init: bool,
        pub multiple: bool,
    }
);
fn fmt_unused_import_autofix_msg(unused_import: &UnusedImport) -> String {
    let UnusedImport { name, multiple, .. } = unused_import;
    if *multiple {
        "Remove unused import".to_string()
    } else {
        format!("Remove unused import: `{name}`")
    }
}
impl Violation for UnusedImport {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport {
            name, ignore_init, ..
        } = self;
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
        let UnusedImport { ignore_init, .. } = self;
        if *ignore_init {
            None
        } else {
            Some(fmt_unused_import_autofix_msg)
        }
    }
}

define_violation!(
    pub struct ImportShadowedByLoopVar(pub String, pub usize);
);
impl Violation for ImportShadowedByLoopVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportShadowedByLoopVar(name, line) = self;
        format!("Import `{name}` from line {line} shadowed by loop variable")
    }
}

define_violation!(
    pub struct ImportStarUsed(pub String);
);
impl Violation for ImportStarUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarUsed(name) = self;
        format!("`from {name} import *` used; unable to detect undefined names")
    }
}

define_violation!(
    pub struct LateFutureImport;
);
impl Violation for LateFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`from __future__` imports must occur at the beginning of the file")
    }
}

define_violation!(
    pub struct ImportStarUsage(pub String, pub Vec<String>);
);
impl Violation for ImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarUsage(name, sources) = self;
        let sources = sources
            .iter()
            .map(|source| format!("`{source}`"))
            .join(", ");
        format!("`{name}` may be undefined, or defined from star imports: {sources}")
    }
}

define_violation!(
    pub struct ImportStarNotPermitted(pub String);
);
impl Violation for ImportStarNotPermitted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarNotPermitted(name) = self;
        format!("`from {name} import *` only allowed at module level")
    }
}

define_violation!(
    pub struct FutureFeatureNotDefined(pub String);
);
impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined(name) = self;
        format!("Future feature `{name}` is not defined")
    }
}

define_violation!(
    pub struct PercentFormatInvalidFormat(pub String);
);
impl Violation for PercentFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatInvalidFormat(message) = self;
        format!("`%`-format string has invalid format string: {message}")
    }
}

define_violation!(
    pub struct PercentFormatExpectedMapping;
);
impl Violation for PercentFormatExpectedMapping {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string expected mapping but got sequence")
    }
}

define_violation!(
    pub struct PercentFormatExpectedSequence;
);
impl Violation for PercentFormatExpectedSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string expected sequence but got mapping")
    }
}

define_violation!(
    pub struct PercentFormatExtraNamedArguments(pub Vec<String>);
);
impl AlwaysAutofixableViolation for PercentFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("`%`-format string has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let PercentFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

define_violation!(
    pub struct PercentFormatMissingArgument(pub Vec<String>);
);
impl Violation for PercentFormatMissingArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatMissingArgument(missing) = self;
        let message = missing.join(", ");
        format!("`%`-format string is missing argument(s) for placeholder(s): {message}")
    }
}

define_violation!(
    pub struct PercentFormatMixedPositionalAndNamed;
);
impl Violation for PercentFormatMixedPositionalAndNamed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string has mixed positional and named placeholders")
    }
}

define_violation!(
    pub struct PercentFormatPositionalCountMismatch(pub usize, pub usize);
);
impl Violation for PercentFormatPositionalCountMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatPositionalCountMismatch(wanted, got) = self;
        format!("`%`-format string has {wanted} placeholder(s) but {got} substitution(s)")
    }
}

define_violation!(
    pub struct PercentFormatStarRequiresSequence;
);
impl Violation for PercentFormatStarRequiresSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string `*` specifier requires sequence")
    }
}

define_violation!(
    pub struct PercentFormatUnsupportedFormatCharacter(pub char);
);
impl Violation for PercentFormatUnsupportedFormatCharacter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatUnsupportedFormatCharacter(char) = self;
        format!("`%`-format string has unsupported format character '{char}'")
    }
}

define_violation!(
    pub struct StringDotFormatInvalidFormat(pub String);
);
impl Violation for StringDotFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatInvalidFormat(message) = self;
        format!("`.format` call has invalid format string: {message}")
    }
}

define_violation!(
    pub struct StringDotFormatExtraNamedArguments(pub Vec<String>);
);
impl AlwaysAutofixableViolation for StringDotFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("`.format` call has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let StringDotFormatExtraNamedArguments(missing) = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

define_violation!(
    pub struct StringDotFormatExtraPositionalArguments(pub Vec<String>);
);
impl Violation for StringDotFormatExtraPositionalArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraPositionalArguments(missing) = self;
        let message = missing.join(", ");
        format!("`.format` call has unused arguments at position(s): {message}")
    }
}

define_violation!(
    pub struct StringDotFormatMissingArguments(pub Vec<String>);
);
impl Violation for StringDotFormatMissingArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatMissingArguments(missing) = self;
        let message = missing.join(", ");
        format!("`.format` call is missing argument(s) for placeholder(s): {message}")
    }
}

define_violation!(
    pub struct StringDotFormatMixingAutomatic;
);
impl Violation for StringDotFormatMixingAutomatic {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.format` string mixes automatic and manual numbering")
    }
}

define_violation!(
    pub struct FStringMissingPlaceholders;
);
impl AlwaysAutofixableViolation for FStringMissingPlaceholders {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("f-string without any placeholders")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous `f` prefix".to_string()
    }
}

define_violation!(
    pub struct MultiValueRepeatedKeyLiteral(pub String, pub bool);
);
impl Violation for MultiValueRepeatedKeyLiteral {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
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
}

define_violation!(
    pub struct MultiValueRepeatedKeyVariable(pub String, pub bool);
);
impl Violation for MultiValueRepeatedKeyVariable {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
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
}

define_violation!(
    pub struct ExpressionsInStarAssignment;
);
impl Violation for ExpressionsInStarAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many expressions in star-unpacking assignment")
    }
}

define_violation!(
    pub struct TwoStarredExpressions;
);
impl Violation for TwoStarredExpressions {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Two starred expressions in assignment")
    }
}

define_violation!(
    pub struct AssertTuple;
);
impl Violation for AssertTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assert test is a non-empty tuple, which is always `True`")
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
    #[derive_message_formats]
    fn message(&self) -> String {
        let IsLiteral(cmpop) = self;
        match cmpop {
            IsCmpop::Is => format!("Use `==` to compare constant literals"),
            IsCmpop::IsNot => format!("Use `!=` to compare constant literals"),
        }
    }

    fn autofix_title(&self) -> String {
        let IsLiteral(cmpop) = self;
        match cmpop {
            IsCmpop::Is => "Replace `is` with `==`".to_string(),
            IsCmpop::IsNot => "Replace `is not` with `!=`".to_string(),
        }
    }
}

define_violation!(
    pub struct InvalidPrintSyntax;
);
impl Violation for InvalidPrintSyntax {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `>>` is invalid with `print` function")
    }
}

define_violation!(
    pub struct IfTuple;
);
impl Violation for IfTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("If test is a tuple, which is always `True`")
    }
}

define_violation!(
    pub struct BreakOutsideLoop;
);
impl Violation for BreakOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`break` outside loop")
    }
}

define_violation!(
    pub struct ContinueOutsideLoop;
);
impl Violation for ContinueOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`continue` not properly in loop")
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
    #[derive_message_formats]
    fn message(&self) -> String {
        let YieldOutsideFunction(keyword) = self;
        format!("`{keyword}` statement outside of a function")
    }
}

define_violation!(
    pub struct ReturnOutsideFunction;
);
impl Violation for ReturnOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`return` statement outside of a function/method")
    }
}

define_violation!(
    pub struct DefaultExceptNotLast;
);
impl Violation for DefaultExceptNotLast {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("An `except` block as not the last exception handler")
    }
}

define_violation!(
    pub struct ForwardAnnotationSyntaxError(pub String);
);
impl Violation for ForwardAnnotationSyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ForwardAnnotationSyntaxError(body) = self;
        format!("Syntax error in forward annotation: `{body}`")
    }
}

define_violation!(
    pub struct RedefinedWhileUnused(pub String, pub usize);
);
impl Violation for RedefinedWhileUnused {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedWhileUnused(name, line) = self;
        format!("Redefinition of unused `{name}` from line {line}")
    }
}

define_violation!(
    pub struct UndefinedName(pub String);
);
impl Violation for UndefinedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedName(name) = self;
        format!("Undefined name `{name}`")
    }
}

define_violation!(
    pub struct UndefinedExport(pub String);
);
impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport(name) = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}

define_violation!(
    pub struct UndefinedLocal(pub String);
);
impl Violation for UndefinedLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocal(name) = self;
        format!("Local variable `{name}` referenced before assignment")
    }
}

define_violation!(
    pub struct UnusedVariable(pub String);
);
impl AlwaysAutofixableViolation for UnusedVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedVariable(name) = self;
        format!("Local variable `{name}` is assigned to but never used")
    }

    fn autofix_title(&self) -> String {
        let UnusedVariable(name) = self;
        format!("Remove assignment to unused variable `{name}`")
    }
}

define_violation!(
    pub struct UnusedAnnotation(pub String);
);
impl Violation for UnusedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedAnnotation(name) = self;
        format!("Local variable `{name}` is annotated but never used")
    }
}

define_violation!(
    pub struct RaiseNotImplemented;
);
impl AlwaysAutofixableViolation for RaiseNotImplemented {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`raise NotImplemented` should be `raise NotImplementedError`")
    }

    fn autofix_title(&self) -> String {
        "Use `raise NotImplementedError`".to_string()
    }
}

// pylint

define_violation!(
    pub struct UselessImportAlias;
);
impl AlwaysAutofixableViolation for UselessImportAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Import alias does not rename original package")
    }

    fn autofix_title(&self) -> String {
        "Remove import alias".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryDirectLambdaCall;
);
impl Violation for UnnecessaryDirectLambdaCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Lambda expression called directly. Execute the expression inline instead.")
    }
}

define_violation!(
    pub struct NonlocalWithoutBinding(pub String);
);
impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding(name) = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}

define_violation!(
    pub struct UsedPriorGlobalDeclaration(pub String, pub usize);
);
impl Violation for UsedPriorGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UsedPriorGlobalDeclaration(name, line) = self;
        format!("Name `{name}` is used prior to global declaration on line {line}")
    }
}

define_violation!(
    pub struct AwaitOutsideAsync;
);
impl Violation for AwaitOutsideAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`await` should be used within an async function")
    }
}

define_violation!(
    pub struct PropertyWithParameters;
);
impl Violation for PropertyWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot have defined parameters for properties")
    }
}

define_violation!(
    pub struct ConsiderUsingFromImport(pub String, pub String);
);
impl Violation for ConsiderUsingFromImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingFromImport(module, name) = self;
        format!("Use `from {module} import {name}` in lieu of alias")
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
    #[derive_message_formats]
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
}

define_violation!(
    pub struct ConsiderMergingIsinstance(pub String, pub Vec<String>);
);
impl Violation for ConsiderMergingIsinstance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderMergingIsinstance(obj, types) = self;
        let types = types.join(", ");
        format!("Merge these isinstance calls: `isinstance({obj}, ({types}))`")
    }
}

define_violation!(
    pub struct UseSysExit(pub String);
);
impl Violation for UseSysExit {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UseSysExit(name) = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|UseSysExit(name)| format!("Replace `{name}` with `sys.exit()`"))
    }
}

define_violation!(
    pub struct MagicValueComparison {
        pub value: String,
    }
);
impl Violation for MagicValueComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MagicValueComparison { value } = self;
        format!(
            "Magic value used in comparison, consider replacing {value} with a constant variable"
        )
    }
}

define_violation!(
    pub struct UselessElseOnLoop;
);
impl Violation for UselessElseOnLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Else clause on loop without a break statement, remove the else and de-indent all the \
             code inside it"
        )
    }
}

define_violation!(
    pub struct GlobalVariableNotAssigned(pub String);
);
impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned(name) = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}

// flake8-builtins

define_violation!(
    pub struct BuiltinVariableShadowing(pub String);
);
impl Violation for BuiltinVariableShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinVariableShadowing(name) = self;
        format!("Variable `{name}` is shadowing a python builtin")
    }
}

define_violation!(
    pub struct BuiltinArgumentShadowing(pub String);
);
impl Violation for BuiltinArgumentShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinArgumentShadowing(name) = self;
        format!("Argument `{name}` is shadowing a python builtin")
    }
}

define_violation!(
    pub struct BuiltinAttributeShadowing(pub String);
);
impl Violation for BuiltinAttributeShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinAttributeShadowing(name) = self;
        format!("Class attribute `{name}` is shadowing a python builtin")
    }
}

// flake8-bugbear

define_violation!(
    pub struct UnaryPrefixIncrement;
);
impl Violation for UnaryPrefixIncrement {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Python does not support the unary prefix increment")
    }
}

define_violation!(
    pub struct AssignmentToOsEnviron;
);
impl Violation for AssignmentToOsEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assigning to `os.environ` doesn't clear the environment")
    }
}

define_violation!(
    pub struct UnreliableCallableCheck;
);
impl Violation for UnreliableCallableCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            " Using `hasattr(x, '__call__')` to test if x is callable is unreliable. Use \
             `callable(x)` for consistent results."
        )
    }
}

define_violation!(
    pub struct StripWithMultiCharacters;
);
impl Violation for StripWithMultiCharacters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `.strip()` with multi-character strings is misleading the reader")
    }
}

define_violation!(
    pub struct MutableArgumentDefault;
);
impl Violation for MutableArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
}

define_violation!(
    pub struct UnusedLoopControlVariable {
        /// The name of the loop control variable.
        pub name: String,
        /// Whether the variable is safe to rename. A variable is unsafe to
        /// rename if it _might_ be used by magic control flow (e.g.,
        /// `locals()`).
        pub safe: bool,
    }
);
impl Violation for UnusedLoopControlVariable {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLoopControlVariable { name, safe } = self;
        if *safe {
            format!("Loop control variable `{name}` not used within loop body")
        } else {
            format!("Loop control variable `{name}` may not be used within loop body")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let UnusedLoopControlVariable { safe, .. } = self;
        if *safe {
            Some(|UnusedLoopControlVariable { name, .. }| {
                format!("Rename unused `{name}` to `_{name}`")
            })
        } else {
            None
        }
    }
}

define_violation!(
    pub struct FunctionCallArgumentDefault(pub Option<String>);
);
impl Violation for FunctionCallArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionCallArgumentDefault(name) = self;
        if let Some(name) = name {
            format!("Do not perform function call `{name}` in argument defaults")
        } else {
            format!("Do not perform function call in argument defaults")
        }
    }
}

define_violation!(
    pub struct GetAttrWithConstant;
);
impl AlwaysAutofixableViolation for GetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `getattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn autofix_title(&self) -> String {
        "Replace `getattr` with attribute access".to_string()
    }
}

define_violation!(
    pub struct SetAttrWithConstant;
);
impl AlwaysAutofixableViolation for SetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `setattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn autofix_title(&self) -> String {
        "Replace `setattr` with assignment".to_string()
    }
}

define_violation!(
    pub struct DoNotAssertFalse;
);
impl AlwaysAutofixableViolation for DoNotAssertFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not `assert False` (`python -O` removes these calls), raise `AssertionError()`")
    }

    fn autofix_title(&self) -> String {
        "Replace `assert False`".to_string()
    }
}

define_violation!(
    pub struct JumpStatementInFinally(pub String);
);
impl Violation for JumpStatementInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        let JumpStatementInFinally(name) = self;
        format!("`{name}` inside `finally` blocks cause exceptions to be silenced")
    }
}

define_violation!(
    pub struct RedundantTupleInExceptionHandler(pub String);
);
impl AlwaysAutofixableViolation for RedundantTupleInExceptionHandler {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct DuplicateHandlerException(pub Vec<String>);
);
impl AlwaysAutofixableViolation for DuplicateHandlerException {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct UselessComparison;
);
impl Violation for UselessComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Pointless comparison. This comparison does nothing but waste CPU instructions. \
             Either prepend `assert` or remove it."
        )
    }
}

define_violation!(
    pub struct CannotRaiseLiteral;
);
impl Violation for CannotRaiseLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot raise a literal. Did you intend to return it or raise an Exception?")
    }
}

define_violation!(
    pub struct NoAssertRaisesException;
);
impl Violation for NoAssertRaisesException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`assertRaises(Exception)` should be considered evil")
    }
}

define_violation!(
    pub struct UselessExpression;
);
impl Violation for UselessExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found useless expression. Either assign it to a variable or remove it.")
    }
}

define_violation!(
    pub struct CachedInstanceMethod;
);
impl Violation for CachedInstanceMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use of `functools.lru_cache` or `functools.cache` on methods can lead to memory leaks"
        )
    }
}

define_violation!(
    pub struct LoopVariableOverridesIterator(pub String);
);
impl Violation for LoopVariableOverridesIterator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoopVariableOverridesIterator(name) = self;
        format!("Loop control variable `{name}` overrides iterable it iterates")
    }
}

define_violation!(
    pub struct FStringDocstring;
);
impl Violation for FStringDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "f-string used as docstring. This will be interpreted by python as a joined string \
             rather than a docstring."
        )
    }
}

define_violation!(
    pub struct UselessContextlibSuppress;
);
impl Violation for UselessContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "No arguments passed to `contextlib.suppress`. No exceptions will be suppressed and \
             therefore this context manager is redundant"
        )
    }
}

define_violation!(
    pub struct FunctionUsesLoopVariable(pub String);
);
impl Violation for FunctionUsesLoopVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionUsesLoopVariable(name) = self;
        format!("Function definition does not bind loop variable `{name}`")
    }
}

define_violation!(
    pub struct AbstractBaseClassWithoutAbstractMethod(pub String);
);
impl Violation for AbstractBaseClassWithoutAbstractMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AbstractBaseClassWithoutAbstractMethod(name) = self;
        format!("`{name}` is an abstract base class, but it has no abstract methods")
    }
}

define_violation!(
    pub struct DuplicateTryBlockException(pub String);
);
impl Violation for DuplicateTryBlockException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateTryBlockException(name) = self;
        format!("try-except block with duplicate exception `{name}`")
    }
}

define_violation!(
    pub struct StarArgUnpackingAfterKeywordArg;
);
impl Violation for StarArgUnpackingAfterKeywordArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Star-arg unpacking after a keyword argument is strongly discouraged")
    }
}

define_violation!(
    pub struct EmptyMethodWithoutAbstractDecorator(pub String);
);
impl Violation for EmptyMethodWithoutAbstractDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EmptyMethodWithoutAbstractDecorator(name) = self;
        format!(
            "`{name}` is an empty method in an abstract base class, but has no abstract decorator"
        )
    }
}

define_violation!(
    pub struct RaiseWithoutFromInsideExcept;
);
impl Violation for RaiseWithoutFromInsideExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Within an except clause, raise exceptions with `raise ... from err` or `raise ... \
             from None` to distinguish them from errors in exception handling"
        )
    }
}

define_violation!(
    pub struct ZipWithoutExplicitStrict;
);
impl Violation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`zip()` without an explicit `strict=` parameter")
    }
}

// flake8-blind-except

define_violation!(
    pub struct BlindExcept(pub String);
);
impl Violation for BlindExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlindExcept(name) = self;
        format!("Do not catch blind exception: `{name}`")
    }
}

// flake8-comprehensions

define_violation!(
    pub struct UnnecessaryGeneratorList;
);
impl AlwaysAutofixableViolation for UnnecessaryGeneratorList {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `list` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `list` comprehension".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryGeneratorSet;
);
impl AlwaysAutofixableViolation for UnnecessaryGeneratorSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `set` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryGeneratorDict;
);
impl AlwaysAutofixableViolation for UnnecessaryGeneratorDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `dict` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryListComprehensionSet;
);
impl AlwaysAutofixableViolation for UnnecessaryListComprehensionSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` comprehension (rewrite as a `set` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryListComprehensionDict;
);
impl AlwaysAutofixableViolation for UnnecessaryListComprehensionDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` comprehension (rewrite as a `dict` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryLiteralSet(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralSet(obj_type) = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `set` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` literal".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryLiteralDict(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralDict(obj_type) = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `dict` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` literal".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryCollectionCall(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryCollectionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCollectionCall(obj_type) = self;
        format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a literal".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryLiteralWithinTupleCall(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralWithinTupleCall {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct UnnecessaryLiteralWithinListCall(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryLiteralWithinListCall {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct UnnecessaryListCall;
);
impl AlwaysAutofixableViolation for UnnecessaryListCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` call (remove the outer call to `list()`)")
    }

    fn autofix_title(&self) -> String {
        "Remove outer `list` call".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryCallAroundSorted(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryCallAroundSorted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCallAroundSorted(func) = self;
        format!("Unnecessary `{func}` call around `sorted()`")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryCallAroundSorted(func) = self;
        format!("Remove unnecessary `{func}` call")
    }
}

define_violation!(
    pub struct UnnecessaryDoubleCastOrProcess(pub String, pub String);
);
impl Violation for UnnecessaryDoubleCastOrProcess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDoubleCastOrProcess(inner, outer) = self;
        format!("Unnecessary `{inner}` call within `{outer}()`")
    }
}

define_violation!(
    pub struct UnnecessarySubscriptReversal(pub String);
);
impl Violation for UnnecessarySubscriptReversal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessarySubscriptReversal(func) = self;
        format!("Unnecessary subscript reversal of iterable within `{func}()`")
    }
}

define_violation!(
    pub struct UnnecessaryComprehension(pub String);
);
impl AlwaysAutofixableViolation for UnnecessaryComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryComprehension(obj_type) = self;
        format!("Unnecessary `{obj_type}` comprehension (rewrite using `{obj_type}()`)")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryComprehension(obj_type) = self;
        format!("Rewrite using `{obj_type}()`")
    }
}

define_violation!(
    pub struct UnnecessaryMap(pub String);
);
impl Violation for UnnecessaryMap {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryMap(obj_type) = self;
        if obj_type == "generator" {
            format!("Unnecessary `map` usage (rewrite using a generator expression)")
        } else {
            format!("Unnecessary `map` usage (rewrite using a `{obj_type}` comprehension)")
        }
    }
}

// flake8-debugger

define_violation!(
    pub struct Debugger(pub DebuggerUsingType);
);
impl Violation for Debugger {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Debugger(using_type) = self;
        match using_type {
            DebuggerUsingType::Call(name) => format!("Trace found: `{name}` used"),
            DebuggerUsingType::Import(name) => format!("Import for `{name}` found"),
        }
    }
}

// mccabe

define_violation!(
    pub struct FunctionIsTooComplex(pub String, pub usize);
);
impl Violation for FunctionIsTooComplex {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionIsTooComplex(name, complexity) = self;
        format!("`{name}` is too complex ({complexity})")
    }
}

// flake8-return

define_violation!(
    pub struct UnnecessaryReturnNone;
);
impl AlwaysAutofixableViolation for UnnecessaryReturnNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not explicitly `return None` in function if it is the only possible return value"
        )
    }

    fn autofix_title(&self) -> String {
        "Remove explicit `return None`".to_string()
    }
}

define_violation!(
    pub struct ImplicitReturnValue;
);
impl AlwaysAutofixableViolation for ImplicitReturnValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not implicitly `return None` in function able to return non-`None` value")
    }

    fn autofix_title(&self) -> String {
        "Add explicit `None` return value".to_string()
    }
}

define_violation!(
    pub struct ImplicitReturn;
);
impl AlwaysAutofixableViolation for ImplicitReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing explicit `return` at the end of function able to return non-`None` value")
    }

    fn autofix_title(&self) -> String {
        "Add explicit `return` statement".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryAssign;
);
impl Violation for UnnecessaryAssign {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary variable assignment before `return` statement")
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
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseReturn(branch) = self;
        format!("Unnecessary `{branch}` after `return` statement")
    }
}

define_violation!(
    pub struct SuperfluousElseRaise(pub Branch);
);
impl Violation for SuperfluousElseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseRaise(branch) = self;
        format!("Unnecessary `{branch}` after `raise` statement")
    }
}

define_violation!(
    pub struct SuperfluousElseContinue(pub Branch);
);
impl Violation for SuperfluousElseContinue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseContinue(branch) = self;
        format!("Unnecessary `{branch}` after `continue` statement")
    }
}

define_violation!(
    pub struct SuperfluousElseBreak(pub Branch);
);
impl Violation for SuperfluousElseBreak {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseBreak(branch) = self;
        format!("Unnecessary `{branch}` after `break` statement")
    }
}

// flake8-implicit-str-concat

define_violation!(
    pub struct SingleLineImplicitStringConcatenation;
);
impl Violation for SingleLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals on one line")
    }
}

define_violation!(
    pub struct MultiLineImplicitStringConcatenation;
);
impl Violation for MultiLineImplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Implicitly concatenated string literals over continuation line")
    }
}

define_violation!(
    pub struct ExplicitStringConcatenation;
);
impl Violation for ExplicitStringConcatenation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Explicitly concatenated string should be implicitly concatenated")
    }
}

// flake8-print

define_violation!(
    pub struct PrintFound;
);
impl AlwaysAutofixableViolation for PrintFound {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`print` found")
    }

    fn autofix_title(&self) -> String {
        "Remove `print`".to_string()
    }
}

define_violation!(
    pub struct PPrintFound;
);
impl AlwaysAutofixableViolation for PPrintFound {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pprint` found")
    }

    fn autofix_title(&self) -> String {
        "Remove `pprint`".to_string()
    }
}

// flake8-quotes

define_violation!(
    pub struct BadQuotesInlineString(pub Quote);
);
impl AlwaysAutofixableViolation for BadQuotesInlineString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesInlineString(quote) = self;
        match quote {
            Quote::Single => format!("Double quotes found but single quotes preferred"),
            Quote::Double => format!("Single quotes found but double quotes preferred"),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesInlineString(quote) = self;
        match quote {
            Quote::Single => "Replace double quotes with single quotes".to_string(),
            Quote::Double => "Replace single quotes with double quotes".to_string(),
        }
    }
}

define_violation!(
    pub struct BadQuotesMultilineString(pub Quote);
);
impl AlwaysAutofixableViolation for BadQuotesMultilineString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesMultilineString(quote) = self;
        match quote {
            Quote::Single => format!("Double quote multiline found but single quotes preferred"),
            Quote::Double => format!("Single quote multiline found but double quotes preferred"),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesMultilineString(quote) = self;
        match quote {
            Quote::Single => "Replace double multiline quotes with single quotes".to_string(),
            Quote::Double => "Replace single multiline quotes with double quotes".to_string(),
        }
    }
}

define_violation!(
    pub struct BadQuotesDocstring(pub Quote);
);
impl AlwaysAutofixableViolation for BadQuotesDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadQuotesDocstring(quote) = self;
        match quote {
            Quote::Single => format!("Double quote docstring found but single quotes preferred"),
            Quote::Double => format!("Single quote docstring found but double quotes preferred"),
        }
    }

    fn autofix_title(&self) -> String {
        let BadQuotesDocstring(quote) = self;
        match quote {
            Quote::Single => "Replace double quotes docstring with single quotes".to_string(),
            Quote::Double => "Replace single quotes docstring with double quotes".to_string(),
        }
    }
}

define_violation!(
    pub struct AvoidQuoteEscape;
);
impl AlwaysAutofixableViolation for AvoidQuoteEscape {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Change outer quotes to avoid escaping inner quotes")
    }

    fn autofix_title(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }
}

// flake8-annotations

define_violation!(
    pub struct MissingTypeFunctionArgument(pub String);
);
impl Violation for MissingTypeFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeFunctionArgument(name) = self;
        format!("Missing type annotation for function argument `{name}`")
    }
}

define_violation!(
    pub struct MissingTypeArgs(pub String);
);
impl Violation for MissingTypeArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeArgs(name) = self;
        format!("Missing type annotation for `*{name}`")
    }
}

define_violation!(
    pub struct MissingTypeKwargs(pub String);
);
impl Violation for MissingTypeKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeKwargs(name) = self;
        format!("Missing type annotation for `**{name}`")
    }
}

define_violation!(
    pub struct MissingTypeSelf(pub String);
);
impl Violation for MissingTypeSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeSelf(name) = self;
        format!("Missing type annotation for `{name}` in method")
    }
}

define_violation!(
    pub struct MissingTypeCls(pub String);
);
impl Violation for MissingTypeCls {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeCls(name) = self;
        format!("Missing type annotation for `{name}` in classmethod")
    }
}

define_violation!(
    pub struct MissingReturnTypePublicFunction(pub String);
);
impl Violation for MissingReturnTypePublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePublicFunction(name) = self;
        format!("Missing return type annotation for public function `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypePrivateFunction(pub String);
);
impl Violation for MissingReturnTypePrivateFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePrivateFunction(name) = self;
        format!("Missing return type annotation for private function `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypeSpecialMethod(pub String);
);
impl AlwaysAutofixableViolation for MissingReturnTypeSpecialMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeSpecialMethod(name) = self;
        format!("Missing return type annotation for special method `{name}`")
    }

    fn autofix_title(&self) -> String {
        "Add `None` return type".to_string()
    }
}

define_violation!(
    pub struct MissingReturnTypeStaticMethod(pub String);
);
impl Violation for MissingReturnTypeStaticMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeStaticMethod(name) = self;
        format!("Missing return type annotation for staticmethod `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypeClassMethod(pub String);
);
impl Violation for MissingReturnTypeClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeClassMethod(name) = self;
        format!("Missing return type annotation for classmethod `{name}`")
    }
}

define_violation!(
    pub struct DynamicallyTypedExpression(pub String);
);
impl Violation for DynamicallyTypedExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DynamicallyTypedExpression(name) = self;
        format!("Dynamically typed expressions (typing.Any) are disallowed in `{name}`")
    }
}

// flake8-2020

define_violation!(
    pub struct SysVersionSlice3Referenced;
);
impl Violation for SysVersionSlice3Referenced {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[:3]` referenced (python3.10), use `sys.version_info`")
    }
}

define_violation!(
    pub struct SysVersion2Referenced;
);
impl Violation for SysVersion2Referenced {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[2]` referenced (python3.10), use `sys.version_info`")
    }
}

define_violation!(
    pub struct SysVersionCmpStr3;
);
impl Violation for SysVersionCmpStr3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python3.10), use `sys.version_info`")
    }
}

define_violation!(
    pub struct SysVersionInfo0Eq3Referenced;
);
impl Violation for SysVersionInfo0Eq3Referenced {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version_info[0] == 3` referenced (python4), use `>=`")
    }
}

define_violation!(
    pub struct SixPY3Referenced;
);
impl Violation for SixPY3Referenced {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`six.PY3` referenced (python4), use `not six.PY2`")
    }
}

define_violation!(
    pub struct SysVersionInfo1CmpInt;
);
impl Violation for SysVersionInfo1CmpInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`sys.version_info[1]` compared to integer (python4), compare `sys.version_info` to \
             tuple"
        )
    }
}

define_violation!(
    pub struct SysVersionInfoMinorCmpInt;
);
impl Violation for SysVersionInfoMinorCmpInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`sys.version_info.minor` compared to integer (python4), compare `sys.version_info` \
             to tuple"
        )
    }
}

define_violation!(
    pub struct SysVersion0Referenced;
);
impl Violation for SysVersion0Referenced {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[0]` referenced (python10), use `sys.version_info`")
    }
}

define_violation!(
    pub struct SysVersionCmpStr10;
);
impl Violation for SysVersionCmpStr10 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python10), use `sys.version_info`")
    }
}

define_violation!(
    pub struct SysVersionSlice1Referenced;
);
impl Violation for SysVersionSlice1Referenced {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[:1]` referenced (python10), use `sys.version_info`")
    }
}

// flake8-simplify

define_violation!(
    pub struct OpenFileWithContextHandler;
);
impl Violation for OpenFileWithContextHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use context handler for opening files")
    }
}

define_violation!(
    pub struct UseCapitalEnvironmentVariables(pub String, pub String);
);
impl AlwaysAutofixableViolation for UseCapitalEnvironmentVariables {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UseCapitalEnvironmentVariables(expected, original) = self;
        format!("Use capitalized environment variable `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> String {
        let UseCapitalEnvironmentVariables(expected, original) = self;
        format!("Replace `{original}` with `{expected}`")
    }
}

define_violation!(
    pub struct DuplicateIsinstanceCall(pub String);
);
impl AlwaysAutofixableViolation for DuplicateIsinstanceCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateIsinstanceCall(name) = self;
        format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
    }

    fn autofix_title(&self) -> String {
        let DuplicateIsinstanceCall(name) = self;
        format!("Merge `isinstance` calls for `{name}`")
    }
}

define_violation!(
    pub struct NestedIfStatements;
);
impl AlwaysAutofixableViolation for NestedIfStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a single `if` statement instead of nested `if` statements")
    }

    fn autofix_title(&self) -> String {
        "Combine `if` statements using `and`".to_string()
    }
}

define_violation!(
    pub struct ReturnBoolConditionDirectly(pub String);
);
impl AlwaysAutofixableViolation for ReturnBoolConditionDirectly {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ReturnBoolConditionDirectly(cond) = self;
        format!("Return the condition `{cond}` directly")
    }

    fn autofix_title(&self) -> String {
        let ReturnBoolConditionDirectly(cond) = self;
        format!("Replace with `return {cond}`")
    }
}

define_violation!(
    pub struct UseContextlibSuppress(pub String);
);
impl Violation for UseContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UseContextlibSuppress(exception) = self;
        format!("Use `contextlib.suppress({exception})` instead of try-except-pass")
    }
}

define_violation!(
    pub struct ReturnInTryExceptFinally;
);
impl Violation for ReturnInTryExceptFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't use `return` in `try`/`except` and `finally`")
    }
}

define_violation!(
    pub struct UseTernaryOperator(pub String);
);
impl Violation for UseTernaryOperator {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UseTernaryOperator(contents) = self;
        format!("Use ternary operator `{contents}` instead of if-else-block")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|UseTernaryOperator(contents)| format!("Replace if-else-block with `{contents}`"))
    }
}

define_violation!(
    pub struct CompareWithTuple {
        pub replacement: String,
    }
);
impl AlwaysAutofixableViolation for CompareWithTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Use `{replacement}` instead of multiple equality comparisons")
    }

    fn autofix_title(&self) -> String {
        let CompareWithTuple { replacement, .. } = self;
        format!("Replace with `{replacement}`")
    }
}

define_violation!(
    pub struct ConvertLoopToAny(pub String);
);
impl AlwaysAutofixableViolation for ConvertLoopToAny {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertLoopToAny(any) = self;
        format!("Use `{any}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ConvertLoopToAny(any) = self;
        format!("Replace with `{any}`")
    }
}

define_violation!(
    pub struct ConvertLoopToAll(pub String);
);
impl AlwaysAutofixableViolation for ConvertLoopToAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertLoopToAll(all) = self;
        format!("Use `{all}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ConvertLoopToAll(all) = self;
        format!("Replace with `{all}`")
    }
}

define_violation!(
    pub struct MultipleWithStatements;
);
impl AlwaysAutofixableViolation for MultipleWithStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use a single `with` statement with multiple contexts instead of nested `with` \
             statements"
        )
    }

    fn autofix_title(&self) -> String {
        "Combine `with` statements".to_string()
    }
}

define_violation!(
    pub struct KeyInDict(pub String, pub String);
);
impl AlwaysAutofixableViolation for KeyInDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let KeyInDict(key, dict) = self;
        format!("Use `{key} in {dict}` instead of `{key} in {dict}.keys()`")
    }

    fn autofix_title(&self) -> String {
        let KeyInDict(key, dict) = self;
        format!("Convert to `{key} in {dict}`")
    }
}

define_violation!(
    pub struct NegateEqualOp(pub String, pub String);
);
impl AlwaysAutofixableViolation for NegateEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateEqualOp(left, right) = self;
        format!("Use `{left} != {right}` instead of `not {left} == {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `!=` operator".to_string()
    }
}

define_violation!(
    pub struct NegateNotEqualOp(pub String, pub String);
);
impl AlwaysAutofixableViolation for NegateNotEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateNotEqualOp(left, right) = self;
        format!("Use `{left} == {right}` instead of `not {left} != {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `==` operator".to_string()
    }
}

define_violation!(
    pub struct DoubleNegation(pub String);
);
impl AlwaysAutofixableViolation for DoubleNegation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DoubleNegation(expr) = self;
        format!("Use `{expr}` instead of `not (not {expr})`")
    }

    fn autofix_title(&self) -> String {
        let DoubleNegation(expr) = self;
        format!("Replace with `{expr}`")
    }
}

define_violation!(
    pub struct AAndNotA(pub String);
);
impl AlwaysAutofixableViolation for AAndNotA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AAndNotA(name) = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

define_violation!(
    pub struct AOrNotA(pub String);
);
impl AlwaysAutofixableViolation for AOrNotA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AOrNotA(name) = self;
        format!("Use `True` instead of `{name} or not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

define_violation!(
    pub struct OrTrue;
);
impl AlwaysAutofixableViolation for OrTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `True` instead of `... or True`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

define_violation!(
    pub struct AndFalse;
);
impl AlwaysAutofixableViolation for AndFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `False` instead of `... and False`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

define_violation!(
    pub struct YodaConditions {
        pub suggestion: String,
    }
);
impl AlwaysAutofixableViolation for YodaConditions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let YodaConditions { suggestion } = self;
        format!("Yoda conditions are discouraged, use `{suggestion}` instead")
    }

    fn autofix_title(&self) -> String {
        let YodaConditions { suggestion } = self;
        format!("Replace Yoda condition with `{suggestion}`")
    }
}

define_violation!(
    pub struct IfExprWithTrueFalse(pub String);
);
impl AlwaysAutofixableViolation for IfExprWithTrueFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTrueFalse(expr) = self;
        format!("Use `bool({expr})` instead of `True if {expr} else False`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTrueFalse(expr) = self;
        format!("Replace with `not {expr}")
    }
}

define_violation!(
    pub struct IfExprWithFalseTrue(pub String);
);
impl AlwaysAutofixableViolation for IfExprWithFalseTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithFalseTrue(expr) = self;
        format!("Use `not {expr}` instead of `False if {expr} else True`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithFalseTrue(expr) = self;
        format!("Replace with `bool({expr})")
    }
}

define_violation!(
    pub struct IfExprWithTwistedArms(pub String, pub String);
);
impl AlwaysAutofixableViolation for IfExprWithTwistedArms {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct DictGetWithDefault(pub String);
);
impl AlwaysAutofixableViolation for DictGetWithDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DictGetWithDefault(contents) = self;
        format!("Use `{contents}` instead of an `if` block")
    }

    fn autofix_title(&self) -> String {
        let DictGetWithDefault(contents) = self;
        format!("Replace with `{contents}`")
    }
}

define_violation!(
    pub struct UnpackInsteadOfConcatenatingToCollectionLiteral(pub String);
);
impl Violation for UnpackInsteadOfConcatenatingToCollectionLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnpackInsteadOfConcatenatingToCollectionLiteral(expr) = self;
        format!("Consider `{expr}` instead of concatenation")
    }
}

// pyupgrade

define_violation!(
    pub struct UselessMetaclassType;
);
impl AlwaysAutofixableViolation for UselessMetaclassType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__metaclass__ = type` is implied")
    }

    fn autofix_title(&self) -> String {
        "Remove `__metaclass__ = type`".to_string()
    }
}

define_violation!(
    pub struct TypeOfPrimitive(pub Primitive);
);
impl AlwaysAutofixableViolation for TypeOfPrimitive {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeOfPrimitive(primitive) = self;
        format!("Use `{}` instead of `type(...)`", primitive.builtin())
    }

    fn autofix_title(&self) -> String {
        let TypeOfPrimitive(primitive) = self;
        format!("Replace `type(...)` with `{}`", primitive.builtin())
    }
}

define_violation!(
    pub struct UselessObjectInheritance(pub String);
);
impl AlwaysAutofixableViolation for UselessObjectInheritance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UselessObjectInheritance(name) = self;
        format!("Class `{name}` inherits from `object`")
    }

    fn autofix_title(&self) -> String {
        "Remove `object` inheritance".to_string()
    }
}

define_violation!(
    pub struct DeprecatedUnittestAlias(pub String, pub String);
);
impl AlwaysAutofixableViolation for DeprecatedUnittestAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedUnittestAlias(alias, target) = self;
        format!("`{alias}` is deprecated, use `{target}`")
    }

    fn autofix_title(&self) -> String {
        let DeprecatedUnittestAlias(alias, target) = self;
        format!("Replace `{target}` with `{alias}`")
    }
}

define_violation!(
    pub struct UsePEP585Annotation(pub String);
);
impl AlwaysAutofixableViolation for UsePEP585Annotation {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct UsePEP604Annotation;
);
impl AlwaysAutofixableViolation for UsePEP604Annotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` for type annotations")
    }

    fn autofix_title(&self) -> String {
        "Convert to `X | Y`".to_string()
    }
}

define_violation!(
    pub struct SuperCallWithParameters;
);
impl AlwaysAutofixableViolation for SuperCallWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `super()` instead of `super(__class__, self)`")
    }

    fn autofix_title(&self) -> String {
        "Remove `__super__` parameters".to_string()
    }
}

define_violation!(
    pub struct PEP3120UnnecessaryCodingComment;
);
impl AlwaysAutofixableViolation for PEP3120UnnecessaryCodingComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("UTF-8 encoding declaration is unnecessary")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary coding comment".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryFutureImport(pub Vec<String>);
);
impl AlwaysAutofixableViolation for UnnecessaryFutureImport {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct LRUCacheWithoutParameters;
);
impl AlwaysAutofixableViolation for LRUCacheWithoutParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parameters to `functools.lru_cache`")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary parameters".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryEncodeUTF8;
);
impl AlwaysAutofixableViolation for UnnecessaryEncodeUTF8 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary call to `encode` as UTF-8")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `encode`".to_string()
    }
}

define_violation!(
    pub struct ConvertTypedDictFunctionalToClass(pub String);
);
impl AlwaysAutofixableViolation for ConvertTypedDictFunctionalToClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass(name) = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertTypedDictFunctionalToClass(name) = self;
        format!("Convert `{name}` to class syntax")
    }
}

define_violation!(
    pub struct ConvertNamedTupleFunctionalToClass(pub String);
);
impl AlwaysAutofixableViolation for ConvertNamedTupleFunctionalToClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass(name) = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertNamedTupleFunctionalToClass(name) = self;
        format!("Convert `{name}` to class syntax")
    }
}

define_violation!(
    pub struct RedundantOpenModes(pub Option<String>);
);
impl AlwaysAutofixableViolation for RedundantOpenModes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantOpenModes(replacement) = self;
        match replacement {
            None => format!("Unnecessary open mode parameters"),
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
}

define_violation!(
    pub struct RemoveSixCompat;
);
impl AlwaysAutofixableViolation for RemoveSixCompat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `six` compatibility usage")
    }

    fn autofix_title(&self) -> String {
        "Remove `six` usage".to_string()
    }
}

define_violation!(
    pub struct DatetimeTimezoneUTC {
        pub straight_import: bool,
    }
);
impl Violation for DatetimeTimezoneUTC {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `datetime.UTC` alias")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        if self.straight_import {
            Some(|_| "Convert to `datetime.UTC` alias".to_string())
        } else {
            None
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
    #[derive_message_formats]
    fn message(&self) -> String {
        let NativeLiterals(literal_type) = self;
        format!("Unnecessary call to `{literal_type}`")
    }

    fn autofix_title(&self) -> String {
        let NativeLiterals(literal_type) = self;
        format!("Replace with `{literal_type}`")
    }
}

define_violation!(
    pub struct TypingTextStrAlias;
);
impl AlwaysAutofixableViolation for TypingTextStrAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`typing.Text` is deprecated, use `str`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `str`".to_string()
    }
}

define_violation!(
    pub struct OpenAlias;
);
impl AlwaysAutofixableViolation for OpenAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title(&self) -> String {
        "Replace with builtin `open`".to_string()
    }
}

define_violation!(
    pub struct ReplaceUniversalNewlines;
);
impl AlwaysAutofixableViolation for ReplaceUniversalNewlines {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`universal_newlines` is deprecated, use `text`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `text` keyword argument".to_string()
    }
}

define_violation!(
    pub struct PrintfStringFormatting;
);
impl AlwaysAutofixableViolation for PrintfStringFormatting {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use format specifiers instead of percent format")
    }

    fn autofix_title(&self) -> String {
        "Replace with format specifiers".to_string()
    }
}

define_violation!(
    pub struct ReplaceStdoutStderr;
);
impl AlwaysAutofixableViolation for ReplaceStdoutStderr {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Sending stdout and stderr to pipe is deprecated, use `capture_output`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `capture_output` keyword argument".to_string()
    }
}

define_violation!(
    pub struct RewriteCElementTree;
);
impl AlwaysAutofixableViolation for RewriteCElementTree {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`cElementTree` is deprecated, use `ElementTree`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `ElementTree`".to_string()
    }
}

define_violation!(
    pub struct OSErrorAlias(pub Option<String>);
);
impl AlwaysAutofixableViolation for OSErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace aliased errors with `OSError`")
    }

    fn autofix_title(&self) -> String {
        let OSErrorAlias(name) = self;
        match name {
            None => "Replace with builtin `OSError`".to_string(),
            Some(name) => format!("Replace `{name}` with builtin `OSError`"),
        }
    }
}

define_violation!(
    pub struct RewriteUnicodeLiteral;
);
impl AlwaysAutofixableViolation for RewriteUnicodeLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove unicode literals from strings")
    }

    fn autofix_title(&self) -> String {
        "Remove unicode prefix".to_string()
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
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`mock` is deprecated, use `unittest.mock`")
    }

    fn autofix_title(&self) -> String {
        let RewriteMockImport(reference_type) = self;
        match reference_type {
            MockReference::Import => "Import from `unittest.mock` instead".to_string(),
            MockReference::Attribute => "Replace `mock.mock` with `mock`".to_string(),
        }
    }
}

define_violation!(
    pub struct RewriteListComprehension;
);
impl AlwaysAutofixableViolation for RewriteListComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace unpacked list comprehension with a generator expression")
    }

    fn autofix_title(&self) -> String {
        "Replace with generator expression".to_string()
    }
}

define_violation!(
    pub struct RewriteYieldFrom;
);
impl AlwaysAutofixableViolation for RewriteYieldFrom {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace `yield` over `for` loop with `yield from`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `yield from`".to_string()
    }
}

define_violation!(
    pub struct UnnecessaryBuiltinImport(pub Vec<String>);
);
impl AlwaysAutofixableViolation for UnnecessaryBuiltinImport {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct FormatLiterals;
);
impl AlwaysAutofixableViolation for FormatLiterals {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use implicit references for positional format fields")
    }

    fn autofix_title(&self) -> String {
        "Remove explicit positional indexes".to_string()
    }
}

define_violation!(
    pub struct ExtraneousParentheses;
);
impl AlwaysAutofixableViolation for ExtraneousParentheses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid extraneous parentheses")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous parentheses".to_string()
    }
}

define_violation!(
    pub struct FString;
);
impl AlwaysAutofixableViolation for FString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use f-string instead of `format` call")
    }

    fn autofix_title(&self) -> String {
        "Convert to f-string".to_string()
    }
}

define_violation!(
    pub struct FunctoolsCache;
);
impl AlwaysAutofixableViolation for FunctoolsCache {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `@functools.cache` instead of `@functools.lru_cache(maxsize=None)`")
    }

    fn autofix_title(&self) -> String {
        "Rewrite with `@functools.cache".to_string()
    }
}

// pydocstyle

define_violation!(
    pub struct PublicModule;
);
impl Violation for PublicModule {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public module")
    }
}

define_violation!(
    pub struct PublicClass;
);
impl Violation for PublicClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public class")
    }
}

define_violation!(
    pub struct PublicMethod;
);
impl Violation for PublicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public method")
    }
}

define_violation!(
    pub struct PublicFunction;
);
impl Violation for PublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public function")
    }
}

define_violation!(
    pub struct PublicPackage;
);
impl Violation for PublicPackage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public package")
    }
}

define_violation!(
    pub struct MagicMethod;
);
impl Violation for MagicMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in magic method")
    }
}

define_violation!(
    pub struct PublicNestedClass;
);
impl Violation for PublicNestedClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in public nested class")
    }
}

define_violation!(
    pub struct PublicInit;
);
impl Violation for PublicInit {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing docstring in `__init__`")
    }
}

define_violation!(
    pub struct FitsOnOneLine;
);
impl AlwaysAutofixableViolation for FitsOnOneLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("One-line docstring should fit on one line")
    }

    fn autofix_title(&self) -> String {
        "Reformat to one line".to_string()
    }
}

define_violation!(
    pub struct NoBlankLineBeforeFunction(pub usize);
);
impl AlwaysAutofixableViolation for NoBlankLineBeforeFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineBeforeFunction(num_lines) = self;
        format!("No blank lines allowed before function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before function docstring".to_string()
    }
}

define_violation!(
    pub struct NoBlankLineAfterFunction(pub usize);
);
impl AlwaysAutofixableViolation for NoBlankLineAfterFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineAfterFunction(num_lines) = self;
        format!("No blank lines allowed after function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) after function docstring".to_string()
    }
}

define_violation!(
    pub struct OneBlankLineBeforeClass(pub usize);
);
impl AlwaysAutofixableViolation for OneBlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("1 blank line required before class docstring")
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line before class docstring".to_string()
    }
}

define_violation!(
    pub struct OneBlankLineAfterClass(pub usize);
);
impl AlwaysAutofixableViolation for OneBlankLineAfterClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("1 blank line required after class docstring")
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line after class docstring".to_string()
    }
}

define_violation!(
    pub struct BlankLineAfterSummary(pub usize);
);
fn fmt_blank_line_after_summary_autofix_msg(_: &BlankLineAfterSummary) -> String {
    "Insert single blank line".to_string()
}
impl Violation for BlankLineAfterSummary {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSummary(num_lines) = self;
        if *num_lines == 0 {
            format!("1 blank line required between summary line and description")
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
}

define_violation!(
    pub struct IndentWithSpaces;
);
impl Violation for IndentWithSpaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring should be indented with spaces, not tabs")
    }
}

define_violation!(
    pub struct NoUnderIndentation;
);
impl AlwaysAutofixableViolation for NoUnderIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is under-indented")
    }

    fn autofix_title(&self) -> String {
        "Increase indentation".to_string()
    }
}

define_violation!(
    pub struct NoOverIndentation;
);
impl AlwaysAutofixableViolation for NoOverIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is over-indented")
    }

    fn autofix_title(&self) -> String {
        "Remove over-indentation".to_string()
    }
}

define_violation!(
    pub struct NewLineAfterLastParagraph;
);
impl AlwaysAutofixableViolation for NewLineAfterLastParagraph {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring closing quotes should be on a separate line")
    }

    fn autofix_title(&self) -> String {
        "Move closing quotes to new line".to_string()
    }
}

define_violation!(
    pub struct NoSurroundingWhitespace;
);
impl AlwaysAutofixableViolation for NoSurroundingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No whitespaces allowed surrounding docstring text")
    }

    fn autofix_title(&self) -> String {
        "Trim surrounding whitespace".to_string()
    }
}

define_violation!(
    pub struct NoBlankLineBeforeClass(pub usize);
);
impl AlwaysAutofixableViolation for NoBlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No blank lines allowed before class docstring")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before class docstring".to_string()
    }
}

define_violation!(
    pub struct MultiLineSummaryFirstLine;
);
impl AlwaysAutofixableViolation for MultiLineSummaryFirstLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring summary should start at the first line")
    }

    fn autofix_title(&self) -> String {
        "Remove whitespace after opening quotes".to_string()
    }
}

define_violation!(
    pub struct MultiLineSummarySecondLine;
);
impl AlwaysAutofixableViolation for MultiLineSummarySecondLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring summary should start at the second line")
    }

    fn autofix_title(&self) -> String {
        "Insert line break and indentation after opening quotes".to_string()
    }
}

define_violation!(
    pub struct SectionNotOverIndented(pub String);
);
impl AlwaysAutofixableViolation for SectionNotOverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionNotOverIndented(name) = self;
        format!("Section is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNotOverIndented(name) = self;
        format!("Remove over-indentation from \"{name}\"")
    }
}

define_violation!(
    pub struct SectionUnderlineNotOverIndented(pub String);
);
impl AlwaysAutofixableViolation for SectionUnderlineNotOverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineNotOverIndented(name) = self;
        format!("Section underline is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineNotOverIndented(name) = self;
        format!("Remove over-indentation from \"{name}\" underline")
    }
}

define_violation!(
    pub struct UsesTripleQuotes;
);
impl Violation for UsesTripleQuotes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use """triple double quotes""""#)
    }
}

define_violation!(
    pub struct UsesRPrefixForBackslashedContent;
);
impl Violation for UsesRPrefixForBackslashedContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use r""" if any backslashes in a docstring"#)
    }
}

define_violation!(
    pub struct EndsInPeriod;
);
impl AlwaysAutofixableViolation for EndsInPeriod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should end with a period")
    }

    fn autofix_title(&self) -> String {
        "Add period".to_string()
    }
}

define_violation!(
    pub struct NoSignature;
);
impl Violation for NoSignature {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should not be the function's signature")
    }
}

define_violation!(
    pub struct FirstLineCapitalized;
);
impl Violation for FirstLineCapitalized {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First word of the first line should be properly capitalized")
    }
}

define_violation!(
    pub struct NoThisPrefix;
);
impl Violation for NoThisPrefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"First word of the docstring should not be "This""#)
    }
}

define_violation!(
    pub struct CapitalizeSectionName(pub String);
);
impl AlwaysAutofixableViolation for CapitalizeSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CapitalizeSectionName(name) = self;
        format!("Section name should be properly capitalized (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let CapitalizeSectionName(name) = self;
        format!("Capitalize \"{name}\"")
    }
}

define_violation!(
    pub struct NewLineAfterSectionName(pub String);
);
impl AlwaysAutofixableViolation for NewLineAfterSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NewLineAfterSectionName(name) = self;
        format!("Section name should end with a newline (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let NewLineAfterSectionName(name) = self;
        format!("Add newline after \"{name}\"")
    }
}

define_violation!(
    pub struct DashedUnderlineAfterSection(pub String);
);
impl AlwaysAutofixableViolation for DashedUnderlineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DashedUnderlineAfterSection(name) = self;
        format!("Missing dashed underline after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let DashedUnderlineAfterSection(name) = self;
        format!("Add dashed line under \"{name}\"")
    }
}

define_violation!(
    pub struct SectionUnderlineAfterName(pub String);
);
impl AlwaysAutofixableViolation for SectionUnderlineAfterName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineAfterName(name) = self;
        format!("Section underline should be in the line following the section's name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineAfterName(name) = self;
        format!("Add underline to \"{name}\"")
    }
}

define_violation!(
    pub struct SectionUnderlineMatchesSectionLength(pub String);
);
impl AlwaysAutofixableViolation for SectionUnderlineMatchesSectionLength {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineMatchesSectionLength(name) = self;
        format!("Section underline should match the length of its name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineMatchesSectionLength(name) = self;
        format!("Adjust underline length to match \"{name}\"")
    }
}

define_violation!(
    pub struct BlankLineAfterSection(pub String);
);
impl AlwaysAutofixableViolation for BlankLineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSection(name) = self;
        format!("Missing blank line after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterSection(name) = self;
        format!("Add blank line after \"{name}\"")
    }
}

define_violation!(
    pub struct BlankLineBeforeSection(pub String);
);
impl AlwaysAutofixableViolation for BlankLineBeforeSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineBeforeSection(name) = self;
        format!("Missing blank line before section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineBeforeSection(name) = self;
        format!("Add blank line before \"{name}\"")
    }
}

define_violation!(
    pub struct NoBlankLinesBetweenHeaderAndContent(pub String);
);
impl AlwaysAutofixableViolation for NoBlankLinesBetweenHeaderAndContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLinesBetweenHeaderAndContent(name) = self;
        format!("No blank lines allowed between a section header and its content (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s)".to_string()
    }
}

define_violation!(
    pub struct BlankLineAfterLastSection(pub String);
);
impl AlwaysAutofixableViolation for BlankLineAfterLastSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterLastSection(name) = self;
        format!("Missing blank line after last section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterLastSection(name) = self;
        format!("Add blank line after \"{name}\"")
    }
}

define_violation!(
    pub struct NonEmptySection(pub String);
);
impl Violation for NonEmptySection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonEmptySection(name) = self;
        format!("Section has no content (\"{name}\")")
    }
}

define_violation!(
    pub struct EndsInPunctuation;
);
impl AlwaysAutofixableViolation for EndsInPunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should end with a period, question mark, or exclamation point")
    }

    fn autofix_title(&self) -> String {
        "Add closing punctuation".to_string()
    }
}

define_violation!(
    pub struct SectionNameEndsInColon(pub String);
);
impl AlwaysAutofixableViolation for SectionNameEndsInColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionNameEndsInColon(name) = self;
        format!("Section name should end with a colon (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNameEndsInColon(name) = self;
        format!("Add colon to \"{name}\"")
    }
}

define_violation!(
    pub struct DocumentAllArguments(pub Vec<String>);
);
impl Violation for DocumentAllArguments {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct SkipDocstring;
);
impl Violation for SkipDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function decorated with `@overload` shouldn't contain a docstring")
    }
}

define_violation!(
    pub struct NonEmpty;
);
impl Violation for NonEmpty {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is empty")
    }
}

// pep8-naming

define_violation!(
    pub struct InvalidClassName(pub String);
);
impl Violation for InvalidClassName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidClassName(name) = self;
        format!("Class name `{name}` should use CapWords convention ")
    }
}

define_violation!(
    pub struct InvalidFunctionName(pub String);
);
impl Violation for InvalidFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidFunctionName(name) = self;
        format!("Function name `{name}` should be lowercase")
    }
}

define_violation!(
    pub struct InvalidArgumentName(pub String);
);
impl Violation for InvalidArgumentName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidArgumentName(name) = self;
        format!("Argument name `{name}` should be lowercase")
    }
}

define_violation!(
    pub struct InvalidFirstArgumentNameForClassMethod;
);
impl Violation for InvalidFirstArgumentNameForClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a class method should be named `cls`")
    }
}

define_violation!(
    pub struct InvalidFirstArgumentNameForMethod;
);
impl Violation for InvalidFirstArgumentNameForMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a method should be named `self`")
    }
}

define_violation!(
    pub struct NonLowercaseVariableInFunction(pub String);
);
impl Violation for NonLowercaseVariableInFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonLowercaseVariableInFunction(name) = self;
        format!("Variable `{name}` in function should be lowercase")
    }
}

define_violation!(
    pub struct DunderFunctionName;
);
impl Violation for DunderFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function name should not start and end with `__`")
    }
}

define_violation!(
    pub struct ConstantImportedAsNonConstant(pub String, pub String);
);
impl Violation for ConstantImportedAsNonConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant(name, asname) = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }
}

define_violation!(
    pub struct LowercaseImportedAsNonLowercase(pub String, pub String);
);
impl Violation for LowercaseImportedAsNonLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LowercaseImportedAsNonLowercase(name, asname) = self;
        format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
    }
}

define_violation!(
    pub struct CamelcaseImportedAsLowercase(pub String, pub String);
);
impl Violation for CamelcaseImportedAsLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsLowercase(name, asname) = self;
        format!("Camelcase `{name}` imported as lowercase `{asname}`")
    }
}

define_violation!(
    pub struct CamelcaseImportedAsConstant(pub String, pub String);
);
impl Violation for CamelcaseImportedAsConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsConstant(name, asname) = self;
        format!("Camelcase `{name}` imported as constant `{asname}`")
    }
}

define_violation!(
    pub struct MixedCaseVariableInClassScope(pub String);
);
impl Violation for MixedCaseVariableInClassScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInClassScope(name) = self;
        format!("Variable `{name}` in class scope should not be mixedCase")
    }
}

define_violation!(
    pub struct MixedCaseVariableInGlobalScope(pub String);
);
impl Violation for MixedCaseVariableInGlobalScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInGlobalScope(name) = self;
        format!("Variable `{name}` in global scope should not be mixedCase")
    }
}

define_violation!(
    pub struct CamelcaseImportedAsAcronym(pub String, pub String);
);
impl Violation for CamelcaseImportedAsAcronym {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsAcronym(name, asname) = self;
        format!("Camelcase `{name}` imported as acronym `{asname}`")
    }
}

define_violation!(
    pub struct ErrorSuffixOnExceptionName(pub String);
);
impl Violation for ErrorSuffixOnExceptionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ErrorSuffixOnExceptionName(name) = self;
        format!("Exception name `{name}` should be named with an Error suffix")
    }
}

// flake8-bandit

define_violation!(
    pub struct Jinja2AutoescapeFalse(pub bool);
);
impl Violation for Jinja2AutoescapeFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Jinja2AutoescapeFalse(value) = self;
        match value {
            true => format!(
                "Using jinja2 templates with `autoescape=False` is dangerous and can lead to XSS. \
                 Ensure `autoescape=True` or use the `select_autoescape` function."
            ),
            false => format!(
                "By default, jinja2 sets `autoescape` to `False`. Consider using \
                 `autoescape=True` or the `select_autoescape` function to mitigate XSS \
                 vulnerabilities."
            ),
        }
    }
}

define_violation!(
    pub struct AssertUsed;
);
impl Violation for AssertUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `assert` detected")
    }
}

define_violation!(
    pub struct ExecUsed;
);
impl Violation for ExecUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `exec` detected")
    }
}

define_violation!(
    pub struct BadFilePermissions(pub u16);
);
impl Violation for BadFilePermissions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadFilePermissions(mask) = self;
        format!("`os.chmod` setting a permissive mask `{mask:#o}` on file or directory",)
    }
}

define_violation!(
    pub struct HardcodedBindAllInterfaces;
);
impl Violation for HardcodedBindAllInterfaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible binding to all interfaces")
    }
}

define_violation!(
    pub struct HardcodedPasswordString(pub String);
);
impl Violation for HardcodedPasswordString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordString(string) = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

define_violation!(
    pub struct HardcodedPasswordFuncArg(pub String);
);
impl Violation for HardcodedPasswordFuncArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordFuncArg(string) = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

define_violation!(
    pub struct HardcodedPasswordDefault(pub String);
);
impl Violation for HardcodedPasswordDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordDefault(string) = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

define_violation!(
    pub struct HardcodedTempFile(pub String);
);
impl Violation for HardcodedTempFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedTempFile(string) = self;
        format!(
            "Probable insecure usage of temporary file or directory: \"{}\"",
            string.escape_debug()
        )
    }
}

define_violation!(
    pub struct RequestWithoutTimeout(pub Option<String>);
);
impl Violation for RequestWithoutTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithoutTimeout(timeout) = self;
        match timeout {
            Some(value) => {
                format!("Probable use of requests call with timeout set to `{value}`")
            }
            None => format!("Probable use of requests call without timeout"),
        }
    }
}

define_violation!(
    pub struct HashlibInsecureHashFunction(pub String);
);
impl Violation for HashlibInsecureHashFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HashlibInsecureHashFunction(string) = self;
        format!(
            "Probable use of insecure hash functions in `hashlib`: \"{}\"",
            string.escape_debug()
        )
    }
}

define_violation!(
    pub struct RequestWithNoCertValidation(pub String);
);
impl Violation for RequestWithNoCertValidation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithNoCertValidation(string) = self;
        format!(
            "Probable use of `{string}` call with `verify=False` disabling SSL certificate checks"
        )
    }
}

define_violation!(
    pub struct UnsafeYAMLLoad(pub Option<String>);
);
impl Violation for UnsafeYAMLLoad {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsafeYAMLLoad(loader) = self;
        match loader {
            Some(name) => {
                format!(
                    "Probable use of unsafe loader `{name}` with `yaml.load`. Allows \
                     instantiation of arbitrary objects. Consider `yaml.safe_load`."
                )
            }
            None => format!(
                "Probable use of unsafe `yaml.load`. Allows instantiation of arbitrary objects. \
                 Consider `yaml.safe_load`."
            ),
        }
    }
}

define_violation!(
    pub struct SnmpInsecureVersion;
);
impl Violation for SnmpInsecureVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of SNMPv1 and SNMPv2 is insecure. Use SNMPv3 if able.")
    }
}

define_violation!(
    pub struct SnmpWeakCryptography;
);
impl Violation for SnmpWeakCryptography {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "You should not use SNMPv3 without encryption. `noAuthNoPriv` & `authNoPriv` is \
             insecure."
        )
    }
}

// flake8-unused-arguments

define_violation!(
    pub struct UnusedFunctionArgument(pub String);
);
impl Violation for UnusedFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedFunctionArgument(name) = self;
        format!("Unused function argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedMethodArgument(pub String);
);
impl Violation for UnusedMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedMethodArgument(name) = self;
        format!("Unused method argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedClassMethodArgument(pub String);
);
impl Violation for UnusedClassMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedClassMethodArgument(name) = self;
        format!("Unused class method argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedStaticMethodArgument(pub String);
);
impl Violation for UnusedStaticMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedStaticMethodArgument(name) = self;
        format!("Unused static method argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedLambdaArgument(pub String);
);
impl Violation for UnusedLambdaArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLambdaArgument(name) = self;
        format!("Unused lambda argument: `{name}`")
    }
}

// flake8-datetimez

define_violation!(
    pub struct CallDatetimeWithoutTzinfo;
);
impl Violation for CallDatetimeWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime()` without `tzinfo` argument is not allowed")
    }
}

define_violation!(
    pub struct CallDatetimeToday;
);
impl Violation for CallDatetimeToday {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.today()` is not allowed, use \
             `datetime.datetime.now(tz=)` instead"
        )
    }
}

define_violation!(
    pub struct CallDatetimeUtcnow;
);
impl Violation for CallDatetimeUtcnow {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.utcnow()` is not allowed, use \
             `datetime.datetime.now(tz=)` instead"
        )
    }
}

define_violation!(
    pub struct CallDatetimeUtcfromtimestamp;
);
impl Violation for CallDatetimeUtcfromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.utcfromtimestamp()` is not allowed, use \
             `datetime.datetime.fromtimestamp(ts, tz=)` instead"
        )
    }
}

define_violation!(
    pub struct CallDatetimeNowWithoutTzinfo;
);
impl Violation for CallDatetimeNowWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime.now()` without `tz` argument is not allowed")
    }
}

define_violation!(
    pub struct CallDatetimeFromtimestamp;
);
impl Violation for CallDatetimeFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.fromtimestamp()` without `tz` argument is not allowed"
        )
    }
}

define_violation!(
    pub struct CallDatetimeStrptimeWithoutZone;
);
impl Violation for CallDatetimeStrptimeWithoutZone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.strptime()` without %z must be followed by \
             `.replace(tzinfo=)` or `.astimezone()`"
        )
    }
}

define_violation!(
    pub struct CallDateToday;
);
impl Violation for CallDateToday {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.date.today()` is not allowed, use \
             `datetime.datetime.now(tz=).date()` instead"
        )
    }
}

define_violation!(
    pub struct CallDateFromtimestamp;
);
impl Violation for CallDateFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.date.fromtimestamp()` is not allowed, use \
             `datetime.datetime.fromtimestamp(ts, tz=).date()` instead"
        )
    }
}

// pygrep-hooks

define_violation!(
    pub struct NoEval;
);
impl Violation for NoEval {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No builtin `eval()` allowed")
    }
}

define_violation!(
    pub struct DeprecatedLogWarn;
);
impl Violation for DeprecatedLogWarn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`warn` is deprecated in favor of `warning`")
    }
}

define_violation!(
    pub struct BlanketTypeIgnore;
);
impl Violation for BlanketTypeIgnore {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use specific rule codes when ignoring type issues")
    }
}

define_violation!(
    pub struct BlanketNOQA;
);
impl Violation for BlanketNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use specific rule codes when using `noqa`")
    }
}

// pandas-vet

define_violation!(
    pub struct UseOfInplaceArgument;
);
impl Violation for UseOfInplaceArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`inplace=True` should be avoided; it has inconsistent behavior")
    }
}

define_violation!(
    pub struct UseOfDotIsNull;
);
impl Violation for UseOfDotIsNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.isna` is preferred to `.isnull`; functionality is equivalent")
    }
}

define_violation!(
    pub struct UseOfDotNotNull;
);
impl Violation for UseOfDotNotNull {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.notna` is preferred to `.notnull`; functionality is equivalent")
    }
}

define_violation!(
    pub struct UseOfDotIx;
);
impl Violation for UseOfDotIx {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.ix` is deprecated; use more explicit `.loc` or `.iloc`")
    }
}

define_violation!(
    pub struct UseOfDotAt;
);
impl Violation for UseOfDotAt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.loc` instead of `.at`.  If speed is important, use numpy.")
    }
}

define_violation!(
    pub struct UseOfDotIat;
);
impl Violation for UseOfDotIat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.iloc` instead of `.iat`.  If speed is important, use numpy.")
    }
}

define_violation!(
    pub struct UseOfDotPivotOrUnstack;
);
impl Violation for UseOfDotPivotOrUnstack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`.pivot_table` is preferred to `.pivot` or `.unstack`; provides same functionality"
        )
    }
}

define_violation!(
    pub struct UseOfDotValues;
);
impl Violation for UseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.to_numpy()` instead of `.values`")
    }
}

define_violation!(
    pub struct UseOfDotReadTable;
);
impl Violation for UseOfDotReadTable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.read_csv` is preferred to `.read_table`; provides same functionality")
    }
}

define_violation!(
    pub struct UseOfDotStack;
);
impl Violation for UseOfDotStack {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.melt` is preferred to `.stack`; provides same functionality")
    }
}

define_violation!(
    pub struct UseOfPdMerge;
);
impl Violation for UseOfPdMerge {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use `.merge` method instead of `pd.merge` function. They have equivalent \
             functionality."
        )
    }
}

define_violation!(
    pub struct DfIsABadVariableName;
);
impl Violation for DfIsABadVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`df` is a bad variable name. Be kinder to your future self.")
    }
}

// flake8-errmsg

define_violation!(
    pub struct RawStringInException;
);
impl Violation for RawStringInException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a string literal, assign to variable first")
    }
}

define_violation!(
    pub struct FStringInException;
);
impl Violation for FStringInException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use an f-string literal, assign to variable first")
    }
}

define_violation!(
    pub struct DotFormatInException;
);
impl Violation for DotFormatInException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a `.format()` string directly, assign to variable first")
    }
}

// flake8-pytest-style

define_violation!(
    pub struct IncorrectFixtureParenthesesStyle(pub String, pub String);
);
impl AlwaysAutofixableViolation for IncorrectFixtureParenthesesStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectFixtureParenthesesStyle(expected_parens, actual_parens) = self;
        format!("Use `@pytest.fixture{expected_parens}` over `@pytest.fixture{actual_parens}`")
    }

    fn autofix_title(&self) -> String {
        "Add/remove parentheses".to_string()
    }
}

define_violation!(
    pub struct FixturePositionalArgs(pub String);
);
impl Violation for FixturePositionalArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FixturePositionalArgs(function) = self;
        format!("Configuration for fixture `{function}` specified via positional args, use kwargs")
    }
}

define_violation!(
    pub struct ExtraneousScopeFunction;
);
impl Violation for ExtraneousScopeFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`scope='function'` is implied in `@pytest.fixture()`")
    }
}

define_violation!(
    pub struct MissingFixtureNameUnderscore(pub String);
);
impl Violation for MissingFixtureNameUnderscore {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingFixtureNameUnderscore(function) = self;
        format!("Fixture `{function}` does not return anything, add leading underscore")
    }
}

define_violation!(
    pub struct IncorrectFixtureNameUnderscore(pub String);
);
impl Violation for IncorrectFixtureNameUnderscore {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IncorrectFixtureNameUnderscore(function) = self;
        format!("Fixture `{function}` returns a value, remove leading underscore")
    }
}

define_violation!(
    pub struct ParametrizeNamesWrongType(pub ParametrizeNameType);
);
impl AlwaysAutofixableViolation for ParametrizeNamesWrongType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ParametrizeNamesWrongType(expected) = self;
        format!("Wrong name(s) type in `@pytest.mark.parametrize`, expected `{expected}`")
    }

    fn autofix_title(&self) -> String {
        let ParametrizeNamesWrongType(expected) = self;
        format!("Use a `{expected}` for parameter names")
    }
}

define_violation!(
    pub struct ParametrizeValuesWrongType(pub ParametrizeValuesType, pub ParametrizeValuesRowType);
);
impl Violation for ParametrizeValuesWrongType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ParametrizeValuesWrongType(values, row) = self;
        format!("Wrong values type in `@pytest.mark.parametrize` expected `{values}` of `{row}`")
    }
}

define_violation!(
    pub struct PatchWithLambda;
);
impl Violation for PatchWithLambda {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `return_value=` instead of patching with `lambda`")
    }
}

define_violation!(
    pub struct UnittestAssertion(pub String);
);
impl AlwaysAutofixableViolation for UnittestAssertion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnittestAssertion(assertion) = self;
        format!("Use a regular `assert` instead of unittest-style `{assertion}`")
    }

    fn autofix_title(&self) -> String {
        let UnittestAssertion(assertion) = self;
        format!("Replace `{assertion}(...)` with `assert ...`")
    }
}

define_violation!(
    pub struct RaisesWithoutException;
);
impl Violation for RaisesWithoutException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("set the expected exception in `pytest.raises()`")
    }
}

define_violation!(
    pub struct RaisesTooBroad(pub String);
);
impl Violation for RaisesTooBroad {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RaisesTooBroad(exception) = self;
        format!(
            "`pytest.raises({exception})` is too broad, set the `match` parameter or use a more \
             specific exception"
        )
    }
}

define_violation!(
    pub struct RaisesWithMultipleStatements;
);
impl Violation for RaisesWithMultipleStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pytest.raises()` block should contain a single simple statement")
    }
}

define_violation!(
    pub struct IncorrectPytestImport;
);
impl Violation for IncorrectPytestImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found incorrect import of pytest, use simple `import pytest` instead")
    }
}

define_violation!(
    pub struct AssertAlwaysFalse;
);
impl Violation for AssertAlwaysFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion always fails, replace with `pytest.fail()`")
    }
}

define_violation!(
    pub struct FailWithoutMessage;
);
impl Violation for FailWithoutMessage {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No message passed to `pytest.fail()`")
    }
}

define_violation!(
    pub struct AssertInExcept(pub String);
);
impl Violation for AssertInExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AssertInExcept(name) = self;
        format!(
            "Found assertion on exception `{name}` in except block, use `pytest.raises()` instead"
        )
    }
}

define_violation!(
    pub struct CompositeAssertion;
);
impl Violation for CompositeAssertion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assertion should be broken down into multiple parts")
    }
}

define_violation!(
    pub struct FixtureParamWithoutValue(pub String);
);
impl Violation for FixtureParamWithoutValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FixtureParamWithoutValue(name) = self;
        format!(
            "Fixture `{name}` without value is injected as parameter, use \
             `@pytest.mark.usefixtures` instead"
        )
    }
}

define_violation!(
    pub struct DeprecatedYieldFixture;
);
impl Violation for DeprecatedYieldFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`@pytest.yield_fixture` is deprecated, use `@pytest.fixture`")
    }
}

define_violation!(
    pub struct FixtureFinalizerCallback;
);
impl Violation for FixtureFinalizerCallback {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `yield` instead of `request.addfinalizer`")
    }
}

define_violation!(
    pub struct UselessYieldFixture(pub String);
);
impl AlwaysAutofixableViolation for UselessYieldFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UselessYieldFixture(name) = self;
        format!("No teardown in fixture `{name}`, use `return` instead of `yield`")
    }

    fn autofix_title(&self) -> String {
        "Replace `yield` with `return`".to_string()
    }
}

define_violation!(
    pub struct IncorrectMarkParenthesesStyle(pub String, pub String, pub String);
);
impl AlwaysAutofixableViolation for IncorrectMarkParenthesesStyle {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct UnnecessaryAsyncioMarkOnFixture;
);
impl AlwaysAutofixableViolation for UnnecessaryAsyncioMarkOnFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pytest.mark.asyncio` is unnecessary for fixtures")
    }

    fn autofix_title(&self) -> String {
        "Remove `pytest.mark.asyncio`".to_string()
    }
}

define_violation!(
    pub struct ErroneousUseFixturesOnFixture;
);
impl AlwaysAutofixableViolation for ErroneousUseFixturesOnFixture {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pytest.mark.usefixtures` has no effect on fixtures")
    }

    fn autofix_title(&self) -> String {
        "Remove `pytest.mark.usefixtures`".to_string()
    }
}

define_violation!(
    pub struct UseFixturesWithoutParameters;
);
impl AlwaysAutofixableViolation for UseFixturesWithoutParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Useless `pytest.mark.usefixtures` without parameters")
    }

    fn autofix_title(&self) -> String {
        "Remove `usefixtures` decorator or pass parameters".to_string()
    }
}

// ruff

define_violation!(
    pub struct AmbiguousUnicodeCharacterString {
        pub confusable: char,
        pub representant: char,
    }
);
impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterString {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct AmbiguousUnicodeCharacterDocstring {
        pub confusable: char,
        pub representant: char,
    }
);
impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterDocstring {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct AmbiguousUnicodeCharacterComment {
        pub confusable: char,
        pub representant: char,
    }
);
impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterComment {
    #[derive_message_formats]
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
}

define_violation!(
    pub struct KeywordArgumentBeforeStarArgument(pub String);
);
impl Violation for KeywordArgumentBeforeStarArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let KeywordArgumentBeforeStarArgument(name) = self;
        format!("Keyword argument `{name}` must come after starred arguments")
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
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedNOQA(codes) = self;
        match codes {
            None => format!("Unused blanket `noqa` directive"),
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
                    format!("Unused `noqa` directive")
                } else {
                    format!("Unused `noqa` directive ({})", codes_by_reason.join("; "))
                }
            }
        }
    }

    fn autofix_title(&self) -> String {
        "Remove unused `noqa` directive".to_string()
    }
}
