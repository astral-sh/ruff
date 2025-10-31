use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks that all Django URL route definitions using `django.urls.path()`
/// do not start with a leading slash.
///
/// ## Why is this bad?
/// Django's URL patterns should not start with a leading slash. When using
/// `include()` or when patterns are combined, leading slashes can cause
/// issues with URL resolution. The Django documentation recommends that
/// URL patterns should not have leading slashes, as they are not necessary
/// and can lead to unexpected behavior.
///
/// ## Example
/// ```python
/// from django.urls import path
/// from . import views
///
/// urlpatterns = [
///     path("/help/", views.help_view),  # Leading slash
///     path("/about/", views.about_view),  # Leading slash
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
pub(crate) struct DjangoURLPathWithLeadingSlash {
    url_pattern: String,
}

impl AlwaysFixableViolation for DjangoURLPathWithLeadingSlash {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DjangoURLPathWithLeadingSlash { url_pattern } = self;
        format!("URL route `{url_pattern}` has an unnecessary leading slash")
    }

    fn fix_title(&self) -> String {
        "Remove leading slash".to_string()
    }
}

/// DJ101
pub(crate) fn url_path_with_leading_slash(checker: &Checker, call: &ast::ExprCall) {
    // Check if this is a call to django.urls.path or any additional configured path functions
    let is_path_function = checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            let segments = qualified_name.segments();

            // Check if it's the default django.urls.path
            if matches!(segments, ["django", "urls", "path"]) {
                return true;
            }

            // Check if it matches any additional configured path functions
            let qualified_name_str = segments.join(".");
            checker
                .settings()
                .flake8_django
                .additional_path_functions
                .iter()
                .any(|path| path == &qualified_name_str)
        });

    if !is_path_function {
        return;
    }

    // Get the first argument (the route pattern)
    let Some(route_arg) = call.arguments.args.first() else {
        return;
    };

    // Check if it's a string literal
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = route_arg {
        let route = value.to_str();

        // Skip empty strings and root path "/"
        if route.is_empty() || route == "/" {
            return;
        }

        // Check if route starts with a leading slash
        if route.starts_with('/') {
            // Report diagnostic for routes with leading slash
            let mut diagnostic = checker.report_diagnostic(
                DjangoURLPathWithLeadingSlash {
                    url_pattern: route.to_string(),
                },
                route_arg.range(),
            );

            // Determine the quote style to find the insertion point for removal
            let string_content = checker.locator().slice(route_arg.range());
            let quote_len =
                if string_content.starts_with("'''") || string_content.starts_with("\"\"\"") {
                    3
                } else if string_content.starts_with('\'') || string_content.starts_with('"') {
                    1
                } else {
                    return; // Invalid string format
                };

            // Remove the leading slash (after the opening quote(s))
            let removal_start = route_arg.range().start() + TextSize::new(quote_len);
            let removal_end = removal_start + TextSize::new(1); // Remove one character (the slash)

            diagnostic.set_fix(Fix::safe_edit(Edit::deletion(removal_start, removal_end)));
        }
    }
}
