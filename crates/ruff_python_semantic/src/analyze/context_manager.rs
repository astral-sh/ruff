use ruff_python_ast::{self as ast, Expr};

use crate::SemanticModel;

/// Returns `true` if any of the `with` statement's context managers may swallow an exception
/// raised in its body, letting execution resume after the `with` statement.
///
/// A context manager suppresses an exception by returning a truthy value from `__exit__`. We
/// can't know that in general, since `__exit__` is usually defined in another module, so we
/// recognize the handful of context managers that are widely used for exactly that purpose:
///
/// ```python
/// def func():
///     with pytest.raises(ValueError):
///         raise ValueError("boom")
///     # `pytest.raises` swallowed the exception, so we get here.
/// ```
pub fn may_suppress_exceptions(with: &ast::StmtWith, semantic: &SemanticModel) -> bool {
    with.items.iter().any(|item| {
        let Expr::Call(ast::ExprCall { func, .. }) = &item.context_expr else {
            return false;
        };

        if semantic
            .resolve_qualified_name(func)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["contextlib", "suppress"] | ["pytest", "raises"]
                )
            })
        {
            return true;
        }

        // `unittest`'s `assertRaises` helpers are called on a `TestCase` instance (typically
        // `self`), which we can't resolve, so match on the method name alone.
        matches!(
            func.as_ref(),
            Expr::Attribute(ast::ExprAttribute { attr, .. })
                if matches!(
                    attr.as_str(),
                    "assertRaises" | "assertRaisesRegex" | "assertRaisesRegexp" | "failUnlessRaises"
                )
        )
    })
}
