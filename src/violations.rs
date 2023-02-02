#![allow(clippy::useless_format)]
use std::fmt;

use itertools::Itertools;
use ruff_macros::derive_message_formats;
use rustpython_ast::Cmpop;
use serde::{Deserialize, Serialize};

use crate::define_violation;
use crate::rules::flake8_debugger::types::DebuggerUsingType;
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
    pub struct IOError {
        pub message: String,
    }
);
impl Violation for IOError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IOError { message } = self;
        format!("{message}")
    }
}

define_violation!(
    pub struct SyntaxError {
        pub message: String,
    }
);
impl Violation for SyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SyntaxError { message } = self;
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
    pub struct ImportShadowedByLoopVar {
        pub name: String,
        pub line: usize,
    }
);
impl Violation for ImportShadowedByLoopVar {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportShadowedByLoopVar { name, line } = self;
        format!("Import `{name}` from line {line} shadowed by loop variable")
    }
}

define_violation!(
    pub struct ImportStarUsed {
        pub name: String,
    }
);
impl Violation for ImportStarUsed {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarUsed { name } = self;
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
    pub struct ImportStarUsage {
        pub name: String,
        pub sources: Vec<String>,
    }
);
impl Violation for ImportStarUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarUsage { name, sources } = self;
        let sources = sources
            .iter()
            .map(|source| format!("`{source}`"))
            .join(", ");
        format!("`{name}` may be undefined, or defined from star imports: {sources}")
    }
}

define_violation!(
    pub struct ImportStarNotPermitted {
        pub name: String,
    }
);
impl Violation for ImportStarNotPermitted {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportStarNotPermitted { name } = self;
        format!("`from {name} import *` only allowed at module level")
    }
}

define_violation!(
    pub struct FutureFeatureNotDefined {
        pub name: String,
    }
);
impl Violation for FutureFeatureNotDefined {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FutureFeatureNotDefined { name } = self;
        format!("Future feature `{name}` is not defined")
    }
}

define_violation!(
    pub struct PercentFormatInvalidFormat {
        pub message: String,
    }
);
impl Violation for PercentFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatInvalidFormat { message } = self;
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
    pub struct PercentFormatExtraNamedArguments {
        pub missing: Vec<String>,
    }
);
impl AlwaysAutofixableViolation for PercentFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("`%`-format string has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let PercentFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

define_violation!(
    pub struct PercentFormatMissingArgument {
        pub missing: Vec<String>,
    }
);
impl Violation for PercentFormatMissingArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatMissingArgument { missing } = self;
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
    pub struct PercentFormatPositionalCountMismatch {
        pub wanted: usize,
        pub got: usize,
    }
);
impl Violation for PercentFormatPositionalCountMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatPositionalCountMismatch { wanted, got } = self;
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
    pub struct PercentFormatUnsupportedFormatCharacter {
        pub char: char,
    }
);
impl Violation for PercentFormatUnsupportedFormatCharacter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatUnsupportedFormatCharacter { char } = self;
        format!("`%`-format string has unsupported format character '{char}'")
    }
}

define_violation!(
    pub struct StringDotFormatInvalidFormat {
        pub message: String,
    }
);
impl Violation for StringDotFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatInvalidFormat { message } = self;
        format!("`.format` call has invalid format string: {message}")
    }
}

define_violation!(
    pub struct StringDotFormatExtraNamedArguments {
        pub missing: Vec<String>,
    }
);
impl AlwaysAutofixableViolation for StringDotFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let StringDotFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

define_violation!(
    pub struct StringDotFormatExtraPositionalArguments {
        pub missing: Vec<String>,
    }
);
impl Violation for StringDotFormatExtraPositionalArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraPositionalArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call has unused arguments at position(s): {message}")
    }
}

define_violation!(
    pub struct StringDotFormatMissingArguments {
        pub missing: Vec<String>,
    }
);
impl Violation for StringDotFormatMissingArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatMissingArguments { missing } = self;
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
    pub struct MultiValueRepeatedKeyLiteral {
        pub name: String,
        pub repeated_value: bool,
    }
);
impl Violation for MultiValueRepeatedKeyLiteral {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let MultiValueRepeatedKeyLiteral { name, .. } = self;
        format!("Dictionary key literal `{name}` repeated")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let MultiValueRepeatedKeyLiteral { repeated_value, .. } = self;
        if *repeated_value {
            Some(|MultiValueRepeatedKeyLiteral { name, .. }| {
                format!("Remove repeated key literal `{name}`")
            })
        } else {
            None
        }
    }
}

define_violation!(
    pub struct MultiValueRepeatedKeyVariable {
        pub name: String,
        pub repeated_value: bool,
    }
);
impl Violation for MultiValueRepeatedKeyVariable {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let MultiValueRepeatedKeyVariable { name, .. } = self;
        format!("Dictionary key `{name}` repeated")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let MultiValueRepeatedKeyVariable { repeated_value, .. } = self;
        if *repeated_value {
            Some(|MultiValueRepeatedKeyVariable { name, .. }| {
                format!("Remove repeated key `{name}`")
            })
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
    pub struct IsLiteral {
        pub cmpop: IsCmpop,
    }
);
impl AlwaysAutofixableViolation for IsLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IsLiteral { cmpop } = self;
        match cmpop {
            IsCmpop::Is => format!("Use `==` to compare constant literals"),
            IsCmpop::IsNot => format!("Use `!=` to compare constant literals"),
        }
    }

    fn autofix_title(&self) -> String {
        let IsLiteral { cmpop } = self;
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
    pub struct YieldOutsideFunction {
        pub keyword: DeferralKeyword,
    }
);
impl Violation for YieldOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let YieldOutsideFunction { keyword } = self;
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
    pub struct ForwardAnnotationSyntaxError {
        pub body: String,
    }
);
impl Violation for ForwardAnnotationSyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ForwardAnnotationSyntaxError { body } = self;
        format!("Syntax error in forward annotation: `{body}`")
    }
}

define_violation!(
    pub struct RedefinedWhileUnused {
        pub name: String,
        pub line: usize,
    }
);
impl Violation for RedefinedWhileUnused {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedWhileUnused { name, line } = self;
        format!("Redefinition of unused `{name}` from line {line}")
    }
}

define_violation!(
    pub struct UndefinedName {
        pub name: String,
    }
);
impl Violation for UndefinedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedName { name } = self;
        format!("Undefined name `{name}`")
    }
}

define_violation!(
    pub struct UndefinedExport {
        pub name: String,
    }
);
impl Violation for UndefinedExport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedExport { name } = self;
        format!("Undefined name `{name}` in `__all__`")
    }
}

define_violation!(
    pub struct UndefinedLocal {
        pub name: String,
    }
);
impl Violation for UndefinedLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocal { name } = self;
        format!("Local variable `{name}` referenced before assignment")
    }
}

define_violation!(
    pub struct UnusedVariable {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for UnusedVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedVariable { name } = self;
        format!("Local variable `{name}` is assigned to but never used")
    }

    fn autofix_title(&self) -> String {
        let UnusedVariable { name } = self;
        format!("Remove assignment to unused variable `{name}`")
    }
}

define_violation!(
    pub struct UnusedAnnotation {
        pub name: String,
    }
);
impl Violation for UnusedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedAnnotation { name } = self;
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
    pub struct NonlocalWithoutBinding {
        pub name: String,
    }
);
impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding { name } = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}

define_violation!(
    pub struct UsedPriorGlobalDeclaration {
        pub name: String,
        pub line: usize,
    }
);
impl Violation for UsedPriorGlobalDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UsedPriorGlobalDeclaration { name, line } = self;
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
    pub struct ConsiderUsingFromImport {
        pub module: String,
        pub name: String,
    }
);
impl Violation for ConsiderUsingFromImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderUsingFromImport { module, name } = self;
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
    pub struct ConsiderMergingIsinstance {
        pub obj: String,
        pub types: Vec<String>,
    }
);
impl Violation for ConsiderMergingIsinstance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConsiderMergingIsinstance { obj, types } = self;
        let types = types.join(", ");
        format!("Merge these isinstance calls: `isinstance({obj}, ({types}))`")
    }
}

define_violation!(
    pub struct UseSysExit {
        pub name: String,
    }
);
impl Violation for UseSysExit {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UseSysExit { name } = self;
        format!("Use `sys.exit()` instead of `{name}`")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|UseSysExit { name }| format!("Replace `{name}` with `sys.exit()`"))
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
    pub struct GlobalVariableNotAssigned {
        pub name: String,
    }
);
impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned { name } = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}

// flake8-debugger

define_violation!(
    pub struct Debugger {
        pub using_type: DebuggerUsingType,
    }
);
impl Violation for Debugger {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Debugger { using_type } = self;
        match using_type {
            DebuggerUsingType::Call(name) => format!("Trace found: `{name}` used"),
            DebuggerUsingType::Import(name) => format!("Import for `{name}` found"),
        }
    }
}

// mccabe

define_violation!(
    pub struct FunctionIsTooComplex {
        pub name: String,
        pub complexity: usize,
    }
);
impl Violation for FunctionIsTooComplex {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionIsTooComplex { name, complexity } = self;
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
    pub struct SuperfluousElseReturn {
        pub branch: Branch,
    }
);
impl Violation for SuperfluousElseReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseReturn { branch } = self;
        format!("Unnecessary `{branch}` after `return` statement")
    }
}

define_violation!(
    pub struct SuperfluousElseRaise {
        pub branch: Branch,
    }
);
impl Violation for SuperfluousElseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseRaise { branch } = self;
        format!("Unnecessary `{branch}` after `raise` statement")
    }
}

define_violation!(
    pub struct SuperfluousElseContinue {
        pub branch: Branch,
    }
);
impl Violation for SuperfluousElseContinue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseContinue { branch } = self;
        format!("Unnecessary `{branch}` after `continue` statement")
    }
}

define_violation!(
    pub struct SuperfluousElseBreak {
        pub branch: Branch,
    }
);
impl Violation for SuperfluousElseBreak {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SuperfluousElseBreak { branch } = self;
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
        format!("Implicitly concatenated string literals over multiple lines")
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

// flake8-annotations

define_violation!(
    pub struct MissingTypeFunctionArgument {
        pub name: String,
    }
);
impl Violation for MissingTypeFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeFunctionArgument { name } = self;
        format!("Missing type annotation for function argument `{name}`")
    }
}

define_violation!(
    pub struct MissingTypeArgs {
        pub name: String,
    }
);
impl Violation for MissingTypeArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeArgs { name } = self;
        format!("Missing type annotation for `*{name}`")
    }
}

define_violation!(
    pub struct MissingTypeKwargs {
        pub name: String,
    }
);
impl Violation for MissingTypeKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeKwargs { name } = self;
        format!("Missing type annotation for `**{name}`")
    }
}

define_violation!(
    pub struct MissingTypeSelf {
        pub name: String,
    }
);
impl Violation for MissingTypeSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeSelf { name } = self;
        format!("Missing type annotation for `{name}` in method")
    }
}

define_violation!(
    pub struct MissingTypeCls {
        pub name: String,
    }
);
impl Violation for MissingTypeCls {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingTypeCls { name } = self;
        format!("Missing type annotation for `{name}` in classmethod")
    }
}

define_violation!(
    pub struct MissingReturnTypePublicFunction {
        pub name: String,
    }
);
impl Violation for MissingReturnTypePublicFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePublicFunction { name } = self;
        format!("Missing return type annotation for public function `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypePrivateFunction {
        pub name: String,
    }
);
impl Violation for MissingReturnTypePrivateFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypePrivateFunction { name } = self;
        format!("Missing return type annotation for private function `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypeSpecialMethod {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for MissingReturnTypeSpecialMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeSpecialMethod { name } = self;
        format!("Missing return type annotation for special method `{name}`")
    }

    fn autofix_title(&self) -> String {
        "Add `None` return type".to_string()
    }
}

define_violation!(
    pub struct MissingReturnTypeStaticMethod {
        pub name: String,
    }
);
impl Violation for MissingReturnTypeStaticMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeStaticMethod { name } = self;
        format!("Missing return type annotation for staticmethod `{name}`")
    }
}

define_violation!(
    pub struct MissingReturnTypeClassMethod {
        pub name: String,
    }
);
impl Violation for MissingReturnTypeClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingReturnTypeClassMethod { name } = self;
        format!("Missing return type annotation for classmethod `{name}`")
    }
}

define_violation!(
    pub struct DynamicallyTypedExpression {
        pub name: String,
    }
);
impl Violation for DynamicallyTypedExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DynamicallyTypedExpression { name } = self;
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
    pub struct UseCapitalEnvironmentVariables {
        pub expected: String,
        pub original: String,
    }
);
impl AlwaysAutofixableViolation for UseCapitalEnvironmentVariables {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UseCapitalEnvironmentVariables { expected, original } = self;
        format!("Use capitalized environment variable `{expected}` instead of `{original}`")
    }

    fn autofix_title(&self) -> String {
        let UseCapitalEnvironmentVariables { expected, original } = self;
        format!("Replace `{original}` with `{expected}`")
    }
}

define_violation!(
    pub struct DuplicateIsinstanceCall {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for DuplicateIsinstanceCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateIsinstanceCall { name } = self;
        format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
    }

    fn autofix_title(&self) -> String {
        let DuplicateIsinstanceCall { name } = self;
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
    pub struct ReturnBoolConditionDirectly {
        pub cond: String,
    }
);
impl AlwaysAutofixableViolation for ReturnBoolConditionDirectly {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ReturnBoolConditionDirectly { cond } = self;
        format!("Return the condition `{cond}` directly")
    }

    fn autofix_title(&self) -> String {
        let ReturnBoolConditionDirectly { cond } = self;
        format!("Replace with `return {cond}`")
    }
}

define_violation!(
    pub struct UseContextlibSuppress {
        pub exception: String,
    }
);
impl Violation for UseContextlibSuppress {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UseContextlibSuppress { exception } = self;
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
    pub struct UseTernaryOperator {
        pub contents: String,
    }
);
impl Violation for UseTernaryOperator {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UseTernaryOperator { contents } = self;
        format!("Use ternary operator `{contents}` instead of if-else-block")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|UseTernaryOperator { contents }| format!("Replace if-else-block with `{contents}`"))
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
    pub struct ConvertLoopToAny {
        pub any: String,
    }
);
impl AlwaysAutofixableViolation for ConvertLoopToAny {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertLoopToAny { any } = self;
        format!("Use `{any}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ConvertLoopToAny { any } = self;
        format!("Replace with `{any}`")
    }
}

define_violation!(
    pub struct ConvertLoopToAll {
        pub all: String,
    }
);
impl AlwaysAutofixableViolation for ConvertLoopToAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertLoopToAll { all } = self;
        format!("Use `{all}` instead of `for` loop")
    }

    fn autofix_title(&self) -> String {
        let ConvertLoopToAll { all } = self;
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
    pub struct KeyInDict {
        pub key: String,
        pub dict: String,
    }
);
impl AlwaysAutofixableViolation for KeyInDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let KeyInDict { key, dict } = self;
        format!("Use `{key} in {dict}` instead of `{key} in {dict}.keys()`")
    }

    fn autofix_title(&self) -> String {
        let KeyInDict { key, dict } = self;
        format!("Convert to `{key} in {dict}`")
    }
}

define_violation!(
    pub struct NegateEqualOp {
        pub left: String,
        pub right: String,
    }
);
impl AlwaysAutofixableViolation for NegateEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateEqualOp { left, right } = self;
        format!("Use `{left} != {right}` instead of `not {left} == {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `!=` operator".to_string()
    }
}

define_violation!(
    pub struct NegateNotEqualOp {
        pub left: String,
        pub right: String,
    }
);
impl AlwaysAutofixableViolation for NegateNotEqualOp {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NegateNotEqualOp { left, right } = self;
        format!("Use `{left} == {right}` instead of `not {left} != {right}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `==` operator".to_string()
    }
}

define_violation!(
    pub struct DoubleNegation {
        pub expr: String,
    }
);
impl AlwaysAutofixableViolation for DoubleNegation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Use `{expr}` instead of `not (not {expr})`")
    }

    fn autofix_title(&self) -> String {
        let DoubleNegation { expr } = self;
        format!("Replace with `{expr}`")
    }
}

define_violation!(
    pub struct AAndNotA {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for AAndNotA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AAndNotA { name } = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

define_violation!(
    pub struct AOrNotA {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for AOrNotA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AOrNotA { name } = self;
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
        pub suggestion: Option<String>,
    }
);
impl Violation for YodaConditions {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let YodaConditions { suggestion } = self;
        if let Some(suggestion) = suggestion {
            format!("Yoda conditions are discouraged, use `{suggestion}` instead")
        } else {
            format!("Yoda conditions are discouraged")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let YodaConditions { suggestion, .. } = self;
        if suggestion.is_some() {
            Some(|YodaConditions { suggestion }| {
                let suggestion = suggestion.as_ref().unwrap();
                format!("Replace Yoda condition with `{suggestion}`")
            })
        } else {
            None
        }
    }
}

define_violation!(
    pub struct IfExprWithTrueFalse {
        pub expr: String,
    }
);
impl AlwaysAutofixableViolation for IfExprWithTrueFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTrueFalse { expr } = self;
        format!("Use `bool({expr})` instead of `True if {expr} else False`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTrueFalse { expr } = self;
        format!("Replace with `not {expr}")
    }
}

define_violation!(
    pub struct IfExprWithFalseTrue {
        pub expr: String,
    }
);
impl AlwaysAutofixableViolation for IfExprWithFalseTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithFalseTrue { expr } = self;
        format!("Use `not {expr}` instead of `False if {expr} else True`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithFalseTrue { expr } = self;
        format!("Replace with `bool({expr})")
    }
}

define_violation!(
    pub struct IfExprWithTwistedArms {
        pub expr_body: String,
        pub expr_else: String,
    }
);
impl AlwaysAutofixableViolation for IfExprWithTwistedArms {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!(
            "Use `{expr_else} if {expr_else} else {expr_body}` instead of `{expr_body} if not \
             {expr_else} else {expr_else}`"
        )
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!("Replace with `{expr_else} if {expr_else} else {expr_body}`")
    }
}

define_violation!(
    pub struct DictGetWithDefault {
        pub contents: String,
    }
);
impl AlwaysAutofixableViolation for DictGetWithDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DictGetWithDefault { contents } = self;
        format!("Use `{contents}` instead of an `if` block")
    }

    fn autofix_title(&self) -> String {
        let DictGetWithDefault { contents } = self;
        format!("Replace with `{contents}`")
    }
}

define_violation!(
    pub struct UnpackInsteadOfConcatenatingToCollectionLiteral {
        pub expr: String,
    }
);
impl Violation for UnpackInsteadOfConcatenatingToCollectionLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnpackInsteadOfConcatenatingToCollectionLiteral { expr } = self;
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
    pub struct TypeOfPrimitive {
        pub primitive: Primitive,
    }
);
impl AlwaysAutofixableViolation for TypeOfPrimitive {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeOfPrimitive { primitive } = self;
        format!("Use `{}` instead of `type(...)`", primitive.builtin())
    }

    fn autofix_title(&self) -> String {
        let TypeOfPrimitive { primitive } = self;
        format!("Replace `type(...)` with `{}`", primitive.builtin())
    }
}

define_violation!(
    pub struct UselessObjectInheritance {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for UselessObjectInheritance {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UselessObjectInheritance { name } = self;
        format!("Class `{name}` inherits from `object`")
    }

    fn autofix_title(&self) -> String {
        "Remove `object` inheritance".to_string()
    }
}

define_violation!(
    pub struct DeprecatedUnittestAlias {
        pub alias: String,
        pub target: String,
    }
);
impl AlwaysAutofixableViolation for DeprecatedUnittestAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedUnittestAlias { alias, target } = self;
        format!("`{alias}` is deprecated, use `{target}`")
    }

    fn autofix_title(&self) -> String {
        let DeprecatedUnittestAlias { alias, target } = self;
        format!("Replace `{target}` with `{alias}`")
    }
}

define_violation!(
    pub struct UsePEP585Annotation {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for UsePEP585Annotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UsePEP585Annotation { name } = self;
        format!(
            "Use `{}` instead of `{}` for type annotations",
            name.to_lowercase(),
            name,
        )
    }

    fn autofix_title(&self) -> String {
        let UsePEP585Annotation { name } = self;
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
    pub struct UnnecessaryFutureImport {
        pub names: Vec<String>,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryFutureImport { names } = self;
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
    pub struct ConvertTypedDictFunctionalToClass {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for ConvertTypedDictFunctionalToClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` to class syntax")
    }
}

define_violation!(
    pub struct ConvertNamedTupleFunctionalToClass {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for ConvertNamedTupleFunctionalToClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass { name } = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertNamedTupleFunctionalToClass { name } = self;
        format!("Convert `{name}` to class syntax")
    }
}

define_violation!(
    pub struct RedundantOpenModes {
        pub replacement: Option<String>,
    }
);
impl AlwaysAutofixableViolation for RedundantOpenModes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        match replacement {
            None => format!("Unnecessary open mode parameters"),
            Some(replacement) => {
                format!("Unnecessary open mode parameters, use \"{replacement}\"")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        match replacement {
            None => "Remove open mode parameters".to_string(),
            Some(replacement) => {
                format!("Replace with \"{replacement}\"")
            }
        }
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
    pub struct NativeLiterals {
        pub literal_type: LiteralType,
    }
);
impl AlwaysAutofixableViolation for NativeLiterals {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NativeLiterals { literal_type } = self;
        format!("Unnecessary call to `{literal_type}`")
    }

    fn autofix_title(&self) -> String {
        let NativeLiterals { literal_type } = self;
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
    pub struct OSErrorAlias {
        pub name: Option<String>,
    }
);
impl AlwaysAutofixableViolation for OSErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace aliased errors with `OSError`")
    }

    fn autofix_title(&self) -> String {
        let OSErrorAlias { name } = self;
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
    pub struct RewriteMockImport {
        pub reference_type: MockReference,
    }
);
impl AlwaysAutofixableViolation for RewriteMockImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`mock` is deprecated, use `unittest.mock`")
    }

    fn autofix_title(&self) -> String {
        let RewriteMockImport { reference_type } = self;
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
    pub struct UnnecessaryBuiltinImport {
        pub names: Vec<String>,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryBuiltinImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryBuiltinImport { names } = self;
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
    pub struct NoBlankLineBeforeFunction {
        pub num_lines: usize,
    }
);
impl AlwaysAutofixableViolation for NoBlankLineBeforeFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineBeforeFunction { num_lines } = self;
        format!("No blank lines allowed before function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before function docstring".to_string()
    }
}

define_violation!(
    pub struct NoBlankLineAfterFunction {
        pub num_lines: usize,
    }
);
impl AlwaysAutofixableViolation for NoBlankLineAfterFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLineAfterFunction { num_lines } = self;
        format!("No blank lines allowed after function docstring (found {num_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) after function docstring".to_string()
    }
}

define_violation!(
    pub struct OneBlankLineBeforeClass {
        pub lines: usize,
    }
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
    pub struct OneBlankLineAfterClass {
        pub lines: usize,
    }
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
    pub struct BlankLineAfterSummary {
        pub num_lines: usize,
    }
);
fn fmt_blank_line_after_summary_autofix_msg(_: &BlankLineAfterSummary) -> String {
    "Insert single blank line".to_string()
}
impl Violation for BlankLineAfterSummary {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSummary { num_lines } = self;
        if *num_lines == 0 {
            format!("1 blank line required between summary line and description")
        } else {
            format!(
                "1 blank line required between summary line and description (found {num_lines})"
            )
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let BlankLineAfterSummary { num_lines } = self;
        if *num_lines > 0 {
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
    pub struct NoBlankLineBeforeClass {
        pub lines: usize,
    }
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
    pub struct SectionNotOverIndented {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for SectionNotOverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionNotOverIndented { name } = self;
        format!("Section is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNotOverIndented { name } = self;
        format!("Remove over-indentation from \"{name}\"")
    }
}

define_violation!(
    pub struct SectionUnderlineNotOverIndented {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for SectionUnderlineNotOverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineNotOverIndented { name } = self;
        format!("Section underline is over-indented (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineNotOverIndented { name } = self;
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
    pub struct CapitalizeSectionName {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for CapitalizeSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CapitalizeSectionName { name } = self;
        format!("Section name should be properly capitalized (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let CapitalizeSectionName { name } = self;
        format!("Capitalize \"{name}\"")
    }
}

define_violation!(
    pub struct NewLineAfterSectionName {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for NewLineAfterSectionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NewLineAfterSectionName { name } = self;
        format!("Section name should end with a newline (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let NewLineAfterSectionName { name } = self;
        format!("Add newline after \"{name}\"")
    }
}

define_violation!(
    pub struct DashedUnderlineAfterSection {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for DashedUnderlineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DashedUnderlineAfterSection { name } = self;
        format!("Missing dashed underline after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let DashedUnderlineAfterSection { name } = self;
        format!("Add dashed line under \"{name}\"")
    }
}

define_violation!(
    pub struct SectionUnderlineAfterName {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for SectionUnderlineAfterName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineAfterName { name } = self;
        format!("Section underline should be in the line following the section's name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineAfterName { name } = self;
        format!("Add underline to \"{name}\"")
    }
}

define_violation!(
    pub struct SectionUnderlineMatchesSectionLength {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for SectionUnderlineMatchesSectionLength {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionUnderlineMatchesSectionLength { name } = self;
        format!("Section underline should match the length of its name (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionUnderlineMatchesSectionLength { name } = self;
        format!("Adjust underline length to match \"{name}\"")
    }
}

define_violation!(
    pub struct BlankLineAfterSection {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for BlankLineAfterSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSection { name } = self;
        format!("Missing blank line after section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

define_violation!(
    pub struct BlankLineBeforeSection {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for BlankLineBeforeSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineBeforeSection { name } = self;
        format!("Missing blank line before section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineBeforeSection { name } = self;
        format!("Add blank line before \"{name}\"")
    }
}

define_violation!(
    pub struct NoBlankLinesBetweenHeaderAndContent {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for NoBlankLinesBetweenHeaderAndContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoBlankLinesBetweenHeaderAndContent { name } = self;
        format!("No blank lines allowed between a section header and its content (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s)".to_string()
    }
}

define_violation!(
    pub struct BlankLineAfterLastSection {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for BlankLineAfterLastSection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterLastSection { name } = self;
        format!("Missing blank line after last section (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let BlankLineAfterLastSection { name } = self;
        format!("Add blank line after \"{name}\"")
    }
}

define_violation!(
    pub struct NonEmptySection {
        pub name: String,
    }
);
impl Violation for NonEmptySection {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonEmptySection { name } = self;
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
    pub struct SectionNameEndsInColon {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for SectionNameEndsInColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SectionNameEndsInColon { name } = self;
        format!("Section name should end with a colon (\"{name}\")")
    }

    fn autofix_title(&self) -> String {
        let SectionNameEndsInColon { name } = self;
        format!("Add colon to \"{name}\"")
    }
}

define_violation!(
    pub struct DocumentAllArguments {
        pub names: Vec<String>,
    }
);
impl Violation for DocumentAllArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DocumentAllArguments { names } = self;
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
    pub struct InvalidClassName {
        pub name: String,
    }
);
impl Violation for InvalidClassName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidClassName { name } = self;
        format!("Class name `{name}` should use CapWords convention ")
    }
}

define_violation!(
    pub struct InvalidFunctionName {
        pub name: String,
    }
);
impl Violation for InvalidFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidFunctionName { name } = self;
        format!("Function name `{name}` should be lowercase")
    }
}

define_violation!(
    pub struct InvalidArgumentName {
        pub name: String,
    }
);
impl Violation for InvalidArgumentName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidArgumentName { name } = self;
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
    pub struct NonLowercaseVariableInFunction {
        pub name: String,
    }
);
impl Violation for NonLowercaseVariableInFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonLowercaseVariableInFunction { name } = self;
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
    pub struct ConstantImportedAsNonConstant {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for ConstantImportedAsNonConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConstantImportedAsNonConstant { name, asname } = self;
        format!("Constant `{name}` imported as non-constant `{asname}`")
    }
}

define_violation!(
    pub struct LowercaseImportedAsNonLowercase {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for LowercaseImportedAsNonLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LowercaseImportedAsNonLowercase { name, asname } = self;
        format!("Lowercase `{name}` imported as non-lowercase `{asname}`")
    }
}

define_violation!(
    pub struct CamelcaseImportedAsLowercase {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for CamelcaseImportedAsLowercase {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsLowercase { name, asname } = self;
        format!("Camelcase `{name}` imported as lowercase `{asname}`")
    }
}

define_violation!(
    pub struct CamelcaseImportedAsConstant {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for CamelcaseImportedAsConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsConstant { name, asname } = self;
        format!("Camelcase `{name}` imported as constant `{asname}`")
    }
}

define_violation!(
    pub struct MixedCaseVariableInClassScope {
        pub name: String,
    }
);
impl Violation for MixedCaseVariableInClassScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInClassScope { name } = self;
        format!("Variable `{name}` in class scope should not be mixedCase")
    }
}

define_violation!(
    pub struct MixedCaseVariableInGlobalScope {
        pub name: String,
    }
);
impl Violation for MixedCaseVariableInGlobalScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInGlobalScope { name } = self;
        format!("Variable `{name}` in global scope should not be mixedCase")
    }
}

define_violation!(
    pub struct CamelcaseImportedAsAcronym {
        pub name: String,
        pub asname: String,
    }
);
impl Violation for CamelcaseImportedAsAcronym {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CamelcaseImportedAsAcronym { name, asname } = self;
        format!("Camelcase `{name}` imported as acronym `{asname}`")
    }
}

define_violation!(
    pub struct ErrorSuffixOnExceptionName {
        pub name: String,
    }
);
impl Violation for ErrorSuffixOnExceptionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ErrorSuffixOnExceptionName { name } = self;
        format!("Exception name `{name}` should be named with an Error suffix")
    }
}

// flake8-bandit

define_violation!(
    pub struct Jinja2AutoescapeFalse {
        pub value: bool,
    }
);
impl Violation for Jinja2AutoescapeFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Jinja2AutoescapeFalse { value } = self;
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
    pub struct BadFilePermissions {
        pub mask: u16,
    }
);
impl Violation for BadFilePermissions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BadFilePermissions { mask } = self;
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
    pub struct HardcodedPasswordString {
        pub string: String,
    }
);
impl Violation for HardcodedPasswordString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordString { string } = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

define_violation!(
    pub struct HardcodedPasswordFuncArg {
        pub string: String,
    }
);
impl Violation for HardcodedPasswordFuncArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordFuncArg { string } = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

define_violation!(
    pub struct HardcodedPasswordDefault {
        pub string: String,
    }
);
impl Violation for HardcodedPasswordDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordDefault { string } = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

define_violation!(
    pub struct HardcodedTempFile {
        pub string: String,
    }
);
impl Violation for HardcodedTempFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedTempFile { string } = self;
        format!(
            "Probable insecure usage of temporary file or directory: \"{}\"",
            string.escape_debug()
        )
    }
}

define_violation!(
    pub struct RequestWithoutTimeout {
        pub timeout: Option<String>,
    }
);
impl Violation for RequestWithoutTimeout {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithoutTimeout { timeout } = self;
        match timeout {
            Some(value) => {
                format!("Probable use of requests call with timeout set to `{value}`")
            }
            None => format!("Probable use of requests call without timeout"),
        }
    }
}

define_violation!(
    pub struct HashlibInsecureHashFunction {
        pub string: String,
    }
);
impl Violation for HashlibInsecureHashFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HashlibInsecureHashFunction { string } = self;
        format!(
            "Probable use of insecure hash functions in `hashlib`: \"{}\"",
            string.escape_debug()
        )
    }
}

define_violation!(
    pub struct RequestWithNoCertValidation {
        pub string: String,
    }
);
impl Violation for RequestWithNoCertValidation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RequestWithNoCertValidation { string } = self;
        format!(
            "Probable use of `{string}` call with `verify=False` disabling SSL certificate checks"
        )
    }
}

define_violation!(
    pub struct UnsafeYAMLLoad {
        pub loader: Option<String>,
    }
);
impl Violation for UnsafeYAMLLoad {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsafeYAMLLoad { loader } = self;
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
    pub struct UnusedFunctionArgument {
        pub name: String,
    }
);
impl Violation for UnusedFunctionArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedFunctionArgument { name } = self;
        format!("Unused function argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedMethodArgument {
        pub name: String,
    }
);
impl Violation for UnusedMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedMethodArgument { name } = self;
        format!("Unused method argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedClassMethodArgument {
        pub name: String,
    }
);
impl Violation for UnusedClassMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedClassMethodArgument { name } = self;
        format!("Unused class method argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedStaticMethodArgument {
        pub name: String,
    }
);
impl Violation for UnusedStaticMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedStaticMethodArgument { name } = self;
        format!("Unused static method argument: `{name}`")
    }
}

define_violation!(
    pub struct UnusedLambdaArgument {
        pub name: String,
    }
);
impl Violation for UnusedLambdaArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLambdaArgument { name } = self;
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
    pub struct KeywordArgumentBeforeStarArgument {
        pub name: String,
    }
);
impl Violation for KeywordArgumentBeforeStarArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let KeywordArgumentBeforeStarArgument { name } = self;
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
    pub struct UnusedNOQA {
        pub codes: Option<UnusedCodes>,
    }
);
impl AlwaysAutofixableViolation for UnusedNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedNOQA { codes } = self;
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
