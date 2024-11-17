use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ExprName};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls of the form `map(int, __version__.split("."))`.
///
/// ## Why is this bad?
/// `__version__` does not always contain integral-like elements.
///
/// ```python
/// import matplotlib  # `__version__ == "3.9.1.post-1"` in our environment
///
/// # ValueError: invalid literal for int() with base 10: 'post1'
/// tuple(map(int, matplotlib.__version__.split(".")))
/// ```
///
/// See also [*Version specifiers* | Packaging spec][version-specifier].
///
/// ## Example
/// ```python
/// tuple(map(int, matplotlib.__version__.split(".")))
/// ```
///
/// Use instead:
/// ```python
/// import packaging.version as version
///
/// version.parse(matplotlib.__version__)
/// ```
///
/// [version-specifier]: https://packaging.python.org/en/latest/specifications/version-specifiers/#version-specifiers
#[violation]
pub struct MapIntVersionParsing;

impl Violation for MapIntVersionParsing {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__version__` may contain non-integral-like elements".to_string()
    }
}

/// RUF048
pub(crate) fn map_int_version_parsing(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    let Some((first, second)) = map_call_with_two_arguments(semantic, call) else {
        return;
    };

    if !semantic.match_builtin_expr(first, "int") || !is_dunder_version_split_dot(second) {
        return;
    }

    let diagnostic = Diagnostic::new(MapIntVersionParsing, call.range());

    checker.diagnostics.push(diagnostic);
}

fn map_call_with_two_arguments<'a>(
    semantic: &SemanticModel,
    call: &'a ExprCall,
) -> Option<(&'a Expr, &'a Expr)> {
    let (func, positionals) = func_and_positionals(call)?;

    if !semantic.match_builtin_expr(func, "map") {
        return None;
    };

    let [first, second] = positionals else {
        return None;
    };

    Some((first, second))
}

/// Whether `expr` has the form `__version__.split(".")` or `something.__version__.split(".")`.
fn is_dunder_version_split_dot(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    let Some((func, arguments)) = func_and_positionals(call) else {
        return false;
    };

    let [argument] = arguments else {
        return false;
    };

    is_dunder_version_split(func) && is_single_dot_string(argument)
}

fn is_dunder_version_split(func: &Expr) -> bool {
    // foo.__version__.split(".")
    // ---- value ---- ^^^^^ attr
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = func else {
        return false;
    };
    if attr != "split" {
        return false;
    }

    is_dunder_version(value)
}

fn is_dunder_version(expr: &Expr) -> bool {
    if let Expr::Name(ExprName { id, .. }) = expr {
        return id == "__version__";
    }

    // foo.__version__.split(".")
    //     ^^^^^^^^^^^ attr
    let Expr::Attribute(ExprAttribute { attr, .. }) = expr else {
        return false;
    };

    attr == "__version__"
}

fn is_single_dot_string(argument: &Expr) -> bool {
    let Some(string) = argument.as_string_literal_expr() else {
        return false;
    };

    let mut string_chars = string.value.chars();
    let (first, second) = (string_chars.next(), string_chars.next());

    matches!((first, second), (Some('.'), None))
}

/// Extracts the function being called and its positional arguments.
/// Returns `None` if there are keyword arguments.
fn func_and_positionals(expr: &ExprCall) -> Option<(&Expr, &[Expr])> {
    let func = &expr.func;
    let arguments = &expr.arguments;

    if !arguments.keywords.is_empty() {
        return None;
    }

    Some((func.as_ref(), arguments.args.as_ref()))
}
