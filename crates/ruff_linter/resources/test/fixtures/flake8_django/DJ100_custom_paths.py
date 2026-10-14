from mytools import path as mypath
from . import views

# Test that custom path functions are also checked
urlpatterns_custom = [
    mypath("help", views.help_view),  # DJ100
    mypath("about", views.about_view),  # DJ100
]

# OK - custom path with trailing slash
urlpatterns_custom_ok = [
    mypath("help/", views.help_view),
    mypath("about/", views.about_view),
]

# Test multiple violations in same list
urlpatterns_multiple = [
    mypath("api/users", views.users_view),  # DJ100
    mypath("api/posts", views.posts_view),  # DJ100
    mypath("api/comments/", views.comments_view),  # OK
]

# OK - root path and empty string
urlpatterns_edge_cases = [
    mypath("/", views.root_view),  # OK - root path
    mypath("", views.empty_view),  # OK - empty string
]
