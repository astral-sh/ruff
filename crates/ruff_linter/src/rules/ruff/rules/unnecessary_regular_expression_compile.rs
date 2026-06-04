use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{AnyNodeRef, Arguments, Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::analyze::typing::find_binding_value;
use ruff_python_semantic::{Binding, Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `re.compile()` calls whose result is used exactly once, either
/// inline or through a single-use local variable.
///
/// ## Why is this bad?
/// The compiled pattern object returned by `re.compile()` only pays off when it
/// is reused: compiling once and matching many times avoids recompiling the
/// pattern on every call. If the pattern is used only once, the `re` module's
/// top-level functions such as `re.match` and `re.sub` are equivalent, shorter,
/// and avoid the intermediate object.
///
/// ## Example
/// ```python
/// import re
///
/// re.compile(pattern).match(string)
/// ```
///
/// Use instead:
/// ```python
/// import re
///
/// re.match(pattern, string)
/// ```
///
/// If the pattern is genuinely reused, store it instead so the intent is clear:
/// ```python
/// import re
///
/// PATTERN = re.compile(pattern)
/// PATTERN.match(first)
/// PATTERN.match(second)
/// ```
///
/// ## Known problems
/// To stay sound without whole-program analysis, the bound form is only flagged
/// for local variables in a function that are assigned once and read exactly
/// once. A module- or class-level compiled pattern is never flagged, as it may
/// be imported and reused from another module.
///
/// ## References
/// - [Python documentation: `re.compile`](https://docs.python.org/3/library/re.html#re.compile)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct UnnecessaryRegularExpressionCompile {
    re_func: &'static str,
}

impl Violation for UnnecessaryRegularExpressionCompile {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Compiled regular expression is used only once".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryRegularExpressionCompile { re_func } = self;
        Some(format!(
            "Replace with `re.{re_func}()` or store the compiled pattern"
        ))
    }
}

/// RUF076: inline form, e.g. `re.compile(pattern).match(string)`.
pub(crate) fn unnecessary_regular_expression_compile(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();
    if !semantic.seen_module(Modules::RE) {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };
    let Some(re_func) = reducible_re_method(attr.as_str(), &call.arguments) else {
        return;
    };

    let Expr::Call(ExprCall { func, .. }) = value.as_ref() else {
        return;
    };
    if !is_re_compile(func, semantic) {
        return;
    }

    checker.report_diagnostic(
        UnnecessaryRegularExpressionCompile { re_func },
        call.range(),
    );
}

/// RUF076: bound form, e.g. `pattern = re.compile(...)` read exactly once as `pattern.match(...)`.
pub(crate) fn unnecessary_regular_expression_compile_binding(checker: &Checker, binding: &Binding) {
    let semantic = checker.semantic();
    if !semantic.seen_module(Modules::RE) {
        return;
    }

    // Function-local only: a module- or class-level pattern may be imported and reused elsewhere.
    if !binding.kind.is_assignment() || !semantic.scopes[binding.scope].kind.is_function() {
        return;
    }

    let Some(Expr::Call(ExprCall { func, .. })) = find_binding_value(binding, semantic) else {
        return;
    };
    if !is_re_compile(func, semantic) {
        return;
    }

    // More than one read means the pattern is genuinely reused.
    let mut references = binding.references();
    let (Some(reference_id), None) = (references.next(), references.next()) else {
        return;
    };
    let reference = semantic.reference(reference_id);
    if reference.scope_id() != binding.scope {
        return;
    }

    // The single use must be a `<name>.<method>(...)` call, not e.g. returning or passing the pattern.
    let Some(node_id) = reference.expression_id() else {
        return;
    };
    let Some(attribute_id) = semantic.parent_expression_id(node_id) else {
        return;
    };
    let Some(attribute) = semantic.expression(attribute_id) else {
        return;
    };
    let Expr::Attribute(ExprAttribute { attr, .. }) = attribute else {
        return;
    };
    let Some(Expr::Call(call)) = semantic.parent_expression(attribute_id) else {
        return;
    };
    // Guard against the attribute being an argument rather than the callee, e.g. `f(pattern.match)`.
    if !AnyNodeRef::from(call.func.as_ref()).ptr_eq(AnyNodeRef::from(attribute)) {
        return;
    }
    let Some(re_func) = reducible_re_method(attr.as_str(), &call.arguments) else {
        return;
    };

    checker.report_diagnostic(
        UnnecessaryRegularExpressionCompile { re_func },
        call.range(),
    );
}

/// If `pattern.<attr>(<arguments>)` is equivalent to a call to the top-level `re.<attr>(...)`,
/// returns that function's name.
///
/// `search`, `match`, `fullmatch`, `findall`, and `finditer` accept optional `pos`/`endpos`
/// arguments that the top-level functions do not (their trailing argument is `flags`), so they only
/// reduce when called with the single `string` argument. The parameters of `sub`, `subn`, and
/// `split` are a positional prefix of the top-level functions', so any argument shape reduces.
fn reducible_re_method(attr: &str, arguments: &Arguments) -> Option<&'static str> {
    let only_string_argument = arguments.args.len() == 1 && arguments.keywords.is_empty();
    Some(match attr {
        "search" if only_string_argument => "search",
        "match" if only_string_argument => "match",
        "fullmatch" if only_string_argument => "fullmatch",
        "findall" if only_string_argument => "findall",
        "finditer" if only_string_argument => "finditer",
        "sub" => "sub",
        "subn" => "subn",
        "split" => "split",
        _ => return None,
    })
}

/// Returns `true` if `func` resolves to `re.compile`.
fn is_re_compile(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["re", "compile"]))
}
