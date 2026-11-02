use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Arguments, Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `re.compile()` calls whose result is used immediately and only
/// once, e.g. `re.compile(pattern).match(string)`.
///
/// ## Why is this bad?
/// Compiling a pattern inline and immediately calling a method on it is
/// equivalent to calling the corresponding top-level `re` function, such as
/// `re.match` or `re.sub`. The top-level function is shorter.
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
/// ## References
/// - [Python documentation: `re.compile`](https://docs.python.org/3/library/re.html#re.compile)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.17.2", category = Category::Complexity)]
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

/// `unnecessary-regular-expression-compile`
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

    let Expr::Call(compile) = value.as_ref() else {
        return;
    };
    if !is_re_compile(&compile.func, semantic) {
        return;
    }

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
/// `split` are a positional prefix of the top-level functions', so any explicit argument shape
/// reduces; unpacked (`*`/`**`) arguments have an unknown shape and never reduce.
fn reducible_re_method(attr: &str, arguments: &Arguments) -> Option<&'static str> {
    if arguments.args.iter().any(Expr::is_starred_expr)
        || arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none())
    {
        return None;
    }
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
