#![allow(clippy::useless_format)]
use itertools::Itertools;
use ruff_macros::derive_message_formats;

use serde::{Deserialize, Serialize};

use crate::define_violation;
use crate::rules::flake8_debugger::types::DebuggerUsingType;

use crate::violation::{AlwaysAutofixableViolation, Violation};

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
