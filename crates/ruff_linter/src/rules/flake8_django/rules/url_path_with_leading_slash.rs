use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_django::helpers::is_path_function;
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
#[violation_metadata(preview_since = "0.15.11")]
pub(crate) struct DjangoUrlPathWithLeadingSlash {
    url_pattern: String,
}

impl AlwaysFixableViolation for DjangoUrlPathWithLeadingSlash {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DjangoUrlPathWithLeadingSlash { url_pattern } = self;
        format!("URL route `{url_pattern}` has an unnecessary leading slash")
    }

    fn fix_title(&self) -> String {
        "Remove the leading slash".to_string()
    }
}

/// DJ101
pub(crate) fn url_path_with_leading_slash(checker: &Checker, call: &ast::ExprCall) {
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

    if route.is_empty() || route == "/" || !route.starts_with('/') {
        return;
    }

    let Some(first_literal) = value.iter().next() else {
        return;
    };
    if !checker
        .locator()
        .slice(first_literal.content_range())
        .starts_with('/')
    {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        DjangoUrlPathWithLeadingSlash {
            url_pattern: route.to_string(),
        },
        route_arg.range(),
    );

    let slash_pos = first_literal.content_range().start();

    diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
        slash_pos,
        slash_pos + TextSize::new(1),
    )));
}
