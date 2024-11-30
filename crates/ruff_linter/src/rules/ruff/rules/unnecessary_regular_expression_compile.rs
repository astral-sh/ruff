use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for compiled regex patterns that are directly used.
///
/// ## Why is this bad?
/// The compiled regular expression object returned by `re.compile` can
/// be used to make programs more efficient when the expression is reused
/// several times.
///
/// If the object is not stored, then the expressio nis functionally equivalent
/// to the functions in the `re` module, which are more concise, such as `re.match`
/// and `re.split`.
///
/// ## Example
/// ```python
/// import re
///
/// result = re.compile(pattern).match(string)
/// ```
///
/// Use instead:
///
/// ```python
/// import re
///
/// result = re.match(pattern, string)
/// ```
///
/// or assign the compiled regular expression pattern to a variable:
///
/// ```python
/// import re
///
/// PATTERN = re.compile(pattern)
/// result = PATTERN.match(string)
/// ```
///
/// ## References
/// - [Python documentation: `re.compile`](https://docs.python.org/3/library/re.html#re.compile)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryRegularExpressionCompile {
    re_kind: String,
}

impl Violation for UnnecessaryRegularExpressionCompile {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryRegularExpressionCompile { re_kind } = &self;
        format!("Use of `re.compile().{re_kind}()`")
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryRegularExpressionCompile { re_kind } = &self;
        Some(format!("Replace with `re.{re_kind}()`"))
    }
}

/// RUF056
pub(crate) fn unnecessary_regular_expression_compile(checker: &mut Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::RE) {
        return;
    }

    if call.arguments.is_empty() {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if !matches!(
        attr.as_str(),
        "split" | "match" | "finditer" | "search" | "subn" | "sub" | "fullmatch"
    ) {
        return;
    }

    let Expr::Call(ExprCall { func, .. }) = value.as_ref() else {
        return;
    };

    if checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["re", "compile"]))
    {
        let diagnostic = Diagnostic::new(
            UnnecessaryRegularExpressionCompile {
                re_kind: attr.to_string(),
            },
            call.range(),
        );
        checker.diagnostics.push(diagnostic);
    }
}
