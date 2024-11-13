use std::ops::Deref;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, ExprName};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls of the form `tuple(map(int, __version__.split(".")))`.
///
/// ## Why is this bad?
/// `__version__` does not always contain integral-like elements.
///
/// ```python
/// import matplotlib  # 3.9.1.post-1
///
/// # ValueError: invalid literal for int() with base 10: 'post1'
/// tuple(map(int, matplotlib.__version__.split(".")))
/// ```
///
/// See also [PEP 440].
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
/// [PEP 440]: https://peps.python.org/pep-0440/
#[violation]
pub struct TupleMapIntVersionParsing;

impl Violation for TupleMapIntVersionParsing {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__version__` may contain non-integral-like elements".to_string()
    }
}

/// RUF048
pub(crate) fn tuple_map_int_version_parsing(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    let Some(Expr::Call(child_call)) = tuple_like_call_with_single_argument(semantic, call) else {
        return;
    };

    let Some((first, second)) = map_call_with_two_arguments(semantic, child_call) else {
        return;
    };

    if !semantic.match_builtin_expr(first, "int") || !is_dunder_version_split_dot(second) {
        return;
    }

    let diagnostic = Diagnostic::new(TupleMapIntVersionParsing, call.range());

    checker.diagnostics.push(diagnostic);
}

fn tuple_like_call_with_single_argument<'a>(
    semantic: &SemanticModel,
    call: &'a ExprCall,
) -> Option<&'a Expr> {
    let Some((func, positionals)) = func_and_positionals(call) else {
        return None;
    };

    let func_is = |symbol: &str| semantic.match_builtin_expr(func, symbol);

    if !func_is("tuple") && !func_is("list") || positionals.len() != 1 {
        return None;
    };

    positionals.first()
}

fn map_call_with_two_arguments<'a>(
    semantic: &SemanticModel,
    call: &'a ExprCall,
) -> Option<(&'a Expr, &'a Expr)> {
    let Some((func, positionals)) = func_and_positionals(call) else {
        return None;
    };

    if !semantic.match_builtin_expr(func, "map") || positionals.len() != 2 {
        return None;
    };

    Some((positionals.first().unwrap(), positionals.last().unwrap()))
}

/// Whether `expr` has the form `__version__.split(".")` or `something.__version__.split(".")`.
fn is_dunder_version_split_dot(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    let Some((func, arguments)) = func_and_positionals(call) else {
        return false;
    };
    let argument = if arguments.len() == 1 {
        arguments.first().unwrap()
    } else {
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
    if attr.as_str() != "split" {
        return false;
    }

    is_dunder_version(value)
}

fn is_dunder_version(expr: &Expr) -> bool {
    if let Expr::Name(ExprName { id, .. }) = expr {
        return id.as_str() == "__version__";
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

    Some((func.deref(), arguments.args.deref()))
}
