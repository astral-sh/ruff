#![allow(clippy::useless_format)]
use std::fmt;

use itertools::Itertools;
use ruff_macros::derive_message_formats;

use serde::{Deserialize, Serialize};

use crate::define_violation;
use crate::rules::flake8_debugger::types::DebuggerUsingType;

use crate::violation::{AlwaysAutofixableViolation, AutofixKind, Availability, Violation};

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
