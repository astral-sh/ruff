use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that all Django URL route definitions using `django.urls.path()`
/// end with a trailing slash.
///
/// ## Why is this bad?
/// Django's convention is to use trailing slashes in URL patterns. This is
/// enforced by the `APPEND_SLASH` setting (enabled by default), which
/// redirects requests without trailing slashes to URLs with them. Omitting
/// the trailing slash can lead to unnecessary redirects and inconsistent URL
/// patterns throughout your application.
///
/// ## Example
/// ```python
/// from django.urls import path
/// from . import views
///
/// urlpatterns = [
///     path("help", views.help_view),  # Missing trailing slash
///     path("about", views.about_view),  # Missing trailing slash
/// ]
/// ```
///
/// Use instead:
/// ```python
/// from django.urls import path
/// from . import views
///
/// urlpatterns = [
///     path("help/", views.help_view),
///     path("about/", views.about_view),
/// ]
/// ```
///
/// ## References
/// - [Django documentation: URL dispatcher](https://docs.djangoproject.com/en/stable/topics/http/urls/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.14.1")]
pub(crate) struct DjangoURLPathWithoutTrailingSlash {
    url_pattern: String,
}

impl Violation for DjangoURLPathWithoutTrailingSlash {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DjangoURLPathWithoutTrailingSlash { url_pattern } = self;
        format!("URL route `{url_pattern}` is missing a trailing slash")
    }
}

/// DJ014
pub(crate) fn url_path_without_trailing_slash(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    // Check if this is a call to django.urls.path
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["django", "urls", "path"])
        })
    {
        return;
    }

    // Get the first argument (the route pattern)
    let Some(route_arg) = call.arguments.args.first() else {
        return;
    };

    // Check if it's a string literal
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = route_arg {
        let route = value.to_str();

        // Skip empty strings
        if route.is_empty() {
            return;
        }

        // Skip route parameters (routes with angle brackets like "<int:id>")
        // These are often at the end and shouldn't require trailing slashes
        // Also skip routes that are just "/" or already end with "/"
        if route == "/" || route.ends_with('/') {
            return;
        }

        // Report diagnostic for routes without trailing slash
        checker.report_diagnostic(
            DjangoURLPathWithoutTrailingSlash {
                url_pattern: route.to_string(),
            },
            route_arg.range(),
        );
    }
}
