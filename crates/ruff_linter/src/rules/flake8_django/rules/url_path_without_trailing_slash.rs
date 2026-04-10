use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, StringFlags};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_django::helpers::is_path_function;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[violation_metadata(preview_since = "0.15.11")]
pub(crate) struct DjangoUrlPathWithoutTrailingSlash {
    url_pattern: String,
}

impl AlwaysFixableViolation for DjangoUrlPathWithoutTrailingSlash {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DjangoUrlPathWithoutTrailingSlash { url_pattern } = self;
        format!("URL route `{url_pattern}` is missing a trailing slash")
    }

    fn fix_title(&self) -> String {
        "Add a trailing slash".to_string()
    }
}

/// DJ100
pub(crate) fn url_path_without_trailing_slash(checker: &Checker, call: &ast::ExprCall) {
    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    if !is_path_function(&qualified_name, checker) {
        return;
    }

    let Some(route_arg) = call.arguments.find_positional(0) else {
        return;
    };

    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = route_arg else {
        return;
    };

    let route = value.to_str();

    if route.is_empty() || route == "/" || route.ends_with('/') {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        DjangoUrlPathWithoutTrailingSlash {
            url_pattern: route.to_string(),
        },
        route_arg.range(),
    );

    let Some(last_literal) = value.iter().last() else {
        return;
    };
    let insertion_point = last_literal.end() - last_literal.flags.closer_len();

    diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
        "/".to_string(),
        insertion_point,
    )));
}
