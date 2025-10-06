from django.urls import path
from mytools import path as mypath
from . import views

# Test that custom path functions are also checked for leading slashes
urlpatterns_custom = [
    mypath("/help/", views.help_view),  # DJ015
    mypath("/about/", views.about_view),  # DJ015
]

# OK - custom path without leading slash
urlpatterns_custom_ok = [
    mypath("help/", views.help_view),
    mypath("about/", views.about_view),
]

# Test that default django.urls.path still works
urlpatterns_default = [
    path("/contact/", views.contact_view),  # DJ015
    path("contact/", views.contact_ok),  # OK
]

# OK - root path and empty string
urlpatterns_edge_cases = [
    path("/", views.root_view),  # OK - root path
    mypath("/", views.root_view),  # OK - root path
    path("", views.empty_view),  # OK - empty string
    mypath("", views.empty_view),  # OK - empty string
]
