use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
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
#[derive(ViolationMetadata)]
pub(crate) struct MapIntVersionParsing;

impl Violation for MapIntVersionParsing {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__version__` may contain non-integral-like elements".to_string()
    }
}

/// RUF048
pub(crate) fn map_int_version_parsing(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();

    let Some((first, second)) = map_call_with_two_arguments(semantic, call) else {
        return;
    };

    if is_dunder_version_split_dot(second) && semantic.match_builtin_expr(first, "int") {
        checker.report_diagnostic(Diagnostic::new(MapIntVersionParsing, call.range()));
    }
}

fn map_call_with_two_arguments<'a>(
    semantic: &SemanticModel,
    call: &'a ast::ExprCall,
) -> Option<(&'a ast::Expr, &'a ast::Expr)> {
    let ast::ExprCall {
        func,
        arguments:
            ast::Arguments {
                args,
                keywords,
                range: _,
            },
        range: _,
    } = call;

    if !keywords.is_empty() {
        return None;
    }

    let [first, second] = &**args else {
        return None;
    };

    if !semantic.match_builtin_expr(func, "map") {
        return None;
    }

    Some((first, second))
}

/// Whether `expr` has the form `__version__.split(".")` or `something.__version__.split(".")`.
fn is_dunder_version_split_dot(expr: &ast::Expr) -> bool {
    let ast::Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return false;
    };

    if arguments.len() != 1 {
        return false;
    }

    let Some(ast::Expr::StringLiteral(ast::ExprStringLiteral { value, range: _ })) =
        arguments.find_argument_value("sep", 0)
    else {
        return false;
    };

    if value.to_str() != "." {
        return false;
    }

    is_dunder_version_split(func)
}

fn is_dunder_version_split(func: &ast::Expr) -> bool {
    // foo.__version__.split(".")
    // ---- value ---- ^^^^^ attr
    let ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func else {
        return false;
    };
    if attr != "split" {
        return false;
    }
    is_dunder_version(value)
}

fn is_dunder_version(expr: &ast::Expr) -> bool {
    if let ast::Expr::Name(ast::ExprName { id, .. }) = expr {
        return id == "__version__";
    }

    // foo.__version__.split(".")
    //     ^^^^^^^^^^^ attr
    let ast::Expr::Attribute(ast::ExprAttribute { attr, .. }) = expr else {
        return false;
    };

    attr == "__version__"
}
