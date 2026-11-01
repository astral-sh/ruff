use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall, ExprKind};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "0.10.0", category = Category::Suspicious)]
pub(crate) struct MapIntVersionParsing;

impl Violation for MapIntVersionParsing {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__version__` may contain non-integral-like elements".to_string()
    }
}

/// RUF048
pub(crate) fn map_int_version_parsing(checker: &Checker, call: ExprCall<'_>) {
    let semantic = checker.semantic();

    let Some((first, second)) = map_call_with_two_arguments(semantic, call) else {
        return;
    };

    if is_dunder_version_split_dot(second) && semantic.match_builtin_expr(first, "int") {
        checker.report_diagnostic(MapIntVersionParsing, call.range());
    }
}

fn map_call_with_two_arguments<'a>(
    semantic: &SemanticModel,
    call: ExprCall<'a>,
) -> Option<(Expr<'a>, Expr<'a>)> {
    let func = call.func();
    let arguments = call.arguments();
    let args = arguments.args();
    let keywords = arguments.keywords();

    if !keywords.is_empty() {
        return None;
    }

    let [first, second] = args.as_array()?;

    if !semantic.match_builtin_expr(func, "map") {
        return None;
    }

    Some((first, second))
}

/// Whether `expr` has the form `__version__.split(".")` or `something.__version__.split(".")`.
fn is_dunder_version_split_dot(expr: Expr<'_>) -> bool {
    let ExprKind::Call(call_node) = expr.classify() else {
        return false;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();

    if arguments.len() != 1 {
        return false;
    }

    let Some(ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields {
        value,
        range: _,
        node: _,
    })) = arguments.find_argument_value("sep", 0).map(Expr::fields)
    else {
        return false;
    };

    if value.to_str() != "." {
        return false;
    }

    is_dunder_version_split(func)
}

fn is_dunder_version_split(func: Expr<'_>) -> bool {
    // foo.__version__.split(".")
    // ---- value ---- ^^^^^ attr
    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let attr = attribute.attr();
    let value = attribute.value();
    if attr != "split" {
        return false;
    }
    is_dunder_version(value)
}

fn is_dunder_version(expr: Expr<'_>) -> bool {
    if let ExprKind::Name(name) = expr.classify() {
        let id = name.id();
        return id == "__version__";
    }

    // foo.__version__.split(".")
    //     ^^^^^^^^^^^ attr
    let ExprKind::Attribute(attribute) = expr.classify() else {
        return false;
    };
    let attr = attribute.attr();

    attr == "__version__"
}
