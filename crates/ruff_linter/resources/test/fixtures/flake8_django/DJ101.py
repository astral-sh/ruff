from django.urls import path
from . import views

# Errors - leading slash
urlpatterns = [
    path("/help/", views.help_view),  # DJ101
    path("/about/", views.about_view),  # DJ101
    path("/contact/", views.contact_view),  # DJ101
    path("/api/users/", views.users_view),  # DJ101
    path("/blog/posts/", views.posts_view),  # DJ101
]

# OK - no leading slash
urlpatterns_ok = [
    path("help/", views.help_view),
    path("about/", views.about_view),
    path("contact/", views.contact_view),
    path("api/users/", views.users_view),
    path("blog/posts/", views.posts_view),
]

# OK - just root path
urlpatterns_root = [
    path("/", views.index_view),
    path("", views.home_view),
]

# OK - with path parameters
urlpatterns_params = [
    path("users/<int:id>/", views.user_detail),
    path("posts/<slug:slug>/", views.post_detail),
]

# Mixed cases
urlpatterns_mixed = [
    path("good/", views.good_view),
    path("/bad/", views.bad_view),  # DJ101
    path("also-good/", views.also_good_view),
    path("/also-bad/", views.also_bad_view),  # DJ101
]

# Edge cases with different quote styles
urlpatterns_quotes = [
    path('/single-quote/', views.single_quote_view),  # DJ101
    path("/double-quote/", views.double_quote_view),  # DJ101
    path('''/triple-single/''', views.triple_single_view),  # DJ101
    path("""/triple-double/""", views.triple_double_view),  # DJ101
]

# Error - leading trail slash and argument should stay in message
urlpatterns_params_bad = [
    path("/bad/<slug:slug>/", views.bad_view),  # DJ101
    path("/<slug:slug>", views.bad_view),  # DJ101
]
